// SPDX-License-Identifier: MIT
//! Fixed-box text candidate patching. Any structural binding, focus, hover, viewport
//! or markup change falls back to validated full layout. Text has no intrinsic
//! Taffy measure in this frontend; its fixed box never depends on the string.
use crate::{
    Diagnostic, Node,
    frontend::Resolve,
    layout::{self, RawNode, ViewSpec, Widget},
};
use bead_core::Value;
use bead_render::{Palette, Primitive, Scene};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
struct Text {
    masked: bool,
    node: Option<usize>,
    binding: String,
    index: usize,
    placeholder: Option<String>,
    foreground: [u8; 4],
    muted: [u8; 4],
}
struct Scroll {
    id: String,
    binding: String,
    limit: f64,
}
struct ClipRegion {
    id: String,
    bounds: bead_render::Rect,
}
struct Floating {
    index: usize,
    node: String,
}
/// A view `bead.hovered` can name: its box and label primitives with both colour states.
struct Hover {
    id: String,
    rounded: Option<usize>,
    text: Option<usize>,
    idle: ([u8; 4], [u8; 4]),
    hot: ([u8; 4], [u8; 4]),
    /// Whether the idle and the hovered state draw a box (a transparent one is not drawn).
    boxed: (bool, bool),
    /// Where a box appears when the other state draws one, and the view's layer: in front of
    /// the view's own primitives (behind its shadow). `None` when the view drew nothing, so the
    /// patch cannot tell whether it was culled, and misses instead.
    insert_at: Option<(usize, usize)>,
    radius: f32,
    /// An empty input shows its placeholder in the muted colour: its label colour
    /// cannot be patched from the hover state alone.
    input: bool,
}
impl Hover {
    fn wants_box(&self, hot: bool) -> bool {
        if hot { self.boxed.1 } else { self.boxed.0 }
    }
}
#[cfg(test)]
thread_local! {
    static PATCH_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(super) fn patch_hits() -> usize {
    PATCH_HITS.get()
}
/// A `ui.animated` marker in the view and what its sampled value patches.
struct Animated {
    marker: serde_json::Value,
    kind: Animate,
}
enum Animate {
    /// `radius`: the view's own box and shadow.
    Radius {
        rounded: Option<usize>,
        shadow: Option<usize>,
    },
    /// An `ui.alpha(role, marker)` amount: `base` is the colour at amount 1.
    Alpha {
        primitive: usize,
        slot: Slot,
        base: [u8; 4],
    },
    /// An `ui.mix(a, b, marker)` amount between two colours.
    Mix {
        primitive: usize,
        slot: Slot,
        from: [u8; 4],
        to: [u8; 4],
    },
    /// `join`: how far the box and shadow reach behind the joined edge (`extent` = radius).
    Join {
        rounded: Option<usize>,
        shadow: Option<usize>,
        edge: bead_core::Edge,
        radius: f32,
    },
    /// `translate_x`/`translate_y`: the layer's subtree shifts by (sample - base).
    Translate {
        layer: usize,
        vertical: bool,
        base: f32,
    },
    /// Any other property: the cached layout is stale until the motion ends.
    Layout { property: String },
}
#[derive(Clone, Copy)]
enum Slot {
    Fill,
    Text,
    Border,
}
/// Reserved interaction stores the patch handles without a rebuild.
const INTERACTION: [&str; 3] = ["bead.hovered", "bead.focused", "bead.focus_visible"];
pub(super) struct Cached {
    /// When the scene was built: a motion finished by then is already at its target.
    built_ns: u64,
    animated: Vec<Animated>,
    body_layer: Option<usize>,
    raw: Rc<RawNode>,
    scene: Scene,
    nodes: Vec<Node>,
    viewport: (f32, f32),
    /// Colors are baked into the cached scene, so a palette change must miss.
    palette: Palette,
    values: BTreeMap<String, Value>,
    structural: BTreeSet<String>,
    text: Vec<Text>,
    hover: Vec<Hover>,
    scroll: Vec<Scroll>,
    panel_open: Option<String>,
    clip_regions: Vec<ClipRegion>,
    floating: Vec<Floating>,
    parents: Vec<Option<usize>>,
    scroll_layers: Vec<Option<usize>>,
    clip_layers: Vec<Option<usize>>,
    primitive_owners: Vec<usize>,
    node_owners: Vec<usize>,
    _charge: Option<bead_core::cache::Charge>,
}
fn bindings(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    if let Some(name) = value.get("__bind").and_then(|v| v.as_str()) {
        out.insert(name.into());
    } else {
        match value {
            serde_json::Value::Object(o) => {
                for v in o.values() {
                    bindings(v, out);
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    bindings(v, out);
                }
            }
            _ => {}
        }
    }
}
/// Take primitive `index` out of `scene`, renumbering layers and clips after it.
fn remove_primitive(scene: &mut Scene, index: usize) {
    scene.primitives.remove(index);
    scene.clips = std::mem::take(&mut scene.clips)
        .into_iter()
        .filter(|(i, _)| *i != index)
        .map(|(i, clip)| (if i > index { i - 1 } else { i }, clip))
        .collect();
    for layer in &mut scene.layers {
        if layer.own.start > index {
            layer.own.start -= 1;
        }
        if layer.own.end > index {
            layer.own.end -= 1;
        }
        if layer.subtree_end > index {
            layer.subtree_end -= 1;
        }
    }
}
/// Put `primitive` in front of layer `owner`'s own primitives (at `index`), with their clip.
fn insert_primitive(scene: &mut Scene, index: usize, owner: usize, primitive: Primitive) {
    let clip = scene.clips.get(&index).copied();
    scene.primitives.insert(index, primitive);
    scene.clips = std::mem::take(&mut scene.clips)
        .into_iter()
        .map(|(i, clip)| (if i >= index { i + 1 } else { i }, clip))
        .collect();
    if let Some(clip) = clip {
        scene.clips.insert(index, clip);
    }
    // Behind the owner's own shadow: every layer whose subtree ends here contains the owner.
    let inside = scene.layers[owner].own.start < index;
    for (position, layer) in scene.layers.iter_mut().enumerate() {
        // Layers precede their descendants: an empty ancestor range ending here stays
        // before the new primitive, a later layer starting here moves behind it.
        let after = position > owner;
        if layer.own.start > index || (layer.own.start == index && after) {
            layer.own.start += 1;
        }
        if layer.own.end > index || (layer.own.end == index && (after || position == owner)) {
            layer.own.end += 1;
        }
        if layer.subtree_end > index
            || (layer.subtree_end == index && (after || position == owner || inside))
        {
            layer.subtree_end += 1;
        }
    }
}
impl Cached {
    pub fn new(
        raw: Rc<RawNode>,
        view: &ViewSpec,
        scene: &Scene,
        nodes: &[Node],
        resolve: &Resolve<'_>,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<Option<Self>, Diagnostic> {
        let mut cached = Self {
            _charge: None,
            built_ns: bead_core::animation::build_clock(),
            animated: vec![],
            body_layer: view
                .body
                .as_ref()
                .and_then(|body| scene.layers.iter().position(|l| &l.id == body)),
            raw: raw.clone(),
            scene: scene.clone(),
            nodes: nodes.to_vec(),
            viewport,
            palette: Palette(palette.0.clone()),
            values: BTreeMap::new(),
            structural: BTreeSet::new(),
            text: vec![],
            hover: vec![],
            scroll: vec![],
            panel_open: None,
            clip_regions: vec![],
            floating: vec![],
            parents: vec![],
            scroll_layers: vec![],
            clip_layers: vec![],
            primitive_owners: vec![],
            node_owners: vec![],
        };
        /// Layer and node positions by view id, built once per cache.
        struct Index<'a> {
            layers: BTreeMap<&'a str, usize>,
            nodes: BTreeMap<&'a str, usize>,
        }
        let index = Index {
            layers: scene
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| (l.id.as_str(), i))
                .collect(),
            nodes: nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.as_str(), i))
                .collect(),
        };
        fn walk(
            raw: &RawNode,
            view: &ViewSpec,
            cached: &mut Cached,
            lookup: &Index<'_>,
            palette: &Palette,
            implicit_parent_label: bool,
            root: bool,
        ) -> Result<bool, Diagnostic> {
            if raw.kind != view.kind {
                return Ok(false);
            }
            let own = lookup
                .layers
                .get(view.id.as_str())
                .map(|&layer| cached.scene.layers[layer].own.clone());
            let primitive = |kind: fn(&Primitive) -> bool| {
                own.clone()
                    .and_then(|own| own.into_iter().find(|i| kind(&cached.scene.primitives[*i])))
            };
            if lookup.nodes.contains_key(view.id.as_str()) {
                let idle = layout::surface_colors(view, false, palette)?;
                let hot = layout::surface_colors(view, true, palette)?;
                if idle != hot {
                    let boxed =
                        |fill: [u8; 4]| fill[3] > 0 || view.border_width > 0. || view.selected;
                    cached.hover.push(Hover {
                        id: view.id.clone(),
                        rounded: primitive(|p| matches!(p, Primitive::Rounded { .. })),
                        text: (view.kind != Widget::TextInput)
                            .then(|| primitive(|p| matches!(p, Primitive::Text { .. })))
                            .flatten(),
                        idle,
                        hot,
                        boxed: (boxed(idle.0), boxed(hot.0)),
                        insert_at: own
                            .as_ref()
                            .filter(|own| !own.is_empty() && view.kind != Widget::Panel)
                            .map(|own| {
                                let shadow = matches!(
                                    cached.scene.primitives[own.start],
                                    Primitive::Shadow { .. }
                                );
                                (
                                    own.start + usize::from(shadow),
                                    lookup.layers[view.id.as_str()],
                                )
                            }),
                        radius: view.radius,
                        input: view.kind == Widget::TextInput,
                    });
                }
            }
            for (property, value) in &raw.props {
                if let Some(marker) = layout::marker(value) {
                    let kind = match (property.as_str(), lookup.layers.get(view.id.as_str())) {
                        // A joined box moves its rect with the radius too: rebuild instead.
                        ("radius", _) if view.join_edge.is_none() => Animate::Radius {
                            rounded: primitive(|p| matches!(p, Primitive::Rounded { .. })),
                            shadow: primitive(|p| matches!(p, Primitive::Shadow { .. })),
                        },
                        ("join", _) if view.join_edge.is_some() => Animate::Join {
                            rounded: primitive(|p| matches!(p, Primitive::Rounded { .. })),
                            shadow: primitive(|p| matches!(p, Primitive::Shadow { .. })),
                            edge: view.join_edge.expect("checked"),
                            radius: view.radius,
                        },
                        ("translate_x", Some(&layer)) => Animate::Translate {
                            layer,
                            vertical: false,
                            base: view.translate_x,
                        },
                        ("translate_y", Some(&layer)) => Animate::Translate {
                            layer,
                            vertical: true,
                            base: view.translate_y,
                        },
                        _ => Animate::Layout {
                            property: property.clone(),
                        },
                    };
                    cached.animated.push(Animated {
                        marker: marker.clone(),
                        kind,
                    });
                    continue;
                }
                if let Some(marker) = value.get("alpha").and_then(layout::marker) {
                    let (index, slot) = match property.as_str() {
                        "fill" => (
                            primitive(|p| matches!(p, Primitive::Rounded { .. })),
                            Slot::Fill,
                        ),
                        "color" => (
                            primitive(|p| matches!(p, Primitive::Text { .. })),
                            Slot::Text,
                        ),
                        "border_color" => (
                            primitive(|p| matches!(p, Primitive::Rounded { .. })),
                            Slot::Border,
                        ),
                        _ => (None, Slot::Fill),
                    };
                    let colour = |expression: &serde_json::Value| {
                        layout::color(Some(expression), [0; 4], palette, &raw.source)
                    };
                    // A hovered view recolours the same primitive: leave that to a rebuild.
                    let kind = match (
                        index,
                        value.get("role"),
                        value.get("base"),
                        value.get("over"),
                    ) {
                        (Some(primitive), Some(role), _, _)
                            if !cached.hover.iter().any(|h| h.id == view.id) =>
                        {
                            Animate::Alpha {
                                primitive,
                                slot,
                                base: colour(&serde_json::json!({ "role": role, "alpha": 1. }))?,
                            }
                        }
                        (Some(primitive), None, Some(base), Some(over))
                            if !cached.hover.iter().any(|h| h.id == view.id) =>
                        {
                            Animate::Mix {
                                primitive,
                                slot,
                                from: colour(base)?,
                                to: colour(over)?,
                            }
                        }
                        _ => Animate::Layout {
                            property: property.clone(),
                        },
                    };
                    cached.animated.push(Animated {
                        marker: marker.clone(),
                        kind,
                    });
                    continue;
                }
                let mut names = BTreeSet::new();
                bindings(value, &mut names);
                let panel_open = root
                    && raw.kind == Widget::Panel
                    && property == "open"
                    && value.get("__bind").is_some();
                if panel_open && let Some(binding) = names.first() {
                    cached.panel_open = Some(binding.clone());
                }
                let text = property == "text"
                    && matches!(view.kind, Widget::Text | Widget::TextInput)
                    && view.children.is_empty()
                    && !implicit_parent_label
                    && value.get("__bind").is_some();
                if text {
                    if let Some(index) = primitive(|p| matches!(p, Primitive::Text { .. })) {
                        let foreground = layout::surface_colors(view, false, palette)?.1;
                        cached.text.push(Text {
                            masked: view.masked,
                            node: if view.aria_label.is_empty() && !view.masked {
                                lookup.nodes.get(view.id.as_str()).copied()
                            } else {
                                None
                            },
                            binding: names.first().cloned().unwrap_or_default(),
                            index,
                            placeholder: (view.kind == Widget::TextInput)
                                .then(|| view.placeholder.clone()),
                            foreground,
                            muted: palette.get("onSurfaceVariant").unwrap_or(foreground),
                        });
                    } else {
                        cached.structural.extend(names);
                    }
                } else if property == "scroll" && view.kind == Widget::Scroll {
                    // Scroll is the only numeric binding that changes geometry
                    // without changing the layout tree; resolve it after traversal.
                } else if !panel_open {
                    cached.structural.extend(names);
                }
            }
            // ViewSpec flattens transparent Groups. Match the same child
            // sequence instead of silently zipping a Group onto a Text node.
            fn children<'a>(raw: &'a RawNode, out: &mut Vec<&'a RawNode>, cached: &mut Cached) {
                for child in &raw.children {
                    if child.kind == Widget::Group {
                        for value in child.props.values() {
                            bindings(value, &mut cached.structural);
                        }
                        children(child, out, cached);
                    } else {
                        out.push(child);
                    }
                }
            }
            let mut flattened = Vec::new();
            children(raw, &mut flattened, cached);
            if flattened.len() != view.children.len() {
                return Ok(false);
            }
            for (raw, view) in flattened.into_iter().zip(&view.children) {
                if !walk(
                    raw,
                    view,
                    cached,
                    lookup,
                    palette,
                    implicit_parent_label || (view.action.is_some() && view.aria_label.is_empty()),
                    false,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        if !walk(&raw, view, &mut cached, &index, palette, false, true)? {
            return Ok(None);
        }
        fn scrolls(view: &ViewSpec, scene: &Scene, nodes: &[Node], out: &mut Vec<Scroll>) {
            if let Some(binding) = &view.scroll
                && scene.layers.iter().any(|layer| layer.id == view.id)
                && let Some(node) = nodes.iter().find(|node| node.id == view.id)
            {
                out.push(Scroll {
                    id: view.id.clone(),
                    binding: binding.clone(),
                    limit: node.scroll_limit.unwrap_or(0.),
                });
            }
            for child in &view.children {
                scrolls(child, scene, nodes, out);
            }
        }
        fn clip_regions(view: &ViewSpec, scene: &Scene, out: &mut Vec<ClipRegion>) {
            if (view.clip_children || view.scroll.is_some())
                && let Some(layer) = scene.layers.iter().find(|layer| layer.id == view.id)
            {
                out.push(ClipRegion {
                    id: view.id.clone(),
                    bounds: layer.bounds,
                });
            }
            for child in &view.children {
                clip_regions(child, scene, out);
            }
        }
        scrolls(view, scene, nodes, &mut cached.scroll);
        clip_regions(view, scene, &mut cached.clip_regions);
        for scroll in &cached.scroll {
            cached
                .values
                .insert(scroll.binding.clone(), resolve(&scroll.binding)?);
        }
        let mut owners = vec![false; scene.primitives.len()];
        for layer in &scene.layers {
            for index in layer.own.clone() {
                if index < owners.len() {
                    owners[index] = true;
                }
            }
        }
        let focused = resolve("bead.focused").ok().and_then(|value| match value {
            Value::Text(id) => Some(id),
            _ => None,
        });
        for (index, owned) in owners.iter().enumerate() {
            if *owned {
                continue;
            }
            let Some(id) = focused.as_deref() else {
                return Ok(None);
            };
            let Some(node) = nodes.iter().find(|node| node.id == id) else {
                return Ok(None);
            };
            let (Primitive::Rounded { rect: frame, .. }, Primitive::Rounded { rect: hit, .. }) =
                (&scene.primitives[index], &node.draw)
            else {
                return Ok(None);
            };
            if frame.x != hit.x || frame.y != hit.y || frame.w != hit.w || frame.h != hit.h {
                return Ok(None);
            }
            cached.floating.push(Floating {
                index,
                node: id.to_owned(),
            });
        }
        let translated = cached
            .animated
            .iter()
            .any(|a| matches!(a.kind, Animate::Translate { .. }));
        if !cached.scroll.is_empty() || translated {
            cached.parents = Vec::with_capacity(scene.layers.len());
            cached.scroll_layers = Vec::with_capacity(scene.layers.len());
            cached.clip_layers = Vec::with_capacity(scene.layers.len());
            cached.node_owners = Vec::with_capacity(nodes.len());
            for layer in &scene.layers {
                cached.parents.push(
                    layer
                        .parent
                        .as_ref()
                        .and_then(|id| scene.layers.iter().position(|l| &l.id == id)),
                );
                cached
                    .scroll_layers
                    .push(cached.scroll.iter().position(|s| s.id == layer.id));
                cached
                    .clip_layers
                    .push(cached.clip_regions.iter().position(|c| c.id == layer.id));
            }
            cached.primitive_owners = vec![usize::MAX; scene.primitives.len()];
            for (owner, layer) in scene.layers.iter().enumerate() {
                for index in layer.own.clone() {
                    cached.primitive_owners[index] = owner;
                }
            }
            for floating in &cached.floating {
                let Some(owner) = scene.layers.iter().position(|l| l.id == floating.node) else {
                    return Ok(None);
                };
                cached.primitive_owners[floating.index] = owner;
            }
            for node in nodes {
                let Some(owner) = scene.layers.iter().position(|l| l.id == node.id) else {
                    return Ok(None);
                };
                cached.node_owners.push(owner);
            }
            if cached.primitive_owners.contains(&usize::MAX) {
                return Ok(None);
            }
        }
        for name in cached
            .structural
            .iter()
            .map(String::as_str)
            .chain(cached.text.iter().map(|t| t.binding.as_str()))
            .chain(cached.panel_open.as_deref())
            .chain(INTERACTION)
        {
            cached.values.insert(name.to_owned(), resolve(name)?);
        }
        let bytes = layout::raw_bytes(&raw)
            + (cached.parents.capacity()
                + cached.scroll_layers.capacity()
                + cached.clip_layers.capacity())
                * std::mem::size_of::<Option<usize>>()
            + (cached.primitive_owners.capacity() + cached.node_owners.capacity())
                * std::mem::size_of::<usize>()
            + cached
                .scene
                .primitives
                .iter()
                .map(bead_render::primitive_bytes)
                .sum::<usize>()
            + cached
                .nodes
                .iter()
                .map(|n| {
                    n.id.capacity()
                        + n.label.capacity()
                        + n.preview_key.capacity()
                        + std::mem::size_of::<String>()
                        + 2 * std::mem::size_of::<f32>()
                        + n.hit_polygon.as_ref().map_or(0, |points| {
                            points.capacity() * std::mem::size_of::<[f32; 2]>()
                        })
                        + bead_render::primitive_bytes(&n.draw)
                        + 256
                        + n.bind
                            .iter()
                            .map(|(k, v)| k.capacity() + v.capacity() + 64)
                            .sum::<usize>()
                        + [
                            &n.visible,
                            &n.action,
                            &n.icon,
                            &n.input,
                            &n.scroll,
                            &n.scroll_store,
                        ]
                        .into_iter()
                        .map(|s| s.as_ref().map_or(0, String::capacity))
                        .sum::<usize>()
                        + n.action_arg
                            .as_ref()
                            .map_or(0, bead_core::cache::value_bytes)
                })
                .sum::<usize>()
            + cached
                .values
                .iter()
                .map(|(k, v)| k.capacity() + 64 + bead_core::cache::value_bytes(v))
                .sum::<usize>()
            + cached
                .structural
                .iter()
                .map(|s| s.capacity() + 64)
                .sum::<usize>()
            + cached
                .text
                .iter()
                .map(|t| {
                    t.binding.capacity() + t.placeholder.as_ref().map_or(0, String::capacity) + 128
                })
                .sum::<usize>()
            + cached
                .hover
                .iter()
                .map(|h| h.id.capacity() + std::mem::size_of::<Hover>())
                .sum::<usize>()
            + cached
                .scroll
                .iter()
                .map(|s| s.id.capacity() + s.binding.capacity() + 96)
                .sum::<usize>()
            + cached
                .panel_open
                .as_ref()
                .map_or(0, |open| open.capacity() + 32)
            + cached
                .clip_regions
                .iter()
                .map(|region| region.id.capacity() + std::mem::size_of::<bead_render::Rect>() + 32)
                .sum::<usize>();
        let bytes = bytes
            + cached
                .floating
                .iter()
                .map(|item| item.node.capacity() + 32)
                .sum::<usize>();
        let budget = bead_core::cache::Budget::new("materialized-view", 8 * bead_core::cache::MIB);
        match budget.reserve(bytes) {
            Ok(c) => {
                cached._charge = Some(c);
                Ok(Some(cached))
            }
            Err(_) => {
                if bead_core::profile::enabled() {
                    println!("INPUT_PATCH_MISS kind=budget bytes={bytes}");
                }
                Ok(None)
            }
        }
    }
    pub fn patch(
        &self,
        raw: &Rc<RawNode>,
        resolve: &Resolve<'_>,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<Option<(Scene, Vec<Node>)>, Diagnostic> {
        if palette.0 != self.palette.0 {
            return Ok(None);
        }
        if !Rc::ptr_eq(raw, &self.raw) || viewport != self.viewport {
            if bead_core::profile::enabled() {
                println!(
                    "INPUT_PATCH_MISS kind={}",
                    if viewport != self.viewport {
                        "viewport"
                    } else {
                        "markup"
                    }
                );
            }
            return Ok(None);
        }
        let now_ns = bead_core::animation::clock_ns();
        // A layout-affecting motion still running when this scene was built: relayout (no Luau).
        if let Some(property) = self.animated.iter().find_map(|a| match &a.kind {
            Animate::Layout { property } if layout::marker_active(&a.marker, self.built_ns) => {
                Some(property)
            }
            _ => None,
        }) {
            if bead_core::profile::enabled() {
                println!("INPUT_PATCH_MISS kind=animated_layout property={property}");
            }
            return Ok(None);
        }
        let mut changed = BTreeMap::new();
        for (name, old) in &self.values {
            let value = resolve(name)?;
            if value != *old {
                let scroll_binding = self.scroll.iter().any(|scroll| &scroll.binding == name);
                let panel_open_binding = self.panel_open.as_ref().is_some_and(|open| open == name);
                let typed = match name.as_str() {
                    "bead.focus_visible" => matches!(value, Value::Bool(_)),
                    _ => {
                        matches!(value, Value::Text(_))
                            || (scroll_binding && matches!(value, Value::Number(_)))
                            || (panel_open_binding && matches!(value, Value::Bool(_)))
                    }
                };
                if self.structural.contains(name) || !typed {
                    if bead_core::profile::enabled() {
                        println!("INPUT_PATCH_MISS kind=structural binding={name}");
                    }
                    return Ok(None);
                }
                changed.insert(name.clone(), value);
            }
        }
        // The focus ring is one primitive after the last layer: it stays, moves or misses.
        let focus_target =
            if changed.contains_key("bead.focused") || changed.contains_key("bead.focus_visible") {
                let current = |name: &str| changed.get(name).or(self.values.get(name));
                let visible = current("bead.focus_visible") != Some(&Value::Bool(false));
                let node = match current("bead.focused") {
                    Some(Value::Text(id)) if visible && !id.is_empty() => {
                        self.nodes.iter().position(|n| &n.id == id && n.focus_ring)
                    }
                    _ => None,
                };
                if self.floating.len() != usize::from(node.is_some()) {
                    if bead_core::profile::enabled() {
                        println!("INPUT_PATCH_MISS kind=focus_ring");
                    }
                    return Ok(None);
                }
                node
            } else {
                None
            };
        if let Some(Value::Text(next)) = changed.get("bead.hovered") {
            let previous = match self.values.get("bead.hovered") {
                Some(Value::Text(id)) => id.as_str(),
                _ => "",
            };
            // shortcut: only node-bearing views are tracked, a host-set hover id of a plain
            // container is ignored here; track every view if a host ever needs that.
            for (id, hot) in [(previous, false), (next.as_str(), true)] {
                if let Some(hover) = self.hover.iter().find(|h| h.id == id)
                    && (hover.wants_box(hot)
                        && hover.rounded.is_none()
                        && hover.insert_at.is_none()
                        || hover.input && hover.idle.1 != hover.hot.1)
                {
                    if bead_core::profile::enabled() {
                        println!("INPUT_PATCH_MISS kind=hover_box id={id}");
                    }
                    return Ok(None);
                }
            }
        }
        #[cfg(test)]
        PATCH_HITS.set(PATCH_HITS.get() + 1);
        let mut scene = self.scene.clone();
        let mut nodes = self.nodes.clone();
        if let Some(open) = &self.panel_open
            && let Some(value) = changed.get(open)
        {
            let Value::Bool(value) = value else {
                return Ok(None);
            };
            let Some(motion) = &mut scene.motion else {
                return Ok(None);
            };
            motion.open = *value;
        }
        let hovered = match changed
            .get("bead.hovered")
            .or(self.values.get("bead.hovered"))
        {
            Some(Value::Text(id)) => id.as_str(),
            _ => "",
        };
        for text in &self.text {
            if let Some(Value::Text(value)) = changed.get(&text.binding)
                && let Primitive::Text {
                    text: paint, color, ..
                } = &mut scene.primitives[text.index]
            {
                let placeholder = value.is_empty() && text.placeholder.is_some();
                let hot = self
                    .hover
                    .iter()
                    .find(|h| h.text == Some(text.index) && h.id == hovered)
                    .map(|h| h.hot.1);
                *paint = if placeholder {
                    text.placeholder.clone().unwrap_or_default()
                } else if text.masked {
                    crate::input::mask(value)
                } else {
                    value.clone()
                };
                *color = if placeholder {
                    text.muted
                } else {
                    hot.unwrap_or(text.foreground)
                };
                if let Some(node) = text.node {
                    nodes[node].label = paint.clone();
                }
            }
        }
        if let Some(Value::Text(next)) = changed.get("bead.hovered") {
            let previous = match self.values.get("bead.hovered") {
                Some(Value::Text(id)) => id.as_str(),
                _ => "",
            };
            for (id, hot) in [(previous, false), (next.as_str(), true)] {
                let Some(hover) = self.hover.iter().find(|h| h.id == id) else {
                    continue;
                };
                let (fill, foreground) = if hot { hover.hot } else { hover.idle };
                if let Some(index) = hover.rounded
                    && let Primitive::Rounded { fill: paint, .. } = &mut scene.primitives[index]
                {
                    *paint = fill;
                }
                if let Some(index) = hover.text
                    && let Primitive::Text { color, .. } = &mut scene.primitives[index]
                {
                    *color = foreground;
                }
            }
        }
        for animated in &self.animated {
            let Some(value) = layout::sample(&animated.marker, now_ns) else {
                continue;
            };
            match animated.kind {
                Animate::Radius { rounded, shadow } => {
                    for index in [rounded, shadow].into_iter().flatten() {
                        if let Primitive::Rounded { radius, .. }
                        | Primitive::Shadow { radius, .. } = &mut scene.primitives[index]
                        {
                            *radius = value as f32;
                        }
                    }
                }
                Animate::Join {
                    rounded,
                    shadow,
                    edge,
                    radius,
                } => {
                    let extent = (value.clamp(0., 1.) as f32) * radius;
                    for index in [rounded, shadow].into_iter().flatten() {
                        // The cached rect already reaches `base` behind the edge: re-extend from its clip.
                        if let Some(clip) = scene.clips.get(&index).copied()
                            && let Primitive::Rounded { rect, .. } | Primitive::Shadow { rect, .. } =
                                &mut scene.primitives[index]
                        {
                            use bead_core::Edge;
                            match edge {
                                Edge::Top => {
                                    rect.h -= clip.y - rect.y - extent;
                                    rect.y = clip.y - extent;
                                }
                                Edge::Bottom => rect.h = clip.y + clip.h - rect.y + extent,
                                Edge::Left => {
                                    rect.w -= clip.x - rect.x - extent;
                                    rect.x = clip.x - extent;
                                }
                                Edge::Right => rect.w = clip.x + clip.w - rect.x + extent,
                            }
                        }
                    }
                }
                Animate::Mix {
                    primitive,
                    slot,
                    from,
                    to,
                } => {
                    let amount = value.clamp(0., 1.);
                    let colour = std::array::from_fn(|i| {
                        (f64::from(from[i]) * (1. - amount) + f64::from(to[i]) * amount).round()
                            as u8
                    });
                    match (slot, &mut scene.primitives[primitive]) {
                        (Slot::Fill, Primitive::Rounded { fill, .. }) => *fill = colour,
                        (Slot::Text, Primitive::Text { color, .. }) => *color = colour,
                        (
                            Slot::Border,
                            Primitive::Rounded {
                                border: Some(border),
                                ..
                            },
                        ) => border.0 = colour,
                        _ => {}
                    }
                }
                Animate::Alpha {
                    primitive,
                    slot,
                    base,
                } => {
                    let mut colour = base;
                    colour[3] = (f64::from(base[3]) * value.clamp(0., 1.)).round() as u8;
                    match (slot, &mut scene.primitives[primitive]) {
                        (Slot::Fill, Primitive::Rounded { fill, .. }) => *fill = colour,
                        (Slot::Text, Primitive::Text { color, .. }) => *color = colour,
                        (
                            Slot::Border,
                            Primitive::Rounded {
                                border: Some(border),
                                ..
                            },
                        ) => border.0 = colour,
                        _ => {}
                    }
                }
                Animate::Translate { .. } | Animate::Layout { .. } => {}
            }
        }
        fn translate(primitive: &mut Primitive, dx: f32, dy: f32) {
            match primitive {
                Primitive::Arc { rect, .. }
                | Primitive::Polyline { rect, .. }
                | Primitive::Rounded { rect, .. }
                | Primitive::Text { rect, .. }
                | Primitive::Image { rect, .. }
                | Primitive::Shadow { rect, .. }
                | Primitive::Graph { rect, .. } => *rect = rect.translated(dx, dy),
            }
        }
        let translated = self
            .animated
            .iter()
            .any(|a| matches!(a.kind, Animate::Translate { .. }));
        if translated || self.scroll.iter().any(|s| changed.contains_key(&s.binding)) {
            // Ownership and clipping topology are invariant until layout changes.
            // Follow cached indices instead of rebuilding String maps per frame.
            let shift = |layer: usize| {
                let (mut dx, mut dy) = (0., 0.);
                // A translated view moves its own box and everything below it.
                let mut at = Some(layer);
                while let Some(index) = at {
                    for animated in &self.animated {
                        if let Animate::Translate {
                            layer,
                            vertical,
                            base,
                        } = animated.kind
                            && layer == index
                        {
                            let delta = layout::sample(&animated.marker, now_ns)
                                .map_or(0., |v| v as f32 - base);
                            if vertical {
                                dy += delta;
                            } else {
                                dx += delta;
                            }
                        }
                    }
                    at = self.parents[index];
                }
                let mut parent = self.parents[layer];
                while let Some(index) = parent {
                    if let Some(scroll) = self.scroll_layers[index] {
                        let scroll = &self.scroll[scroll];
                        let old = match self.values.get(&scroll.binding) {
                            Some(Value::Number(v)) => *v,
                            _ => 0.,
                        };
                        let new = match changed.get(&scroll.binding) {
                            Some(Value::Number(v)) => *v,
                            _ => old,
                        };
                        dy +=
                            old.clamp(0., scroll.limit) as f32 - new.clamp(0., scroll.limit) as f32;
                    }
                    parent = self.parents[index];
                }
                (dx, dy)
            };
            let clip = |layer: usize| {
                let mut result = None;
                let mut parent = self.parents[layer];
                while let Some(index) = parent {
                    if let Some(region) = self.clip_layers[index] {
                        let (dx, dy) = shift(index);
                        let bounds = self.clip_regions[region].bounds.translated(dx, dy);
                        result = Some(result.map_or(bounds, |previous: bead_render::Rect| {
                            previous.intersect(bounds)
                        }));
                    }
                    parent = self.parents[index];
                }
                result
            };
            for (index, primitive) in scene.primitives.iter_mut().enumerate() {
                let owner = self.primitive_owners[index];
                let (dx, dy) = shift(owner);
                translate(primitive, dx, dy);
                if let Some(bounds) = clip(owner) {
                    scene.clips.insert(index, bounds);
                } else {
                    scene.clips.remove(&index);
                }
            }
            for (index, layer) in scene.layers.iter_mut().enumerate() {
                let (dx, dy) = shift(index);
                layer.bounds = layer.bounds.translated(dx, dy);
            }
            for (index, node) in nodes.iter_mut().enumerate() {
                let owner = self.node_owners[index];
                let (dx, dy) = shift(owner);
                translate(&mut node.draw, dx, dy);
                node.clip = clip(owner);
            }
        }
        // The body's parts and bounds follow whatever the patches above changed.
        if let Some(motion) = &mut scene.motion
            && motion.body.is_some()
        {
            for (index, original) in &mut motion.parts {
                *original = scene.primitives[*index].clone();
            }
            if let Some(layer) = self.body_layer {
                motion.body = Some(scene.layers[layer].bounds);
            }
        }
        if let (Some(node), Some(ring)) = (focus_target, self.floating.first()) {
            let node = &nodes[node];
            if let Primitive::Rounded { rect, .. } = &mut scene.primitives[ring.index] {
                *rect = crate::rect(&node.draw);
            }
            match node.clip {
                Some(clip) => scene.clips.insert(ring.index, clip),
                None => scene.clips.remove(&ring.index),
            };
        }
        // Last, since it renumbers primitives: a box that only one hover state draws.
        if let Some(Value::Text(next)) = changed.get("bead.hovered") {
            let previous = match self.values.get("bead.hovered") {
                Some(Value::Text(id)) => id.as_str(),
                _ => "",
            };
            // Cached indices: a removal before the insertion point shifts it by one.
            let mut removed = None;
            for (id, hot) in [(previous, false), (next.as_str(), true)] {
                let Some(hover) = self.hover.iter().find(|h| h.id == id) else {
                    continue;
                };
                match (hover.wants_box(hot), hover.rounded) {
                    (false, Some(index)) => {
                        remove_primitive(&mut scene, index);
                        removed = Some(index);
                    }
                    (true, None) => {
                        let (cached_at, layer) = hover.insert_at.expect("checked before patching");
                        let at = cached_at - usize::from(removed.is_some_and(|r| r < cached_at));
                        let primitive = Primitive::Rounded {
                            rect: scene.layers[layer].bounds,
                            radius: hover.radius,
                            fill: if hot { hover.hot.0 } else { hover.idle.0 },
                            border: None,
                            radial: None,
                        };
                        insert_primitive(&mut scene, at, layer, primitive);
                    }
                    _ => {}
                }
            }
        }
        Ok(Some((scene, nodes)))
    }
}
/// An interaction pose (spec 0280) a view asked for.
enum Pose {
    /// `visible_on_hover`: the subtree's alpha follows whether `group` holds the pointer or keyboard focus.
    Reveal { group: String, group_layer: usize },
    /// `press_scale` / `pressed_fill` while this view or a descendant is pressed.
    Press { scale: f32, fill: Option<[u8; 4]> },
    /// A direct child of a `reflow` container: slides from where it was drawn, relative to the container.
    Reflow {
        parent: usize,
        duration: f64,
        y_key: String,
    },
}
struct Hook {
    layer: usize,
    id: String,
    key: String,
    pose: Pose,
}
/// Hover reveal, press and list reflow, sampled on every frame onto the laid-out or patched scene:
/// no Luau run and no layout, and nothing to do for a view that asks for none of them. The motion
/// state outlives the cached scene, so a rebuild mid-motion continues from the drawn pose.
#[derive(Default)]
pub(super) struct Poses {
    hooks: Vec<Hook>,
    motion: bead_core::animation::Animations,
    /// The clock of the last posed frame: a motion running then needs one more frame.
    posed: f64,
    /// The node hovered when the pointer went down: views holding it are pressed while the
    /// pointer stays inside them, until the release.
    press: Option<String>,
}
/// Whether view `id` is `group` or inside it (scoped ids extend their ancestors' ids).
fn within(id: &str, group: &str) -> bool {
    id.strip_prefix(group)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}
fn mix(a: [u8; 4], b: [u8; 4], amount: f32) -> [u8; 4] {
    std::array::from_fn(|i| {
        (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * amount).round() as u8
    })
}
pub(super) fn fade(primitive: &mut Primitive, alpha: f32) {
    let fade = |c: &mut [u8; 4]| c[3] = (f32::from(c[3]) * alpha).round() as u8;
    match primitive {
        Primitive::Rounded {
            fill,
            border,
            radial,
            ..
        } => {
            fade(fill);
            if let Some((c, _)) = border {
                fade(c);
            }
            // The layers sit between the fill and the border, so they fade with them.
            if let Some(layers) = radial {
                *layers = layers
                    .iter()
                    .cloned()
                    .map(|mut layer| {
                        layer.stops.iter_mut().for_each(|(_, c)| fade(c));
                        layer
                    })
                    .collect();
            }
        }
        Primitive::Text { color, shadow, .. } => {
            fade(color);
            if let Some(shadow) = shadow {
                fade(&mut shadow.color);
            }
        }
        Primitive::Arc { color, .. } => fade(color),
        Primitive::Polyline { color, fill, .. } => {
            fade(color);
            if let Some(c) = fill {
                fade(c);
            }
        }
        Primitive::Image { tint: Some(c), .. } => fade(c),
        Primitive::Image { opacity, .. } => *opacity *= alpha,
        Primitive::Shadow { alpha: a, .. } => *a *= alpha,
        // shortcut: a graph has no alpha and stays drawn, add one if a hover-only graph ever needs it.
        Primitive::Graph { .. } => {}
    }
}
/// `primitive` scaled by `s` about the centre of `around` (a shadow keeps its margin).
fn scale_about(primitive: &mut Primitive, around: bead_render::Rect, s: f32) {
    let (cx, cy) = (around.x + around.w / 2., around.y + around.h / 2.);
    let scaled = |r: bead_render::Rect| {
        bead_render::Rect::new(cx + (r.x - cx) * s, cy + (r.y - cy) * s, r.w * s, r.h * s)
    };
    match primitive {
        Primitive::Rounded { rect, radius, .. } | Primitive::Image { rect, radius, .. } => {
            *rect = scaled(*rect);
            *radius *= s;
        }
        Primitive::Text { rect, size, .. } => {
            *rect = scaled(*rect);
            *size *= s;
        }
        Primitive::Shadow {
            rect,
            radius,
            blur,
            spread,
            dy,
            ..
        } => {
            let m = *blur * 3. + *spread;
            let inner = scaled(bead_render::Rect::new(
                rect.x + m,
                rect.y + m - *dy,
                rect.w - 2. * m,
                rect.h - 2. * m,
            ));
            *rect = bead_render::Rect::new(
                inner.x - m,
                inner.y - m + *dy,
                inner.w + 2. * m,
                inner.h + 2. * m,
            );
            *radius *= s;
        }
        Primitive::Arc { rect, .. }
        | Primitive::Polyline { rect, .. }
        | Primitive::Graph { rect, .. } => *rect = scaled(*rect),
    }
}
impl Poses {
    /// Read the poses of a fresh layout; the motions of views that left are dropped.
    pub(super) fn set_hooks(
        &mut self,
        view: &ViewSpec,
        scene: &Scene,
        palette: &Palette,
    ) -> Result<(), Diagnostic> {
        enum Spec<'a> {
            Reveal(&'a str),
            Press(f32, Option<[u8; 4]>),
            Reflow(&'a str, f32),
        }
        fn walk<'a>(
            view: &'a ViewSpec,
            ancestors: &mut Vec<&'a str>,
            palette: &Palette,
            out: &mut Vec<(&'a str, Spec<'a>)>,
        ) -> Result<(), Diagnostic> {
            if let Some(name) = &view.visible_on_hover {
                let group = ancestors
                    .iter()
                    .rev()
                    .copied()
                    .find(|id| {
                        id.strip_suffix(name.as_str())
                            .is_some_and(|scope| scope.is_empty() || scope.ends_with('.'))
                    })
                    .ok_or_else(|| {
                        view.source
                            .error(format!("visible_on_hover names no ancestor id {name}"))
                    })?;
                out.push((view.id.as_str(), Spec::Reveal(group)));
            }
            if view.press_scale != 1. || view.pressed_fill.is_some() {
                let fill = view
                    .pressed_fill
                    .as_ref()
                    .map(|fill| layout::color(Some(fill), [0; 4], palette, &view.source))
                    .transpose()?;
                out.push((view.id.as_str(), Spec::Press(view.press_scale, fill)));
            }
            if view.reflow > 0. {
                for child in &view.children {
                    out.push((
                        child.id.as_str(),
                        Spec::Reflow(view.id.as_str(), view.reflow),
                    ));
                }
            }
            ancestors.push(view.id.as_str());
            for child in &view.children {
                walk(child, ancestors, palette, out)?;
            }
            ancestors.pop();
            Ok(())
        }
        let mut found = Vec::new();
        walk(view, &mut Vec::new(), palette, &mut found)?;
        self.hooks.clear();
        if !found.is_empty() {
            let layers: BTreeMap<&str, usize> = scene
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| (l.id.as_str(), i))
                .collect();
            for (id, spec) in found {
                // A culled or hidden view has no layer and nothing to pose.
                let Some(&layer) = layers.get(id) else {
                    continue;
                };
                let (kind, pose) = match spec {
                    Spec::Reveal(group) => match layers.get(group) {
                        Some(&group_layer) => (
                            "r",
                            Pose::Reveal {
                                group: group.to_owned(),
                                group_layer,
                            },
                        ),
                        None => continue,
                    },
                    Spec::Press(scale, fill) => ("p", Pose::Press { scale, fill }),
                    Spec::Reflow(parent, duration) => match layers.get(parent) {
                        Some(&parent) => (
                            "x",
                            Pose::Reflow {
                                parent,
                                duration: f64::from(duration),
                                y_key: format!("y:{id}"),
                            },
                        ),
                        None => continue,
                    },
                };
                self.hooks.push(Hook {
                    layer,
                    id: id.to_owned(),
                    key: format!("{kind}:{id}"),
                    pose,
                });
            }
        }
        let keys: BTreeSet<&str> = self
            .hooks
            .iter()
            .flat_map(|hook| {
                let y = match &hook.pose {
                    Pose::Reflow { y_key, .. } => Some(y_key.as_str()),
                    _ => None,
                };
                [Some(hook.key.as_str()), y]
            })
            .flatten()
            .collect();
        self.motion.retain(|key| keys.contains(key));
        Ok(())
    }
    /// Pose `scene` for the interaction state in `stores` at `now` (animation clock, seconds).
    pub(super) fn pose(&mut self, scene: &mut Scene, stores: &bead_core::Stores, now: f64) {
        if self.hooks.is_empty() {
            return;
        }
        self.posed = now;
        fn text<'a>(stores: &'a bead_core::Stores, name: &str) -> &'a str {
            match stores.get(name) {
                Some(Value::Text(id)) => id,
                _ => "",
            }
        }
        let (hovered, focused) = (text(stores, "bead.hovered"), text(stores, "bead.focused"));
        let keyboard = stores.get("bead.focus_visible") != Some(&Value::Bool(false));
        let pressed = stores.get("bead.pressed") == Some(&Value::Bool(true));
        if !pressed {
            self.press = None;
        } else if self.press.is_none() {
            self.press = Some(hovered.to_owned());
        }
        let origin = self.press.clone().unwrap_or_default();
        let token = |name: &str| bead_core::animation::duration(name).unwrap_or(0.1);
        let curve = bead_core::animation::Curve::EaseOut;
        // Innermost first: an outer scale then carries the already-posed inner views with it.
        for hook in self.hooks.iter().rev() {
            let Some(layer) = scene.layers.get(hook.layer) else {
                continue;
            };
            let (own, range, bounds) = (
                layer.own.clone(),
                layer.own.start..layer.subtree_end,
                layer.bounds,
            );
            match &hook.pose {
                Pose::Reveal { group, .. } => {
                    let shown = within(hovered, group) || (keyboard && within(focused, group));
                    let target = f32::from(u8::from(shown));
                    self.motion
                        .target(&hook.key, target, now, token("fade"), curve);
                    let alpha = self.motion.value(&hook.key, now).map_or(target, |v| v.0);
                    if alpha < 1. {
                        for primitive in &mut scene.primitives[range] {
                            fade(primitive, alpha);
                        }
                    }
                }
                Pose::Press { scale, fill } => {
                    let down = !origin.is_empty()
                        && within(&origin, &hook.id)
                        && within(hovered, &hook.id);
                    let duration = token(if down { "press" } else { "release" });
                    // A view never pressed keeps no motion state; its first press starts from rest.
                    if self.motion.value(&hook.key, now).is_none() {
                        if !down {
                            continue;
                        }
                        self.motion.target(&hook.key, 0., now, duration, curve);
                    }
                    self.motion
                        .target(&hook.key, f32::from(u8::from(down)), now, duration, curve);
                    let amount = self.motion.value(&hook.key, now).map_or(0., |v| v.0);
                    if amount <= 0. {
                        continue;
                    }
                    if let Some(to) = fill
                        && let Some(Primitive::Rounded { fill, .. }) = scene.primitives[own]
                            .iter_mut()
                            .find(|p| matches!(p, Primitive::Rounded { .. }))
                    {
                        *fill = mix(*fill, *to, amount);
                    }
                    let s = 1. + (scale - 1.) * amount;
                    if s != 1. {
                        for primitive in &mut scene.primitives[range] {
                            scale_about(primitive, bounds, s);
                        }
                    }
                }
                Pose::Reflow {
                    parent,
                    duration,
                    y_key,
                } => {
                    let Some(origin) = scene.layers.get(*parent).map(|l| l.bounds) else {
                        continue;
                    };
                    // Relative to the container, so scrolling or moving the whole list is no reflow.
                    let (x, y) = (bounds.x - origin.x, bounds.y - origin.y);
                    self.motion.target(&hook.key, x, now, *duration, curve);
                    self.motion.target(y_key, y, now, *duration, curve);
                    let dx = self.motion.value(&hook.key, now).map_or(x, |v| v.0) - x;
                    let dy = self.motion.value(y_key, now).map_or(y, |v| v.0) - y;
                    if dx != 0. || dy != 0. {
                        // shortcut: clips stay at rest, so content clipped inside a sliding item
                        // is cut at its new place; move clips too if a host needs that.
                        for primitive in &mut scene.primitives[range] {
                            let (Primitive::Arc { rect, .. }
                            | Primitive::Polyline { rect, .. }
                            | Primitive::Rounded { rect, .. }
                            | Primitive::Text { rect, .. }
                            | Primitive::Image { rect, .. }
                            | Primitive::Shadow { rect, .. }
                            | Primitive::Graph { rect, .. }) = primitive;
                            *rect = rect.translated(dx, dy);
                        }
                    }
                }
            }
        }
    }
    /// Whether the last posed frame was mid-motion, so another frame is due.
    pub(super) fn active(&self) -> bool {
        !self.hooks.is_empty() && self.motion.active(self.posed)
    }
    /// The deepest `visible_on_hover` group whose box in `scene` holds the point.
    pub(super) fn group_at(&self, scene: &Scene, x: f32, y: f32) -> Option<String> {
        self.hooks
            .iter()
            .filter_map(|hook| match &hook.pose {
                Pose::Reveal { group, group_layer } => scene
                    .layers
                    .get(*group_layer)
                    .filter(|l| &l.id == group && l.bounds.hit_test(x, y, None, None))
                    .map(|_| group),
                _ => None,
            })
            .max_by_key(|group| group.len())
            .cloned()
    }
}

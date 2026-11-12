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
pub(super) struct Cached {
    raw: Rc<RawNode>,
    scene: Scene,
    nodes: Vec<Node>,
    viewport: (f32, f32),
    /// Colors are baked into the cached scene, so a palette change must miss.
    palette: Palette,
    values: BTreeMap<String, Value>,
    structural: BTreeSet<String>,
    text: Vec<Text>,
    scroll: Vec<Scroll>,
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
            raw: raw.clone(),
            scene: scene.clone(),
            nodes: nodes.to_vec(),
            viewport,
            palette: Palette(palette.0.clone()),
            values: BTreeMap::new(),
            structural: BTreeSet::from([
                "bead.focused".into(),
                "bead.hovered".into(),
                "bead.focus_visible".into(),
            ]),
            text: vec![],
            scroll: vec![],
            clip_regions: vec![],
            floating: vec![],
            parents: vec![],
            scroll_layers: vec![],
            clip_layers: vec![],
            primitive_owners: vec![],
            node_owners: vec![],
        };
        fn walk(
            raw: &RawNode,
            view: &ViewSpec,
            cached: &mut Cached,
            palette: &Palette,
            implicit_parent_label: bool,
        ) -> Result<bool, Diagnostic> {
            if raw.kind != view.kind {
                return Ok(false);
            }
            for (property, value) in &raw.props {
                let mut names = BTreeSet::new();
                bindings(value, &mut names);
                let text = property == "text"
                    && matches!(view.kind, Widget::Text | Widget::TextInput)
                    && view.children.is_empty()
                    && !implicit_parent_label
                    && value.get("__bind").is_some();
                if text {
                    let index = cached
                        .scene
                        .layers
                        .iter()
                        .find(|l| l.id == view.id)
                        .and_then(|l| {
                            l.own.clone().find(|i| {
                                matches!(cached.scene.primitives[*i], Primitive::Text { .. })
                            })
                        });
                    if let Some(index) = index {
                        let foreground = layout::color(
                            view.color.as_ref(),
                            palette.get("onSurface").unwrap_or([240, 240, 240, 255]),
                            palette,
                            &view.source,
                        )?;
                        cached.text.push(Text {
                            masked: view.masked,
                            node: if view.aria_label.is_empty() && !view.masked {
                                cached.nodes.iter().position(|n| n.id == view.id)
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
                } else {
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
                    palette,
                    implicit_parent_label || (view.action.is_some() && view.aria_label.is_empty()),
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        if !walk(&raw, view, &mut cached, palette, false)? {
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
        if !cached.scroll.is_empty() {
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
            .chain(cached.text.iter().map(|t| &t.binding))
        {
            cached.values.insert(name.clone(), resolve(name)?);
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
                .scroll
                .iter()
                .map(|s| s.id.capacity() + s.binding.capacity() + 96)
                .sum::<usize>()
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
        let mut changed = BTreeMap::new();
        for (name, old) in &self.values {
            let value = resolve(name)?;
            if value != *old {
                let scroll_binding = self.scroll.iter().any(|scroll| &scroll.binding == name);
                if self.structural.contains(name)
                    || !(matches!(value, Value::Text(_))
                        || (scroll_binding && matches!(value, Value::Number(_))))
                {
                    if bead_core::profile::enabled() {
                        println!("INPUT_PATCH_MISS kind=structural binding={name}");
                    }
                    return Ok(None);
                }
                changed.insert(name.clone(), value);
            }
        }
        let mut scene = self.scene.clone();
        let mut nodes = self.nodes.clone();
        for text in &self.text {
            if let Some(Value::Text(value)) = changed.get(&text.binding)
                && let Primitive::Text {
                    text: paint, color, ..
                } = &mut scene.primitives[text.index]
            {
                let placeholder = value.is_empty() && text.placeholder.is_some();
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
                    text.foreground
                };
                if let Some(node) = text.node {
                    nodes[node].label = paint.clone();
                }
            }
        }
        fn translate(primitive: &mut Primitive, dy: f32) {
            match primitive {
                Primitive::Arc { rect, .. }
                | Primitive::Polyline { rect, .. }
                | Primitive::Rounded { rect, .. }
                | Primitive::Text { rect, .. }
                | Primitive::Image { rect, .. }
                | Primitive::Shadow { rect, .. } => *rect = rect.translated(0., dy),
            }
        }
        if self.scroll.iter().any(|s| changed.contains_key(&s.binding)) {
            // Ownership and clipping topology are invariant until layout changes.
            // Follow cached indices instead of rebuilding String maps per frame.
            let shift = |layer: usize| {
                let mut dy = 0.;
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
                dy
            };
            let clip = |layer: usize| {
                let mut result = None;
                let mut parent = self.parents[layer];
                while let Some(index) = parent {
                    if let Some(region) = self.clip_layers[index] {
                        let bounds = self.clip_regions[region]
                            .bounds
                            .translated(0., shift(index));
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
                translate(primitive, shift(owner));
                if let Some(bounds) = clip(owner) {
                    scene.clips.insert(index, bounds);
                } else {
                    scene.clips.remove(&index);
                }
            }
            for (index, layer) in scene.layers.iter_mut().enumerate() {
                layer.bounds = layer.bounds.translated(0., shift(index));
            }
            for (index, node) in nodes.iter_mut().enumerate() {
                let owner = self.node_owners[index];
                translate(&mut node.draw, shift(owner));
                node.clip = clip(owner);
            }
        }
        Ok(Some((scene, nodes)))
    }
}

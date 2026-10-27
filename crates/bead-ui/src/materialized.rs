// SPDX-License-Identifier: MIT OR Apache-2.0
//! Retained patches for fixed-box text and absolute clipped fills. Any structural
//! binding, focus, hover, viewport or markup change falls back to validated layout.
//! Patched text and clip width never affect Taffy's intrinsic measurements.
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
enum RetainedChange {
    ClipWidth { id: String, width: f32 },
    Text { id: String, text: String },
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
    fn retained_changes(old: &RawNode, new: &RawNode, out: &mut Vec<RetainedChange>) -> bool {
        if old.kind != new.kind
            || old.source != new.source
            || old.scope != new.scope
            || old.children.len() != new.children.len()
            || old.props.len() != new.props.len()
        {
            return false;
        }
        for (name, value) in &old.props {
            let Some(next) = new.props.get(name) else {
                return false;
            };
            if value == next {
                continue;
            }
            match name.as_str() {
                "width"
                    if old.kind == Widget::Box
                        && old.props.get("clip_children")
                            == Some(&serde_json::Value::Bool(true))
                        && old
                            .props
                            .get("position")
                            .and_then(serde_json::Value::as_str)
                            == Some("absolute")
                        && out
                            .iter()
                            .all(|change| !matches!(change, RetainedChange::ClipWidth { .. })) =>
                {
                    let Some(width) = next
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0. && *v <= f64::from(f32::MAX))
                    else {
                        return false;
                    };
                    let Some(id) = old.props.get("id").and_then(serde_json::Value::as_str) else {
                        return false;
                    };
                    out.push(RetainedChange::ClipWidth {
                        id: id.to_owned(),
                        width: width as f32,
                    });
                }
                "text"
                    if old.kind == Widget::Text
                        && out
                            .iter()
                            .all(|change| !matches!(change, RetainedChange::Text { .. })) =>
                {
                    let (Some(id), Some(text)) = (
                        old.props.get("id").and_then(serde_json::Value::as_str),
                        next.as_str(),
                    ) else {
                        return false;
                    };
                    out.push(RetainedChange::Text {
                        id: id.to_owned(),
                        text: text.to_owned(),
                    });
                }
                _ => return false,
            }
        }
        old.children
            .iter()
            .zip(&new.children)
            .all(|(old, new)| Self::retained_changes(old, new, out))
    }

    fn intersect(a: bead_render::Rect, b: bead_render::Rect) -> bead_render::Rect {
        let x = a.x.max(b.x);
        let y = a.y.max(b.y);
        bead_render::Rect::new(
            x,
            y,
            ((a.x + a.w).min(b.x + b.w) - x).max(0.),
            ((a.y + a.h).min(b.y + b.h) - y).max(0.),
        )
    }

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
        for name in cached
            .structural
            .iter()
            .chain(cached.text.iter().map(|t| &t.binding))
        {
            cached.values.insert(name.clone(), resolve(name)?);
        }
        let bytes = layout::raw_bytes(&raw)
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
        &mut self,
        raw: &Rc<RawNode>,
        resolve: &Resolve<'_>,
        palette: &Palette,
        viewport: (f32, f32),
    ) -> Result<Option<(Scene, Vec<Node>)>, Diagnostic> {
        if palette.0 != self.palette.0 {
            return Ok(None);
        }
        let mut retained = Vec::new();
        if !Rc::ptr_eq(raw, &self.raw) && !Self::retained_changes(&self.raw, raw, &mut retained)
            || viewport != self.viewport
        {
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
        for change in &retained {
            match change {
                RetainedChange::Text { id, text } => {
                    let Some(layer) = scene.layers.iter().find(|layer| layer.id == *id) else {
                        return Ok(None);
                    };
                    let Some(index) = layer.own.clone().find(|index| {
                        matches!(scene.primitives.get(*index), Some(Primitive::Text { .. }))
                    }) else {
                        return Ok(None);
                    };
                    let Some(Primitive::Text { text: current, .. }) =
                        scene.primitives.get_mut(index)
                    else {
                        return Ok(None);
                    };
                    *current = text.clone();
                }
                RetainedChange::ClipWidth { id, width } => {
                    let Some(layer) = scene.layers.iter_mut().find(|layer| layer.id == *id) else {
                        return Ok(None);
                    };
                    layer.bounds.w = *width;
                    let clip = layer.bounds;
                    if let Some(region) =
                        self.clip_regions.iter_mut().find(|region| region.id == *id)
                    {
                        region.bounds = clip;
                    } else {
                        return Ok(None);
                    }
                    let mut ancestors = BTreeMap::<String, Vec<String>>::new();
                    let clip_ids: BTreeSet<_> =
                        self.clip_regions.iter().map(|r| r.id.as_str()).collect();
                    for layer in &scene.layers {
                        let mut chain = layer
                            .parent
                            .as_deref()
                            .and_then(|parent| ancestors.get(parent))
                            .cloned()
                            .unwrap_or_default();
                        if let Some(parent) = &layer.parent
                            && clip_ids.contains(parent.as_str())
                        {
                            chain.insert(0, parent.clone());
                        }
                        ancestors.insert(layer.id.clone(), chain);
                    }
                    let clip_by_id: BTreeMap<_, _> = self
                        .clip_regions
                        .iter()
                        .map(|r| (r.id.as_str(), r.bounds))
                        .collect();
                    scene.clips.clear();
                    for layer in &scene.layers {
                        let mut inherited = None;
                        for ancestor in ancestors
                            .get(&layer.id)
                            .map_or(&[][..], Vec::as_slice)
                            .iter()
                            .rev()
                        {
                            if let Some(bounds) = clip_by_id.get(ancestor.as_str()) {
                                inherited = Some(
                                    inherited
                                        .map_or(*bounds, |prior| Self::intersect(prior, *bounds)),
                                );
                            }
                        }
                        if let Some(bounds) = inherited {
                            for index in layer.own.clone() {
                                scene.clips.insert(index, bounds);
                            }
                        }
                    }
                    for node in &mut nodes {
                        if let Some(bounds) = ancestors
                            .get(&node.id)
                            .map_or(&[][..], Vec::as_slice)
                            .iter()
                            .rev()
                            .filter_map(|id| clip_by_id.get(id.as_str()).copied())
                            .reduce(Self::intersect)
                        {
                            node.clip = Some(bounds);
                        }
                        if node.id == *id {
                            match &mut node.draw {
                                Primitive::Arc { rect, .. }
                                | Primitive::Polyline { rect, .. }
                                | Primitive::Rounded { rect, .. }
                                | Primitive::Text { rect, .. }
                                | Primitive::Image { rect, .. }
                                | Primitive::Shadow { rect, .. } => rect.w = *width,
                            }
                        }
                    }
                }
            }
        }
        if !retained.is_empty() {
            self.raw = raw.clone();
            self.scene = scene.clone();
            self.nodes = nodes.clone();
        }
        fn translated(mut primitive: Primitive, dy: f32) -> Primitive {
            match &mut primitive {
                Primitive::Arc { rect, .. }
                | Primitive::Polyline { rect, .. }
                | Primitive::Rounded { rect, .. }
                | Primitive::Text { rect, .. }
                | Primitive::Image { rect, .. }
                | Primitive::Shadow { rect, .. } => *rect = rect.translated(0., dy),
            }
            primitive
        }
        let scroll_changed = self
            .scroll
            .iter()
            .any(|scroll| changed.contains_key(&scroll.binding));
        if scroll_changed {
            let offsets: BTreeMap<_, _> = self
                .scroll
                .iter()
                .map(|scroll| {
                    let old = match self.values.get(&scroll.binding) {
                        Some(Value::Number(v)) => *v,
                        _ => 0.,
                    };
                    let new = match changed.get(&scroll.binding) {
                        Some(Value::Number(v)) => *v,
                        _ => old,
                    };
                    (
                        scroll.id.clone(),
                        (
                            old.clamp(0., scroll.limit) as f32,
                            new.clamp(0., scroll.limit) as f32,
                        ),
                    )
                })
                .collect();
            let scroll_ids: BTreeSet<_> = self
                .scroll
                .iter()
                .map(|scroll| scroll.id.as_str())
                .collect();
            let clip_ids: BTreeSet<_> = self
                .clip_regions
                .iter()
                .map(|region| region.id.as_str())
                .collect();
            let clip_by_id: BTreeMap<_, _> = self
                .clip_regions
                .iter()
                .map(|region| (region.id.as_str(), region))
                .collect();
            let mut ancestors = BTreeMap::<String, Vec<String>>::new();
            let mut clip_ancestors = BTreeMap::<String, Vec<String>>::new();
            for layer in &self.scene.layers {
                let parent = layer.parent.as_deref();
                let mut chain = parent
                    .and_then(|parent| ancestors.get(parent))
                    .cloned()
                    .unwrap_or_default();
                if let Some(parent) = &layer.parent
                    && scroll_ids.contains(parent.as_str())
                {
                    chain.insert(0, parent.clone());
                }
                ancestors.insert(layer.id.clone(), chain);
                let mut clips = parent
                    .and_then(|parent| clip_ancestors.get(parent))
                    .cloned()
                    .unwrap_or_default();
                if let Some(parent) = &layer.parent
                    && clip_ids.contains(parent.as_str())
                {
                    clips.insert(0, parent.clone());
                }
                clip_ancestors.insert(layer.id.clone(), clips);
            }
            let ancestor_scrolls = |id: &str| ancestors.get(id).cloned().unwrap_or_default();
            let shift = |ids: &[String]| {
                ids.iter()
                    .map(|id| {
                        let (old, new) = offsets[id];
                        old - new
                    })
                    .sum::<f32>()
            };
            let inherited_clip = |ids: &[String]| -> Option<bead_render::Rect> {
                let mut clip = None;
                for id in ids.iter().rev() {
                    let region = clip_by_id.get(id.as_str())?;
                    let ancestors = ancestor_scrolls(id);
                    let dy = shift(&ancestors);
                    let rect = region.bounds.translated(0., dy);
                    clip =
                        Some(clip.map_or(rect, |prior: bead_render::Rect| prior.intersect(rect)));
                }
                clip
            };
            let mut owner = vec![String::new(); scene.primitives.len()];
            for layer in &scene.layers {
                for index in layer.own.clone() {
                    if index < owner.len() {
                        owner[index] = layer.id.clone();
                    }
                }
            }
            for item in &self.floating {
                owner[item.index] = item.node.clone();
            }
            if owner.iter().any(String::is_empty) {
                return Ok(None);
            }
            scene.clips.clear();
            for (index, id) in owner.iter().enumerate() {
                let ancestors = ancestor_scrolls(id);
                let dy = shift(&ancestors);
                if dy != 0. {
                    scene.primitives[index] = translated(scene.primitives[index].clone(), dy);
                }
                if let Some(clip) = inherited_clip(&clip_ancestors[id]) {
                    scene.clips.insert(index, clip);
                }
            }
            for layer in &mut scene.layers {
                let ancestors = ancestor_scrolls(&layer.id);
                layer.bounds = layer.bounds.translated(0., shift(&ancestors));
            }
            for node in &mut nodes {
                let ancestors = ancestor_scrolls(&node.id);
                let dy = shift(&ancestors);
                if dy != 0. {
                    node.draw = translated(node.draw.clone(), dy);
                }
                node.clip = inherited_clip(&clip_ancestors[&node.id]);
            }
        }
        Ok(Some((scene, nodes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(width: f64, text: &str) -> Rc<RawNode> {
        Rc::new(
            serde_json::from_value(serde_json::json!({
                "kind":"box",
                "props":{"id":"window","position":"absolute","clip_children":true,"width":width},
                "children":[{"kind":"text","props":{"id":"value","width":64,"height":20,"text":text},"children":[],"source":{"file":"view.luau","line":2},"scope":null}],
                "source":{"file":"view.luau","line":1},
                "scope":null
            }))
            .unwrap(),
        )
    }

    #[test]
    fn retained_clip_width_and_fixed_text_changes_skip_layout_compatibility() {
        let old = tree(42., "42%");
        let new = tree(58., "58%");
        let mut changes = Vec::new();
        assert!(Cached::retained_changes(&old, &new, &mut changes));
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|change| matches!(change, RetainedChange::ClipWidth { id, width } if id == "window" && *width == 58.)));
        assert!(changes.iter().any(|change| matches!(change, RetainedChange::Text { id, text } if id == "value" && text == "58%")));
    }

    #[test]
    fn retained_patch_rejects_unclipped_geometry_and_structure_changes() {
        let old = tree(42., "42%");
        let mut unclipped = (*tree(58., "58%")).clone();
        unclipped
            .props
            .insert("clip_children".into(), serde_json::Value::Bool(false));
        let mut changes = Vec::new();
        assert!(!Cached::retained_changes(&old, &unclipped, &mut changes));
        let mut structural = (*tree(42., "42%")).clone();
        structural
            .props
            .insert("height".into(), serde_json::json!(8));
        let mut changes = Vec::new();
        assert!(!Cached::retained_changes(&old, &structural, &mut changes));
    }
}

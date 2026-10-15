// SPDX-License-Identifier: MIT OR Apache-2.0
//! Text-only candidate patching. Any structural binding, focus, hover, viewport
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
    node: Option<usize>,
    binding: String,
    index: usize,
    placeholder: Option<String>,
    foreground: [u8; 4],
    muted: [u8; 4],
}
pub(super) struct Cached {
    raw: Rc<RawNode>,
    scene: Scene,
    nodes: Vec<Node>,
    viewport: (f32, f32),
    values: BTreeMap<String, Value>,
    structural: BTreeSet<String>,
    text: Vec<Text>,
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
    ) -> Result<Self, Diagnostic> {
        let mut cached = Self {
            raw: raw.clone(),
            scene: scene.clone(),
            nodes: nodes.to_vec(),
            viewport,
            values: BTreeMap::new(),
            structural: BTreeSet::from(["bead.focused".into(), "bead.hovered".into()]),
            text: vec![],
        };
        fn walk(
            raw: &RawNode,
            view: &ViewSpec,
            cached: &mut Cached,
            palette: &Palette,
            implicit_parent_label: bool,
        ) -> Result<(), Diagnostic> {
            for (property, value) in &raw.props {
                let mut names = BTreeSet::new();
                bindings(value, &mut names);
                let text = property == "text"
                    && view.kind == Widget::TextInput
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
                            node: if view.aria_label.is_empty() {
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
                } else {
                    cached.structural.extend(names);
                }
            }
            for (raw, view) in raw.children.iter().zip(&view.children) {
                walk(
                    raw,
                    view,
                    cached,
                    palette,
                    implicit_parent_label || (view.action.is_some() && view.aria_label.is_empty()),
                )?;
            }
            Ok(())
        }
        walk(&raw, view, &mut cached, palette, false)?;
        for name in cached
            .structural
            .iter()
            .chain(cached.text.iter().map(|t| &t.binding))
        {
            cached.values.insert(name.clone(), resolve(name)?);
        }
        Ok(cached)
    }
    pub fn patch(
        &self,
        raw: &Rc<RawNode>,
        resolve: &Resolve<'_>,
        viewport: (f32, f32),
    ) -> Result<Option<(Scene, Vec<Node>)>, Diagnostic> {
        if !Rc::ptr_eq(raw, &self.raw) || viewport != self.viewport {
            return Ok(None);
        }
        let mut changed = BTreeMap::new();
        for (name, old) in &self.values {
            let value = resolve(name)?;
            if value != *old {
                if self.structural.contains(name) || !matches!(value, Value::Text(_)) {
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
        Ok(Some((scene, nodes)))
    }
}

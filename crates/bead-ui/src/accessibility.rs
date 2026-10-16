// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Node, rect};
use accesskit::{Action, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use bead_core::{Stores, Value};
pub fn id(key: &str) -> NodeId {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in key.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    NodeId(hash | 1 << 63)
}
pub fn tree(views: &[Node], stores: &Stores) -> TreeUpdate {
    let root = NodeId(1);
    let mut parent = accesskit::Node::new(Role::Window);
    parent.set_label("Nacre Shell");
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let mut focus = root;
    for view in views {
        let node_id = id(&view.id);
        let mut node = accesskit::Node::new(if view.input.is_some() {
            Role::TextInput
        } else if view.action.is_some() {
            Role::Button
        } else {
            Role::ScrollView
        });
        node.set_label(view.label.clone());
        let r = rect(&view.draw);
        node.set_bounds(accesskit::Rect {
            x0: f64::from(r.x + 32.),
            y0: f64::from(r.y + 32.),
            x1: f64::from(r.x + r.w + 32.),
            y1: f64::from(r.y + r.h + 32.),
        });
        if view.focusable || view.input.is_some() {
            node.add_action(Action::Focus);
        }
        if view.action.is_some() {
            node.add_action(Action::Click);
        }
        if let Some(store) = &view.input {
            if let Some(Value::Text(value)) = stores.get(store) {
                node.set_value(value.clone());
            }
            node.add_action(Action::SetValue);
        }
        if stores.get("bead.focused") == Some(&Value::Text(view.id.clone())) {
            focus = node_id;
        }
        children.push(node_id);
        nodes.push((node_id, node));
    }
    parent.set_children(children);
    nodes.insert(0, (root, parent));
    let mut info = TreeInfo::new(root);
    info.toolkit_name = Some("Bead".into());
    TreeUpdate {
        nodes,
        tree: Some(info),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// Add static text from the committed scene, keeping it out of pointer/focus
/// nodes. Layer IDs keep text identity stable across value changes; owned ranges
/// avoid exposing the same primitive twice or duplicating a control's own text.
pub fn tree_with_scene(views: &[Node], stores: &Stores, scene: &bead_render::Scene) -> TreeUpdate {
    use bead_render::{Primitive, Rect};
    use std::collections::{BTreeMap, BTreeSet};
    let mut update = tree(views, stores);
    if scene.layers.is_empty() {
        return update;
    }
    for (_, node) in update.nodes.iter_mut().skip(1) {
        if let Some(bounds) = node.bounds() {
            let (dx, dy) = (
                f64::from(scene.offset.0) - 32.,
                f64::from(scene.offset.1) - 32.,
            );
            node.set_bounds(accesskit::Rect {
                x0: bounds.x0 + dx,
                y0: bounds.y0 + dy,
                x1: bounds.x1 + dx,
                y1: bounds.y1 + dy,
            });
        }
    }
    let root = NodeId(1);
    let controls: BTreeMap<_, _> = views.iter().map(|n| (n.id.as_str(), id(&n.id))).collect();
    let mut parents = BTreeMap::new();
    let mut children = BTreeMap::<NodeId, Vec<NodeId>>::new();
    let mut seen = BTreeSet::new();
    fn intersection(a: Rect, b: Rect) -> Option<Rect> {
        let x = a.x.max(b.x);
        let y = a.y.max(b.y);
        let w = (a.x + a.w).min(b.x + b.w) - x;
        let h = (a.y + a.h).min(b.y + b.h) - y;
        (w > 0. && h > 0.).then_some(Rect::new(x, y, w, h))
    }
    for layer in &scene.layers {
        let parent = layer
            .parent
            .as_deref()
            .and_then(|p| parents.get(p))
            .copied()
            .unwrap_or(root);
        let control = controls.get(layer.id.as_str()).copied();
        parents.insert(layer.id.as_str(), control.unwrap_or(parent));
        if let Some(node) = control {
            children.entry(parent).or_default().push(node);
            seen.insert(node);
            continue;
        }
        for (ordinal, index) in layer.own.clone().enumerate() {
            let Some(Primitive::Text {
                rect,
                text,
                family,
                color,
                ..
            }) = scene.primitives.get(index)
            else {
                continue;
            };
            if text.is_empty() || color[3] == 0 || family.starts_with("Material Symbols") {
                continue;
            }
            let Some(mut bounds) = intersection(*rect, scene.bounds) else {
                continue;
            };
            if let Some(clip) = scene.clips.get(&index) {
                let Some(clipped) = intersection(bounds, *clip) else {
                    continue;
                };
                bounds = clipped;
            }
            let node_id = id(&format!("static-text:{}:{ordinal}", layer.id));
            let mut node = accesskit::Node::new(Role::Label);
            node.set_value(text.clone()); // AccessKit Label content belongs in value.
            node.set_bounds(accesskit::Rect {
                x0: f64::from(bounds.x + scene.offset.0),
                y0: f64::from(bounds.y + scene.offset.1),
                x1: f64::from(bounds.x + bounds.w + scene.offset.0),
                y1: f64::from(bounds.y + bounds.h + scene.offset.1),
            });
            children.entry(parent).or_default().push(node_id);
            update.nodes.push((node_id, node));
            seen.insert(node_id);
        }
    }
    // Keep compatibility nodes not represented by Luau scene layers attached.
    for (node_id, _) in update.nodes.iter().skip(1) {
        if !seen.contains(node_id) {
            children.entry(root).or_default().push(*node_id);
        }
    }
    for (node_id, node) in &mut update.nodes {
        if let Some(children) = children.remove(node_id) {
            node.set_children(children);
        }
    }
    update
}

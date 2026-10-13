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

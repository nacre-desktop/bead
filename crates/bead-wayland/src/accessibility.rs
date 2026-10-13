// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::UiEvent;
use std::sync::{Arc, Mutex};
pub struct Bridge {
    pub adapter: accesskit_unix::Adapter,
    pub tree: Arc<Mutex<Option<accesskit::TreeUpdate>>>,
}
struct Activation(Arc<Mutex<Option<accesskit::TreeUpdate>>>);
impl accesskit::ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        self.0.lock().ok()?.clone()
    }
}
struct Actions(calloop::channel::Sender<UiEvent>);
impl accesskit::ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        let action = match request.action {
            accesskit::Action::Focus => "focus",
            accesskit::Action::Click => "click",
            accesskit::Action::SetValue => "set-value",
            _ => "unsupported",
        };
        let value = match request.data {
            Some(accesskit::ActionData::Value(value)) => Some(value.to_string()),
            _ => None,
        };
        let _ = self.0.send(UiEvent::Accessibility {
            target: request.target_node.0,
            action: action.into(),
            value,
        });
    }
}
struct Deactivation;
impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}
impl Bridge {
    pub fn new(sender: calloop::channel::Sender<UiEvent>) -> Self {
        let tree = Arc::new(Mutex::new(None));
        let adapter =
            accesskit_unix::Adapter::new(Activation(tree.clone()), Actions(sender), Deactivation);
        Self { adapter, tree }
    }
    pub fn update(&mut self, tree: accesskit::TreeUpdate) {
        *self.tree.lock().unwrap() = Some(tree.clone());
        self.adapter.update_if_active(|| tree);
    }
}

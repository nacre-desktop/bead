// SPDX-License-Identifier: MIT OR Apache-2.0
//! Explicit action-to-capability policy; theme prompts are a later frontend concern.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub struct Registry {
    actions: BTreeMap<String, String>,
    grants: BTreeSet<String>,
}
impl Registry {
    pub fn register(&mut self, action: &str, capability: &str) -> Result<(), String> {
        if action.is_empty() || capability.is_empty() || self.actions.contains_key(action) {
            return Err("invalid/duplicate action capability".into());
        }
        self.actions.insert(action.into(), capability.into());
        Ok(())
    }
    pub fn grant(&mut self, capability: &str) {
        self.grants.insert(capability.into());
    }
    pub fn authorize(&self, action: &str) -> Result<(), String> {
        let capability = self
            .actions
            .get(action)
            .ok_or_else(|| format!("unregistered action {action}"))?;
        if self.grants.contains(capability) {
            Ok(())
        } else {
            Err(format!("action {action} requires {capability}"))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_capabilities_and_unknown_actions_fail_closed() {
        let mut registry = Registry::default();
        registry.register("audio.set", "audio").unwrap();
        assert!(registry.authorize("audio.set").is_err());
        assert!(registry.authorize("process.exec").is_err());
        registry.grant("audio");
        assert!(registry.authorize("audio.set").is_ok());
        assert!(registry.register("audio.set", "other").is_err());
    }
}

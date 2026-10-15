// SPDX-License-Identifier: MIT OR Apache-2.0
//! Durable, typed state keyed independently from any view generation.
pub mod animation;
pub mod capabilities;
pub mod profile;
pub mod recovery;
pub mod source;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// A surface's owning edge; inward vectors also place popouts away from bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}
impl Edge {
    pub fn inward(self) -> (f32, f32) {
        match self {
            Self::Top => (0., 1.),
            Self::Bottom => (0., -1.),
            Self::Left => (1., 0.),
            Self::Right => (-1., 0.),
        }
    }
    pub fn closed_offset(self, width: f32, height: f32, progress: f32) -> (f32, f32) {
        let (x, y) = self.inward();
        let distance = if x == 0. { height } else { width };
        (
            -x * distance * (1. - progress),
            -y * distance * (1. - progress),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<serde_json::Value>),
}
impl Value {
    fn same_type(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
    fn valid(&self) -> bool {
        match self {
            Self::Number(n) => n.is_finite(),
            Self::Text(s) => s.len() <= 65536,
            Self::List(items) => {
                items.len() <= 2048
                    && items.iter().all(|item| item.is_object())
                    && serde_json::to_vec(items).is_ok_and(|bytes| bytes.len() <= 1024 * 1024)
            }
            _ => true,
        }
    }
}
#[derive(Clone, Default, Debug)]
pub struct Stores {
    values: BTreeMap<String, Value>,
    versions: BTreeMap<String, u64>,
    revision: u64,
}
impl Stores {
    pub fn values(&self) -> &BTreeMap<String, Value> {
        &self.values
    }
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn version(&self, id: &str) -> Option<u64> {
        self.versions.get(id).copied()
    }
    /// Stage schema reconciliation without mutating the live stores. Existing values win.
    pub fn reconcile(&self, schema: &BTreeMap<String, Value>) -> Result<Self, String> {
        let mut next = self.clone();
        for (id, default) in schema {
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                || !default.valid()
            {
                return Err(format!("invalid state definition {id}"));
            }
            if let Some(current) = next.values.get(id) {
                if !current.same_type(default) {
                    return Err(format!(
                        "state {id} changes type; supply an explicit migration later"
                    ));
                }
            } else {
                next.values.insert(id.clone(), default.clone());
                next.versions.insert(id.clone(), 0);
            }
        }
        if next.values.len() > 4096 {
            return Err("state retention exceeds 4096 stable IDs".into());
        }
        Ok(next)
    }
    /// A batch either succeeds in full or leaves all values/revisions unchanged.
    pub fn commit(&mut self, changes: BTreeMap<String, Value>) -> Result<bool, String> {
        for (id, value) in &changes {
            let current = self
                .values
                .get(id)
                .ok_or_else(|| format!("unknown state {id}"))?;
            if !current.same_type(value) || !value.valid() {
                return Err(format!("invalid value/type for {id}"));
            }
        }
        let changed: Vec<_> = changes
            .into_iter()
            .filter(|(id, value)| self.values.get(id) != Some(value))
            .collect();
        if changed.is_empty() {
            return Ok(false);
        }
        self.revision += 1;
        for (id, value) in changed {
            self.values.insert(id.clone(), value);
            self.versions.insert(id, self.revision);
        }
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_batch_is_atomic_and_equal_values_do_not_notify() {
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("input.text".into(), Value::Text("saved".into())),
                ("panel.open".into(), Value::Bool(true)),
            ]))
            .unwrap();
        let revision = stores.revision();
        assert!(
            stores
                .commit(BTreeMap::from([
                    ("input.text".into(), Value::Text("lost".into())),
                    ("panel.open".into(), Value::Number(0.))
                ]))
                .is_err()
        );
        assert_eq!(stores.get("input.text"), Some(&Value::Text("saved".into())));
        assert_eq!(stores.revision(), revision);
        assert!(!stores.commit(stores.values().clone()).unwrap());
        assert!(
            stores
                .commit(BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
                .unwrap()
        );
        assert_eq!(stores.version("input.text"), Some(0));
        assert_eq!(stores.version("panel.open"), Some(1));
    }
    #[test]
    fn state_outlives_removed_views_and_schema_rejects_type_changes() {
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([(
                "input.text".into(),
                Value::Text("typed".into()),
            )]))
            .unwrap();
        let absent = stores.reconcile(&BTreeMap::new()).unwrap();
        let back = absent
            .reconcile(&BTreeMap::from([(
                "input.text".into(),
                Value::Text("new default".into()),
            )]))
            .unwrap();
        assert_eq!(back.get("input.text"), stores.get("input.text"));
        assert!(
            back.reconcile(&BTreeMap::from([("input.text".into(), Value::Bool(true))]))
                .is_err()
        );
    }
}

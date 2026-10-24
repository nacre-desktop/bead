// SPDX-License-Identifier: GPL-3.0-only
//! The optional updates bar item reads the published dashboard count, never a provider.
use bead::core::{Value, source::Snapshot};
use nacre_protocol::core_services::{CoreSnapshot, UpdatesState};
use std::collections::BTreeMap;

pub fn values(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let count = serde_json::from_value::<UpdatesState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && state.health.available
                && !state.health.stale
                && CoreSnapshot::Updates(state.clone()).valid()
        })
        .and_then(|state| state.count)
        .unwrap_or(0);
    BTreeMap::from([("bar.updates.count".into(), Value::Number(count.into()))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::core::source::Source;
    use bead_test::source::FakeSource;
    #[test]
    fn unknown_stale_and_disconnected_counts_hide_the_item() {
        let line = include_str!("../tests/protocol-v1/core-dashboard-updates.jsonl")
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|v| v.get("data").cloned())
            .expect("snapshot fixture");
        let source = FakeSource::new(line).unwrap();
        let mut snapshot = (*source.snapshot()).clone();
        let count = snapshot.data["count"].as_u64().unwrap();
        assert_eq!(
            values(&snapshot)["bar.updates.count"],
            Value::Number(count as f64)
        );
        snapshot.connected = false;
        assert_eq!(values(&snapshot)["bar.updates.count"], Value::Number(0.));
        snapshot.connected = true;
        std::sync::Arc::make_mut(&mut snapshot.data)["health"]["stale"] = true.into();
        assert_eq!(values(&snapshot)["bar.updates.count"], Value::Number(0.));
    }
}

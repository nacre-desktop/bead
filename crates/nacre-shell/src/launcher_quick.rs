// SPDX-License-Identifier: GPL-3.0-only
//! Launcher Quick projects the control-centre toggles while the launcher is open.
use crate::{control_center::ControlCenter, i18n::tr, nacre_source::Client};
use bead::core::{
    Stores, Value,
    source::{self, Source, Subscription},
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

const ITEMS: [(&str, &str, &str, &str); 3] = [
    (
        "actions.controls.dnd",
        "notifications.dnd",
        "bead.launcher.quick-dnd-label",
        "bead.launcher.quick-dnd-reason",
    ),
    (
        "actions.controls.night-light",
        "night-light.state",
        "bead.panel.control-night",
        "bead.launcher.quick-night-reason",
    ),
    (
        "actions.controls.stay-awake",
        "stay-awake.state",
        "bead.panel.control-awake",
        "bead.launcher.quick-awake-reason",
    ),
];

pub struct Quick {
    control: ControlCenter,
    updates: Option<Arc<dyn Source>>,
}

impl Quick {
    pub fn new(client: Option<Client>, topics: Vec<(&'static str, Arc<dyn Source>)>) -> Self {
        let updates = topics
            .iter()
            .find(|(name, _)| *name == "dashboard.updates")
            .map(|(_, source)| source.clone());
        let control = ControlCenter::new(client, topics, None, Vec::new());
        Self { control, updates }
    }

    pub fn lease(client: Client) -> source::Result<Self> {
        let topics = ["controls", "notifications", "dashboard.updates"]
            .into_iter()
            .map(|name| {
                let topic = client.view_topic(name)?;
                Ok((name, topic))
            })
            .collect::<source::Result<Vec<_>>>()?;
        Ok(Self::new(Some(client), topics))
    }

    pub fn subscribe(
        &self,
        notify: Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>,
    ) -> source::Result<Vec<Subscription>> {
        self.control.subscribe(notify)
    }

    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        if stores.get("panel.open") == Some(&Value::Bool(false)) {
            // Keep the last visible frame intact during the closing animation.
            // Runtime retirement releases its stores, subscriptions and leases together.
            return BTreeMap::new();
        }
        self.control.poll_quick();
        let rows = ITEMS
            .iter()
            .map(|(id, source, label_id, reason_id)| {
                let state = self.control.quick_state(source);
                let available = state.is_some();
                let (on, toggle, locked, status, icon) = state.unwrap_or((
                    false,
                    false,
                    false,
                    tr("bead.launcher.quick-unavailable", &[]),
                    "",
                ));
                let reason = if locked {
                    tr("bead.launcher.quick-external", &[])
                } else if !available || !toggle {
                    tr("bead.launcher.quick-unavailable", &[])
                } else {
                    tr(reason_id, &[])
                };
                let pending = self.control.quick_pending(source);
                json!({"id": id, "on": on, "enabled": available && toggle && !pending,
                "label": tr(label_id, &[]), "reason": reason,
                "status": if available { status } else { "—".into() }, "icon": icon})
            })
            .collect();
        let count = self
            .updates
            .as_ref()
            .map(|source| source.snapshot())
            .filter(|updates| {
                updates.connected
                    && updates.data["health"]["available"] == true
                    && updates.data["health"]["stale"] != true
            })
            .and_then(|updates| updates.data["count"].as_u64())
            .filter(|count| *count <= u64::from(u32::MAX))
            .map(|count| count as f64)
            .unwrap_or(-1.);
        let message = self.control.quick_message();
        BTreeMap::from([
            ("launcher.quick".into(), Value::list(rows)),
            ("launcher.updates.count".into(), Value::Number(count)),
            ("launcher.quick_error".into(), Value::Text(message)),
        ])
    }

    pub fn act(&self, id: &str) {
        let Some((_, source, _, _)) = ITEMS.iter().find(|(candidate, ..)| *candidate == id) else {
            return;
        };
        if self.control.quick_pending(source)
            || !self
                .control
                .quick_state(source)
                .is_some_and(|(_, enabled, _, _, _)| enabled)
        {
            return;
        }
        self.control.act(&format!("toggle:{source}"));
    }
}

#[cfg(test)]
#[path = "launcher_quick_tests.rs"]
mod tests;

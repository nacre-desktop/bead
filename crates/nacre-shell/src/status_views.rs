// SPDX-License-Identifier: GPL-3.0-only
//! Read-only projections of the existing hosted Desktop Source, not new backends.
use bead::core::{
    Value,
    source::{self, Source},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Network,
    Bluetooth,
    Battery,
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Network, Self::Bluetooth, Self::Battery];
    pub fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Battery => "battery",
        }
    }
    pub fn projection(self) -> fn(&bead::services::Snapshot) -> serde_json::Value {
        match self {
            Self::Network => |s| serde_json::json!({"network":s.network}),
            Self::Bluetooth => |s| serde_json::json!({"bluetooth":s.bluetooth}),
            Self::Battery => |s| serde_json::json!({"battery":s.battery}),
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Battery => "Battery",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Network => "wifi",
            Self::Bluetooth => "bluetooth",
            Self::Battery => "battery_full",
        }
    }
}

pub struct Status {
    kind: Kind,
    source: Arc<dyn Source>,
}
impl Status {
    pub fn new(kind: Kind, source: Arc<dyn Source>) -> Self {
        Self { kind, source }
    }
    pub fn subscribe(
        &self,
        notify: Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>,
    ) -> source::Result<source::Subscription> {
        self.source.subscribe(Arc::new(move |_| {
            notify(bead::wayland::UiEvent::Model(BTreeMap::new()))
        }))
    }
    pub fn project(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        // Sources retain their last good snapshot while disconnected. Keep its
        // display too; the shell's connection Source owns the reconnecting mark.
        let data = &snapshot.data[self.kind.name()];
        let available = data["available"].as_bool() == Some(true);
        let text = |key: &str| data[key].as_str();
        let boolean = |key: &str| data[key].as_bool();
        let mut primary = format!("{} unavailable", self.kind.title());
        let mut secondary = "Status data unavailable".to_owned();
        let mut detail = String::new();
        let mut ready = false;
        let mut active = false;
        match self.kind {
            Kind::Network if available => {
                if let (Some(connected), Some(wireless)) =
                    (boolean("connected"), boolean("wireless_enabled"))
                {
                    ready = true;
                    active = connected || wireless;
                    primary = if connected {
                        text("name")
                            .filter(|s| !s.is_empty())
                            .unwrap_or("Connected")
                            .into()
                    } else {
                        "Not connected".into()
                    };
                    secondary = if wireless { "Wi-Fi on" } else { "Wi-Fi off" }.into();
                    detail = data["strength"]
                        .as_u64()
                        .filter(|n| *n <= 100)
                        .map(|n| format!("Signal strength {n}%"))
                        .unwrap_or_else(|| "Connection controls unavailable".into());
                }
            }
            Kind::Bluetooth if available => {
                if let (Some(powered), Some(devices)) =
                    (boolean("powered"), data["connected_devices"].as_array())
                    && devices.iter().all(serde_json::Value::is_string)
                {
                    ready = true;
                    active = powered;
                    primary = if !powered {
                        "Bluetooth off".into()
                    } else if devices.is_empty() {
                        "Bluetooth on".into()
                    } else {
                        format!(
                            "{} device{} connected",
                            devices.len(),
                            if devices.len() == 1 { "" } else { "s" }
                        )
                    };
                    secondary = if !powered {
                        "Power is off"
                    } else if devices.is_empty() {
                        "No devices connected"
                    } else {
                        "Bluetooth on"
                    }
                    .into();
                    detail = if powered && !devices.is_empty() {
                        devices
                            .iter()
                            .take(2)
                            .filter_map(serde_json::Value::as_str)
                            .map(|s| line(s, 24))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        "Device controls unavailable".into()
                    };
                }
            }
            Kind::Battery if available => {
                if let (Some(percent), Some(charging)) = (
                    data["percentage"]
                        .as_f64()
                        .filter(|n| n.is_finite() && (0.0..=100.0).contains(n)),
                    boolean("charging"),
                ) {
                    ready = true;
                    active = true;
                    primary = format!("{percent:.0}%");
                    // The current host has no on-battery or time estimate field.
                    secondary = if charging { "Charging" } else { "Not charging" }.into();
                    detail = "Power profiles unavailable".into();
                }
            }
            _ => {}
        }
        BTreeMap::from([
            ("status.ready".into(), Value::Bool(ready)),
            ("status.active".into(), Value::Bool(active)),
            (
                "status.heading".into(),
                Value::Text(self.kind.title().into()),
            ),
            ("status.icon".into(), Value::Text(self.kind.icon().into())),
            ("status.primary".into(), Value::Text(line(&primary, 32))),
            ("status.kind".into(), Value::Text(self.kind.name().into())),
            ("status.secondary".into(), Value::Text(secondary)),
            ("status.detail".into(), Value::Text(line(&detail, 48))),
        ])
    }
}
fn line(text: &str, max: usize) -> String {
    let mut chars = text.chars().map(|c| if c.is_control() { ' ' } else { c });
    let mut out: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

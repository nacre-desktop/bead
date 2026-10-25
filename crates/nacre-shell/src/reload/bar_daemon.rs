// SPDX-License-Identifier: GPL-3.0-only
//! Reconnecting daemon projections for the always-visible bar status icons.
use bead::core::{
    Value,
    source::{self, Snapshot, Source, Subscription},
};
use nacre_protocol::core_services::{
    AudioKind, AudioState, BatteryState, BluetoothState, CoreSnapshot, NetworkState,
};
use std::{collections::BTreeMap, sync::Arc};

const KEYS: [&str; 10] = [
    "bead.desktop.network",
    "bead.desktop.bluetooth",
    "bead.desktop.battery",
    "bead.desktop.battery.value",
    "bead.desktop.battery.value.width",
    "bead.desktop.audio",
    "bead.desktop.audio.percent",
    "bead.desktop.audio.device",
    "bead.desktop.audio.label",
    "bead.desktop.audio.available",
];

fn unavailable() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (KEYS[0].into(), Value::Text("wifi_off".into())),
        (KEYS[1].into(), Value::Text("bluetooth_disabled".into())),
        (KEYS[2].into(), Value::Text("battery_unknown".into())),
        (KEYS[3].into(), Value::Text(String::new())),
        (KEYS[4].into(), Value::Number(3.)),
        (KEYS[5].into(), Value::Text("volume_mute".into())),
        (KEYS[6].into(), Value::Number(0.)),
        (KEYS[7].into(), Value::Text("Output unavailable".into())),
        (KEYS[8].into(), Value::Text("Unavailable".into())),
        (KEYS[9].into(), Value::Bool(false)),
    ])
}

fn network(snapshot: &Snapshot) -> Value {
    let state = serde_json::from_value::<NetworkState>((*snapshot.data).clone()).ok();
    let connected = snapshot.connected
        && state.as_ref().is_some_and(|state| {
            state.health.available
                && !state.health.stale
                && CoreSnapshot::Network(state.clone()).valid()
                && state.wireless_enabled
                && state.devices.items.values().any(|device| {
                    device.connected && matches!(device.kind.as_str(), "wifi" | "wireless")
                })
        });
    Value::Text(if connected { "wifi" } else { "wifi_off" }.into())
}

fn bluetooth(snapshot: &Snapshot) -> Value {
    let state = serde_json::from_value::<BluetoothState>((*snapshot.data).clone()).ok();
    let powered = snapshot.connected
        && state.as_ref().is_some_and(|state| {
            CoreSnapshot::Bluetooth(state.clone()).valid()
                && state.health.available
                && !state.health.stale
                && state.adapters.items.values().any(|adapter| adapter.powered)
        });
    Value::Text(
        if powered {
            "bluetooth"
        } else {
            "bluetooth_disabled"
        }
        .into(),
    )
}

fn battery(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<BatteryState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && CoreSnapshot::Battery(state.clone()).valid()
                && state.health.available
                && !state.health.stale
        });
    let Some(state) = state else {
        return unavailable()
            .into_iter()
            .filter(|(key, _)| KEYS[2..5].contains(&key.as_str()))
            .collect();
    };
    let value = state
        .percentage
        .map_or(String::new(), |percentage| format!("{percentage:.0}"));
    BTreeMap::from([
        (
            KEYS[2].into(),
            Value::Text(
                if state.charging == Some(true) {
                    "battery_charging_full"
                } else {
                    "battery_full"
                }
                .into(),
            ),
        ),
        (KEYS[3].into(), Value::Text(value.clone())),
        (
            KEYS[4].into(),
            Value::Number(if value.is_empty() {
                3.
            } else {
                7. + 7.5 * value.len() as f64
            }),
        ),
    ])
}

fn audio(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<AudioState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && CoreSnapshot::Audio(state.clone()).valid()
                && state.health.available
                && !state.health.stale
        });
    let node = state.as_ref().and_then(|state| {
        state
            .default_output
            .as_ref()
            .and_then(|id| state.nodes.items.get(id))
            .filter(|node| node.kind == AudioKind::Output)
    });
    let level = node.and_then(|node| {
        if let Some(readback) = &node.readback {
            readback
                .perceptual_volume
                .filter(|value| value.is_finite() && *value >= 0.)
        } else if node.ready {
            Some(node.volume.max(0.).cbrt())
        } else {
            None
        }
    });
    let muted = node.and_then(|node| {
        if let Some(readback) = &node.readback {
            readback.muted
        } else if node.ready {
            Some(node.muted)
        } else {
            None
        }
    });
    let percent = level.map_or(0., |value| value * 100.);
    let label = level.map_or_else(
        || "Unavailable".into(),
        |value| format!("{:.0}%", value * 100.),
    );
    let icon = if muted == Some(true) {
        "volume_off"
    } else if level.is_some() {
        "volume_up"
    } else {
        "volume_mute"
    };
    BTreeMap::from([
        (KEYS[5].into(), Value::Text(icon.into())),
        (KEYS[6].into(), Value::Number(percent)),
        (
            KEYS[7].into(),
            Value::Text(node.map_or_else(
                || "Output unavailable".into(),
                |node| {
                    if node.name.trim().is_empty() {
                        "Default output".into()
                    } else {
                        node.name.clone()
                    }
                },
            )),
        ),
        (KEYS[8].into(), Value::Text(label)),
        (KEYS[9].into(), Value::Bool(level.is_some())),
    ])
}

fn project(name: &str, snapshot: &Snapshot) -> BTreeMap<String, Value> {
    match name {
        "network" => BTreeMap::from([(KEYS[0].into(), network(snapshot))]),
        "bluetooth" => BTreeMap::from([(KEYS[1].into(), bluetooth(snapshot))]),
        "battery" => battery(snapshot),
        "audio" => audio(snapshot),
        _ => BTreeMap::new(),
    }
}

pub struct BarDaemon {
    topics: Vec<(&'static str, Arc<dyn Source>)>,
}
impl BarDaemon {
    pub fn new(client: &crate::nacre_source::Client) -> source::Result<Self> {
        let topics = ["network", "bluetooth", "battery", "audio"]
            .into_iter()
            .map(|name| client.view_topic(name).map(|source| (name, source)))
            .collect::<source::Result<Vec<_>>>()?;
        Ok(Self { topics })
    }
    pub fn values(&self) -> BTreeMap<String, Value> {
        let mut values = unavailable();
        for (name, source) in &self.topics {
            values.extend(project(name, &source.snapshot()));
        }
        values
    }
    pub fn subscribe(
        &self,
        wake: Arc<dyn Fn(BTreeMap<String, Value>) + Send + Sync>,
    ) -> source::Result<Vec<Subscription>> {
        let mut subscriptions = Vec::new();
        for (name, source) in &self.topics {
            let name = *name;
            let wake = wake.clone();
            subscriptions.push(source.subscribe(Arc::new(move |snapshot| {
                wake(project(name, &snapshot));
            }))?);
        }
        Ok(subscriptions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(data: serde_json::Value, connected: bool) -> Snapshot {
        Snapshot {
            epoch: "fixture-boot".into(),
            revision: 0,
            data: Arc::new(data),
            connected,
        }
    }

    #[test]
    fn network_and_battery_projections_recover_after_disconnection() {
        let network = serde_json::json!({
            "health":{"available":true,"stale":false}, "wireless_enabled":true,
            "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
            "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
            "connections":{"items":{"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wifi-connection"]},
            "primary_connection":"wifi-connection"
        });
        assert_eq!(
            super::network(&snapshot(network.clone(), false)),
            Value::Text("wifi_off".into())
        );
        assert_eq!(
            super::network(&snapshot(network, true)),
            Value::Text("wifi".into())
        );

        let battery_data = serde_json::json!({
            "health":{"available":true,"stale":false}, "percentage":83.0, "charging":true,
            "state":1, "time_to_full_seconds":5400, "on_battery":false
        });
        assert_eq!(
            battery(&snapshot(battery_data.clone(), true))[KEYS[3]],
            Value::Text("83".into())
        );
        assert_eq!(
            battery(&snapshot(battery_data, false))[KEYS[2]],
            Value::Text("battery_unknown".into())
        );
    }

    #[test]
    fn connected_wifi_device_does_not_require_a_profile_or_access_point() {
        // Real daemon shape: device kind is "wifi"; profile/AP discovery may be empty.
        let data = serde_json::json!({
            "health":{"available":true,"stale":false}, "wireless_enabled":true,
            "devices":{"items":{"wlan0":{"name":"wlan0","kind":"wifi","connected":true}},"order":["wlan0"]},
            "access_points":{"items":{},"order":[]},
            "connections":{"items":{},"order":[]}, "primary_connection":null
        });
        assert_eq!(
            super::network(&snapshot(data.clone(), true)),
            Value::Text("wifi".into())
        );
        for (field, value) in [
            ("wireless_enabled", serde_json::json!(false)),
            ("health", serde_json::json!({"available":true,"stale":true})),
            (
                "devices",
                serde_json::json!({"items":{"eth0":{"name":"Wired","kind":"ethernet","connected":true}},"order":["eth0"]}),
            ),
        ] {
            let mut off = data.clone();
            off[field] = value;
            assert_eq!(
                super::network(&snapshot(off, true)),
                Value::Text("wifi_off".into())
            );
        }
        let mut disconnected = data;
        disconnected["devices"]["items"]["wlan0"]["connected"] = serde_json::json!(false);
        assert_eq!(
            super::network(&snapshot(disconnected, true)),
            Value::Text("wifi_off".into())
        );
    }

    #[test]
    fn absent_or_stale_topics_project_unavailable_icons() {
        let unavailable = snapshot(serde_json::json!({}), false);
        assert_eq!(super::network(&unavailable), Value::Text("wifi_off".into()));
        assert_eq!(
            bluetooth(&unavailable),
            Value::Text("bluetooth_disabled".into())
        );
        assert_eq!(
            battery(&unavailable)[KEYS[2]],
            Value::Text("battery_unknown".into())
        );
    }

    #[test]
    fn bluetooth_and_audio_project_fresh_values_and_recover_after_disconnect() {
        let bluetooth_data = serde_json::json!({
            "health":{"available":true,"stale":false},
            "adapters":{"items":{"adapter":{"name":"Adapter","powered":true,"discovering":false}},"order":["adapter"]},
            "devices":{"items":{},"order":[]}
        });
        assert_eq!(
            bluetooth(&snapshot(bluetooth_data.clone(), true)),
            Value::Text("bluetooth".into())
        );
        assert_eq!(
            bluetooth(&snapshot(bluetooth_data, false)),
            Value::Text("bluetooth_disabled".into())
        );

        let audio_data = serde_json::json!({
            "health":{"available":true,"stale":false}, "default_output":"out",
            "default_input":null,
            "nodes":{"items":{"out":{"name":"Studio speakers","kind":"output","volume":0.125,"muted":false,"ready":true,"readback":{"linear_volume":0.125,"perceptual_volume":0.5,"muted":false}}},"order":["out"]}
        });
        let fresh = audio(&snapshot(audio_data.clone(), true));
        assert_eq!(fresh[KEYS[5]], Value::Text("volume_up".into()));
        assert_eq!(fresh[KEYS[6]], Value::Number(50.));
        assert_eq!(fresh[KEYS[7]], Value::Text("Studio speakers".into()));
        assert_eq!(fresh[KEYS[9]], Value::Bool(true));
        let down = audio(&snapshot(audio_data, false));
        assert_eq!(down[KEYS[5]], Value::Text("volume_mute".into()));
        assert_eq!(down[KEYS[8]], Value::Text("Unavailable".into()));
        assert_eq!(down[KEYS[9]], Value::Bool(false));
    }
}

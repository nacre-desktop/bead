// SPDX-License-Identifier: GPL-3.0-only
//! Read-only projections of the existing hosted Desktop Source, not new backends.
use bead::core::{
    Value,
    source::{self, Source},
};
use nacre_protocol::core_services::{BatteryState, BluetoothState, CoreSnapshot, NetworkState};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

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
    mode: Mode,
    last_good: RefCell<Option<BTreeMap<String, Value>>>,
}
#[derive(Clone, Copy)]
enum Mode {
    Legacy,
    CoreNetwork,
    CoreBluetooth,
    CoreBattery,
}
impl Status {
    pub fn new(kind: Kind, source: Arc<dyn Source>) -> Self {
        Self {
            kind,
            source,
            mode: Mode::Legacy,
            last_good: RefCell::new(None),
        }
    }
    pub fn core_network(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Network,
            source,
            mode: Mode::CoreNetwork,
            last_good: RefCell::new(None),
        }
    }
    pub fn core_bluetooth(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Bluetooth,
            source,
            mode: Mode::CoreBluetooth,
            last_good: RefCell::new(None),
        }
    }
    pub fn core_battery(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Battery,
            source,
            mode: Mode::CoreBattery,
            last_good: RefCell::new(None),
        }
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
        match self.mode {
            Mode::CoreNetwork => return self.project_core_network(),
            Mode::CoreBluetooth => return self.project_core_bluetooth(),
            Mode::CoreBattery => return self.project_core_battery(),
            Mode::Legacy => {}
        }
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

    fn project_core_network(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let network = serde_json::from_value::<NetworkState>(snapshot.data.as_ref().clone())
            .ok()
            .map(CoreSnapshot::Network);
        let fresh = snapshot.connected
            && network.as_ref().is_some_and(|network| {
                matches!(network, CoreSnapshot::Network(state) if state.health.available && !state.health.stale)
                    && network.valid()
            });
        if fresh && let Some(CoreSnapshot::Network(state)) = network {
            let display = core_network_display(&state);
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_network_values(
                display,
                true,
                if state.wireless_enabled {
                    "Wi-Fi on"
                } else {
                    "Wi-Fi off"
                },
            );
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text("Network unavailable".into()),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(
                if cached.is_some() {
                    "Status stale"
                } else {
                    "Status data unavailable"
                }
                .into(),
            ),
        );
        display.insert("status.heading".into(), Value::Text("Network".into()));
        display.insert("status.icon".into(), Value::Text("wifi".into()));
        display.insert("status.kind".into(), Value::Text("network".into()));
        display
    }

    fn project_core_bluetooth(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let bluetooth = serde_json::from_value::<BluetoothState>(snapshot.data.as_ref().clone())
            .ok()
            .map(CoreSnapshot::Bluetooth);
        let fresh = snapshot.connected
            && bluetooth.as_ref().is_some_and(|state| {
                matches!(state, CoreSnapshot::Bluetooth(value) if value.health.available && !value.health.stale)
                    && state.valid()
            });
        if fresh && let Some(CoreSnapshot::Bluetooth(state)) = bluetooth {
            let display = core_bluetooth_display(&state);
            let secondary = if display["status.active"] == Value::Bool(true) {
                "Bluetooth on"
            } else {
                "Bluetooth off"
            };
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_bluetooth_values(display, true, secondary);
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text("Bluetooth unavailable".into()),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(
                if cached.is_some() {
                    "Status stale"
                } else {
                    "Status data unavailable"
                }
                .into(),
            ),
        );
        display.insert("status.heading".into(), Value::Text("Bluetooth".into()));
        display.insert("status.icon".into(), Value::Text("bluetooth".into()));
        display.insert("status.kind".into(), Value::Text("bluetooth".into()));
        display
    }

    fn project_core_battery(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let battery = serde_json::from_value::<BatteryState>(snapshot.data.as_ref().clone())
            .ok()
            .map(CoreSnapshot::Battery);
        let fresh = snapshot.connected
            && battery.as_ref().is_some_and(|state| {
                matches!(state, CoreSnapshot::Battery(value) if value.health.available && !value.health.stale)
                    && state.valid()
            });
        if fresh && let Some(CoreSnapshot::Battery(state)) = battery {
            let display = core_battery_display(&state);
            let secondary = match state.charging {
                Some(true) => "Charging",
                Some(false) => "Not charging",
                None => "Charging state unknown",
            };
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_battery_values(display, true, secondary);
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text("Battery unavailable".into()),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(
                if cached.is_some() {
                    "Status stale"
                } else {
                    "Status data unavailable"
                }
                .into(),
            ),
        );
        display.insert("status.heading".into(), Value::Text("Battery".into()));
        display.insert("status.icon".into(), Value::Text("battery_full".into()));
        display.insert("status.kind".into(), Value::Text("battery".into()));
        display
    }
}

fn core_network_display(state: &NetworkState) -> BTreeMap<String, Value> {
    let primary = state.primary_connection.as_ref();
    let connection = primary.and_then(|id| state.connections.items.get(id));
    let matching_ap = primary.and_then(|id| {
        state
            .access_points
            .order
            .iter()
            .filter_map(|ap_id| state.access_points.items.get(ap_id))
            .find(|ap| ap.active && ap.connection.as_ref() == Some(id))
    });
    let connection_name = connection
        .map(|connection| connection.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Connected");
    let primary_text = matching_ap
        .map(|ap| ap.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if connection.is_some() {
                connection_name
            } else {
                "Not connected"
            }
        });
    let detail = matching_ap
        .map(|ap| format!("Signal strength {}%", ap.strength))
        .unwrap_or_else(|| "Connection controls unavailable".into());
    BTreeMap::from([
        ("status.primary".into(), Value::Text(line(primary_text, 32))),
        ("status.detail".into(), Value::Text(line(&detail, 48))),
        (
            "status.active".into(),
            Value::Bool(primary.is_some() || state.wireless_enabled),
        ),
    ])
}

fn core_network_values(
    mut display: BTreeMap<String, Value>,
    ready: bool,
    secondary: &str,
) -> BTreeMap<String, Value> {
    display.insert("status.ready".into(), Value::Bool(ready));
    display.insert("status.heading".into(), Value::Text("Network".into()));
    display.insert("status.icon".into(), Value::Text("wifi".into()));
    display.insert("status.kind".into(), Value::Text("network".into()));
    display.insert("status.secondary".into(), Value::Text(secondary.into()));
    display
}
fn core_bluetooth_display(state: &BluetoothState) -> BTreeMap<String, Value> {
    let powered = state.adapters.items.values().any(|adapter| adapter.powered);
    let connected = state
        .devices
        .order
        .iter()
        .filter_map(|id| state.devices.items.get(id))
        .filter(|device| {
            device.connected
                && state
                    .adapters
                    .items
                    .get(&device.adapter)
                    .is_some_and(|adapter| adapter.powered)
        })
        .collect::<Vec<_>>();
    let primary = if !powered {
        "Bluetooth off".to_owned()
    } else if let Some(device) = connected.first() {
        if device.name.is_empty() {
            "Connected device".to_owned()
        } else {
            device.name.clone()
        }
    } else {
        "No connected devices".to_owned()
    };
    let detail = if connected.is_empty() {
        "Connection controls unavailable".to_owned()
    } else {
        format!("{} connected devices", connected.len())
    };
    BTreeMap::from([
        ("status.primary".into(), Value::Text(line(&primary, 32))),
        ("status.detail".into(), Value::Text(line(&detail, 48))),
        ("status.active".into(), Value::Bool(powered)),
    ])
}
fn core_bluetooth_values(
    mut display: BTreeMap<String, Value>,
    ready: bool,
    secondary: &str,
) -> BTreeMap<String, Value> {
    display.insert("status.ready".into(), Value::Bool(ready));
    display.insert("status.heading".into(), Value::Text("Bluetooth".into()));
    display.insert("status.icon".into(), Value::Text("bluetooth".into()));
    display.insert("status.kind".into(), Value::Text("bluetooth".into()));
    display.insert("status.secondary".into(), Value::Text(secondary.into()));
    display
}
fn core_battery_display(state: &BatteryState) -> BTreeMap<String, Value> {
    let primary = state
        .percentage
        .map(|percentage| format!("{percentage:.0}%"))
        .unwrap_or_else(|| "Battery level unknown".into());
    BTreeMap::from([
        ("status.primary".into(), Value::Text(line(&primary, 32))),
        (
            "status.detail".into(),
            Value::Text("Power controls unavailable".into()),
        ),
        (
            "status.active".into(),
            Value::Bool(state.charging == Some(true)),
        ),
    ])
}
fn core_battery_values(
    mut display: BTreeMap<String, Value>,
    ready: bool,
    secondary: &str,
) -> BTreeMap<String, Value> {
    display.insert("status.ready".into(), Value::Bool(ready));
    display.insert("status.heading".into(), Value::Text("Battery".into()));
    display.insert("status.icon".into(), Value::Text("battery_full".into()));
    display.insert("status.kind".into(), Value::Text("battery".into()));
    display.insert("status.secondary".into(), Value::Text(secondary.into()));
    display
}
fn line(text: &str, max: usize) -> String {
    let mut chars = text.chars().map(|c| if c.is_control() { ' ' } else { c });
    let mut out: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

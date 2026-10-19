// SPDX-License-Identifier: GPL-3.0-only
//! Read-only projections of the existing hosted Desktop Source, not new backends.
use bead::core::{
    Value,
    source::{self, Source},
};
use nacre_protocol::core_services::{
    AudioKind, AudioState, BatteryState, BluetoothState, CoreSnapshot, NetworkState,
};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Network,
    Bluetooth,
    Battery,
    /// Dedicated Sound route; intentionally absent from the three bar status popouts.
    Audio,
}
impl Kind {
    pub const ALL: [Self; 3] = [Self::Network, Self::Bluetooth, Self::Battery];
    pub fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Battery => "battery",
            Self::Audio => "audio",
        }
    }
    pub fn projection(self) -> fn(&bead::services::Snapshot) -> serde_json::Value {
        match self {
            Self::Network => |s| serde_json::json!({"network":s.network}),
            Self::Bluetooth => |s| serde_json::json!({"bluetooth":s.bluetooth}),
            Self::Battery => |s| serde_json::json!({"battery":s.battery}),
            Self::Audio => |s| serde_json::json!({"audio":s.audio}),
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Battery => "Battery",
            Self::Audio => "Sound output",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Network => "wifi",
            Self::Bluetooth => "bluetooth",
            Self::Battery => "battery_full",
            Self::Audio => "volume_up",
        }
    }
}

pub struct Status {
    kind: Kind,
    source: Arc<dyn Source>,
    mode: Mode,
    last_good: RefCell<Option<BTreeMap<String, Value>>>,
    wifi_control: Option<RefCell<WifiControl>>,
    bluetooth_control: Option<RefCell<BluetoothControl>>,
}
struct WifiControl {
    client: crate::nacre_source::Client,
    pending: Option<(String, crate::nacre_source::CallTicket)>,
    message: Option<String>,
    last_boot: Option<String>,
    checked: bool,
}
struct BluetoothControl {
    client: crate::nacre_source::Client,
    pending: Option<(String, String, crate::nacre_source::CallTicket)>,
    message: Option<String>,
    last_boot: Option<String>,
    checked: bool,
}
#[derive(Clone, Copy)]
enum Mode {
    Legacy,
    CoreNetwork,
    CoreNetworkControl,
    CoreBluetooth,
    CoreBluetoothControl,
    CoreBattery,
    CoreAudio,
}
impl Status {
    pub fn new(kind: Kind, source: Arc<dyn Source>) -> Self {
        Self {
            kind,
            source,
            mode: Mode::Legacy,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: None,
        }
    }
    pub fn core_network(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Network,
            source,
            mode: Mode::CoreNetwork,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: None,
        }
    }
    pub fn core_network_control(
        source: Arc<dyn Source>,
        client: crate::nacre_source::Client,
    ) -> Self {
        Self {
            kind: Kind::Network,
            source,
            mode: Mode::CoreNetworkControl,
            last_good: RefCell::new(None),
            wifi_control: Some(RefCell::new(WifiControl {
                client,
                pending: None,
                message: None,
                last_boot: None,
                checked: false,
            })),
            bluetooth_control: None,
        }
    }
    pub fn core_bluetooth(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Bluetooth,
            source,
            mode: Mode::CoreBluetooth,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: None,
        }
    }
    pub fn core_bluetooth_control(
        source: Arc<dyn Source>,
        client: crate::nacre_source::Client,
    ) -> Self {
        Self {
            kind: Kind::Bluetooth,
            source,
            mode: Mode::CoreBluetoothControl,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: Some(RefCell::new(BluetoothControl {
                client,
                pending: None,
                message: None,
                last_boot: None,
                checked: false,
            })),
        }
    }
    pub fn core_battery(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Battery,
            source,
            mode: Mode::CoreBattery,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: None,
        }
    }
    pub fn core_audio(source: Arc<dyn Source>) -> Self {
        Self {
            kind: Kind::Audio,
            source,
            mode: Mode::CoreAudio,
            last_good: RefCell::new(None),
            wifi_control: None,
            bluetooth_control: None,
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
    pub fn subscribe_control_link(
        &self,
        notify: crate::nacre_source::CallWake,
    ) -> source::Result<source::Subscription> {
        if let Some(control) = &self.wifi_control {
            return control
                .borrow()
                .client
                .subscribe(Arc::new(move |_| notify()));
        }
        if let Some(control) = &self.bluetooth_control {
            return control
                .borrow()
                .client
                .subscribe(Arc::new(move |_| notify()));
        }
        Err(source::Error::new(
            "unavailable",
            "Control is disabled",
            false,
        ))
    }
    pub fn wifi_action(&self, wake: crate::nacre_source::CallWake) {
        let Some(control) = &self.wifi_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = wifi_candidate(&snapshot, &client);
        if !candidate.eligible {
            return;
        }
        let Some(boot) = candidate.topic_boot.clone() else {
            return;
        };
        let Some(state) = candidate.state else {
            return;
        };
        let mut control = control.borrow_mut();
        if control.pending.is_some() {
            return;
        }
        if control
            .last_boot
            .as_deref()
            .is_some_and(|previous| previous != boot)
        {
            control.message = None;
        }
        control.last_boot = Some(boot.clone());
        control.message = None;
        match client.begin_call(
            "network.set_wifi".into(),
            serde_json::json!({"enabled": !state.wireless_enabled}),
            Duration::from_millis(candidate.timeout_ms.min(10_000) as u64),
            crate::nacre_source::CallGuard {
                boot: boot.clone(),
                version: 1,
            },
            Some(wake),
        ) {
            Ok(ticket) => control.pending = Some((boot, ticket)),
            Err(_) => control.message = Some("Wi-Fi change failed".into()),
        }
    }
    pub fn bluetooth_action(&self, wake: crate::nacre_source::CallWake) {
        let Some(control) = &self.bluetooth_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = bluetooth_candidate(&snapshot, &client);
        if !candidate.eligible {
            return;
        }
        let (Some(boot), Some(adapter_id)) =
            (candidate.topic_boot.clone(), candidate.adapter_id.clone())
        else {
            return;
        };
        let Some(adapter) = candidate.adapter.as_ref() else {
            return;
        };
        let mut control = control.borrow_mut();
        if control.pending.is_some() {
            return;
        }
        if control
            .last_boot
            .as_deref()
            .is_some_and(|previous| previous != boot)
        {
            control.message = None;
        }
        control.last_boot = Some(boot.clone());
        control.message = None;
        match client.begin_call(
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter": adapter_id, "powered": !adapter.powered}),
            Duration::from_millis(candidate.timeout_ms.min(10_000) as u64),
            crate::nacre_source::CallGuard {
                boot: boot.clone(),
                version: 1,
            },
            Some(wake),
        ) {
            Ok(ticket) => control.pending = Some((boot, adapter_id, ticket)),
            Err(_) => control.message = Some("Bluetooth power change failed".into()),
        }
    }
    pub fn project(&self) -> BTreeMap<String, Value> {
        match self.mode {
            Mode::CoreNetwork => return self.project_core_network(),
            Mode::CoreNetworkControl => return self.project_core_network_control(),
            Mode::CoreBluetooth => return self.project_core_bluetooth(),
            Mode::CoreBluetoothControl => return self.project_core_bluetooth_control(),
            Mode::CoreBattery => return self.project_core_battery(),
            Mode::CoreAudio => return self.project_core_audio(),
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
        self.project_core_network_snapshot(&snapshot)
    }

    fn project_core_network_snapshot(
        &self,
        snapshot: &bead::core::source::Snapshot,
    ) -> BTreeMap<String, Value> {
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

    fn project_core_network_control(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let control_cell = self
            .wifi_control
            .as_ref()
            .expect("controlled network has Wi-Fi controller");
        let client = control_cell.borrow().client.clone();
        let candidate = wifi_candidate(&snapshot, &client);
        let mut base = self.project_core_network_snapshot(&snapshot);
        let mut control = control_cell.borrow_mut();
        if let Some(boot) = candidate.topic_boot.as_ref()
            && control
                .last_boot
                .as_deref()
                .is_some_and(|previous| previous != boot)
        {
            control.message = None;
        }
        if let Some(boot) = candidate.topic_boot.as_ref() {
            control.last_boot = Some(boot.clone());
        }
        let current_identity = candidate.eligible_identity.as_deref();
        if control.pending.as_ref().is_some_and(|(boot, _)| {
            !candidate.link_connected || current_identity != Some(boot.as_str())
        }) {
            control.pending = None;
        }
        let terminal = control
            .pending
            .as_mut()
            .and_then(|(_, ticket)| ticket.try_take());
        if let Some(result) = terminal {
            control.pending = None;
            control.message = match result {
                Ok(payload)
                    if payload.as_object().is_some_and(|object| {
                        object.len() == 1
                            && object
                                .get("changed")
                                .and_then(serde_json::Value::as_bool)
                                .is_some()
                    }) =>
                {
                    None
                }
                _ => Some("Wi-Fi change failed".into()),
            };
        }
        let checked = candidate
            .state
            .as_ref()
            .map(|state| state.wireless_enabled)
            .unwrap_or(control.checked);
        if candidate.fresh {
            control.checked = checked;
        }
        let busy = control.pending.is_some();
        let message = if !candidate.eligible {
            "Wi-Fi control unavailable"
        } else if busy {
            "Changing Wi-Fi…"
        } else {
            control.message.as_deref().unwrap_or("")
        };
        base.extend([
            ("network.wifi_checked".into(), Value::Bool(control.checked)),
            (
                "network.wifi_enabled".into(),
                Value::Bool(candidate.eligible && !busy),
            ),
            ("network.wifi_busy".into(), Value::Bool(busy)),
            ("network.wifi_message".into(), Value::Text(message.into())),
        ]);
        base
    }

    fn project_core_bluetooth(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        self.project_core_bluetooth_snapshot(&snapshot)
    }

    fn project_core_bluetooth_snapshot(
        &self,
        snapshot: &bead::core::source::Snapshot,
    ) -> BTreeMap<String, Value> {
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

    fn project_core_bluetooth_control(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let control_cell = self
            .bluetooth_control
            .as_ref()
            .expect("controlled Bluetooth has controller");
        let client = control_cell.borrow().client.clone();
        let candidate = bluetooth_candidate(&snapshot, &client);
        let mut base = self.project_core_bluetooth_snapshot(&snapshot);
        let mut control = control_cell.borrow_mut();
        if candidate.topic_boot.as_ref().is_some_and(|boot| {
            control
                .last_boot
                .as_deref()
                .is_some_and(|previous| previous != boot)
        }) {
            control.message = None;
        }
        if let Some(boot) = candidate.topic_boot.as_ref() {
            control.last_boot = Some(boot.clone());
        }
        let same_target = candidate
            .eligible_identity
            .as_ref()
            .is_some_and(|(boot, adapter)| {
                control
                    .pending
                    .as_ref()
                    .is_some_and(|(pending_boot, pending_adapter, _)| {
                        boot == pending_boot && adapter == pending_adapter
                    })
            });
        if control.pending.is_some() && (!candidate.link_connected || !same_target) {
            control.pending = None;
        }
        let terminal = control
            .pending
            .as_mut()
            .and_then(|(_, _, ticket)| ticket.try_take());
        if let Some(result) = terminal {
            control.pending = None;
            control.message = match result {
                Ok(payload)
                    if payload.as_object().is_some_and(|object| {
                        object.len() == 1
                            && object
                                .get("changed")
                                .and_then(serde_json::Value::as_bool)
                                .is_some()
                    }) =>
                {
                    None
                }
                _ => Some("Bluetooth power change failed".into()),
            };
        }
        if candidate.fresh {
            control.checked = candidate
                .adapter
                .as_ref()
                .is_some_and(|adapter| adapter.powered);
        }
        let busy = control.pending.is_some();
        let message = if !candidate.eligible {
            "Bluetooth power control unavailable"
        } else if busy {
            "Changing Bluetooth power…"
        } else {
            control.message.as_deref().unwrap_or("")
        };
        base.extend([
            (
                "bluetooth.power_visible".into(),
                Value::Bool(candidate.fresh && candidate.adapter_id.is_some()),
            ),
            (
                "bluetooth.power_checked".into(),
                Value::Bool(control.checked),
            ),
            (
                "bluetooth.power_enabled".into(),
                Value::Bool(candidate.eligible && !busy),
            ),
            ("bluetooth.power_busy".into(), Value::Bool(busy)),
            (
                "bluetooth.power_message".into(),
                Value::Text(message.into()),
            ),
        ]);
        base
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

    fn project_core_audio(&self) -> BTreeMap<String, Value> {
        let snapshot = self.source.snapshot();
        let audio = serde_json::from_value::<AudioState>(snapshot.data.as_ref().clone())
            .ok()
            .map(CoreSnapshot::Audio);
        let fresh = snapshot.connected
            && audio.as_ref().is_some_and(|state| {
                matches!(state, CoreSnapshot::Audio(value) if value.health.available && !value.health.stale)
                    && state.valid()
            });
        if fresh && let Some(CoreSnapshot::Audio(state)) = audio {
            let display = core_audio_display(&state);
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_audio_values(display, true, "Audio status available");
        }
        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                ("status.primary".into(), Value::Text("Audio unavailable".into())),
                ("status.device".into(), Value::Text("Output unavailable".into())),
                ("status.mute".into(), Value::Text("Mute state unknown".into())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert("status.health".into(), Value::Text(if cached.is_some() { "Status stale" } else { "Status data unavailable" }.into()));
        display.insert("status.heading".into(), Value::Text("Sound output".into()));
        display.insert("status.icon".into(), Value::Text("volume_up".into()));
        display.insert("status.kind".into(), Value::Text("audio".into()));
        display
    }
}

struct WifiCandidate {
    topic_boot: Option<String>,
    eligible_identity: Option<String>,
    state: Option<NetworkState>,
    fresh: bool,
    link_connected: bool,
    eligible: bool,
    timeout_ms: u32,
}
struct BluetoothCandidate {
    topic_boot: Option<String>,
    eligible_identity: Option<(String, String)>,
    adapter_id: Option<String>,
    adapter: Option<nacre_protocol::core_services::BluetoothAdapter>,
    fresh: bool,
    link_connected: bool,
    eligible: bool,
    timeout_ms: u32,
}
fn wifi_candidate(
    topic: &bead::core::source::Snapshot,
    client: &crate::nacre_source::Client,
) -> WifiCandidate {
    let link = client.snapshot();
    let status = client.status();
    let topic_boot = topic.epoch.clone();
    let link_boot = link.data.get("boot").and_then(serde_json::Value::as_str);
    let identity_matches = topic.connected
        && link.connected
        && status.boot.as_deref() == Some(topic.epoch.as_str())
        && link_boot == Some(topic.epoch.as_str());
    let state = serde_json::from_value::<NetworkState>(topic.data.as_ref().clone()).ok();
    let fresh = identity_matches
        && state.as_ref().is_some_and(|state| {
            state.health.available
                && !state.health.stale
                && CoreSnapshot::Network(state.clone()).valid()
        });
    let method = status.methods.get("network.set_wifi");
    let eligible = fresh && method.is_some_and(|method| method.v == 1);
    WifiCandidate {
        topic_boot: Some(topic_boot),
        eligible_identity: identity_matches.then(|| topic.epoch.clone()),
        state,
        fresh,
        link_connected: link.connected,
        eligible,
        timeout_ms: method.map_or(10_000, |method| method.timeout.min(10_000)),
    }
}

fn bluetooth_candidate(
    topic: &bead::core::source::Snapshot,
    client: &crate::nacre_source::Client,
) -> BluetoothCandidate {
    let link = client.snapshot();
    let status = client.status();
    let topic_boot = topic.epoch.clone();
    let link_boot = link.data.get("boot").and_then(serde_json::Value::as_str);
    let identity_matches = topic.connected
        && link.connected
        && status.boot.as_deref() == Some(topic.epoch.as_str())
        && link_boot == Some(topic.epoch.as_str());
    let state = serde_json::from_value::<BluetoothState>(topic.data.as_ref().clone()).ok();
    let fresh = identity_matches
        && state.as_ref().is_some_and(|state| {
            state.health.available
                && !state.health.stale
                && CoreSnapshot::Bluetooth(state.clone()).valid()
        });
    let sole = state.as_ref().and_then(|state| {
        if state.adapters.order.len() != 1 || state.adapters.items.len() != 1 {
            return None;
        }
        let id = state.adapters.order.first()?;
        let adapter = state.adapters.items.get(id)?;
        Some((id.0.clone(), adapter.clone()))
    });
    let method = status.methods.get("bluetooth.set_power");
    let eligible = fresh && sole.is_some() && method.is_some_and(|method| method.v == 1);
    let (adapter_id, adapter) =
        sole.map_or((None, None), |(id, adapter)| (Some(id), Some(adapter)));
    BluetoothCandidate {
        topic_boot: Some(topic_boot.clone()),
        eligible_identity: if identity_matches && eligible {
            adapter_id.clone().map(|id| (topic_boot, id))
        } else {
            None
        },
        adapter_id,
        adapter,
        fresh,
        link_connected: link.connected,
        eligible,
        timeout_ms: method.map_or(10_000, |method| method.timeout.min(10_000)),
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
fn core_audio_display(state: &AudioState) -> BTreeMap<String, Value> {
    let node = state.default_output.as_ref()
        .and_then(|id| state.nodes.items.get(id))
        .filter(|node| node.kind == AudioKind::Output);
    let Some(node) = node else {
        return BTreeMap::from([
            ("status.primary".into(), Value::Text("Output unavailable".into())),
            ("status.device".into(), Value::Text("Output unavailable".into())),
            ("status.mute".into(), Value::Text("Mute state unknown".into())),
            ("status.active".into(), Value::Bool(false)),
        ]);
    };
    let (volume, muted) = if let Some(readback) = &node.readback {
        (
            readback.perceptual_volume.filter(|v| v.is_finite() && *v >= 0.0).map(|v| format!("{:.0}%", v * 100.)),
            readback.muted.map(|m| if m { "Muted" } else { "Not muted" }.to_owned()),
        )
    } else if node.ready {
        (
            Some(format!("{:.0}%", node.volume.cbrt() * 100.)),
            Some(if node.muted { "Muted" } else { "Not muted" }.to_owned()),
        )
    } else { (None, None) };
    let device = if node.name.trim().is_empty() { "Default output" } else { &node.name };
    let active = muted.as_deref() == Some("Muted");
    BTreeMap::from([
        (
            "status.primary".into(),
            Value::Text(line(&volume.unwrap_or_else(|| "Volume unknown".into()), 24)),
        ),
        ("status.device".into(), Value::Text(line(device, 40))),
        ("status.mute".into(), Value::Text(muted.unwrap_or_else(|| "Mute state unknown".into()))),
        ("status.active".into(), Value::Bool(active)),
    ])
}
fn core_audio_values(mut display: BTreeMap<String, Value>, ready: bool, health: &str) -> BTreeMap<String, Value> {
    display.insert("status.ready".into(), Value::Bool(ready));
    display.insert("status.health".into(), Value::Text(health.into()));
    display.insert("status.heading".into(), Value::Text("Sound output".into()));
    display.insert("status.icon".into(), Value::Text("volume_up".into()));
    display.insert("status.kind".into(), Value::Text("audio".into()));
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

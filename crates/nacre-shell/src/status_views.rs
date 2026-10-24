// SPDX-License-Identifier: GPL-3.0-only
//! Read-only projections of the existing hosted Desktop Source, not new backends.
use crate::i18n::{tr, tr_n};
use bead::core::{
    Value,
    source::{self, Source},
};
use nacre_protocol::core_services::{
    AudioKind, AudioState, BatteryState, BluetoothState, CoreSnapshot, NetworkState,
};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc, time::Duration};

mod devices;
mod join;
mod pairing;
pub(crate) use join::SECRET_STORE;
use join::{Page, Prompt, Reprompt};
pub(crate) use pairing::SECRET_STORE as BLUETOOTH_SECRET_STORE;

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
    fn title(self) -> String {
        tr(
            match self {
                Self::Network => "bead.panel.network-title",
                Self::Bluetooth => "bead.panel.bluetooth",
                Self::Battery => "bead.panel.battery-title",
                Self::Audio => "bead.panel.sound-title",
            },
            &[],
        )
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
    link: Option<LinkOp>,
    link_message: Option<String>,
    message: Option<String>,
    last_boot: Option<String>,
    checked: bool,
    /// Join/forget flow (N0042); the view itself lives in Luau, the flow stays at this edge.
    page: Page,
    prompt: Option<Prompt>,
    form_error: Option<&'static str>,
    forget: Option<(String, std::time::Instant)>,
    confirm_timeout: Duration,
}
/// One in-flight connect/disconnect. `ticket` is the admission call; after an
/// admitted change the row stays pending until topic readback or `deadline`.
struct LinkOp {
    boot: String,
    key: String,
    method: &'static str,
    ticket: Option<crate::nacre_source::CallTicket>,
    deadline: Option<std::time::Instant>,
    /// Set for joins that sent a secret: a wrong password re-opens this form.
    reprompt: Option<Reprompt>,
}
const LINK_READBACK: Duration = Duration::from_secs(30);
const MAX_LINK_ROWS: usize = 5;
/// Empty-list body height. Populated lists use 56 px cards and 8 px gaps; the
/// frontend adds its form footer only when those controls are available.
pub(crate) const NETWORK_BASE_HEIGHT: f64 = 246.;
pub(crate) fn network_popout_height(rows: usize, message: bool) -> f64 {
    let rows = rows.min(MAX_LINK_ROWS);
    (if rows == 0 {
        NETWORK_BASE_HEIGHT
    } else {
        130. + 64. * rows as f64
    }) + if message { 28. } else { 0. }
}
struct LinkTarget {
    method: &'static str,
    params: serde_json::Value,
}
struct LinkRow {
    key: String,
    label: String,
    action: &'static str,
    active: bool,
    secured: bool,
}
struct BluetoothControl {
    client: crate::nacre_source::Client,
    pending: Option<(String, String, crate::nacre_source::CallTicket)>,
    message: Option<String>,
    last_boot: Option<String>,
    checked: bool,
    /// Pairing started from the device list (N0055 edge in `pairing.rs`) and its last notice.
    flow: Option<pairing::Flow>,
    notice: Option<&'static str>,
    /// Find devices (N0047): the admission call, and (boot, adapter, seen running) of a scan this
    /// panel started, so closing it stops the scan unless it already ended.
    scan: Option<crate::nacre_source::CallTicket>,
    scan_owned: Option<(String, String, bool)>,
    /// A connect or disconnect in flight.
    link: Option<devices::DeviceOp>,
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
                link: None,
                link_message: None,
                message: None,
                last_boot: None,
                checked: false,
                page: Page::List,
                prompt: None,
                form_error: None,
                forget: None,
                confirm_timeout: join::CONFIRM_TIMEOUT,
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
                flow: None,
                notice: None,
                scan: None,
                scan_owned: None,
                link: None,
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
        if control.pending.is_some() || control.link.is_some() {
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
            Err(_) => control.message = Some(tr("bead.panel.wifi-change-failed", &[])),
        }
    }
    /// Connect (`network.connect`) or disconnect (`network.disconnect`) the row
    /// `key`. The row is re-resolved against the current fresh topic; a missing
    /// or changed row sends nothing. Never retries.
    pub fn network_link_action(
        &self,
        method: &str,
        key: &str,
        wake: crate::nacre_source::CallWake,
    ) {
        let Some(control) = &self.wifi_control else {
            return;
        };
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = wifi_candidate(&snapshot, &client);
        let (Some(boot), Some(state)) = (candidate.topic_boot.clone(), candidate.state.as_ref())
        else {
            return;
        };
        let mut control = control.borrow_mut();
        if !candidate.fresh || control.pending.is_some() || control.link.is_some() {
            return;
        }
        let status = client.status();
        let advertised = ["network.connect", "network.disconnect"]
            .into_iter()
            .find(|name| *name == method)
            .and_then(|name| status.methods.get(name).map(|m| (name, m)));
        let Some((method, advertised)) = advertised.filter(|(_, m)| m.v == 1) else {
            return;
        };
        control.link_message = None;
        let Some(target) = resolve_link(state, key).filter(|target| target.method == method) else {
            control.link_message = Some(tr("bead.panel.network-list-changed", &[]));
            return;
        };
        // A secured network without a saved profile needs its password first.
        if let Some(prompt) = join::secret_prompt(state, key, &target) {
            control.prompt = Some(prompt);
            control.page = Page::Prompt;
            control.form_error = None;
            return;
        }
        join::dispatch(
            &mut control,
            &client,
            boot,
            (method, advertised.timeout),
            target,
            key.into(),
            None,
            wake,
        );
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
            Err(_) => control.message = Some(tr("bead.panel.bluetooth-power-failed", &[])),
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
        let mut primary = tr(
            "bead.panel.status-unavailable",
            &[("name", &self.kind.title())],
        );
        let mut secondary = tr("bead.panel.status-data-unavailable", &[]);
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
                            .map_or_else(|| tr("bead.common.connected", &[]), str::to_owned)
                    } else {
                        tr("bead.common.not-connected", &[])
                    };
                    secondary = tr(
                        if wireless {
                            "bead.panel.wifi-on"
                        } else {
                            "bead.panel.wifi-off"
                        },
                        &[],
                    );
                    detail = data["strength"]
                        .as_u64()
                        .filter(|n| *n <= 100)
                        .map(|n| tr("bead.panel.signal-strength", &[("percent", &n.to_string())]))
                        .unwrap_or_else(|| tr("bead.panel.connection-controls-unavailable", &[]));
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
                        tr("bead.panel.bluetooth-off", &[])
                    } else if devices.is_empty() {
                        tr("bead.panel.bluetooth-on", &[])
                    } else {
                        tr_n(
                            "bead.panel.bluetooth-devices-connected",
                            devices.len() as u64,
                            &[],
                        )
                    };
                    secondary = tr(
                        if !powered {
                            "bead.panel.bluetooth-power-off"
                        } else if devices.is_empty() {
                            "bead.panel.bluetooth-none-connected"
                        } else {
                            "bead.panel.bluetooth-on"
                        },
                        &[],
                    );
                    detail = if powered && !devices.is_empty() {
                        devices
                            .iter()
                            .take(2)
                            .filter_map(serde_json::Value::as_str)
                            .map(|s| line(s, 24))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        tr("bead.panel.bluetooth-device-controls-unavailable", &[])
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
                    secondary = tr(
                        if charging {
                            "bead.panel.battery-charging"
                        } else {
                            "bead.panel.battery-not-charging"
                        },
                        &[],
                    );
                    detail = tr("bead.panel.battery-profiles-unavailable", &[]);
                }
            }
            _ => {}
        }
        BTreeMap::from([
            ("status.ready".into(), Value::Bool(ready)),
            ("status.active".into(), Value::Bool(active)),
            ("status.heading".into(), Value::Text(self.kind.title())),
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
                &tr(
                    if state.wireless_enabled {
                        "bead.panel.wifi-on"
                    } else {
                        "bead.panel.wifi-off"
                    },
                    &[],
                ),
            );
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text(tr("bead.panel.network-unavailable", &[])),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(tr(
                if cached.is_some() {
                    "bead.panel.status-stale"
                } else {
                    "bead.panel.status-data-unavailable"
                },
                &[],
            )),
        );
        display.insert(
            "status.heading".into(),
            Value::Text(tr("bead.panel.network-title", &[])),
        );
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
            control.link_message = None;
            control.reset_flow();
        }
        if let Some(boot) = candidate.topic_boot.as_ref() {
            control.last_boot = Some(boot.clone());
        }
        if !candidate.fresh {
            control.reset_flow();
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
                _ => Some(tr("bead.panel.wifi-change-failed", &[])),
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
        let authentication_error = candidate.state.as_ref().is_some_and(|state| {
            state.health.available
                && !state.health.stale
                && state.health.error.as_ref().is_some_and(|error| {
                    error.code == nacre_protocol::ErrorCode::Unavailable
                        && error.retry
                        && error.message == "Network authentication required or cancelled"
                })
        });
        if candidate
            .topic_boot
            .as_ref()
            .is_some_and(|boot| control.link.as_ref().is_some_and(|op| &op.boot != boot))
            || !candidate.link_connected
        {
            control.link = None;
        }
        let link_terminal = control
            .link
            .as_mut()
            .and_then(|op| op.ticket.as_mut().and_then(|ticket| ticket.try_take()));
        if let Some(result) = link_terminal {
            let join = control
                .link
                .as_ref()
                .is_some_and(|op| op.reprompt.is_some());
            match result {
                Ok(payload)
                    if payload.as_object().is_some_and(|object| {
                        object.len() == 1
                            && object
                                .get("changed")
                                .and_then(serde_json::Value::as_bool)
                                .is_some()
                    }) =>
                {
                    let changed = payload["changed"].as_bool() == Some(true);
                    if join {
                        // The form is done; the row shows progress and a wrong password re-opens it.
                        control.reset_flow();
                    }
                    if changed && let Some(op) = control.link.as_mut() {
                        op.ticket = None;
                        op.deadline = Some(std::time::Instant::now() + LINK_READBACK);
                    } else {
                        control.link = None;
                    }
                }
                Ok(_) => {
                    control.link = None;
                    control.link_message = Some(tr("bead.panel.network-change-failed", &[]));
                }
                Err(error) => {
                    let key = control.link.take().map(|op| op.key);
                    if join {
                        control.form_error = Some(join::join_error_id(&error.code, &error.message));
                    } else if let Some(prompt) = key.and_then(|key| {
                        join::needs_secret(&error.code, &error.message)
                            .then(|| {
                                candidate
                                    .state
                                    .as_ref()
                                    .and_then(|s| join::prompt_for(s, &key))
                            })
                            .flatten()
                    }) {
                        // The daemon wants a password the tap did not carry (for example the saved
                        // profile is gone): ask for it instead of failing.
                        control.prompt = Some(prompt);
                        control.page = Page::Prompt;
                        control.form_error = None;
                    } else {
                        control.link_message = Some(link_error_message(&error.code));
                    }
                }
            }
        }
        // After admission the topic readback decides: clear once the row no longer
        // resolves to the same operation, on authentication failure, or at deadline.
        if let Some(op) = control.link.as_ref()
            && op.ticket.is_none()
            && (authentication_error
                || op.deadline.is_some_and(|d| std::time::Instant::now() >= d)
                || !candidate.fresh
                || candidate
                    .state
                    .as_ref()
                    .is_none_or(|state| !join::still_pending(state, op)))
            && let Some(op) = control.link.take()
            && authentication_error
            && let Some(again) = op.reprompt
        {
            control.reopen(again);
        }
        let (can_connect, can_disconnect) = {
            let methods = client.status().methods;
            let has = |name: &str| methods.get(name).is_some_and(|m| m.v == 1);
            (has("network.connect"), has("network.disconnect"))
        };
        let link_busy = control.link.is_some();
        let rows = match candidate.state.as_ref().filter(|_| candidate.fresh) {
            Some(state) => link_rows(state)
                .into_iter()
                .map(|row| {
                    let pending = control.link.as_ref().is_some_and(|op| op.key == row.key);
                    let allowed = if row.active {
                        can_disconnect
                    } else {
                        can_connect
                    };
                    let action = tr(
                        if pending {
                            if row.active {
                                "bead.panel.network-disconnecting"
                            } else {
                                "bead.panel.network-connecting"
                            }
                        } else if !row.active && authentication_error {
                            "bead.panel.network-retry"
                        } else {
                            row.action
                        },
                        &[],
                    );
                    serde_json::json!({
                        "id": row.key,
                        "label": row.label,
                        "action": action,
                        "active": row.active,
                        "secured": row.secured,
                        "enabled": allowed && !link_busy && !busy,
                    })
                })
                .collect(),
            None => Vec::new(),
        };
        let row_count = rows.len();
        let forms = join::project(
            &mut control,
            candidate.state.as_ref().filter(|_| candidate.fresh),
            &client,
        );
        let link_message = if link_busy {
            ""
        } else {
            control.link_message.as_deref().unwrap_or("")
        }
        .to_owned();
        let message = if !candidate.eligible {
            tr("bead.panel.wifi-control-unavailable", &[])
        } else if busy {
            tr("bead.panel.wifi-changing", &[])
        } else if authentication_error {
            tr("bead.panel.network-auth-required", &[])
        } else {
            control.message.clone().unwrap_or_default()
        };
        base.extend([
            ("network.wifi_checked".into(), Value::Bool(control.checked)),
            (
                "network.wifi_enabled".into(),
                Value::Bool(candidate.eligible && !busy),
            ),
            ("network.wifi_busy".into(), Value::Bool(busy)),
            ("network.wifi_message".into(), Value::Text(message)),
            (
                "network.health_message".into(),
                Value::Text(line(
                    snapshot.data["health"]["error"]["message"]
                        .as_str()
                        .unwrap_or(""),
                    160,
                )),
            ),
            ("network.rows".into(), Value::list(rows)),
            (
                "network.height".into(),
                Value::Number(network_popout_height(row_count, !link_message.is_empty())),
            ),
            (
                "network.link_visible".into(),
                Value::Bool(!link_message.is_empty()),
            ),
            ("network.link_message".into(), Value::Text(link_message)),
        ]);
        base.extend(forms);
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
            let secondary = tr(
                if display["status.active"] == Value::Bool(true) {
                    "bead.panel.bluetooth-on"
                } else {
                    "bead.panel.bluetooth-off"
                },
                &[],
            );
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_bluetooth_values(display, true, &secondary);
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text(tr("bead.panel.bluetooth-unavailable", &[])),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(tr(
                if cached.is_some() {
                    "bead.panel.status-stale"
                } else {
                    "bead.panel.status-data-unavailable"
                },
                &[],
            )),
        );
        display.insert(
            "status.heading".into(),
            Value::Text(tr("bead.panel.bluetooth", &[])),
        );
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
                _ => Some(tr("bead.panel.bluetooth-power-failed", &[])),
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
            tr("bead.panel.bluetooth-power-unavailable", &[])
        } else if busy {
            tr("bead.panel.bluetooth-power-changing", &[])
        } else {
            control.message.clone().unwrap_or_default()
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
            ("bluetooth.power_message".into(), Value::Text(message)),
        ]);
        base.extend(pairing::project(&mut control, &candidate));
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
            let secondary = battery_estimate(&state);
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_battery_values(display, true, &secondary);
        }

        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text(tr("bead.panel.battery-unavailable", &[])),
                ),
                ("status.detail".into(), Value::Text(String::new())),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.secondary".into(),
            Value::Text(tr(
                if cached.is_some() {
                    "bead.panel.status-stale"
                } else {
                    "bead.panel.status-data-unavailable"
                },
                &[],
            )),
        );
        display.insert(
            "status.heading".into(),
            Value::Text(tr("bead.panel.battery-title", &[])),
        );
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
            let mut display = core_audio_display(&state);
            let input = state
                .default_input
                .as_ref()
                .and_then(|id| state.nodes.items.get(id))
                .filter(|node| node.ready)
                .map(|node| {
                    if node.name.trim().is_empty() {
                        tr("bead.panel.audio-default-input", &[])
                    } else {
                        line(&node.name, 40)
                    }
                })
                .unwrap_or_else(|| tr("bead.panel.audio-input-unavailable", &[]));
            let streams = state
                .nodes
                .order
                .iter()
                .filter_map(|id| state.nodes.items.get(id).map(|node| (id, node)))
                .filter(|(_, node)| {
                    matches!(&node.kind, &AudioKind::Playback | &AudioKind::Capture)
                })
                .map(|(id, node)| {
                    serde_json::json!({
                        "id": id.0.clone(),
                        "label": if node.name.trim().is_empty() {
                            tr("bead.panel.audio-application", &[])
                        } else {
                            line(&node.name, 40)
                        },
                    })
                })
                .collect::<Vec<_>>();
            display.insert("audio.input_device".into(), Value::Text(input));
            display.insert("audio.streams".into(), Value::list(streams));
            *self.last_good.borrow_mut() = Some(display.clone());
            return core_audio_values(display, true, &tr("bead.panel.audio-available", &[]));
        }
        let cached = self.last_good.borrow().clone();
        let mut display = cached.clone().unwrap_or_else(|| {
            BTreeMap::from([
                (
                    "status.primary".into(),
                    Value::Text(tr("bead.panel.audio-unavailable", &[])),
                ),
                (
                    "status.device".into(),
                    Value::Text(tr("bead.panel.audio-output-unavailable", &[])),
                ),
                (
                    "status.mute".into(),
                    Value::Text(tr("bead.panel.audio-mute-unknown", &[])),
                ),
                (
                    "audio.input_device".into(),
                    Value::Text(tr("bead.panel.audio-input-unavailable", &[])),
                ),
                (
                    "audio.streams".into(),
                    Value::list(Vec::<serde_json::Value>::new()),
                ),
                ("status.active".into(), Value::Bool(false)),
            ])
        });
        display.insert("status.ready".into(), Value::Bool(false));
        display.insert(
            "status.health".into(),
            Value::Text(tr(
                if cached.is_some() {
                    "bead.panel.status-stale"
                } else {
                    "bead.panel.status-data-unavailable"
                },
                &[],
            )),
        );
        display.insert(
            "status.heading".into(),
            Value::Text(tr("bead.panel.sound-title", &[])),
        );
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
    state: Option<BluetoothState>,
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
        state,
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

fn link_error_message(code: &str) -> String {
    tr(link_error_id(code), &[])
}
fn link_error_id(code: &str) -> &'static str {
    match code {
        "permission_denied" => "bead.panel.network-not-permitted",
        "stale_revision" => "bead.panel.network-list-changed-retry",
        "unavailable" => "bead.panel.network-control-unavailable",
        "busy" => "bead.panel.network-busy",
        "invalid_request" | "validation" => "bead.panel.network-rejected",
        "timeout" => "bead.panel.network-timed-out",
        _ => "bead.panel.network-change-failed",
    }
}
/// The single device of `kind`, optionally required to be connected.
fn sole_device<'a>(
    state: &'a NetworkState,
    kind: &str,
    connected: bool,
) -> Option<&'a nacre_protocol::core_services::EntityId> {
    let mut found = state.devices.order.iter().filter(|id| {
        state
            .devices
            .items
            .get(*id)
            .is_some_and(|d| d.kind == kind && (!connected || d.connected))
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}
/// Row keys: `ap:<id>` / `conn:<id>` connect, `dev:<device>` disconnects.
fn resolve_link(state: &NetworkState, key: &str) -> Option<LinkTarget> {
    use nacre_protocol::core_services::{DeviceTarget, EntityId, NetworkConnect, NetworkTarget};
    let (kind, id) = key.split_once(':')?;
    let id = EntityId(id.into());
    let connect = |device: &EntityId, target| LinkTarget {
        method: "network.connect",
        params: serde_json::to_value(NetworkConnect {
            device: device.clone(),
            target,
            secret: None,
            security: None,
        })
        .unwrap_or_default(),
    };
    match kind {
        "ap" => {
            let ap = state.access_points.items.get(&id).filter(|ap| !ap.active)?;
            state.devices.items.get(&ap.device)?;
            let saved = ap
                .connection
                .as_ref()
                .filter(|c| state.connections.items.get(*c).is_some_and(|c| !c.active));
            Some(connect(
                &ap.device,
                saved.map_or(NetworkTarget::AccessPoint(id.clone()), |c| {
                    NetworkTarget::Connection(c.clone())
                }),
            ))
        }
        "conn" => {
            let connection = state.connections.items.get(&id).filter(|c| !c.active)?;
            let device = sole_device(state, &connection.kind, false)?;
            Some(connect(device, NetworkTarget::Connection(id)))
        }
        "forget" => {
            // Saved Wi-Fi profiles only (they carry an SSID).
            state
                .connections
                .items
                .get(&id)
                .filter(|c| c.ssid.is_some())?;
            Some(LinkTarget {
                method: "network.forget",
                params: serde_json::to_value(nacre_protocol::core_services::ConnectionTarget {
                    connection: id,
                })
                .unwrap_or_default(),
            })
        }
        "dev" => {
            state.devices.items.get(&id).filter(|d| d.connected)?;
            Some(LinkTarget {
                method: "network.disconnect",
                params: serde_json::to_value(DeviceTarget { device: id }).unwrap_or_default(),
            })
        }
        _ => None,
    }
}
/// Bounded rows: access points (active first, then strongest), then saved
/// connections that no listed access point already represents.
fn link_rows(state: &NetworkState) -> Vec<LinkRow> {
    let mut rows: Vec<LinkRow> = Vec::new();
    let mut aps: Vec<_> = state
        .access_points
        .order
        .iter()
        .filter_map(|id| state.access_points.items.get(id).map(|ap| (id, ap)))
        .filter(|(_, ap)| state.devices.items.contains_key(&ap.device))
        .collect();
    aps.sort_by_key(|(_, ap)| (!ap.active, std::cmp::Reverse(ap.strength)));
    let name = |text: &str| {
        line(
            &if text.is_empty() {
                tr("bead.panel.network-hidden", &[])
            } else {
                text.to_owned()
            },
            28,
        )
    };
    for (id, ap) in aps {
        let key = if ap.active {
            format!("dev:{}", ap.device.0)
        } else {
            format!("ap:{}", id.0)
        };
        if rows.iter().any(|row| row.key == key) {
            continue;
        }
        rows.push(LinkRow {
            key,
            label: name(&ap.name),
            action: if ap.active {
                "bead.panel.network-disconnect"
            } else {
                "bead.panel.network-connect"
            },
            active: ap.active,
            secured: ap.secured,
        });
    }
    let represented: std::collections::BTreeSet<_> = state
        .access_points
        .items
        .values()
        .filter_map(|ap| ap.connection.as_ref())
        .collect();
    for id in &state.connections.order {
        let Some(connection) = state.connections.items.get(id) else {
            continue;
        };
        if represented.contains(id) {
            continue;
        }
        let key = if connection.active {
            match sole_device(state, &connection.kind, true) {
                Some(device) => format!("dev:{}", device.0),
                None => continue,
            }
        } else if sole_device(state, &connection.kind, false).is_some() {
            format!("conn:{}", id.0)
        } else {
            continue;
        };
        if rows.iter().any(|row| row.key == key) {
            continue;
        }
        rows.push(LinkRow {
            key,
            label: name(&connection.name),
            action: if connection.active {
                "bead.panel.network-disconnect"
            } else {
                "bead.panel.network-connect"
            },
            active: connection.active,
            secured: connection.security.as_deref().is_some_and(|s| s != "open"),
        });
    }
    rows.truncate(MAX_LINK_ROWS);
    rows
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
        .map_or_else(|| tr("bead.common.connected", &[]), str::to_owned);
    let primary_text = matching_ap
        .map(|ap| ap.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if connection.is_some() {
                connection_name
            } else {
                tr("bead.common.not-connected", &[])
            }
        });
    let detail = matching_ap
        .map(|ap| {
            tr(
                "bead.panel.signal-strength",
                &[("percent", &ap.strength.to_string())],
            )
        })
        .unwrap_or_else(|| tr("bead.panel.connection-controls-unavailable", &[]));
    BTreeMap::from([
        (
            "status.primary".into(),
            Value::Text(line(&primary_text, 32)),
        ),
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
    display.insert(
        "status.heading".into(),
        Value::Text(tr("bead.panel.network-title", &[])),
    );
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
        tr("bead.panel.bluetooth-off", &[])
    } else if let Some(device) = connected.first() {
        if device.name.is_empty() {
            tr("bead.panel.bluetooth-device-connected", &[])
        } else {
            device.name.clone()
        }
    } else {
        tr("bead.panel.bluetooth-none", &[])
    };
    let detail = if connected.is_empty() {
        tr("bead.panel.connection-controls-unavailable", &[])
    } else {
        tr_n(
            "bead.panel.bluetooth-devices-detail",
            connected.len() as u64,
            &[],
        )
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
    display.insert(
        "status.heading".into(),
        Value::Text(tr("bead.panel.bluetooth", &[])),
    );
    display.insert("status.icon".into(), Value::Text("bluetooth".into()));
    display.insert("status.kind".into(), Value::Text("bluetooth".into()));
    display.insert("status.secondary".into(), Value::Text(secondary.into()));
    display
}
/// The QML battery estimate: time to empty on battery, time to full on mains, else the plain state.
pub(crate) fn battery_estimate(state: &BatteryState) -> String {
    let on_battery = state.on_battery == Some(true);
    let seconds = if on_battery {
        state.time_to_empty_seconds
    } else {
        state.time_to_full_seconds
    };
    let Some(seconds) = seconds.filter(|seconds| *seconds > 0) else {
        return tr(
            if on_battery {
                "bead.panel.battery-remaining-unknown"
            } else {
                "bead.panel.control-battery-unknown"
            },
            &[],
        );
    };
    let minutes = seconds.div_ceil(60).max(1);
    let (hours, rest) = ((minutes / 60).to_string(), (minutes % 60).to_string());
    if minutes >= 60 {
        let id = if on_battery {
            "bead.panel.battery-remaining-hours"
        } else {
            "bead.panel.battery-full-hours"
        };
        tr(id, &[("hours", &hours), ("minutes", &rest)])
    } else {
        let id = if on_battery {
            "bead.panel.battery-remaining-minutes"
        } else {
            "bead.panel.battery-full-minutes"
        };
        tr(id, &[("minutes", &rest)])
    }
}
fn core_battery_display(state: &BatteryState) -> BTreeMap<String, Value> {
    let primary = state
        .percentage
        .map(|percentage| format!("{percentage:.0}%"))
        .unwrap_or_else(|| tr("bead.panel.battery-level-unknown", &[]));
    BTreeMap::from([
        ("status.primary".into(), Value::Text(line(&primary, 32))),
        (
            "battery.level".into(),
            Value::Number(
                state
                    .percentage
                    .filter(|percentage| percentage.is_finite())
                    .unwrap_or(0.0),
            ),
        ),
        (
            "status.detail".into(),
            Value::Text(tr("bead.panel.battery-profiles-unavailable", &[])),
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
    display.insert(
        "status.heading".into(),
        Value::Text(tr("bead.panel.battery-title", &[])),
    );
    display.insert("status.icon".into(), Value::Text("battery_full".into()));
    display.insert("status.kind".into(), Value::Text("battery".into()));
    display.insert("status.secondary".into(), Value::Text(secondary.into()));
    display
}
fn core_audio_display(state: &AudioState) -> BTreeMap<String, Value> {
    let node = state
        .default_output
        .as_ref()
        .and_then(|id| state.nodes.items.get(id))
        .filter(|node| node.kind == AudioKind::Output);
    let Some(node) = node else {
        return BTreeMap::from([
            (
                "status.primary".into(),
                Value::Text(tr("bead.panel.audio-output-unavailable", &[])),
            ),
            (
                "status.device".into(),
                Value::Text(tr("bead.panel.audio-output-unavailable", &[])),
            ),
            (
                "status.mute".into(),
                Value::Text(tr("bead.panel.audio-mute-unknown", &[])),
            ),
            ("audio.level".into(), Value::Number(0.0)),
            ("audio.level_available".into(), Value::Bool(false)),
            ("status.active".into(), Value::Bool(false)),
        ]);
    };
    let (volume, muted) = if let Some(readback) = &node.readback {
        (
            readback
                .perceptual_volume
                .filter(|v| v.is_finite() && *v >= 0.0)
                .and_then(audio_percent),
            readback.muted.map(muted_text),
        )
    } else if node.ready {
        (
            audio_percent(node.volume.cbrt()),
            Some(muted_text(node.muted)),
        )
    } else {
        (None, None)
    };
    let device = if node.name.trim().is_empty() {
        tr("bead.panel.audio-default-output", &[])
    } else {
        node.name.clone()
    };
    let active = muted.as_deref() == Some(muted_text(true).as_str());
    let level = node
        .readback
        .as_ref()
        .and_then(|readback| readback.perceptual_volume)
        .filter(|value| value.is_finite())
        .or_else(|| node.ready.then_some(node.volume.cbrt()))
        .map(|value| value.clamp(0.0, 1.0) * 100.0);
    BTreeMap::from([
        (
            "status.primary".into(),
            Value::Text(line(
                &volume.unwrap_or_else(|| tr("bead.panel.audio-volume-unknown", &[])),
                24,
            )),
        ),
        ("status.device".into(), Value::Text(line(&device, 40))),
        ("audio.level".into(), Value::Number(level.unwrap_or(0.0))),
        ("audio.level_available".into(), Value::Bool(level.is_some())),
        (
            "status.mute".into(),
            Value::Text(muted.unwrap_or_else(|| tr("bead.panel.audio-mute-unknown", &[]))),
        ),
        ("status.active".into(), Value::Bool(active)),
    ])
}
fn core_audio_values(
    mut display: BTreeMap<String, Value>,
    ready: bool,
    health: &str,
) -> BTreeMap<String, Value> {
    display.insert("status.ready".into(), Value::Bool(ready));
    display.insert("status.health".into(), Value::Text(health.into()));
    display.insert(
        "status.heading".into(),
        Value::Text(tr("bead.panel.sound-title", &[])),
    );
    display.insert("status.icon".into(), Value::Text("volume_up".into()));
    display.insert("status.kind".into(), Value::Text("audio".into()));
    display
}
fn muted_text(muted: bool) -> String {
    tr(
        if muted {
            "bead.panel.audio-muted"
        } else {
            "bead.panel.audio-not-muted"
        },
        &[],
    )
}
fn audio_percent(perceptual: f64) -> Option<String> {
    let percent = perceptual * 100.0;
    percent.is_finite().then(|| format!("{percent:.0}%"))
}
fn line(text: &str, max: usize) -> String {
    let mut chars = text.chars().map(|c| if c.is_control() { ' ' } else { c });
    let mut out: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
pub(crate) fn link_rows_for_test(state: &NetworkState) -> Vec<String> {
    link_rows(state).into_iter().map(|row| row.key).collect()
}
#[cfg(test)]
pub(crate) fn resolve_link_for_test(
    state: &NetworkState,
    key: &str,
) -> Option<(&'static str, serde_json::Value)> {
    resolve_link(state, key).map(|target| (target.method, target.params))
}

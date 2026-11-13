// SPDX-License-Identifier: MIT
//! Small reusable host pieces for applications authored as folders of Luau files.
use crate::{
    render::{Damage, Palette, PixelFormat, Rect, RendererKind, Scene, ShmTarget},
    ui::Session,
    wayland::{SceneSource, UiEvent},
};
use bead_backends::{Action as CompositorAction, Backend as CompositorBackend, Id as CompositorId};
use bead_core::Value;
use bead_services::{
    Action as DesktopAction, ClockSecondsLease, Config as DesktopConfig, Desktop, Snapshot,
    Subscription,
    bluetooth::{Bluetooth as BluetoothDevices, BluetoothAction, BluetoothSnapshot},
    media::{Media, MediaAction, MediaSnapshot, monotonic_millis},
    network::{Network as NetworkDevices, NetworkAction, NetworkSnapshot},
    power_profiles::{PowerProfiles, PowerProfilesAction, PowerProfilesSnapshot},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Every desktop service a folder app may declare; hosts validate manifests against it.
pub const DESKTOP_SERVICES: [&str; 12] = [
    "clock",
    "audio",
    "battery",
    "network",
    "bluetooth",
    "tray",
    "session",
    "media",
    "power",
    "bluetooth_devices",
    "network_devices",
    "workspaces",
];
#[derive(Clone, Debug, Default)]
pub struct FolderServices {
    pub desktop: BTreeSet<String>,
    pub commands: BTreeMap<String, CommandSpec>,
}
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    pub interval: Option<Duration>,
    pub timeout: Duration,
}
#[derive(Default)]
struct CommandState {
    results: BTreeMap<String, (String, bool, String)>,
    active: BTreeSet<String>,
}
type WakeCallback = Arc<dyn Fn(UiEvent) + Send + Sync>;

fn desktop_config(services: &BTreeSet<String>) -> DesktopConfig {
    let mut config = DesktopConfig::default();
    config.audio = services.contains("audio");
    config.bus = services.iter().any(|service| {
        matches!(
            service.as_str(),
            "battery" | "network" | "bluetooth" | "tray" | "session"
        )
    });
    config.tray_watcher = services.contains("tray");
    config
}

#[derive(Default)]
struct RuntimeSignals {
    callbacks: Mutex<BTreeMap<u64, WakeCallback>>,
    next: AtomicU64,
}
impl RuntimeSignals {
    fn register(&self, callback: WakeCallback) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.callbacks.lock().unwrap().insert(id, callback.clone());
        callback(UiEvent::Model(BTreeMap::new()));
        id
    }
    fn unregister(&self, id: u64) {
        self.callbacks.lock().unwrap().remove(&id);
    }
    fn notify_one(&self, id: u64) {
        let callback = self.callbacks.lock().unwrap().get(&id).cloned();
        if let Some(callback) = callback {
            callback(UiEvent::Model(BTreeMap::new()));
        }
    }
    fn notify(&self) {
        let callbacks = self
            .callbacks
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for callback in callbacks {
            callback(UiEvent::Model(BTreeMap::new()));
        }
    }
}

/// Wake every view when a service publishes, without keeping the runtime alive.
fn notifier<T: 'static>(signals: &Arc<RuntimeSignals>) -> impl Fn(T) + Send + Sync + 'static {
    let signals = Arc::downgrade(signals);
    move |_| {
        if let Some(signals) = signals.upgrade() {
            signals.notify();
        }
    }
}

/// Process-wide owner for the services shared by all views of a folder app.
pub struct FolderServicesRuntime {
    services: FolderServices,
    desktop: Option<Arc<Desktop>>,
    _subscription: Option<Subscription>,
    _clock_lease: Option<ClockSecondsLease>,
    media: Option<Arc<Media>>,
    _media_subscription: Option<bead_services::media::Subscription>,
    power: Option<Arc<PowerProfiles>>,
    _power_subscription: Option<bead_services::power_profiles::Subscription>,
    bluetooth_devices: Option<Arc<BluetoothDevices>>,
    _bluetooth_subscription: Option<bead_services::bluetooth::Subscription>,
    network_devices: Option<Arc<NetworkDevices>>,
    _network_subscription: Option<bead_services::network::Subscription>,
    compositor: Option<Arc<dyn CompositorBackend>>,
    _compositor_subscription: Option<bead_backends::Subscription>,
    commands: Arc<Mutex<CommandState>>,
    signals: Arc<RuntimeSignals>,
    stop: Arc<(Mutex<bool>, Condvar)>,
    scheduler: Mutex<Option<std::thread::JoinHandle<()>>>,
    media_tick: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl FolderServicesRuntime {
    /// Start one shared runtime, or return `None` when nothing was declared.
    pub fn start(services: FolderServices) -> Option<Arc<Self>> {
        if services.desktop.is_empty() && services.commands.is_empty() {
            return None;
        }
        let desktop_enabled = services.desktop.iter().any(|service| {
            matches!(
                service.as_str(),
                "audio" | "battery" | "network" | "bluetooth" | "tray" | "session" | "clock"
            )
        });
        let desktop = if !desktop_enabled {
            None
        } else {
            Some(Desktop::start_config(desktop_config(&services.desktop)))
        };
        let media = services
            .desktop
            .contains("media")
            .then(Media::start_session)
            .flatten();
        let power = services
            .desktop
            .contains("power")
            .then(PowerProfiles::start_system)
            .flatten();
        let bluetooth_devices = services
            .desktop
            .contains("bluetooth_devices")
            .then(BluetoothDevices::start_system)
            .flatten();
        let network_devices = services
            .desktop
            .contains("network_devices")
            .then(NetworkDevices::start_system)
            .flatten();
        let compositor = services
            .desktop
            .contains("workspaces")
            .then(bead_backends::detect)
            .flatten();
        let clock_lease = if services.desktop.contains("clock") {
            desktop
                .as_ref()
                .map(|desktop| desktop.clock_seconds_lease())
        } else {
            None
        };
        let signals = Arc::new(RuntimeSignals::default());
        let media_subscription = media
            .as_ref()
            .map(|media| media.subscribe(notifier(&signals)));
        let power_subscription = power
            .as_ref()
            .map(|power| power.subscribe(notifier(&signals)));
        let bluetooth_subscription = bluetooth_devices
            .as_ref()
            .map(|service| service.subscribe(notifier(&signals)));
        let network_subscription = network_devices
            .as_ref()
            .map(|service| service.subscribe(notifier(&signals)));
        let compositor_subscription = compositor
            .as_ref()
            .map(|backend| backend.subscribe(Box::new(notifier(&signals))));
        let subscription = desktop
            .as_ref()
            .map(|desktop| desktop.subscribe(notifier(&signals)));
        let commands = Arc::new(Mutex::new(CommandState::default()));
        let wake: WakeCallback = {
            let signals = signals.clone();
            Arc::new(move |_| signals.notify())
        };
        let intervals = services
            .commands
            .iter()
            .filter_map(|(name, spec)| spec.interval.map(|interval| (name.clone(), interval)))
            .collect::<Vec<_>>();
        for (name, spec) in &services.commands {
            if spec.interval.is_some() {
                launch_command(name, &services.commands, &commands, &wake);
            }
        }
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let media_tick = media.as_ref().map(|media| {
            let media = media.clone();
            let signals = signals.clone();
            let stop_signal = stop.clone();
            std::thread::spawn(move || {
                let mut last_tick = monotonic_millis();
                loop {
                    let stopped = stop_signal.0.lock().unwrap();
                    if *stopped {
                        break;
                    }
                    let (stopped, timeout) = stop_signal
                        .1
                        .wait_timeout(stopped, Duration::from_secs(1))
                        .unwrap();
                    if *stopped {
                        break;
                    }
                    drop(stopped);
                    let now = monotonic_millis();
                    if timeout.timed_out() && media_tick_due(&media.snapshot(), last_tick, now) {
                        signals.notify();
                        last_tick = now;
                    }
                }
            })
        });
        let scheduler = if intervals.is_empty() {
            None
        } else {
            let specs = services.commands.clone();
            let commands = commands.clone();
            let wake = wake.clone();
            let stop_signal = stop.clone();
            Some(std::thread::spawn(move || {
                let mut next = intervals
                    .into_iter()
                    .map(|(name, interval)| (name, interval, std::time::Instant::now()))
                    .collect::<Vec<_>>();
                loop {
                    let wait = next
                        .iter()
                        .map(|(_, interval, started)| interval.saturating_sub(started.elapsed()))
                        .min()
                        .unwrap_or(Duration::from_secs(60));
                    let stopped = stop_signal.0.lock().unwrap();
                    if *stopped {
                        break;
                    }
                    let (stopped, _) = stop_signal.1.wait_timeout(stopped, wait).unwrap();
                    if *stopped {
                        break;
                    }
                    drop(stopped);
                    for (name, interval, started) in &mut next {
                        if started.elapsed() >= *interval {
                            launch_command(name, &specs, &commands, &wake);
                            *started = std::time::Instant::now();
                        }
                    }
                }
            }))
        };
        Some(Arc::new(Self {
            services,
            desktop,
            _subscription: subscription,
            _clock_lease: clock_lease,
            media,
            _media_subscription: media_subscription,
            power,
            _power_subscription: power_subscription,
            bluetooth_devices,
            _bluetooth_subscription: bluetooth_subscription,
            network_devices,
            _network_subscription: network_subscription,
            compositor,
            _compositor_subscription: compositor_subscription,
            commands,
            signals,
            stop,
            scheduler: Mutex::new(scheduler),
            media_tick: Mutex::new(media_tick),
        }))
    }

    fn register(&self, callback: WakeCallback) -> u64 {
        self.signals.register(callback)
    }
    fn unregister(&self, id: u64) {
        self.signals.unregister(id);
    }
    fn notify_view(&self, id: u64) {
        self.signals.notify_one(id);
    }
    fn run_command(&self, name: &str) {
        if !self.services.commands.contains_key(name) {
            eprintln!("unknown command '{name}'");
            return;
        }
        let wake: WakeCallback = {
            let signals = self.signals.clone();
            Arc::new(move |_| signals.notify())
        };
        launch_command(name, &self.services.commands, &self.commands, &wake);
    }
    fn command_values(&self) -> BTreeMap<String, (String, bool, String)> {
        self.commands.lock().unwrap().results.clone()
    }
}
impl Drop for FolderServicesRuntime {
    fn drop(&mut self) {
        let (stopped, wake) = &*self.stop;
        *stopped.lock().unwrap() = true;
        wake.notify_all();
        if let Some(thread) = self.scheduler.lock().unwrap().take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.media_tick.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

/// Project the raw desktop snapshot into stable, app-facing stores.
pub fn desktop_values(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    let mut put = |key: &str, value| {
        values.insert(format!("desktop.{key}"), value);
    };
    put("clock", Value::Text(snapshot.clock.clone()));
    put("clock.seconds", Value::Text(snapshot.clock_seconds.clone()));
    put(
        "date",
        Value::Text(
            snapshot
                .date
                .filter(|d| d.is_valid())
                .map_or_else(String::new, |d| {
                    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
                }),
        ),
    );
    put("audio.available", Value::Bool(snapshot.audio.connected));
    put(
        "audio.volume",
        Value::Number(
            snapshot
                .audio
                .volume
                .filter(|v| v.is_finite())
                .map_or(-1., |v| v.clamp(0., 1.)),
        ),
    );
    put(
        "audio.muted",
        Value::Bool(snapshot.audio.muted.unwrap_or(false)),
    );
    put(
        "audio.sink",
        Value::Text(snapshot.audio.default_sink.clone().unwrap_or_default()),
    );
    put("battery.available", Value::Bool(snapshot.battery.available));
    put(
        "battery.percent",
        Value::Number(snapshot.battery.percentage.unwrap_or(-1.)),
    );
    put("battery.charging", Value::Bool(snapshot.battery.charging));
    put(
        "battery.on_battery",
        Value::Bool(snapshot.battery.on_battery),
    );
    put("network.available", Value::Bool(snapshot.network.available));
    put("network.connected", Value::Bool(snapshot.network.connected));
    put(
        "network.wireless",
        Value::Bool(snapshot.network.wireless_enabled),
    );
    put("network.name", Value::Text(snapshot.network.name.clone()));
    put(
        "network.strength",
        Value::Number(snapshot.network.strength.map_or(-1., f64::from)),
    );
    put(
        "bluetooth.available",
        Value::Bool(snapshot.bluetooth.available),
    );
    put("bluetooth.powered", Value::Bool(snapshot.bluetooth.powered));
    put(
        "bluetooth.connected",
        Value::list(
            snapshot
                .bluetooth
                .connected_devices
                .iter()
                .map(|name| serde_json::json!({"name": name}))
                .collect(),
        ),
    );
    put("tray", Value::list(snapshot.tray.iter().map(|item| serde_json::json!({"id": item.id, "title": item.title, "icon_name": item.icon_name, "attention": item.attention, "has_menu": item.has_menu})).collect()));
    put("session.locked", Value::Bool(snapshot.session.locked));
    values
}
fn compositor_values(snapshot: Option<&bead_backends::Snapshot>) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "desktop.workspaces".into(),
            Value::list(snapshot.map_or_else(Vec::new, |snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .map(|workspace| {
                        serde_json::json!({
                            "id": workspace.id.0,
                            "index": workspace.index,
                            "name": workspace.name,
                            "output": workspace.output.as_ref().map_or_else(String::new, |id| id.0.clone()),
                            "active": workspace.active,
                            "focused": workspace.focused,
                            "urgent": workspace.urgent,
                        })
                    })
                    .collect()
            })),
        ),
        (
            "desktop.window.title".into(),
            Value::Text(
                snapshot
                    .and_then(bead_backends::Snapshot::focused_window)
                    .map_or_else(String::new, |window| window.title.clone()),
            ),
        ),
        (
            "desktop.window.app_id".into(),
            Value::Text(
                snapshot
                    .and_then(bead_backends::Snapshot::focused_window)
                    .map_or_else(String::new, |window| window.app_id.clone()),
            ),
        ),
        (
            "desktop.compositor".into(),
            Value::Text(snapshot.map_or_else(String::new, |snapshot| snapshot.backend.clone())),
        ),
    ])
}
fn media_values(snapshot: &MediaSnapshot, now: u64) -> BTreeMap<String, Value> {
    let player = snapshot
        .active
        .as_ref()
        .and_then(|id| snapshot.players.iter().find(|p| &p.id == id));
    let position = player
        .map(|p| {
            if p.status == "Playing" {
                p.position_ms
                    .saturating_add(now.saturating_sub(p.position_at) as i64)
                    .min(if p.length_ms > 0 {
                        p.length_ms
                    } else {
                        i64::MAX
                    })
            } else {
                p.position_ms
            }
        })
        .unwrap_or(-1);
    let mut values = BTreeMap::new();
    macro_rules! put {
        ($key:expr, $value:expr) => {
            values.insert(format!("desktop.media.{}", $key), $value);
        };
    }
    put!("available", Value::Bool(player.is_some()));
    put!(
        "player",
        Value::Text(player.map_or_else(String::new, |p| p.identity.clone()))
    );
    put!(
        "status",
        Value::Text(player.map_or_else(String::new, |p| p.status.clone()))
    );
    put!(
        "title",
        Value::Text(player.map_or_else(String::new, |p| p.title.clone()))
    );
    put!(
        "artist",
        Value::Text(player.map_or_else(String::new, |p| p.artist.clone()))
    );
    put!(
        "album",
        Value::Text(player.map_or_else(String::new, |p| p.album.clone()))
    );
    put!(
        "art_url",
        Value::Text(player.map_or_else(String::new, |p| p.art_url.clone()))
    );
    put!(
        "length_ms",
        Value::Number(player.map_or(-1, |p| p.length_ms) as f64)
    );
    put!("position_ms", Value::Number(position as f64));
    put!(
        "can_play_pause",
        Value::Bool(player.is_some_and(|p| p.can_play_pause))
    );
    put!("can_next", Value::Bool(player.is_some_and(|p| p.can_next)));
    put!(
        "can_previous",
        Value::Bool(player.is_some_and(|p| p.can_previous))
    );
    put!("can_seek", Value::Bool(player.is_some_and(|p| p.can_seek)));
    values
}
fn power_values(snapshot: &PowerProfilesSnapshot) -> BTreeMap<String, Value> {
    BTreeMap::from([
        (
            "desktop.power.available".into(),
            Value::Bool(snapshot.available),
        ),
        (
            "desktop.power.profile".into(),
            Value::Text(snapshot.active.clone()),
        ),
        (
            "desktop.power.profiles".into(),
            Value::list(
                snapshot
                    .profiles
                    .iter()
                    .map(|name| serde_json::json!({"name": name}))
                    .collect(),
            ),
        ),
        (
            "desktop.power.degraded".into(),
            Value::Text(snapshot.degraded.clone()),
        ),
    ])
}
fn bluetooth_device_values(snapshot: &BluetoothSnapshot) -> BTreeMap<String, Value> {
    let adapter = snapshot.adapters.first();
    BTreeMap::from([
        ("desktop.bluetooth_devices.available".into(),Value::Bool(snapshot.available)),
        ("desktop.bluetooth_devices.powered".into(),Value::Bool(adapter.is_some_and(|a|a.powered))),
        ("desktop.bluetooth_devices.discovering".into(),Value::Bool(adapter.is_some_and(|a|a.discovering))),
        ("desktop.bluetooth_devices.devices".into(),Value::list(snapshot.devices.iter().map(|d|serde_json::json!({"path":d.path,"name":d.name,"address":d.address,"icon":d.icon,"paired":d.paired,"connected":d.connected,"battery":d.battery})).collect())),
    ])
}
fn network_device_values(snapshot: &NetworkSnapshot) -> BTreeMap<String, Value> {
    let primary = snapshot.primary.as_ref();
    let access_points = snapshot.access_points.iter().map(|a| {
        serde_json::json!({"ssid": a.ssid, "strength": a.strength, "security": a.security,
            "active": a.active, "known": a.known})
    });
    let known = snapshot
        .known_connections
        .iter()
        .map(|k| serde_json::json!({"id": k.id, "kind": k.kind}));
    BTreeMap::from([
        (
            "desktop.network_devices.available".into(),
            Value::Bool(snapshot.available),
        ),
        (
            "desktop.network_devices.wireless_enabled".into(),
            Value::Bool(snapshot.wireless_enabled),
        ),
        (
            "desktop.network_devices.primary_name".into(),
            Value::Text(primary.map_or_else(String::new, |p| p.name.clone())),
        ),
        (
            "desktop.network_devices.primary_kind".into(),
            Value::Text(primary.map_or("", |p| p.kind).into()),
        ),
        (
            "desktop.network_devices.access_points".into(),
            Value::list(access_points.collect()),
        ),
        (
            "desktop.network_devices.known".into(),
            Value::list(known.collect()),
        ),
    ])
}
/// Resolve a folder request to a service action on the first Wi-Fi device that applies.
fn network_action(
    snapshot: &NetworkSnapshot,
    request: NetworkRequest,
) -> Result<NetworkAction, String> {
    let wifi = |connected: bool| {
        snapshot
            .devices
            .iter()
            .find(|d| d.kind == "wifi" && (!connected || !d.active_connection.is_empty()))
            .map(|d| d.path.clone())
    };
    Ok(match request {
        NetworkRequest::Wireless(on) => NetworkAction::SetWirelessEnabled(on),
        NetworkRequest::Scan => {
            NetworkAction::Scan(wifi(false).ok_or("no Wi-Fi device is available")?)
        }
        NetworkRequest::Disconnect => {
            NetworkAction::Disconnect(wifi(true).ok_or("Wi-Fi is not connected")?)
        }
        NetworkRequest::Connect(ssid) => {
            let ap = snapshot
                .access_points
                .iter()
                .find(|ap| ap.ssid == ssid)
                .ok_or_else(|| format!("unknown Wi-Fi network '{ssid}'"))?;
            let known = snapshot
                .known_connections
                .iter()
                .find(|k| k.kind == "wifi" && k.ssid == ap.ssid_bytes);
            match known {
                Some(known) => NetworkAction::ActivateKnown(known.path.clone(), ap.device.clone()),
                // The service rejects secured networks: they need a password agent.
                None => NetworkAction::ActivateOpen(ap.ssid_bytes.clone(), ap.device.clone()),
            }
        }
        NetworkRequest::Forget(id) => {
            let mut matches = snapshot.known_connections.iter().filter(|k| k.id == id);
            match (matches.next(), matches.next()) {
                (Some(known), None) => NetworkAction::Forget(known.path.clone()),
                (None, _) => return Err(format!("unknown saved connection '{id}'")),
                _ => return Err(format!("saved connection id '{id}' is ambiguous")),
            }
        }
    })
}
fn default_bluetooth_adapter(snapshot: &BluetoothSnapshot) -> Option<String> {
    snapshot.adapters.first().map(|a| a.path.clone())
}
fn media_is_playing(snapshot: &MediaSnapshot) -> bool {
    snapshot
        .active
        .as_ref()
        .and_then(|id| snapshot.players.iter().find(|p| &p.id == id))
        .is_some_and(|p| p.status == "Playing")
}
fn media_tick_due(snapshot: &MediaSnapshot, last_tick: u64, now: u64) -> bool {
    media_is_playing(snapshot) && now.saturating_sub(last_tick) >= 1_000
}
fn media_position_due(snapshot: &MediaSnapshot, last_tick: Option<u64>, now: u64) -> bool {
    !media_is_playing(snapshot) || last_tick.is_none_or(|last| now.saturating_sub(last) >= 1_000)
}
fn declared_desktop_values(
    snapshot: &Snapshot,
    declared: &BTreeSet<String>,
) -> BTreeMap<String, Value> {
    let mut values = desktop_values(snapshot)
        .into_iter()
        .filter(|(key, _)| {
            key.strip_prefix("desktop.")
                .map(|rest| {
                    if rest == "date" {
                        "clock"
                    } else {
                        rest.split('.').next().unwrap_or_default()
                    }
                })
                .is_some_and(|service| declared.contains(service))
        })
        .collect::<BTreeMap<_, _>>();
    if declared.contains("workspaces") {
        values.extend(compositor_values(None));
    }
    values
}

#[derive(Clone, Debug)]
enum DesktopRequest {
    Action(DesktopAction),
    Media(MediaRequest),
    Power(PowerRequest),
    Bluetooth(BluetoothRequest),
    Network(NetworkRequest),
    Compositor(CompositorAction),
    Command(String),
}
fn send_compositor_action(
    backend: Option<&dyn CompositorBackend>,
    action: CompositorAction,
) -> Result<(), String> {
    let Some(backend) = backend else {
        return Err("no compositor detected (niri or Hyprland)".into());
    };
    backend.action(action).map_err(|error| error.to_string())
}
#[derive(Clone, Debug)]
enum PowerRequest {
    SetProfile(String),
    Cycle,
}
#[derive(Clone, Debug)]
enum MediaRequest {
    PlayPause,
    Next,
    Previous,
    Seek(i64),
}
#[derive(Clone, Debug)]
enum BluetoothRequest {
    Power(bool),
    Scan(bool),
    Connect(String),
    Disconnect(String),
    Forget(String),
}
#[derive(Clone, Debug)]
enum NetworkRequest {
    Wireless(bool),
    Scan,
    Connect(String),
    Disconnect,
    Forget(String),
}
fn route_folder_action(
    name: &str,
    argument: &Value,
    declared: &BTreeSet<String>,
    pointer: Option<(f32, f32)>,
) -> Result<Option<DesktopRequest>, String> {
    if let Some(command) = name.strip_prefix("command.") {
        return if !command.is_empty() {
            Ok(Some(DesktopRequest::Command(command.into())))
        } else {
            Err("command action needs a name".into())
        };
    }
    if !name.starts_with("desktop.") {
        return Ok(None);
    }
    let need = match name {
        "desktop.volume" | "desktop.mute" => "audio",
        "desktop.tray.activate" | "desktop.tray.secondary" | "desktop.tray.menu" => "tray",
        "desktop.media.play_pause"
        | "desktop.media.next"
        | "desktop.media.previous"
        | "desktop.media.seek" => "media",
        "desktop.power.set" | "desktop.power.cycle" => "power",
        "desktop.bluetooth_devices.power"
        | "desktop.bluetooth_devices.scan"
        | "desktop.bluetooth_devices.connect"
        | "desktop.bluetooth_devices.disconnect"
        | "desktop.bluetooth_devices.forget" => "bluetooth_devices",
        "desktop.network_devices.wireless"
        | "desktop.network_devices.scan"
        | "desktop.network_devices.connect"
        | "desktop.network_devices.disconnect"
        | "desktop.network_devices.forget" => "network_devices",
        "desktop.workspace.focus" | "desktop.window.focus" => "workspaces",
        _ => return Err(format!("unknown desktop action '{name}'")),
    };
    if !declared.contains(need) {
        return Err(format!(
            "desktop action '{name}' requires undeclared service '{need}'"
        ));
    }
    if name == "desktop.workspace.focus" {
        let action = match argument {
            Value::Number(index)
                if index.is_finite()
                    && index.fract() == 0.
                    && *index >= i64::MIN as f64
                    && *index < i64::MAX as f64 =>
            {
                CompositorAction::SwitchWorkspaceIndex {
                    index: *index as i64,
                }
            }
            Value::Text(id) if !id.is_empty() => CompositorAction::SwitchWorkspace {
                workspace: CompositorId(id.clone()),
            },
            Value::Number(_) => {
                return Err(
                    "desktop.workspace.focus expects an integer index or workspace id".into(),
                );
            }
            _ => return Err("desktop.workspace.focus expects a number index or text id".into()),
        };
        return Ok(Some(DesktopRequest::Compositor(action)));
    }
    if name == "desktop.window.focus" {
        let Value::Text(id) = argument else {
            return Err("desktop.window.focus expects a window id".into());
        };
        if id.is_empty() {
            return Err("desktop.window.focus expects a non-empty window id".into());
        }
        return Ok(Some(DesktopRequest::Compositor(
            CompositorAction::FocusWindow {
                window: CompositorId(id.clone()),
            },
        )));
    }
    if name.starts_with("desktop.media.") {
        let action = match name {
            "desktop.media.play_pause" => MediaRequest::PlayPause,
            "desktop.media.next" => MediaRequest::Next,
            "desktop.media.previous" => MediaRequest::Previous,
            "desktop.media.seek" => match argument {
                Value::Number(v) if v.is_finite() && *v >= 0. => MediaRequest::Seek(*v as i64),
                _ => return Err("desktop.media.seek expects a non-negative number".into()),
            },
            _ => return Err(format!("{name} expects no argument")),
        };
        return Ok(Some(DesktopRequest::Media(action)));
    }
    if name.starts_with("desktop.bluetooth_devices.") {
        let action = match (name, argument) {
            ("desktop.bluetooth_devices.power", Value::Bool(v)) => BluetoothRequest::Power(*v),
            ("desktop.bluetooth_devices.scan", Value::Bool(v)) => BluetoothRequest::Scan(*v),
            ("desktop.bluetooth_devices.connect", Value::Text(v)) => {
                BluetoothRequest::Connect(v.clone())
            }
            ("desktop.bluetooth_devices.disconnect", Value::Text(v)) => {
                BluetoothRequest::Disconnect(v.clone())
            }
            ("desktop.bluetooth_devices.forget", Value::Text(v)) => {
                BluetoothRequest::Forget(v.clone())
            }
            ("desktop.bluetooth_devices.power", _) => {
                return Err("desktop.bluetooth_devices.power expects a boolean".into());
            }
            ("desktop.bluetooth_devices.scan", _) => {
                return Err("desktop.bluetooth_devices.scan expects a boolean".into());
            }
            _ => return Err(format!("{name} expects a device path")),
        };
        return Ok(Some(DesktopRequest::Bluetooth(action)));
    }
    if name.starts_with("desktop.network_devices.") {
        let request = match (name, argument) {
            ("desktop.network_devices.wireless", Value::Bool(v)) => NetworkRequest::Wireless(*v),
            ("desktop.network_devices.wireless", _) => {
                return Err(format!("{name} expects a boolean"));
            }
            ("desktop.network_devices.scan", _) => NetworkRequest::Scan,
            ("desktop.network_devices.disconnect", _) => NetworkRequest::Disconnect,
            ("desktop.network_devices.connect", Value::Text(v)) => {
                NetworkRequest::Connect(v.clone())
            }
            ("desktop.network_devices.forget", Value::Text(v)) => NetworkRequest::Forget(v.clone()),
            _ => return Err(format!("{name} expects text")),
        };
        return Ok(Some(DesktopRequest::Network(request)));
    }
    if name == "desktop.power.set" {
        let Value::Text(profile) = argument else {
            return Err("desktop.power.set expects a profile name".into());
        };
        return Ok(Some(DesktopRequest::Power(PowerRequest::SetProfile(
            profile.clone(),
        ))));
    }
    if name == "desktop.power.cycle" {
        return Ok(Some(DesktopRequest::Power(PowerRequest::Cycle)));
    }
    let point = pointer
        .map(|(x, y)| (x.round() as i32, y.round() as i32))
        .unwrap_or((0, 0));
    let action = match name {
        "desktop.volume" => match argument {
            Value::Number(v) if v.is_finite() => DesktopAction::Volume(v.clamp(0., 1.)),
            _ => return Err("desktop.volume expects a number".into()),
        },
        "desktop.mute" => match argument {
            Value::Bool(v) => DesktopAction::Mute(*v),
            _ => return Err("desktop.mute expects a boolean".into()),
        },
        "desktop.tray.activate" | "desktop.tray.secondary" | "desktop.tray.menu" => {
            let Value::Text(item) = argument else {
                return Err(format!("{name} expects a tray item id"));
            };
            let (item, x, y) = (item.clone(), point.0, point.1);
            match name {
                "desktop.tray.activate" => DesktopAction::TrayActivate { item, x, y },
                "desktop.tray.secondary" => DesktopAction::TraySecondaryActivate { item, x, y },
                _ => DesktopAction::TrayContextMenu { item, x, y },
            }
        }
        _ => unreachable!(),
    };
    Ok(Some(DesktopRequest::Action(action)))
}
fn toggled_mute(snapshot: &Snapshot) -> Value {
    Value::Bool(!snapshot.audio.muted.unwrap_or(false))
}
fn launch_command(
    name: &str,
    specs: &BTreeMap<String, CommandSpec>,
    state: &Arc<Mutex<CommandState>>,
    wake: &WakeCallback,
) {
    let Some(spec) = specs.get(name).cloned() else {
        return;
    };
    if let Ok(mut state) = state.lock()
        && !state.active.insert(name.to_owned())
    {
        return;
    }
    let name = name.to_owned();
    let state = state.clone();
    let wake = wake.clone();
    std::thread::spawn(move || {
        let (stdout, ok, error) = execute_command(&spec);
        if let Ok(mut shared) = state.lock() {
            shared.active.remove(&name);
            shared.results.insert(name, (stdout, ok, error));
        }
        wake(UiEvent::Model(BTreeMap::new()));
    });
}
fn execute_command(spec: &CommandSpec) -> (String, bool, String) {
    let mut command = bead_services::process::command(&spec.argv[0]);
    command.args(&spec.argv[1..]);
    match bead_services::process::run(
        &mut command,
        &[],
        bead_services::process::Limits {
            timeout: spec.timeout,
            output: 64 * 1024,
            ..Default::default()
        },
    ) {
        Ok(output) => {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            let capped = if trimmed.len() > 4096 {
                let end = trimmed
                    .char_indices()
                    .take_while(|(i, c)| *i + c.len_utf8() <= 4096)
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
                &trimmed[..end]
            } else {
                trimmed
            };
            (
                capped.to_owned(),
                output.code == 0,
                if output.code == 0 {
                    String::new()
                } else {
                    format!("command exited {}", output.code)
                },
            )
        }
        Err(e) => (
            String::new(),
            false,
            e.replace("process deadline exceeded", "command timeout exceeded"),
        ),
    }
}

/// A Luau-backed scene source rooted in an application's folder.
pub struct FolderView {
    pub session: Session,
    pending: Option<Scene>,
    size: (u32, u32),
    runtime: Option<Arc<FolderServicesRuntime>>,
    registration: Option<u64>,
    initialized: bool,
    needs_external_refresh: bool,
    last_revision: Option<u64>,
    last_media_position_tick: Option<u64>,
    pointer: Option<(f32, f32)>,
    /// B0250: the host's event sender, kept for `panel.*` actions.
    wakeup: Option<WakeCallback>,
    /// B0250: set on a bar that may open popup panels; its pointer position along this
    /// edge becomes the panel centre. Panel views open other panels without a centre.
    panel_edge: Option<bead_core::Edge>,
    fixed_viewport: bool,
    /// `@icon:NAME` slots resolved through the icon directories, memoised per name (spec 0264).
    icons: HashMap<String, Option<PathBuf>>,
}
/// Find NAME.svg (preferred) or NAME.png under `icons/hicolor` and `pixmaps` of XDG_DATA_HOME and
/// XDG_DATA_DIRS. shortcut: no index.theme inheritance or size matching (the first PNG wins);
/// upgrade when a folder app needs a themed icon set.
fn icon_file(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let mut png = None;
    for data in home.into_iter().chain(dirs.split(':').map(PathBuf::from)) {
        let mut stack = vec![data.join("icons/hicolor"), data.join("pixmaps")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                // Symlinked directories are not followed, so a looping theme cannot hang the walk.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    stack.push(path);
                } else if path.file_stem().is_some_and(|stem| stem == name) {
                    match path.extension().and_then(|e| e.to_str()) {
                        Some("svg") => return Some(path),
                        Some("png") if png.is_none() => png = Some(path),
                        _ => {}
                    }
                }
            }
        }
    }
    png
}
/// Host actions a folder app with `[panels]` may send (spec 0250).
pub const PANEL_ACTIONS: [&str; 3] = ["panel.open", "panel.toggle", "panel.close"];
/// The host event for a `panel.*` action: a `CompanionSelect` along the bar axis or a close.
fn panel_event(
    name: &str,
    argument: &Value,
    pointer: Option<(f32, f32)>,
    bar_edge: Option<bead_core::Edge>,
) -> Result<UiEvent, String> {
    if name == "panel.close" {
        return Ok(UiEvent::CompanionVisibility(false));
    }
    let Value::Text(view) = argument else {
        return Err(format!("{name} needs a panel name"));
    };
    let center = pointer.zip(bar_edge).map(|((x, y), edge)| match edge {
        bead_core::Edge::Top | bead_core::Edge::Bottom => x,
        bead_core::Edge::Left | bead_core::Edge::Right => y,
    });
    Ok(UiEvent::CompanionSelect {
        view: view.clone(),
        output: None,
        toggle: name == "panel.toggle",
        center,
    })
}
impl FolderView {
    pub fn load(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_inner(
            root,
            view,
            palette,
            title,
            size,
            None,
            FolderServices::default(),
        )
    }
    pub fn load_with_services(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
        services: FolderServices,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_inner(root, view, palette, title, size, None, services)
    }
    pub fn load_with_runtime(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
        runtime: Arc<FolderServicesRuntime>,
    ) -> Result<Self, Box<dyn Error>> {
        let services = runtime.services.clone();
        Self::load_inner(root, view, palette, title, size, Some(runtime), services)
    }
    fn load_inner(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
        runtime: Option<Arc<FolderServicesRuntime>>,
        services: FolderServices,
    ) -> Result<Self, Box<dyn Error>> {
        crate::ui::plugins::configure(
            &root,
            crate::ui::plugins::Config {
                system: vec![root.join("plugins")],
                user: None,
                state: None,
            },
        );
        let palette = if palette.is_absolute() {
            palette
        } else {
            root.join(palette)
        };
        let colors = Palette::read(&palette)?;
        let fallback = Scene::empty(size.0 as f32, size.1 as f32);
        let mut session = Session::new(root, view, palette, colors, fallback);
        let mut host_stores = declared_desktop_values(&Snapshot::default(), &services.desktop);
        if services.desktop.contains("media") {
            host_stores.extend(media_values(&MediaSnapshot::default(), 0));
        }
        if services.desktop.contains("power") {
            host_stores.extend(power_values(&PowerProfilesSnapshot::default()));
        }
        if services.desktop.contains("bluetooth_devices") {
            host_stores.extend(bluetooth_device_values(&BluetoothSnapshot::default()));
        }
        if services.desktop.contains("network_devices") {
            host_stores.extend(network_device_values(&NetworkSnapshot::default()));
        }
        for name in services.commands.keys() {
            host_stores.insert(
                format!("commands.{name}.stdout"),
                Value::Text(String::new()),
            );
            host_stores.insert(format!("commands.{name}.ok"), Value::Bool(false));
            host_stores.insert(format!("commands.{name}.error"), Value::Text(String::new()));
        }
        session.declare_host_stores(host_stores)?;
        let mut host_actions = Vec::new();
        for name in ["desktop.volume", "desktop.mute", "desktop.mute.toggle"] {
            if services.desktop.contains("audio") {
                host_actions.push(name.to_owned());
            }
        }
        for name in [
            "desktop.tray.activate",
            "desktop.tray.secondary",
            "desktop.tray.menu",
        ] {
            if services.desktop.contains("tray") {
                host_actions.push(name.to_owned());
            }
        }
        if services.desktop.contains("media") {
            host_actions.extend(
                [
                    "desktop.media.play_pause",
                    "desktop.media.next",
                    "desktop.media.previous",
                    "desktop.media.seek",
                ]
                .into_iter()
                .map(str::to_owned),
            );
        }
        if services.desktop.contains("workspaces") {
            host_actions.extend(
                ["desktop.workspace.focus", "desktop.window.focus"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        if services.desktop.contains("power") {
            host_actions.extend(
                ["desktop.power.set", "desktop.power.cycle"]
                    .into_iter()
                    .map(str::to_owned),
            );
        }
        if services.desktop.contains("bluetooth_devices") {
            host_actions.extend(
                [
                    "desktop.bluetooth_devices.power",
                    "desktop.bluetooth_devices.scan",
                    "desktop.bluetooth_devices.connect",
                    "desktop.bluetooth_devices.disconnect",
                    "desktop.bluetooth_devices.forget",
                ]
                .into_iter()
                .map(str::to_owned),
            );
        }
        if services.desktop.contains("network_devices") {
            host_actions.extend(
                [
                    "desktop.network_devices.wireless",
                    "desktop.network_devices.scan",
                    "desktop.network_devices.connect",
                    "desktop.network_devices.disconnect",
                    "desktop.network_devices.forget",
                ]
                .into_iter()
                .map(str::to_owned),
            );
        }
        host_actions.extend(
            services
                .commands
                .keys()
                .map(|name| format!("command.{name}")),
        );
        session.set_host_actions(host_actions);
        session.set_accessibility_label(title);
        session.set_viewport(size.0, size.1);
        Ok(Self {
            session,
            pending: None,
            size,
            runtime,
            registration: None,
            initialized: false,
            needs_external_refresh: false,
            last_revision: None,
            last_media_position_tick: None,
            pointer: None,
            wakeup: None,
            panel_edge: None,
            fixed_viewport: false,
            icons: HashMap::new(),
        })
    }
    /// Allow `panel.open|toggle|close` (spec 0250). `bar_edge` marks the bar whose pointer
    /// position along that edge centres the panel; panel views pass `None` and keep
    /// their declared viewport when the panel controller supplies output dimensions.
    pub fn with_panel_actions(mut self, bar_edge: Option<bead_core::Edge>) -> Self {
        self.session
            .add_host_actions(PANEL_ACTIONS.iter().map(|name| (*name).to_owned()));
        self.panel_edge = bar_edge;
        self.fixed_viewport = bar_edge.is_none();
        self
    }
    fn apply_external(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        let mut changes = BTreeMap::new();
        if let Some(runtime) = &self.runtime {
            if let Some(desktop) = &runtime.desktop {
                let snapshot = desktop.snapshot();
                if self
                    .last_revision
                    .is_none_or(|revision| snapshot.revision >= revision)
                {
                    self.last_revision = Some(snapshot.revision);
                    changes.extend(declared_desktop_values(
                        &snapshot,
                        &runtime.services.desktop,
                    ));
                }
            }
            if runtime.services.desktop.contains("media") {
                let snapshot = runtime
                    .media
                    .as_ref()
                    .map_or_else(MediaSnapshot::default, |media| media.snapshot());
                let now = monotonic_millis();
                let mut values = media_values(&snapshot, now);
                if media_is_playing(&snapshot) {
                    if media_position_due(&snapshot, self.last_media_position_tick, now) {
                        self.last_media_position_tick = Some(now);
                    } else {
                        values.remove("desktop.media.position_ms");
                    }
                } else {
                    self.last_media_position_tick = None;
                }
                changes.extend(values);
            }
            if runtime.services.desktop.contains("power") {
                let snapshot = runtime
                    .power
                    .as_ref()
                    .map_or_else(PowerProfilesSnapshot::default, |power| power.snapshot());
                changes.extend(power_values(&snapshot));
            }
            if runtime.services.desktop.contains("bluetooth_devices") {
                let snapshot = runtime
                    .bluetooth_devices
                    .as_ref()
                    .map_or_else(BluetoothSnapshot::default, |service| service.snapshot());
                changes.extend(bluetooth_device_values(&snapshot));
            }
            if runtime.services.desktop.contains("network_devices") {
                let snapshot = runtime
                    .network_devices
                    .as_ref()
                    .map_or_else(NetworkSnapshot::default, |service| service.snapshot());
                changes.extend(network_device_values(&snapshot));
            }
            if runtime.services.desktop.contains("workspaces") {
                let snapshot = runtime
                    .compositor
                    .as_ref()
                    .map(|backend| backend.snapshot());
                changes.extend(compositor_values(snapshot.as_deref()));
            }
            for (name, (stdout, ok, error)) in runtime.command_values() {
                changes.insert(format!("commands.{name}.stdout"), Value::Text(stdout));
                changes.insert(format!("commands.{name}.ok"), Value::Bool(ok));
                changes.insert(format!("commands.{name}.error"), Value::Text(error));
            }
        }
        let count = changes.len();
        changes.retain(|key, _| self.session.has_store(key));
        if !self.initialized && changes.len() != count {
            self.needs_external_refresh = true;
        }
        if changes.is_empty() {
            Ok(None)
        } else {
            Ok(self
                .session
                .update_model(changes)?
                .map(|s| self.surface_scene(s)))
        }
    }
    fn surface_scene(&mut self, mut scene: Scene) -> Scene {
        // Folder views occupy their surface; Luau layout reserves a panel shadow
        // margin by default, which would clip thin bars and offset their input.
        scene.offset = (0., 0.);
        scene.bounds = Rect::new(0., 0., self.size.0 as f32, self.size.1 as f32);
        // Icon slots become theme files; an unknown icon keeps its slot path, which the renderer names.
        for primitive in &mut scene.primitives {
            if let crate::render::Primitive::Image { path, .. } = primitive
                && let Some(name) = path.to_str().and_then(|p| p.strip_prefix("@icon:"))
                && let Some(file) = self
                    .icons
                    .entry(name.to_owned())
                    .or_insert_with(|| icon_file(name))
            {
                *path = file.clone();
            }
        }
        scene
    }
    /// Stage, validate, and render one update on the CPU without a display connection.
    pub fn check_render(&mut self, event: UiEvent) -> Result<Scene, Box<dyn Error>> {
        let scene = self.stage(event)?.ok_or("expected a scene update")?;
        self.validate()?;
        let mut renderer = crate::render::renderer(RendererKind::Cpu)?;
        renderer.prepare(&scene, 1.)?;
        let mut pixels = vec![0; (self.size.0 * self.size.1 * 4) as usize];
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width: self.size.0,
                height: self.size.1,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(self.size.0, self.size.1),
        )?;
        self.commit();
        Ok(scene)
    }
}
impl SceneSource for FolderView {
    fn scroll_regions(&self) -> Vec<Rect> {
        self.session.scroll_regions()
    }
    fn wakeup_at(&self) -> Option<std::time::Instant> {
        self.session.preview_deadline()
    }
    fn frame_interval(&mut self, interval: Duration) {
        self.session.set_frame_interval(interval);
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        let wakeup: WakeCallback = Arc::from(wakeup);
        self.wakeup = Some(wakeup.clone());
        let frame = wakeup.clone();
        self.session
            .set_wakeup(Arc::new(move || frame(UiEvent::Model(BTreeMap::new()))));
        if let Some(runtime) = &self.runtime {
            if let Some(id) = self.registration.take() {
                runtime.unregister(id);
            }
            self.registration = Some(runtime.register(wakeup));
        }
    }
    fn viewport(&mut self, w: u32, h: u32) -> bool {
        if self.fixed_viewport {
            return false;
        }
        self.size = (w, h);
        self.session.set_viewport(w, h)
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        match &event {
            UiEvent::Click { x, y }
            | UiEvent::Hover { x, y }
            | UiEvent::Drag { x, y }
            | UiEvent::LocalDrag { x, y }
            | UiEvent::Scroll { x, y, .. } => self.pointer = Some((*x, *y)),
            _ => {}
        }
        // A closed, kept-alive panel stops receiving service wakeups; reopening registers
        // again, which delivers one fresh model hint.
        if let (UiEvent::PanelVisibility(open), Some(runtime), Some(wakeup)) =
            (&event, &self.runtime, &self.wakeup)
        {
            match (open, self.registration) {
                (false, Some(id)) => {
                    runtime.unregister(id);
                    self.registration = None;
                }
                (true, None) => self.registration = Some(runtime.register(wakeup.clone())),
                _ => {}
            }
        }
        // Host-owned actions never reach behavior.luau: input updates focus and press
        // state only, then the action is routed below.
        let host_action = match &event {
            UiEvent::Click { x, y } => self.session.press_action_at(*x, *y),
            UiEvent::Release => self.session.release_action(),
            UiEvent::Drag { x, y } => self.session.drag_action(*x, *y),
            UiEvent::Command { command, .. } => match command.as_str() {
                "enter" | "space" => self.session.focused_action(),
                "left" | "right" => self.session.range_action(command == "left"),
                _ => None,
            },
            _ => None,
        }
        .filter(|(_, name, _)| self.session.is_host_action(name));
        let mut feedback = None;
        let event = if let Some((id, name, mut argument)) = host_action {
            feedback = match &event {
                UiEvent::Click { x, y } => {
                    self.session.capture_pointer(*x, *y);
                    self.session.pointer_focus_id(&id, true, Some(*x))?
                }
                UiEvent::Drag { x, .. } => self.session.pointer_focus_id(&id, false, Some(*x))?,
                UiEvent::Release => self.session.release_without_activation()?,
                UiEvent::Command { command, .. }
                    if matches!(command.as_str(), "left" | "right") =>
                {
                    self.session.range_step_id(&id)?
                }
                _ => self.session.focus_id(&id)?,
            };
            if matches!(event, UiEvent::Click { .. } | UiEvent::Drag { .. }) {
                // A slider's value at the pointer, authored after the feedback above.
                argument = self.session.pointer_argument(&id).unwrap_or(argument);
            }
            UiEvent::Action { name, argument }
        } else {
            event
        };
        let reload = matches!(&event, UiEvent::Reload);
        let model_update = matches!(&event, UiEvent::Model(_));
        let scene = match event {
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Release => self.session.release()?,
            UiEvent::Hover { x, y } => self.session.hover(x, y)?,
            UiEvent::Drag { x, y } => self.session.drag(x, y)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Model(changes) => self.session.update_model(changes)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "tab" => self.session.focus_next(shift)?,
                "enter" | "space" => self.session.activate_focused()?,
                "escape" => self.session.graph_key("escape")?.flatten(),
                "left" => self
                    .session
                    .edit_key(crate::ui::input::EditKey::Left, shift)?,
                "right" => self
                    .session
                    .edit_key(crate::ui::input::EditKey::Right, shift)?,
                "home" | "end" | "up" | "down" | "page-up" | "page-down" => {
                    self.session.scroll_key(&command, shift, self.pointer)?
                }
                _ => None,
            },
            UiEvent::Action { name, argument }
                if PANEL_ACTIONS.contains(&name.as_str()) && self.session.is_host_action(&name) =>
            {
                match panel_event(&name, &argument, self.pointer, self.panel_edge) {
                    Ok(event) => {
                        if let Some(wakeup) = &self.wakeup {
                            wakeup(event);
                        }
                    }
                    Err(e) => eprintln!("folder action: {e}"),
                }
                None
            }
            UiEvent::Action { name, argument } => {
                let (name, argument) = if name == "desktop.mute.toggle" {
                    let Some(snapshot) = self
                        .runtime
                        .as_ref()
                        .and_then(|runtime| runtime.desktop.as_ref())
                        .map(|desktop| desktop.snapshot())
                    else {
                        eprintln!(
                            "desktop action desktop.mute.toggle: audio service is unavailable"
                        );
                        return Ok(None);
                    };
                    if !snapshot.audio.connected {
                        eprintln!(
                            "desktop action desktop.mute.toggle: audio service is unavailable"
                        );
                        return Ok(None);
                    }
                    ("desktop.mute".to_owned(), toggled_mute(&snapshot))
                } else {
                    (name, argument)
                };
                let empty = BTreeSet::new();
                let declared = self
                    .runtime
                    .as_ref()
                    .map_or(&empty, |runtime| &runtime.services.desktop);
                match route_folder_action(&name, &argument, declared, self.pointer) {
                    Ok(Some(DesktopRequest::Action(action))) => {
                        if let Some(desktop) = self
                            .runtime
                            .as_ref()
                            .and_then(|runtime| runtime.desktop.as_ref())
                        {
                            if let Err(e) = desktop.action(action) {
                                eprintln!("desktop action {name}: {e}");
                            }
                        } else {
                            eprintln!("desktop action {name}: desktop service is not enabled");
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Media(action))) => {
                        if let Some(media) = self.runtime.as_ref().and_then(|r| r.media.as_ref()) {
                            let active = media.snapshot().active;
                            let result = active
                                .ok_or_else(|| "no active media player".to_owned())
                                .and_then(|id| {
                                    media.action(match action {
                                        MediaRequest::PlayPause => MediaAction::PlayPause(id),
                                        MediaRequest::Next => MediaAction::Next(id),
                                        MediaRequest::Previous => MediaAction::Previous(id),
                                        MediaRequest::Seek(ms) => MediaAction::Seek(id, ms),
                                    })
                                });
                            if let Err(e) = result {
                                eprintln!("desktop action {name}: {e}");
                            }
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Power(action))) => {
                        if let Some(power) = self.runtime.as_ref().and_then(|r| r.power.as_ref()) {
                            let snapshot = power.snapshot();
                            let profile = match action {
                                PowerRequest::SetProfile(profile) => Some(profile),
                                PowerRequest::Cycle => next_power_profile(&snapshot),
                            };
                            if let Some(profile) = profile
                                && let Err(error) =
                                    power.action(PowerProfilesAction::SetProfile(profile))
                            {
                                eprintln!("desktop action {name}: {error}");
                            }
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Bluetooth(action))) => {
                        if let Some(service) = self
                            .runtime
                            .as_ref()
                            .and_then(|r| r.bluetooth_devices.as_ref())
                        {
                            let snapshot = service.snapshot();
                            let result = match action {
                                BluetoothRequest::Power(value) => {
                                    default_bluetooth_adapter(&snapshot)
                                        .ok_or_else(|| {
                                            "no Bluetooth adapter is available".to_owned()
                                        })
                                        .and_then(|path| {
                                            service.action(BluetoothAction::SetPowered(path, value))
                                        })
                                }
                                BluetoothRequest::Scan(value) => {
                                    default_bluetooth_adapter(&snapshot)
                                        .ok_or_else(|| {
                                            "no Bluetooth adapter is available".to_owned()
                                        })
                                        .and_then(|path| {
                                            service.action(if value {
                                                BluetoothAction::StartDiscovery(path)
                                            } else {
                                                BluetoothAction::StopDiscovery(path)
                                            })
                                        })
                                }
                                BluetoothRequest::Connect(path) => {
                                    service.action(BluetoothAction::Connect(path))
                                }
                                BluetoothRequest::Disconnect(path) => {
                                    service.action(BluetoothAction::Disconnect(path))
                                }
                                BluetoothRequest::Forget(path) => {
                                    service.action(BluetoothAction::Remove(path))
                                }
                            };
                            if let Err(error) = result {
                                eprintln!("desktop action {name}: {error}");
                            }
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Network(request))) => {
                        if let Some(service) = self
                            .runtime
                            .as_ref()
                            .and_then(|r| r.network_devices.as_ref())
                            && let Err(error) = network_action(&service.snapshot(), request)
                                .and_then(|action| service.action(action))
                        {
                            eprintln!("desktop action {name}: {error}");
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Compositor(action))) => {
                        let backend = self
                            .runtime
                            .as_ref()
                            .and_then(|runtime| runtime.compositor.as_deref());
                        if let Err(error) = send_compositor_action(backend, action) {
                            eprintln!("desktop action {name}: {error}");
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Command(command))) => {
                        if let Some(runtime) = &self.runtime {
                            runtime.run_command(&command);
                        } else {
                            eprintln!("unknown command '{command}'");
                        }
                        None
                    }
                    Err(e) => {
                        eprintln!("folder action: {e}");
                        None
                    }
                    Ok(None) => {
                        let patch = self.session.action_values(&name, argument)?;
                        self.session.update(patch)?
                    }
                }
            }
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::WatchError(e) => return Err(e.into()),
            _ => None,
        };
        let mut scene = scene.or(feedback);
        if reload || model_update {
            scene = self.apply_external()?.or(scene);
        }
        let scene = scene.map(|scene| self.surface_scene(scene));
        self.pending = scene.clone();
        Ok(scene)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn state_summary(&self) -> String {
        self.session.state_summary()
    }
    fn state_revision(&self) -> Option<u64> {
        Some(self.session.state_revision())
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn commit(&mut self) -> bool {
        let Some(scene) = self.pending.take() else {
            return true;
        };
        self.session.commit(scene);
        self.initialized = true;
        if self.needs_external_refresh {
            self.needs_external_refresh = false;
            if let (Some(runtime), Some(id)) = (&self.runtime, self.registration) {
                runtime.notify_view(id);
            }
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        let scene = self.session.reject(error);
        self.surface_scene(scene)
    }
}
fn next_power_profile(snapshot: &PowerProfilesSnapshot) -> Option<String> {
    if snapshot.profiles.is_empty() {
        return None;
    }
    let index = snapshot
        .profiles
        .iter()
        .position(|profile| profile == &snapshot.active)
        .map_or(0, |i| (i + 1) % snapshot.profiles.len());
    Some(snapshot.profiles[index].clone())
}

impl Drop for FolderView {
    fn drop(&mut self) {
        if let (Some(runtime), Some(id)) = (&self.runtime, self.registration.take()) {
            runtime.unregister(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn folder_services_declare_typed_stores_without_behavior_boilerplate() {
        let root = std::env::temp_dir().join(format!(
            "bead-folder-host-stores-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("view")).unwrap();
        std::fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"202020"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}",
        )
        .unwrap();
        std::fs::write(root.join("view/main.luau"), "local ui=require('bead.ui');return ui.Column{id='root',width='fill',height='fill',ui.Text{id='clock',text=ui.bind('desktop.clock')},ui.Text{id='stdout',text=ui.bind('commands.kernel.stdout')},ui.Button{id='focus-workspace',text='Workspace',on_activate=ui.action('desktop.workspace.focus',2)},ui.Button{id='focus-window',text='Window',on_activate=ui.action('desktop.window.focus','window:opaque')}}").unwrap();
        let services = FolderServices {
            desktop: BTreeSet::from(["clock".into(), "workspaces".into()]),
            commands: BTreeMap::from([(
                "kernel".into(),
                CommandSpec {
                    argv: vec!["uname".into()],
                    interval: None,
                    timeout: Duration::from_secs(5),
                },
            )]),
        };
        let mut view = FolderView::load_with_services(
            root.clone(),
            "view/main.luau".into(),
            "palette.json".into(),
            "Stores",
            (400, 80),
            services.clone(),
        )
        .unwrap();
        let scene = view.check_render(UiEvent::Reload).unwrap();
        assert!(scene.primitives.iter().any(|primitive| matches!(primitive, crate::render::Primitive::Text { text, .. } if text.is_empty())));
        assert!(view.session.has_store("desktop.clock"));
        assert!(view.session.has_store("desktop.workspaces"));
        assert!(view.session.has_store("desktop.window.title"));
        assert!(view.session.has_store("desktop.window.app_id"));
        assert!(view.session.has_store("desktop.compositor"));
        assert!(view.session.has_store("commands.kernel.stdout"));

        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={['desktop.clock']=''},actions={},computed={}} ",
        )
        .unwrap();
        assert!(
            view.check_render(UiEvent::Reload).is_ok(),
            "same-type view declaration remains valid"
        );
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={['desktop.clock']=false},actions={},computed={}} ",
        )
        .unwrap();
        let error = view
            .check_render(UiEvent::Reload)
            .err()
            .expect("conflicting view declaration is rejected")
            .to_string();
        assert!(
            error.contains(
                "host-declared store desktop.clock is Text, but behavior.luau declares Bool"
            ),
            "{error}"
        );
        drop(view);

        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}} ",
        )
        .unwrap();
        let mut plain = FolderView::load(
            root.clone(),
            "view/main.luau".into(),
            "palette.json".into(),
            "No stores",
            (400, 80),
        )
        .unwrap();
        let error = plain
            .check_render(UiEvent::Reload)
            .err()
            .expect("undeclared host state is rejected")
            .to_string();
        assert!(
            error.contains("unknown"),
            "host without declarations keeps unknown-state behavior: {error}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clicked_host_actions_are_routed_natively_not_through_behavior_actions() {
        let root = std::env::temp_dir().join(format!(
            "bead-folder-host-click-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("view")).unwrap();
        std::fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"202020"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}",
        )
        .unwrap();
        std::fs::write(
            root.join("view/main.luau"),
            "local ui=require('bead.ui');return ui.Row{id='root',width='fill',height='fill',ui.Button{id='play',width=120,height=30,text='Play',on_activate=ui.action('desktop.media.play_pause')},ui.Button{id='later',width=120,height=30,text='Later',activate_on_release=true,on_activate=ui.action('desktop.media.next')}}",
        )
        .unwrap();
        let mut view = FolderView::load_with_services(
            root.clone(),
            "view/main.luau".into(),
            "palette.json".into(),
            "Host actions",
            (400, 40),
            FolderServices {
                desktop: BTreeSet::from(["media".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        view.stage(UiEvent::Reload).unwrap();
        view.commit();
        for (x, y) in [(60., 15.), (180., 15.)] {
            view.stage(UiEvent::Click { x, y })
                .unwrap_or_else(|e| panic!("press at {x}: {e}"));
            view.stage(UiEvent::Release)
                .unwrap_or_else(|e| panic!("release at {x}: {e}"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tray_service_enables_the_fallback_watcher() {
        assert!(desktop_config(&BTreeSet::from(["tray".to_owned()])).tray_watcher);
    }

    #[test]
    fn folder_source_exposes_content_regions_and_keyboard_animation_wakeups() {
        let root = std::env::temp_dir().join(format!(
            "bead-scroll-host-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("view")).unwrap();
        std::fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"202020"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={offset=0},actions={},computed={}}",
        )
        .unwrap();
        std::fs::write(root.join("view/main.luau"),"local ui=require('bead.ui');return ui.Scroll{id='list',width=200,height=100,scroll=ui.bind('offset'),ui.Column{height=600,shrink=0}}").unwrap();
        let mut view = FolderView::load(
            root.clone(),
            "view/main.luau".into(),
            "palette.json".into(),
            "Scroll host",
            (200, 100),
        )
        .unwrap();
        view.stage(UiEvent::Reload).unwrap();
        assert!(view.commit());
        assert_eq!(view.scroll_regions(), vec![Rect::new(0., 0., 200., 100.)]);
        view.stage(UiEvent::Hover { x: 20., y: 40. }).unwrap();
        view.commit();
        assert!(
            view.stage(UiEvent::Command {
                command: "page-down".into(),
                shift: false
            })
            .unwrap()
            .is_some()
        );
        view.commit();
        assert!(view.wakeup_at().is_some());
        drop(view);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_bar_removes_panel_margin_and_uses_surface_viewport() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-bar");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Bar",
            (440, 36),
        )
        .unwrap();
        // Reproduce the old primary path: output-sized layout, then a 36px buffer.
        view.viewport(1280, 720);
        let raw = view.session.reload().unwrap();
        assert_eq!(raw.offset, (32., 32.));
        for primitive in &raw.primitives {
            if let crate::render::Primitive::Text { rect, .. } = primitive {
                assert!(
                    rect.y > 36.,
                    "old path positions centered labels against the full output height"
                );
            }
        }
        assert!(view.viewport(1280, 36));
        let scene = view.check_render(UiEvent::Reload).unwrap();
        assert_eq!(scene.offset, (0., 0.));
        assert_eq!(scene.bounds, Rect::new(0., 0., 1280., 36.));
        let mut renderer = crate::render::renderer(RendererKind::Cpu).unwrap();
        for (scene, visible) in [(&raw, false), (&scene, true)] {
            let mut bytes = vec![0; 1280 * 36 * 4];
            renderer.prepare(scene, 1.).unwrap();
            renderer
                .render(
                    scene,
                    ShmTarget {
                        bytes: &mut bytes,
                        width: 1280,
                        height: 36,
                        scale: 1.,
                        format: PixelFormat::Rgba,
                    },
                    Damage::full(1280, 36),
                )
                .unwrap();
            assert_eq!(
                bytes[64 * 4 + 3] != 0,
                visible,
                "surface fill along the top edge, outside rounded corners"
            );
        }
        for label in ["Hello bar", "12:34"] {
            assert!(scene.primitives.iter().any(|p| matches!(p,
                crate::render::Primitive::Text { text, rect, .. }
                if text == label && rect.y < 36. && rect.x >= 0. && rect.x + rect.w <= 1280.
            )));
        }
    }

    #[test]
    fn reload_failure_notice_fits_a_thin_bar() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-bar");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Bar",
            (1280, 36),
        )
        .unwrap();
        view.check_render(UiEvent::Reload).unwrap();
        let scene = view.error_scene("view/main.luau:13:1: syntax error: boom\nmore".into());
        let notice = scene.primitives.iter().find_map(|p| match p {
            crate::render::Primitive::Text { text, rect, .. } if text.starts_with("Reload") => {
                Some((text.as_str(), *rect))
            }
            _ => None,
        });
        let (text, rect) = notice.expect("reload failure notice");
        assert_eq!(
            text,
            "Reload failed — old UI kept: view/main.luau:13:1: syntax error: boom"
        );
        assert!(rect.y >= 0. && rect.y + rect.h <= 36., "{rect:?}");
    }

    #[test]
    fn desktop_projection_covers_live_and_unknown_values() {
        let empty = desktop_values(&Snapshot::default());
        assert_eq!(empty["desktop.battery.percent"], Value::Number(-1.));
        assert_eq!(empty["desktop.audio.volume"], Value::Number(-1.));
        assert_eq!(empty["desktop.network.strength"], Value::Number(-1.));
        let mut snapshot = Snapshot::default();
        snapshot.clock = "12:34".into();
        snapshot.clock_seconds = "12:34:56".into();
        snapshot.date = Some(bead_services::LocalDate {
            year: 2026,
            month: 11,
            day: 12,
        });
        snapshot.audio = bead_services::Audio {
            connected: true,
            default_sink: Some("Speakers".into()),
            volume: Some(0.42),
            muted: Some(true),
            ..Default::default()
        };
        snapshot.battery = bead_services::Battery {
            available: true,
            percentage: Some(83.),
            charging: true,
            on_battery: false,
        };
        snapshot.network = bead_services::Network {
            available: true,
            connected: true,
            wireless_enabled: true,
            name: "wifi".into(),
            strength: Some(70),
        };
        snapshot.bluetooth = bead_services::Bluetooth {
            available: true,
            powered: true,
            connected_devices: vec!["Headphones".into()],
        };
        snapshot.tray = vec![bead_services::TrayItem {
            id: "item".into(),
            title: "Tray".into(),
            icon_name: "icon".into(),
            attention: true,
            has_menu: true,
            ..Default::default()
        }];
        snapshot.session = bead_services::Session {
            locked: true,
            ..Default::default()
        };
        let values = desktop_values(&snapshot);
        assert_eq!(
            values["desktop.clock.seconds"],
            Value::Text("12:34:56".into())
        );
        assert_eq!(values["desktop.date"], Value::Text("2026-11-12".into()));
        assert_eq!(values["desktop.audio.volume"], Value::Number(0.42));
        assert_eq!(values["desktop.battery.percent"], Value::Number(83.));
        assert_eq!(values["desktop.network.strength"], Value::Number(70.));
        assert_eq!(values["desktop.audio.available"], Value::Bool(true));
        assert_eq!(values["desktop.audio.muted"], Value::Bool(true));
        assert_eq!(values["desktop.audio.sink"], Value::Text("Speakers".into()));
        assert_eq!(values["desktop.battery.charging"], Value::Bool(true));
        assert_eq!(values["desktop.network.connected"], Value::Bool(true));
        assert_eq!(values["desktop.network.wireless"], Value::Bool(true));
        assert_eq!(values["desktop.network.name"], Value::Text("wifi".into()));
        assert_eq!(values["desktop.bluetooth.powered"], Value::Bool(true));
        assert!(
            matches!(&values["desktop.bluetooth.connected"],Value::List(devices) if devices[0]["name"]=="Headphones")
        );
        assert!(
            matches!(&values["desktop.tray"],Value::List(items) if items[0]["id"]=="item" && items[0]["title"]=="Tray" && items[0]["icon_name"]=="icon" && items[0]["attention"]==true && items[0]["has_menu"]==true)
        );
        assert_eq!(values["desktop.session.locked"], Value::Bool(true));
        let subset = declared_desktop_values(&snapshot, &BTreeSet::from(["clock".to_owned()]));
        assert_eq!(
            subset.keys().cloned().collect::<Vec<_>>(),
            vec!["desktop.clock", "desktop.clock.seconds", "desktop.date"]
        );
    }

    #[test]
    fn media_projection_and_position_tick_use_controlled_monotonic_time() {
        let empty = media_values(&MediaSnapshot::default(), 10_000);
        assert_eq!(empty["desktop.media.available"], Value::Bool(false));
        assert_eq!(empty["desktop.media.player"], Value::Text(String::new()));
        assert_eq!(empty["desktop.media.position_ms"], Value::Number(-1.));
        let player = bead_services::media::Player {
            id: "org.mpris.MediaPlayer2.example".into(),
            identity: "Example Player".into(),
            status: "Playing".into(),
            title: "Track".into(),
            artist: "One, Two".into(),
            length_ms: 20_000,
            position_ms: 5_000,
            position_at: 10_000,
            can_play_pause: true,
            ..Default::default()
        };
        let mut snapshot = MediaSnapshot::default();
        snapshot.revision = 1;
        snapshot.players = vec![player];
        snapshot.active = Some("org.mpris.MediaPlayer2.example".into());
        assert!(media_is_playing(&snapshot));
        let values = media_values(&snapshot, 11_000);
        assert_eq!(values["desktop.media.available"], Value::Bool(true));
        assert_eq!(
            values["desktop.media.player"],
            Value::Text("Example Player".into())
        );
        assert_eq!(values["desktop.media.position_ms"], Value::Number(6_000.));
        let mut paused = snapshot.clone();
        paused.players[0].status = "Paused".into();
        assert!(!media_is_playing(&paused));
        assert!(!media_tick_due(&paused, 10_000, 12_000));
        assert!(!media_tick_due(&snapshot, 10_000, 10_999));
        assert!(media_tick_due(&snapshot, 10_000, 11_000));
        assert!(!media_position_due(&snapshot, Some(10_000), 10_999));
        assert!(media_position_due(&snapshot, Some(10_000), 11_000));
        assert!(media_position_due(&snapshot, None, 10_000));
        assert!(media_position_due(&paused, Some(10_000), 10_001));
        assert_eq!(
            media_values(&paused, 15_000)["desktop.media.position_ms"],
            Value::Number(5_000.)
        );
    }

    #[test]
    fn power_projection_and_actions_follow_daemon_order() {
        let empty = power_values(&PowerProfilesSnapshot::default());
        assert_eq!(empty["desktop.power.available"], Value::Bool(false));
        assert_eq!(empty["desktop.power.profile"], Value::Text(String::new()));
        assert!(
            matches!(&empty["desktop.power.profiles"], Value::List(profiles) if profiles.is_empty())
        );
        let mut snapshot = PowerProfilesSnapshot::default();
        snapshot.revision = 2;
        snapshot.available = true;
        snapshot.active = "balanced".into();
        snapshot.profiles = vec![
            "power-saver".into(),
            "balanced".into(),
            "performance".into(),
        ];
        let values = power_values(&snapshot);
        assert_eq!(
            values["desktop.power.profile"],
            Value::Text("balanced".into())
        );
        assert!(
            matches!(&values["desktop.power.profiles"], Value::List(profiles) if profiles[0]["name"] == "power-saver")
        );
        assert_eq!(
            next_power_profile(&snapshot).as_deref(),
            Some("performance")
        );
        let mut last = snapshot.clone();
        last.active = "performance".into();
        assert_eq!(next_power_profile(&last).as_deref(), Some("power-saver"));
        let declared = BTreeSet::from(["power".to_owned()]);
        assert!(
            matches!(route_folder_action("desktop.power.set", &Value::Text("balanced".into()), &declared, None), Ok(Some(DesktopRequest::Power(PowerRequest::SetProfile(profile)))) if profile == "balanced")
        );
        assert!(matches!(
            route_folder_action("desktop.power.cycle", &Value::Bool(false), &declared, None),
            Ok(Some(DesktopRequest::Power(PowerRequest::Cycle)))
        ));
        assert!(
            route_folder_action("desktop.power.set", &Value::Bool(false), &declared, None).is_err()
        );
        assert!(
            route_folder_action(
                "desktop.power.cycle",
                &Value::Bool(false),
                &BTreeSet::new(),
                None
            )
            .unwrap_err()
            .contains("undeclared service")
        );
    }

    #[test]
    fn media_actions_route_only_when_declared_and_validate_seek() {
        let declared = BTreeSet::from(["media".to_owned()]);
        assert!(matches!(
            route_folder_action("desktop.media.next", &Value::Bool(false), &declared, None),
            Ok(Some(DesktopRequest::Media(MediaRequest::Next)))
        ));
        assert!(matches!(
            route_folder_action("desktop.media.seek", &Value::Number(1200.), &declared, None),
            Ok(Some(DesktopRequest::Media(MediaRequest::Seek(1200))))
        ));
        assert!(
            route_folder_action("desktop.media.seek", &Value::Number(-1.), &declared, None)
                .is_err()
        );
        assert!(
            route_folder_action(
                "desktop.media.play_pause",
                &Value::Bool(false),
                &BTreeSet::new(),
                None
            )
            .unwrap_err()
            .contains("undeclared service")
        );
    }

    #[test]
    fn bluetooth_device_projection_and_actions_are_opt_in_and_typed() {
        let empty = bluetooth_device_values(&BluetoothSnapshot::default());
        assert_eq!(
            empty["desktop.bluetooth_devices.available"],
            Value::Bool(false)
        );
        assert_eq!(
            empty["desktop.bluetooth_devices.powered"],
            Value::Bool(false)
        );
        assert!(
            matches!(&empty["desktop.bluetooth_devices.devices"],Value::List(devices) if devices.is_empty())
        );
        let mut snapshot = BluetoothSnapshot::default();
        snapshot.revision = 1;
        snapshot.available = true;
        snapshot.adapters = vec![bead_services::bluetooth::BluetoothAdapter {
            path: "/org/bluez/hci0".into(),
            name: "adapter".into(),
            powered: true,
            discovering: true,
            pairable: true,
        }];
        snapshot.devices = vec![bead_services::bluetooth::BluetoothDevice {
            path: "/org/bluez/hci0/dev_AA".into(),
            adapter: "/org/bluez/hci0".into(),
            address: "AA".into(),
            name: "Headphones".into(),
            icon: "audio-headphones".into(),
            paired: true,
            trusted: true,
            connected: true,
            battery: Some(73),
            rssi: Some(-42),
        }];
        let values = bluetooth_device_values(&snapshot);
        assert_eq!(
            values["desktop.bluetooth_devices.powered"],
            Value::Bool(true)
        );
        assert_eq!(
            values["desktop.bluetooth_devices.discovering"],
            Value::Bool(true)
        );
        assert!(
            matches!(&values["desktop.bluetooth_devices.devices"],Value::List(devices) if devices[0]["name"]=="Headphones"&&devices[0]["battery"]==73)
        );
        let declared = BTreeSet::from(["bluetooth_devices".to_owned()]);
        assert!(matches!(
            route_folder_action(
                "desktop.bluetooth_devices.power",
                &Value::Bool(true),
                &declared,
                None
            ),
            Ok(Some(DesktopRequest::Bluetooth(BluetoothRequest::Power(
                true
            ))))
        ));
        assert!(matches!(
            route_folder_action(
                "desktop.bluetooth_devices.scan",
                &Value::Bool(false),
                &declared,
                None
            ),
            Ok(Some(DesktopRequest::Bluetooth(BluetoothRequest::Scan(
                false
            ))))
        ));
        assert!(
            matches!(route_folder_action("desktop.bluetooth_devices.connect",&Value::Text("/device".into()),&declared,None),Ok(Some(DesktopRequest::Bluetooth(BluetoothRequest::Connect(path)))) if path=="/device")
        );
        assert!(matches!(
            route_folder_action(
                "desktop.bluetooth_devices.disconnect",
                &Value::Text("/device".into()),
                &declared,
                None
            ),
            Ok(Some(DesktopRequest::Bluetooth(BluetoothRequest::Disconnect(path)))) if path == "/device"
        ));
        assert!(matches!(
            route_folder_action(
                "desktop.bluetooth_devices.forget",
                &Value::Text("/device".into()),
                &declared,
                None
            ),
            Ok(Some(DesktopRequest::Bluetooth(BluetoothRequest::Forget(path)))) if path == "/device"
        ));
        assert!(
            route_folder_action(
                "desktop.bluetooth_devices.scan",
                &Value::Bool(true),
                &BTreeSet::new(),
                None
            )
            .unwrap_err()
            .contains("undeclared service")
        );
        assert!(
            route_folder_action(
                "desktop.bluetooth_devices.forget",
                &Value::Bool(false),
                &declared,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn network_device_projection_routing_and_resolution() {
        use bead_services::network::{
            NetworkAccessPoint, NetworkConnection, NetworkDevice, NetworkPrimary,
        };
        let empty = network_device_values(&NetworkSnapshot::default());
        assert_eq!(empty.len(), 6);
        assert_eq!(
            empty["desktop.network_devices.available"],
            Value::Bool(false)
        );
        assert_eq!(
            empty["desktop.network_devices.primary_kind"],
            Value::Text(String::new())
        );
        assert!(
            matches!(&empty["desktop.network_devices.access_points"], Value::List(l) if l.is_empty())
        );
        let point = |ssid: &str, security, known| NetworkAccessPoint {
            path: format!("/ap/{ssid}"),
            device: "/dev/wlan".into(),
            ssid: ssid.into(),
            ssid_bytes: ssid.as_bytes().to_vec(),
            strength: 60,
            security,
            frequency_mhz: 2412,
            active: known,
            known,
        };
        let saved = |path: &str, id: &str, ssid: &str| NetworkConnection {
            path: path.into(),
            id: id.into(),
            kind: if ssid.is_empty() { "ethernet" } else { "wifi" },
            ssid: ssid.as_bytes().to_vec(),
        };
        let mut snapshot = NetworkSnapshot::default();
        snapshot.revision = 1;
        snapshot.available = true;
        snapshot.wireless_enabled = true;
        snapshot.wireless_hardware_enabled = true;
        snapshot.primary = Some(NetworkPrimary {
            kind: "wifi",
            name: "Home".into(),
            device: "wlan0".into(),
            state: "activated",
        });
        snapshot.devices = vec![
            NetworkDevice {
                path: "/dev/eth".into(),
                interface: "eth0".into(),
                kind: "ethernet",
                state: "connected",
                active_connection: "/active/eth".into(),
            },
            NetworkDevice {
                path: "/dev/wlan".into(),
                interface: "wlan0".into(),
                kind: "wifi",
                state: "connected",
                active_connection: "/active/wlan".into(),
            },
        ];
        snapshot.access_points = vec![
            point("Home", "wpa", true),
            point("Cafe", "open", false),
            point("Secure", "wpa3", false),
        ];
        snapshot.known_connections = vec![
            saved("/s/1", "Home", "Home"),
            saved("/s/2", "Wired", ""),
            saved("/s/3", "Twin", ""),
            saved("/s/4", "Twin", ""),
        ];
        let values = network_device_values(&snapshot);
        assert_eq!(
            values["desktop.network_devices.wireless_enabled"],
            Value::Bool(true)
        );
        assert_eq!(
            values["desktop.network_devices.primary_name"],
            Value::Text("Home".into())
        );
        assert_eq!(
            values["desktop.network_devices.primary_kind"],
            Value::Text("wifi".into())
        );
        assert!(
            matches!(&values["desktop.network_devices.access_points"], Value::List(l) if l[0]["ssid"] == "Home" && l[0]["strength"] == 60 && l[0]["security"] == "wpa" && l[0]["active"] == true && l[0]["known"] == true && l[1]["known"] == false)
        );
        assert!(
            matches!(&values["desktop.network_devices.known"], Value::List(l) if l.len() == 4 && l[1]["id"] == "Wired" && l[1]["kind"] == "ethernet")
        );

        let declared = BTreeSet::from(["network_devices".to_owned()]);
        let route = |name: &str, argument: Value| match route_folder_action(
            name, &argument, &declared, None,
        )? {
            Some(DesktopRequest::Network(request)) => network_action(&snapshot, request),
            other => Err(format!("unexpected route {other:?}")),
        };
        assert!(matches!(
            route("desktop.network_devices.wireless", Value::Bool(false)),
            Ok(NetworkAction::SetWirelessEnabled(false))
        ));
        assert!(
            matches!(route("desktop.network_devices.scan", Value::Bool(true)), Ok(NetworkAction::Scan(d)) if d == "/dev/wlan")
        );
        assert!(
            matches!(route("desktop.network_devices.disconnect", Value::Bool(true)), Ok(NetworkAction::Disconnect(d)) if d == "/dev/wlan")
        );
        assert!(
            matches!(route("desktop.network_devices.connect", Value::Text("Home".into())), Ok(NetworkAction::ActivateKnown(c, d)) if c == "/s/1" && d == "/dev/wlan")
        );
        assert!(
            matches!(route("desktop.network_devices.connect", Value::Text("Cafe".into())), Ok(NetworkAction::ActivateOpen(ssid, d)) if ssid == b"Cafe" && d == "/dev/wlan")
        );
        // Secured unknown networks route to ActivateOpen, which the service rejects for the agent.
        assert!(matches!(
            route(
                "desktop.network_devices.connect",
                Value::Text("Secure".into())
            ),
            Ok(NetworkAction::ActivateOpen(..))
        ));
        assert!(
            route(
                "desktop.network_devices.connect",
                Value::Text("Gone".into())
            )
            .is_err()
        );
        assert!(
            matches!(route("desktop.network_devices.forget", Value::Text("Wired".into())), Ok(NetworkAction::Forget(c)) if c == "/s/2")
        );
        assert!(
            route("desktop.network_devices.forget", Value::Text("Twin".into()))
                .unwrap_err()
                .contains("ambiguous")
        );
        assert!(route("desktop.network_devices.wireless", Value::Text("on".into())).is_err());
        assert!(route("desktop.network_devices.connect", Value::Bool(true)).is_err());
        assert!(
            route_folder_action(
                "desktop.network_devices.scan",
                &Value::Bool(true),
                &BTreeSet::from(["network".to_owned()]),
                None
            )
            .unwrap_err()
            .contains("undeclared service 'network_devices'")
        );
        let mut idle = snapshot.clone();
        idle.devices.clear();
        assert!(network_action(&idle, NetworkRequest::Scan).is_err());
        assert!(network_action(&idle, NetworkRequest::Disconnect).is_err());
        assert!(
            !declared_desktop_values(&Snapshot::default(), &declared)
                .keys()
                .any(|key| key.starts_with("desktop.network.")),
            "network_devices does not publish the Desktop network summary"
        );
    }

    #[test]
    fn compositor_projection_and_actions_are_opt_in_and_typed() {
        let empty = compositor_values(None);
        assert!(matches!(&empty["desktop.workspaces"], Value::List(items) if items.is_empty()));
        assert_eq!(empty["desktop.window.title"], Value::Text(String::new()));
        assert_eq!(empty["desktop.window.app_id"], Value::Text(String::new()));
        assert_eq!(empty["desktop.compositor"], Value::Text(String::new()));

        let snapshot = bead_backends::Snapshot {
            backend: "niri".into(),
            workspaces: vec![bead_backends::Workspace {
                id: CompositorId("workspace:opaque".into()),
                name: "main".into(),
                index: 2,
                output: Some(CompositorId("output:opaque".into())),
                active: true,
                focused: true,
                urgent: false,
                fixed_slot: false,
                visiting: false,
            }],
            windows: vec![bead_backends::Window {
                id: CompositorId("window:opaque".into()),
                title: "Editor".into(),
                app_id: "org.example.Editor".into(),
                workspace: None,
                output: None,
                focused: true,
                floating: false,
                fullscreen: None,
                urgent: false,
            }],
            ..Default::default()
        };
        let values = compositor_values(Some(&snapshot));
        assert!(matches!(&values["desktop.workspaces"], Value::List(items)
            if items[0]["id"] == "workspace:opaque" && items[0]["index"] == 2
                && items[0]["name"] == "main" && items[0]["output"] == "output:opaque"
                && items[0]["active"] == true && items[0]["focused"] == true
                && items[0]["urgent"] == false));
        assert_eq!(values["desktop.window.title"], Value::Text("Editor".into()));
        assert_eq!(
            values["desktop.window.app_id"],
            Value::Text("org.example.Editor".into())
        );
        assert_eq!(values["desktop.compositor"], Value::Text("niri".into()));

        let declared = BTreeSet::from(["workspaces".to_owned()]);
        let index_action = route_folder_action(
            "desktop.workspace.focus",
            &Value::Number(2.),
            &declared,
            None,
        )
        .unwrap()
        .unwrap();
        assert!(matches!(
            index_action,
            DesktopRequest::Compositor(CompositorAction::SwitchWorkspaceIndex { index: 2 })
        ));
        let id_action = route_folder_action(
            "desktop.workspace.focus",
            &Value::Text("workspace:opaque".into()),
            &declared,
            None,
        )
        .unwrap()
        .unwrap();
        assert!(matches!(
            id_action,
            DesktopRequest::Compositor(CompositorAction::SwitchWorkspace { workspace })
                if workspace.0 == "workspace:opaque"
        ));
        let window_action = route_folder_action(
            "desktop.window.focus",
            &Value::Text("window:opaque".into()),
            &declared,
            None,
        )
        .unwrap()
        .unwrap();
        assert!(matches!(
            window_action,
            DesktopRequest::Compositor(CompositorAction::FocusWindow { window })
                if window.0 == "window:opaque"
        ));
        assert!(
            route_folder_action(
                "desktop.workspace.focus",
                &Value::Number(1.5),
                &declared,
                None
            )
            .is_err()
        );
        assert!(
            route_folder_action("desktop.window.focus", &Value::Bool(true), &declared, None)
                .is_err()
        );
        assert!(
            route_folder_action(
                "desktop.workspace.focus",
                &Value::Number(1.),
                &BTreeSet::new(),
                None
            )
            .unwrap_err()
            .contains("undeclared service 'workspaces'")
        );

        struct FakeBackend {
            actions: Mutex<Vec<CompositorAction>>,
        }
        impl CompositorBackend for FakeBackend {
            fn snapshot(&self) -> Arc<bead_backends::Snapshot> {
                Arc::new(bead_backends::Snapshot::default())
            }
            fn action(&self, action: CompositorAction) -> Result<(), bead_backends::Error> {
                self.actions.lock().unwrap().push(action);
                Ok(())
            }
            fn subscribe(
                &self,
                _callback: Box<dyn Fn(bead_backends::Event) + Send + Sync>,
            ) -> bead_backends::Subscription {
                bead_backends::Subscription::new(|| {})
            }
        }
        let fake = FakeBackend {
            actions: Mutex::new(Vec::new()),
        };
        let action = CompositorAction::FocusWindow {
            window: CompositorId("window:opaque".into()),
        };
        send_compositor_action(Some(&fake), action.clone()).unwrap();
        assert_eq!(fake.actions.lock().unwrap().as_slice(), &[action]);
        assert!(
            send_compositor_action(None, CompositorAction::SwitchWorkspaceIndex { index: 1 })
                .unwrap_err()
                .contains("no compositor detected")
        );
    }

    #[test]
    fn action_router_checks_declarations_and_clamps_volume() {
        let declared = BTreeSet::from(["audio".to_owned(), "tray".to_owned()]);
        let Ok(Some(DesktopRequest::Action(DesktopAction::Volume(v)))) =
            route_folder_action("desktop.volume", &Value::Number(2.), &declared, None)
        else {
            panic!("volume was not routed")
        };
        assert_eq!(v, 1.);
        let Ok(Some(DesktopRequest::Action(DesktopAction::Volume(low)))) =
            route_folder_action("desktop.volume", &Value::Number(-1.), &declared, None)
        else {
            panic!("low volume was not routed")
        };
        assert_eq!(low, 0.);
        assert!(
            route_folder_action("desktop.mute", &Value::Bool(true), &BTreeSet::new(), None)
                .unwrap_err()
                .contains("undeclared service")
        );
        assert!(
            route_folder_action("desktop.unknown", &Value::Bool(true), &declared, None)
                .unwrap_err()
                .contains("unknown desktop action")
        );
        let Ok(Some(DesktopRequest::Action(DesktopAction::TrayActivate { item, x, y }))) =
            route_folder_action(
                "desktop.tray.activate",
                &Value::Text("x".into()),
                &declared,
                None,
            )
        else {
            panic!("tray was not routed")
        };
        assert_eq!((item, x, y), ("x".into(), 0, 0));
        let Ok(Some(DesktopRequest::Action(DesktopAction::TraySecondaryActivate { x, y, .. }))) =
            route_folder_action(
                "desktop.tray.secondary",
                &Value::Text("x".into()),
                &declared,
                Some((12.7, 4.3)),
            )
        else {
            panic!("tray secondary action was not routed")
        };
        assert_eq!((x, y), (13, 4));
        assert!(
            route_folder_action("increment", &Value::Bool(true), &declared, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn mute_toggle_inverts_latest_snapshot() {
        let mut snapshot = Snapshot::default();
        snapshot.audio.muted = Some(false);
        assert_eq!(toggled_mute(&snapshot), Value::Bool(true));
        snapshot.audio.muted = Some(true);
        assert_eq!(toggled_mute(&snapshot), Value::Bool(false));
    }

    #[test]
    fn declared_commands_trim_bound_fail_and_time_out() {
        let spec = |argv: &[&str], timeout| CommandSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            interval: None,
            timeout: Duration::from_secs(timeout),
        };
        assert_eq!(
            execute_command(&spec(&["echo", "hi"], 5)),
            ("hi".into(), true, String::new())
        );
        let (_, ok, error) = execute_command(&spec(&["false"], 5));
        assert!(!ok);
        assert!(error.contains("exited"));
        let (_, ok, error) = execute_command(&spec(&["sleep", "10"], 1));
        assert!(!ok);
        assert!(error.contains("timeout"));
        let (stdout, ok, _) = execute_command(&spec(&["python3", "-c", "print('x'*5000)"], 5));
        assert!(ok);
        assert_eq!(stdout.len(), 4096);
        let (_, ok, error) = execute_command(&spec(&["python3", "-c", "print('x'*70000)"], 5));
        assert!(!ok);
        assert!(error.contains("output limit"));
    }

    #[test]
    fn shared_runtime_delivers_one_command_result_to_two_views() {
        use std::{os::unix::fs::PermissionsExt, sync::mpsc};

        let id = format!(
            "{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        );
        let base = std::env::temp_dir().join(format!("bead-folder-runtime-{id}"));
        std::fs::create_dir_all(&base).unwrap();
        let marker = base.join("runs");
        let go = base.join("go");
        let script = base.join("command.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nwhile [ ! -e '{}' ]; do sleep 0.01; done\nprintf x >> '{}'\nprintf 'hi\\n'\n",
                go.display(), marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let root = base.join("app");
        std::fs::create_dir_all(root.join("view")).unwrap();
        std::fs::write(
            root.join("view/main.luau"),
            "local ui=require('bead.ui'); return ui.Text{id='command',text=ui.bind('commands.kernel.stdout'),width=120,height=24}",
        )
        .unwrap();
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={['commands.kernel.stdout']='',['commands.kernel.ok']=false,['commands.kernel.error']=''},actions={},computed={}}",
        )
        .unwrap();
        std::fs::write(
            root.join("view/bundle.json"),
            r#"{"plugins":true,"modules":[]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{}}"#,
        )
        .unwrap();
        let runtime = FolderServicesRuntime::start(FolderServices {
            desktop: BTreeSet::from(["clock".into()]),
            commands: BTreeMap::from([(
                "kernel".into(),
                CommandSpec {
                    argv: vec![script.display().to_string()],
                    interval: Some(Duration::from_secs(600)),
                    timeout: Duration::from_secs(5),
                },
            )]),
        })
        .unwrap();
        let mut first = FolderView::load_with_runtime(
            root.clone(),
            "view/main.luau".into(),
            "palette.json".into(),
            "Status one",
            (1280, 42),
            runtime.clone(),
        )
        .unwrap();
        let mut second = FolderView::load_with_runtime(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Status two",
            (1280, 42),
            runtime.clone(),
        )
        .unwrap();
        assert!(Arc::ptr_eq(
            first.runtime.as_ref().unwrap(),
            second.runtime.as_ref().unwrap()
        ));
        let (first_tx, first_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        first.services(Box::new(move |event| {
            let _ = first_tx.send(event);
        }));
        second.services(Box::new(move |event| {
            let _ = second_tx.send(event);
        }));
        let _ = first_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let _ = second_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        std::fs::write(&go, "go").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while runtime
            .command_values()
            .get("kernel")
            .is_none_or(|result| result.0 != "hi")
        {
            assert!(
                std::time::Instant::now() < deadline,
                "startup command did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = first_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let _ = second_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let _ = first.check_render(UiEvent::Reload).unwrap();
        let _ = second.check_render(UiEvent::Reload).unwrap();
        let _ = first.check_render(UiEvent::Model(BTreeMap::new())).unwrap();
        let _ = second
            .check_render(UiEvent::Model(BTreeMap::new()))
            .unwrap();
        for view in [&first, &second] {
            let stores: serde_json::Value =
                serde_json::from_str(&view.session.state_summary()).unwrap();
            assert_eq!(stores["commands.kernel.stdout"], "hi");
            assert!(stores["commands.kernel.ok"].as_bool().unwrap());
        }
        assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), 1);
        drop(first);
        drop(second);
        drop(runtime);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn runtime_with_every_service_declared_stops_within_a_bound() {
        let runtime = FolderServicesRuntime::start(FolderServices {
            desktop: DESKTOP_SERVICES.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let started = std::time::Instant::now();
        drop(runtime);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "service workers must stop promptly: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn interval_scheduler_stops_promptly_when_runtime_drops() {
        let runtime = FolderServicesRuntime::start(FolderServices {
            commands: BTreeMap::from([(
                "slow".into(),
                CommandSpec {
                    argv: vec!["true".into()],
                    interval: Some(Duration::from_secs(600)),
                    timeout: Duration::from_secs(5),
                },
            )]),
            ..Default::default()
        })
        .unwrap();
        let started = std::time::Instant::now();
        drop(runtime);
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn hello_status_renders_populated_workspaces_and_updates_focus() {
        fn model(focused: &str, title: &str) -> BTreeMap<String, Value> {
            BTreeMap::from([
                ("desktop.workspaces".into(), Value::list([("code", "Code"), ("web", "Web")]
                    .into_iter().enumerate().map(|(i, (id, name))| serde_json::json!({
                        "id": id, "index": i + 1, "name": name, "output": "",
                        "active": id == focused, "focused": id == focused, "urgent": false
                    })).collect())),
                ("desktop.window.title".into(), Value::Text(title.into())),
            ])
        }
        fn button_fill(scene: &Scene, label: &str) -> [u8; 4] {
            use crate::render::Primitive;
            let text = scene
                .primitives
                .iter()
                .find_map(|p| match p {
                    Primitive::Text { text, rect, .. } if text == label => Some(*rect),
                    _ => None,
                })
                .expect("workspace button label");
            let (x, y) = (text.x + text.w / 2., text.y + text.h / 2.);
            scene
                .primitives
                .iter()
                .filter_map(|p| match p {
                    Primitive::Rounded { rect, fill, .. }
                        if x >= rect.x
                            && y >= rect.y
                            && x <= rect.x + rect.w
                            && y <= rect.y + rect.h =>
                    {
                        Some((rect, *fill))
                    }
                    _ => None,
                })
                .min_by(|(a, _), (b, _)| (a.w * a.h).total_cmp(&(b.w * b.h)))
                .expect("workspace button background")
                .1
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-status");
        let services = FolderServices {
            desktop: ["clock", "battery", "audio", "media", "power", "workspaces"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            commands: BTreeMap::from([(
                "kernel".into(),
                CommandSpec {
                    argv: vec!["uname".into(), "-r".into()],
                    interval: None,
                    timeout: Duration::from_secs(1),
                },
            )]),
        };
        let mut view = FolderView::load_with_services(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Status",
            (1280, 76),
            services.clone(),
        )
        .unwrap();
        view.check_render(UiEvent::Reload).unwrap();
        let first = view
            .check_render(UiEvent::Model(model("code", "Editor")))
            .unwrap();
        assert_ne!(button_fill(&first, "Code"), button_fill(&first, "Web"));
        let second = view
            .check_render(UiEvent::Model(model("web", "Browser")))
            .unwrap();
        assert_eq!(button_fill(&first, "Code"), button_fill(&second, "Web"));
        assert_eq!(button_fill(&first, "Web"), button_fill(&second, "Code"));
        assert!(second.primitives.iter().any(|p| matches!(p,
            crate::render::Primitive::Text {text, ..} if text == "Browser")));

        struct RecordingBackend(Mutex<Vec<CompositorAction>>);
        impl CompositorBackend for RecordingBackend {
            fn snapshot(&self) -> Arc<bead_backends::Snapshot> {
                Arc::new(bead_backends::Snapshot::default())
            }
            fn action(&self, action: CompositorAction) -> Result<(), bead_backends::Error> {
                self.0.lock().unwrap().push(action);
                Ok(())
            }
            fn subscribe(
                &self,
                _: Box<dyn Fn(bead_backends::Event) + Send + Sync>,
            ) -> bead_backends::Subscription {
                bead_backends::Subscription::new(|| {})
            }
        }
        let backend = Arc::new(RecordingBackend(Mutex::new(Vec::new())));
        // Start no desktop transports or commands; inject only the recording backend.
        let mut runtime = FolderServicesRuntime::start(FolderServices {
            commands: services.commands.clone(),
            ..Default::default()
        })
        .unwrap();
        let state = Arc::get_mut(&mut runtime).unwrap();
        state.services.desktop = services.desktop;
        state.compositor = Some(backend.clone());
        view.runtime = Some(runtime);
        let target = second
            .primitives
            .iter()
            .find_map(|p| match p {
                crate::render::Primitive::Text { text, rect, .. } if text == "Web" => Some(*rect),
                _ => None,
            })
            .unwrap();
        view.check_render(UiEvent::Click {
            x: target.x + target.w / 2.,
            y: target.y + target.h / 2.,
        })
        .unwrap();
        view.stage(UiEvent::Release).unwrap();
        view.commit();
        view.check_render(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
        assert_eq!(
            *backend.0.lock().unwrap(),
            vec![
                CompositorAction::SwitchWorkspace {
                    workspace: CompositorId("web".into()),
                };
                2
            ]
        );
    }

    #[test]
    fn folder_view_click_uses_the_drawn_counter_button_coordinates() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-window");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Window",
            (440, 300),
        )
        .unwrap();
        let scene = view.check_render(UiEvent::Reload).unwrap();
        let label = scene
            .primitives
            .iter()
            .find_map(|p| {
                if let crate::render::Primitive::Text { text, rect, .. } = p {
                    (text == "Button presses: 0").then_some(*rect)
                } else {
                    None
                }
            })
            .expect("counter button label");
        let button = scene
            .primitives
            .iter()
            .filter_map(|p| {
                if let crate::render::Primitive::Rounded { rect, .. } = p {
                    (label.x >= rect.x
                        && label.y >= rect.y
                        && label.x + label.w <= rect.x + rect.w
                        && label.y + label.h <= rect.y + rect.h)
                        .then_some(*rect)
                } else {
                    None
                }
            })
            .min_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            .expect("drawn counter button rectangle");
        let x = button.x + button.w / 2.;
        let y = button.y + button.h / 2.;
        let clicked = view.check_render(UiEvent::Click { x, y }).unwrap();
        let _ = view.check_render(UiEvent::Release).unwrap();
        assert!(clicked.primitives.iter().any(
            |p| matches!(p,crate::render::Primitive::Text{text,..} if text=="Button presses: 1")
        ));
        assert!(view.session.state_summary().contains("\"count\":1"));
    }

    #[test]
    fn panel_actions_need_declaration_and_emit_host_events_with_the_bar_centre() {
        use crate::wayland::SceneSource;
        let root = std::env::temp_dir().join(format!(
            "bead-folder-host-panels-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("view")).unwrap();
        std::fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"202020"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}",
        )
        .unwrap();
        std::fs::write(root.join("view/main.luau"), "local ui=require('bead.ui');return ui.Row{id='root',width='fill',height='fill',ui.Button{id='clock',text='12:34',width=80,height=30,on_activate=ui.action('panel.toggle','calendar')},ui.Button{id='close',text='x',width=30,height=30,on_activate=ui.action('panel.close')}}").unwrap();
        let load = || {
            FolderView::load(
                root.clone(),
                "view/main.luau".into(),
                "palette.json".into(),
                "Panels",
                (400, 36),
            )
            .unwrap()
        };
        assert!(load().check_render(UiEvent::Reload).is_err());

        let mut panel = load().with_panel_actions(None);
        assert!(!panel.viewport(1280, 720));
        let scene = panel.check_render(UiEvent::Reload).unwrap();
        assert_eq!((scene.bounds.w, scene.bounds.h), (400., 36.));
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut bar = load().with_panel_actions(Some(bead_core::Edge::Top));
        let sink = events.clone();
        bar.services(Box::new(move |event| sink.lock().unwrap().push(event)));
        bar.check_render(UiEvent::Reload).unwrap();
        for x in [40., 95.] {
            bar.stage(UiEvent::Click { x, y: 18. }).unwrap();
            bar.stage(UiEvent::Release).unwrap();
        }
        {
            let captured = events.lock().unwrap();
            assert!(
                matches!(&captured[0], UiEvent::CompanionSelect { view, output: None, toggle: true, center: Some(x) } if view == "calendar" && *x == 40.),
                "{captured:?}"
            );
            assert!(matches!(captured[1], UiEvent::CompanionVisibility(false)));
            assert_eq!(captured.len(), 2);
        }
        // Focus through the same keyboard path as a real panel, then activate its close button.
        bar.check_render(UiEvent::Command {
            command: "tab".into(),
            shift: false,
        })
        .unwrap();
        bar.stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
        assert_eq!(events.lock().unwrap().len(), 3);
        // A host-owned kit slider must route exact pointer previews, drags, and keyboard steps
        // without trying to find a behavior.luau handler for the host action.
        std::fs::write(root.join("view/main.luau"), "local kit=require('bead.kit');return kit.Slider{id='level',width=200,value=0,min=0,max=1,step=0.1,on_change='panel.open',change_arg=function(n) return tostring(n) end}").unwrap();
        std::fs::write(root.join("palette.json"), r#"{"formatVersion":1,"colours":{"surface":"202020","surfaceContainerHighest":"303030","primary":"ffffff","secondary":"cccccc"}}"#).unwrap();
        let mut slider = load().with_panel_actions(None);
        let sink = events.clone();
        slider.services(Box::new(move |event| sink.lock().unwrap().push(event)));
        slider.check_render(UiEvent::Reload).unwrap();
        events.lock().unwrap().clear();
        slider
            .check_render(UiEvent::Click { x: 55., y: 15. })
            .unwrap();
        slider
            .check_render(UiEvent::Drag { x: 151., y: 15. })
            .unwrap();
        slider.check_render(UiEvent::Release).unwrap();
        slider
            .check_render(UiEvent::Command {
                command: "right".into(),
                shift: false,
            })
            .unwrap();
        let levels: Vec<f64> = events
            .lock()
            .unwrap()
            .iter()
            .map(|event| {
                let UiEvent::CompanionSelect { view, .. } = event else {
                    panic!("unexpected {event:?}")
                };
                view.parse().unwrap()
            })
            .collect();
        assert_eq!(levels.len(), 3);
        assert!((levels[0] - 0.25).abs() < 0.01, "{levels:?}");
        assert!(levels[1] > 0.7 && levels[2] > levels[1], "{levels:?}");
        assert!(matches!(
            panel_event("panel.open", &Value::Text("v".into()), Some((3., 7.)), Some(bead_core::Edge::Left)),
            Ok(UiEvent::CompanionSelect { toggle: false, center: Some(y), .. }) if y == 7.
        ));
        assert!(matches!(
            panel_event("panel.open", &Value::Text("v".into()), Some((3., 7.)), None),
            Ok(UiEvent::CompanionSelect { center: None, .. })
        ));
        assert!(panel_event("panel.open", &Value::Bool(true), None, None).is_err());
        std::fs::write(root.join("view/main.luau"), "local ui=require('bead.ui');return ui.Button{id='local',text='Local',width=80,height=30,on_activate=ui.action('panel.close')}").unwrap();
        std::fs::write(root.join("view/behavior.luau"), "return {stores={count=0},actions={['panel.close']=function(s) s.count=s.count+1 end},computed={}}").unwrap();
        let mut local = load();
        local.check_render(UiEvent::Reload).unwrap();
        local
            .check_render(UiEvent::Action {
                name: "panel.close".into(),
                argument: Value::Bool(true),
            })
            .unwrap();
        assert!(local.session.state_summary().contains("\"count\":1"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn closed_panel_gets_no_service_wakeups_and_resubscribes_on_reopen() {
        use std::sync::mpsc;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-status");
        let runtime = FolderServicesRuntime::start(FolderServices {
            desktop: BTreeSet::from(["clock".into()]),
            commands: BTreeMap::new(),
        })
        .unwrap();
        let mut view = FolderView::load_with_runtime(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Status",
            (1280, 42),
            runtime.clone(),
        )
        .unwrap();
        let (tx, rx) = mpsc::channel();
        view.services(Box::new(move |event| {
            let _ = tx.send(event);
        }));
        let _ = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        view.stage(UiEvent::PanelVisibility(false)).unwrap();
        runtime.signals.notify();
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        view.stage(UiEvent::PanelVisibility(true)).unwrap();
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)),
            Ok(UiEvent::Model(_))
        ));
        runtime.signals.notify();
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_ok());
    }
}

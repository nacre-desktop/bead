// SPDX-License-Identifier: MIT
//! Event-driven client for the public NetworkManager system-bus API.
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap, HashSet, hash_map::Entry},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use zbus::{
    Connection,
    fdo::DBusProxy,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";
const SAVED: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const TRACKED: [&str; 6] = [NM, SETTINGS_IFACE, DEVICE, WIRELESS, AP, ACTIVE];
const SCAN_INTERVAL: Duration = Duration::from_secs(10);
const AGENT: &str = "secured network needs a password agent";
type Props = HashMap<String, OwnedValue>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkPrimary {
    /// `wifi`, `ethernet`, `vpn` or `other`.
    pub kind: &'static str,
    pub name: String,
    /// Interface name of the connection's first device.
    pub device: String,
    /// `activating`, `activated`, `deactivating`, `deactivated` or `unknown`.
    pub state: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkDevice {
    pub path: String,
    pub interface: String,
    /// `wifi`, `ethernet` or `other`.
    pub kind: &'static str,
    /// `unmanaged`, `unavailable`, `disconnected`, `connecting`, `connected`, `disconnecting`,
    /// `failed` or `unknown`.
    pub state: &'static str,
    /// Active connection path, empty when idle.
    pub active_connection: String,
}
/// One visible network: the strongest access point of an SSID across Wi-Fi devices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkAccessPoint {
    pub path: String,
    /// The Wi-Fi device that sees `path`.
    pub device: String,
    /// Lossy UTF-8 for display.
    pub ssid: String,
    /// Raw SSID used for matching and activation.
    pub ssid_bytes: Vec<u8>,
    pub strength: u8,
    /// `open` (no credentials needed), `wpa`, `wpa3`, `enterprise` or `wep`.
    pub security: &'static str,
    pub frequency_mhz: u32,
    /// Any access point with this raw SSID is active, including a weaker one.
    pub active: bool,
    /// A saved Wi-Fi connection exists for this SSID.
    pub known: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkConnection {
    pub path: String,
    pub id: String,
    /// `wifi`, `ethernet`, `vpn` or `other`.
    pub kind: &'static str,
    /// Raw SSID for Wi-Fi connections, empty otherwise.
    pub ssid: Vec<u8>,
}
/// Non-exhaustive: start from `NetworkSnapshot::default()` and assign fields.
/// ```compile_fail
/// let snapshot = bead_services::network::NetworkSnapshot { available: true, ..Default::default() };
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct NetworkSnapshot {
    pub revision: u64,
    pub available: bool,
    pub wireless_enabled: bool,
    pub wireless_hardware_enabled: bool,
    pub primary: Option<NetworkPrimary>,
    pub devices: Vec<NetworkDevice>,
    /// Deduplicated by SSID, strongest first.
    pub access_points: Vec<NetworkAccessPoint>,
    pub known_connections: Vec<NetworkConnection>,
}
#[derive(Clone, Debug)]
pub enum NetworkAction {
    SetWirelessEnabled(bool),
    /// Wi-Fi device path; one request per device every 10 seconds.
    Scan(String),
    /// Saved connection path, device path.
    ActivateKnown(String, String),
    /// Raw SSID of a visible open network, Wi-Fi device path. No secrets are sent.
    ActivateOpen(Vec<u8>, String),
    /// Device path; deactivates its active connection.
    Disconnect(String),
    /// Saved connection path to delete.
    Forget(String),
}
type Callback = Arc<dyn Fn(Arc<NetworkSnapshot>) + Send + Sync>;
struct Shared {
    snapshot: Mutex<Arc<NetworkSnapshot>>,
    callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    stopped: tokio::sync::Notify,
    scans: Mutex<HashMap<String, Instant>>,
}
impl Shared {
    fn publish(&self, mut next: NetworkSnapshot) {
        let mut old = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        next.revision = old.revision;
        if next == **old {
            return;
        }
        next.revision = old.revision.saturating_add(1);
        let next = Arc::new(next);
        *old = next.clone();
        drop(old);
        let callbacks = self
            .callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect::<Vec<_>>();
        callbacks.into_iter().for_each(|cb| cb(next.clone()));
    }
}
pub struct Subscription {
    shared: Weak<Shared>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(s) = self.shared.upgrade() {
            s.callbacks
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.id);
        }
    }
}
/// Owns a worker watching `org.freedesktop.NetworkManager` on the supplied system-bus connection.
pub struct Network {
    shared: Arc<Shared>,
    connection: Connection,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Network {
    /// Connect to the system bus and start, returning `None` if it is unavailable.
    #[doc(hidden)]
    pub fn start_system() -> Option<Arc<Self>> {
        let (runtime, connection) = crate::bus::connect_blocking(crate::bus::system_connection())?;
        Some(Self::spawn(Some(runtime), connection))
    }
    pub fn start(connection: Connection) -> Arc<Self> {
        Self::spawn(None, connection)
    }
    fn spawn(runtime: Option<tokio::runtime::Runtime>, connection: Connection) -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(NetworkSnapshot::default())),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stopped: tokio::sync::Notify::new(),
            scans: Mutex::new(HashMap::new()),
        });
        let conn = connection.clone();
        let worker = crate::bus::spawn_worker(
            runtime,
            shared.clone(),
            |s| &s.stopped,
            |s| s.publish(NetworkSnapshot::default()),
            move |s| run(conn.clone(), s),
        );
        Arc::new(Self {
            shared,
            connection,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn snapshot(&self) -> NetworkSnapshot {
        (**self
            .shared
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner()))
        .clone()
    }
    pub fn subscribe(
        &self,
        callback: impl Fn(Arc<NetworkSnapshot>) + Send + Sync + 'static,
    ) -> Subscription {
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let callback: Callback = Arc::new(callback);
        self.shared
            .callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, callback.clone());
        callback(
            self.shared
                .snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        );
        Subscription {
            shared: Arc::downgrade(&self.shared),
            id,
        }
    }
    /// Validate `action` against the current snapshot and send it; blocks until NM replies.
    pub fn action(&self, action: NetworkAction) -> Result<(), String> {
        self.action_at(action, Instant::now())
    }
    /// [`Self::action`] with an explicit clock for the scan rate limit.
    #[doc(hidden)]
    pub fn action_at(&self, action: NetworkAction, now: Instant) -> Result<(), String> {
        let s = self.snapshot();
        if !s.available {
            return Err("NetworkManager is unavailable".into());
        }
        let device = |path: &str| {
            s.devices
                .iter()
                .find(|d| d.path == path)
                .ok_or_else(|| format!("unknown network device '{path}'"))
        };
        let wifi = |path: &str| {
            device(path).and_then(|d| {
                if d.kind == "wifi" {
                    Ok(d)
                } else {
                    Err(format!("'{}' is not a Wi-Fi device", d.interface))
                }
            })
        };
        let saved = |path: &str| {
            s.known_connections
                .iter()
                .find(|k| k.path == path)
                .ok_or_else(|| format!("unknown saved connection '{path}'"))
        };
        let c = &self.connection;
        let rt = crate::bus::runtime().map_err(|e| e.to_string())?;
        match action {
            NetworkAction::SetWirelessEnabled(on) => rt.block_on(call(
                c,
                ROOT,
                PROPERTIES,
                "Set",
                &(NM, "WirelessEnabled", Value::from(on)),
            )),
            NetworkAction::Scan(path) => {
                wifi(&path)?;
                let mut scans = self.shared.scans.lock().unwrap_or_else(|e| e.into_inner());
                if scans
                    .get(&path)
                    .is_some_and(|last| now.saturating_duration_since(*last) < SCAN_INTERVAL)
                {
                    return Err("Wi-Fi scans are limited to one per 10 seconds per device".into());
                }
                scans.insert(path.clone(), now);
                drop(scans);
                let options = HashMap::<String, OwnedValue>::new();
                rt.block_on(call(c, &path, WIRELESS, "RequestScan", &(options,)))
            }
            NetworkAction::ActivateKnown(connection, path) => {
                saved(&connection)?;
                device(&path)?;
                let args = (object(&connection)?, object(&path)?, object("/")?);
                rt.block_on(call::<_, OwnedObjectPath>(
                    c,
                    ROOT,
                    NM,
                    "ActivateConnection",
                    &args,
                ))
                .map(drop)
            }
            NetworkAction::ActivateOpen(ssid, path) => {
                wifi(&path)?;
                let ap = s
                    .access_points
                    .iter()
                    .find(|ap| ap.ssid_bytes == ssid)
                    .ok_or("unknown Wi-Fi network")?;
                if ap.security != "open" {
                    return Err(AGENT.into());
                }
                // NetworkManager completes the profile from the access point (or, for another
                // device, from the SSID); no security section is sent.
                let specific = if ap.device == path {
                    ap.path.as_str()
                } else {
                    "/"
                };
                let settings = HashMap::from([
                    (
                        "connection",
                        HashMap::from([("type", Value::from("802-11-wireless"))]),
                    ),
                    (
                        "802-11-wireless",
                        HashMap::from([("ssid", Value::from(ssid))]),
                    ),
                ]);
                let args = (settings, object(&path)?, object(specific)?);
                rt.block_on(call::<_, (OwnedObjectPath, OwnedObjectPath)>(
                    c,
                    ROOT,
                    NM,
                    "AddAndActivateConnection",
                    &args,
                ))
                .map(drop)
            }
            NetworkAction::Disconnect(path) => {
                let d = device(&path)?;
                if d.active_connection.is_empty() {
                    return Err(format!("'{}' has no active connection", d.interface));
                }
                let args = (object(&d.active_connection)?,);
                rt.block_on(call(c, ROOT, NM, "DeactivateConnection", &args))
            }
            NetworkAction::Forget(connection) => {
                saved(&connection)?;
                rt.block_on(call(c, &connection, SAVED, "Delete", &()))
            }
        }
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        self.shared.stopped.notify_one();
        if let Some(w) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = w.join();
        }
    }
}

async fn run(c: Connection, s: Arc<Shared>) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&c).await?;
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, NM)])
        .await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(NM)?
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(rule, &c, Some(256)).await?;
    let mut owner = dbus
        .get_name_owner(NM.try_into()?)
        .await
        .ok()
        .map(|o| o.to_string());
    let mut cache = HashMap::new();
    if owner.is_some() {
        reconcile(&c, &mut cache).await;
    }
    loop {
        s.publish(build(&cache));
        tokio::select! {
            changed = owners.next() => {
                let changed = changed.ok_or(zbus::Error::Failure("owner stream ended".into()))?;
                owner = changed.args()?.new_owner().as_ref().map(|o| o.to_string()).filter(|o| !o.is_empty());
                cache.clear();
            },
            message = signals.next() => {
                let message = message.ok_or(zbus::Error::Failure("signal stream ended".into()))??;
                let header = message.header();
                if owner.is_none() || header.sender().map(|v| v.as_str()) != owner.as_deref() {
                    continue;
                }
                let path = header.path().map(|p| p.to_string()).unwrap_or_default();
                match header.member().map(|m| m.as_str()) {
                    Some("PropertiesChanged") => {
                        let Ok((iface, changed, invalidated)) = message.body().deserialize::<(String, Props, Vec<String>)>() else {
                            continue;
                        };
                        if !TRACKED.contains(&iface.as_str()) {
                            continue;
                        }
                        if invalidated.is_empty() && let Some(props) = cache.get_mut(&path) {
                            props.extend(changed);
                        } else {
                            cache.remove(&path);
                        }
                    }
                    // Refetch the emitter so list changes are seen even before PropertiesChanged.
                    Some("DeviceAdded" | "DeviceRemoved" | "AccessPointAdded" | "AccessPointRemoved"
                        | "NewConnection" | "ConnectionRemoved" | "Updated" | "Removed") => {
                        cache.remove(&path);
                    }
                    _ => continue,
                }
            },
        }
        if owner.is_some() {
            reconcile(&c, &mut cache).await;
        }
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Root,
    Settings,
    Device,
    AccessPoint,
    Active,
    Saved,
}
/// Fetch referenced objects missing from the cache and drop unreferenced ones.
async fn reconcile(c: &Connection, cache: &mut HashMap<String, Props>) {
    let mut queue = vec![
        (ROOT.to_owned(), Kind::Root),
        (SETTINGS.to_owned(), Kind::Settings),
    ];
    let mut seen = HashSet::new();
    while let Some((path, kind)) = queue.pop() {
        if path == "/" || !seen.insert(path.clone()) {
            continue;
        }
        if !cache.contains_key(&path) {
            let Some(props) = fetch(c, &path, kind).await else {
                continue;
            };
            cache.insert(path.clone(), props);
        }
        let children: &[(&str, Kind)] = match kind {
            Kind::Root => &[
                ("Devices", Kind::Device),
                ("ActiveConnections", Kind::Active),
            ],
            Kind::Settings => &[("Connections", Kind::Saved)],
            Kind::Device => &[("AccessPoints", Kind::AccessPoint)],
            _ => &[],
        };
        for (key, child) in children {
            queue.extend(paths(&cache[&path], key).into_iter().map(|p| (p, *child)));
        }
    }
    cache.retain(|path, _| seen.contains(path));
}
async fn fetch(c: &Connection, path: &str, kind: Kind) -> Option<Props> {
    let iface = match kind {
        Kind::Root => NM,
        Kind::Settings => SETTINGS_IFACE,
        Kind::Device => DEVICE,
        Kind::AccessPoint => AP,
        Kind::Active => ACTIVE,
        Kind::Saved => {
            // GetSettings never returns secrets; keep only what the snapshot needs.
            let mut settings: HashMap<String, Props> =
                call(c, path, SAVED, "GetSettings", &()).await.ok()?;
            let mut take = |group: &str, key: &str| {
                Some((key.to_owned(), settings.get_mut(group)?.remove(key)?))
            };
            return Some(
                [
                    take("connection", "id"),
                    take("connection", "type"),
                    take("802-11-wireless", "ssid"),
                ]
                .into_iter()
                .flatten()
                .collect(),
            );
        }
    };
    let mut props: Props = call(c, path, PROPERTIES, "GetAll", &(iface,)).await.ok()?;
    if matches!(kind, Kind::Device) && get::<u32>(&props, "DeviceType") == Some(2) {
        props.extend(
            call::<_, Props>(c, path, PROPERTIES, "GetAll", &(WIRELESS,))
                .await
                .ok()?,
        );
    }
    Some(props)
}
fn build(cache: &HashMap<String, Props>) -> NetworkSnapshot {
    let Some(root) = cache.get(ROOT) else {
        return NetworkSnapshot::default();
    };
    let devices: Vec<NetworkDevice> = paths(root, "Devices")
        .into_iter()
        .filter_map(|path| {
            let p = cache.get(&path)?;
            Some(NetworkDevice {
                interface: text(p, "Interface"),
                kind: device_kind(get::<u32>(p, "DeviceType").unwrap_or(0)),
                state: device_state(get::<u32>(p, "State").unwrap_or(0)),
                active_connection: object_at(p, "ActiveConnection"),
                path,
            })
        })
        .collect();
    let known_connections: Vec<NetworkConnection> = cache
        .get(SETTINGS)
        .map_or_else(Vec::new, |s| paths(s, "Connections"))
        .into_iter()
        .filter_map(|path| {
            let p = cache.get(&path)?;
            Some(NetworkConnection {
                id: text(p, "id"),
                kind: connection_kind(&text(p, "type")),
                ssid: bytes(p, "ssid"),
                path,
            })
        })
        .collect();
    let mut networks: HashMap<Vec<u8>, NetworkAccessPoint> = HashMap::new();
    for device in devices.iter().filter(|d| d.kind == "wifi") {
        let props = &cache[&device.path];
        let active = object_at(props, "ActiveAccessPoint");
        for path in paths(props, "AccessPoints") {
            let Some(p) = cache.get(&path) else {
                continue;
            };
            let ssid = bytes(p, "Ssid");
            if ssid.is_empty() {
                continue; // hidden networks have no name to show or match
            }
            let keys =
                get::<u32>(p, "WpaFlags").unwrap_or(0) | get::<u32>(p, "RsnFlags").unwrap_or(0);
            let ap = NetworkAccessPoint {
                active: path == active,
                known: known_connections
                    .iter()
                    .any(|k| k.kind == "wifi" && k.ssid == ssid),
                ssid: String::from_utf8_lossy(&ssid).into_owned(),
                strength: get::<u8>(p, "Strength").unwrap_or(0),
                security: security(get::<u32>(p, "Flags").unwrap_or(0), keys),
                frequency_mhz: get::<u32>(p, "Frequency").unwrap_or(0),
                device: device.path.clone(),
                ssid_bytes: ssid,
                path,
            };
            match networks.entry(ap.ssid_bytes.clone()) {
                Entry::Vacant(e) => {
                    e.insert(ap);
                }
                Entry::Occupied(mut e) => {
                    let active = ap.active || e.get().active;
                    if ap.strength > e.get().strength {
                        e.insert(ap);
                    }
                    e.get_mut().active = active;
                }
            }
        }
    }
    let mut access_points: Vec<_> = networks.into_values().collect();
    access_points.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then_with(|| a.ssid_bytes.cmp(&b.ssid_bytes))
    });
    let primary = cache
        .get(&object_at(root, "PrimaryConnection"))
        .map(|p| NetworkPrimary {
            kind: connection_kind(&text(p, "Type")),
            name: text(p, "Id"),
            device: paths(p, "Devices")
                .first()
                .and_then(|path| devices.iter().find(|d| &d.path == path))
                .map(|d| d.interface.clone())
                .unwrap_or_default(),
            state: active_state(get::<u32>(p, "State").unwrap_or(0)),
        });
    NetworkSnapshot {
        revision: 0,
        available: true,
        wireless_enabled: get(root, "WirelessEnabled").unwrap_or(false),
        wireless_hardware_enabled: get(root, "WirelessHardwareEnabled").unwrap_or(false),
        primary,
        devices,
        access_points,
        known_connections,
    }
}
fn device_kind(kind: u32) -> &'static str {
    match kind {
        1 => "ethernet",
        2 => "wifi",
        _ => "other",
    }
}
fn device_state(state: u32) -> &'static str {
    match state {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40..=90 => "connecting",
        100 => "connected",
        110 => "disconnecting",
        120 => "failed",
        _ => "unknown",
    }
}
fn active_state(state: u32) -> &'static str {
    match state {
        1 => "activating",
        2 => "activated",
        3 => "deactivating",
        4 => "deactivated",
        _ => "unknown",
    }
}
fn connection_kind(kind: &str) -> &'static str {
    match kind {
        "802-11-wireless" => "wifi",
        "802-3-ethernet" => "ethernet",
        "vpn" | "wireguard" => "vpn",
        _ => "other",
    }
}
/// Classify NM 802.11 AP flags (`Flags`) and combined WPA/RSN key-management flags.
fn security(flags: u32, keys: u32) -> &'static str {
    if keys & (0x200 | 0x2000) != 0 {
        "enterprise" // 802.1X or EAP Suite-B
    } else if keys & 0x400 != 0 {
        "wpa3" // SAE
    } else if keys & 0x100 != 0 {
        "wpa" // PSK
    } else if keys & (0x800 | 0x1000) != 0 || flags & 0x1 == 0 {
        "open" // no privacy, or OWE which needs no credentials
    } else {
        "wep" // privacy without WPA/RSN
    }
}
async fn call<B, R>(
    c: &Connection,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> Result<R, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    let reply = c
        .call_method(Some(NM), path, Some(iface), method, body)
        .await
        .map_err(|e| e.to_string())?;
    reply.body().deserialize().map_err(|e| e.to_string())
}
fn object(path: &str) -> Result<ObjectPath<'_>, String> {
    ObjectPath::try_from(path).map_err(|e| e.to_string())
}
fn get<'a, T: TryFrom<&'a OwnedValue>>(props: &'a Props, key: &str) -> Option<T> {
    props.get(key).and_then(|v| T::try_from(v).ok())
}
fn text(props: &Props, key: &str) -> String {
    get::<&str>(props, key).unwrap_or_default().to_owned()
}
/// An object-path property, empty for NM's `/` "none" value.
fn object_at(props: &Props, key: &str) -> String {
    get::<&ObjectPath>(props, key)
        .map(|p| p.as_str())
        .filter(|p| *p != "/")
        .unwrap_or_default()
        .to_owned()
}
fn paths(props: &Props, key: &str) -> Vec<String> {
    props
        .get(key)
        .and_then(|v| Vec::<OwnedObjectPath>::try_from(v.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.to_string())
        .collect()
}
fn bytes(props: &Props, key: &str) -> Vec<u8> {
    props
        .get(key)
        .and_then(|v| Vec::<u8>::try_from(v.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn access_point_security_classification() {
        assert_eq!(super::security(0, 0), "open");
        assert_eq!(super::security(1, 0x800), "open");
        assert_eq!(super::security(1, 0), "wep");
        assert_eq!(super::security(1, 0x188), "wpa");
        assert_eq!(super::security(1, 0x500), "wpa3");
        assert_eq!(super::security(1, 0x300), "enterprise");
    }
}

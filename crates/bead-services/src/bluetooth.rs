// SPDX-License-Identifier: MIT
//! Event-driven client for the public BlueZ system-bus API.
use futures_util::StreamExt;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    fdo::DBusProxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};

const BLUEZ: &str = "org.bluez";
const ROOT: &str = "/";
const OM: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";
type Properties = HashMap<String, OwnedValue>;
type Interfaces = HashMap<String, Properties>;
type ManagedObjects = HashMap<String, Interfaces>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BluetoothAdapter {
    pub path: String,
    pub name: String,
    pub powered: bool,
    pub discovering: bool,
    pub pairable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub path: String,
    pub adapter: String,
    pub address: String,
    pub name: String,
    pub icon: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BluetoothSnapshot {
    pub revision: u64,
    pub available: bool,
    pub adapters: Vec<BluetoothAdapter>,
    pub devices: Vec<BluetoothDevice>,
}
#[derive(Clone, Debug)]
pub enum BluetoothAction {
    SetPowered(String, bool),
    StartDiscovery(String),
    StopDiscovery(String),
    Connect(String),
    Disconnect(String),
    Pair(String),
    SetTrusted(String, bool),
    Remove(String),
}
type Callback = Arc<dyn Fn(Arc<BluetoothSnapshot>) + Send + Sync>;
struct Shared {
    snapshot: Mutex<Arc<BluetoothSnapshot>>,
    objects: Mutex<ManagedObjects>,
    callbacks: Mutex<BTreeMap<u64, Callback>>,
    next: AtomicU64,
    stopped: tokio::sync::Notify,
    scan_timeout: Duration,
    deadlines: Mutex<HashMap<String, tokio::time::Instant>>,
    deadline_changed: tokio::sync::Notify,
}
impl Shared {
    fn replace(&self, available: bool, objects: ManagedObjects) {
        *self.objects.lock().unwrap_or_else(|e| e.into_inner()) = objects.clone();
        let mut adapters = Vec::new();
        let mut devices = Vec::new();
        for (path, interfaces) in objects {
            if let Some(props) = interfaces.get(ADAPTER) {
                adapters.push(BluetoothAdapter {
                    path: path.clone(),
                    name: string(props, "Alias")
                        .or_else(|| string(props, "Name"))
                        .unwrap_or_default(),
                    powered: boolean(props, "Powered"),
                    discovering: boolean(props, "Discovering"),
                    pairable: boolean(props, "Pairable"),
                });
            }
            if let Some(props) = interfaces.get(DEVICE) {
                let battery = interfaces
                    .get(BATTERY)
                    .and_then(|p| number_u8(p, "Percentage"));
                devices.push(BluetoothDevice {
                    path,
                    adapter: string(props, "Adapter").unwrap_or_default(),
                    address: string(props, "Address").unwrap_or_default(),
                    name: string(props, "Alias")
                        .or_else(|| string(props, "Name"))
                        .unwrap_or_default(),
                    icon: string(props, "Icon").unwrap_or_default(),
                    paired: boolean(props, "Paired"),
                    trusted: boolean(props, "Trusted"),
                    connected: boolean(props, "Connected"),
                    battery,
                    rssi: number_i16(props, "RSSI"),
                });
            }
        }
        adapters.sort_by(|a, b| a.path.cmp(&b.path));
        devices.sort_by(|a, b| a.path.cmp(&b.path));
        let mut old = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = BluetoothSnapshot {
            revision: old.revision,
            available,
            adapters,
            devices,
        };
        if next.available == old.available
            && next.adapters == old.adapters
            && next.devices == old.devices
        {
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
/// Owns a worker watching `org.bluez` on the supplied system-bus connection.
pub struct Bluetooth {
    shared: Arc<Shared>,
    connection: Connection,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Bluetooth {
    /// Connect to the system bus and start, returning `None` if it is unavailable.
    #[doc(hidden)]
    pub fn start_system() -> Option<Arc<Self>> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()
            .and_then(|rt| rt.block_on(system_connection()).ok())
            .map(Self::start)
    }
    pub fn start(connection: Connection) -> Arc<Self> {
        Self::start_with_timeout(connection, Duration::from_secs(30))
    }
    #[doc(hidden)]
    pub fn start_with_timeout(connection: Connection, scan_timeout: Duration) -> Arc<Self> {
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Arc::new(BluetoothSnapshot::default())),
            objects: Mutex::new(HashMap::new()),
            callbacks: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            stopped: tokio::sync::Notify::new(),
            scan_timeout,
            deadlines: Mutex::new(HashMap::new()),
            deadline_changed: tokio::sync::Notify::new(),
        });
        let state = shared.clone();
        let conn = connection.clone();
        let worker = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            while rt.block_on(run(conn.clone(), state.clone())).is_err() {
                state.replace(false, HashMap::new());
                std::thread::sleep(Duration::from_millis(200));
            }
        });
        Arc::new(Self {
            shared,
            connection,
            worker: Mutex::new(Some(worker)),
        })
    }
    pub fn snapshot(&self) -> BluetoothSnapshot {
        (**self
            .shared
            .snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner()))
        .clone()
    }
    pub fn subscribe(
        &self,
        callback: impl Fn(Arc<BluetoothSnapshot>) + Send + Sync + 'static,
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
    pub fn action(&self, action: BluetoothAction) -> Result<(), String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(action_async(&self.connection, &self.shared, action))
    }
}
impl Drop for Bluetooth {
    fn drop(&mut self) {
        self.shared.stopped.notify_one();
        if let Some(w) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = w.join();
        }
    }
}

async fn system_connection() -> zbus::Result<Connection> {
    if let Ok(address) = std::env::var("DBUS_SYSTEM_BUS_ADDRESS") {
        zbus::connection::Builder::address(address.as_str())?
            .build()
            .await
    } else {
        Connection::system().await
    }
}
async fn run(c: Connection, s: Arc<Shared>) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&c).await?;
    let mut owners = dbus.receive_name_owner_changed().await?;
    let mut signals = zbus::MessageStream::for_match_rule(
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .build(),
        &c,
        Some(128),
    )
    .await?;
    let mut owner = dbus
        .get_name_owner(BLUEZ.try_into().expect("valid name"))
        .await
        .ok()
        .map(|owner| owner.to_string());
    if owner.is_some() {
        refresh(&c, &s).await;
    } else {
        s.replace(false, HashMap::new());
    }
    loop {
        tokio::select! {
            _=s.stopped.notified()=>return Ok(()),
            changed=owners.next()=>{let changed=changed.ok_or(zbus::Error::Failure("owner stream ended".into()))?;let args=changed.args()?;if args.name().as_str()==BLUEZ {owner=args.new_owner().as_ref().filter(|v|!v.as_str().is_empty()).map(|v|v.to_string());s.deadlines.lock().unwrap_or_else(|e|e.into_inner()).clear();if owner.is_some(){refresh(&c,&s).await}else{s.replace(false,HashMap::new());}}},
            message=signals.next()=>{if let Some(Ok(m))=message{let h=m.header();let sender=h.sender().map(|v|v.to_string());if owner==sender{let iface=h.interface().map(|v|v.as_str());if (iface==Some("org.freedesktop.DBus.Properties")&&h.member().is_some_and(|v|v.as_str()=="PropertiesChanged"))||(iface==Some(OM)&&h.member().is_some_and(|v|matches!(v.as_str(),"InterfacesAdded"|"InterfacesRemoved"))){refresh(&c,&s).await;}}}},
            _=async {tokio::select!{_=s.deadline_changed.notified()=>{},_=async {let deadline=s.deadlines.lock().unwrap_or_else(|e|e.into_inner()).values().min().copied();if let Some(deadline)=deadline{tokio::time::sleep_until(deadline).await}else{std::future::pending::<()>().await}}=>{}}}=>{let now=tokio::time::Instant::now();let expired={let mut deadlines=s.deadlines.lock().unwrap_or_else(|e|e.into_inner());let expired=deadlines.iter().filter(|(_,d)|**d<=now).map(|(p,_)|p.clone()).collect::<Vec<_>>();for p in &expired{deadlines.remove(p);}expired};for p in expired{let _=call(&c,&p,ADAPTER,"StopDiscovery",&()).await;}refresh(&c,&s).await;},
        }
    }
}
async fn refresh(c: &Connection, s: &Shared) {
    let Ok(proxy) = Proxy::new(c, BLUEZ, ROOT, OM).await else {
        s.replace(false, HashMap::new());
        return;
    };
    let objects: Result<HashMap<OwnedObjectPath, Interfaces>, _> =
        proxy.call("GetManagedObjects", &()).await;
    match objects {
        Ok(objects) => s.replace(
            true,
            objects
                .into_iter()
                .map(|(p, i)| (p.to_string(), i))
                .collect(),
        ),
        Err(_) => s.replace(true, HashMap::new()),
    }
}
async fn call<A>(
    c: &Connection,
    path: &str,
    iface: &str,
    method: &str,
    args: &A,
) -> Result<(), String>
where
    A: serde::Serialize + zbus::zvariant::Type,
{
    Proxy::new(c, BLUEZ, path, iface)
        .await
        .map_err(|e| e.to_string())?
        .call::<_, _, ()>(method, args)
        .await
        .map_err(|e| e.to_string())
}
async fn action_async(c: &Connection, s: &Shared, a: BluetoothAction) -> Result<(), String> {
    let objects = s.objects.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match a {
        BluetoothAction::SetPowered(path, value) => {
            validate(&objects, &path, ADAPTER)?;
            let p = Proxy::new(c, BLUEZ, path.as_str(), ADAPTER)
                .await
                .map_err(|e| e.to_string())?;
            p.set_property("Powered", value)
                .await
                .map_err(|e| e.to_string())?;
        }
        BluetoothAction::StartDiscovery(path) => {
            validate(&objects, &path, ADAPTER)?;
            call(c, &path, ADAPTER, "StartDiscovery", &()).await?;
            s.deadlines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(path, tokio::time::Instant::now() + s.scan_timeout);
            s.deadline_changed.notify_one();
        }
        BluetoothAction::StopDiscovery(path) => {
            validate(&objects, &path, ADAPTER)?;
            call(c, &path, ADAPTER, "StopDiscovery", &()).await?;
            s.deadlines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&path);
            s.deadline_changed.notify_one();
        }
        BluetoothAction::Connect(path) => {
            validate(&objects, &path, DEVICE)?;
            call(c, &path, DEVICE, "Connect", &()).await?;
        }
        BluetoothAction::Disconnect(path) => {
            validate(&objects, &path, DEVICE)?;
            call(c, &path, DEVICE, "Disconnect", &()).await?;
        }
        BluetoothAction::Pair(path) => {
            validate(&objects, &path, DEVICE)?;
            call(c, &path, DEVICE, "Pair", &()).await.map_err(|e| {
                if e.contains("Authentication") || e.contains("Canceled") || e.contains("Rejected")
                {
                    "pairing requires a Bluetooth agent for PIN, passkey, or confirmation"
                        .to_owned()
                } else {
                    e
                }
            })?;
        }
        BluetoothAction::SetTrusted(path, value) => {
            validate(&objects, &path, DEVICE)?;
            let p = Proxy::new(c, BLUEZ, path.as_str(), DEVICE)
                .await
                .map_err(|e| e.to_string())?;
            p.set_property("Trusted", value)
                .await
                .map_err(|e| e.to_string())?;
        }
        BluetoothAction::Remove(path) => {
            let interfaces = objects.get(&path).ok_or("device path is unavailable")?;
            let props = interfaces.get(DEVICE).ok_or("device path is unavailable")?;
            let adapter = string(props, "Adapter").ok_or("device has no adapter")?;
            validate(&objects, &adapter, ADAPTER)?;
            let device: OwnedObjectPath = path
                .try_into()
                .map_err(|e: zbus::zvariant::Error| e.to_string())?;
            call(c, &adapter, ADAPTER, "RemoveDevice", &(device,)).await?;
        }
    }
    Ok(())
}
fn validate(objects: &ManagedObjects, path: &str, iface: &str) -> Result<(), String> {
    if objects.get(path).is_some_and(|i| i.contains_key(iface)) {
        Ok(())
    } else {
        Err(format!("unknown Bluetooth {iface} path '{path}'"))
    }
}
fn value<'a>(m: &'a HashMap<String, OwnedValue>, key: &str) -> Option<&'a OwnedValue> {
    m.get(key)
}
fn string(m: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let v = value(m, key)?;
    if let Ok(v) = <&str>::try_from(v) {
        Some(v.to_owned())
    } else {
        OwnedObjectPath::try_from(v.clone())
            .ok()
            .map(|p| p.to_string())
    }
}
fn boolean(m: &HashMap<String, OwnedValue>, key: &str) -> bool {
    value(m, key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn number_u8(m: &HashMap<String, OwnedValue>, key: &str) -> Option<u8> {
    value(m, key).and_then(|v| u8::try_from(v).ok())
}
fn number_i16(m: &HashMap<String, OwnedValue>, key: &str) -> Option<i16> {
    value(m, key).and_then(|v| i16::try_from(v).ok())
}

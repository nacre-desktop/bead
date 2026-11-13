// SPDX-License-Identifier: MIT
//! BlueZ ObjectManager, property notifications, owner lifecycle, and controls on a private bus.
use bead_services::bluetooth::{Bluetooth, BluetoothAction};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const NAME: &str = "org.bluez";
const ADAPTER_PATH: &str = "/org/bluez/hci0";
const DEVICE_PATH: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
type Props = HashMap<String, OwnedValue>;
type Managed = HashMap<OwnedObjectPath, HashMap<String, Props>>;
#[derive(Default)]
struct State {
    powered: bool,
    discovering: bool,
    connected: bool,
    trusted: bool,
    calls: Vec<String>,
    objects: Managed,
}
fn owned(v: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(v).unwrap()
}
fn props(values: Vec<(&str, Value<'_>)>) -> Props {
    values
        .into_iter()
        .map(|(k, v)| (k.into(), owned(v)))
        .collect()
}
fn managed(s: &State) -> Managed {
    s.objects.clone()
}
struct Manager(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Manager {
    fn get_managed_objects(&self) -> Managed {
        managed(&self.0.lock().unwrap())
    }
    #[zbus(signal)]
    async fn interfaces_added(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        path: OwnedObjectPath,
        interfaces: HashMap<String, Props>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn interfaces_removed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        path: OwnedObjectPath,
        interfaces: Vec<String>,
    ) -> zbus::Result<()>;
}
struct Adapter(Arc<Mutex<State>>);
#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn alias(&self) -> String {
        "Test Adapter".into()
    }
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.0.lock().unwrap().powered
    }
    #[zbus(property)]
    fn set_powered(&self, v: bool) {
        let mut s = self.0.lock().unwrap();
        s.powered = v;
        s.calls.push(format!("Powered:{v}"));
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(ADAPTER_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Adapter1"))
        {
            p.insert("Powered".into(), owned(Value::from(v)));
        }
    }
    #[zbus(property)]
    fn discovering(&self) -> bool {
        self.0.lock().unwrap().discovering
    }
    #[zbus(property)]
    fn set_discovering(&self, v: bool) {
        let mut s = self.0.lock().unwrap();
        s.discovering = v;
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(ADAPTER_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Adapter1"))
        {
            p.insert("Discovering".into(), owned(Value::from(v)));
        }
    }
    #[zbus(property)]
    fn pairable(&self) -> bool {
        true
    }
    fn start_discovery(&self) {
        let mut s = self.0.lock().unwrap();
        s.discovering = true;
        s.calls.push("StartDiscovery".into());
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(ADAPTER_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Adapter1"))
        {
            p.insert("Discovering".into(), owned(Value::from(true)));
        }
    }
    fn stop_discovery(&self) {
        let mut s = self.0.lock().unwrap();
        s.discovering = false;
        s.calls.push("StopDiscovery".into());
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(ADAPTER_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Adapter1"))
        {
            p.insert("Discovering".into(), owned(Value::from(false)));
        }
    }
    fn remove_device(&self, path: OwnedObjectPath) {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("RemoveDevice:{path}"));
        s.objects.remove(&path);
    }
}
struct Device(Arc<Mutex<State>>);
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(ADAPTER_PATH).unwrap()
    }
    #[zbus(property)]
    fn address(&self) -> String {
        "AA:BB:CC:DD:EE:FF".into()
    }
    #[zbus(property)]
    fn alias(&self) -> String {
        "Headphones".into()
    }
    #[zbus(property)]
    fn icon(&self) -> String {
        "audio-headphones".into()
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn trusted(&self) -> bool {
        self.0.lock().unwrap().trusted
    }
    #[zbus(property)]
    fn set_trusted(&self, v: bool) {
        self.0.lock().unwrap().trusted = v;
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.lock().unwrap().connected
    }
    #[zbus(property)]
    fn set_connected(&self, v: bool) {
        let mut s = self.0.lock().unwrap();
        s.connected = v;
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(DEVICE_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Device1"))
        {
            p.insert("Connected".into(), owned(Value::from(v)));
        }
    }
    fn connect(&self) {
        let mut s = self.0.lock().unwrap();
        s.connected = true;
        s.calls.push("Connect".into());
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(DEVICE_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Device1"))
        {
            p.insert("Connected".into(), owned(Value::from(true)));
        }
    }
    fn disconnect(&self) {
        let mut s = self.0.lock().unwrap();
        s.connected = false;
        s.calls.push("Disconnect".into());
        if let Some(p) = s
            .objects
            .get_mut(&OwnedObjectPath::try_from(DEVICE_PATH).unwrap())
            .and_then(|i| i.get_mut("org.bluez.Device1"))
        {
            p.insert("Connected".into(), owned(Value::from(false)));
        }
    }
    fn pair(&self) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::Failed(
            "org.bluez.Error.AuthenticationRejected".into(),
        ))
    }
}
struct Battery;
#[zbus::interface(name = "org.bluez.Battery1")]
impl Battery {
    #[zbus(property)]
    fn percentage(&self) -> u8 {
        73
    }
}
async fn fixture() -> (Connection, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State {
        powered: false,
        discovering: false,
        connected: false,
        trusted: false,
        calls: vec![],
        objects: Managed::new(),
    }));
    {
        let mut s = state.lock().unwrap();
        s.objects.insert(
            OwnedObjectPath::try_from(ADAPTER_PATH).unwrap(),
            HashMap::from([(
                "org.bluez.Adapter1".into(),
                props(vec![
                    ("Alias", Value::from("Test Adapter")),
                    ("Powered", Value::from(false)),
                    ("Discovering", Value::from(false)),
                    ("Pairable", Value::from(true)),
                ]),
            )]),
        );
        s.objects.insert(
            OwnedObjectPath::try_from(DEVICE_PATH).unwrap(),
            HashMap::from([
                (
                    "org.bluez.Device1".into(),
                    props(vec![
                        (
                            "Adapter",
                            Value::from(OwnedObjectPath::try_from(ADAPTER_PATH).unwrap()),
                        ),
                        ("Address", Value::from("AA:BB:CC:DD:EE:FF")),
                        ("Alias", Value::from("Headphones")),
                        ("Icon", Value::from("audio-headphones")),
                        ("Paired", Value::from(true)),
                        ("Trusted", Value::from(false)),
                        ("Connected", Value::from(false)),
                        ("RSSI", Value::from(-42_i16)),
                    ]),
                ),
                (
                    "org.bluez.Battery1".into(),
                    props(vec![("Percentage", Value::from(73_u8))]),
                ),
            ]),
        );
    }
    let c = Connection::session().await.unwrap();
    c.request_name(NAME).await.unwrap();
    c.object_server()
        .at("/", Manager(state.clone()))
        .await
        .unwrap();
    c.object_server()
        .at(ADAPTER_PATH, Adapter(state.clone()))
        .await
        .unwrap();
    c.object_server()
        .at(DEVICE_PATH, Device(state.clone()))
        .await
        .unwrap();
    c.object_server().at(DEVICE_PATH, Battery).await.unwrap();
    (c, state)
}
async fn wait(s: &Bluetooth, p: impl Fn(&bead_services::bluetooth::BluetoothSnapshot) -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if p(&s.snapshot()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Bluetooth state timeout");
}
async fn perform(service: &Arc<Bluetooth>, action: BluetoothAction) -> Result<(), String> {
    // Bluetooth::action owns a blocking runtime, so keep it off Tokio's executor.
    let service = Arc::clone(service);
    tokio::task::spawn_blocking(move || service.action(action))
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn bluetooth_objects_signals_actions_and_owner_loss() {
    assert!(std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS").is_some());
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    let (owner, state) = fixture().await;
    let client = Connection::session().await.unwrap();
    let service = Bluetooth::start_with_timeout(client, Duration::from_millis(200));
    wait(&service, |s| s.available && s.devices.len() == 1).await;
    let snap = service.snapshot();
    assert_eq!(snap.adapters[0].name, "Test Adapter");
    assert_eq!(snap.devices[0].name, "Headphones");
    assert_eq!(snap.devices[0].battery, Some(73));
    assert_eq!(snap.devices[0].rssi, Some(-42));
    assert!(
        perform(&service, BluetoothAction::Connect("/invalid".into()))
            .await
            .is_err()
    );
    assert!(
        perform(&service, BluetoothAction::Connect(DEVICE_PATH.into()))
            .await
            .is_ok()
    );
    assert!(
        perform(&service, BluetoothAction::Disconnect(DEVICE_PATH.into()))
            .await
            .is_ok()
    );
    perform(
        &service,
        BluetoothAction::SetPowered(ADAPTER_PATH.into(), true),
    )
    .await
    .unwrap();
    wait(&service, |s| s.adapters.first().is_some_and(|a| a.powered)).await;
    perform(
        &service,
        BluetoothAction::SetTrusted(DEVICE_PATH.into(), true),
    )
    .await
    .unwrap();
    assert!(
        perform(&service, BluetoothAction::Pair(DEVICE_PATH.into()))
            .await
            .unwrap_err()
            .contains("agent")
    );
    assert!(
        perform(
            &service,
            BluetoothAction::StartDiscovery(ADAPTER_PATH.into())
        )
        .await
        .is_ok()
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if state
                .lock()
                .unwrap()
                .calls
                .iter()
                .any(|call| call == "StopDiscovery")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("bounded discovery did not stop");
    let adapter = Proxy::new(&owner, NAME, ADAPTER_PATH, "org.bluez.Adapter1")
        .await
        .unwrap();
    adapter.set_property("Discovering", true).await.unwrap();
    wait(&service, |s| {
        s.adapters.first().is_some_and(|a| a.discovering)
    })
    .await;
    let device = Proxy::new(&owner, NAME, DEVICE_PATH, "org.bluez.Device1")
        .await
        .unwrap();
    device.set_property("Connected", true).await.unwrap();
    wait(&service, |s| s.devices.first().is_some_and(|d| d.connected)).await;
    adapter.set_property("Discovering", false).await.unwrap();
    wait(&service, |s| {
        s.adapters.first().is_some_and(|a| !a.discovering)
    })
    .await;
    let emitter = zbus::object_server::SignalEmitter::new(&owner, "/").unwrap();
    let path = OwnedObjectPath::try_from(DEVICE_PATH).unwrap();
    let interfaces = state.lock().unwrap().objects.get(&path).unwrap().clone();
    state.lock().unwrap().objects.remove(&path);
    Manager::interfaces_removed(
        &emitter,
        path.clone(),
        vec!["org.bluez.Device1".into(), "org.bluez.Battery1".into()],
    )
    .await
    .unwrap();
    wait(&service, |s| s.devices.is_empty()).await;
    state
        .lock()
        .unwrap()
        .objects
        .insert(path.clone(), interfaces.clone());
    Manager::interfaces_added(&emitter, path.clone(), interfaces)
        .await
        .unwrap();
    wait(&service, |s| s.devices.len() == 1).await;
    assert!(
        perform(&service, BluetoothAction::Remove(DEVICE_PATH.into()))
            .await
            .is_ok()
    );
    assert!(
        state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|c| c == &format!("RemoveDevice:{DEVICE_PATH}"))
    );
    let calls = state.lock().unwrap().calls.clone();
    assert!(calls.iter().any(|c| c == "Connect"));
    assert!(calls.iter().any(|c| c == "Disconnect"));
    assert!(calls.iter().any(|c| c == "Powered:true"));
    assert!(calls.iter().any(|c| c == "StartDiscovery"));
    assert!(calls.iter().any(|c| c == "StopDiscovery"));
    drop(adapter);
    drop(device);
    drop(emitter);
    drop(owner);
    wait(&service, |s| !s.available).await;
    // The production constructor: its connection must outlive the runtime that built it, and
    // dropping the service must stop the worker promptly.
    let (_owner, _state) = fixture().await;
    let service = tokio::task::spawn_blocking(Bluetooth::start_system)
        .await
        .unwrap()
        .expect("private system bus");
    wait(&service, |s| s.available && s.devices.len() == 1).await;
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(service);
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(5))
        .expect("Bluetooth shutdown must not hang");
}

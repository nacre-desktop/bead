// SPDX-License-Identifier: MIT
//! NetworkManager discovery, signals, validated actions, and owner loss on a private bus.
use bead_services::network::{Network, NetworkAction, NetworkPrimary, NetworkSnapshot};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zbus::{
    Connection, Proxy,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const NAME: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const WLAN: &str = "/org/freedesktop/NetworkManager/Devices/1";
const ETH: &str = "/org/freedesktop/NetworkManager/Devices/2";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const HOME: &str = "/org/freedesktop/NetworkManager/Settings/1";
const WIRED: &str = "/org/freedesktop/NetworkManager/Settings/2";
const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/";
type Calls = Arc<Mutex<Vec<String>>>;
type Settings = HashMap<String, HashMap<String, OwnedValue>>;
fn path(p: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(p).unwrap()
}
fn ap(n: u8) -> OwnedObjectPath {
    path(&format!("{AP}{n}"))
}
fn owned(v: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(v).unwrap()
}

struct Manager {
    calls: Calls,
    wireless: bool,
    devices: Vec<OwnedObjectPath>,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.devices.clone()
    }
    #[zbus(property)]
    fn set_devices(&mut self, v: Vec<OwnedObjectPath>) {
        self.devices = v;
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(ACTIVE)]
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        path(ACTIVE)
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.wireless
    }
    #[zbus(property)]
    fn set_wireless_enabled(&mut self, v: bool) {
        self.wireless = v;
        self.calls
            .lock()
            .unwrap()
            .push(format!("WirelessEnabled:{v}"));
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    fn activate_connection(
        &self,
        connection: OwnedObjectPath,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
    ) -> OwnedObjectPath {
        self.calls.lock().unwrap().push(format!(
            "ActivateConnection:{connection}:{device}:{specific}"
        ));
        path(ACTIVE)
    }
    fn add_and_activate_connection(
        &self,
        settings: Settings,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        let ssid = Vec::<u8>::try_from(settings["802-11-wireless"]["ssid"].clone()).unwrap();
        let mut groups = settings.keys().cloned().collect::<Vec<_>>();
        groups.sort();
        self.calls.lock().unwrap().push(format!(
            "AddAndActivateConnection:{}:{device}:{specific}:{}",
            String::from_utf8_lossy(&ssid),
            groups.join(",")
        ));
        (
            path("/org/freedesktop/NetworkManager/Settings/9"),
            path(ACTIVE),
        )
    }
    fn deactivate_connection(&self, active: OwnedObjectPath) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("DeactivateConnection:{active}"));
    }
    #[zbus(signal)]
    async fn device_added(emitter: &SignalEmitter<'_>, device: OwnedObjectPath)
    -> zbus::Result<()>;
    #[zbus(signal)]
    async fn device_removed(
        emitter: &SignalEmitter<'_>,
        device: OwnedObjectPath,
    ) -> zbus::Result<()>;
}
struct Device {
    name: &'static str,
    kind: u32,
    active: &'static str,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property, name = "Interface")]
    fn iface(&self) -> String {
        self.name.into()
    }
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.kind
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        if self.active == "/" { 30 } else { 100 }
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        path(self.active)
    }
}
struct Wireless {
    calls: Calls,
    access_points: Vec<OwnedObjectPath>,
    active: OwnedObjectPath,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        self.access_points.clone()
    }
    #[zbus(property)]
    fn set_access_points(&mut self, v: Vec<OwnedObjectPath>) {
        self.access_points = v;
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        self.active.clone()
    }
    #[zbus(property)]
    fn set_active_access_point(&mut self, v: OwnedObjectPath) {
        self.active = v;
    }
    fn request_scan(&self, _options: HashMap<String, OwnedValue>) {
        self.calls.lock().unwrap().push("RequestScan".into());
    }
    #[zbus(signal)]
    async fn access_point_added(
        emitter: &SignalEmitter<'_>,
        access_point: OwnedObjectPath,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn access_point_removed(
        emitter: &SignalEmitter<'_>,
        access_point: OwnedObjectPath,
    ) -> zbus::Result<()>;
}
struct AccessPoint {
    ssid: &'static [u8],
    strength: u8,
    flags: u32,
    rsn: u32,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.ssid.to_vec()
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.strength
    }
    #[zbus(property)]
    fn set_strength(&mut self, v: u8) {
        self.strength = v;
    }
    #[zbus(property)]
    fn flags(&self) -> u32 {
        self.flags
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        self.rsn
    }
    #[zbus(property)]
    fn frequency(&self) -> u32 {
        2412
    }
}
struct Active;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn id(&self) -> String {
        "Home".into()
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "802-11-wireless".into()
    }
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(WLAN)]
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
}
struct SettingsRoot;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl SettingsRoot {
    #[zbus(property)]
    fn connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(HOME), path(WIRED)]
    }
}
struct Saved {
    calls: Calls,
    id: &'static str,
    kind: &'static str,
    ssid: &'static [u8],
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Saved {
    fn get_settings(&self) -> Settings {
        let mut settings = HashMap::from([(
            "connection".to_owned(),
            HashMap::from([
                ("id".to_owned(), owned(Value::from(self.id))),
                ("type".to_owned(), owned(Value::from(self.kind))),
            ]),
        )]);
        if !self.ssid.is_empty() {
            settings.insert(
                "802-11-wireless".into(),
                HashMap::from([("ssid".to_owned(), owned(Value::from(self.ssid.to_vec())))]),
            );
        }
        settings
    }
    fn delete(&self) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("Delete:{}", self.id));
    }
}

async fn fixture() -> (Connection, Calls) {
    let calls = Calls::default();
    let c = Connection::session().await.unwrap();
    let server = c.object_server();
    let manager = Manager {
        calls: calls.clone(),
        wireless: true,
        devices: vec![path(WLAN)],
    };
    server.at(ROOT, manager).await.unwrap();
    let wlan = Device {
        name: "wlan0",
        kind: 2,
        active: ACTIVE,
    };
    server.at(WLAN, wlan).await.unwrap();
    let wireless = Wireless {
        calls: calls.clone(),
        access_points: (1..=6).map(ap).collect(),
        active: ap(1),
    };
    server.at(WLAN, wireless).await.unwrap();
    let eth = Device {
        name: "eth0",
        kind: 1,
        active: "/",
    };
    server.at(ETH, eth).await.unwrap();
    let points: [(&[u8], u8, u32, u32); 6] = [
        (b"Home", 70, 1, 0x188),
        (b"Home", 40, 1, 0x188),
        (b"Cafe", 50, 0, 0),
        (b"Secure", 60, 1, 0x400),
        (b"Corp", 45, 1, 0x200),
        (b"Caf\xe9", 30, 1, 0),
    ];
    for (n, (ssid, strength, flags, rsn)) in (1..).zip(points) {
        let point = AccessPoint {
            ssid,
            strength,
            flags,
            rsn,
        };
        server.at(ap(n), point).await.unwrap();
    }
    server.at(ACTIVE, Active).await.unwrap();
    server
        .at("/org/freedesktop/NetworkManager/Settings", SettingsRoot)
        .await
        .unwrap();
    for (at, id, kind, ssid) in [
        (HOME, "Home", "802-11-wireless", b"Home".as_slice()),
        (WIRED, "Wired", "802-3-ethernet", b"".as_slice()),
    ] {
        let saved = Saved {
            calls: calls.clone(),
            id,
            kind,
            ssid,
        };
        server.at(at, saved).await.unwrap();
    }
    c.request_name(NAME).await.unwrap();
    (c, calls)
}
async fn wait(s: &Network, p: impl Fn(&NetworkSnapshot) -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if p(&s.snapshot()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("network state timeout: {:#?}", s.snapshot()));
}
async fn perform(
    service: &Arc<Network>,
    action: NetworkAction,
    now: Instant,
) -> Result<(), String> {
    // Network::action owns a blocking runtime, so keep it off Tokio's executor.
    let service = Arc::clone(service);
    tokio::task::spawn_blocking(move || service.action_at(action, now))
        .await
        .unwrap()
}
fn ssids(s: &NetworkSnapshot) -> Vec<&str> {
    s.access_points.iter().map(|a| a.ssid.as_str()).collect()
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn network_shutdown_with_closed_connection() {
    let connection = Connection::session().await.unwrap();
    connection.clone().close().await.unwrap();
    let (finished, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let service = Network::start(connection);
        drop(service);
        let _ = finished.send(());
    });
    receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("network shutdown must interrupt failed bus setup");
    worker.join().unwrap();
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn network_discovery_signals_actions_and_owner_loss() {
    assert!(std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS").is_some());
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    let (owner, calls) = fixture().await;
    // The production constructor: its connection must outlive the runtime that built it.
    let service = tokio::task::spawn_blocking(Network::start_system)
        .await
        .unwrap()
        .expect("private system bus");
    wait(&service, |s| s.available && s.access_points.len() == 5).await;
    let snap = service.snapshot();
    assert!(snap.wireless_enabled && snap.wireless_hardware_enabled);
    assert_eq!(
        snap.primary,
        Some(NetworkPrimary {
            kind: "wifi",
            name: "Home".into(),
            device: "wlan0".into(),
            state: "activated",
        })
    );
    assert_eq!(snap.devices.len(), 1);
    let wlan = &snap.devices[0];
    assert_eq!(
        (wlan.interface.as_str(), wlan.kind, wlan.state),
        ("wlan0", "wifi", "connected")
    );
    assert_eq!(wlan.active_connection, ACTIVE);
    assert_eq!(
        ssids(&snap),
        ["Home", "Secure", "Cafe", "Corp", "Caf\u{fffd}"]
    );
    let security = snap.access_points.iter().map(|a| a.security);
    assert_eq!(
        security.collect::<Vec<_>>(),
        ["wpa", "wpa3", "open", "enterprise", "wep"]
    );
    let home = &snap.access_points[0];
    assert_eq!(home.path, ap(1).as_str());
    assert!(home.active && home.known && home.strength == 70);
    assert_eq!(home.frequency_mhz, 2412);
    assert!(!snap.access_points[2].known && !snap.access_points[2].active);
    assert_eq!(snap.access_points[4].ssid_bytes, b"Caf\xe9");
    let known = snap
        .known_connections
        .iter()
        .map(|k| (k.id.as_str(), k.kind));
    assert_eq!(
        known.collect::<Vec<_>>(),
        [("Home", "wifi"), ("Wired", "ethernet")]
    );

    let cafe = Proxy::new(
        &owner,
        NAME,
        ap(3),
        "org.freedesktop.NetworkManager.AccessPoint",
    )
    .await
    .unwrap();
    cafe.set_property("Strength", 90_u8).await.unwrap();
    wait(&service, |s| ssids(s).first() == Some(&"Cafe")).await;
    let wireless = Proxy::new(
        &owner,
        NAME,
        WLAN,
        "org.freedesktop.NetworkManager.Device.Wireless",
    )
    .await
    .unwrap();
    wireless
        .set_property("ActiveAccessPoint", ap(3))
        .await
        .unwrap();
    wait(&service, |s| {
        s.access_points[0].active && s.access_points.iter().filter(|a| a.active).count() == 1
    })
    .await;
    let wlan_emitter = SignalEmitter::new(&owner, WLAN).unwrap();
    wireless
        .set_property("AccessPoints", (1..=5).map(ap).collect::<Vec<_>>())
        .await
        .unwrap();
    Wireless::access_point_removed(&wlan_emitter, ap(6))
        .await
        .unwrap();
    wait(&service, |s| s.access_points.len() == 4).await;
    // Update the fake directly so only AccessPointAdded drives discovery.
    owner
        .object_server()
        .interface::<_, Wireless>(WLAN)
        .await
        .unwrap()
        .get_mut()
        .await
        .access_points
        .push(ap(6));
    Wireless::access_point_added(&wlan_emitter, ap(6))
        .await
        .unwrap();
    wait(&service, |s| s.access_points.len() == 5).await;

    let manager = Proxy::new(&owner, NAME, ROOT, NAME).await.unwrap();
    let root_emitter = SignalEmitter::new(&owner, ROOT).unwrap();
    manager
        .set_property("Devices", vec![path(WLAN), path(ETH)])
        .await
        .unwrap();
    Manager::device_added(&root_emitter, path(ETH))
        .await
        .unwrap();
    wait(&service, |s| s.devices.len() == 2).await;
    let eth = service.snapshot().devices[1].clone();
    assert_eq!(
        (eth.interface.as_str(), eth.kind, eth.state),
        ("eth0", "ethernet", "disconnected")
    );
    assert!(eth.active_connection.is_empty());

    let t0 = Instant::now();
    let invalid = [
        NetworkAction::Scan(ETH.into()),
        NetworkAction::Scan("/invalid".into()),
        NetworkAction::ActivateKnown("/invalid".into(), WLAN.into()),
        NetworkAction::ActivateKnown(HOME.into(), "/invalid".into()),
        NetworkAction::ActivateOpen(b"Missing".to_vec(), WLAN.into()),
        NetworkAction::ActivateOpen(b"Cafe".to_vec(), ETH.into()),
        NetworkAction::Disconnect(ETH.into()),
        NetworkAction::Forget("/invalid".into()),
    ];
    for action in invalid {
        assert!(
            perform(&service, action.clone(), t0).await.is_err(),
            "{action:?}"
        );
    }
    let agent = perform(
        &service,
        NetworkAction::ActivateOpen(b"Secure".to_vec(), WLAN.into()),
        t0,
    )
    .await
    .unwrap_err();
    assert_eq!(agent, "secured network needs a password agent");
    let scan = NetworkAction::Scan(WLAN.into());
    perform(&service, scan.clone(), t0).await.unwrap();
    let limited = perform(&service, scan.clone(), t0 + Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(limited.contains("10 seconds"), "{limited}");
    perform(&service, scan, t0 + Duration::from_secs(11))
        .await
        .unwrap();
    for action in [
        NetworkAction::ActivateKnown(HOME.into(), WLAN.into()),
        NetworkAction::ActivateOpen(b"Cafe".to_vec(), WLAN.into()),
        NetworkAction::Disconnect(WLAN.into()),
        NetworkAction::Forget(HOME.into()),
        NetworkAction::SetWirelessEnabled(false),
    ] {
        perform(&service, action, t0).await.unwrap();
    }
    wait(&service, |s| !s.wireless_enabled).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [
            "RequestScan".to_owned(),
            "RequestScan".into(),
            format!("ActivateConnection:{HOME}:{WLAN}:/"),
            format!(
                "AddAndActivateConnection:Cafe:{WLAN}:{}:802-11-wireless,connection",
                ap(3)
            ),
            format!("DeactivateConnection:{ACTIVE}"),
            "Delete:Home".into(),
            "WirelessEnabled:false".into(),
        ]
    );

    manager
        .set_property("Devices", vec![path(WLAN)])
        .await
        .unwrap();
    Manager::device_removed(&root_emitter, path(ETH))
        .await
        .unwrap();
    wait(&service, |s| s.devices.len() == 1).await;

    drop((cafe, wireless, manager, wlan_emitter, root_emitter));
    drop(owner);
    wait(&service, |s| !s.available && s.access_points.is_empty()).await;
    let (owner, _) = fixture().await;
    wait(&service, |s| s.available && s.access_points.len() == 5).await;
    drop(owner);
}

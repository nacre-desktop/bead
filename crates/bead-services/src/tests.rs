// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::*;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

struct Power;
#[zbus::interface(name = "org.freedesktop.UPower")]
impl Power {
    fn get_display_device(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/UPower/devices/DisplayDevice").unwrap()
    }
}
struct BatteryDevice;
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl BatteryDevice {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        77.5
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        2
    }
}
struct NetworkManager(u32);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl NetworkManager {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/ActiveConnection/1").unwrap()
    }
}
struct ActiveConnection;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> &str {
        "Fixture Wi-Fi"
    }
}
struct Bluez(bool);
#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl Bluez {
    fn get_managed_objects(
        &self,
    ) -> HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> {
        HashMap::from([(
            OwnedObjectPath::try_from("/org/bluez/hci0").unwrap(),
            HashMap::from([(
                "org.bluez.Adapter1".into(),
                HashMap::from([(
                    "Powered".into(),
                    Value::from(self.0).try_to_owned().unwrap(),
                )]),
            )]),
        )])
    }
}
async fn network(state: u32) -> Connection {
    zbus::connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager", NetworkManager(state))
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/ActiveConnection/1",
            ActiveConnection,
        )
        .unwrap()
        .build()
        .await
        .unwrap()
}
async fn bluez(powered: bool) -> Connection {
    zbus::connection::Builder::session()
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/", Bluez(powered))
        .unwrap()
        .build()
        .await
        .unwrap()
}
async fn wait(desktop: &Desktop, predicate: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if predicate(&desktop.snapshot()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("private service state timeout: {:?}", desktop.snapshot());
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires private D-Bus and explicit private PipeWire runtime"]
async fn private_bus_sensor_owner_restarts_and_subscription_cleanup() {
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    assert_eq!(
        std::env::var("DBUS_SYSTEM_BUS_ADDRESS"),
        std::env::var("DBUS_SESSION_BUS_ADDRESS")
    );
    assert_eq!(
        std::env::var("PIPEWIRE_REMOTE").as_deref(),
        Ok("bead-test-unavailable")
    );
    let power = zbus::connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at("/org/freedesktop/UPower", Power)
        .unwrap()
        .serve_at(
            "/org/freedesktop/UPower/devices/DisplayDevice",
            BatteryDevice,
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let nm = network(70).await;
    let bt = bluez(true).await;
    let desktop = Desktop::start();
    let updates = Arc::new(AtomicUsize::new(0));
    let count = updates.clone();
    let weak = Arc::downgrade(&desktop);
    let subscription = desktop.subscribe(move |_| {
        let _snapshot = weak.upgrade().unwrap().snapshot();
        count.fetch_add(1, Ordering::SeqCst);
    });
    wait(&desktop, |s| {
        s.battery.available && s.network.connected && s.bluetooth.powered
    })
    .await;
    assert_eq!(desktop.snapshot().battery.percentage, Some(77.5));
    assert_eq!(desktop.snapshot().network.name, "Fixture Wi-Fi");
    assert!(desktop.action(Action::Volume(f64::NAN)).is_err());
    assert!(desktop.action(Action::Mute(true)).is_err());
    nm.close().await.unwrap();
    wait(&desktop, |s| !s.network.available).await;
    let nm = network(40).await;
    wait(&desktop, |s| s.network.available && !s.network.connected).await;
    bt.close().await.unwrap();
    wait(&desktop, |s| !s.bluetooth.available).await;
    let bt = bluez(false).await;
    wait(&desktop, |s| s.bluetooth.available && !s.bluetooth.powered).await;
    power.close().await.unwrap();
    wait(&desktop, |s| !s.battery.available).await;
    drop(subscription);
    assert_eq!(desktop.subscription_count(), 0);
    assert!(updates.load(Ordering::SeqCst) >= 6);
    drop(desktop);
    drop(nm);
    drop(bt);
}
#[test]
fn service_snapshot_marks_absence_and_serializes_optionals() {
    let snapshot = Snapshot::default();
    assert!(!snapshot.audio.connected);
    assert!(snapshot.audio.volume.is_none());
    assert!(snapshot.battery.percentage.is_none());
    assert!(!snapshot.network.available);
    assert!(!snapshot.bluetooth.available);
    assert!(snapshot.tray.is_empty());
    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["audio"]["volume"], serde_json::Value::Null);
}

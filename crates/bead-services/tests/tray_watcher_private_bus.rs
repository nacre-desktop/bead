// SPDX-License-Identifier: MIT
//! StatusNotifierWatcher ownership proof on a private D-Bus session.
use std::time::Duration;
use bead_services::{Config, tray_watcher};
use futures_util::StreamExt;
use zbus::{Connection, Proxy, fdo::{DBusProxy, RequestNameFlags}};
#[allow(dead_code)]
#[path = "../examples/tray-fixture.rs"]
mod fixture;

const NAME: &str = "org.kde.StatusNotifierWatcher";

async fn wait_owner(connection: &Connection, expected: bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if DBusProxy::new(connection).await.unwrap().name_has_owner(NAME.try_into().unwrap()).await.unwrap() == expected { return; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("watcher ownership timeout");
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn watcher_claims_registers_tracks_owners_and_yields_replacement() {
    assert_eq!(std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(), Ok("1"));
    let connection = Connection::session().await.unwrap();
    let handle = tray_watcher::spawn(connection.clone());
    wait_owner(&connection, true).await;

    let proxy = Proxy::new(&connection, NAME, "/StatusNotifierWatcher", NAME).await.unwrap();
    let version: i32 = proxy.get_property("ProtocolVersion").await.unwrap();
    assert_eq!(version, 0);
    let items: Vec<String> = proxy.get_property("RegisteredStatusNotifierItems").await.unwrap();
    assert!(items.is_empty());
    let has_host: bool = proxy.get_property("IsStatusNotifierHostRegistered").await.unwrap();
    assert!(!has_host);

    let item_bus = Connection::session().await.unwrap();
    let item = Proxy::new(&item_bus, NAME, "/StatusNotifierWatcher", NAME).await.unwrap();
    item.call::<_, _, ()>("RegisterStatusNotifierItem", &("org.example.Item",)).await.unwrap();
    item.call::<_, _, ()>("RegisterStatusNotifierItem", &("/org/example/Item",)).await.unwrap();
    let items: Vec<String> = proxy.get_property("RegisteredStatusNotifierItems").await.unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|id| id == "org.example.Item/StatusNotifierItem"));
    assert!(items.iter().any(|id| id.ends_with("/org/example/Item")));

    let mut unregistered = proxy.receive_signal("StatusNotifierItemUnregistered").await.unwrap();
    drop(item_bus);
    let signal = tokio::time::timeout(Duration::from_secs(4), unregistered.next())
        .await
        .expect("unregistered signal timeout")
        .expect("unregistered signal stream ended");
    let removed: String = signal.body().deserialize().unwrap();
    assert!(removed.ends_with("/org/example/Item") || removed == "org.example.Item/StatusNotifierItem");
    let items: Vec<String> = proxy.get_property("RegisteredStatusNotifierItems").await.unwrap();
    assert!(items.is_empty());

    let foreign = Connection::session().await.unwrap();
    let dbus = DBusProxy::new(&foreign).await.unwrap();
    dbus.request_name(NAME.try_into().unwrap(), RequestNameFlags::ReplaceExisting.into()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let owner = DBusProxy::new(&connection).await.unwrap().get_name_owner(NAME.try_into().unwrap()).await.unwrap();
            if Some(owner.as_ref()) == foreign.unique_name() { return; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("foreign watcher replaced fallback owner");
    drop(foreign);
    wait_owner(&connection, true).await;
    drop(handle);
    wait_owner(&connection, false).await;
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn watcher_is_opt_in_and_waits_for_foreign_owner() {
    assert_eq!(std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(), Ok("1"));
    let connection = Connection::session().await.unwrap();
    let foreign = Connection::session().await.unwrap();
    DBusProxy::new(&foreign).await.unwrap().request_name(NAME.try_into().unwrap(), RequestNameFlags::DoNotQueue.into()).await.unwrap();
    let handle = tray_watcher::spawn(connection.clone());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!DBusProxy::new(&connection).await.unwrap().name_has_owner(NAME.try_into().unwrap()).await.unwrap());
    drop(foreign);
    wait_owner(&connection, true).await;
    drop(handle);
    wait_owner(&connection, false).await;

    let _desktop = bead_services::Desktop::start_config(Config::default());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!DBusProxy::new(&connection).await.unwrap().name_has_owner(NAME.try_into().unwrap()).await.unwrap());
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn desktop_opt_in_starts_watcher_before_tray_client_snapshot() {
    assert_eq!(std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(), Ok("1"));
    let service = bead_services::Desktop::start_config(Config {
        audio: false,
        bus: true,
        tray_watcher: true,
    });
    let fixture_task = tokio::spawn(async { let _ = fixture::run().await; });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if service.snapshot().tray.len() >= 2 { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("existing tray client snapshot receives fallback registrations");
    service.stop();
    fixture_task.abort();
}

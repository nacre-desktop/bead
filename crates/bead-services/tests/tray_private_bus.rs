// SPDX-License-Identifier: MIT OR Apache-2.0
//! SNI/DBusMenu integration proof. Run under dbus-run-session with the same
//! private address supplied as DBUS_SYSTEM_BUS_ADDRESS.
#[allow(dead_code)]
#[path = "../examples/tray-fixture.rs"]
mod fixture;

use bead_services::{Action, Config, Desktop};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zbus::{message::Header, object_server::SignalEmitter};

#[derive(Default)]
struct ForeignWatcher(Mutex<HashSet<String>>);
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl ForeignWatcher {
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
    fn register_status_notifier_host(&self, _service: &str) {}
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let owner = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing sender".into()))?;
        let address = if service.starts_with('/') {
            format!("{owner}{service}")
        } else {
            service.to_owned()
        };
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(address.clone());
        self.registered_status_notifier_items_changed(&emitter)
            .await?;
        Self::status_notifier_item_registered(&emitter, &address).await?;
        Ok(())
    }
    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;
}

fn start_fixture() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {
        let _ = fixture::run().await;
    })
}

async fn wait_item(service: &Desktop, present: bool) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let item = service.snapshot().tray.first().map(|item| item.id.clone());
            if item.is_some() == present {
                return Some(item.unwrap_or_default());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .ok()
    .flatten()
}

async fn call(service: Arc<Desktop>, action: Action) {
    tokio::task::spawn_blocking(move || service.call(action))
        .await
        .expect("service action worker")
        .unwrap();
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn private_sni_registration_actions_owner_loss_and_recovery() {
    assert!(
        std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS").is_some(),
        "run only with DBUS_SYSTEM_BUS_ADDRESS set to the private session bus"
    );
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1"),
        "run only in the private proof environment"
    );

    let foreign_watcher = zbus::connection::Builder::session()
        .unwrap()
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at("/StatusNotifierWatcher", ForeignWatcher::default())
        .unwrap()
        .build()
        .await
        .expect("private foreign watcher");
    let service = Desktop::start_config(Config {
        audio: false,
        bus: true,
    });
    async fn wait_replacement(service: &Desktop, old: &str) -> Option<String> {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                if let Some(item) = service.snapshot().tray.first()
                    && item.id != old
                {
                    return item.id.clone();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .ok()
    }
    let mut fixture = start_fixture();
    let id = wait_item(&service, true)
        .await
        .expect("fake SNI item registered");
    let snapshot = service.snapshot();
    let item = &snapshot.tray[0];
    assert_eq!(item.title, "Private tray fixture 0");
    assert_eq!(item.tooltip_title, "Private tray fixture");
    assert_eq!(item.icon_pixmap.len(), 1);
    call(
        service.clone(),
        Action::TrayActivate {
            item: id.clone(),
            x: 4,
            y: 7,
        },
    )
    .await;
    call(
        service.clone(),
        Action::TraySecondaryActivate {
            item: id.clone(),
            x: 4,
            y: 7,
        },
    )
    .await;
    call(
        service.clone(),
        Action::TrayScroll {
            item: id.clone(),
            delta: 1,
            horizontal: false,
        },
    )
    .await;
    call(service.clone(), Action::TrayPrepare { item: id.clone() }).await;
    call(
        service.clone(),
        Action::TrayPrepareEntry {
            item: id.clone(),
            entry: 2,
        },
    )
    .await;
    assert_eq!(
        service.snapshot().tray[0].menu[1].children[0].label,
        "Prepared action"
    );
    call(
        service.clone(),
        Action::TrayMenu {
            item: id.clone(),
            entry: 1,
        },
    )
    .await;
    assert!(
        tokio::task::spawn_blocking({
            let service = service.clone();
            let id = id.clone();
            move || {
                service.call(Action::TrayMenu {
                    item: id,
                    entry: 999,
                })
            }
        })
        .await
        .unwrap()
        .is_err()
    );
    if std::env::var_os("BEAD_FIXTURE_UPDATE").is_some() {
        tokio::time::timeout(Duration::from_secs(2), async {
            while service
                .snapshot()
                .tray
                .first()
                .is_none_or(|item| item.title != "Private tray fixture 1")
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("SNI title and icon update observed");
        tokio::time::timeout(Duration::from_secs(2), async {
            while service.snapshot().tray.first().is_none_or(|item| {
                item.menu
                    .first()
                    .is_none_or(|entry| entry.label != "Updated action")
            }) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("DBusMenu layout and property updates observed");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let snapshot = service.snapshot();
                if snapshot.tray.first().is_some_and(|item| {
                    item.attention && item.icon_pixmap.first().is_some_and(|pixmap| pixmap.0 == 8)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("NewAttentionIcon pixmap is projected");
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let snapshot = service.snapshot();
                if snapshot.tray.first().is_some_and(|item| {
                    item.attention
                        && item
                            .icon_pixmap
                            .first()
                            .is_some_and(|pixmap| pixmap.0 == 24)
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("empty attention pixmap falls back to normal icon");
        let item = service.snapshot();
        let ids: Vec<_> = item.tray[0].menu.iter().map(|entry| entry.id).collect();
        assert_eq!(
            ids,
            [1, 2],
            "non-root LayoutUpdated preserves sibling menu entries"
        );
    }
    assert_eq!(
        fixture::take_calls(),
        [
            "activate 4 7",
            "secondary 4 7",
            "scroll 1 vertical",
            "prepare 0",
            "prepare 2",
            "menu 1 clicked"
        ]
    );

    fixture.abort();
    let _ = fixture.await;
    assert_eq!(wait_item(&service, false).await.as_deref(), Some(""));
    fixture = start_fixture();
    let replacement = wait_replacement(&service, &id)
        .await
        .expect("replacement SNI registration");
    assert_ne!(
        replacement, id,
        "owner replacement receives a fresh unique identity"
    );
    drop(foreign_watcher);
    assert_eq!(
        wait_item(&service, true).await.as_deref(),
        Some(replacement.as_str())
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if fixture::take_calls()
                .iter()
                .any(|call| call == "reregister")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("item re-registers after watcher ownership changes");
    call(
        service.clone(),
        Action::TrayActivate {
            item: replacement.clone(),
            x: 0,
            y: 0,
        },
    )
    .await;
    assert_eq!(fixture::take_calls(), ["activate 0 0"]);
    fixture.abort();
    let _ = fixture.await;
    service.stop();
    println!("TRAY_PRIVATE_BUS_PASS registration=1 actions=7 owner_loss=1 watcher_reacquire=1");
}

struct OwnedDaemon(Option<Child>);
impl OwnedDaemon {
    fn start(address: &str) -> Self {
        let child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--nopidfile",
                &format!("--address={address}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start test-owned private dbus-daemon");
        Self(Some(child))
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.0.take() {
            if child.try_wait().expect("check owned daemon").is_none() {
                child.kill().expect("stop exact test-owned daemon PID");
            }
            let _ = child.wait();
        }
    }
}
impl Drop for OwnedDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

struct TempRoot(PathBuf);
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn wait_bus(address: &str) {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Ok(builder) = zbus::connection::Builder::address(address)
                && let Ok(connection) = builder.build().await
            {
                drop(connection);
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("private bus readiness timeout");
}

fn marker(path: &Path, value: &str) {
    fs::write(path, value).expect("write lifecycle marker");
}

async fn wait_marker(path: &Path) -> String {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(value) = fs::read_to_string(path) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("lifecycle marker timeout")
}

#[tokio::test]
#[ignore = "supervisor for a test-owned private D-Bus restart"]
async fn session_bus_eof_reconnect_supervised() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("bead-tray-restart-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).expect("create private temp root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
        .expect("protect private temp root");
    let _root = TempRoot(root.clone());
    let address = format!("unix:path={}/bus", root.display());
    let mut daemon = OwnedDaemon::start(&address);
    wait_bus(&address).await;
    let ready = root.join("ready");
    let gone = root.join("gone");
    let done = root.join("done");
    let mut helper = OwnedDaemon(Some(
        Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "session_bus_reconnect_client_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("BEAD_TRAY_RECONNECT_HELPER_DIR", &root)
            .env("BEAD_PRIVATE_SERVICES_TEST", "1")
            .env("DBUS_SESSION_BUS_ADDRESS", &address)
            .env("DBUS_SYSTEM_BUS_ADDRESS", &address)
            .env("BEAD_FIXTURE_GENERATION", "10")
            .env_remove("BEAD_FIXTURE_UPDATE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start reconnect helper test child"),
    ));
    let first = wait_marker(&ready).await;
    assert!(
        first.starts_with("ready:"),
        "unexpected helper ready marker: {first}"
    );
    daemon.stop(); // exact child process created above
    let lost = wait_marker(&gone).await;
    assert!(
        lost.starts_with("gone:"),
        "helper did not observe session bus EOF: {lost}"
    );
    daemon = OwnedDaemon::start(&address);
    wait_bus(&address).await;
    let complete = wait_marker(&done).await;
    assert!(
        complete.starts_with("done:"),
        "helper did not recover: {complete}"
    );
    let status = helper
        .0
        .as_mut()
        .expect("helper child exists")
        .wait()
        .expect("wait reconnect helper");
    assert!(status.success(), "reconnect helper failed: {status}");
    daemon.stop();
    println!(
        "TRAY_SESSION_RECONNECT_PASS initial_item=1 bus_eof=1 empty_snapshot=1 recovered_item=1 activation=1"
    );
}

#[tokio::test]
#[ignore = "child helper for session_bus_eof_reconnect_supervised"]
async fn session_bus_reconnect_client_helper() {
    let Some(root) = std::env::var_os("BEAD_TRAY_RECONNECT_HELPER_DIR").map(PathBuf::from) else {
        return;
    };
    let service = Desktop::start_config(Config {
        audio: false,
        bus: true,
    });
    let mut fixture = tokio::spawn(async {
        let _ = fixture::run().await;
    });
    let old_id = wait_item(&service, true).await.expect("initial tray item");
    marker(&root.join("ready"), &format!("ready:{old_id}"));
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if service.snapshot().tray.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("service clears tray after bus EOF");
    marker(&root.join("gone"), &format!("gone:{old_id}"));
    wait_bus(&std::env::var("DBUS_SESSION_BUS_ADDRESS").expect("private bus address")).await;
    let _ = fixture.await;
    fixture = tokio::spawn(async {
        let _ = fixture::run().await;
    });
    let new_id = wait_item(&service, true)
        .await
        .expect("tray after session bus recovery");
    assert!(
        service
            .snapshot()
            .tray
            .iter()
            .any(|item| item.id == new_id && item.title == "Private tray fixture 10")
    );
    call(
        service.clone(),
        Action::TrayActivate {
            item: new_id.clone(),
            x: 0,
            y: 0,
        },
    )
    .await;
    assert_eq!(fixture::take_calls(), ["activate 0 0"]);
    assert!(
        !service
            .snapshot()
            .tray
            .iter()
            .any(|item| item.id == old_id && item.title != "Private tray fixture 10")
    );
    marker(&root.join("done"), &format!("done:{new_id}"));
    fixture.abort();
    let _ = fixture.await;
    service.stop();
}

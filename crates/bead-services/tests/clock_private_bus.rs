// SPDX-License-Identifier: MIT OR Apache-2.0
//! Run on a private bus, never the host system bus. A slow logind read crosses
//! the minute boundary without suspend or any change to the host clock.
use bead_services::{Config, Desktop};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Login(Arc<AtomicBool>, Arc<tokio::sync::Notify>);
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login {
    async fn get_session(&self, _id: &str) -> zbus::zvariant::OwnedObjectPath {
        if self.0.load(Ordering::SeqCst) {
            self.1.notify_one();
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/login1/session/proof").unwrap()
    }
}

#[tokio::test]
#[ignore = "requires an explicitly private D-Bus session; waits for a real minute boundary"]
async fn busy_signal_handler_cannot_skip_a_minute_tick() {
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    assert_eq!(
        std::env::var("DBUS_SYSTEM_BUS_ADDRESS"),
        std::env::var("DBUS_SESSION_BUS_ADDRESS")
    );
    let slow = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(tokio::sync::Notify::new());
    let connection = zbus::connection::Builder::session()
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at(
            "/org/freedesktop/login1",
            Login(slow.clone(), entered.clone()),
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let desktop = Desktop::start_config(Config {
        audio: false,
        bus: true,
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while desktop.snapshot().clock.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let mut until =
        Duration::from_secs(60) - Duration::from_millis((now.as_millis() % 60_000) as u64);
    if until < Duration::from_secs(2) {
        until += Duration::from_secs(60);
    }
    let boundary = SystemTime::now() + until;
    tokio::time::sleep(until - Duration::from_millis(500)).await;
    let before = desktop.snapshot().clock.clone();
    slow.store(true, Ordering::SeqCst);
    connection
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "SessionNew",
            &("proof",),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_millis(250), entered.notified())
        .await
        .expect("slow signal handler must be entered before the minute boundary");
    assert!(
        SystemTime::now() < boundary,
        "signal arrived after the boundary"
    );
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        while desktop.snapshot().clock == before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    slow.store(false, Ordering::SeqCst);
    desktop.stop();
    assert!(
        result.is_ok(),
        "minute boundary was lost while a signal handler was running"
    );
}

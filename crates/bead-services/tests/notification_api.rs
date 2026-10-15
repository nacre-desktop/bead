// SPDX-License-Identifier: MIT OR Apache-2.0
//! The same wire parity fixture runs with no UI/compositor against the host API.
#[allow(dead_code)]
#[path = "../examples/notification-client.rs"]
mod fixture;
use bead_services::notification_bus::{self, Config, Control, PrivateBus};
use std::{os::unix::fs::PermissionsExt, time::Duration};

#[test]
fn injected_service_parity_and_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("bead-host-api-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let bus = PrivateBus::start(root.join("bus"))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    rt.block_on(async {
        let connection = bus.connect().await?;
        let config = || Config {
            connection: connection.clone(),
            history: root.join("history.json"),
        };
        let service = notification_bus::start(config()).await?;
        let mut updates = service.subscribe();
        assert!(service.snapshot().is_object());
        // A failed second start must neither replace nor remove our existing server.
        assert!(notification_bus::start(config()).await.is_err());
        let competing = bus.connect().await?;
        assert!(
            notification_bus::start(Config {
                connection: competing,
                history: root.join("other.json")
            })
            .await
            .is_err()
        );
        fixture::parity(&bus.address).await?;
        assert!(
            service
                .call(Control::Action(u32::MAX, "missing".into()))
                .await
                .is_err()
        );
        assert!(
            service
                .call(Control::Osd {
                    channel: "bad".into(),
                    value: f64::NAN,
                    muted: false
                })
                .await
                .is_err()
        );
        service.call(Control::Dnd(true)).await?;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !updates.borrow_and_update()["dnd"]
                .as_bool()
                .unwrap_or(false)
            {
                updates.changed().await?;
            }
            Ok::<_, tokio::sync::watch::error::RecvError>(())
        })
        .await??;
        let count = service.snapshot()["history_count"]
            .as_u64()
            .ok_or("missing history count")?;
        service.stop().await?;
        let restored = notification_bus::start(config()).await?;
        assert_eq!(restored.snapshot()["history_count"], count);
        assert_eq!(restored.snapshot()["dnd"], true);
        assert_eq!(restored.snapshot()["popup_count"], 0);
        // Subscribe retains only the newest snapshot, independent of reader speed.
        let slow = restored.subscribe();
        for i in 0..200 {
            restored.call(Control::Dnd(i % 2 == 0)).await?;
        }
        restored.call(Control::Dnd(true)).await?;
        tokio::task::yield_now().await;
        assert_eq!(slow.borrow()["dnd"], true);
        drop(restored); // cancellation also releases interfaces/name on injected connection
        let proxy = zbus::fdo::DBusProxy::new(&connection).await?;
        tokio::time::timeout(Duration::from_secs(1), async {
            while proxy
                .name_has_owner("org.freedesktop.Notifications".try_into()?)
                .await?
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<_, zbus::Error>(())
        })
        .await??;
        let again = notification_bus::start(config()).await?;
        again.stop().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    println!("HOST_API_PARITY_PASS no_ui=true current_thread=true");
    Ok(())
}

#[test]
fn composite_host_contract_without_outside_connections() {
    let service = bead_services::Desktop::start_config(bead_services::Config {
        audio: false,
        bus: false,
    });
    let subscription = service.subscribe(|snapshot| {
        assert!(!snapshot.audio.connected);
    });
    assert!(serde_json::to_value(&*service.snapshot()).is_ok());
    assert!(service.call(bead_services::Action::Mute(false)).is_err());
    drop(subscription);
    service.stop();
    service.stop();
}

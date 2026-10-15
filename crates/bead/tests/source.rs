// SPDX-License-Identifier: MIT OR Apache-2.0
#[allow(dead_code)]
#[path = "../../bead-services/examples/notification-client.rs"]
mod fixture;
use bead::core::source::{Applied, Mirror, Source};
use serde_json::json;
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn injected_notification_handle_source_runs_wire_parity_without_ui()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("bead-source-api-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    let _cleanup = Cleanup(root.clone());
    let bus = bead::services::notification_bus::PrivateBus::start(root.join("bus"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let handle =
            bead::services::notification_bus::start(bead::services::notification_bus::Config {
                connection: bus.connect().await?,
                history: root.join("history.json"),
            })
            .await?;
        let source = bead::source::HostedNotifications::attach(handle)?;
        fixture::parity(&bus.address).await?;
        source
            .call(
                "notifications.dnd".into(),
                json!({"value":true}),
                Duration::from_secs(1),
            )
            .await?;
        let until = Instant::now() + Duration::from_secs(1);
        while source.snapshot().data["dnd"] != true {
            assert!(Instant::now() < until);
            tokio::task::yield_now().await;
        }
        let mut mirror = Mirror::new(Instant::now());
        let mut stores = bead::core::Stores::default();
        assert_eq!(
            mirror.project(
                &mut stores,
                source.snapshot(),
                |data| Ok(BTreeMap::from([(
                    "notifications.dnd".into(),
                    bead::core::Value::Bool(data["dnd"].as_bool().unwrap_or(false))
                )])),
                Instant::now()
            )?,
            Applied::Updated
        );
        assert_eq!(
            stores.get("notifications.dnd"),
            Some(&bead::core::Value::Bool(true))
        );
        assert!(
            source
                .call(
                    "notifications.dnd".into(),
                    json!({"value":"bad"}),
                    Duration::from_secs(1)
                )
                .await
                .is_err()
        );
        assert!(
            source
                .call(
                    "notifications.action".into(),
                    json!({"id":u32::MAX,"key":"unknown"}),
                    Duration::from_secs(1)
                )
                .await
                .is_err()
        );
        source.stop().await?;
        assert!(!source.snapshot().connected);
        let check = bus.connect().await?;
        let owner = zbus::fdo::DBusProxy::new(&check)
            .await?
            .name_has_owner("org.freedesktop.Notifications".try_into()?)
            .await?;
        assert!(!owner);
        let desktop = bead::services::Desktop::start_config(bead::services::Config {
            audio: false,
            bus: false,
        });
        let source = bead::source::Desktop::attach(desktop)?;
        assert_eq!(source.snapshot().data["audio"]["connected"], false);
        assert!(
            source
                .call(
                    "audio.mute".into(),
                    json!({"value":false}),
                    Duration::from_secs(1)
                )
                .await
                .is_err()
        );
        source.stop().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
}

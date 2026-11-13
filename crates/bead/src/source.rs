// SPDX-License-Identifier: MIT
//! In-process implementations of the same Source interface used by remote views.
use bead_core::source::{
    Callback, Error, Publisher, Reply, Result, Snapshot, Source, Subscription,
};
use bead_services::notification_bus::{Control, Server};
use serde_json::{Value as Json, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
static EPOCH: AtomicU64 = AtomicU64::new(1);
/// Private compatibility host. No ambient bus discovery; the service still owns state.
pub struct Notifications {
    publisher: Publisher,
    server: Mutex<Option<Arc<Server>>>,
}
impl Notifications {
    pub fn private_with_options(
        socket: PathBuf,
        history: PathBuf,
        options: bead_services::notification_bus::Options,
    ) -> Result<Self> {
        let publisher = Publisher::new(
            format!("notifications-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            json!({}),
        )?;
        let changed = publisher.clone();
        let stopped = publisher.clone();
        let server = Server::start_with_options(
            socket,
            history,
            options,
            move |value| {
                if changed.publish(value).is_err() {
                    let _ = changed.disconnect();
                }
            },
            move || {
                let _ = stopped.disconnect();
            },
        )
        .map_err(|_| {
            Error::new(
                "unavailable",
                "private notification host failed to start",
                true,
            )
        })?;
        Ok(Self {
            publisher,
            server: Mutex::new(Some(Arc::new(server))),
        })
    }
    fn server(&self) -> Result<Arc<Server>> {
        self.server
            .lock()
            .map_err(|_| Error::new("internal", "notification host poisoned", false))?
            .clone()
            .ok_or_else(|| Error::new("unavailable", "notification host stopped", true))
    }
    pub fn address(&self) -> Result<String> {
        Ok(self.server()?.address.clone())
    }
    /// Existing synchronous shell command ingress; new Source calls acknowledge processing.
    pub fn control(&self, action: Control) -> std::result::Result<(), String> {
        self.server().map_err(|e| e.to_string())?.control(action)
    }
}
fn action(method: &str, params: &Json) -> Result<Control> {
    let invalid = || Error::new("validation", "invalid notification source action", false);
    let id = || {
        params["id"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(invalid)
    };
    Ok(match method {
        "notifications.dnd" => Control::Dnd(params["value"].as_bool().ok_or_else(invalid)?),
        "notifications.action" => {
            Control::Action(id()?, params["key"].as_str().ok_or_else(invalid)?.into())
        }
        "notifications.dismiss" => Control::Dismiss(id()?),
        "notifications.details" => Control::Details(id()?),
        "notifications.hover" => {
            Control::Hover(id()?, params["value"].as_bool().ok_or_else(invalid)?)
        }
        "notifications.hide" => Control::HidePopups,
        "notifications.clear" => Control::ClearHistory,
        "notifications.osd" => Control::Osd {
            channel: params["channel"].as_str().ok_or_else(invalid)?.into(),
            value: params["value"].as_f64().ok_or_else(invalid)?,
            muted: params["muted"].as_bool().ok_or_else(invalid)?,
        },
        _ => {
            return Err(Error::new(
                "unknown_method",
                "unknown notification source action",
                false,
            ));
        }
    })
}
impl Source for Notifications {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, method: String, params: Json) -> Reply<'_> {
        Box::pin(async move {
            let command = action(&method, &params)?;
            self.server()?
                .call(command)
                .await
                .map_err(|_| Error::new("unavailable", "notification action failed", true))?;
            Ok(Json::Null)
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async {
            Err(Error::new(
                "unavailable",
                "host must inject a replacement connection",
                true,
            ))
        })
    }
    fn stop(&self) -> Reply<'_> {
        Box::pin(async move {
            let server = self
                .server
                .lock()
                .map_err(|_| Error::new("internal", "notification host poisoned", false))?
                .take();
            // Joining the compatibility host belongs to the host's bounded blocking pool.
            if let Some(server) = server {
                let _ = server.call(Control::Stop).await;
                tokio::task::spawn_blocking(move || drop(server))
                    .await
                    .map_err(|_| Error::new("internal", "notification host stop failed", false))?;
            }
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}
/// Read-only clock projection from a host's existing desktop service.
/// Tray pixmaps and unrelated sensor state never enter this Source.
pub struct Clock {
    publisher: Publisher,
    _service: Arc<bead_services::Desktop>,
    subscription: Mutex<Option<bead_services::Subscription>>,
}
fn clock_data(snapshot: &bead_services::Snapshot) -> Json {
    serde_json::json!({"clock":snapshot.clock,"date":snapshot.date})
}
impl Clock {
    pub fn attach(service: Arc<bead_services::Desktop>) -> Result<Self> {
        let initial = service.snapshot();
        let publisher = Publisher::at(
            format!("clock-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            initial.revision,
            clock_data(&initial),
        )?;
        let changed = publisher.clone();
        let subscription = service.subscribe(move |snapshot| {
            if changed
                .publish_revision(snapshot.revision, clock_data(&snapshot))
                .is_err()
            {
                let _ = changed.disconnect();
            }
        });
        Ok(Self {
            publisher,
            _service: service,
            subscription: Mutex::new(Some(subscription)),
        })
    }
}
impl Source for Clock {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, _method: String, _params: Json) -> Reply<'_> {
        Box::pin(async {
            Err(Error::new(
                "unknown_method",
                "clock source is read-only",
                false,
            ))
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
    fn stop(&self) -> Reply<'_> {
        Box::pin(async {
            self.subscription
                .lock()
                .map_err(|_| Error::new("internal", "clock subscription unavailable", false))?
                .take();
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}

/// Read-only projection of an existing hosted Desktop. Select fields before
/// serialization so a small view does not copy or retain unrelated tray/menu data.
/// Dropping this handle removes its subscription; it never starts/stops a backend.
pub struct DesktopView {
    publisher: Publisher,
    _service: Arc<bead_services::Desktop>,
    subscription: Mutex<Option<bead_services::Subscription>>,
}
impl DesktopView {
    pub fn attach(
        service: Arc<bead_services::Desktop>,
        project: fn(&bead_services::Snapshot) -> Json,
    ) -> Result<Self> {
        let initial = service.snapshot();
        let publisher = Publisher::at(
            format!("desktop-view-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            initial.revision,
            project(&initial),
        )?;
        let changed = publisher.clone();
        let subscription = service.subscribe(move |snapshot| {
            if changed
                .publish_revision(snapshot.revision, project(&snapshot))
                .is_err()
            {
                let _ = changed.disconnect();
            }
        });
        Ok(Self {
            publisher,
            _service: service,
            subscription: Mutex::new(Some(subscription)),
        })
    }
}
impl Source for DesktopView {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, _method: String, _params: Json) -> Reply<'_> {
        Box::pin(async {
            Err(Error::new(
                "unknown_method",
                "desktop view is read-only",
                false,
            ))
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
    fn stop(&self) -> Reply<'_> {
        Box::pin(async {
            self.subscription
                .lock()
                .map_err(|_| {
                    Error::new("internal", "desktop view subscription unavailable", false)
                })?
                .take();
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}

#[cfg(test)]
mod clock_projection_tests {
    use super::*;
    #[test]
    fn clock_source_excludes_bulk_tray_payloads() {
        let mut snapshot = bead_services::Snapshot::default();
        snapshot.clock = "12:34".into();
        snapshot.date = Some(bead_services::LocalDate {
            year: 2024,
            month: 2,
            day: 29,
        });
        snapshot.tray = vec![bead_services::TrayItem {
            icon_pixmap: vec![(1024, 1024, vec![255; 4 * 1024 * 1024])],
            ..Default::default()
        }];
        let data = clock_data(&snapshot);
        assert_eq!(data["date"]["day"], 29);
        assert!(serde_json::to_vec(&data).unwrap().len() < 128);
        assert!(data.get("tray").is_none());
    }
    #[test]
    fn stopping_clock_detaches_only_its_subscription() {
        let mut config = bead_services::Config::default();
        config.audio = false;
        config.bus = false;
        let host = bead_services::Desktop::start_config(config);
        let source = Clock::attach(host.clone()).unwrap();
        assert_eq!(host.subscription_count(), 1);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        assert_eq!(
            runtime
                .block_on(source.invoke("audio.volume".into(), Json::Null))
                .unwrap_err()
                .code,
            "unknown_method"
        );
        runtime.block_on(source.stop()).unwrap();
        assert_eq!(host.subscription_count(), 0);
        assert!(!source.snapshot().connected);
        // A sibling view can still acquire and release its own subscription.
        let sibling = Clock::attach(host.clone()).unwrap();
        assert_eq!(host.subscription_count(), 1);
        drop(sibling);
        assert_eq!(host.subscription_count(), 0);
        host.stop();
    }
}

#[cfg(test)]
mod desktop_view_tests {
    use super::*;
    #[test]
    fn stopping_a_read_only_projection_detaches_only_its_own_subscription() {
        let mut config = bead_services::Config::default();
        config.audio = false;
        config.bus = false;
        let host = bead_services::Desktop::start_config(config);
        let source =
            DesktopView::attach(host.clone(), |s| serde_json::json!({"battery":s.battery}))
                .unwrap();
        let sibling =
            DesktopView::attach(host.clone(), |s| serde_json::json!({"network":s.network}))
                .unwrap();
        assert_eq!(host.subscription_count(), 2);
        assert_eq!(source.snapshot().data.as_object().unwrap().len(), 1);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        assert_eq!(
            runtime
                .block_on(source.invoke("audio.volume".into(), Json::Null))
                .unwrap_err()
                .code,
            "unknown_method"
        );
        runtime.block_on(source.stop()).unwrap();
        assert!(!source.snapshot().connected);
        assert_eq!(host.subscription_count(), 1);
        drop(sibling);
        assert_eq!(host.subscription_count(), 0);
        let fresh = DesktopView::attach(
            host.clone(),
            |s| serde_json::json!({"bluetooth":s.bluetooth}),
        )
        .unwrap();
        assert!(fresh.snapshot().connected);
        drop(fresh);
        assert_eq!(host.subscription_count(), 0);
        host.stop();
    }
}

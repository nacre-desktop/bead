// SPDX-License-Identifier: MIT OR Apache-2.0
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
    pub fn private(socket: PathBuf, history: PathBuf) -> Result<Self> {
        let publisher = Publisher::new(
            format!("notifications-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            json!({}),
        )?;
        let changed = publisher.clone();
        let stopped = publisher.clone();
        let server = Server::start_with_lifecycle(
            socket,
            history,
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
/// Existing composite service as a Source. The default bar retains its typed path;
/// this adapter is also usable by other in-process shells and headless hosts.
pub struct Desktop {
    publisher: Publisher,
    service: Arc<bead_services::Desktop>,
    _subscription: bead_services::Subscription,
}
impl Desktop {
    pub fn attach(service: Arc<bead_services::Desktop>) -> Result<Self> {
        let publisher = Publisher::new(
            format!("desktop-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            json!({}),
        )?;
        let changed = publisher.clone();
        let subscription = service.subscribe(move |snapshot| {
            match serde_json::to_value(&*snapshot)
                .map_err(|_| ())
                .and_then(|value| changed.publish(value).map_err(|_| ()))
            {
                Ok(()) => {}
                Err(()) => {
                    let _ = changed.disconnect();
                }
            }
        });
        Ok(Self {
            publisher,
            service,
            _subscription: subscription,
        })
    }
}
impl Source for Desktop {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, method: String, params: Json) -> Reply<'_> {
        Box::pin(async move {
            let action = match method.as_str() {
                "audio.volume" => bead_services::Action::Volume(
                    params["value"]
                        .as_f64()
                        .ok_or_else(|| Error::new("validation", "invalid volume", false))?,
                ),
                "audio.mute" => bead_services::Action::Mute(
                    params["value"]
                        .as_bool()
                        .ok_or_else(|| Error::new("validation", "invalid mute", false))?,
                ),
                _ => {
                    return Err(Error::new(
                        "unknown_method",
                        "unknown desktop source action",
                        false,
                    ));
                }
            };
            let service = self.service.clone();
            tokio::task::spawn_blocking(move || service.call(action))
                .await
                .map_err(|_| Error::new("internal", "desktop call worker failed", false))?
                .map_err(|_| Error::new("unavailable", "desktop action unavailable", true))?;
            Ok(Json::Null)
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    } // existing service owns its event-driven reconnect loop
    fn stop(&self) -> Reply<'_> {
        Box::pin(async move {
            let service = self.service.clone();
            tokio::task::spawn_blocking(move || service.stop())
                .await
                .map_err(|_| Error::new("internal", "desktop stop worker failed", false))?;
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}
/// Host-agnostic service handle as a Source, on the caller's existing runtime.
pub struct HostedNotifications {
    publisher: Publisher,
    handle: Arc<bead_services::notification_bus::Handle>,
    forwarding: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl HostedNotifications {
    pub fn attach(handle: bead_services::notification_bus::Handle) -> Result<Self> {
        let publisher = Publisher::new(
            format!("notifications-{}", EPOCH.fetch_add(1, Ordering::Relaxed)),
            handle.snapshot(),
        )?;
        let mut updates = handle.subscribe();
        let changed = publisher.clone();
        let forwarding = tokio::spawn(async move {
            while updates.changed().await.is_ok() {
                let value = updates.borrow_and_update().clone();
                if changed.publish(value).is_err() {
                    break;
                }
            }
            let _ = changed.disconnect();
        });
        Ok(Self {
            publisher,
            handle: Arc::new(handle),
            forwarding: Mutex::new(Some(forwarding)),
        })
    }
}
impl Source for HostedNotifications {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, method: String, params: Json) -> Reply<'_> {
        Box::pin(async move {
            self.handle
                .call(action(&method, &params)?)
                .await
                .map_err(|_| Error::new("unavailable", "notification action unavailable", true))?;
            Ok(Json::Null)
        })
    }
    fn reconnect(&self) -> Reply<'_> {
        Box::pin(async {
            Err(Error::new(
                "unavailable",
                "host must inject a replacement service handle",
                true,
            ))
        })
    }
    fn stop(&self) -> Reply<'_> {
        Box::pin(async move {
            let _ = self.handle.call(Control::Stop).await;
            let task = self
                .forwarding
                .lock()
                .map_err(|_| Error::new("internal", "source task poisoned", false))?
                .take();
            if let Some(mut task) = task {
                match tokio::time::timeout(std::time::Duration::from_secs(3), &mut task).await {
                    Ok(Ok(())) => {}
                    _ => {
                        task.abort();
                        return Err(Error::new(
                            "timeout",
                            "notification source stop timed out",
                            true,
                        ));
                    }
                }
            }
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}
impl Drop for HostedNotifications {
    fn drop(&mut self) {
        if let Ok(mut task) = self.forwarding.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
}

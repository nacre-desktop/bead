// SPDX-License-Identifier: GPL-3.0-only
//! Nacre projection of a private native notification server.
use bead::{
    core::Value,
    core::source::{Applied, Mirror, Source},
    services::notification_bus::Control,
    wayland::UiEvent,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
type ControlFn = Arc<dyn Fn(Control) -> Result<(), String> + Send + Sync>;
const POLICY_PATH: &str = "/org/freedesktop/Notifications";
const NACRE_POLICY_INTERFACE: &str = "org.nacre.Bead.Notifications";

struct NacreNotificationPolicy(bead::services::notification_bus::Policy);
#[zbus::interface(name = "org.nacre.Bead.Notifications")]
impl NacreNotificationPolicy {
    fn set_do_not_disturb(&self, value: bool) -> zbus::fdo::Result<()> {
        self.0.set_do_not_disturb(value)
    }
    fn inhibit(
        &self,
        reason: String,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        let owner = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::Failed("missing owner".into()))?
            .to_string();
        self.0.inhibit(owner, reason)
    }
    fn uninhibit(
        &self,
        cookie: u32,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<()> {
        let owner = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::Failed("missing owner".into()))?
            .to_string();
        self.0.uninhibit(&owner, cookie)
    }
    fn get_history(&self) -> zbus::fdo::Result<String> {
        self.0.history()
    }
    fn invoke_action(&self, id: u32, key: String) -> zbus::fdo::Result<()> {
        self.0.invoke_action(id, key)
    }
    fn dismiss(&self, id: u32) -> zbus::fdo::Result<()> {
        self.0.dismiss(id)
    }
    fn clear_history(&self) -> zbus::fdo::Result<()> {
        self.0.clear_history()
    }
    fn show_osd(&self, channel: String, value: f64, muted: bool) -> zbus::fdo::Result<()> {
        self.0.show_osd(channel, value, muted)
    }
    #[zbus(property)]
    fn inhibited(&self) -> zbus::fdo::Result<bool> {
        self.0.inhibited()
    }
}
struct NacreNotificationPolicyExtension;
impl bead::services::notification_bus::PolicyExtension for NacreNotificationPolicyExtension {
    fn register(
        &self,
        connection: zbus::Connection,
        policy: bead::services::notification_bus::Policy,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        Box::pin(async move {
            let installed = connection
                .object_server()
                .at(POLICY_PATH, NacreNotificationPolicy(policy))
                .await
                .map_err(|error| error.to_string())?;
            if !installed {
                return Err("Nacre notification compatibility interface already installed".into());
            }
            Ok(())
        })
    }
    fn unregister(
        &self,
        connection: zbus::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>> {
        Box::pin(async move {
            connection
                .object_server()
                .remove::<NacreNotificationPolicy, _>(POLICY_PATH)
                .await
                .map_err(|error| error.to_string())?;
            Ok(())
        })
    }
    fn interface_name(&self) -> &'static str {
        NACRE_POLICY_INTERFACE
    }
}
fn nacre_notification_options() -> bead::services::notification_bus::Options {
    bead::services::notification_bus::Options {
        information: bead::services::notification_bus::ServerInformation {
            name: "Bead".into(),
            vendor: "Nacre".into(),
            version: "0.1.0".into(),
            spec_version: "1.2".into(),
        },
        policy_extension: Some(Arc::new(NacreNotificationPolicyExtension)),
    }
}
pub type Wakeup = Arc<dyn Fn(UiEvent) + Send + Sync>;
/// Keeps a view subscription alive; dropping it unsubscribes.
pub type Handle = Box<dyn std::any::Any + Send>;
/// What the shell's notice and OSD surfaces need from a notification backend: the
/// private compatibility owner below or the in-process daemon consumer in `notice_service`.
pub trait Notices: Send + Sync {
    fn subscribe(&self, wakeup: Wakeup, primary: bool) -> Handle;
    /// A bar session that drives the transient `notices`/`osd` panel views: it receives
    /// only open/close offers, never models. Hosts without panel views ignore it.
    fn drive(&self, _wakeup: Wakeup) -> Handle {
        Box::new(())
    }
    /// The panel view now shown (None when closed), for suppression bookkeeping.
    fn panel_changed(&self, _view: Option<&str>) {}
    /// `key` is `id[|revision]` of the card under the pointer, or `osd`.
    fn hover(&self, key: &str, on: bool);
    fn action(&self, name: &str, arg: &Value) -> Result<(), String>;
    fn audio_changed(&self, audio: &bead::services::Audio);
    fn feedback(&self, args: &[serde_json::Value]) -> Result<serde_json::Value, &'static str>;
    /// Harness-only OSD preview; it never asks the daemon to emit a device event.
    fn preview(&self, _args: &[serde_json::Value]) -> Result<serde_json::Value, &'static str> {
        Err("OSD preview is unavailable")
    }
    fn command(&self, call: &bead::services::ipc::Call) -> Result<serde_json::Value, &'static str>;
}
struct Shared {
    last: serde_json::Value,
    mirror: Mirror,
    wakeups: BTreeMap<u64, (Wakeup, bool)>,
    next: u64,
    audio: Option<bead::services::Audio>,
}
pub struct Subscription {
    shared: std::sync::Weak<Mutex<Shared>>,
    id: u64,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade()
            && let Ok(mut s) = shared.lock()
        {
            s.wakeups.remove(&self.id);
        }
    }
}
pub struct Owner {
    _source: Arc<dyn Source>,
    control: ControlFn,
    _source_subscription: bead::core::source::Subscription,
    shared: Arc<Mutex<Shared>>,
}
impl Owner {
    pub fn start(socket: PathBuf, history: PathBuf) -> Result<Self, String> {
        let shared = Arc::new(Mutex::new(Shared {
            mirror: Mirror::new(std::time::Instant::now()),
            last: serde_json::json!({"rows":[],"popup_count":0,"history_count":0,"history_ready":false,"error":"","timers":[],"dnd":false,"osd":null}),
            wakeups: BTreeMap::new(),
            next: 0,
            audio: None,
        }));
        let state = shared.clone();
        let (server, control, address): (Arc<dyn Source>, ControlFn, Option<String>) =
            if let Some(path) = std::env::var_os("BEAD_PRIVATE_NOTIFICATION_FIXTURE") {
                if std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() != Ok("1") {
                    return Err("fixture requires private proof environment".into());
                }
                let bytes = std::fs::read(path).map_err(|_| "fixture read failed")?;
                if bytes.len() > 1_048_576 {
                    return Err("fixture exceeds protocol-sized bound".into());
                }
                let data = serde_json::from_slice(&bytes).map_err(|_| "fixture JSON invalid")?;
                let source = bead_test::source::FakeSource::new(data).map_err(|e| e.to_string())?;
                (
                    Arc::new(source),
                    Arc::new(|_| Err("fixture actions disabled".into())),
                    None,
                )
            } else {
                let server = Arc::new(
                    bead::source::Notifications::private_with_options(
                        socket,
                        history,
                        nacre_notification_options(),
                    )
                    .map_err(|e| e.to_string())?,
                );
                let address = server.address().map_err(|e| e.to_string())?;
                let controller = server.clone();
                (
                    server,
                    Arc::new(move |action| controller.control(action)),
                    Some(address),
                )
            };
        let subscription = server
            .subscribe(Arc::new(move |source_snapshot| {
                let snapshot = &*source_snapshot.data;
                let wakeups = if let Ok(mut state) = state.lock() {
                    if !matches!(
                        state
                            .mirror
                            .accept(source_snapshot.clone(), std::time::Instant::now()),
                        Ok(Applied::Updated)
                    ) {
                        return;
                    }
                    state.last = snapshot.clone();
                    state.wakeups.values().cloned().collect::<Vec<_>>()
                } else {
                    vec![]
                };
                for (wakeup, primary) in wakeups {
                    if primary {
                        wakeup(UiEvent::CompanionVisibility(snapshot["osd"].is_object()));
                    }
                    let mut models = values(snapshot);
                    if !snapshot["osd"].is_object() {
                        models.retain(|key, _| {
                            !matches!(
                                key.as_str(),
                                "osd.kind" | "osd.value" | "osd.muted" | "osd.label" | "osd.device"
                            )
                        });
                    }
                    wakeup(UiEvent::Model(models));
                }
            }))
            .map_err(|e| e.to_string())?;
        if let Some(address) = address {
            println!("NOTIFICATIONS_PRIVATE address={address}");
        } else {
            println!("NOTIFICATIONS_FIXTURE source=true");
        }
        Ok(Self {
            _source: server,
            control,
            shared,
            _source_subscription: subscription,
        })
    }
    fn control(&self, action: Control) -> Result<(), String> {
        (self.control)(action)
    }
}
impl Notices for Owner {
    fn subscribe(&self, wakeup: Wakeup, primary: bool) -> Handle {
        let (last, id) = if let Ok(mut state) = self.shared.lock() {
            state.next += 1;
            let id = state.next;
            state.wakeups.insert(id, (wakeup.clone(), primary));
            (state.last.clone(), id)
        } else {
            (serde_json::Value::Null, 0)
        };
        if primary {
            wakeup(UiEvent::CompanionVisibility(last["osd"].is_object()));
        }
        wakeup(UiEvent::Model(values(&last)));
        Box::new(Subscription {
            shared: Arc::downgrade(&self.shared),
            id,
        })
    }
    fn audio_changed(&self, audio: &bead::services::Audio) {
        let previous = if let Ok(mut state) = self.shared.lock() {
            state.audio.replace(audio.clone())
        } else {
            None
        };
        if let Some(previous) = previous {
            for (channel, old, old_mute, new, muted) in [
                (
                    "volume",
                    previous.volume,
                    previous.muted,
                    audio.volume,
                    audio.muted,
                ),
                (
                    "microphone",
                    previous.mic_volume,
                    previous.mic_muted,
                    audio.mic_volume,
                    audio.mic_muted,
                ),
            ] {
                if old.is_some() && new.is_some() && (old != new || old_mute != muted) {
                    let _ = self.control(Control::Osd {
                        channel: channel.into(),
                        value: new.unwrap_or(0.).clamp(0., 1.),
                        muted: muted.unwrap_or(false),
                    });
                }
            }
        }
    }
    fn hover(&self, key: &str, value: bool) {
        if let Some(id) = key.split('|').next().and_then(|id| id.parse().ok()) {
            let _ = self.control(Control::Hover(id, value));
        }
    }
    fn feedback(&self, args: &[serde_json::Value]) -> Result<serde_json::Value, &'static str> {
        if args.len() != 3 {
            return Err("OSD show needs channel, normalized level and muted boolean");
        }
        let channel = args[0]
            .as_str()
            .filter(|s| ["volume", "brightness", "microphone", "capslock"].contains(s))
            .ok_or("unknown OSD channel")?;
        let value = args[1]
            .as_f64()
            .or_else(|| args[1].as_str().and_then(|s| s.parse().ok()))
            .filter(|n| n.is_finite() && (0. ..=1.).contains(n))
            .ok_or("OSD level must be within0..1")?;
        let muted = args[2]
            .as_bool()
            .or_else(|| args[2].as_str().and_then(|s| s.parse().ok()))
            .ok_or("OSD muted must be boolean")?;
        self.control(Control::Osd {
            channel: channel.into(),
            value,
            muted,
        })
        .map_err(|_| "notification server stopped")?;
        Ok(serde_json::Value::Null)
    }
    fn command(&self, call: &bead::services::ipc::Call) -> Result<serde_json::Value, &'static str> {
        match call.method.as_str() {
            "counts" => {
                let state = self
                    .shared
                    .lock()
                    .map_err(|_| "notification state unavailable")?;
                Ok(serde_json::Value::String(serde_json::json!({"popups":state.last["popup_count"],"retained":state.last["history_count"],"timers":state.last["timers"],"historyReady":state.last["history_ready"],"error":state.last["error"]}).to_string()))
            }
            "clear" => self
                .control(Control::HidePopups)
                .map(|_| serde_json::Value::Null)
                .map_err(|_| "notification server stopped"),
            "clearHistory" => self
                .control(Control::ClearHistory)
                .map(|_| serde_json::Value::Null)
                .map_err(|_| "notification server stopped"),
            "dismissKey" => {
                let id = call
                    .args
                    .first()
                    .and_then(|a| a.as_str())
                    .and_then(|s| s.parse().ok())
                    .ok_or("invalid notification key")?;
                self.control(Control::Dismiss(id))
                    .map(|_| serde_json::Value::Null)
                    .map_err(|_| "notification server stopped")
            }
            _ => Err("unknown notification command"),
        }
    }
    fn action(&self, name: &str, arg: &Value) -> Result<(), String> {
        let text = if let Value::Text(s) = arg {
            s.as_str()
        } else {
            ""
        };
        let command = match name {
            "notifications.details" | "notifications.card" => {
                Control::Details(text.parse().map_err(|_| "invalid notification ID")?)
            }
            "notifications.dismiss" => {
                Control::Dismiss(text.parse().map_err(|_| "invalid notification ID")?)
            }
            "notifications.action" => {
                let (id, key) = text.split_once('|').ok_or("action needs ID/key")?;
                Control::Action(
                    id.parse().map_err(|_| "invalid notification ID")?,
                    key.into(),
                )
            }
            "notifications.dnd" => Control::Dnd(matches!(arg, Value::Bool(true))),
            "notifications.clear" => Control::ClearHistory,
            _ => return Err("unknown notification action".into()),
        };
        self.control(command)
    }
}
pub fn values(snapshot: &serde_json::Value) -> BTreeMap<String, Value> {
    let rows = snapshot["rows"].as_array().cloned().unwrap_or_default();
    let osd = &snapshot["osd"];
    let kind = osd["channel"].as_str().unwrap_or("volume");
    BTreeMap::from([
        (
            "notifications.open".into(),
            Value::Bool(snapshot["popup_count"].as_u64().unwrap_or(0) > 0),
        ),
        ("notifications.rows".into(), Value::list(rows)),
        (
            "notifications.dnd".into(),
            Value::Bool(snapshot["dnd"].as_bool().unwrap_or(false)),
        ),
        (
            "notifications.history_count".into(),
            Value::Number(snapshot["history_count"].as_u64().unwrap_or(0) as f64),
        ),
        ("osd.open".into(), Value::Bool(osd.is_object())),
        ("osd.kind".into(), Value::Text(kind.into())),
        (
            "osd.value".into(),
            Value::Number(osd["value"].as_f64().unwrap_or(0.).clamp(0., 1.)),
        ),
        (
            "osd.muted".into(),
            Value::Bool(osd["muted"].as_bool().unwrap_or(false)),
        ),
        ("osd.label".into(), Value::Text(String::new())),
        ("osd.device".into(), Value::Text(String::new())),
    ])
}
#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn local_and_fake_sources_project_identical_luau_bindings()
    -> Result<(), Box<dyn std::error::Error>> {
        let now = std::time::Instant::now();
        let data = serde_json::json!({"rows":[{"id":"1","app":"Example","summary":"Hello","body":"Fixture","actions":[],"expanded":false}],"popup_count":1,"history_count":1,"dnd":true,"osd":{"channel":"volume","value":0.5,"muted":false}});
        let local = bead::core::source::Publisher::new("local".into(), data.clone())?;
        let fake = bead_test::source::FakeSource::new(data)?;
        let mut left = Mirror::new(now);
        let mut right = Mirror::new(now);
        left.accept(local.snapshot(), now)?;
        right.accept(fake.snapshot(), now)?;
        assert_eq!(
            values(&left.snapshot().ok_or("local")?.data),
            values(&right.snapshot().ok_or("fake")?.data)
        );
        Ok(())
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, time::Duration};

    #[test]
    fn nacre_policy_legacy_wire_api_delegates_to_neutral_policy()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("nacre-policy-wire-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let bus = bead::services::notification_bus::PrivateBus::start(root.join("bus"))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let service_connection = bus.connect().await?;
            let service = bead::services::notification_bus::start_with_options(
                bead::services::notification_bus::Config {
                    connection: service_connection.clone(),
                    history: root.join("history.json"),
                },
                nacre_notification_options(),
            )
            .await?;
            let client = bus.connect().await?;
            let legacy = zbus::Proxy::new(
                &client,
                "org.freedesktop.Notifications",
                POLICY_PATH,
                NACRE_POLICY_INTERFACE,
            )
            .await?;
            let neutral = zbus::Proxy::new(
                &client,
                "org.freedesktop.Notifications",
                POLICY_PATH,
                "org.bead.Notifications",
            )
            .await?;
            let standard = zbus::Proxy::new(
                &client,
                "org.freedesktop.Notifications",
                POLICY_PATH,
                "org.freedesktop.Notifications",
            )
            .await?;
            let info: (String, String, String, String) =
                standard.call("GetServerInformation", &()).await?;
            assert_eq!(
                info,
                ("Bead".into(), "Nacre".into(), "0.1.0".into(), "1.2".into())
            );
            let introspect = zbus::Proxy::new(
                &client,
                "org.freedesktop.Notifications",
                POLICY_PATH,
                "org.freedesktop.DBus.Introspectable",
            )
            .await?;
            let xml: String = introspect.call("Introspect", &()).await?;
            for member in [
                "SetDoNotDisturb",
                "Inhibit",
                "Uninhibit",
                "GetHistory",
                "InvokeAction",
                "Dismiss",
                "ClearHistory",
                "ShowOsd",
                "Inhibited",
            ] {
                assert!(xml.contains(member), "legacy member missing: {member}");
            }
            legacy.call::<_, _, ()>("SetDoNotDisturb", &(true,)).await?;
            tokio::time::timeout(Duration::from_secs(1), async {
                while !neutral.get_property::<bool>("Inhibited").await? {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Ok::<(), zbus::Error>(())
            })
            .await??;
            let cookie: u32 = legacy.call("Inhibit", &("compatibility test",)).await?;
            let foreign_connection = bus.connect().await?;
            let foreign = zbus::Proxy::new(
                &foreign_connection,
                "org.freedesktop.Notifications",
                POLICY_PATH,
                "org.bead.Notifications",
            )
            .await?;
            assert!(
                foreign
                    .call::<_, _, ()>("Uninhibit", &(cookie,))
                    .await
                    .is_err()
            );
            neutral.call::<_, _, ()>("Uninhibit", &(cookie,)).await?;
            legacy
                .call::<_, _, ()>("SetDoNotDisturb", &(false,))
                .await?;
            tokio::time::timeout(Duration::from_secs(1), async {
                while neutral.get_property::<bool>("Inhibited").await? {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Ok::<(), zbus::Error>(())
            })
            .await??;
            service.stop().await?;
            assert!(
                service_connection
                    .object_server()
                    .interface::<_, NacreNotificationPolicy>(POLICY_PATH)
                    .await
                    .is_err()
            );
            tokio::time::timeout(Duration::from_secs(1), async {
                while zbus::fdo::DBusProxy::new(&client)
                    .await?
                    .name_has_owner("org.freedesktop.Notifications".try_into()?)
                    .await?
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                Ok::<(), zbus::Error>(())
            })
            .await??;
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
        Ok(())
    }
}

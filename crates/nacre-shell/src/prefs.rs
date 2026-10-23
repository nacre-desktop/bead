// SPDX-License-Identifier: GPL-3.0-only
//! B0035: the owner's Nacre settings, read-only. Follows the published
//! `settings.store.desktop` and `settings.store.dashboard` topics, applies Nacre's
//! defaults and republishes one small effective-preferences Source, so every
//! consumer reads the same decoded values and wakes on a change without a restart.
//! Nacre keys live only here (ADR 0004); the framework gets the generic motion switch,
//! text style and shell control (`ShellControl`).
use bead::core::source::{Callback, Publisher, Result, Snapshot, Source, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::sync::{Arc, Mutex};

pub const DESKTOP: &str = "settings.store.desktop";
pub const DASHBOARD: &str = "settings.store.dashboard";
/// Nacre's `NacreDashboardRegistry.core` order, the default of `dashboard.tabs`.
const CORE: [&str; 4] = ["today", "media", "weather", "system"];
/// Tab lists written before the core set was fixed; QML treats them as unset.
/// `NacreTokens.qml` defaults: families and the body size `textSize` is relative to.
pub const INTERFACE_FONT: &str = "IBM Plex Sans";
pub const MONO_FONT: &str = "JetBrainsMono Nerd Font";
const BODY_SIZE: i32 = 12;
const LEGACY: [&str; 5] = ["today", "weather", "media", "system", "focus"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    /// `desktop.reduceMotion` or `desktop.animations == false` (QML `motionEnabled`).
    pub reduce_motion: bool,
    /// `desktop.weatherFahrenheit`.
    pub fahrenheit: bool,
    /// `dashboard.tabs` filtered to the core pages, in the owner's order.
    pub core_tabs: Vec<String>,
    /// `dashboardEnabled`, `controlCenterEnabled`, `topBarEnabled`, `frameSheen`:
    /// QML treats anything but `false` as on.
    pub dashboard_enabled: bool,
    pub control_center_enabled: bool,
    pub top_bar_enabled: bool,
    pub frame_sheen: bool,
    /// `interfaceFont` / `monoFont`: empty means Nacre's default family.
    pub interface_font: String,
    pub mono_font: String,
    /// `textSize` minus Nacre's body size (12): added to every text token.
    pub text_offset: i32,
}
impl Default for Prefs {
    fn default() -> Self {
        Self::decode(&Json::Null, &Json::Null)
    }
}
/// The `data` member of a published `SettingsStore`, or null for anything else.
fn store<'a>(snapshot: &'a Snapshot, id: &str) -> &'a Json {
    let document = &*snapshot.data;
    if document.get("store").and_then(Json::as_str) == Some(id) {
        &document["data"]
    } else {
        &Json::Null
    }
}
impl Prefs {
    pub fn decode(desktop: &Json, dashboard: &Json) -> Self {
        let flag = |key: &str| desktop.get(key).and_then(Json::as_bool);
        let saved: Vec<&str> = dashboard
            .get("tabs")
            .and_then(Json::as_array)
            .map(|a| a.iter().filter_map(Json::as_str).collect())
            .unwrap_or_default();
        let mut core: Vec<String> = Vec::new();
        if saved.is_empty() || saved == LEGACY {
            core.extend(CORE.map(String::from));
        } else {
            for name in saved.iter().filter(|n| CORE.contains(n)) {
                if !core.iter().any(|c| c == name) {
                    core.push((*name).into());
                }
            }
        }
        if core.is_empty() {
            core.push("today".into());
        }
        let on = |key: &str| flag(key) != Some(false);
        let family = |key: &str| {
            desktop
                .get(key)
                .and_then(Json::as_str)
                .map(str::trim)
                .filter(|f| !f.is_empty() && f.len() <= 100 && !f.chars().any(char::is_control))
                .unwrap_or_default()
                .to_owned()
        };
        // JS `value || body`: zero and non-numbers fall back to the body size.
        let size = desktop
            .get("textSize")
            .and_then(Json::as_f64)
            .filter(|v| v.is_finite() && *v != 0.)
            .map_or(BODY_SIZE, |v| v.round().clamp(1., 100.) as i32);
        Self {
            dashboard_enabled: on("dashboardEnabled"),
            control_center_enabled: on("controlCenterEnabled"),
            top_bar_enabled: on("topBarEnabled"),
            frame_sheen: on("frameSheen"),
            interface_font: family("interfaceFont"),
            mono_font: family("monoFont"),
            text_offset: size - BODY_SIZE,
            reduce_motion: flag("reduceMotion") == Some(true) || flag("animations") == Some(false),
            fahrenheit: flag("weatherFahrenheit") == Some(true),
            core_tabs: core,
        }
    }
    /// Generic text style: Luau markup names `IBM Plex Sans` and
    /// `JetBrainsMono Nerd Font` literally, so those names map to the owner's choice.
    pub fn text_style(&self) -> bead::ui::text_style::TextStyle {
        let alias =
            |from: &str, to: &str| (!to.is_empty() && to != from).then(|| (from.into(), to.into()));
        bead::ui::text_style::TextStyle {
            aliases: [
                alias(INTERFACE_FONT, &self.interface_font),
                alias(MONO_FONT, &self.mono_font),
            ]
            .into_iter()
            .flatten()
            .collect(),
            size_offset: self.text_offset as f32,
        }
    }
    /// Host stores the bar markup reads to drop the button of a disabled panel (the
    /// control center has no single bar button; its opens are refused by the gate).
    pub fn bar_stores(&self) -> std::collections::BTreeMap<String, bead::core::Value> {
        use bead::core::Value::Bool;
        [("bar.dashboard.enabled".into(), Bool(self.dashboard_enabled))].into()
    }
    pub fn from_source(source: &dyn Source) -> Self {
        serde_json::from_value((*source.snapshot().data).clone()).unwrap_or_default()
    }
}

pub type Apply = Arc<dyn Fn(&Prefs) + Send + Sync>;

pub struct Service {
    _subscriptions: Vec<Subscription>,
}
struct Shared(Publisher);
impl Source for Shared {
    fn snapshot(&self) -> Arc<Snapshot> {
        self.0.snapshot()
    }
    fn subscribe(&self, callback: Callback) -> Result<Subscription> {
        self.0.subscribe(callback)
    }
    fn invoke(&self, _: String, _: Json) -> bead::core::source::Reply<'_> {
        Box::pin(async {
            Err(bead::core::source::Error::new(
                "unsupported",
                "preferences are read-only",
                false,
            ))
        })
    }
    fn reconnect(&self) -> bead::core::source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
    fn stop(&self) -> bead::core::source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
}
impl Service {
    /// `apply` runs once at start and then only when the decoded values change. The
    /// returned Source carries the effective [`Prefs`] for views to follow.
    pub fn start(
        desktop: Arc<dyn Source>,
        dashboard: Arc<dyn Source>,
        apply: Apply,
    ) -> Result<(Self, Arc<dyn Source>)> {
        let first = Prefs::default();
        let publisher = Publisher::new("prefs".into(), serde_json::to_value(&first).unwrap())?;
        let effective: Arc<dyn Source> = Arc::new(Shared(publisher.clone()));
        let current = Arc::new(Mutex::new(None::<Prefs>));
        let update: Callback = {
            let (desktop, dashboard) = (desktop.clone(), dashboard.clone());
            Arc::new(move |_| {
                let next = Prefs::decode(
                    store(&desktop.snapshot(), "desktop"),
                    store(&dashboard.snapshot(), "dashboard"),
                );
                let mut held = current.lock().unwrap_or_else(|p| p.into_inner());
                if held.as_ref() == Some(&next) {
                    return;
                }
                apply(&next);
                let _ = publisher.publish(serde_json::to_value(&next).unwrap());
                *held = Some(next);
            })
        };
        let subscriptions = vec![
            desktop.subscribe(update.clone())?,
            dashboard.subscribe(update)?,
        ];
        Ok((
            Self {
                _subscriptions: subscriptions,
            },
            effective,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_doc(id: &str, data: Json) -> Json {
        json!({"store": id, "rev": 1, "exists": true, "data": data, "dirty": false})
    }
    #[test]
    fn defaults_match_nacre_when_stores_are_empty_or_wrong_typed() {
        let d = Prefs::default();
        assert!(!d.reduce_motion && !d.fahrenheit);
        assert_eq!(d.core_tabs, ["today", "media", "weather", "system"]);
        let bad = json!({"reduceMotion": "yes", "animations": 0, "weatherFahrenheit": 1});
        assert_eq!(Prefs::decode(&bad, &json!({"tabs": 5})), d);
    }
    #[test]
    fn motion_is_off_for_reduce_motion_or_animations_false() {
        let motion = |v: Json| Prefs::decode(&v, &Json::Null).reduce_motion;
        assert!(motion(json!({"reduceMotion": true})));
        assert!(motion(json!({"animations": false})));
        assert!(!motion(json!({"animations": true, "reduceMotion": false})));
    }
    #[test]
    fn core_tabs_keep_owner_order_drop_unknown_and_never_end_empty() {
        let tabs = |v: Json| Prefs::decode(&Json::Null, &v).core_tabs;
        assert_eq!(
            tabs(json!({"tabs": ["system", "bogus", "today", "system", "focus"]})),
            ["system", "today"]
        );
        assert_eq!(tabs(json!({"tabs": ["focus"]})), ["today"]);
        assert_eq!(
            tabs(json!({"tabs": ["today", "weather", "media", "system", "focus"]})),
            CORE
        );
    }
    #[test]
    fn panel_flags_sheen_fonts_and_text_size_follow_qml_defaults() {
        let d = Prefs::default();
        assert!(d.dashboard_enabled && d.control_center_enabled && d.top_bar_enabled);
        assert!(d.frame_sheen && d.interface_font.is_empty() && d.text_offset == 0);
        assert_eq!(d.text_style(), Default::default());
        // `!== false` semantics: only a real false disables.
        let odd = Prefs::decode(
            &json!({"dashboardEnabled": 0, "topBarEnabled": null, "frameSheen": "no",
                    "interfaceFont": 7, "monoFont": "  ", "textSize": "14"}),
            &Json::Null,
        );
        assert_eq!(odd, d);
        let set = Prefs::decode(
            &json!({"dashboardEnabled": false, "controlCenterEnabled": false,
                    "topBarEnabled": false, "frameSheen": false,
                    "interfaceFont": " DejaVu Sans ", "monoFont": "DejaVu Sans Mono", "textSize": 14}),
            &Json::Null,
        );
        assert!(!set.dashboard_enabled && !set.control_center_enabled && !set.top_bar_enabled);
        assert!(!set.frame_sheen);
        let style = set.text_style();
        assert_eq!(style.size_offset, 2.);
        assert_eq!(style.aliases[INTERFACE_FONT], "DejaVu Sans");
        assert_eq!(style.aliases[MONO_FONT], "DejaVu Sans Mono");
        // `textSize: 0` is falsy in QML and means the body size; a family equal to the
        // default needs no alias.
        let zero = Prefs::decode(
            &json!({"textSize": 0, "interfaceFont": INTERFACE_FONT}),
            &Json::Null,
        );
        assert_eq!(zero.text_style(), Default::default());
        assert_eq!(
            set.bar_stores()["bar.dashboard.enabled"],
            bead::core::Value::Bool(false)
        );
    }
    struct Fake(Publisher);
    impl Fake {
        fn new(id: &str) -> Arc<Self> {
            Arc::new(Self(
                Publisher::new("fake".into(), store_doc(id, json!({}))).unwrap(),
            ))
        }
        fn set(&self, id: &str, data: Json) {
            self.0.publish(store_doc(id, data)).unwrap();
        }
    }
    impl Source for Fake {
        fn snapshot(&self) -> Arc<Snapshot> {
            self.0.snapshot()
        }
        fn subscribe(&self, callback: Callback) -> Result<Subscription> {
            self.0.subscribe(callback)
        }
        fn invoke(&self, _: String, _: Json) -> bead::core::source::Reply<'_> {
            Box::pin(async { Ok(Json::Null) })
        }
        fn reconnect(&self) -> bead::core::source::Reply<'_> {
            Box::pin(async { Ok(Json::Null) })
        }
        fn stop(&self) -> bead::core::source::Reply<'_> {
            Box::pin(async { Ok(Json::Null) })
        }
    }
    #[test]
    fn live_store_changes_reach_apply_and_subscribers_without_restart() {
        let (desktop, dashboard) = (Fake::new("desktop"), Fake::new("dashboard"));
        let applied = Arc::new(Mutex::new(Vec::<bool>::new()));
        let log = applied.clone();
        let (service, effective) = Service::start(
            desktop.clone(),
            dashboard.clone(),
            Arc::new(move |p: &Prefs| log.lock().unwrap().push(p.reduce_motion)),
        )
        .unwrap();
        let wakes = Arc::new(Mutex::new(0_u32));
        let counter = wakes.clone();
        let _watch = effective
            .subscribe(Arc::new(move |_| *counter.lock().unwrap() += 1))
            .unwrap();
        let before = *wakes.lock().unwrap();
        assert_eq!(*applied.lock().unwrap(), [false]);
        assert!(!Prefs::from_source(&*effective).fahrenheit);

        desktop.set(
            "desktop",
            json!({"weatherFahrenheit": true, "animations": false}),
        );
        let now = Prefs::from_source(&*effective);
        assert!(now.fahrenheit && now.reduce_motion);
        assert_eq!(*applied.lock().unwrap(), [false, true]);

        // An unrelated key republishes the store but changes no decoded value.
        desktop.set(
            "desktop",
            json!({"weatherFahrenheit": true, "animations": false, "blur": false}),
        );
        assert_eq!(*applied.lock().unwrap(), [false, true]);
        let woke = *wakes.lock().unwrap();

        dashboard.set("dashboard", json!({"tabs": ["system", "today"]}));
        assert_eq!(
            Prefs::from_source(&*effective).core_tabs,
            ["system", "today"]
        );
        assert_eq!(*wakes.lock().unwrap(), woke + 1);
        assert!(woke > before);

        // The wrong store id never leaks values into the decoded preferences.
        desktop.set("dashboard", json!({"weatherFahrenheit": false}));
        assert!(!Prefs::from_source(&*effective).fahrenheit);
        drop(service);
    }

    /// The real transport: a fake daemon publishes both stores, then edits one, and the
    /// motion switch and the effective Source follow without a restart.
    #[tokio::test(flavor = "current_thread")]
    async fn stores_published_over_the_wire_drive_motion_and_the_effective_source() {
        use crate::nacre_source::{Client, Config};
        use nacre_protocol::{Message, framing};
        use std::{os::unix::fs::PermissionsExt, time::Duration};
        use tokio::{io::BufReader, net::UnixListener, sync::Notify};

        let dir = std::env::temp_dir().join(format!("prefs-wire-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let edit = Arc::new(Notify::new());
        let go = edit.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            assert!(matches!(
                reader.next().await.unwrap().unwrap(),
                Message::Hello { .. }
            ));
            let welcome = Message::Welcome {
                proto: 1,
                min: 1,
                boot: "prefs-boot".into(),
                topics: [DESKTOP, DASHBOARD].map(|t| (t.to_string(), 1)).into(),
                methods: nacre_protocol::core_methods(),
                frame_ms: None,
            };
            framing::write(&mut writer, &welcome).await.unwrap();
            let mut seen = 0;
            while seen < 2 {
                if let Message::Sub { topics } = reader.next().await.unwrap().unwrap() {
                    for topic in topics {
                        let (id, data) = if topic == DESKTOP {
                            ("desktop", json!({"weatherFahrenheit": true}))
                        } else {
                            ("dashboard", json!({"tabs": ["system", "today"]}))
                        };
                        let snap = Message::Snap {
                            topic,
                            seq: 0,
                            data: store_doc(id, data),
                        };
                        framing::write(&mut writer, &snap).await.unwrap();
                        seen += 1;
                    }
                }
            }
            go.notified().await;
            let snap = Message::Snap {
                topic: DESKTOP.into(),
                seq: 1,
                data: store_doc(
                    "desktop",
                    json!({"weatherFahrenheit": true, "reduceMotion": true,
                           "interfaceFont": "DejaVu Sans", "textSize": 14,
                           "dashboardEnabled": false}),
                ),
            };
            framing::write(&mut writer, &snap).await.unwrap();
            while let Ok(Some(_)) = reader.next().await {}
        });
        let client =
            Client::start(Config::new(socket, vec![DESKTOP.into(), DASHBOARD.into()])).unwrap();
        let topic = |name| Arc::new(client.topic(name).unwrap()) as Arc<dyn Source>;
        let motion = Arc::new(Mutex::new(false));
        let record = motion.clone();
        let gate = crate::shell_control::Gate::new();
        let applied = gate.clone();
        let (_service, effective) = Service::start(
            topic(DESKTOP),
            topic(DASHBOARD),
            Arc::new(move |p: &Prefs| {
                *record.lock().unwrap() = p.reduce_motion;
                applied.apply(p);
            }),
        )
        .unwrap();
        for _ in 0..500 {
            let p = Prefs::from_source(&*effective);
            if p.fahrenheit && p.core_tabs == ["system", "today"] {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let p = Prefs::from_source(&*effective);
        assert!(p.fahrenheit && !p.reduce_motion);
        assert_eq!(p.core_tabs, ["system", "today"]);
        assert!(!*motion.lock().unwrap());
        edit.notify_one();
        for _ in 0..500 {
            if *motion.lock().unwrap() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(*motion.lock().unwrap(), "reduced motion applied live");
        let live = Prefs::from_source(&*effective);
        assert!(live.reduce_motion);
        assert_eq!(live.text_style().size_offset, 2.);
        assert_eq!(live.text_style().aliases[INTERFACE_FONT], "DejaVu Sans");
        use bead::wayland::ShellControl;
        assert!(!gate.panel_allowed("dashboard-today") && gate.panel_allowed("sound"));
        client.stop().await.unwrap();
        server.abort();
        std::fs::remove_dir_all(dir).ok();
    }
}

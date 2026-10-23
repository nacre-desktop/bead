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
pub const PROFILE: &str = "settings.store.profile";
pub const LAUNCHER_LAYOUT: &str = "settings.store.launcher-layout";
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
    /// `profile.workspaceCount` (`|| 6`), `profile.workspaceIcons` (one Material symbol
    /// per workspace number, "" for none) and `workspaceWindowHints` (`!== false`).
    pub workspace_count: u32,
    pub workspace_icons: Vec<String>,
    pub window_hints: bool,
    /// `launcher-layout` `order` / `hidden` category ids (strings only, bounded).
    pub rail_order: Vec<String>,
    pub rail_hidden: Vec<String>,
}
const MAX_LIST: usize = 32;
fn strings(value: Option<&Json>, keep_empty: bool) -> Vec<String> {
    value
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_LIST)
                .map(|v| v.as_str().filter(|s| s.len() <= 120 && !s.chars().any(char::is_control)))
                .filter(|s| keep_empty || s.is_some_and(|s| !s.is_empty()))
                .map(|s| s.unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
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
        Self::decode_all(desktop, dashboard, &Json::Null, &Json::Null)
    }
    pub fn decode_all(desktop: &Json, dashboard: &Json, profile: &Json, layout: &Json) -> Self {
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
            // JS `value || 6`: zero and non-numbers fall back to six.
            workspace_count: profile
                .get("workspaceCount")
                .and_then(Json::as_f64)
                .filter(|v| v.is_finite() && *v != 0.)
                .map_or(6, |v| v.round().clamp(1., MAX_LIST as f64) as u32),
            workspace_icons: strings(profile.get("workspaceIcons"), true),
            window_hints: on("workspaceWindowHints"),
            rail_order: strings(layout.get("order"), false),
            rail_hidden: strings(layout.get("hidden"), false),
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
        use bead::core::Value::{Bool, Number};
        let icons = self.workspace_icons.iter().map(|i| Json::from(i.as_str()));
        [
            ("bar.dashboard.enabled".into(), Bool(self.dashboard_enabled)),
            ("bar.workspace.count".into(), Number(self.workspace_count.into())),
            ("bar.workspace.icons".into(), bead::core::Value::list(icons.collect())),
            ("bar.workspace.hints".into(), Bool(self.window_hints)),
        ]
        .into()
    }
    /// Host stores the launcher rail reads (category order and hidden ids).
    pub fn launcher_stores(&self) -> std::collections::BTreeMap<String, bead::core::Value> {
        let list = |ids: &[String]| {
            bead::core::Value::list(ids.iter().map(|i| Json::from(i.as_str())).collect())
        };
        [
            ("launcher.rail.order".into(), list(&self.rail_order)),
            ("launcher.rail.hidden".into(), list(&self.rail_hidden)),
        ]
        .into()
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
        Self::start_with(desktop, dashboard, None, None, apply)
    }
    /// As [`Service::start`], also following `profile` and `launcher-layout` when the
    /// daemon publishes them (absent topics keep Nacre's defaults).
    pub fn start_with(
        desktop: Arc<dyn Source>,
        dashboard: Arc<dyn Source>,
        profile: Option<Arc<dyn Source>>,
        layout: Option<Arc<dyn Source>>,
        apply: Apply,
    ) -> Result<(Self, Arc<dyn Source>)> {
        let first = Prefs::default();
        let publisher = Publisher::new("prefs".into(), serde_json::to_value(&first).unwrap())?;
        let effective: Arc<dyn Source> = Arc::new(Shared(publisher.clone()));
        let current = Arc::new(Mutex::new(None::<Prefs>));
        let update: Callback = {
            let (desktop, dashboard) = (desktop.clone(), dashboard.clone());
            let (profile, layout) = (profile.clone(), layout.clone());
            Arc::new(move |_| {
                let extra = |source: &Option<Arc<dyn Source>>, id: &str| {
                    source.as_ref().map_or(Json::Null, |s| store(&s.snapshot(), id).clone())
                };
                let next = Prefs::decode_all(
                    store(&desktop.snapshot(), "desktop"),
                    store(&dashboard.snapshot(), "dashboard"),
                    &extra(&profile, "profile"),
                    &extra(&layout, "launcher-layout"),
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
        let mut subscriptions = vec![
            desktop.subscribe(update.clone())?,
            dashboard.subscribe(update.clone())?,
        ];
        for source in [&profile, &layout].into_iter().flatten() {
            subscriptions.push(source.subscribe(update.clone())?);
        }
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
    #[test]
    fn workspace_and_rail_prefs_follow_qml_defaults_and_wrong_types() {
        let d = Prefs::default();
        assert_eq!((d.workspace_count, d.window_hints), (6, true));
        assert!(d.workspace_icons.is_empty() && d.rail_order.is_empty() && d.rail_hidden.is_empty());
        // `count || 6`, `!== false`, and anything but a list of strings means unset.
        let odd = Prefs::decode_all(
            &json!({"workspaceWindowHints": 0}),
            &Json::Null,
            &json!({"workspaceCount": 0, "workspaceIcons": "home"}),
            &json!({"order": "home", "hidden": [1, null, ""]}),
        );
        assert_eq!(odd, d);
        let set = Prefs::decode_all(
            &json!({"workspaceWindowHints": false}),
            &Json::Null,
            &json!({"workspaceCount": 4.4, "workspaceIcons": ["home", 5, "code"]}),
            &json!({"order": ["apps", "", "home"], "hidden": ["style"]}),
        );
        assert_eq!((set.workspace_count, set.window_hints), (4, false));
        assert_eq!(set.workspace_icons, ["home", "", "code"]);
        assert_eq!((&set.rail_order[..], &set.rail_hidden[..]), (&["apps".to_string(), "home".into()][..], &["style".to_string()][..]));
        let bar = set.bar_stores();
        assert_eq!(bar["bar.workspace.count"], bead::core::Value::Number(4.));
        assert_eq!(bar["bar.workspace.hints"], bead::core::Value::Bool(false));
        assert_eq!(
            bar["bar.workspace.icons"],
            bead::core::Value::list(vec![json!("home"), json!(""), json!("code")])
        );
        assert_eq!(
            set.launcher_stores()["launcher.rail.hidden"],
            bead::core::Value::list(vec![json!("style")])
        );
        // A hundred icons are bounded.
        let many = Prefs::decode_all(&Json::Null, &Json::Null, &json!({"workspaceIcons": vec!["a"; 100]}), &Json::Null);
        assert_eq!(many.workspace_icons.len(), MAX_LIST);
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

    #[test]
    fn profile_and_launcher_layout_follow_live_and_absent_topics_keep_defaults() {
        let (desktop, dashboard) = (Fake::new("desktop"), Fake::new("dashboard"));
        let (profile, layout) = (Fake::new("profile"), Fake::new("launcher-layout"));
        let (_service, effective) = Service::start_with(
            desktop,
            dashboard,
            Some(profile.clone()),
            Some(layout.clone()),
            Arc::new(|_: &Prefs| {}),
        )
        .unwrap();
        assert_eq!(Prefs::from_source(&*effective), Prefs::default());
        profile.set("profile", json!({"workspaceCount": 3, "workspaceIcons": ["home"]}));
        layout.set("launcher-layout", json!({"order": ["actions"], "hidden": ["search"]}));
        let now = Prefs::from_source(&*effective);
        assert_eq!((now.workspace_count, now.workspace_icons.len()), (3, 1));
        assert_eq!((now.rail_order, now.rail_hidden), (vec!["actions".to_string()], vec!["search".to_string()]));
        // The wrong store id never leaks values into the decoded preferences.
        profile.set("desktop", json!({"workspaceCount": 9}));
        assert_eq!(Prefs::from_source(&*effective).workspace_count, 6);
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

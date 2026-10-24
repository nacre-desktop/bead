// SPDX-License-Identifier: GPL-3.0-only
//! B0063: capability conditions, the custom sections' rows over fixed topics, and their calls
//! over a fake daemon (exact method names and parameter shapes of the published contracts).
use super::settings::*;
use crate::nacre_source::{CallWake, Client, Config};
use bead::core::{Stores, Value, source::Source};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn stores(page: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("settings.page".into(), Value::Text(page.into())),
            ("settings.search".into(), Value::Text(String::new())),
        ]))
        .unwrap()
}
/// The projected rows as a flat list: each section is followed by the rows it carries (B0067 nests them).
fn items(model: &BTreeMap<String, Value>) -> Vec<Json> {
    let mut out = Vec::new();
    for row in nested_items(model) {
        let inner = row["rows"].as_array().cloned().unwrap_or_default();
        out.push(row);
        out.extend(inner);
    }
    out
}
fn nested_items(model: &BTreeMap<String, Value>) -> Vec<Json> {
    match &model["settings.items"] {
        Value::List(rows) => rows.to_vec(),
        other => panic!("{other:?}"),
    }
}
fn nav_ids(model: &BTreeMap<String, Value>) -> Vec<String> {
    match &model["settings.nav"] {
        Value::List(rows) => rows
            .iter()
            .filter_map(|r| r["id"].as_str().map(String::from))
            .collect(),
        other => panic!("{other:?}"),
    }
}
fn row<'a>(rows: &'a [Json], id: &str) -> &'a Json {
    rows.iter().find(|r| r["id"] == id).unwrap_or_else(|| {
        panic!(
            "{id} missing in {:?}",
            rows.iter().map(|r| &r["id"]).collect::<Vec<_>>()
        )
    })
}
fn wake() -> CallWake {
    Arc::new(|| {})
}
fn health() -> Json {
    json!({"available": true, "stale": false})
}
fn store(id: &str, rev: u64, data: Json) -> Json {
    json!({"store": id, "rev": rev, "exists": true, "data": data, "dirty": false})
}

// ---- fake daemon ---------------------------------------------------------------------------

struct Lab {
    dir: std::path::PathBuf,
    client: Client,
    calls: mpsc::UnboundedReceiver<(String, Json)>,
    fail: Arc<Mutex<BTreeMap<String, ErrorCode>>>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl Lab {
    /// The next call the daemon saw: `(method, params)`.
    async fn call(&mut self) -> (String, Json) {
        tokio::time::timeout(Duration::from_secs(5), self.calls.recv())
            .await
            .expect("a call")
            .unwrap()
    }
    fn quiet(&mut self) -> bool {
        self.calls.try_recv().is_err()
    }
}
async fn lab(topics: Vec<(&str, Json)>, methods: &[&str]) -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pages-lab-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let fail: Arc<Mutex<BTreeMap<String, ErrorCode>>> = Default::default();
    let failing = fail.clone();
    let published: BTreeMap<String, Json> = topics
        .into_iter()
        .map(|(n, d)| (n.to_string(), d))
        .collect();
    let advertised: Vec<String> = methods.iter().map(|m| m.to_string()).collect();
    tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        let Ok(Some(Message::Hello { .. })) = reader.next().await else {
            return;
        };
        let mut methods = nacre_protocol::core_methods();
        for m in &advertised {
            methods.insert(
                m.clone(),
                nacre_protocol::Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout: 3000,
                    params: json!({"type": "object"}),
                    result: json!({}),
                },
            );
        }
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "pages-boot".into(),
                topics: published.keys().map(|n| (n.clone(), 1)).collect(),
                methods,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        while let Ok(Some(message)) = reader.next().await {
            match message {
                Message::Sub { topics } => {
                    for topic in topics {
                        let data = published.get(&topic).cloned().unwrap_or(json!({}));
                        framing::write(
                            &mut writer,
                            &Message::Snap {
                                topic,
                                seq: 0,
                                data,
                            },
                        )
                        .await
                        .unwrap();
                    }
                }
                Message::Call { id, m, p, .. } => {
                    let _ = calls_tx.send((m.clone(), p));
                    let code = failing.lock().unwrap().get(&m).cloned();
                    let reply = match code {
                        Some(code) => Reply::Err {
                            err: Failure::new(code, "refused", false),
                        },
                        None if m == "time.zones" => Reply::Ok {
                            ok: json!({"zones": ["Africa/Cairo", "America/New_York", "Europe/Oslo", "Europe/Paris"]}),
                        },
                        None if m == "colors.targets" => Reply::Ok {
                            ok: json!({"targets": [
                                {"id": "gtk3", "label": "GTK 3", "status": "current"},
                                {"id": "gtk4", "label": "GTK 4", "status": "tool-missing"},
                                {"id": "kitty", "label": "Kitty", "status": "not-published"},
                                {"id": "qt", "label": "Qt", "status": "paused"}]}),
                        },
                        None if m == "system.status" => Reply::Ok { ok: status() },
                        None => Reply::Ok {
                            ok: json!({"changed": true}),
                        },
                    };
                    framing::write(&mut writer, &Message::Ret { id, reply })
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(socket, vec![])).unwrap();
    for _ in 0..500 {
        if client.snapshot().connected && client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Lab {
        dir,
        client,
        calls,
        fail,
    }
}
/// Project until `done` holds (the topics arrive asynchronously).
async fn until(
    settings: &Settings,
    at: &Stores,
    done: impl Fn(&BTreeMap<String, Value>) -> bool,
) -> BTreeMap<String, Value> {
    let mut model = settings.project(at);
    for _ in 0..500 {
        if done(&model) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        model = settings.project(at);
    }
    panic!(
        "never reached: {:?}",
        items(&model).iter().map(|r| &r["id"]).collect::<Vec<_>>()
    );
}
fn has(id: &'static str) -> impl Fn(&BTreeMap<String, Value>) -> bool {
    move |m| items(m).iter().any(|r| r["id"] == id)
}
fn option_args(row: &Json) -> Vec<String> {
    row["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_string())
        .collect()
}

/// A `system.status` reply (N0051 C6).
fn status() -> Json {
    json!({"checked": "2026-10-23T20:00:00Z", "release": "0123456789abcdef0123",
        "services": {"nacre-shell.service": "active", "nacre-brain.service": "failed"},
        "failed_services": ["nacre-brain.service"], "config_errors": "",
        "drift": ["~/.config/niri/config.kdl", "~/.config/kitty/kitty.conf"],
        "brain_sync": "synced 5 min ago", "performance": {"label": "idle", "cpu_core_percent": 1.4, "memory_mib": 41.0},
        "session_event": "login", "updates_age_seconds": 7200,
        "aur_review": {"mode": "required", "source": "registered-profile-setting", "profile": "default"}})
}
fn desktop() -> Json {
    store(
        "desktop",
        7,
        json!({"preset": "focused", "lockMedia": false}),
    )
}
fn caps(flags: Json) -> Json {
    json!({"health": health(), "flags": flags})
}

// ---- weather location (B0083) ---------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn weather_location_is_a_city_field_with_a_hint_and_no_automatic_lookup() {
    let l = lab(
        vec![("settings.store.desktop", store("desktop", 7, json!({})))],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("date-time-language");
    let city = "date-time-language.locale.weatherLocation";
    let model = until(&settings, &at, |m| {
        m["settings.writable"] == Value::Bool(true) && items(m).iter().any(|r| r["id"] == city)
    })
    .await;
    let rows = items(&model);
    assert_eq!(row(&rows, city)["kind"], "text");
    assert_eq!(
        row(&rows, city)["description"],
        "Set a city to show weather."
    );
    // No consent switch and no wording about locating by IP address.
    assert!(
        rows.iter()
            .all(|r| !r.to_string().contains("weatherIpLookup"))
    );
    assert!(
        rows.iter()
            .all(|r| !r.to_string().to_lowercase().contains("ipinfo"))
    );
    assert_eq!(
        row(&rows, "date-time-language.locale.weatherFahrenheit")["kind"],
        "toggle"
    );
}

// ---- capabilities ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn capability_flags_hide_and_disable_registry_entries() {
    let flags = json!({"input.controls": true, "touchpad": false, "ai": true, "brain": false,
        "niriWorkspaces": true, "niriFixedWorkspaces": false, "desktop.gapsIn": true,
        "desktop.blur": false, "battery": false, "bluetooth": true});
    let l = lab(
        vec![
            ("capabilities", caps(flags)),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("keyboard-mouse");
    // Until the flags arrive everything is shown; wait for the filtered page.
    let model = until(&settings, &at, |m| {
        has("keyboard-mouse.mouse.mouseSpeed")(m) && !has("section:keyboard-mouse.touchpad")(m)
    })
    .await;
    let rows = items(&model);
    // Sections and settings follow `visibleWhen`: the touchpad section needs a touchpad.
    assert!(
        rows.iter()
            .any(|r| r["id"] == "section:keyboard-mouse.mouse")
    );
    assert!(
        rows.iter()
            .all(|r| r["id"] != "section:keyboard-mouse.touchpad")
    );
    // Pages follow it too: no battery (power), AI present, Brain missing, Bluetooth present.
    let nav = nav_ids(&model);
    assert!(!nav.contains(&"power".to_string()));
    assert!(nav.contains(&"nacre-ai".to_string()) && !nav.contains(&"brain".to_string()));
    assert!(nav.contains(&"bluetooth".to_string()));
    // `addons` shows while either add-on is missing.
    assert!(nav.contains(&"addons".to_string()));
    let windows = items(
        &until(&settings, &stores("windows-workspaces"), |m| {
            has("windows-workspaces.windows.gapsIn")(m)
                && !has("windows-workspaces.windows.blur")(m)
        })
        .await,
    );
    assert!(
        windows
            .iter()
            .all(|r| r["id"] != "windows-workspaces.windows.blur")
    );
    // `enabledWhen`: the per-monitor action row is visible (workspaces exist) but dimmed.
    let action = row(
        &windows,
        "windows-workspaces.workspaces.monitorWorkspaceActions",
    );
    assert_eq!(action["enabled"], false);
    // Search follows the same flags.
    let found = Stores::default()
        .reconcile(&BTreeMap::from([
            ("settings.page".into(), Value::Text("appearance".into())),
            ("settings.search".into(), Value::Text("touchpad".into())),
        ]))
        .unwrap();
    let hits = items(&settings.project(&found));
    assert!(hits.iter().all(|r| {
        !r["id"]
            .as_str()
            .unwrap()
            .starts_with("setting:keyboard-mouse.touchpad.")
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn without_flags_everything_is_shown_and_missing_flags_are_false() {
    // No capabilities topic: the registry default (show all), like before the topic existed.
    let l = lab(
        vec![("settings.store.desktop", desktop())],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let model = until(
        &settings,
        &stores("windows-workspaces"),
        has("windows-workspaces.windows.blur"),
    )
    .await;
    assert!(nav_ids(&model).contains(&"brain".to_string()));
    drop(settings);
    // A published flag map that does not name a flag means false (QML); `bluetooth` and `battery`
    // are flags of the topic (N0051 C1), never derived from the other topics.
    let l = lab(
        vec![
            ("capabilities", caps(json!({"ai": true, "battery": true, "bluetooth": true}))),
            ("bluetooth", json!({"health": health(), "adapters": {"order": ["hci0"], "items": {"hci0": {"name": "hci0", "powered": true, "discovering": false}}}, "devices": {"order": [], "items": {}}})),
            ("battery", json!({"health": health(), "percentage": 50.0, "charging": false})),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("appearance");
    let mut nav = nav_ids(&settings.project(&at));
    for _ in 0..500 {
        if nav.contains(&"power".to_string()) && !nav.contains(&"brain".to_string()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        nav = nav_ids(&settings.project(&at));
    }
    assert!(nav.contains(&"power".to_string()) && nav.contains(&"bluetooth".to_string()));
    assert!(!nav.contains(&"brain".to_string()));
}

// ---- store pages: lock and login, privacy, power -------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn lock_login_privacy_and_power_pages_write_the_stores_and_call_the_lock() {
    let mut l = lab(
        vec![
            (
                "capabilities",
                caps(json!({"powerProfiles": true, "packages": true})),
            ),
            ("settings.store.desktop", desktop()),
            ("settings.store.power", store("power", 2, json!({}))),
        ],
        &["settings.set", "session.request", "login.install_theme"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("lock-login");
    let model = until(&settings, &at, |m| {
        has("login-theme")(m) && m["settings.writable"] == Value::Bool(true)
    })
    .await;
    let rows = items(&model);
    assert_eq!(row(&rows, "lock-login.widgets.lockMedia")["kind"], "toggle");
    assert_eq!(model["settings.writable"], Value::Bool(true));
    settings.act("set:lock-login.widgets.lockMedia", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.set".into(),
            json!({"store":"desktop","key":["lockMedia"],"value":true,"rev":7})
        )
    );
    // "Lock now" is the session request; the login theme install is its own method.
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let lock = row(&rows, "lock-login.lock-now");
    assert_eq!(lock["options"][0]["id"], "do:lock");
    settings.act("do:lock", "", wake());
    assert_eq!(
        l.call().await,
        ("session.request".into(), json!({"action": "lock"}))
    );
    for _ in 0..200 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:install-login", "", wake());
    assert_eq!(l.call().await, ("login.install_theme".into(), json!({})));
    // The install is not advertised: the row is absent (checked on a daemon without it, below).
    let power = until(&settings, &stores("power"), has("power.profiles.enabled")).await;
    assert_eq!(
        row(&items(&power), "power.profiles.enabled")["kind"],
        "toggle"
    );
    let privacy = items(&settings.project(&stores("privacy")));
    assert_eq!(
        row(&privacy, "privacy.history.nativeClipboard")["kind"],
        "toggle"
    );
    // Clearing the notification history is an action row that asks again first.
    let clear = row(&privacy, "privacy.notifications");
    assert_eq!(clear["options"][0]["id"], "do:confirm|clear-history");
    drop(settings);
    let l2 = lab(
        vec![("settings.store.desktop", desktop())],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l2.client.clone()), &root()).unwrap();
    let model = until(
        &settings,
        &stores("lock-login"),
        has("lock-login.widgets.lockMedia"),
    )
    .await;
    let rows = items(&model);
    assert!(rows.iter().all(|r| r["id"] != "login-theme"));
    assert_eq!(
        row(&rows, "lock-login.lock-now")["options"][0]["enabled"],
        false
    );
}

// ---- date and time ----------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn date_time_page_picks_a_zone_sets_ntp_and_says_when_authentication_failed() {
    let mut l = lab(
        vec![
            ("time", json!({"health": health(), "timezone": "Europe/Oslo", "ntp": {"supported": true, "enabled": false}})),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set", "time.zones", "time.set_timezone", "time.set_ntp"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("date-time-language");
    let model = until(&settings, &at, |m| {
        items(m).iter().any(|r| {
            r["id"] == "time.picker" && r["items"].as_array().is_some_and(|i| i.len() == 4)
        })
    })
    .await;
    assert_eq!(l.call().await, ("time.zones".into(), json!({})));
    let rows = items(&model);
    assert_eq!(row(&rows, "time.zone")["value"], "Europe/Oslo");
    assert_eq!(
        option_args(row(&rows, "time.clock")),
        ["do:clock|12-hour", "do:clock|24-hour"]
    );
    assert_eq!(row(&rows, "time.clock")["options"][1]["selected"], true);
    // Filtering narrows the list; the current zone is the selection until another is picked.
    let typed = stores("date-time-language")
        .reconcile(&BTreeMap::from([(
            "settings.pick.text".into(),
            Value::Text("par".into()),
        )]))
        .unwrap();
    let filtered = items(&settings.project(&typed));
    let picker = row(&filtered, "time.picker");
    assert_eq!(picker["items"].as_array().unwrap().len(), 1);
    assert_eq!(picker["items"][0]["id"], "do:zone-pick|Europe/Paris");
    assert_eq!(row(&filtered, "time.set")["enabled"], false);
    settings.act("do:zone-pick|Europe/Paris", "", wake());
    let rows = items(&settings.project(&typed));
    assert_eq!(row(&rows, "time.set")["enabled"], true);
    // A zone the daemon did not list is never picked.
    settings.act("do:zone-pick|Mars/Base", "", wake());
    settings.act("do:zone-set", "", wake());
    assert_eq!(
        l.call().await,
        ("time.set_timezone".into(), json!({"zone": "Europe/Paris"}))
    );
    // A refused prompt (or a missing agent) is `permission_denied`, said plainly.
    l.fail
        .lock()
        .unwrap()
        .insert("time.set_ntp".into(), ErrorCode::PermissionDenied);
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:ntp|on", "", wake());
    assert_eq!(
        l.call().await,
        ("time.set_ntp".into(), json!({"enabled": true}))
    );
    let mut message = String::new();
    for _ in 0..500 {
        let model = settings.project(&at);
        if let Value::Text(text) = &model["settings.message"]
            && !text.is_empty()
        {
            message = text.clone();
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(message.contains("Authentication was refused"), "{message}");
    // 12/24 h is the desktop store key, written with the revision fence.
    settings.act("do:clock|12-hour", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.set".into(),
            json!({"store":"desktop","key":["clockFormat"],"value":"12-hour","rev":7})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn theme_targets_show_their_status_and_retheme_through_set_target() {
    let mut l = lab(
        vec![(
            "settings.store.colors",
            store(
                "colors",
                3,
                json!({"appThemes": {"gtk3": true, "gtk4": false}}),
            ),
        )],
        &["settings.set", "colors.targets", "colors.set_target"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("colors/app-theming");
    let model = until(&settings, &at, |m| {
        items(m).iter().any(|r| {
            r["id"] == "colors/app-theming.targets.gtk4"
                && r["description"]
                    .as_str()
                    .is_some_and(|d| d.contains("not installed"))
        })
    })
    .await;
    assert_eq!(l.call().await, ("colors.targets".into(), json!({})));
    let rows = items(&model);
    let gtk3 = row(&rows, "colors/app-theming.targets.gtk3");
    assert_eq!(
        option_args(gtk3),
        ["do:app-theme|gtk3|on", "do:app-theme|gtk3|off"]
    );
    assert_eq!(gtk3["options"][0]["selected"], true);
    assert_eq!(
        row(&rows, "colors/app-theming.targets.gtk4")["options"][0]["selected"],
        false
    );
    // The four statuses the publisher writes, each with its own text.
    let say = |id: &str| row(&rows, id)["description"].as_str().unwrap().to_string();
    let texts = [
        say("colors/app-theming.targets.gtk3"),
        say("colors/app-theming.targets.gtk4"),
    ];
    assert_ne!(texts[0], texts[1]);
    // Switching a target on is the re-theming call, not a store write; the list is read again.
    settings.act("do:app-theme|gtk4|on", "", wake());
    assert_eq!(
        l.call().await,
        (
            "colors.set_target".into(),
            json!({"id": "gtk4", "enabled": true})
        )
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(l.call().await.0, "colors.targets");
    // An id the daemon did not list never becomes a call.
    settings.act("do:app-theme|nope|on", "", wake());
    settings.act("do:app-theme|gtk3|maybe", "", wake());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(l.quiet());
    // A refused change is said.
    l.fail
        .lock()
        .unwrap()
        .insert("colors.set_target".into(), ErrorCode::Busy);
    settings.act("do:app-theme|gtk3|off", "", wake());
    assert_eq!(l.call().await.0, "colors.set_target");
}

// ---- notifications history and workflows --------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn notification_history_lists_the_window_and_clears_after_a_second_press() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let notice = |summary: &str| {
        json!({"app_name": "Mail", "summary": summary, "body": "Hello there", "created_ms": now - 5 * 60_000,
            "urgency": "normal", "resident": false, "transient": false, "actions": {"order": [], "items": {}}})
    };
    let mut l = lab(
        vec![
            ("notifications", json!({"health": health(), "do_not_disturb": false,
                "active": {"order": [], "items": {}},
                "history": {"order": ["b", "a"], "items": {"a": notice("Older"), "b": notice("Newest")}},
                "history_count": 2, "history_more": false})),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set", "notifications.clear"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("notifications");
    let model = until(&settings, &at, has("history:b")).await;
    let rows = items(&model);
    let ids: Vec<&str> = rows
        .iter()
        .filter_map(|r| r["id"].as_str())
        .filter(|i| i.starts_with("history:"))
        .collect();
    assert_eq!(ids, ["history:b", "history:a"], "newest first");
    assert_eq!(row(&rows, "history:b")["label"], "Newest");
    assert_eq!(row(&rows, "history:b")["value"], "5 min ago");
    assert_eq!(
        row(&rows, "history.clear")["label"],
        "2 saved notifications"
    );
    settings.act("do:clear-history", "", wake());
    assert!(l.quiet(), "clearing needs the confirming press first");
    settings.act("do:confirm|clear-history", "", wake());
    let rows = items(&settings.project(&at));
    assert_eq!(
        row(&rows, "history.clear")["options"][0]["id"],
        "do:clear-history"
    );
    settings.act("do:clear-history", "", wake());
    assert_eq!(
        l.call().await,
        ("notifications.clear".into(), json!({"scope": "history"}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn workflows_page_sends_the_controller_arguments() {
    let mut l = lab(
        vec![
            (
                "settings.desktop",
                json!({"data": {"preset": "music"}, "message": "Saved"}),
            ),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set", "settings.desktop"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("windows-workspaces/workflows");
    let model = until(&settings, &at, has("workflow.message")).await;
    let rows = items(&model);
    let presets = row(&rows, "workflow.presets");
    assert_eq!(presets["options"].as_array().unwrap().len(), 7);
    assert_eq!(
        presets["options"][5]["selected"], true,
        "music is the current preset"
    );
    settings.act("do:preset|focused", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.desktop".into(),
            json!({"arguments": ["preset", "focused"], "rev": 7})
        )
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:role|Mail", "", wake());
    settings.act("do:role|Browser", "", wake());
    settings.act("do:role|Nope", "", wake());
    settings.act("do:save-workflow", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.desktop".into(),
            json!({"arguments": ["save-workflow", "music", "[\"Mail\",\"Browser\"]"], "rev": 7})
        )
    );
}

// ---- sound, network, Bluetooth ------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn sound_page_sets_levels_mute_and_default_by_the_published_shape() {
    let node = |name: &str, kind: &str, volume: f64, muted: bool| json!({"name": name, "kind": kind, "volume": volume, "muted": muted, "ready": true});
    let mut l = lab(
        vec![
            (
                "audio",
                json!({"health": health(), "default_output": "out", "default_input": "in",
                "nodes": {"order": ["out", "hdmi", "in", "app"], "items": {
                    "out": node("Speakers", "output", 0.125, false),
                    "hdmi": node("Monitor", "output", 0.5, true),
                    "in": node("Mic", "input", 1.0, false),
                    "app": node("Player", "playback", 0.5, false)}}}),
            ),
            ("settings.store.desktop", desktop()),
        ],
        &[
            "settings.set",
            "audio.set_volume",
            "audio.set_mute",
            "audio.set_default",
        ],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("sound");
    let model = until(&settings, &at, has("node:out")).await;
    let rows = items(&model);
    assert_eq!(
        row(&rows, "node:out")["slider"]["value"],
        50.0,
        "perceptual: cube root of 0.125"
    );
    assert_eq!(
        row(&rows, "node:out")["buttons"][0]["selected"],
        true,
        "default output"
    );
    assert_eq!(row(&rows, "node:hdmi")["buttons"][1]["icon"], "volume_off");
    assert!(
        row(&rows, "node:app")["buttons"].as_array().unwrap().len() == 1,
        "streams have no default button"
    );
    // The default output is addressed as the sink; another node by its name; the slider is cubic.
    settings.act("do:vol|out|60", "", wake());
    assert_eq!(l.call().await.1, json!({"target": "sink", "value": 0.216}));
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:mute|hdmi", "", wake());
    assert_eq!(
        l.call().await,
        (
            "audio.set_mute".into(),
            json!({"target": "node", "node": "hdmi", "muted": false})
        )
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:default|hdmi", "", wake());
    assert_eq!(
        l.call().await,
        ("audio.set_default".into(), json!({"node": "hdmi"}))
    );
    // A name that cannot be a PipeWire node name never reaches the wire.
    settings.act("do:vol|a b;rm|10", "", wake());
    assert!(l.quiet());
}

#[tokio::test(flavor = "current_thread")]
async fn network_page_joins_open_and_secured_networks_and_forgets_saved_ones() {
    let mut l = lab(
        vec![
            ("network", json!({"health": health(), "wireless_enabled": true,
                "devices": {"order": [], "items": {}},
                "access_points": {"order": ["home", "cafe"], "items": {
                    "home": {"device": "wlan0", "name": "Home", "strength": 70, "secured": true, "active": false},
                    "cafe": {"device": "wlan0", "name": "Cafe", "strength": 40, "secured": false, "active": false}}},
                "connections": {"order": ["c1"], "items": {"c1": {"name": "Old", "kind": "wifi", "active": false, "ssid": "Old"}}}})),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set", "network.connect", "network.forget", "network.set_wifi"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("network");
    until(&settings, &at, has("ap:home")).await;
    settings.act("do:net-connect|cafe", "", wake());
    assert_eq!(
        l.call().await,
        (
            "network.connect".into(),
            json!({"device": "wlan0", "target": {"kind": "access_point", "id": "cafe"}})
        )
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // A secured network without a saved profile asks for the password first and sends it once.
    settings.act("do:net-connect|home", "", wake());
    assert!(l.quiet());
    let rows = items(&settings.project(&at));
    assert_eq!(row(&rows, "network.password")["kind"], "secret");
    settings.secret("correct horse");
    settings.act("do:net-join", "", wake());
    assert_eq!(
        l.call().await,
        (
            "network.connect".into(),
            json!({"device": "wlan0", "secret": "correct horse",
            "target": {"kind": "access_point", "id": "home"}})
        )
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // Forgetting needs the confirming press.
    settings.act("do:net-forget|c1", "", wake());
    assert!(l.quiet());
    settings.act("do:confirm|forget:c1", "", wake());
    settings.act("do:net-forget|c1", "", wake());
    assert_eq!(
        l.call().await,
        ("network.forget".into(), json!({"connection": "c1"}))
    );
    for _ in 0..100 {
        settings.project(&at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    settings.act("do:wifi|off", "", wake());
    assert_eq!(
        l.call().await,
        ("network.set_wifi".into(), json!({"enabled": false}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_page_powers_scans_and_connects_known_devices_only() {
    let mut l = lab(
        vec![
            ("bluetooth", json!({"health": health(),
                "adapters": {"order": ["hci0"], "items": {"hci0": {"name": "Laptop", "powered": true, "discovering": false}}},
                "devices": {"order": ["dev_1", "dev_2"], "items": {
                    "dev_1": {"adapter": "hci0", "name": "Buds", "connected": true, "paired": true, "trusted": true, "pairing": false, "battery": 80},
                    "dev_2": {"adapter": "hci0", "name": "Speaker", "connected": false, "paired": true, "trusted": false, "pairing": false}}}})),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set", "bluetooth.set_power", "bluetooth.set_discovery", "bluetooth.connect", "bluetooth.disconnect", "bluetooth.set_trusted"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("bluetooth");
    let model = until(&settings, &at, has("btdev:dev_1")).await;
    let rows = items(&model);
    assert_eq!(row(&rows, "btdev:dev_1")["detail"], "Connected · 80%");
    assert_eq!(
        row(&rows, "btdev:dev_1")["buttons"][0]["id"],
        "do:bt-disconnect|dev_1"
    );
    settings.act("do:bt-connect|dev_2", "", wake());
    assert_eq!(
        l.call().await,
        ("bluetooth.connect".into(), json!({"device": "dev_2"}))
    );
    let pause = || async {
        for _ in 0..100 {
            settings.project(&at);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    pause().await;
    settings.act("do:bt-scan|hci0|on", "", wake());
    assert_eq!(
        l.call().await,
        (
            "bluetooth.set_discovery".into(),
            json!({"adapter": "hci0", "discovering": true})
        )
    );
    pause().await;
    settings.act("do:bt-power|hci0|off", "", wake());
    assert_eq!(
        l.call().await,
        (
            "bluetooth.set_power".into(),
            json!({"adapter": "hci0", "powered": false})
        )
    );
    pause().await;
    // Trust is a second button on each device: it writes the opposite of what is published.
    assert_eq!(
        row(&rows, "btdev:dev_1")["buttons"][1]["id"],
        "do:bt-trust|dev_1|off"
    );
    settings.act("do:bt-trust|dev_2|on", "", wake());
    assert_eq!(
        l.call().await,
        (
            "bluetooth.set_trusted".into(),
            json!({"device": "dev_2", "trusted": true})
        )
    );
    pause().await;
    // An id the daemon never published is not sent.
    settings.act("do:bt-connect|../../x", "", wake());
    settings.act("do:bt-trust|../../x|on", "", wake());
    assert!(l.quiet());
}

// ---- N0051 contracts: history clears, input store, shortcuts, status, launches ----------------------

async fn settle_calls(settings: &Settings, at: &Stores) {
    for _ in 0..100 {
        settings.project(at);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn privacy_page_clears_clipboard_and_launcher_history_after_a_second_press() {
    let mut l = lab(
        vec![("settings.store.desktop", desktop())],
        &[
            "settings.set",
            "clipboard.clear_history",
            "launcher.clear_history",
        ],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("privacy");
    let model = until(&settings, &at, has("privacy.clipboard")).await;
    let rows = items(&model);
    for (id, op) in [
        ("privacy.clipboard", "clear-clipboard"),
        ("privacy.launcher", "clear-launcher"),
    ] {
        assert_eq!(
            row(&rows, id)["options"][0]["id"],
            format!("do:confirm|{op}"),
            "armed by a first press"
        );
        assert_eq!(row(&rows, id)["options"][0]["enabled"], true);
    }
    // The first press only arms; a different row's confirmation does not carry over.
    settings.act("do:clear-clipboard", "", wake());
    assert!(l.quiet());
    settings.act("do:confirm|clear-launcher", "", wake());
    settings.act("do:clear-clipboard", "", wake());
    assert!(l.quiet(), "armed for the launcher, not the clipboard");
    settings.act("do:confirm|clear-clipboard", "", wake());
    let rows = items(&settings.project(&at));
    assert_eq!(
        row(&rows, "privacy.clipboard")["options"][0]["id"],
        "do:clear-clipboard"
    );
    settings.act("do:clear-clipboard", "", wake());
    assert_eq!(
        l.call().await,
        ("clipboard.clear_history".into(), json!({}))
    );
    settle_calls(&settings, &at).await;
    settings.act("do:confirm|clear-launcher", "", wake());
    settings.act("do:clear-launcher", "", wake());
    assert_eq!(l.call().await, ("launcher.clear_history".into(), json!({})));
}

#[tokio::test(flavor = "current_thread")]
async fn keyboard_page_writes_layouts_and_touchpad_and_lists_the_shortcuts() {
    let mut l = lab(
        vec![
            ("capabilities", caps(json!({"input.controls": true, "touchpad": true}))),
            ("settings.store.desktop", desktop()),
            ("settings.store.software-input", store("software-input", 4,
                json!({"keyboardLayouts": "us", "touchpadEnabled": true}))),
            ("shortcuts", json!({"health": health(), "items": {"order": ["k0", "k1"], "items": {
                "k0": {"key": "SUPER+Q", "description": "Close window"},
                "k1": {"key": "SUPER+SHIFT+S", "description": "", "submap": null, "menu": "actions.emoji"}}}})),
        ],
        &["settings.set", "shortcuts.refresh"],
    )
    .await;
    let layouts = [
        ("us", "English (US)"),
        ("no", "Norwegian"),
        ("de", "German"),
    ];
    let settings = Settings::new(Some(l.client.clone()), &root())
        .unwrap()
        .with_layouts(&layouts);
    let at = stores("keyboard-mouse");
    let model = until(&settings, &at, |m| {
        has("key:k0")(m) && m["settings.writable"] == Value::Bool(true)
    })
    .await;
    // The list is asked for when the page opens (the compositor config is not watched).
    assert_eq!(l.call().await, ("shortcuts.refresh".into(), json!({})));
    let rows = items(&model);
    assert_eq!(row(&rows, "key:k0")["label"], "Close window");
    assert_eq!(row(&rows, "key:k0")["value"], "SUPER+Q");
    assert_eq!(
        row(&rows, "key:k1")["label"],
        "SUPER+SHIFT+S",
        "no description: the key"
    );
    assert_eq!(
        row(&rows, "keyboard-mouse.keyboard.keyboardLayouts")["value"],
        "us"
    );
    let picker = row(&rows, "layout.picker");
    assert_eq!(picker["items"].as_array().unwrap().len(), 3);
    assert_eq!(picker["items"][0]["selected"], true);
    // Adding a layout writes the comma-separated list, fenced by the revision.
    settings.act("do:layout|no", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.set".into(),
            json!({"store": "software-input", "key": ["keyboardLayouts"], "value": "us,no", "rev": 4})
        )
    );
    settle_calls(&settings, &at).await;
    // An id the host list does not have never reaches the wire.
    settings.act("do:layout|xx;yy", "", wake());
    assert!(l.quiet());
    settings.act("do:layout-reset", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.set".into(),
            json!({"store": "software-input", "key": ["keyboardLayouts"], "value": "", "rev": 4})
        )
    );
    settle_calls(&settings, &at).await;
    settings.act("set:keyboard-mouse.touchpad.touchpadEnabled", "", wake());
    assert_eq!(
        l.call().await,
        (
            "settings.set".into(),
            json!({"store": "software-input", "key": ["touchpadEnabled"], "value": false, "rev": 4})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn power_page_refreshes_and_starts_the_service_with_the_packages_menu_as_fallback() {
    let mut l = lab(
        vec![
            (
                "capabilities",
                caps(json!({"powerProfiles": false, "battery": true})),
            ),
            ("settings.store.desktop", desktop()),
            ("settings.store.power", store("power", 2, json!({}))),
        ],
        &[
            "settings.set",
            "capabilities.refresh",
            "tools.launch",
            "ui.dispatch",
        ],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    settings.subscribe(Arc::new(|| {}));
    let at = stores("power");
    let model = until(&settings, &at, has("power.profiles.fix")).await;
    let rows = items(&model);
    assert_eq!(
        row(&rows, "power.profiles.retry")["options"][0]["id"],
        "do:refresh-caps"
    );
    assert_eq!(
        row(&rows, "power.profiles.fix")["options"][0]["id"],
        "do:open|power-service"
    );
    settings.act("do:refresh-caps", "", wake());
    assert_eq!(l.call().await, ("capabilities.refresh".into(), json!({})));
    settle_calls(&settings, &at).await;
    settings.act("do:open|power-service", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "power-service"}))
    );
    settle_calls(&settings, &at).await;
    // Without `powerprofilesctl` the daemon answers `unavailable`: the packages menu opens.
    l.fail
        .lock()
        .unwrap()
        .insert("tools.launch".into(), ErrorCode::Unavailable);
    settings.act("do:open|power-service", "", wake());
    assert_eq!(l.call().await.0, "tools.launch");
    settle_calls(&settings, &at).await;
    assert_eq!(
        l.call().await,
        (
            "ui.dispatch".into(),
            json!({"target": "nacre", "method": "menu", "args": ["software.packages", "power-profiles-daemon"]})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn system_page_shows_the_status_and_launches_the_aur_policy_and_setup() {
    let mut l = lab(
        vec![
            (
                "capabilities",
                caps(json!({"packages": true, "ai": false, "brain": false})),
            ),
            ("settings.store.desktop", desktop()),
            ("settings.store.profile", store("profile", 1, json!({}))),
        ],
        &[
            "settings.set",
            "system.status",
            "tools.launch",
            "ui.dispatch",
            "brain.open",
        ],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("system");
    let model = until(&settings, &at, has("status.release")).await;
    assert_eq!(l.call().await, ("system.status".into(), json!({})));
    let rows = items(&model);
    assert_eq!(row(&rows, "status.release")["value"], "0123456789ab");
    assert_eq!(row(&rows, "status.service:nacre-brain")["value"], "failed");
    assert_eq!(row(&rows, "status.drift")["value"], "2");
    assert_eq!(row(&rows, "status.config")["value"], "Valid");
    assert_eq!(row(&rows, "status.updates")["value"], "2 h ago");
    // Read-only: the status rows have no button, and the AUR policy shows its provenance.
    let aur = row(&rows, "system.packages.aurProvenance");
    assert_eq!(aur["options"][0]["id"], "do:open|aur-review-policy");
    assert!(aur["description"].as_str().unwrap().contains("required"));
    settings.act("do:open|aur-review-policy", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "aur-review-policy"}))
    );
    settle_calls(&settings, &at).await;
    // Welcome is a route to the shell; the updates view does not exist in Bead.
    assert_eq!(
        row(&rows, "system.welcome")["options"][0]["id"],
        "do:open|welcome"
    );
    assert!(rows.iter().all(|r| r["id"] != "system.updates"));
    settings.act("do:open|welcome", "", wake());
    assert_eq!(
        l.call().await,
        (
            "ui.dispatch".into(),
            json!({"target": "welcomeApp", "method": "open", "args": []})
        )
    );
    settle_calls(&settings, &at).await;
    // Add-ons: `nacre setup` for each missing one; Brain opens through its own method.
    let addons = items(&until(&settings, &stores("addons"), has("addon.ai")).await);
    assert_eq!(
        row(&addons, "addon.brain")["options"][0]["id"],
        "do:open|setup"
    );
    settings.act("do:open|setup", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "setup"}))
    );
    settle_calls(&settings, &at).await;
    settings.act("do:open|brain", "", wake());
    assert_eq!(l.call().await, ("brain.open".into(), json!({})));
    settle_calls(&settings, &at).await;
    // Anything outside the closed set is ignored.
    settings.act("do:open|rm -rf", "", wake());
    assert!(l.quiet());
}

#[tokio::test(flavor = "current_thread")]
async fn app_launch_buttons_use_the_closed_set_and_stay_dim_without_the_method() {
    let mut l = lab(
        vec![
            ("capabilities", caps(json!({"ai": true, "brain": true}))),
            ("settings.store.desktop", desktop()),
            ("settings.store.ai-settings", store("ai-settings", 1, json!({}))),
            ("audio", json!({"health": health(), "default_output": "", "default_input": "",
                "nodes": {"order": [], "items": {}}})),
            ("network", json!({"health": health(), "wireless_enabled": true,
                "devices": {"order": [], "items": {}},
                "access_points": {"order": [], "items": {}}, "connections": {"order": [], "items": {}}})),
        ],
        &["settings.set", "tools.launch", "ui.dispatch", "brain.open"],
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let ai = items(&until(&settings, &stores("nacre-ai"), has("ai.open")).await);
    assert_eq!(row(&ai, "ai.open")["options"][0]["id"], "do:open|sidebar");
    assert_eq!(
        row(&ai, "ai.accounts")["options"][0]["id"],
        "do:open|ai-workspace"
    );
    settings.act("do:open|sidebar", "", wake());
    assert_eq!(
        l.call().await,
        (
            "ui.dispatch".into(),
            json!({"target": "nacre", "method": "left", "args": []})
        )
    );
    let at = stores("nacre-ai");
    settle_calls(&settings, &at).await;
    settings.act("do:open|ai-workspace", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "ai-workspace"}))
    );
    settle_calls(&settings, &at).await;
    let sound = items(&until(&settings, &stores("sound"), has("sound.routing")).await);
    assert_eq!(
        row(&sound, "sound.routing")["options"][0]["id"],
        "do:open|audio-manager"
    );
    settings.act("do:open|audio-manager", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "audio-manager"}))
    );
    settle_calls(&settings, &at).await;
    let net = items(&until(&settings, &stores("network"), has("network.editor")).await);
    assert_eq!(
        row(&net, "network.editor")["options"][0]["id"],
        "do:open|connection-editor"
    );
    settings.act("do:open|connection-editor", "", wake());
    assert_eq!(
        l.call().await,
        ("tools.launch".into(), json!({"tool": "connection-editor"}))
    );
    drop(settings);
    // A daemon that advertises none of the methods: the buttons are dim and nothing is sent.
    let mut l2 = lab(
        vec![
            ("capabilities", caps(json!({"ai": true}))),
            ("settings.store.desktop", desktop()),
        ],
        &["settings.set"],
    )
    .await;
    let settings = Settings::new(Some(l2.client.clone()), &root()).unwrap();
    let ai = items(&until(&settings, &stores("nacre-ai"), has("ai.open")).await);
    assert_eq!(row(&ai, "ai.accounts")["options"][0]["enabled"], false);
    settings.act("do:open|ai-workspace", "", wake());
    assert!(l2.quiet());
    let _ = &mut l;
}

// ---- displays (B0089, Nacre N0063) -------------------------------------------------------------------

fn display_output(name: &str, x: i64, scale: f64, modes: Vec<String>, current: &str) -> Json {
    json!({"name": name, "make": "Maker", "model": "Model", "enabled": true, "width": 1920, "height": 1080,
        "refresh_hz": 60.0, "mode": current, "modes": modes, "scale": scale, "x": x, "y": 0, "transform": 0})
}
fn few_modes() -> Vec<String> {
    [
        "1920x1080@60.000Hz",
        "1920x1080@144.000Hz",
        "1280x720@60.000Hz",
    ]
    .map(String::from)
    .to_vec()
}
fn many_modes() -> Vec<String> {
    (0..12)
        .map(|i| format!("{}x{}@60.000Hz", 640 + i * 160, 360 + i * 90))
        .collect()
}
fn displays_topic(mirror: bool, pending: Option<u64>) -> Json {
    let mut topic = json!({"health": health(), "can_configure": true, "can_mirror": mirror,
        "scales": [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0],
        "outputs": [display_output("DP-1", 0, 1.0, few_modes(), "1920x1080@60.000Hz"),
                    display_output("HDMI-A-1", 1920, 1.25, many_modes(), "640x360@60.000Hz")]});
    if let Some(deadline) = pending {
        topic["pending"] = json!({"seconds": 20, "deadline_ms": deadline});
    }
    topic
}
const DISPLAY_METHODS: [&str; 3] = ["display.apply", "display.confirm", "display.revert"];

#[tokio::test(flavor = "current_thread")]
async fn displays_page_draws_the_qml_cards_from_the_topic() {
    let l = lab(
        vec![
            ("displays", displays_topic(false, None)),
            ("settings.store.desktop", desktop()),
        ],
        &DISPLAY_METHODS,
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("displays");
    let model = until(&settings, &at, |m| {
        items(m)
            .iter()
            .any(|r| r["id"] == "section:displays.out.DP-1")
    })
    .await;
    // One registry card ("Connected devices", as QML) holds the parts QML drew.
    assert_eq!(nested_items(&model).len(), 1);
    let rows = items(&model);
    let parts: Vec<&str> = rows
        .iter()
        .filter(|r| r["kind"] == "section")
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert_eq!(
        parts,
        [
            "section:displays.devices",
            "section:displays",
            "section:displays.out.DP-1",
            "section:displays.out.HDMI-A-1"
        ],
        "no Keep part while nothing is pending"
    );
    let part = |id: &str| row(&rows, id).clone();
    // First part: title, the niri mirroring sentence, primary chips and the two arrangements (no Mirror).
    assert_eq!(part("section:displays")["label"], "Displays");
    assert!(
        part("section:displays")["description"]
            .as_str()
            .unwrap()
            .contains("mirroring is unavailable")
    );
    assert_eq!(
        option_args(row(&rows, "displays.primary")),
        ["do:disp-primary|DP-1", "do:disp-primary|HDMI-A-1"]
    );
    assert_eq!(
        row(&rows, "displays.primary")["options"][0]["selected"],
        true
    );
    assert_eq!(
        option_args(row(&rows, "displays.arrange")),
        [
            "do:disp-arrange|extend-right",
            "do:disp-arrange|extend-left"
        ]
    );
    // A part per output: QML's title, the 20 s note, the scale chips and the mode choices.
    assert_eq!(
        part("section:displays.out.DP-1")["label"],
        "DP-1 · 1920×1080 · 60 Hz"
    );
    assert_eq!(
        part("section:displays.out.DP-1")["description"],
        "Display changes revert after 20 seconds unless you keep them."
    );
    let scale = row(&rows, "displays.out.DP-1.scale");
    assert_eq!(scale["kind"], "select", "QML draws the scale as a drop-down");
    assert_eq!(scale["label"], "Scale");
    assert_eq!(scale["text"], "100%");
    let labels: Vec<&str> = scale["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["100%", "125%", "150%", "175%", "200%", "250%", "300%"]
    );
    assert_eq!(scale["options"][0]["selected"], true);
    assert_eq!(
        row(&rows, "displays.out.HDMI-A-1.scale")["options"][1]["selected"],
        true,
        "125 %"
    );
    assert_eq!(row(&rows, "displays.out.HDMI-A-1.scale")["text"], "125%");
    let mode = row(&rows, "displays.out.DP-1.mode");
    assert_eq!(mode["kind"], "select", "the modes are a drop-down too");
    assert_eq!(mode["options"][0]["selected"], true);
    assert_eq!(mode["options"][1]["id"], "do:disp-mode|DP-1|1");
    assert!(mode["filter"].is_null(), "few modes need no search");
    let long = row(&rows, "displays.out.HDMI-A-1.mode");
    assert_eq!(
        (
            long["kind"].clone(),
            long["options"].as_array().unwrap().len()
        ),
        (json!("select"), 12)
    );
    assert_eq!(long["filter"], "Filter modes");
    assert!(long["more"].is_null(), "the whole list scrolls in the popup");
    // The filter field narrows the long list by the typed text, case-insensitively.
    let filtered = stores("displays")
        .reconcile(&BTreeMap::from([(
            "settings.pick.text".into(),
            Value::Text("1920X1080".into()),
        )]))
        .unwrap();
    settings.project(&filtered);
    let rows = items(&settings.project(&filtered));
    assert_eq!(
        row(&rows, "displays.out.HDMI-A-1.mode")["options"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // A compositor that can mirror offers it.
    drop(settings);
    let l = lab(
        vec![("displays", displays_topic(true, None))],
        &DISPLAY_METHODS,
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let rows = items(&until(&settings, &at, has("displays.arrange")).await);
    assert_eq!(
        option_args(row(&rows, "displays.arrange")).last().unwrap(),
        "do:disp-arrange|mirror"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn displays_page_sends_the_exact_contract_calls_and_nothing_unpublished() {
    let mut l = lab(
        vec![
            ("displays", displays_topic(false, None)),
            ("settings.store.desktop", desktop()),
        ],
        &DISPLAY_METHODS,
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("displays");
    until(&settings, &at, has("displays.arrange")).await;
    // The primary is local state; the arrangement goes out with it (QML `layout`).
    settings.act("do:disp-arrange|extend-left", "", wake());
    assert_eq!(
        l.call().await,
        (
            "display.apply".into(),
            json!({"output": "DP-1", "arrangement": "extend-left"})
        )
    );
    settle_calls(&settings, &at).await;
    settings.act("do:disp-primary|HDMI-A-1", "", wake());
    settings.act("do:disp-arrange|extend-right", "", wake());
    assert_eq!(
        l.call().await.1,
        json!({"output": "HDMI-A-1", "arrangement": "extend-right"})
    );
    settle_calls(&settings, &at).await;
    // Scale keeps the mode; a mode keeps the output's scale (QML `edit`).
    settings.act("do:disp-scale|DP-1|1.5", "", wake());
    assert_eq!(l.call().await.1, json!({"output": "DP-1", "scale": 1.5}));
    settle_calls(&settings, &at).await;
    settings.act("do:disp-mode|DP-1|1", "", wake());
    assert_eq!(
        l.call().await.1,
        json!({"output": "DP-1", "scale": 1.0, "mode": "1920x1080@144.000Hz"})
    );
    settle_calls(&settings, &at).await;
    // Nothing the topic does not list reaches the wire: mirror (niri), unknown output, scale,
    // mode index, a name that breaks the argument split.
    for op in [
        "do:disp-arrange|mirror",
        "do:disp-arrange|stack",
        "do:disp-scale|nope|1.5",
        "do:disp-scale|DP-1|1.1",
        "do:disp-scale|DP-1|abc",
        "do:disp-mode|DP-1|7",
        "do:disp-mode|DP-1|-1",
        "do:disp-mode|nope|0",
        "do:disp-primary|nope",
        "do:disp-keep",
        "do:disp-revert",
    ] {
        settings.act(op, "", wake());
    }
    assert!(l.quiet(), "nothing is sent");
}

#[tokio::test(flavor = "current_thread")]
async fn displays_page_counts_down_and_keeps_or_reverts_a_pending_change() {
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 14_500;
    let mut l = lab(
        vec![
            ("displays", displays_topic(false, Some(deadline))),
            ("settings.store.desktop", desktop()),
        ],
        &DISPLAY_METHODS,
    )
    .await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("displays");
    let model = until(&settings, &at, has("displays.keep.buttons")).await;
    let rows = items(&model);
    let keep = row(&rows, "displays.keep.buttons");
    assert_eq!(option_args(keep), ["do:disp-keep", "do:disp-revert"]);
    assert_eq!(
        (
            keep["options"][0]["label"].clone(),
            keep["options"][1]["label"].clone()
        ),
        (json!("Keep"), json!("Revert"))
    );
    let text = row(&rows, "section:displays.keep")["description"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        text == "Reverting in 15 s" || text == "Reverting in 14 s",
        "{text}"
    );
    assert_eq!(
        row(&rows, "section:displays.keep")["label"],
        "Keep this display change?"
    );
    let at_part = |id: &str| rows.iter().position(|r| r["id"] == id).unwrap();
    assert!(
        at_part("section:displays.keep") < at_part("section:displays"),
        "the question comes first, where a 20 s countdown is seen"
    );
    // While pending every change control is off (QML `blocked`) and nothing new is sent.
    assert!(
        rows.iter()
            .filter(|r| r["id"]
                .as_str()
                .is_some_and(|i| i.starts_with("displays.out.") || i == "displays.arrange"))
            .all(|r| r["enabled"] == false)
    );
    settings.act("do:disp-scale|DP-1|2", "", wake());
    settings.act("do:disp-arrange|extend-left", "", wake());
    assert!(l.quiet());
    settings.act("do:disp-keep", "", wake());
    assert_eq!(l.call().await, ("display.confirm".into(), json!({})));
    settle_calls(&settings, &at).await;
    settings.act("do:disp-revert", "", wake());
    assert_eq!(l.call().await, ("display.revert".into(), json!({})));
    // A refusal says why: the daemon's `busy` is the QML sentence.
    settle_calls(&settings, &at).await;
    l.fail
        .lock()
        .unwrap()
        .insert("display.confirm".into(), ErrorCode::Busy);
    settings.act("do:disp-keep", "", wake());
    l.call().await;
    let model = until(&settings, &at, |m| {
        m["settings.message"]
            == Value::Text("Keep or revert the current display change first.".into())
    })
    .await;
    assert_eq!(
        model["settings.message"],
        Value::Text("Keep or revert the current display change first.".into())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn displays_page_is_read_only_without_the_methods_or_the_compositor_flag() {
    // Unadvertised methods: the controls are dim and nothing is sent.
    let mut l = lab(vec![("displays", displays_topic(true, None))], &[]).await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let at = stores("displays");
    let rows = items(&until(&settings, &at, has("displays.arrange")).await);
    assert!(
        row(&rows, "displays.out.DP-1.scale")["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["enabled"] == false)
    );
    settings.act("do:disp-scale|DP-1|2", "", wake());
    assert!(l.quiet());
    drop(settings);
    // A compositor that cannot configure outputs says so and offers nothing.
    let mut topic = displays_topic(false, None);
    topic["can_configure"] = json!(false);
    let l = lab(vec![("displays", topic)], &DISPLAY_METHODS).await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let model = until(&settings, &at, has("displays.arrange")).await;
    let rows = items(&model);
    assert!(
        row(&rows, "section:displays")["description"]
            .as_str()
            .unwrap()
            .starts_with("Display transactions are unavailable")
    );
    assert!(
        row(&rows, "displays.out.DP-1.scale")["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["enabled"] == false)
    );
    drop(settings);
    // No topic at all: one line, no controls.
    let l = lab(vec![], &DISPLAY_METHODS).await;
    let settings = Settings::new(Some(l.client.clone()), &root()).unwrap();
    let rows = items(&until(&settings, &at, has("displays.unavailable")).await);
    assert!(rows.iter().all(|r| r["kind"] != "actions"));
}

/// The window draws the page while every row is off (no daemon, or a change pending): the kit builds
/// each chip's action, so a dim row must still name one. A runtime error shows as a failed pose.
#[test]
fn displays_page_draws_while_every_row_is_off() {
    use super::settings_tests::harness;
    let mut fixed: BTreeMap<String, Arc<dyn Source>> = BTreeMap::new();
    for (topic, data) in topics_for_goldens() {
        fixed.insert(
            topic.into(),
            Arc::new(bead_test::source::FakeSource::new(data).unwrap()),
        );
    }
    let settings = Settings::new(None, &root()).unwrap().with_sources(fixed);
    let at = stores("displays");
    settings.project(&at);
    let rows = items(&settings.project(&at));
    assert!(
        row(&rows, "displays.out.DP-1.scale")["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["enabled"] == false),
        "no daemon: every chip is off"
    );
    let mut h = harness(settings.project(&at), 960, 640, 1.);
    h.pose((0., 0.), 1.).unwrap();
}

// ---- goldens ----------------------------------------------------------------------------------------

/// The enabled look a connected daemon gives: every row and button of the projected page enabled.
fn enable_all(rows: &mut Json) {
    match rows {
        Json::Object(map) => {
            if map.get("enabled") == Some(&json!(false)) {
                map.insert("enabled".into(), json!(true));
            }
            for value in map.values_mut() {
                enable_all(value);
            }
        }
        Json::Array(list) => list.iter_mut().for_each(enable_all),
        _ => {}
    }
}
/// The countdown text changes every second; the golden shows a fixed one.
fn normalize_countdown(rows: &mut Json) {
    match rows {
        Json::Object(map) => {
            if map.get("id") == Some(&json!("section:displays.keep")) {
                map.insert("description".into(), json!("Reverting in 14 s"));
            }
            map.values_mut().for_each(normalize_countdown);
        }
        Json::Array(list) => list.iter_mut().for_each(normalize_countdown),
        _ => {}
    }
}
fn topics_for_goldens() -> Vec<(&'static str, Json)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let notice = |summary: &str, body: &str, minutes: u64| {
        json!({"app_name": "Mail", "summary": summary, "body": body, "created_ms": now - minutes * 60_000,
            "urgency": "normal", "resident": false, "transient": false, "actions": {"order": [], "items": {}}})
    };
    let node = |name: &str, kind: &str, volume: f64, muted: bool| json!({"name": name, "kind": kind, "volume": volume, "muted": muted, "ready": true});
    vec![
        (
            "capabilities",
            caps(
                json!({"input.controls": true, "touchpad": true, "input.touchpadSpeed": true,
            "powerProfiles": true, "packages": true, "ai": true, "brain": true, "battery": true, "bluetooth": true}),
            ),
        ),
        (
            "time",
            json!({"health": health(), "timezone": "Europe/Oslo", "ntp": {"supported": true, "enabled": true}}),
        ),
        (
            "notifications",
            json!({"health": health(), "do_not_disturb": false, "active": {"order": [], "items": {}},
            "history": {"order": ["a", "b", "c"], "items": {
                "a": notice("Build finished", "All 421 tests passed on the desktop job", 4),
                "b": notice("Reminder", "Stand-up in ten minutes", 95),
                "c": notice("Update available", "Three packages can be upgraded", 3000)}},
            "history_count": 3, "history_more": false}),
        ),
        (
            "settings.desktop",
            json!({"data": {"preset": "focused"}, "message": ""}),
        ),
        (
            "audio",
            json!({"health": health(), "default_output": "out", "default_input": "in",
            "nodes": {"order": ["out", "hdmi", "in", "app"], "items": {
                "out": node("Built-in speakers", "output", 0.216, false),
                "hdmi": node("Monitor audio", "output", 0.5, true),
                "in": node("Headset microphone", "input", 0.512, false),
                "app": node("Music player", "playback", 0.125, false)}}}),
        ),
        (
            "network",
            json!({"health": health(), "wireless_enabled": true, "devices": {"order": [], "items": {}},
            "access_points": {"order": ["home", "cafe", "guest"], "items": {
                "home": {"device": "wlan0", "name": "Home network", "strength": 82, "secured": true, "active": true, "connection": "c1"},
                "cafe": {"device": "wlan0", "name": "Cafe", "strength": 55, "secured": false, "active": false},
                "guest": {"device": "wlan0", "name": "Neighbour", "strength": 31, "secured": true, "active": false}}},
            "connections": {"order": ["c1", "c2"], "items": {
                "c1": {"name": "Home network", "kind": "wifi", "active": true, "ssid": "Home network"},
                "c2": {"name": "Office", "kind": "wifi", "active": false, "ssid": "Office"}}}}),
        ),
        (
            "bluetooth",
            json!({"health": health(),
            "adapters": {"order": ["hci0"], "items": {"hci0": {"name": "Laptop adapter", "powered": true, "discovering": false}}},
            "devices": {"order": ["d1", "d2", "d3"], "items": {
                "d1": {"adapter": "hci0", "name": "Wireless earbuds", "connected": true, "paired": true, "trusted": true, "pairing": false, "battery": 80},
                "d2": {"adapter": "hci0", "name": "Speaker", "connected": false, "paired": true, "trusted": false, "pairing": false},
                "d3": {"adapter": "hci0", "name": "Keyboard", "connected": false, "paired": false, "trusted": false, "pairing": false}}}}),
        ),
        (
            "displays",
            json!({"health": health(), "can_configure": true, "can_mirror": false,
            "scales": [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0],
            "outputs": [display_output("DP-1", 0, 1.0, few_modes(), "1920x1080@60.000Hz"),
                        display_output("HDMI-A-1", 1920, 1.25, many_modes(), "640x360@60.000Hz")]}),
        ),
        (
            "battery",
            json!({"health": health(), "percentage": 78.0, "charging": false}),
        ),
        (
            "settings.store.power",
            store(
                "power",
                1,
                json!({"profiles": {"enabled": true, "ac": "performance"}}),
            ),
        ),
        ("settings.store.profile", store("profile", 1, json!({}))),
        (
            "settings.store.software-input",
            store(
                "software-input",
                1,
                json!({"keyboardLayouts": "us,no", "touchpadEnabled": true}),
            ),
        ),
        (
            "shortcuts",
            json!({"health": health(), "items": {"order": ["k0", "k1", "k2"], "items": {
                "k0": {"key": "SUPER+Q", "description": "Close window"},
                "k1": {"key": "SUPER+SPACE", "description": "Open the launcher"},
                "k2": {"key": "SUPER+SHIFT+S", "description": "Take a screenshot"}}}}),
        ),
        (
            "settings.store.ai-settings",
            store("ai-settings", 1, json!({})),
        ),
    ]
}

#[test]
fn custom_page_goldens() {
    use super::settings_tests::{golden, harness};
    let topics = topics_for_goldens();
    let zones = [
        "Africa/Cairo",
        "America/Chicago",
        "America/New_York",
        "Asia/Tokyo",
        "Europe/Oslo",
        "Europe/Paris",
        "Europe/Stockholm",
        "Pacific/Auckland",
    ];
    let desktop = json!({"lockMedia": true, "lockWeather": false, "lockNotifications": true, "clockFormat": "24-hour", "preset": "focused"});
    for (name, page, filter, joining) in [
        (
            "settings-date-time-language-2",
            "date-time-language",
            "",
            false,
        ),
        (
            "settings-date-time-filtered-1",
            "date-time-language",
            "eur",
            false,
        ),
        ("settings-notifications-2", "notifications", "", false),
        (
            "settings-workflows-1",
            "windows-workspaces/workflows",
            "",
            false,
        ),
        ("settings-lock-login-2", "lock-login", "", false),
        ("settings-sound-1", "sound", "", false),
        ("settings-network-1", "network", "", false),
        ("settings-network-password-1", "network", "", true),
        ("settings-bluetooth-1", "bluetooth", "", false),
        ("settings-displays-1", "displays", "", false),
        ("settings-displays-keep-1", "displays", "", false),
        ("settings-power-1", "power", "", false),
        ("settings-keyboard-mouse-1", "keyboard-mouse", "", false),
        (
            "settings-keyboard-layouts-1",
            "keyboard-mouse",
            "nor",
            false,
        ),
        ("settings-privacy-1", "privacy", "", false),
        ("settings-system-1", "system", "", false),
        ("settings-nacre-ai-1", "nacre-ai", "", false),
        ("settings-addons-1", "addons", "", false),
        ("settings-brain-1", "brain", "", false),
    ] {
        let mut fixed: BTreeMap<String, Arc<dyn Source>> = BTreeMap::from([(
            "settings.store.desktop".to_string(),
            Arc::new(
                bead_test::source::FakeSource::new(store("desktop", 1, desktop.clone())).unwrap(),
            ) as Arc<dyn Source>,
        )]);
        for (topic, data) in &topics {
            let mut data = data.clone();
            if *topic == "capabilities" && page == "addons" {
                data["flags"]["ai"] = json!(false);
            }
            if *topic == "displays" && name == "settings-displays-keep-1" {
                data["pending"] = json!({"seconds": 20, "deadline_ms": 4_000_000_000_000_u64});
            }
            fixed.insert(
                (*topic).into(),
                Arc::new(bead_test::source::FakeSource::new(data).unwrap()),
            );
        }
        let settings = Settings::new(None, &root())
            .unwrap()
            .with_sources(fixed)
            .with_zones(&zones)
            .with_layouts(&[
                ("us", "English (US)"),
                ("no", "Norwegian"),
                ("de", "German"),
                ("nl", "Dutch"),
                ("fr", "French"),
            ])
            .with_status(status());
        let at = stores(page)
            .reconcile(&BTreeMap::from([(
                "settings.pick.text".into(),
                Value::Text(filter.into()),
            )]))
            .unwrap();
        settings.project(&at);
        if joining {
            settings.act("do:net-connect|guest", "", wake());
        }
        let mut model = settings.project(&at);
        let mut rows = Json::Array(nested_items(&model));
        enable_all(&mut rows);
        normalize_countdown(&mut rows);
        // (The fake topics are connected, but there is no daemon: methods are not advertised.)
        model.insert(
            "settings.items".into(),
            Value::list(rows.as_array().unwrap().clone()),
        );
        model.insert("settings.writable".into(), Value::Bool(true));
        model.insert("settings.search".into(), Value::Text(String::new()));
        model.insert("settings.pick.text".into(), Value::Text(filter.into()));
        let mut h = harness(model, 960, 640, 1.);
        golden(name, &mut h);
    }
}

#[test]
fn search_finds_the_new_pages_and_their_actions() {
    let settings = Settings::new(None, &root()).unwrap();
    let find = |query: &str| {
        let at = Stores::default()
            .reconcile(&BTreeMap::from([
                ("settings.page".into(), Value::Text("appearance".into())),
                ("settings.search".into(), Value::Text(query.into())),
            ]))
            .unwrap();
        items(&settings.project(&at))
    };
    // Pages by label, registry actions by label, settings by their words.
    assert!(
        find("bluetooth")
            .iter()
            .any(|r| r["id"] == "page:bluetooth")
    );
    assert!(find("sound").iter().any(|r| r["id"] == "page:sound"));
    assert!(
        find("lock screen")
            .iter()
            .any(|r| r["id"] == "action:lock-login.lock-now")
    );
    assert!(
        find("repeat")
            .iter()
            .any(|r| r["id"] == "setting:keyboard-mouse.keyboard.keyRepeatRate")
    );
    assert!(find("profile").iter().any(|r| r["page"] == "power"));
    assert!(find("clipboard").iter().any(|r| r["page"] == "privacy"));
}

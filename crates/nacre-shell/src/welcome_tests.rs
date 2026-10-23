// SPDX-License-Identifier: GPL-3.0-only
//! B0059: Welcome adapter (shortcut rows over a fake daemon, availability, action table), the
//! window's navigation and close through the runtime, and goldens for every tour page.
use super::welcome::*;
use crate::nacre_source::{Client, Config};
use bead::core::{Stores, Value, source::Source};
use bead_test::{Config as HarnessConfig, Harness};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn stores(page: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([(
            "welcome.step".into(),
            Value::Text(page.into()),
        )]))
        .unwrap()
}
fn rows_json() -> Vec<Json> {
    [
        ("binding-0", "Find an application", "Super + Space"),
        ("binding-1", "Open Settings", "Super + ,"),
        ("binding-2", "Close a window", "Super + Q"),
        ("binding-3", "Move focus", "Super + Arrow keys"),
        ("binding-4", "Capture an area", "Super + Shift + S"),
    ]
    .into_iter()
    .map(|(id, title, key)| json!({"id": id, "title": title, "key": key}))
    .collect()
}
fn text(model: &BTreeMap<String, Value>, key: &str) -> String {
    match &model[key] {
        Value::Text(t) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn without_a_daemon_nothing_optional_is_offered_and_the_window_size_is_projected() {
    let welcome = Welcome::new(None);
    welcome.set_viewport(900, 600);
    let model = welcome.project(&stores("welcome"));
    assert_eq!(model["welcome.width"], Value::Number(900.));
    assert_eq!(model["welcome.height"], Value::Number(600.));
    assert_eq!(model["welcome.can.settings"], Value::Bool(true));
    // Colors is offered once its app is registered; AI and Brain need the daemon's topics.
    assert_eq!(
        model["welcome.can.colors"],
        Value::Bool(crate::app::APPS.contains(&"colors"))
    );
    assert_eq!(model["welcome.can.ai"], Value::Bool(false));
    assert_eq!(model["welcome.can.brain"], Value::Bool(false));
    // No daemon: the shortcut page says so instead of waiting.
    assert_eq!(
        text(
            &welcome.project(&stores("shortcuts")),
            "welcome.shortcuts.state"
        ),
        "unavailable"
    );
}

#[test]
fn a_command_counts_only_as_an_executable_file_in_a_path_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("welcome-path-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, mode) in [("runs", 0o755), ("plain", 0o644)] {
        std::fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(mode)).unwrap();
    }
    let path = std::env::join_paths([Path::new("/nonexistent"), dir.as_path()]).unwrap();
    assert!(in_dirs("runs", &path));
    assert!(!in_dirs("plain", &path));
    assert!(!in_dirs("missing", &path));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn actions_select_from_fixed_tables_and_only_a_started_app_closes_the_window() {
    let welcome = Welcome::new(None);
    assert_eq!(welcome.act("close"), Outcome { close: true });
    // The shell is not configured here, and AI and Brain are not published: nothing starts.
    for arg in [
        "route:settings",
        "route:settings:system",
        "route:ai",
        "route:brain",
        "route:colors",
        "route:nope",
        "route:settings:../etc",
        "route:settings:",
        "link:nope",
        "link:",
        "other",
        "",
    ] {
        assert_eq!(welcome.act(arg), Outcome::default(), "{arg}");
    }
}

#[test]
fn shortcut_rows_come_from_the_menu_reply_in_engine_order_and_are_bounded() {
    let welcome = Welcome::new(None).with_shortcuts(rows_json());
    let model = welcome.project(&stores("shortcuts"));
    assert_eq!(text(&model, "welcome.shortcuts.state"), "ready");
    let Value::List(rows) = &model["welcome.shortcuts"] else {
        panic!()
    };
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0]["title"], "Find an application");
    assert_eq!(rows[0]["key"], "Super + Space");
    let empty = Welcome::new(None).with_shortcuts(Vec::new());
    assert_eq!(
        text(
            &empty.project(&stores("shortcuts")),
            "welcome.shortcuts.state"
        ),
        "empty"
    );
}

/// A real client against a fake daemon: the shortcut page asks `menu.query` once for the
/// `learn.shortcuts` route, reads name and key text, and AI and Brain count as installed while
/// their topics are published.
#[tokio::test(flavor = "current_thread")]
async fn shortcuts_are_read_over_the_wire_and_published_topics_enable_ai_and_brain() {
    use nacre_protocol::{Capability, Message, Reply, framing};
    use std::os::unix::fs::PermissionsExt;
    use tokio::{io::BufReader, net::UnixListener, sync::mpsc};
    let dir = std::env::temp_dir().join(format!("welcome-wire-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, mut calls) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        let mut methods = nacre_protocol::core_methods();
        methods.insert(
            "menu.query".into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 3000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
        let welcome = Message::Welcome {
            proto: 1,
            min: 1,
            boot: "welcome-boot".into(),
            topics: [("ai.chat".to_string(), 1), ("brain.recent".to_string(), 1)].into(),
            methods,
            frame_ms: None,
        };
        framing::write(&mut writer, &welcome).await.unwrap();
        while let Ok(Some(message)) = reader.next().await {
            if let Message::Call { id, m, p, .. } = message {
                calls_tx.send((m, p)).unwrap();
                let reply = Reply::Ok {
                    ok: json!({"query":"","mode":"apps","route":"learn.shortcuts","catalog_rev":1,
                        "items":{
                            "learn.shortcuts.binding-0":{"name":"Find an application","kind":"action","description":"Super + Space","breadcrumb":"Learn › Keyboard shortcuts"},
                            "learn.shortcuts.binding-1":{"name":"Open Settings","kind":"action","description":"Super + ,","breadcrumb":"Learn › Keyboard shortcuts"},
                            "learn.shortcuts.binding-2":{"name":"No keys","kind":"action","description":"","breadcrumb":""}},
                        "order":["learn.shortcuts.binding-1","learn.shortcuts.binding-0","learn.shortcuts.binding-2"],
                        "total":3}),
                };
                framing::write(&mut writer, &Message::Ret { id, reply })
                    .await
                    .unwrap();
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
    let welcome = Welcome::new(Some(client.clone()));
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    welcome.subscribe(Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    // Other pages never ask.
    let first = welcome.project(&stores("welcome"));
    assert_eq!(first["welcome.can.ai"], Value::Bool(true));
    // Brain also needs its command; the topic alone does not say it is installed.
    assert_eq!(
        first["welcome.can.brain"],
        Value::Bool(std::env::var_os("PATH").is_some_and(|p| in_dirs("nacre-brain", &p)))
    );
    assert!(calls.try_recv().is_err());
    let at = stores("shortcuts");
    let mut model = welcome.project(&at);
    assert_eq!(text(&model, "welcome.shortcuts.state"), "loading");
    for _ in 0..500 {
        model = welcome.project(&at);
        if text(&model, "welcome.shortcuts.state") == "ready" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (method, params) = calls.recv().await.unwrap();
    assert_eq!(method, "menu.query");
    assert_eq!(
        params,
        json!({"query":"","mode":"apps","route":"learn.shortcuts"})
    );
    assert_eq!(text(&model, "welcome.shortcuts.state"), "ready");
    let Value::List(rows) = &model["welcome.shortcuts"] else {
        panic!()
    };
    // Engine order is kept; a row without keys is not a shortcut.
    assert_eq!(
        rows.iter()
            .map(|r| r["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Open Settings", "Find an application"]
    );
    assert_eq!(rows[1]["key"], "Super + Space");
    assert!(
        woken.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "the reply woke the window"
    );
    // Projecting again never asks again for the same daemon boot.
    welcome.project(&at);
    assert!(calls.try_recv().is_err());
    drop(welcome);
    client.stop().await.unwrap();
    server.abort();
    std::fs::remove_dir_all(dir).ok();
}

fn harness(model: BTreeMap<String, Value>, width: u32, height: u32, scale: f32) -> Harness {
    // The built-in English table is installed once per process; do not race another test to it.
    crate::i18n::init();
    Harness::load(
        &root(),
        Path::new("welcome/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
    )
    .unwrap()
}
fn golden(name: &str, h: &mut Harness) {
    // The window fills its surface: no panel shadow inset.
    let frame = h.pose((0., 0.), 1.).unwrap();
    let path = root().join(format!("tests/goldens/{name}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path).unwrap();
    }
    frame
        .compare(
            &path,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-{name}-diff")),
        )
        .unwrap();
}
fn page_model(
    page: &str,
    guide: &str,
    ai: bool,
    colors: bool,
    size: (u32, u32),
) -> BTreeMap<String, Value> {
    let welcome = Welcome::new(None).with_shortcuts(rows_json());
    welcome.set_viewport(size.0, size.1);
    let mut model = welcome.project(&stores(page));
    for (key, on) in [
        ("welcome.can.ai", ai),
        ("welcome.can.brain", ai),
        ("welcome.can.colors", colors),
    ] {
        model.insert(key.into(), Value::Bool(on));
    }
    model.insert("welcome.step".into(), Value::Text(page.into()));
    model.insert("welcome.guide".into(), Value::Text(guide.into()));
    model
}

#[test]
fn welcome_pages_match_goldens() {
    for (name, page, guide, ai, colors, size, scale) in [
        ("welcome-hero-1", "welcome", "", true, true, (1020, 760), 1.),
        (
            "welcome-colors-1",
            "colors",
            "",
            true,
            true,
            (1020, 760),
            1.,
        ),
        (
            "welcome-shortcuts-1",
            "shortcuts",
            "",
            true,
            true,
            (1020, 760),
            1.,
        ),
        ("welcome-apps-1", "apps", "", true, true, (1020, 760), 1.),
        (
            "welcome-apps-optional-1",
            "apps",
            "",
            false,
            false,
            (1020, 760),
            1.,
        ),
        (
            "welcome-guide-1",
            "apps",
            "ai",
            false,
            false,
            (1020, 760),
            1.,
        ),
        ("welcome-ready-1", "ready", "", true, true, (1020, 760), 1.),
        ("welcome-help-1", "help", "", true, true, (1020, 760), 1.),
        ("welcome-apps-min-1", "apps", "", true, true, (720, 540), 1.),
        (
            "welcome-hero-1.5",
            "welcome",
            "",
            true,
            true,
            (1020, 760),
            1.5,
        ),
    ] {
        let model = page_model(page, guide, ai, colors, size);
        let mut h = harness(model, size.0, size.1, scale);
        golden(name, &mut h);
    }
}

fn runtime() -> (crate::reload::Runtime, Arc<std::sync::Mutex<Vec<String>>>) {
    use bead::wayland::{SceneSource, UiEvent};
    crate::i18n::init();
    let palette = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(root(), "welcome/main.luau".into(), palette, colors, cold);
    let mut runtime = crate::reload::Runtime::new(session)
        .with_welcome(Welcome::new(None).with_shortcuts(rows_json()))
        .unwrap();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = events.clone();
    runtime.services(Box::new(move |event| {
        log.lock().unwrap().push(match event {
            UiEvent::CompanionVisibility(false) => "close".to_string(),
            _ => "other".to_string(),
        });
    }));
    runtime.viewport(1020, 760);
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("welcome view loads");
    runtime.commit();
    (runtime, events)
}
fn click(runtime: &mut crate::reload::Runtime, id: &str) {
    use bead::wayland::{SceneSource, UiEvent};
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(&format!(".{id}")))
        .unwrap_or_else(|| panic!("no node {id}"))
        .draw
        .clone();
    let rect = bead::ui::rect(&node);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
}
fn page(runtime: &crate::reload::Runtime) -> String {
    match runtime.session.stores.get("welcome.step") {
        Some(Value::Text(t)) => t.clone(),
        other => panic!("{other:?}"),
    }
}

/// Back, Next and the step dots walk the tour; the last Next and Skip tour ask the window to
/// close; Escape does too, and Enter on the focused Next button advances.
#[test]
fn the_tour_navigates_and_closes_through_the_runtime() {
    use bead::wayland::{SceneSource, UiEvent};
    let (mut runtime, events) = runtime();
    let ids: Vec<String> = runtime
        .session
        .view_nodes()
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert!(
        ids.iter().any(|i| i == "welcome.body.footer.next"),
        "{ids:?}"
    );
    assert_eq!(page(&runtime), "welcome");
    click(&mut runtime, "next");
    assert_eq!(page(&runtime), "colors");
    click(&mut runtime, "back");
    assert_eq!(page(&runtime), "welcome");
    click(&mut runtime, "step-apps");
    assert_eq!(page(&runtime), "apps");
    // Enter activates the focused Next button.
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(page(&runtime), "ready");
    click(&mut runtime, "ready-help");
    assert_eq!(page(&runtime), "help");
    click(&mut runtime, "back");
    assert_eq!(page(&runtime), "ready");
    assert!(events.lock().unwrap().iter().all(|e| e != "close"));
    click(&mut runtime, "next");
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| *e == "close")
            .count(),
        1
    );
    // Skip tour on the first page closes without moving.
    click(&mut runtime, "step-welcome");
    click(&mut runtime, "back");
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| *e == "close")
            .count(),
        2
    );
    runtime
        .stage(UiEvent::Command {
            command: "escape".into(),
            shift: false,
        })
        .unwrap();
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| *e == "close")
            .count(),
        3
    );
}

/// The window's size reaches the layout: a resize projects again and the hero follows it.
#[test]
fn a_resize_projects_the_new_window_size() {
    use bead::wayland::{SceneSource, UiEvent};
    let (mut runtime, _) = runtime();
    runtime.viewport(800, 600);
    runtime.stage(UiEvent::Relayout).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("welcome.width"),
        Some(&Value::Number(800.))
    );
}

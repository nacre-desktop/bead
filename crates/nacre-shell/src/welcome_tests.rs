// SPDX-License-Identifier: GPL-3.0-only
//! B0059: Welcome adapter (shortcut rows over a fake daemon, availability, action table), the
//! window's navigation and close through the runtime, and goldens for every tour page.
use super::welcome::*;
use bead::core::{Stores, Value};
use bead_test::{Config as HarnessConfig, Harness};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path, sync::Arc};

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
    .map(|(id, title, key)| json!({"id": id, "title": title, "key": key, "group": if id == "binding-2" || id == "binding-3" { "Windows" } else { "" }, "menu": id == "binding-0" || id == "binding-1"}))
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
    assert_eq!(
        welcome.act("close", &Default::default()),
        Outcome {
            close: true,
            ..Outcome::default()
        }
    );
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
        assert_eq!(
            welcome.act(arg, &Default::default()),
            Outcome::default(),
            "{arg}"
        );
    }
}

#[test]
fn shortcut_fixtures_keep_their_order_and_empty_state() {
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
/// N0080's repaired topic can arrive after the window opens; keep compositor order and groups.
#[test]
fn shortcuts_topic_updates_wake_the_window_and_preserve_submaps() {
    use bead_test::source::FakeSource;
    let source = Arc::new(
        FakeSource::new(json!({"health":{"available":false},
        "items":{"order":[],"items":{}}}))
        .unwrap(),
    );
    let welcome = Welcome::new(None).with_source(source.clone());
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    welcome.subscribe(Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    assert_eq!(
        text(
            &welcome.project(&stores("shortcuts")),
            "welcome.shortcuts.state"
        ),
        "unavailable"
    );
    source
        .publish(json!({"health":{"available":true,"stale":false},"items":{
        "order":["settings","focus","empty"],"items":{
            "settings":{"description":"Open Settings","key":"Super + ,","menu":"settings"},
            "focus":{"description":"Move focus","key":"H J K L","submap":"Window navigation"},
            "empty":{"description":"No keys","key":""}}}}))
        .unwrap();
    let model = welcome.project(&stores("shortcuts"));
    assert_eq!(text(&model, "welcome.shortcuts.state"), "ready");
    let Value::List(rows) = &model["welcome.shortcuts"] else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["title"], "Open Settings");
    assert_eq!(rows[0]["menu"], true);
    assert_eq!(rows[1]["group"], "Window navigation");
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > 0);
    source.disconnect().unwrap();
    assert_eq!(
        text(
            &welcome.project(&stores("shortcuts")),
            "welcome.shortcuts.state"
        ),
        "ready",
        "last good rows remain readable after disconnect"
    );
}

fn harness(model: BTreeMap<String, Value>, width: u32, height: u32, scale: f32) -> Harness {
    // The built-in English table is installed once per process; do not race another test to it.
    crate::i18n::init();
    let mut icons = BTreeMap::new();
    if model.get("welcome.step") == Some(&Value::Text("colors".into())) {
        let folder = root().join("welcome/demo");
        let bundle: Json =
            serde_json::from_slice(&std::fs::read(folder.join("palettes.json")).unwrap()).unwrap();
        for (index, scene) in bundle["scenes"].as_array().unwrap().iter().enumerate() {
            icons.insert(
                format!("colors-thumb-{index}"),
                folder.join(scene["thumbnail"].as_str().unwrap()),
            );
        }
        icons.insert(
            "colors-card".into(),
            folder.join(bundle["scenes"][0]["image"].as_str().unwrap()),
        );
    }
    Harness::load_with_icons(
        &root(),
        Path::new("welcome/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
        icons,
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
    if page == "colors" {
        let demo = crate::colors::Colors::bundled_demo(&root()).unwrap();
        model.insert("welcome.demo".into(), Value::list(vec![demo.demo()]));
    }
    model.insert("welcome.step".into(), Value::Text(page.into()));
    model.insert("welcome.guide".into(), Value::Text(guide.into()));
    model
}

/// The demo on the colours page with a finished preview, from fixtures (no daemon).
#[test]
fn welcome_pages_match_goldens() {
    for (name, page, guide, ai, colors, size, scale) in [
        ("welcome-hero-1", "welcome", "", true, true, (1100, 800), 1.),
        (
            "welcome-colors-1",
            "colors",
            "",
            true,
            true,
            (1100, 800),
            1.,
        ),
        (
            "welcome-shortcuts-1",
            "shortcuts",
            "",
            true,
            true,
            (1100, 800),
            1.,
        ),
        ("welcome-apps-1", "apps", "", true, true, (1100, 800), 1.),
        (
            "welcome-apps-optional-1",
            "apps",
            "",
            false,
            false,
            (1100, 800),
            1.,
        ),
        (
            "welcome-guide-1",
            "apps",
            "ai",
            false,
            false,
            (1100, 800),
            1.,
        ),
        ("welcome-ready-1", "ready", "", true, true, (1100, 800), 1.),
        ("welcome-help-1", "help", "", true, true, (1100, 800), 1.),
        ("welcome-apps-min-1", "apps", "", true, true, (720, 540), 1.),
        (
            "welcome-hero-1.5",
            "welcome",
            "",
            true,
            true,
            (1100, 800),
            1.5,
        ),
    ] {
        let model = page_model(page, guide, ai, colors, size);
        let mut h = harness(model, size.0, size.1, scale);
        if size == (1100, 800) && guide.is_empty() {
            assert_fits(&h, page);
        }
        golden(name, &mut h);
    }
}

fn assert_fits(h: &Harness, page: &str) {
    let canvas = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(".canvas"))
        .expect("tour scroll canvas");
    assert_eq!(
        canvas.scroll_limit,
        Some(0.),
        "{page} overflows by {:?}",
        canvas.scroll_limit
    );
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
    runtime.viewport(1100, 800);
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

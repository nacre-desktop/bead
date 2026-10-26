// SPDX-License-Identifier: GPL-3.0-only
use super::*;

struct RecordingMenuSource {
    fixture: crate::launcher::Fixture,
    queries: std::sync::Mutex<Vec<serde_json::Value>>,
    calls: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
}

impl bead::core::source::Source for RecordingMenuSource {
    fn snapshot(&self) -> std::sync::Arc<bead::core::source::Snapshot> {
        self.fixture.snapshot()
    }

    fn subscribe(
        &self,
        callback: bead::core::source::Callback,
    ) -> bead::core::source::Result<bead::core::source::Subscription> {
        self.fixture.subscribe(callback)
    }

    fn invoke(&self, method: String, params: serde_json::Value) -> bead::core::source::Reply<'_> {
        if method == "menu.query" {
            self.queries.lock().unwrap().push(params.clone());
        }
        self.calls
            .lock()
            .unwrap()
            .push((method.clone(), params.clone()));
        self.fixture.invoke(method, params)
    }

    fn reconnect(&self) -> bead::core::source::Reply<'_> {
        self.fixture.reconnect()
    }

    fn stop(&self) -> bead::core::source::Reply<'_> {
        self.fixture.stop()
    }
}

#[test]
fn first_panel_reload_primes_query_before_markup_load() {
    let palette =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let fallback = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        palette.parent().unwrap().into(),
        "missing-launcher-entry.luau".into(),
        palette,
        colors,
        fallback,
    );
    let fixture = crate::launcher::Fixture::new(serde_json::json!({
        "queries": {"": crate::launcher::fixture_reply("", vec![])}
    }))
    .unwrap();
    let source = std::sync::Arc::new(RecordingMenuSource {
        fixture,
        queries: std::sync::Mutex::new(Vec::new()),
        calls: std::sync::Mutex::new(Vec::new()),
    });
    let memory = std::sync::Arc::new(std::sync::Mutex::new(
        crate::key_routes::Launch {
            query: String::new(),
            route: None,
            toggle: false,
            target_view: None,
        }
        .model(),
    ));
    let mut runtime = Runtime::new(session)
        .with_launcher(Some(source.clone()))
        .unwrap()
        .with_launcher_memory(memory)
        .unwrap();
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());

    assert!(runtime.stage(UiEvent::Reload).is_err());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while source.queries.lock().unwrap().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "menu.query was not invoked"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let queries = source.queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["query"], "");
    assert_eq!(queries[0]["mode"], "apps");
    assert_eq!(queries[0]["route"], "home");
    assert!(runtime.pending.is_none());
    assert!(runtime.session.view_nodes().is_empty());
}

#[test]
fn explicit_launcher_query_resets_route_selection_and_scroll() {
    let changes = crate::key_routes::Launch {
        query: "mail".into(),
        route: None,
        toggle: true,
        target_view: None,
    }
    .model();
    assert_eq!(
        changes["launcher.search.text"],
        bead::core::Value::Text("mail".into())
    );
    assert_eq!(
        changes["launcher.mode"],
        bead::core::Value::Text("apps".into())
    );
    assert_eq!(
        changes["launcher.route"],
        bead::core::Value::Text("apps".into())
    );
    assert_eq!(
        changes["launcher.engine_route"],
        bead::core::Value::Text(String::new())
    );
    assert_eq!(
        changes["launcher.selection"],
        bead::core::Value::Text(String::new())
    );
    assert_eq!(changes["launcher.scroll.y"], bead::core::Value::Number(0.));
}

#[test]
fn every_launcher_panel_open_returns_focus_to_search() {
    let changes = panel_visibility_changes(true, true);
    assert_eq!(changes["panel.open"], bead::core::Value::Bool(true));
    assert_eq!(
        changes["bead.focused"],
        bead::core::Value::Text("launcher.search".into())
    );
    assert!(!panel_visibility_changes(false, true).contains_key("bead.focused"));
    assert!(!panel_visibility_changes(true, false).contains_key("bead.focused"));
}

#[test]
fn runtime_scroll_over_launcher_actions_only_scrolls_and_click_still_activates() {
    crate::i18n::init();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let fallback = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session =
        bead::ui::Session::new(root, "ui/main.luau".into(), palette, colors, fallback);
    session.set_viewport(1920, 1200);
    let rows = (0..81)
        .map(|index| {
            serde_json::json!({
                "id": format!("app-{index}"),
                "name": format!("Fixture application {index}"),
                "description": "Runtime launcher scroll regression",
                "icon": "applications-other",
            })
        })
        .collect();
    let source = std::sync::Arc::new(RecordingMenuSource {
        fixture: crate::launcher::Fixture::new(serde_json::json!({
            "queries": {"": crate::launcher::fixture_reply("", rows)}
        }))
        .unwrap(),
        queries: std::sync::Mutex::new(Vec::new()),
        calls: std::sync::Mutex::new(Vec::new()),
    });
    let mut runtime = Runtime::new(session)
        .with_launcher(Some(source.clone()))
        .unwrap();
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.stage(UiEvent::Reload).unwrap();
    runtime.commit();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if matches!(runtime.session.stores.get("launcher.apps"),
            Some(bead::core::Value::List(rows)) if rows.len() == 81)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "launcher query did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let viewport = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.scroll_store.as_deref() == Some("launcher.scroll.y"))
        .expect("real launcher markup has a scroll viewport");
    assert!(viewport.scroll_limit.is_some_and(|limit| limit > 7.));
    let row = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("select_app"))
        .expect("runtime fixture projects a selectable app");
    let selected_id = match &row.action_arg {
        Some(bead::core::Value::Text(id)) => id.clone(),
        _ => panic!("launcher row lost its ID"),
    };
    let rect = bead::ui::rect(&row.draw);
    let point = (rect.x + rect.w / 2., rect.y + rect.h / 2.);

    for (delta, expected) in [(7., 7.), (-7., 0.)] {
        runtime
            .stage(UiEvent::Scroll {
                x: point.0,
                y: point.1,
                delta,
            })
            .unwrap();
        runtime.commit();
        let calls = source.calls.lock().unwrap().clone();
        eprintln!(
            "launcher scroll delta={delta}: offset={:?}, calls={calls:?}",
            runtime.session.stores.get("launcher.scroll.y")
        );
        assert_eq!(
            runtime.session.stores.get("launcher.scroll.y"),
            Some(&bead::core::Value::Number(expected)),
            "scroll over selectable launcher row must not dispatch its action; calls={calls:?}"
        );
        assert!(
            !calls.iter().any(|(method, params)| {
                method == "menu.activate"
                    || (method == "menu.preferences" && params["action"] == "favorite")
            }),
            "scroll must not activate or favorite a row: {calls:?}"
        );
    }

    runtime
        .stage(UiEvent::Hover {
            x: point.0,
            y: point.1,
        })
        .unwrap();
    runtime.commit();
    let favorite = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.action.as_deref() == Some("toggle_favorite")
                && node.action_arg == Some(bead::core::Value::Text(selected_id.clone()))
        })
        .expect("hovered launcher row exposes its actual favorite badge");
    let favorite_rect = bead::ui::rect(&favorite.draw);
    let favorite_point = (
        favorite_rect.x + favorite_rect.w / 2.,
        favorite_rect.y + favorite_rect.h / 2.,
    );
    for (delta, expected) in [(7., 7.), (-7., 0.)] {
        runtime
            .stage(UiEvent::Scroll {
                x: favorite_point.0,
                y: favorite_point.1,
                delta,
            })
            .unwrap();
        runtime.commit();
        let calls = source.calls.lock().unwrap().clone();
        assert_eq!(
            runtime.session.stores.get("launcher.scroll.y"),
            Some(&bead::core::Value::Number(expected)),
            "scroll over favorite badge must scroll, not toggle it; calls={calls:?}"
        );
        assert!(
            !calls.iter().any(|(method, params)| {
                method == "menu.activate"
                    || (method == "menu.preferences" && params["action"] == "favorite")
            }),
            "scroll over favorite badge must not invoke a menu action: {calls:?}"
        );
    }

    runtime
        .stage(UiEvent::Click {
            x: point.0,
            y: point.1,
        })
        .unwrap();
    runtime.commit();
    assert!(
        source
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|(method, params)| method == "menu.activate" && params["id"] == selected_id),
        "a real click must still invoke the selected app"
    );
}

// SPDX-License-Identifier: GPL-3.0-only
use super::*;

struct RecordingMenuSource {
    fixture: crate::launcher::Fixture,
    queries: std::sync::Mutex<Vec<serde_json::Value>>,
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

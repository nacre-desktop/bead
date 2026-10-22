// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn explicit_launcher_query_resets_route_selection_and_scroll() {
    let changes = crate::key_routes::Launch {
        query: "mail".into(),
        route: None,
        toggle: true,
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

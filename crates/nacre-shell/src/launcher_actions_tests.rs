// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use serde_json::json;

/// Published daemon-to-provider rows, in transcript order (provider-1 ... provider-5).
fn golden_rows() -> Vec<Value> {
    include_str!("../tests/protocol-v1/menu-ui-actions.jsonl")
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|f| f["m"] == "ui.menu.activate" && f["id"].as_str().is_some())
        .map(|f| f["p"].clone())
        .collect()
}
fn row(operation: &str, value: Value) -> Value {
    json!({"row":{"id":"r","kind":"action","name":"R","operation":operation,"value":value,"disabled":false},"catalog_rev":3})
}
fn planned(params: Value) -> (Act, bool, Option<MenuView>) {
    let (act, reply) = plan(params.clone()).unwrap_or_else(|e| panic!("{params}: {}", e.message));
    (act, reply.keep_open, reply.view)
}
/// Forms Bead has no inline editor for yet (B0197 known gaps: the QML launcher drew them).
const FORM_GAPS: [&str; 5] = [
    "reminder",
    "tui",
    "webapp",
    "software-theme-import",
    "software-extension-import",
];
const DAEMON_METHODS: [&str; 18] = [
    "appearance.set_mode",
    "wallpaper.next",
    "wallpaper.activate",
    "power.set_profile",
    "audio.set_default",
    "window.focus",
    "recorder.start",
    "recorder.stop",
    "clipboard.copy",
    "session.request",
    "settings.get",
    "settings.set",
    "settings.desktop",
    "player.play_pause",
    "player.next",
    "player.previous",
    "ai.chat.new",
    "ai.chat.send",
];

#[test]
fn published_rows_map_to_the_operations_bead_can_do_and_refuse_the_rest() {
    let rows = golden_rows();
    assert_eq!(rows.len(), 5);
    let (act, keep, _) = planned(rows[0].clone());
    assert!(matches!(act, Act::Copy(ref t) if t == "2") && !keep);
    let (act, keep, _) = planned(rows[1].clone());
    // Restart is confirmed in Bead's own session view, never executed here.
    assert!(
        matches!(act, Act::Shell(ref c) if c.target == "bead.panel" && c.method == "open" && c.args == [json!("session")])
            && !keep
    );
    let (act, _, _) = planned(rows[2].clone());
    assert!(matches!(act, Act::CopyClip(ref id) if id == "fixture-image-1"));
    let (act, keep, _) = planned(rows[3].clone());
    assert!(
        matches!(act, Act::Run { ref argv, ref dir, .. } if argv == &["fixture-app", "space & literal", "$(literal)"] && dir.as_deref() == Some(std::path::Path::new("/fixture")))
            && !keep
    );
    // The published refusal for an operation this UI does not know.
    let error = plan(rows[4].clone()).err().unwrap();
    assert_eq!(
        (error.code.as_str(), error.message.as_str(), error.retry),
        (
            "unavailable",
            "Launcher action is not supported by this UI",
            false
        )
    );
}

/// B0197 acceptance: every action row the launcher can show (the whole Nacre catalogue and
/// one row per provider operation) resolves to a route Bead serves. The only refusals are the
/// listed form gaps, and each resolved route is one Bead or the daemon actually publishes.
#[test]
fn every_visible_launcher_action_resolves_to_a_supported_route() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/launcher-actions.json")).unwrap();
    let navigation: Value = serde_json::from_str(include_str!(
        "../../../shell/settings/registry/navigation.json"
    ))
    .unwrap();
    let rows: Vec<&Value> = ["catalogue", "providers"]
        .iter()
        .flat_map(|k| fixture[*k].as_array().unwrap())
        .collect();
    assert!(rows.len() >= 140, "fixture shrank: {}", rows.len());
    let mut seen = std::collections::BTreeSet::new();
    let mut gaps = Vec::new();
    for row in rows {
        let operation = row["operation"].as_str().unwrap();
        seen.insert(operation.to_owned());
        let params = json!({"row": row, "catalog_rev": 1});
        if operation == "form" && FORM_GAPS.contains(&row["value"].as_str().unwrap()) {
            assert!(plan(params).is_err(), "{row}");
            gaps.push(row["value"].as_str().unwrap().to_owned());
            continue;
        }
        let (act, _, _) = planned(params);
        match act {
            Act::Shell(call) if call.target == "bead.panel" => {
                assert!(matches!(call.method.as_str(), "open" | "toggle"), "{row}")
            }
            Act::Shell(call) => {
                // The same validation `nacrectl ui` routes get, so Bead's command socket accepts it.
                crate::commands::validate(&call).unwrap_or_else(|_| panic!("{row}: {call:?}"));
                if call.method == "settings" {
                    // `Settings::page_for`: a page id, or an alias onto one.
                    let page = call.args[0].as_str().unwrap();
                    let page = navigation["aliases"][page].as_str().unwrap_or(page);
                    let page = page.split('#').next().unwrap();
                    assert!(
                        navigation["pages"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|p| p["id"] == page),
                        "settings page {page}"
                    );
                }
            }
            Act::Run { argv, .. } => assert!(!argv.is_empty() && !argv[0].is_empty(), "{row}"),
            Act::Daemon { method, .. } => assert!(DAEMON_METHODS.contains(&method), "{method}"),
            Act::Toggle(source) => assert!(
                [
                    "notifications.dnd",
                    "night-light.state",
                    "stay-awake.state",
                    "audio.output",
                    "audio.input",
                    "network.wifi",
                    "bluetooth.power"
                ]
                .contains(&source)
            ),
            Act::Player(method) => assert!(DAEMON_METHODS.contains(&method)),
            Act::View
            | Act::Copy(_)
            | Act::CopyClip(_)
            | Act::Lock
            | Act::Flip(_)
            | Act::Desktop(..)
            | Act::Preset(_)
            | Act::Wallpaper(_)
            | Act::Ask(_) => {}
        }
    }
    assert_eq!(gaps.len(), FORM_GAPS.len(), "{gaps:?}");
    for operation in KNOWN {
        assert!(seen.contains(*operation), "no fixture row for {operation}");
    }
}

#[test]
fn recording_and_capture_use_the_recorder_and_wait_for_the_launcher_to_close() {
    let (act, keep, _) = planned(row("tool", json!("record-start")));
    assert!(
        matches!(act, Act::Daemon { method: "recorder.start", ref params, delay: true } if params["mode"] == "region")
            && !keep
    );
    let (act, _, _) = planned(row("tool", json!("record-screen")));
    assert!(
        matches!(act, Act::Daemon { method: "recorder.start", ref params, .. } if params["mode"] == "screen")
    );
    let (act, _, _) = planned(row("tool", json!("record-stop")));
    assert!(matches!(
        act,
        Act::Daemon {
            method: "recorder.stop",
            delay: false,
            ..
        }
    ));
    let (act, _, _) = planned(row("tool", json!("capture-area")));
    assert!(
        matches!(act, Act::Run { ref argv, delay: true, .. } if argv[1..] == ["launcher-workflows", "capture", "capture-area"])
    );
    let (act, _, _) = planned(row("tool", json!("color-picker")));
    assert!(
        matches!(act, Act::Run { ref argv, delay: true, input: None, .. } if argv[1..] == ["menu-backend", "tool", "color-picker"])
    );
    let (act, _, _) = planned(row("tool", json!("updates")));
    assert!(matches!(act, Act::Run { ref argv, .. } if argv[0] == "kitty"));
}

#[test]
fn view_rows_change_the_launcher_and_toggles_keep_it_open() {
    let (act, keep, view) = planned(row("scope", json!("# ")));
    assert!(matches!(act, Act::View) && keep);
    assert_eq!(view.unwrap().query.as_deref(), Some("# "));
    let (_, keep, view) = planned(row("calculator", json!("")));
    assert!(keep && view.unwrap().query.as_deref() == Some("= "));
    let (act, keep, _) = planned(row("toggle", json!("mic-mute")));
    assert!(matches!(act, Act::Toggle("audio.input")) && keep);
    let (act, keep, _) = planned(row("setting-toggle", json!("blur")));
    assert!(matches!(act, Act::Flip(ref k) if k == "blur") && keep);
    let (act, _, _) = planned(row("media", json!("playPause")));
    assert!(matches!(act, Act::Player("player.play_pause")));
    // QML treats the wallpaper route row as the visual picker, not a menu navigation.
    let (act, keep, _) = planned(row("route", json!("wallpaper")));
    assert!(
        matches!(act, Act::Shell(ref c) if c.method == "mode" && c.args == [json!("wallpaper")])
            && keep
    );
}

#[test]
fn malformed_rows_and_bad_operands_fail_closed() {
    assert!(matches!(planned(row("power", json!("lock"))).0, Act::Lock));
    for bad in [
        row("route", json!("Bad Route!")),
        row("copy", json!(7)),
        row("copy-clip", json!("")),
        row("power", json!("reboot-now")),
        row("command", Value::Null),
        row("url", json!("file:///etc/passwd")),
        row("open-file", json!("relative/path")),
        row("toggle", json!("airplane-everything")),
        row("power-profile", json!("turbo")),
        row("settings", json!("../../etc")),
        row("tool", json!("rm -rf")),
        row("legacy", json!("nonsense")),
        json!({"row":{"id":"r","kind":"a","name":"R","operation":"copy","value":"x","disabled":true},"catalog_rev":3}),
        json!({"row":{"id":"r"},"catalog_rev":3}),
        json!({"row":{"id":"r","kind":"a","name":"R","operation":"copy","value":"x\u{0}"},"catalog_rev":3}),
    ] {
        let error = plan(bad.clone()).err().unwrap_or_else(|| panic!("{bad}"));
        assert_eq!(error.code, "validation", "{bad}");
    }
}

#[test]
fn search_uses_the_engine_url_and_encodes_the_fallback() {
    let mut params = row("search-web", json!("a b&c"));
    let (act, _, _) = planned(params.clone());
    assert!(
        matches!(act, Act::Run { ref argv, .. } if argv == &["nacre-app", "browser", "https://duckduckgo.com/?q=a%20b%26c"])
    );
    params["row"]["webUrl"] = json!("https://search.brave.com/search?q=a");
    let (act, _, _) = planned(params);
    assert!(
        matches!(act, Act::Run { ref argv, .. } if argv[2] == "https://search.brave.com/search?q=a")
    );
}

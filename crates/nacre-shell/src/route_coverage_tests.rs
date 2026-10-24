// SPDX-License-Identifier: GPL-3.0-only
//! B0068 drift guard: every route in Nacre's `shell-routes.json` (snapshot `shell/shell-routes.json`)
//! has a Bead handler below, or the table says why it has none. A route added to the snapshot without a
//! row here fails the test; so does a row whose example no longer maps as stated.
use crate::commands::{from_strings, map_views};
use serde_json::Value;

/// What an example of a route does when it reaches Bead's command socket.
#[derive(Debug)]
enum Handler {
    /// Rewritten to a Bead panel/view call (`target`, `method`).
    Panel(&'static str, &'static str),
    /// Launcher family: `key_routes::launcher` turns it into a launcher request.
    Launcher,
    /// Starts or forwards to an on-demand app (`app::route`; unconfigured in unit tests).
    App,
    /// Answered at once with an ok reply (documented no-op).
    Acknowledged,
    /// Passed through to the shell runtime, which calls the daemon (`reload.rs` handlers).
    Runtime(&'static str, &'static str),
    /// Validated, then refused with a stated reason (`invalid-route`: the example id is a placeholder).
    Refused(&'static str),
}
use Handler::*;

const ROUTES: &[(&str, Handler)] = &[
    ("nacre.launcher", Launcher),
    ("nacre.menu", Launcher),
    ("nacre.clipboard", Launcher),
    // overview and palette are daemon menu routes (windows list, style); QML's overview grid has no Bead view
    ("nacre.mode", Launcher),
    ("nacre.hide", Panel("bead.panel", "hide")),
    ("nacre.close", Panel("bead.panel", "close")),
    ("nacre.settings", App),
    ("nacre.left", App),
    ("nacre.controls", Runtime("nacre", "controls")),
    ("nacre.session", Panel("bead.panel", "toggle")),
    ("drawers.toggle", Runtime("drawers", "toggle")),
    ("brightness.step", Runtime("brightness", "step")),
    ("brightness.set", Runtime("brightness", "set")),
    ("keyboardLight.step", Runtime("keyboardLight", "step")),
    ("pearls.detach", Panel("bead.panel", "detach")),
    ("pearls.open", Panel("bead.panel", "pearl-open")),
    ("colorsApp.open", App),
    ("welcomeApp.open", App),
    ("welcomeApp.refresh", Acknowledged),
    ("menuPrompt.open", Refused("invalid-route")),
    ("menuPrompt.cancel", Refused("invalid-route")),
];

fn call(target: &str, method: &str, args: &[&str]) -> bead::services::ipc::Call {
    from_strings(
        target.into(),
        method.into(),
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn code(call: bead::services::ipc::Call) -> String {
    map_views(call).unwrap_err().error.unwrap().code
}

#[test]
fn every_nacre_route_has_a_bead_handler_or_a_stated_refusal() {
    let json: Value =
        serde_json::from_str(include_str!("../../../shell/shell-routes.json")).unwrap();
    let routes = json["routes"].as_array().unwrap();
    let mut seen = Vec::new();
    for route in routes {
        let id = route["id"].as_str().unwrap();
        let (_, expected) = ROUTES
            .iter()
            .find(|(row, _)| *row == id)
            .unwrap_or_else(|| {
                panic!("route {id} has no Bead handler row: add one or an exemption")
            });
        seen.push(id);
        let (target, method) = (
            route["target"].as_str().unwrap(),
            route["method"].as_str().unwrap(),
        );
        for example in route["examples"].as_array().unwrap() {
            let args: Vec<&str> = example
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap())
                .collect();
            let c = call(target, method, &args);
            let label = format!("{id} {args:?}");
            match expected {
                Panel(t, m) => {
                    let mapped = map_views(c).unwrap_or_else(|e| panic!("{label}: {e:?}"));
                    assert_eq!(
                        (mapped.target.as_str(), mapped.method.as_str()),
                        (*t, *m),
                        "{label}"
                    );
                }
                Launcher => {
                    let mapped = map_views(c).unwrap_or_else(|e| panic!("{label}: {e:?}"));
                    assert!(
                        matches!(crate::key_routes::launcher(&mapped), Some(Ok(_))),
                        "{label}"
                    );
                }
                App => {
                    let reply = map_views(c).unwrap_err();
                    assert_eq!(
                        reply.error.as_ref().map(|e| e.message.as_str()),
                        Some("apps are not configured in this shell"),
                        "{label}"
                    );
                }
                Acknowledged => {
                    let reply = map_views(c).unwrap_err();
                    assert!(reply.error.is_none(), "{label}");
                }
                Runtime(t, m) => {
                    let mapped = map_views(c).unwrap_or_else(|e| panic!("{label}: {e:?}"));
                    assert_eq!(
                        (mapped.target.as_str(), mapped.method.as_str()),
                        (*t, *m),
                        "{label}"
                    );
                }
                Refused(why) => assert_eq!(&code(c), why, "{label}"),
            }
        }
    }
    for (id, _) in ROUTES {
        assert!(
            seen.contains(id),
            "row {id} names no route in shell-routes.json"
        );
    }
}

#[test]
fn unsupported_arguments_are_refused_explicitly_not_dropped() {
    for (target, method, args) in [
        ("nacre", "controls", &["updates"][..]),
        ("drawers", "toggle", &["osd"][..]),
        ("drawers", "toggle", &["leftPinned"][..]),
        ("drawers", "toggle", &["dashboardPinned"][..]),
        ("drawers", "toggle", &["previewOnly"][..]),
    ] {
        assert_eq!(
            code(call(target, method, args)),
            "unavailable",
            "{target} {method} {args:?}"
        );
    }
}

#[test]
fn extra_sections_and_drawers_map_to_bead_views() {
    for (target, method, args, view, expected) in [
        ("nacre", "controls", &["session"][..], "session", "open"),
        (
            "nacre",
            "controls",
            &["notifications"][..],
            "dashboard-notifications",
            "open",
        ),
        ("drawers", "toggle", &["session"][..], "session", "toggle"),
    ] {
        let mapped = map_views(call(target, method, args)).unwrap();
        assert_eq!(mapped.target, "bead.panel");
        assert_eq!(mapped.method, expected);
        assert_eq!(mapped.args, vec![Value::String(view.into())]);
    }
    let launcher = map_views(call("drawers", "toggle", &["launcher"])).unwrap();
    assert!(matches!(crate::key_routes::launcher(&launcher), Some(Ok(l)) if l.toggle));
    // The control centre is a pearl kind now.
    let pearl = map_views(call("pearls", "open", &["controls"])).unwrap();
    assert_eq!(pearl.args, vec![Value::String("control".into())]);
}

#[test]
fn a_selector_prompt_is_answered_cancelled_so_the_tool_does_not_wait() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("bead-prompt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("stdin.json");
    let script = dir.join("helper.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s ' \"$@\" > {0}.args\ncat > {0}\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let ident = "0123456789abcdef0123456789abcdef";
    crate::commands::refuse_prompt_for_test(&script, ident).unwrap();
    let args = std::fs::read_to_string(format!("{}.args", out.display())).unwrap();
    assert_eq!(args.trim(), format!("menu-cli respond {ident}"));
    let response: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(response["cancelled"], true);
    assert_eq!(response["value"], "");
    assert!(response["error"].as_str().is_some_and(|e| !e.is_empty()));
    // A valid id is acknowledged; `cancel` has nothing to cancel.
    let cancel = map_views(call("menuPrompt", "cancel", &[ident])).unwrap_err();
    assert!(cancel.error.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

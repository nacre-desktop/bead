// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn slots(values: [&str; SLOTS]) -> [String; SLOTS] {
    values.map(str::to_owned)
}

#[test]
fn rows_open_the_forms_qml_drew() {
    for (operation, value, kind) in [
        ("form", "reminder", "reminder"),
        ("form", "tui", "tui"),
        ("form", "webapp", "webapp"),
        ("form", "software-theme-import", "theme-import"),
        ("form", "software-extension-import", "extension-import"),
        ("tool", "record-start", "recording-area"),
        ("tool", "record-screen", "recording-screen"),
        ("tool", "install-mise", "confirm-mise"),
        ("maintenance", "restart", "confirm-restart"),
    ] {
        assert_eq!(
            kind_for(operation, value),
            Some(kind),
            "{operation} {value}"
        );
    }
    // Saved actions, snippets and the rest still open their Settings page.
    assert_eq!(kind_for("form", "snippet"), None);
    assert_eq!(kind_for("maintenance", "check"), None);
}

#[test]
fn opening_sets_the_starting_values_and_focus() {
    let tui = open("tui", "");
    assert_eq!(tui["launcher.form.kind"], Value::Text("tui".into()));
    assert_eq!(tui["launcher.form.v2"], Value::Text("htop".into()));
    assert_eq!(tui["launcher.form.v4"], Value::Text("kitty".into()));
    assert_eq!(tui["bead.focused"], Value::Text(FIRST_FIELD.into()));
    assert_eq!(
        open("reminder", "")["launcher.form.v1"],
        Value::Text("25".into())
    );
    let recording = open("recording-screen", "Record screen");
    assert_eq!(recording["launcher.form.v1"], Value::Text("none".into()));
    assert_eq!(
        recording["launcher.form.message"],
        Value::Text("Record screen".into())
    );
    assert_eq!(recording["bead.focused"], Value::Text(String::new()));
    let closed = close();
    assert_eq!(closed["launcher.form.kind"], Value::Text(String::new()));
    assert_eq!(
        closed["bead.focused"],
        Value::Text("launcher.search".into())
    );
}

#[test]
fn reminder_and_web_app_validate_like_qml() {
    for bad in [["0", "x"], ["1441", "x"], ["abc", "x"], ["5", "  "]] {
        let values = slots([bad[0], bad[1], "", ""]);
        assert!(work("reminder", &values).is_err(), "{bad:?}");
    }
    let Ok(Work::Helper { argv, input }) = work("reminder", &slots(["0.1", "Tea", "", ""])) else {
        panic!("a valid reminder");
    };
    assert_eq!(argv[1..], ["menu-backend", "reminder", ""]);
    assert_eq!(input, json!({"minutes": "0.1", "message": "Tea"}));
    for bad in [["", "https://a.b"], ["Site", "ftp://a.b"], ["Site", "a.b"]] {
        assert!(
            work("webapp", &slots([bad[0], bad[1], "", ""])).is_err(),
            "{bad:?}"
        );
    }
    let Ok(Work::Helper { argv, input }) =
        work("webapp", &slots(["Site", " https://a.b ", "", ""]))
    else {
        panic!("a valid web app");
    };
    assert_eq!(argv[1..], ["menu-backend", "webapp-create", ""]);
    assert_eq!(input, json!({"label": "Site", "url": "https://a.b"}));
}

#[test]
fn terminal_app_and_imports_build_the_helper_documents() {
    let Ok(Work::Helper { argv, input }) = work(
        "tui",
        &slots(["Top", "htop\n\n--tree", "utilities-terminal", "foot"]),
    ) else {
        panic!("a valid terminal app");
    };
    assert_eq!(argv[1..], ["software-library", "tui-create", ""]);
    assert_eq!(
        input,
        json!({"name": "Top", "command": ["htop", "--tree"], "icon": "utilities-terminal", "terminal": "foot"})
    );
    assert!(work("tui", &slots(["Top", "\n", "i", "kitty"])).is_err());
    assert!(work("tui", &slots(["", "htop", "i", "kitty"])).is_err());
    assert!(work("tui", &slots(["Top", "htop", "i", "xterm"])).is_err());
    let Ok(Work::Helper { argv, input }) =
        work("theme-import", &slots([" /t/n.json ", "", "", ""]))
    else {
        panic!("a valid theme import");
    };
    assert_eq!(argv[1..], ["software-library", "theme-import", ""]);
    assert_eq!(input, json!({"path": "/t/n.json"}));
    let Ok(Work::Helper { argv, input }) = work(
        "extension-import",
        &slots(["https://github.com/o/e", "", "", ""]),
    ) else {
        panic!("a valid extension import");
    };
    assert_eq!(argv[1..], ["software-library", "import", ""]);
    assert_eq!(input, json!({"url": "https://github.com/o/e"}));
    assert!(work("theme-import", &slots(["  ", "", "", ""])).is_err());
    assert!(work("extension-import", &slots(["", "", "", ""])).is_err());
}

#[test]
fn recording_passes_the_sound_choice_to_the_daemon() {
    let none = work("recording-area", &slots(["none", "", "", ""])).unwrap();
    assert_eq!(
        none,
        Work::Daemon {
            method: "recorder.start",
            params: json!({"mode": "region"})
        }
    );
    assert!(none.closes_launcher());
    for (kind, audio, mode) in [
        ("recording-screen", "system", "screen"),
        ("recording-area", "mixed", "region"),
    ] {
        assert_eq!(
            work(kind, &slots([audio, "", "", ""])).unwrap(),
            Work::Daemon {
                method: "recorder.start",
                params: json!({"mode": mode, "audio": audio})
            }
        );
    }
    assert!(work("recording-area", &slots(["loud", "", "", ""])).is_err());
}

#[test]
fn confirmations_run_the_command_once_confirmed() {
    for (kind, tail) in [
        ("confirm-restart", &["maintenance", "restart"][..]),
        ("confirm-mise", &["software-package", "install", "mise"][..]),
    ] {
        let Ok(Work::Terminal(argv)) = work(kind, &slots(["", "", "", ""])) else {
            panic!("{kind}");
        };
        assert_eq!(argv[0], "kitty");
        assert!(argv.ends_with(&tail.iter().map(|s| s.to_string()).collect::<Vec<_>>()));
    }
}

#[test]
fn helper_answers_show_the_message_or_the_error() {
    assert_eq!(
        helper_answer(r#"{"message":"Reminder set"}"#, true),
        Ok("Reminder set".into())
    );
    assert_eq!(helper_answer("{}", true), Ok(String::new()));
    assert_eq!(
        helper_answer(r#"{"error":"Invalid URL"}"#, false),
        Err("Invalid URL".into())
    );
    assert!(helper_answer("", false).is_err());
    let run = Run::default();
    assert!(run.take().is_none());
    run.finish(Ok("done".into()));
    let (changes, message) = finished(run.take().unwrap());
    assert_eq!(message.as_deref(), Some("done"));
    assert_eq!(changes["launcher.form.kind"], Value::Text(String::new()));
    let (changes, message) = finished(Err("nope".into()));
    assert!(message.is_none());
    assert_eq!(changes["launcher.form.error"], Value::Text("nope".into()));
    assert_eq!(changes["launcher.form.busy"], Value::Bool(false));
}

#[test]
fn the_activation_reply_opens_the_form_and_leaves_the_engine_route_alone() {
    use nacre_protocol::menu::{MenuActivated, MenuView};
    let changes = crate::launcher::activation_changes(MenuActivated {
        keep_open: true,
        view: Some(MenuView {
            route: Some("form:tui".into()),
            query: Some("Create a TUI app".into()),
            ..MenuView::default()
        }),
    });
    assert_eq!(changes["panel.open"], Value::Bool(true));
    assert_eq!(changes["launcher.form.kind"], Value::Text("tui".into()));
    assert_eq!(
        changes["launcher.form.message"],
        Value::Text("Create a TUI app".into())
    );
    for untouched in [
        "launcher.engine_route",
        "launcher.search.text",
        "launcher.mode",
    ] {
        assert!(!changes.contains_key(untouched), "{untouched}");
    }
}

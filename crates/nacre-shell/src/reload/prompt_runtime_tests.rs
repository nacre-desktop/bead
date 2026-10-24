// SPDX-License-Identifier: GPL-3.0-only
//! B0093: the real prompt view with typing, arrows, Enter, Escape and the mouse, against a fake helper.
use super::*;
use crate::prompt::tests::{Fake, far, input_json, pick_json, root};
use serde_json::json;

fn runtime(fake: &Fake) -> Runtime {
    let palette = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(root(), "prompt/main.luau".into(), palette, colors, cold);
    let mut runtime = Runtime::new(session).with_prompt(fake.prompt()).unwrap();
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.services(Box::new(|_| {}));
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("prompt view loads");
    runtime.commit();
    runtime
}
fn step(runtime: &mut Runtime, event: UiEvent) {
    runtime.stage(event).unwrap();
    runtime.commit();
}
fn key(runtime: &mut Runtime, name: &str) {
    step(
        runtime,
        UiEvent::Command {
            command: name.into(),
            shift: false,
        },
    );
}
fn type_text(runtime: &mut Runtime, text: &str) {
    step(
        runtime,
        UiEvent::Key {
            text: text.into(),
            backspace: false,
        },
    );
}
/// What Enter answers after these keys: the row the keyboard highlighted ("" when nothing was answered).
fn answer_after(name: &str, keys: &[&str]) -> String {
    let fake = Fake::new(name, &pick_json(far()));
    let mut rt = runtime(&fake);
    for k in keys {
        key(&mut rt, k);
    }
    key(&mut rt, "enter");
    fake.responses()
        .first()
        .map(|r| r["value"].as_str().unwrap_or_default().to_owned())
        .unwrap_or_default()
}
fn open(runtime: &Runtime) -> bool {
    runtime.session.stores.get("panel.open") == Some(&bead::core::Value::Bool(true))
}
/// A mouse click (press and release) on the node that carries `prompt.do` with this argument.
fn click(runtime: &mut Runtime, arg: &str) {
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| {
            n.action.as_deref() == Some("prompt.do")
                && n.action_arg == Some(bead::core::Value::Text(arg.into()))
        })
        .unwrap_or_else(|| panic!("no {arg} button"));
    let rect = bead::ui::rect(&node.draw);
    let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
    step(runtime, UiEvent::Click { x, y });
    step(runtime, UiEvent::Release);
}

#[test]
fn typing_filters_and_enter_chooses_the_highlighted_row() {
    let fake = Fake::new("rt-type", &pick_json(far()));
    let mut rt = runtime(&fake);
    assert_eq!(
        rt.session.stores.get("bead.focused"),
        Some(&bead::core::Value::Text("prompt.search".into()))
    );
    type_text(&mut rt, "wor");
    key(&mut rt, "enter");
    assert_eq!(
        fake.responses(),
        [json!({"value":"ssid-workshop","cancelled":false,"error":""})]
    );
    assert!(!open(&rt), "the panel closes after the answer");
    // Nothing chosen yet: the first enabled row is the highlighted one.
    assert_eq!(answer_after("rt-first", &[]), "ssid-home");
    // Typing again restarts the highlight at the best match, wherever the arrows had left it.
    let fake = Fake::new("rt-restart", &pick_json(far()));
    let mut rt = runtime(&fake);
    key(&mut rt, "down");
    type_text(&mut rt, "g");
    key(&mut rt, "enter");
    assert_eq!(fake.responses()[0]["value"], "ssid-guest");
}

#[test]
fn arrows_skip_disabled_rows_and_stop_at_the_ends() {
    for (keys, expected) in [
        (&["up"][..], "ssid-home"),
        (&["down"][..], "ssid-guest"),
        (&["down", "down"][..], "ssid-workshop"), // c2 is disabled
        (&["down", "down", "down"][..], "ssid-workshop"),
        (&["down", "down", "up"][..], "ssid-guest"),
        (&["down", "up", "up"][..], "ssid-home"),
    ] {
        assert_eq!(answer_after("rt-arrows", keys), expected, "{keys:?}");
    }
}

#[test]
fn escape_cancels_and_a_filter_without_matches_answers_nothing() {
    let fake = Fake::new("rt-escape", &pick_json(far()));
    let mut rt = runtime(&fake);
    type_text(&mut rt, "zzzz");
    key(&mut rt, "enter");
    assert!(fake.responses().is_empty() && open(&rt));
    key(&mut rt, "escape");
    assert_eq!(
        fake.responses(),
        [json!({"value":"","cancelled":true,"error":""})]
    );
    assert!(!open(&rt));
}

#[test]
fn the_mouse_chooses_a_row_and_ignores_a_disabled_one() {
    let fake = Fake::new("rt-mouse", &pick_json(far()));
    let mut rt = runtime(&fake);
    assert!(
        !rt.session
            .view_nodes()
            .iter()
            .any(|n| n.action_arg == Some(bead::core::Value::Text("choose:c2".into()))),
        "a disabled row carries no action"
    );
    click(&mut rt, "choose:c3");
    assert_eq!(fake.responses()[0]["value"], "ssid-workshop");
    assert!(!open(&rt));
}

#[test]
fn input_enter_submits_the_edited_text_and_cancel_cancels() {
    let fake = Fake::new("rt-input", &input_json(far()));
    let mut rt = runtime(&fake);
    assert_eq!(
        rt.session.stores.get("bead.focused"),
        Some(&bead::core::Value::Text("prompt.value".into()))
    );
    type_text(&mut rt, "shop");
    key(&mut rt, "enter");
    let answer = &fake.responses()[0];
    assert_eq!(
        (
            answer["cancelled"].clone(),
            answer["value"].as_str().unwrap().ends_with("shop")
        ),
        (json!(false), true)
    );
    let cancel = Fake::new("rt-input-cancel", &input_json(far()));
    let mut rt = runtime(&cancel);
    click(&mut rt, "cancel");
    assert_eq!(
        cancel.responses(),
        [json!({"value":"","cancelled":true,"error":""})]
    );
    let submit = Fake::new("rt-input-continue", &input_json(far()));
    let mut rt = runtime(&submit);
    click(&mut rt, "submit");
    assert_eq!(submit.responses()[0]["value"], "Work");
    assert!(!open(&rt));
}

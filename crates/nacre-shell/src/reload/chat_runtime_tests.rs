// SPDX-License-Identifier: GPL-3.0-only
//! B0043: the real sidebar template, typing and keys, against a fake daemon.
// One test at a time owns the process-wide `send_failed` queue, across awaits.
#![allow(clippy::await_holding_lock)]
use super::*;
use crate::chat::tests::{Frame, chat_for, lab, meter, root, serial};
use serde_json::json;
use std::time::Duration;

fn runtime(chat: crate::chat::Chat) -> Runtime {
    let palette = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(root(), "chat/main.luau".into(), palette, colors, cold);
    let mut runtime = Runtime::new(session).with_chat(chat).unwrap();
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.services(Box::new(|_| {}));
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("chat template loads");
    runtime.commit();
    runtime
}
fn step(runtime: &mut Runtime, event: UiEvent) {
    runtime.stage(event).unwrap();
    runtime.commit();
}
fn draft(runtime: &Runtime) -> String {
    match runtime.session.stores.get("chat.draft") {
        Some(bead::core::Value::Text(t)) => t.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn typing_shift_enter_and_enter_send_the_exact_text_once() {
    let _guard = serial();
    let mut lab = lab(
        false,
        json!({"available":true,"channels":{"":{"state":{"defaultProvider":"claude","inWorkspace":false,"provider":"claude","account":"A","title":"T","threadId":"t","model":"","features":[],"composerKey":"k","busy":false,"status":"","error":""},"messages":{}}}}),
    )
    .await;
    let chat = chat_for(&lab).await;
    let _ = meter;
    let mut runtime = runtime(chat);
    for _ in 0..300 {
        step(&mut runtime, UiEvent::Model(BTreeMap::new()));
        if matches!(
            runtime.session.stores.get("chat.can_new"),
            Some(bead::core::Value::Bool(true))
        ) && matches!(runtime.session.stores.get("chat.heading"), Some(bead::core::Value::Text(t)) if t.starts_with("Claude"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        runtime.session.stores.get("bead.focused"),
        Some(&bead::core::Value::Text("chat.composer".into()))
    );
    step(
        &mut runtime,
        UiEvent::Key {
            text: "héllo".into(),
            backspace: false,
        },
    );
    step(
        &mut runtime,
        UiEvent::Command {
            command: "enter".into(),
            shift: true,
        },
    );
    step(
        &mut runtime,
        UiEvent::Key {
            text: "wörld".into(),
            backspace: false,
        },
    );
    assert_eq!(
        draft(&runtime),
        "héllo\nwörld",
        "Shift+Enter breaks the line and sends nothing"
    );
    assert!(lab.calls.try_recv().map_or(true, |c| c.0 == "ai.chat.open"));
    step(
        &mut runtime,
        UiEvent::Command {
            command: "enter".into(),
            shift: false,
        },
    );
    let sent = loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 == "ai.chat.send" {
            break call;
        }
    };
    assert_eq!(
        sent.1,
        json!({"channel":"","text":"héllo\nwörld","attachments":[]})
    );
    assert_eq!(
        draft(&runtime),
        "",
        "the composer clears once the call is queued"
    );
    // A reply streams in, then Esc closes the panel.
    lab.push
        .send(Frame::Msg(nacre_protocol::Message::Snap {
            topic: "ai.chat".into(),
            seq: 0,
            data: json!({"available":true,"channels":{"":{"state":{"defaultProvider":"claude","inWorkspace":false,"provider":"claude","account":"A","title":"T","threadId":"t","model":"","features":[],"composerKey":"k","busy":true,"status":"Working","error":""},"messages":{"0000000001":{"id":"u","role":"user","text":"héllo\nwörld"},"0000000002":{"id":"a","role":"assistant","text":"hi `there`"}}}}}),
        }))
        .unwrap();
    for _ in 0..300 {
        step(&mut runtime, UiEvent::Model(BTreeMap::new()));
        if matches!(
            runtime.session.stores.get("chat.empty"),
            Some(bead::core::Value::Bool(false))
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.id == "chat.actions.stop")
    );
    step(
        &mut runtime,
        UiEvent::Command {
            command: "escape".into(),
            shift: false,
        },
    );
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
}

#[test]
fn pin_controls_only_outside_dismissal() {
    let _guard = serial();
    let (_, chat) = crate::chat::tests::offline(json!({"available":true,"channels":{}}));
    let mut runtime = runtime(chat);
    assert!(runtime.outside_dismiss());
    step(&mut runtime, UiEvent::PanelDetached(true));
    assert!(
        !runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.id == "chat.pin")
    );
    step(&mut runtime, UiEvent::PanelDetached(false));
    let pin = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id == "chat.pin")
        .expect("pin button");
    let r = match &pin.draw {
        bead::render::Primitive::Rounded { rect, .. } => *rect,
        _ => panic!("pin hit rect"),
    };
    step(
        &mut runtime,
        UiEvent::Click {
            x: r.x + r.w / 2.,
            y: r.y + r.h / 2.,
        },
    );
    assert!(runtime.outside_dismiss(), "pin waits for release like QML");
    step(&mut runtime, UiEvent::Release);
    assert!(!runtime.outside_dismiss());
    step(
        &mut runtime,
        UiEvent::Command {
            command: "enter".into(),
            shift: false,
        },
    );
    assert!(
        runtime.outside_dismiss(),
        "unpin restores outside dismissal"
    );
    step(
        &mut runtime,
        UiEvent::Command {
            command: "enter".into(),
            shift: false,
        },
    );
    assert!(!runtime.outside_dismiss());
    step(
        &mut runtime,
        UiEvent::Command {
            command: "escape".into(),
            shift: false,
        },
    );
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
    assert!(
        runtime.outside_dismiss(),
        "explicit close clears pin before reopening"
    );
}

#[test]
fn unpinned_sidebar_leaves_with_grace_reentry_and_detached_exemption() {
    let _guard = serial();
    let (_, chat) = crate::chat::tests::offline(json!({"available":true,"channels":{}}));
    let mut runtime = runtime(chat);
    // Explicit key/click opens stay open on leave; only hover opens auto-close.
    step(&mut runtime, UiEvent::PointerRegion(false));
    assert!(runtime.session.hover_close_deadline().is_none());
    step(&mut runtime, UiEvent::PointerRegion(true));
    step(&mut runtime, UiEvent::PointerRegion(false));
    assert!(runtime.session.hover_close_deadline().is_none());
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "panel.hover_opened".into(),
            bead::core::Value::Bool(true),
        )])),
    );
    let deadline = runtime.session.hover_close_deadline().expect("leave grace");
    assert!(deadline > std::time::Instant::now());
    step(&mut runtime, UiEvent::PointerRegion(true));
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "reenter cancels close"
    );
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "chat.pinned".into(),
            bead::core::Value::Bool(true),
        )])),
    );
    step(&mut runtime, UiEvent::PointerRegion(false));
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "pin prevents close"
    );
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "chat.pinned".into(),
            bead::core::Value::Bool(false),
        )])),
    );
    assert!(
        runtime.session.hover_close_deadline().is_some(),
        "unpin outside starts grace"
    );
    step(&mut runtime, UiEvent::PanelDetached(true));
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "detached panels stay open"
    );
    step(&mut runtime, UiEvent::PanelDetached(false));
    let deadline = runtime
        .session
        .hover_close_deadline()
        .expect("attached leave grace");
    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now()) + Duration::from_millis(5),
    );
    step(&mut runtime, UiEvent::Model(BTreeMap::new()));
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
}

#[test]
fn hover_open_leaving_the_lip_without_visiting_the_body_still_closes() {
    let _guard = serial();
    let (_, chat) = crate::chat::tests::offline(json!({"available":true,"channels":{}}));
    let mut runtime = runtime(chat);
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "panel.hover_opened".into(),
            bead::core::Value::Bool(true),
        )])),
    );
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "resting on lip stays open"
    );
    step(&mut runtime, UiEvent::PointerRegion(false));
    let deadline = runtime
        .session
        .hover_close_deadline()
        .expect("lip leave starts grace");
    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now()) + Duration::from_millis(5),
    );
    step(&mut runtime, UiEvent::Model(BTreeMap::new()));
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
}

#[test]
fn lip_and_body_hover_are_a_union_even_when_startup_delays_lip_events() {
    let _guard = serial();
    let (_, chat) = crate::chat::tests::offline(json!({"available":true,"channels":{}}));
    let mut runtime = runtime(chat);
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([
            ("panel.hover_opened".into(), bead::core::Value::Bool(true)),
            ("panel.anchor_hovered".into(), bead::core::Value::Bool(true)),
        ])),
    );
    step(&mut runtime, UiEvent::PointerRegion(true));
    // An earlier lip leave may be delivered after the body's Wayland enter.
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "panel.anchor_hovered".into(),
            bead::core::Value::Bool(false),
        )])),
    );
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "pointer remains in body"
    );
    step(&mut runtime, UiEvent::PointerRegion(false));
    assert!(runtime.session.hover_close_deadline().is_some());
    step(
        &mut runtime,
        UiEvent::Model(BTreeMap::from([(
            "panel.anchor_hovered".into(),
            bead::core::Value::Bool(true),
        )])),
    );
    assert!(
        runtime.session.hover_close_deadline().is_none(),
        "lip reentry cancels grace"
    );
}

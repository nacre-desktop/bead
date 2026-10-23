// SPDX-License-Identifier: GPL-3.0-only
//! B0050: Wi-Fi password prompt, hidden join and forget against a fake daemon, through the real
//! network popout template and its input events.
use super::wifi_runtime_tests::{Lab, controlled_runtime, next, project};
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::{Value, source::Source};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::{sync::Arc, time::Duration};
use tokio::{io::BufReader, sync::mpsc};

const SECRET: &str = "hunter2-Passw0rd";

fn topic(auth_error: bool, with_old: bool) -> serde_json::Value {
    let mut connections = serde_json::json!({
        "c0":{"name":"Home","kind":"wifi","active":true,"ssid":"Home","security":"wpa_psk","autoconnect":true,"last_used":1700000000},
        "c9":{"name":"Old flat","kind":"wifi","active":false,"ssid":"Old flat","security":"sae","autoconnect":true}
    });
    let mut order = serde_json::json!(["c0", "c9"]);
    if !with_old {
        connections.as_object_mut().unwrap().remove("c9");
        order = serde_json::json!(["c0"]);
    }
    let mut state = serde_json::json!({
        "health":{"available":true,"stale":false},
        "wireless_enabled":true,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wifi","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{
            "ap0":{"device":"wifi0","name":"Home","strength":90,"secured":true,"active":true,"connection":"c0"},
            "ap1":{"device":"wifi0","name":"Cafe","strength":60,"secured":false,"active":false},
            "ap3":{"device":"wifi0","name":"Lab","strength":50,"secured":true,"active":false}
        },"order":["ap0","ap1","ap3"]},
        "connections":{"items":connections,"order":order},
        "primary_connection":"c0"
    });
    if auth_error {
        state["health"]["error"] = serde_json::json!({
            "code":"unavailable","message":"Network authentication required or cancelled","retry":true
        });
    }
    state
}
fn welcome() -> Message {
    let mut all = nacre_protocol::core_methods();
    for name in ["network.connect", "network.disconnect", "network.forget"] {
        all.insert(
            name.into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 10_000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
    }
    Message::Welcome {
        proto: 1,
        min: 1,
        boot: "wifi-boot-a".into(),
        topics: std::collections::BTreeMap::from([("network".into(), 1)]),
        methods: all,
        frame_ms: None,
    }
}
type Script = tokio::task::JoinHandle<()>;
type Reader = framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>;
async fn setup(
    script: impl Send + 'static + FnOnce(Reader, tokio::net::unix::OwnedWriteHalf) -> Script,
) -> Result<(Lab, Client, Runtime, Script), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(&mut writer, &welcome()).await.unwrap();
        assert!(matches!(next(&mut reader).await, Message::Sub { .. }));
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: topic(false, true),
            },
        )
        .await
        .unwrap();
        let _ = tx.send(script(reader, writer));
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    let mut runtime = controlled_runtime(&client, &lab.0.join("recovery"))?;
    until(&mut runtime, |r| {
        rows(r).len() == 4 && r.status.is_some() && can(r, "network.can_forget")
    })
    .await;
    server.await?;
    Ok((lab, client, runtime, rx.await?))
}
fn can(runtime: &Runtime, name: &str) -> bool {
    runtime.session.stores.get(name) == Some(&Value::Bool(true))
}
fn rows(runtime: &Runtime) -> Vec<serde_json::Value> {
    list(runtime, "network.rows")
}
fn list(runtime: &Runtime, name: &str) -> Vec<serde_json::Value> {
    match runtime.session.stores.get(name) {
        Some(Value::List(items)) => items.as_ref().clone(),
        other => panic!("{name} missing: {other:?}"),
    }
}
fn text(runtime: &Runtime, name: &str) -> String {
    match runtime.session.stores.get(name) {
        Some(Value::Text(text)) => text.clone(),
        other => panic!("{name} missing: {other:?}"),
    }
}
async fn until(runtime: &mut Runtime, predicate: impl Fn(&Runtime) -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(runtime);
            if predicate(runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        waited.is_ok(),
        "timed out: view {:?} error {:?} rows {:?}",
        runtime.session.stores.get("network.view"),
        runtime.session.stores.get("network.form_error"),
        rows(runtime)
    );
}
fn click(runtime: &mut Runtime, id: &str) {
    let full = format!("status-popout.{id}");
    let rect = {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|n| n.id == full)
            .unwrap_or_else(|| {
                panic!(
                    "{full} missing in {:?}",
                    runtime
                        .session
                        .view_nodes()
                        .iter()
                        .map(|n| n.id.clone())
                        .collect::<Vec<_>>()
                )
            });
        bead::ui::rect(&node.draw)
    };
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
}
/// Type into the focused input; returns the text the staged scene draws.
fn type_text(runtime: &mut Runtime, text: &str) -> Vec<String> {
    let scene = runtime
        .stage(UiEvent::Key {
            text: text.into(),
            backspace: false,
        })
        .unwrap()
        .expect("typing repaints");
    runtime.commit();
    scene
        .primitives
        .iter()
        .filter_map(|p| match p {
            bead::render::Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}
fn enter(runtime: &mut Runtime) {
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
}
fn tap(runtime: &Runtime, key: &str) {
    runtime
        .status
        .as_ref()
        .unwrap()
        .network_link_action("network.connect", key, Arc::new(|| {}));
}
fn ok(changed: bool) -> Reply {
    Reply::Ok {
        ok: serde_json::json!({"changed":changed}),
    }
}
async fn finish(
    runtime: Runtime,
    script: Script,
    client: Client,
    lab: Lab,
) -> Result<(), Box<dyn std::error::Error>> {
    drop(runtime);
    script.await?;
    Source::stop(&client).await?;
    drop(lab);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_prompt_sends_the_secret_once_clears_it_and_reprompts_on_wrong_password()
-> Result<(), Box<dyn std::error::Error>> {
    let (calls_tx, mut calls_rx) = mpsc::channel(4);
    let (ret_tx, ret_rx) = tokio::sync::oneshot::channel::<()>();
    let (snap_tx, snap_rx) = tokio::sync::oneshot::channel::<()>();
    let (lab, client, mut runtime, script) = setup(move |mut reader, mut writer| {
        tokio::spawn(async move {
            let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                panic!("join call expected first")
            };
            calls_tx.send((m, p)).await.unwrap();
            ret_rx.await.unwrap();
            framing::write(
                &mut writer,
                &Message::Ret {
                    id,
                    reply: ok(true),
                },
            )
            .await
            .unwrap();
            snap_rx.await.unwrap();
            framing::write(
                &mut writer,
                &Message::Snap {
                    topic: "network".into(),
                    seq: 1,
                    data: topic(true, true),
                },
            )
            .await
            .unwrap();
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    // A secured network without a saved profile asks first; nothing is sent.
    assert!(
        rows(&runtime)
            .iter()
            .any(|r| r["id"] == "ap:ap3" && r["secured"] == true)
    );
    tap(&runtime, "ap:ap3");
    project(&mut runtime);
    assert_eq!(text(&runtime, "network.view"), "prompt");
    assert_eq!(text(&runtime, "network.prompt_label"), "Lab");
    assert!(calls_rx.try_recv().is_err());
    // Type the password: it shows only as bullets and is invisible to a11y, IME and copy.
    click(&mut runtime, "secret.field");
    let drawn = type_text(&mut runtime, SECRET);
    assert_eq!(text(&runtime, "network.secret"), SECRET);
    assert!(
        drawn.iter().any(|t| t == &"•".repeat(SECRET.len())),
        "{drawn:?}"
    );
    assert!(drawn.iter().all(|t| !t.contains("Passw0rd")));
    assert!(!format!("{:?}", runtime.session.accessibility_tree()).contains("Passw0rd"));
    assert_eq!(
        runtime.session.surrounding_text(),
        Some((String::new(), 0, 0))
    );
    assert!(runtime.session.focused_masked());
    // Enter connects, sends the secret exactly once and empties the buffer.
    enter(&mut runtime);
    let (method, params) = calls_rx.recv().await.unwrap();
    assert_eq!(method, "network.connect");
    assert_eq!(
        params,
        serde_json::json!({"device":"wifi0","target":{"kind":"access_point","id":"ap3"},"secret":SECRET})
    );
    assert_eq!(text(&runtime, "network.secret"), "");
    ret_tx.send(()).unwrap();
    until(&mut runtime, |r| {
        text(r, "network.view") == "list"
            && rows(r)
                .iter()
                .any(|row| row["id"] == "ap:ap3" && row["action"] == "Connecting…")
    })
    .await;
    // The daemon refuses the password after admission: the prompt returns with the error.
    snap_tx.send(()).unwrap();
    until(&mut runtime, |r| text(r, "network.view") == "prompt").await;
    assert_eq!(
        text(&runtime, "network.form_error"),
        "Wrong password. Try again."
    );
    assert_eq!(text(&runtime, "network.prompt_label"), "Lab");
    assert_eq!(text(&runtime, "network.secret"), "");
    // Cancel leaves without another call.
    click(&mut runtime, "actions.cancel");
    assert_eq!(text(&runtime, "network.view"), "list");
    assert_eq!(text(&runtime, "network.form_error"), "");
    finish(runtime, script, client, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn needs_secret_refusal_opens_the_prompt() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, script) = setup(|mut reader, mut writer| {
        tokio::spawn(async move {
            let Message::Call { id, p, .. } = next(&mut reader).await else {
                panic!("connect call expected")
            };
            assert!(p.get("secret").is_none());
            framing::write(
                &mut writer,
                &Message::Ret {
                    id,
                    reply: Reply::Err {
                        err: Failure::new(ErrorCode::Validation, "Network secret required", true),
                    },
                },
            )
            .await
            .unwrap();
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    // An open-looking row the daemon says needs a password.
    tap(&runtime, "ap:ap1");
    until(&mut runtime, |r| text(r, "network.view") == "prompt").await;
    assert_eq!(text(&runtime, "network.prompt_label"), "Cafe");
    assert_eq!(text(&runtime, "network.link_message"), "");
    finish(runtime, script, client, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn hidden_network_form_validates_then_joins_with_its_security()
-> Result<(), Box<dyn std::error::Error>> {
    let (calls_tx, mut calls_rx) = mpsc::channel(4);
    let (lab, client, mut runtime, script) = setup(move |mut reader, mut writer| {
        tokio::spawn(async move {
            let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                panic!("join call expected first")
            };
            calls_tx.send((m, p)).await.unwrap();
            framing::write(
                &mut writer,
                &Message::Ret {
                    id,
                    reply: ok(true),
                },
            )
            .await
            .unwrap();
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    click_footer(&mut runtime, "more.hidden");
    assert_eq!(text(&runtime, "network.view"), "hidden");
    // Nothing is sent for an empty name or a missing password.
    click(&mut runtime, "actions.join");
    assert_eq!(
        text(&runtime, "network.form_error"),
        "Enter a network name (up to 32 bytes)"
    );
    click(&mut runtime, "ssid");
    type_text(&mut runtime, "Hidden Lab");
    click(&mut runtime, "actions.join");
    assert_eq!(text(&runtime, "network.form_error"), "Enter the password");
    assert!(calls_rx.try_recv().is_err());
    click(&mut runtime, "security.sae");
    click(&mut runtime, "secret.field");
    type_text(&mut runtime, SECRET);
    click(&mut runtime, "actions.join");
    let (method, params) = calls_rx.recv().await.unwrap();
    assert_eq!(method, "network.connect");
    assert_eq!(
        params,
        serde_json::json!({"device":"wifi0","target":{"kind":"hidden_ssid","id":"Hidden Lab"},
            "secret":SECRET,"security":"sae"})
    );
    assert_eq!(text(&runtime, "network.secret"), "");
    until(&mut runtime, |r| text(r, "network.view") == "list").await;
    finish(runtime, script, client, lab).await
}
fn click_footer(runtime: &mut Runtime, id: &str) {
    click(runtime, id);
}

#[tokio::test(flavor = "current_thread")]
async fn saved_networks_forget_needs_an_inline_confirm_that_expires()
-> Result<(), Box<dyn std::error::Error>> {
    let (calls_tx, mut calls_rx) = mpsc::channel(4);
    let (snap_tx, snap_rx) = tokio::sync::oneshot::channel::<()>();
    let (lab, client, mut runtime, script) = setup(move |mut reader, mut writer| {
        tokio::spawn(async move {
            let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                panic!("forget call expected")
            };
            calls_tx.send((m, p)).await.unwrap();
            framing::write(
                &mut writer,
                &Message::Ret {
                    id,
                    reply: ok(true),
                },
            )
            .await
            .unwrap();
            snap_rx.await.unwrap();
            framing::write(
                &mut writer,
                &Message::Snap {
                    topic: "network".into(),
                    seq: 1,
                    data: topic(false, false),
                },
            )
            .await
            .unwrap();
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    runtime
        .status
        .as_ref()
        .unwrap()
        .set_confirm_timeout_for_test(Duration::from_millis(200));
    click(&mut runtime, "more.saved");
    assert_eq!(text(&runtime, "network.view"), "saved");
    let ids = |r: &Runtime| -> Vec<String> {
        list(r, "network.saved")
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect()
    };
    // Most recently used first.
    assert_eq!(ids(&runtime), ["forget:c0", "forget:c9"]);
    let wake: crate::nacre_source::CallWake = Arc::new(|| {});
    let forget = |r: &Runtime, confirm: bool| {
        r.status
            .as_ref()
            .unwrap()
            .network_forget("forget:c9", confirm, wake.clone())
    };
    forget(&runtime, true); // not armed: inert
    forget(&runtime, false);
    project(&mut runtime);
    assert_eq!(list(&runtime, "network.saved")[1]["confirming"], true);
    assert!(calls_rx.try_recv().is_err(), "asking sends nothing");
    // Timeout reverts, and a late confirm is inert.
    tokio::time::sleep(Duration::from_millis(300)).await;
    project(&mut runtime);
    assert_eq!(list(&runtime, "network.saved")[1]["confirming"], false);
    forget(&runtime, true);
    project(&mut runtime);
    assert!(calls_rx.try_recv().is_err());
    // Ask then confirm sends exactly one forget.
    forget(&runtime, false);
    forget(&runtime, true);
    let (method, params) = calls_rx.recv().await.unwrap();
    assert_eq!(method, "network.forget");
    assert_eq!(params, serde_json::json!({"connection":"c9"}));
    until(&mut runtime, |r| {
        !list(r, "network.saved")
            .iter()
            .all(|row| row["enabled"] == true)
    })
    .await;
    snap_tx.send(()).unwrap();
    until(&mut runtime, |r| ids(r) == ["forget:c0"]).await;
    finish(runtime, script, client, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn secret_stays_out_of_traces_summaries_and_saved_state()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, script) = setup(|mut reader, writer| {
        tokio::spawn(async move {
            let _writer = writer;
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    let path = lab.0.join("state/panels.json");
    let (file, _) = crate::state::File::open(path.clone())?;
    runtime.state_file = Some(file);
    runtime.trace = true;
    tap(&runtime, "ap:ap3");
    project(&mut runtime);
    click(&mut runtime, "secret.field");
    type_text(&mut runtime, SECRET);
    assert_eq!(text(&runtime, "network.secret"), SECRET);
    for _ in 0..3 {
        project(&mut runtime);
    }
    // The `UI_STATE` trace dump, accessibility tree, drawn text, trace markers and the saved
    // panel state never carry the password; the dump still shows other stores.
    let summary = runtime.session.state_summary();
    assert!(
        summary.contains("network.view") && !summary.contains(SECRET),
        "{summary}"
    );
    assert!(!format!("{:?}", runtime.session.accessibility_tree()).contains(SECRET));
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(!saved.contains(SECRET) && !saved.contains("network.secret"));
    // Esc leaves the prompt and clears the buffer.
    runtime
        .stage(UiEvent::Command {
            command: "escape".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(text(&runtime, "network.secret"), "");
    assert_eq!(text(&runtime, "network.view"), "list");
    finish(runtime, script, client, lab).await
}

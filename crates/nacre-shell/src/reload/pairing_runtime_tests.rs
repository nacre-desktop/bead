// SPDX-License-Identifier: GPL-3.0-only
//! B0055: Bluetooth device list and pairing against a fake daemon, through the real Bluetooth
//! popout template and its input events (one test per prompt kind of N0045).
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::{Value, source::Source};
use nacre_protocol::{Capability, ErrorCode, Failure, Id, Message, Reply, framing};
use std::{
    collections::BTreeMap, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration,
};
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

const PIN: &str = "Zx9-secret-PIN";

struct Lab(PathBuf);
impl Lab {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bluetooth-pairing-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn devices() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "adapters":{"items":{"hci0":{"name":"Adapter","powered":true,"discovering":false}},"order":["hci0"]},
        "devices":{"items":{
            "dev_C":{"adapter":"hci0","name":"Fixture phone","connected":false,"paired":true,"trusted":true,"pairing":false,"icon":"phone"},
            "dev_B":{"adapter":"hci0","name":"Fixture keyboard","connected":false,"paired":false,"trusted":false,"pairing":false,"icon":"input-keyboard"},
            "dev_A":{"adapter":"hci0","name":"Fixture headphones","connected":true,"paired":true,"trusted":true,"pairing":false,"battery":80,"icon":"audio-headset"}
        },"order":["dev_C","dev_B","dev_A"]}
    })
}
fn pairing(prompt: Option<(&str, Option<&str>)>) -> serde_json::Value {
    let mut state = serde_json::json!({"health":{"available":true,"stale":false}});
    if let Some((kind, code)) = prompt {
        let mut p = serde_json::json!({"device":"dev_B","kind":kind,"name":"Fixture keyboard","timeout_ms":60000});
        if let Some(code) = code {
            p["code"] = code.into();
        }
        state["prompt"] = p;
    }
    state
}
fn welcome(with_pair: bool) -> Message {
    let mut methods = nacre_protocol::core_methods();
    let all: &[&str] = if with_pair {
        &[
            "bluetooth.pair",
            "bluetooth.pair.respond",
            "bluetooth.pair.cancel",
        ]
    } else {
        &[]
    };
    for name in all {
        methods.insert(
            (*name).into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 120_000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
    }
    Message::Welcome {
        proto: 1,
        min: 1,
        boot: "bt-boot".into(),
        topics: BTreeMap::from([("bluetooth".into(), 1), ("bluetooth.pairing".into(), 1)]),
        methods,
        frame_ms: None,
    }
}

/// The fake daemon: every inbound message lands in `inbox`; what the test puts in `out` is sent.
struct Daemon {
    inbox: mpsc::UnboundedReceiver<Message>,
    out: mpsc::UnboundedSender<Message>,
    seq: u64,
    server: tokio::task::JoinHandle<()>,
}
impl Daemon {
    async fn next(&mut self) -> Message {
        tokio::time::timeout(Duration::from_secs(5), self.inbox.recv())
            .await
            .expect("daemon message in time")
            .expect("daemon inbox open")
    }
    /// The next call; lease releases in between are not the test's business.
    async fn call(&mut self) -> (Id, String, serde_json::Value) {
        loop {
            match self.next().await {
                Message::Call { id, m, p, .. } => return (id, m, p),
                Message::Unsub { .. } => {}
                other => panic!("call expected, got {other:?}"),
            }
        }
    }
    /// A call and the Sub for `topic`, in whichever order they arrive.
    async fn call_and_sub(&mut self, topic: &str) -> (Id, String, serde_json::Value) {
        let (mut call, mut subbed) = (None, false);
        while call.is_none() || !subbed {
            match self.next().await {
                Message::Call { id, m, p, .. } => call = Some((id, m, p)),
                Message::Sub { topics } if topics == [topic] => subbed = true,
                Message::Unsub { .. } => {}
                other => panic!("call or sub expected, got {other:?}"),
            }
        }
        call.unwrap()
    }
    async fn sub(&mut self, topic: &str) {
        assert_eq!(
            self.next().await,
            Message::Sub {
                topics: vec![topic.into()]
            }
        );
    }
    fn snap(&mut self, topic: &str, data: serde_json::Value) {
        self.seq += 1;
        let _ = self.out.send(Message::Snap {
            topic: topic.into(),
            seq: self.seq,
            data,
        });
    }
    fn ret(&self, id: Id, reply: Reply) {
        let _ = self.out.send(Message::Ret { id, reply });
    }
    fn ok(&self, id: Id) {
        self.ret(
            id,
            Reply::Ok {
                ok: serde_json::json!({"changed":true}),
            },
        );
    }
    fn err(&self, id: Id, code: ErrorCode, message: &str, retry: bool) {
        self.ret(
            id,
            Reply::Err {
                err: Failure::new(code, message, retry),
            },
        );
    }
    /// The whole daemon-side traffic left after the runtime is gone, without waiting for more.
    async fn drain(&mut self) -> Vec<Message> {
        let mut seen = Vec::new();
        while let Ok(Some(message)) =
            tokio::time::timeout(Duration::from_millis(300), self.inbox.recv()).await
        {
            seen.push(message);
        }
        seen
    }
}

async fn setup(
    with_pair: bool,
) -> Result<(Lab, Client, Runtime, Daemon), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let socket = lab.0.join("daemon.sock");
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let (inbox_tx, inbox) = mpsc::unbounded_channel();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        framing::write(&mut writer, &welcome(with_pair))
            .await
            .unwrap();
        loop {
            tokio::select! {
                message = reader.next() => match message {
                    Ok(Some(Message::Ping { id })) => {
                        framing::write(&mut writer, &Message::Pong { id }).await.unwrap();
                    }
                    Ok(Some(Message::Pong { .. })) => {}
                    Ok(Some(message)) => { let _ = inbox_tx.send(message); }
                    _ => break,
                },
                Some(message) = out_rx.recv() => {
                    if framing::write(&mut writer, &message).await.is_err() { break; }
                }
            }
        }
    });
    let client = Client::start(Config::new(socket, vec![]))?;
    let mut daemon = Daemon {
        inbox,
        out,
        seq: 0,
        server,
    };
    let mut runtime = controlled_runtime(&client, lab.0.join("recovery"))?;
    daemon.sub("bluetooth").await;
    daemon.snap("bluetooth", devices());
    until(&mut runtime, |r| list(r, "bluetooth.devices").len() == 3).await;
    Ok((lab, client, runtime, daemon))
}
fn controlled_runtime(
    client: &Client,
    recovery: PathBuf,
) -> Result<Runtime, Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session =
        bead::ui::Session::new(root, "status/bluetooth.luau".into(), palette, colors, cold);
    let mut runtime = Runtime::new(session)
        .with_core_bluetooth_control(Arc::new(client.lease_topic("bluetooth")?), client.clone())?;
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.session.enable_recovery(recovery);
    runtime.services(Box::new(|_| {}));
    runtime
        .stage(UiEvent::Reload)?
        .expect("controlled template loads");
    runtime.commit();
    Ok(runtime)
}

fn project(runtime: &mut Runtime) {
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
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
        "timed out: view {:?} kind {:?}",
        runtime.session.stores.get("bluetooth.view"),
        runtime.session.stores.get("bluetooth.pair_kind")
    );
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
fn click_node(runtime: &mut Runtime, pick: impl Fn(&bead::ui::Node) -> bool) {
    let rect = {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|n| pick(n))
            .expect("node to click");
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
fn click(runtime: &mut Runtime, id: &str) {
    let full = format!("status-popout.{id}");
    click_node(runtime, |n| n.id == full);
}
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
fn press_pair(runtime: &mut Runtime) {
    click_node(runtime, |n| n.action.as_deref() == Some("bluetooth.pair"));
}
async fn finish(
    runtime: Runtime,
    client: Client,
    daemon: Daemon,
    lab: Lab,
) -> Result<(), Box<dyn std::error::Error>> {
    drop(runtime);
    Source::stop(&client).await?;
    drop(daemon.out);
    let _ = daemon.server.await;
    drop(lab);
    Ok(())
}
/// Start pairing `dev_B`; returns the id of the waiting `bluetooth.pair` call.
async fn begin(runtime: &mut Runtime, daemon: &mut Daemon) -> Id {
    press_pair(runtime);
    // The lease's Sub and the waiting call leave on different paths: either may come first.
    let (id, method, params) = daemon.call_and_sub("bluetooth.pairing").await;
    assert_eq!(method, "bluetooth.pair");
    assert_eq!(params, serde_json::json!({"device":"dev_B"}));
    daemon.snap("bluetooth.pairing", pairing(None));
    until(runtime, |r| text(r, "bluetooth.view") == "pair").await;
    assert_eq!(text(runtime, "bluetooth.pair_kind"), "waiting");
    assert_eq!(
        text(runtime, "bluetooth.pair_title"),
        "Pair with Fixture keyboard"
    );
    assert_eq!(
        text(runtime, "bluetooth.pair_text"),
        "Confirm pairing on your device."
    );
    id
}
async fn prompt(runtime: &mut Runtime, daemon: &mut Daemon, kind: &str, code: Option<&str>) {
    daemon.snap("bluetooth.pairing", pairing(Some((kind, code))));
    until(runtime, |r| text(r, "bluetooth.pair_kind") == kind).await;
    let left = text(runtime, "bluetooth.pair_countdown");
    let seconds: u32 = left
        .strip_prefix("Expires in ")
        .and_then(|rest| rest.strip_suffix(" s"))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("countdown text {left:?}"));
    assert!((50..=60).contains(&seconds), "{left}");
}
/// The daemon withdraws the prompt and the waiting call returns: back on the list, paired.
async fn succeed(runtime: &mut Runtime, daemon: &mut Daemon, pair_id: Id) {
    daemon.snap("bluetooth.pairing", pairing(None));
    daemon.ok(pair_id);
    until(runtime, |r| text(r, "bluetooth.view") == "list").await;
    assert_eq!(text(runtime, "bluetooth.list_message"), "Device paired.");
    // The fake daemon does not change the device list, so the row offers Pair again.
    assert_eq!(list(runtime, "bluetooth.devices")[1]["enabled"], true);
}

#[tokio::test(flavor = "current_thread")]
async fn the_device_list_orders_rows_and_only_unpaired_devices_offer_pair()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, runtime, daemon) = setup(true).await?;
    let rows = list(&runtime, "bluetooth.devices");
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["dev:dev_A", "dev:dev_B", "dev:dev_C"]);
    assert_eq!(rows[0]["caption"], "80%");
    assert_eq!(rows[0]["icon"], "headphones");
    assert_eq!(rows[0]["action"], "");
    assert_eq!(rows[1]["caption"], "Ready to pair");
    assert_eq!(rows[1]["icon"], "keyboard");
    assert_eq!(rows[1]["action"], "Pair");
    assert_eq!(rows[1]["enabled"], true);
    assert_eq!(rows[2]["caption"], "Paired");
    assert_eq!(rows[2]["icon"], "smartphone");
    assert_eq!(rows[2]["enabled"], false);
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn without_the_pair_method_rows_are_disabled_and_nothing_is_sent()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(false).await?;
    assert_eq!(list(&runtime, "bluetooth.devices")[1]["enabled"], false);
    runtime
        .status
        .as_ref()
        .unwrap()
        .bluetooth_pair("dev:dev_B", Arc::new(|| {}));
    project(&mut runtime);
    assert_eq!(text(&runtime, "bluetooth.view"), "list");
    assert_eq!(
        text(&runtime, "bluetooth.list_message"),
        "Bluetooth pairing is not available."
    );
    drop(runtime);
    Source::stop(&client).await?;
    assert!(
        daemon
            .drain()
            .await
            .iter()
            .all(|m| !matches!(m, Message::Call { .. }))
    );
    drop(lab);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn confirm_shows_the_code_and_accept_answers_once() -> Result<(), Box<dyn std::error::Error>>
{
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "confirm", Some("123456")).await;
    assert_eq!(
        text(&runtime, "bluetooth.pair_text"),
        "Does 123456 match the code on your device?"
    );
    click(&mut runtime, "actions.accept");
    let (id, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.respond");
    assert_eq!(params, serde_json::json!({"device":"dev_B","accept":true}));
    daemon.ok(id);
    succeed(&mut runtime, &mut daemon, pair_id).await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn reject_answers_no_and_leaves_without_an_error() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "authorize", None).await;
    assert_eq!(
        text(&runtime, "bluetooth.pair_text"),
        "Allow this device to connect?"
    );
    click(&mut runtime, "actions.reject");
    let (id, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.respond");
    assert_eq!(params, serde_json::json!({"device":"dev_B","accept":false}));
    daemon.ok(id);
    // The list is back at once; the rejected pairing's failure is not shown.
    until(&mut runtime, |r| text(r, "bluetooth.view") == "list").await;
    assert_eq!(list(&runtime, "bluetooth.devices")[1]["enabled"], false);
    daemon.err(
        pair_id,
        ErrorCode::PermissionDenied,
        "Pairing was rejected",
        false,
    );
    until(&mut runtime, |r| {
        list(r, "bluetooth.devices")[1]["enabled"] == true
    })
    .await;
    assert_eq!(text(&runtime, "bluetooth.list_message"), "");
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn pin_entry_takes_focus_sends_the_pin_once_and_clears_it()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "pin", None).await;
    assert_eq!(
        text(&runtime, "bluetooth.pair_text"),
        "Enter the pairing code from your device."
    );
    // The field already has focus: typing shows only bullets.
    let drawn = type_text(&mut runtime, PIN);
    assert_eq!(text(&runtime, "bluetooth.secret"), PIN);
    assert!(
        drawn.iter().any(|t| t == &"•".repeat(PIN.len())),
        "{drawn:?}"
    );
    assert!(drawn.iter().all(|t| !t.contains("secret")));
    assert!(runtime.session.focused_masked());
    enter(&mut runtime);
    let (id, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.respond");
    assert_eq!(
        params,
        serde_json::json!({"device":"dev_B","accept":true,"pin":PIN})
    );
    assert_eq!(text(&runtime, "bluetooth.secret"), "");
    daemon.ok(id);
    succeed(&mut runtime, &mut daemon, pair_id).await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn a_bad_entry_sends_nothing_and_keeps_the_text() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "pin", None).await;
    enter(&mut runtime);
    assert_eq!(
        text(&runtime, "bluetooth.pair_error"),
        "Enter 1 to 16 characters."
    );
    type_text(&mut runtime, "this pin is way too long to send");
    click(&mut runtime, "actions.submit");
    assert_eq!(
        text(&runtime, "bluetooth.pair_error"),
        "Enter 1 to 16 characters."
    );
    assert_eq!(
        text(&runtime, "bluetooth.secret"),
        "this pin is way too long to send"
    );
    // Leaving clears the buffer and cancels the pairing.
    click(&mut runtime, "actions.cancel");
    assert_eq!(text(&runtime, "bluetooth.secret"), "");
    let (id, method, _) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.cancel");
    daemon.ok(id);
    daemon.err(
        pair_id,
        ErrorCode::PermissionDenied,
        "Authentication was cancelled",
        false,
    );
    until(&mut runtime, |r| {
        list(r, "bluetooth.devices")[1]["enabled"] == true
    })
    .await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn passkey_entry_sends_a_number() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "passkey", None).await;
    type_text(&mut runtime, "12ab");
    enter(&mut runtime);
    assert_eq!(
        text(&runtime, "bluetooth.pair_error"),
        "Enter the six-digit passkey."
    );
    assert_eq!(text(&runtime, "bluetooth.secret"), "12ab");
    for _ in 0..4 {
        runtime
            .stage(UiEvent::Key {
                text: String::new(),
                backspace: true,
            })
            .unwrap();
        runtime.commit();
    }
    type_text(&mut runtime, "004217");
    enter(&mut runtime);
    let (id, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.respond");
    assert_eq!(
        params,
        serde_json::json!({"device":"dev_B","accept":true,"passkey":4217})
    );
    assert_eq!(text(&runtime, "bluetooth.secret"), "");
    daemon.ok(id);
    succeed(&mut runtime, &mut daemon, pair_id).await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn display_shows_the_code_and_only_offers_cancel() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "display", Some("654321")).await;
    assert_eq!(
        text(&runtime, "bluetooth.pair_text"),
        "Type 654321 on your device, then press Enter."
    );
    let ids: Vec<_> = runtime
        .session
        .view_nodes()
        .iter()
        .filter(|n| n.id.starts_with("status-popout.actions."))
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(ids, ["status-popout.actions.cancel"]);
    // The device finishes pairing by itself once the code was typed there.
    succeed(&mut runtime, &mut daemon, pair_id).await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn a_wrong_pin_shows_the_error_and_retry_pairs_again()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "pin", None).await;
    type_text(&mut runtime, "0000");
    enter(&mut runtime);
    let (id, _, _) = daemon.call().await;
    daemon.ok(id);
    daemon.snap("bluetooth.pairing", pairing(None));
    daemon.err(
        pair_id,
        ErrorCode::PermissionDenied,
        "Pairing authentication failed",
        true,
    );
    until(&mut runtime, |r| text(r, "bluetooth.pair_kind") == "error").await;
    assert_eq!(text(&runtime, "bluetooth.view"), "pair");
    assert_eq!(
        text(&runtime, "bluetooth.pair_text"),
        "Wrong code. Try again."
    );
    assert_eq!(
        runtime.session.stores.get("bluetooth.pair_retry"),
        Some(&Value::Bool(true))
    );
    assert_eq!(text(&runtime, "bluetooth.secret"), "");
    // The lease ended with the failure; Retry watches it again and sends a fresh call.
    assert_eq!(
        daemon.next().await,
        Message::Unsub {
            topics: vec!["bluetooth.pairing".into()]
        }
    );
    click(&mut runtime, "actions.retry");
    let (retry_id, method, params) = daemon.call_and_sub("bluetooth.pairing").await;
    assert_eq!(method, "bluetooth.pair");
    assert_eq!(params, serde_json::json!({"device":"dev_B"}));
    daemon.snap("bluetooth.pairing", pairing(None));
    until(&mut runtime, |r| {
        text(r, "bluetooth.pair_kind") == "waiting"
    })
    .await;
    succeed(&mut runtime, &mut daemon, retry_id).await;
    finish(runtime, client, daemon, lab).await
}

#[tokio::test(flavor = "current_thread")]
async fn failures_map_to_their_messages_and_only_some_offer_retry()
-> Result<(), Box<dyn std::error::Error>> {
    for (code, message, retry, shown, offers_retry) in [
        (
            ErrorCode::Timeout,
            "Pairing timed out",
            true,
            "Pairing timed out. Make the device discoverable and try again.",
            true,
        ),
        (
            ErrorCode::Busy,
            "Another pairing is running",
            true,
            "Another pairing is in progress.",
            true,
        ),
        (
            ErrorCode::PermissionDenied,
            "Pairing was rejected",
            false,
            "Pairing was rejected.",
            false,
        ),
        (
            ErrorCode::StaleRevision,
            "Device is gone",
            false,
            "That device is no longer available. Try again.",
            false,
        ),
        (
            ErrorCode::Unavailable,
            "Bluetooth is unavailable",
            false,
            "Bluetooth pairing is not available.",
            false,
        ),
    ] {
        let (lab, client, mut runtime, mut daemon) = setup(true).await?;
        let pair_id = begin(&mut runtime, &mut daemon).await;
        daemon.err(pair_id, code, message, retry);
        until(&mut runtime, |r| text(r, "bluetooth.pair_kind") == "error").await;
        assert_eq!(text(&runtime, "bluetooth.pair_text"), shown);
        assert_eq!(
            runtime.session.stores.get("bluetooth.pair_retry"),
            Some(&Value::Bool(offers_retry))
        );
        // Cancel closes the error page without another call.
        click(&mut runtime, "actions.cancel");
        project(&mut runtime);
        assert_eq!(text(&runtime, "bluetooth.view"), "list");
        finish(runtime, client, daemon, lab).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn closing_the_panel_mid_pairing_cancels_it() -> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let _pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "confirm", Some("111111")).await;
    drop(runtime);
    let (_, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.cancel");
    assert_eq!(params, serde_json::json!({"device":"dev_B"}));
    Source::stop(&client).await?;
    drop(daemon.out);
    let _ = daemon.server.await;
    drop(lab);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn the_pin_stays_out_of_traces_summaries_and_saved_state()
-> Result<(), Box<dyn std::error::Error>> {
    let (lab, client, mut runtime, mut daemon) = setup(true).await?;
    let path = lab.0.join("state/panels.json");
    let (file, _) = crate::state::File::open(path.clone())?;
    runtime.state_file = Some(file);
    runtime.trace = true;
    let _pair_id = begin(&mut runtime, &mut daemon).await;
    prompt(&mut runtime, &mut daemon, "pin", None).await;
    type_text(&mut runtime, PIN);
    for _ in 0..3 {
        project(&mut runtime);
    }
    assert_eq!(text(&runtime, "bluetooth.secret"), PIN);
    let summary = runtime.session.state_summary();
    assert!(
        summary.contains("bluetooth.view") && !summary.contains(PIN),
        "{summary}"
    );
    assert!(!format!("{:?}", runtime.session.accessibility_tree()).contains(PIN));
    assert_eq!(
        runtime.session.surrounding_text(),
        Some((String::new(), 0, 0))
    );
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(!saved.contains(PIN) && !saved.contains("bluetooth.secret"));
    // Esc leaves the pairing page, clears the buffer and cancels the pairing.
    runtime
        .stage(UiEvent::Command {
            command: "escape".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(text(&runtime, "bluetooth.secret"), "");
    assert_eq!(text(&runtime, "bluetooth.view"), "list");
    let (_, method, params) = daemon.call().await;
    assert_eq!(method, "bluetooth.pair.cancel");
    assert!(!params.to_string().contains(PIN));
    finish(runtime, client, daemon, lab).await
}

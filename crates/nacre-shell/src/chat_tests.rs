// SPDX-License-Identifier: GPL-3.0-only
//! B0043: the sidebar model against fixtures and a fake daemon, plus goldens.
// One test at a time owns the process-wide `send_failed` queue, across awaits.
#![allow(clippy::await_holding_lock)]
use super::*;
use crate::nacre_source::Config;
use bead_test::{Harness, source::FakeSource};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::path::Path;
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

/// `route_event` feeds one process-wide queue, so tests that project run one by one.
pub(crate) static SERIAL: Mutex<()> = Mutex::new(());
pub(crate) fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}
pub(crate) fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
pub(crate) fn meter() -> Meter {
    Meter::fixed(vec![
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../bead-test/fonts/IBMPlexSans.ttf"),
    ])
}
fn state(busy: bool, status: &str, question: Option<Json>) -> Json {
    let mut s = json!({"defaultProvider":"claude","inWorkspace":false,"provider":"claude","account":"Default","title":"Fix the build","threadId":"t1","model":"","features":[],"composerKey":"k","busy":busy,"status":status,"error":""});
    if let Some(q) = question {
        s["question"] = q;
    }
    s
}
fn topic(messages: &[(&str, &str, &str)], st: Json) -> Json {
    let msgs: serde_json::Map<String, Json> = messages
        .iter()
        .enumerate()
        .map(|(i, (id, role, text))| {
            (
                format!("{:010}", i + 1),
                json!({"id": id, "role": role, "text": text}),
            )
        })
        .collect();
    json!({"available": true, "channels": {"": {"state": st, "messages": msgs}}})
}
fn approval() -> Json {
    json!({"id":"approval-1","kind":"approval","questions":[{"id":"approval","question":"Run the build command?","options":[{"label":"Allow once"},{"label":"Decline"}]}]})
}
fn stores(pairs: &[(&str, Value)]) -> Stores {
    Stores::default()
        .reconcile(
            &pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
        .unwrap()
}
fn offline(data: Json) -> (Arc<FakeSource>, Chat) {
    let source = Arc::new(FakeSource::new(data).unwrap());
    (source.clone(), Chat::new(None, source, meter()))
}
fn cells(out: &BTreeMap<String, Value>) -> Vec<Json> {
    match &out["chat.cells"] {
        Value::List(items) => items.as_ref().clone(),
        other => panic!("{other:?}"),
    }
}
fn num(out: &BTreeMap<String, Value>, key: &str) -> f64 {
    match &out[key] {
        Value::Number(n) => *n,
        other => panic!("{key}: {other:?}"),
    }
}
fn string(out: &BTreeMap<String, Value>, key: &str) -> String {
    match &out[key] {
        Value::Text(t) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn streaming_text_keeps_earlier_cells_and_follows_the_bottom() {
    let _guard = serial();
    let long = "word ".repeat(400);
    let (source, chat) = offline(topic(
        &[("u1", "user", "hello")],
        state(true, "Working", None),
    ));
    let first = chat.project(&Stores::default());
    assert!(num(&first, "chat.extent") > 0.);
    let mut previous = cells(&first);
    let mut text = String::new();
    for step in 0..4 {
        text.push_str(&long);
        source
            .publish(topic(
                &[("u1", "user", "hello"), ("a1", "assistant", &text)],
                state(true, "Working", None),
            ))
            .unwrap();
        let st = stores(&[("chat.scroll", Value::Number(num(&first, "chat.scroll")))]);
        let out = chat.project(&st);
        let now = cells(&out);
        // Everything laid out before the streaming message is unchanged between tokens.
        for cell in now
            .iter()
            .filter(|c| !c["id"].as_str().unwrap().starts_with("0000000002:"))
        {
            if let Some(old) = previous.iter().find(|c| c["id"] == cell["id"]) {
                assert_eq!(old, cell);
            }
        }
        assert!(
            now.iter()
                .any(|c| c["id"].as_str().unwrap().starts_with("0000000002:")),
            "step {step}"
        );
        previous = now;
    }
}

#[test]
fn reading_away_stops_following_until_the_bottom_is_reached_again() {
    let _guard = serial();
    let msgs: Vec<(String, String)> = (0..30)
        .map(|i| (format!("m{i}"), format!("message {i} {}", "x ".repeat(60))))
        .collect();
    let rows: Vec<(&str, &str, &str)> = msgs
        .iter()
        .map(|(a, b)| (a.as_str(), "assistant", b.as_str()))
        .collect();
    let (source, chat) = offline(topic(&rows, state(false, "", None)));
    let out = chat.project(&Stores::default());
    let bottom = num(&out, "chat.scroll");
    assert!(bottom > 500.);
    // The reader scrolls to the top: the next update keeps that position.
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(0.))]));
    assert_eq!(num(&out, "chat.scroll"), 0.);
    source
        .publish(topic(&rows, state(false, "still", None)))
        .unwrap();
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(0.))]));
    assert_eq!(num(&out, "chat.scroll"), 0.);
    // Only the cells near the viewport exist (virtualized), not all 30 messages.
    let visible = cells(&out);
    assert!(visible.len() < 30 * 4, "{}", visible.len());
    assert!(visible.iter().all(|c| c["y"].as_f64().unwrap() < 1500.));
    // Back near the bottom: following resumes; "Latest" does the same.
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(bottom - 10.))]));
    assert!(
        (num(&out, "chat.scroll")
            - (num(&out, "chat.extent") - num(&out, "chat.transcript_height")))
        .abs()
            < 0.01
    );
    chat.act("latest", &Stores::default());
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(bottom - 10.))]));
    assert!(
        (num(&out, "chat.scroll")
            - (num(&out, "chat.extent") - num(&out, "chat.transcript_height")))
        .abs()
            < 0.01
    );
}

#[test]
fn assistant_markdown_blocks_and_copy_actions() {
    let _guard = serial();
    let md = "Intro **bold**.\n\n- one\n- two\n\n```sh\ncargo build\n```\n\n```\nls\n```";
    let (_s, chat) = offline(topic(
        &[("u", "user", "**not** markdown"), ("a", "assistant", md)],
        state(false, "", None),
    ));
    let out = chat.project(&Stores::default());
    let all = cells(&out);
    let texts: Vec<String> = all
        .iter()
        .map(|c| c["t"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        texts.contains(&"**not** markdown".to_string()),
        "user text stays plain"
    );
    assert!(texts.contains(&"Intro bold.".to_string()));
    assert!(texts.contains(&"•".to_string()) && texts.contains(&"cargo build".to_string()));
    let buttons: Vec<_> = all
        .iter()
        .filter(|c| c["k"] == "button")
        .map(|c| c["a"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        buttons,
        [
            "copy:0000000001",
            "copy:0000000002",
            "code:0000000002:0",
            "code:0000000002:1"
        ]
    );
    assert_eq!(
        chat.act("copy:0000000002", &Stores::default())
            .copy
            .as_deref(),
        Some(md)
    );
    assert_eq!(
        chat.act("code:0000000002:1", &Stores::default())
            .copy
            .as_deref(),
        Some("ls")
    );
    assert_eq!(chat.act("code:0000000002:9", &Stores::default()).copy, None);
    // Code blocks are monospace text over a filled box.
    assert!(
        all.iter()
            .any(|c| c["f"] == MONO && c["t"] == "cargo build")
    );
}

#[test]
fn approval_card_needs_every_answer_and_option_fills_it() {
    let _guard = serial();
    let (_s, chat) = offline(topic(
        &[("u", "user", "run it")],
        state(true, "Waiting", Some(approval())),
    ));
    let out = chat.project(&Stores::default());
    assert_eq!(out["chat.has_question"], Value::Bool(true));
    assert_eq!(out["chat.can_reply"], Value::Bool(false));
    let Value::List(rows) = &out["chat.question_rows"] else {
        panic!()
    };
    let kinds: Vec<_> = rows.iter().map(|r| r["k"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["text", "option", "option", "input", "reply"]);
    let pick = chat.act("option:0:1", &Stores::default());
    assert_eq!(pick.stores["chat.answer.0"], Value::Text("Decline".into()));
    let out = chat.project(&stores(&[("chat.answer.0", Value::Text("Decline".into()))]));
    assert_eq!(
        out["chat.can_reply"],
        Value::Bool(false),
        "no client: calls unavailable"
    );
    // Out-of-range choices change nothing.
    assert!(chat.act("option:0:7", &Stores::default()).stores.is_empty());
    assert!(chat.act("option:5:0", &Stores::default()).stores.is_empty());
}

#[test]
fn composer_grows_with_its_text_and_the_footer_shifts_the_transcript() {
    let _guard = serial();
    let (_s, chat) = offline(topic(&[], state(false, "", None)));
    let one = chat.project(&stores(&[("chat.draft", Value::Text("hi".into()))]));
    let many = chat.project(&stores(&[("chat.draft", Value::Text("line\n".repeat(7)))]));
    assert!(num(&many, "chat.composer_height") > num(&one, "chat.composer_height"));
    assert!(num(&many, "chat.transcript_height") < num(&one, "chat.transcript_height"));
    assert_eq!(one["chat.empty"], Value::Bool(true));
    assert_eq!(
        one["chat.can_send"],
        Value::Bool(false),
        "no daemon methods"
    );
}

#[test]
fn error_status_and_hidden_panel() {
    let _guard = serial();
    let mut st = state(false, "", None);
    st["error"] = json!("Provider failed");
    let (_s, chat) = offline(topic(&[("u", "user", "x")], st));
    let out = chat.project(&Stores::default());
    assert_eq!(string(&out, "chat.status"), "Provider failed");
    assert_eq!(out["chat.status_error"], Value::Bool(true));
    // A hidden panel projects nothing and holds no layout.
    assert!(
        chat.project(&stores(&[("panel.open", Value::Bool(false))]))
            .is_empty()
    );
}

// ---- fake daemon -----------------------------------------------------------

pub(crate) enum Frame {
    Msg(Message),
    Drop,
}
pub(crate) struct Lab {
    dir: std::path::PathBuf,
    pub client: Client,
    pub calls: mpsc::UnboundedReceiver<(String, Json)>,
    pub push: mpsc::UnboundedSender<Frame>,
    pub fail: Arc<Mutex<Vec<String>>>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn welcome(boot: u32, saved: bool) -> Message {
    let mut methods = nacre_protocol::core_methods();
    methods.extend(nacre_protocol::ai_chat::methods());
    if saved {
        methods.insert(
            SAVED.into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 3000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
    }
    Message::Welcome {
        proto: 1,
        min: 1,
        boot: format!("chat-boot-{boot}"),
        topics: BTreeMap::from([("ai.chat".into(), 1)]),
        methods,
        frame_ms: None,
    }
}
pub(crate) async fn lab(saved: bool, initial: Json) -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "chat-lab-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let (push, mut frames) = mpsc::unbounded_channel::<Frame>();
    let fail: Arc<Mutex<Vec<String>>> = Default::default();
    let failing = fail.clone();
    tokio::spawn(async move {
        let mut boot = 0;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            boot += 1;
            let (read, mut writer) = stream.into_split();
            let (inbox, mut incoming) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut reader = framing::Reader::new(BufReader::new(read));
                while let Ok(Some(message)) = reader.next().await {
                    if inbox.send(message).is_err() {
                        break;
                    }
                }
            });
            let Some(Message::Hello { .. }) = incoming.recv().await else {
                continue;
            };
            framing::write(&mut writer, &welcome(boot, saved))
                .await
                .unwrap();
            let mut seq = 0;
            loop {
                tokio::select! {
                    message = incoming.recv() => match message {
                        Some(Message::Sub { .. }) => {
                            framing::write(&mut writer, &Message::Snap { topic: "ai.chat".into(), seq, data: initial.clone() }).await.unwrap();
                        }
                        Some(Message::Call { id, m, p, .. }) => {
                            let _ = calls_tx.send((m.clone(), p));
                            let reply = if failing.lock().unwrap().contains(&m) {
                                Reply::Err { err: Failure::new(ErrorCode::Unavailable, "no worker", false) }
                            } else if m == SAVED {
                                Reply::Ok { ok: json!({"chats":[{"key":"abc123","id":"t9","title":"Old chat","agent":"codex","account":"Default","state":"saved"}]}) }
                            } else {
                                Reply::Ok { ok: json!({"channel":""}) }
                            };
                            framing::write(&mut writer, &Message::Ret { id, reply }).await.unwrap();
                        }
                        Some(_) => {}
                        None => break,
                    },
                    frame = frames.recv() => match frame {
                        Some(Frame::Msg(Message::Snap { topic, data, .. })) => {
                            seq += 1;
                            framing::write(&mut writer, &Message::Snap { topic, seq, data }).await.unwrap();
                        }
                        Some(Frame::Msg(message)) => framing::write(&mut writer, &message).await.unwrap(),
                        Some(Frame::Drop) => break,
                        None => return,
                    },
                }
            }
        }
    });
    let mut config = Config::new(socket, vec![]);
    config.event = Some(Arc::new(route_event));
    let client = Client::start(config).unwrap();
    Lab {
        dir,
        client,
        calls,
        push,
        fail,
    }
}
fn snap(data: Json) -> Frame {
    Frame::Msg(Message::Snap {
        topic: "ai.chat".into(),
        seq: 0,
        data,
    })
}
pub(crate) async fn chat_for(lab: &Lab) -> Chat {
    for _ in 0..500 {
        if lab.client.snapshot().connected && lab.client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let topic = lab.client.lease_topic("ai.chat").unwrap();
    Chat::new(Some(lab.client.clone()), Arc::new(topic), meter())
}
async fn until(
    chat: &Chat,
    st: &Stores,
    what: &str,
    done: impl Fn(&BTreeMap<String, Value>) -> bool,
) -> BTreeMap<String, Value> {
    for _ in 0..800 {
        let out = chat.project(st);
        if done(&out) {
            return out;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
fn opened(chat: &Chat) -> bool {
    chat.st.borrow().opened_at.is_some()
}

#[tokio::test(flavor = "current_thread")]
async fn open_heartbeat_send_stop_and_approve_use_the_published_calls() {
    let _guard = serial();
    let mut lab = lab(
        false,
        topic(&[("u1", "user", "hi")], state(false, "", None)),
    )
    .await;
    let chat = chat_for(&lab)
        .await
        .with_heartbeat(Duration::from_millis(80));
    let none = Stores::default();
    let out = until(&chat, &none, "history", |o| num(o, "chat.extent") > 0.).await;
    assert_eq!(out["chat.can_send"], Value::Bool(false), "empty draft");
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("ai.chat.open".into(), json!({"channel":""}))
    );
    // Heartbeat: the worker is refreshed again after the period, not before.
    for _ in 0..300 {
        chat.project(&none);
        if let Ok(call) = lab.calls.try_recv() {
            assert_eq!(call.0, "ai.chat.open");
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Send: exact text, then the composer clears and the view follows.
    let draft = stores(&[("chat.draft", Value::Text("build it\nplease".into()))]);
    let out = until(&chat, &draft, "can_send", |o| {
        o["chat.can_send"] == Value::Bool(true)
    })
    .await;
    assert_eq!(out["chat.can_new"], Value::Bool(true));
    let sent = chat.act("send", &draft);
    assert_eq!(sent.stores["chat.draft"], Value::Text(String::new()));
    loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 == "ai.chat.send" {
            assert_eq!(
                call.1,
                json!({"channel":"","text":"build it\nplease","attachments":[]})
            );
            break;
        }
    }
    // Whitespace-only text is never sent.
    assert!(
        chat.act(
            "send",
            &stores(&[("chat.draft", Value::Text("  \n".into()))])
        )
        .stores
        .is_empty()
    );
    // Stop while busy, approval answers.
    lab.push
        .send(snap(topic(
            &[("u1", "user", "hi"), ("a1", "assistant", "run?")],
            state(true, "Waiting", Some(approval())),
        )))
        .unwrap();
    let out = until(&chat, &none, "question", |o| {
        o["chat.has_question"] == Value::Bool(true)
    })
    .await;
    assert_eq!(out["chat.can_stop"], Value::Bool(true));
    chat.act("stop", &none);
    let pick = chat.act("option:0:0", &none);
    let answered = stores(&[("chat.answer.0", pick.stores["chat.answer.0"].clone())]);
    chat.act("reply", &answered);
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let call = lab.calls.recv().await.unwrap();
        if call.0 != "ai.chat.open" {
            seen.push(call);
        }
    }
    assert_eq!(seen[0], ("ai.chat.cancel".into(), json!({"channel":""})));
    assert_eq!(
        seen[1],
        (
            "ai.chat.approve".into(),
            json!({"channel":"","id":"approval-1","answers":{"approval":{"answers":["Allow once"]}}})
        )
    );
    // New chat and workspace are refused while busy, and a provider switch is explicit.
    chat.act("new", &none);
    chat.act("provider:codex", &none);
    chat.act("provider:bogus", &none);
    let call = loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 != "ai.chat.open" {
            break call;
        }
    };
    assert_eq!(
        call,
        (
            "ai.chat.provider".into(),
            json!({"channel":"","provider":"codex"})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_call_shows_an_error_and_a_refused_send_restores_the_text() {
    let _guard = serial();
    let mut lab = lab(false, topic(&[], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let none = Stores::default();
    until(&chat, &none, "state", |o| {
        string(o, "chat.heading").starts_with("Claude")
    })
    .await;
    lab.fail.lock().unwrap().push("ai.chat.send".into());
    let draft = stores(&[("chat.draft", Value::Text("lost".into()))]);
    chat.act("send", &draft);
    let out = until(&chat, &draft, "error", |o| {
        o["chat.status_error"] == Value::Bool(true)
    })
    .await;
    assert_eq!(string(&out, "chat.status"), "Request failed");
    assert!(lab.calls.try_recv().is_ok());
    // The worker later refuses a dispatched submission: the text returns in front of
    // what was typed since.
    lab.push
        .send(Frame::Msg(Message::Ev {
            topic: "ai.chat".into(),
            name: "send_failed".into(),
            seq: 1,
            data: json!({"channel":"","text":"unsent","attachments":[]}),
        }))
        .unwrap();
    let typed = stores(&[("chat.draft", Value::Text("typed".into()))]);
    let out = until(&chat, &typed, "restore", |o| o.contains_key("chat.draft")).await;
    assert_eq!(string(&out, "chat.draft"), "unsent\n\ntyped");
    // Other channels' events are ignored.
    route_event(&Message::Ev {
        topic: "ai.chat".into(),
        name: "send_failed".into(),
        seq: 2,
        data: json!({"channel":"p1","text":"other","attachments":[]}),
    });
    assert!(!chat.project(&typed).contains_key("chat.draft"));
}

#[tokio::test(flavor = "current_thread")]
async fn a_dropped_daemon_link_reopens_the_worker_and_resyncs() {
    let _guard = serial();
    let mut lab = lab(
        false,
        topic(&[("u1", "user", "before")], state(false, "", None)),
    )
    .await;
    let chat = chat_for(&lab).await;
    let none = Stores::default();
    until(&chat, &none, "history", |o| num(o, "chat.extent") > 0.).await;
    assert_eq!(lab.calls.recv().await.unwrap().0, "ai.chat.open");
    lab.push.send(Frame::Drop).unwrap();
    until(&chat, &none, "disconnect", |o| {
        o["chat.can_new"] == Value::Bool(false)
    })
    .await;
    assert!(!opened(&chat));
    // The client reconnects with a new boot id; the next projection reopens the channel.
    let out = until(&chat, &none, "reconnect", |o| {
        o["chat.can_new"] == Value::Bool(true)
    })
    .await;
    assert!(
        num(&out, "chat.extent") > 0.,
        "transcript survives the reconnect"
    );
    assert_eq!(lab.calls.recv().await.unwrap().0, "ai.chat.open");
}

#[tokio::test(flavor = "current_thread")]
async fn saved_chats_are_listed_only_when_published_and_resume_by_key() {
    let _guard = serial();
    let mut lab = lab(true, topic(&[], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let chats = stores(&[("chat.section", Value::Text("chats".into()))]);
    let out = until(
        &chat,
        &chats,
        "saved rows",
        |o| matches!(&o["chat.saved"], Value::List(r) if !r.is_empty()),
    )
    .await;
    let Value::List(rows) = &out["chat.saved"] else {
        panic!()
    };
    assert_eq!(rows[0]["a"], "resume:abc123");
    assert_eq!(rows[0]["t"], "Old chat");
    let resumed = chat.act("resume:abc123", &chats);
    assert_eq!(resumed.stores["chat.section"], Value::Text("chat".into()));
    loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 == "ai.chat.resume" {
            assert_eq!(call.1, json!({"channel":"","key":"abc123"}));
            break;
        }
    }
    // Without the method the tab says so.
    let (_s, plain) = offline(topic(&[], state(false, "", None)));
    let out = plain.project(&chats);
    assert!(string(&out, "chat.saved_state").contains("not available"));
}

// ---- goldens and the streaming frame budget ----------------------------------

fn harness(out: BTreeMap<String, Value>, scale: f32) -> bead_test::Result<Harness> {
    Harness::load(
        &root(),
        Path::new("chat/main.luau"),
        &root().join("palette.json"),
        out,
        bead_test::Config {
            width: 534,
            height: 714,
            scale,
        },
    )
}
fn golden(name: &str, h: &mut Harness, scale: f32) -> bead_test::Result<()> {
    let frame = h.render()?;
    let path = root().join(format!("tests/goldens/chat-{name}-{scale}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("bead-chat-{name}-{scale}-diff")),
    )
}

#[test]
fn chat_states_match_goldens() -> bead_test::Result<()> {
    let _guard = serial();
    let code = "Here is the fix:\n\n1. Update the manifest\n2. Run the build\n\n```sh\ncargo build --release\n```\n\nSee [the docs](https://example.org) for `flags`.";
    let states: [(&str, Json, &str); 4] = [
        ("empty", topic(&[], state(false, "", None)), ""),
        (
            "conversation",
            topic(
                &[
                    ("u1", "user", "Why does the release build fail on CI?"),
                    (
                        "a1",
                        "assistant",
                        "The linker runs out of memory because the job builds with too many parallel jobs. Limit it to two and the build fits comfortably.",
                    ),
                    ("u2", "user", "Thanks. Anything else?"),
                ],
                state(true, "Thinking", None),
            ),
            "Can you also\ncheck the cache?",
        ),
        (
            "code",
            topic(
                &[("u1", "user", "Show me"), ("a1", "assistant", code)],
                state(false, "", None),
            ),
            "",
        ),
        (
            "approval",
            topic(
                &[
                    ("u1", "user", "Build it"),
                    ("a1", "assistant", "Ready to run the build."),
                ],
                state(true, "Waiting", Some(approval())),
            ),
            "",
        ),
    ];
    for scale in [1., 1.5] {
        for (name, data, draft) in &states {
            let (_s, chat) = offline(data.clone());
            let mut out = chat.project(&stores(&[("chat.draft", Value::Text((*draft).into()))]));
            out.insert("chat.draft".into(), Value::Text((*draft).into()));
            let mut h = harness(out, scale)?;
            // A composer, transcript and (for approval) question rows exist as nodes.
            let ids: Vec<String> = h
                .session
                .view_nodes()
                .iter()
                .map(|n| n.id.clone())
                .collect();
            assert!(ids.iter().any(|i| i == "chat.composer"), "{ids:?}");
            assert_eq!(
                ids.iter().any(|i| i.starts_with("chat.questions")),
                *name == "approval"
            );
            golden(name, &mut h, scale)?;
        }
    }
    Ok(())
}

#[test]
fn streaming_at_fifty_tokens_per_second_stays_inside_the_frame_budget() -> bead_test::Result<()> {
    let _guard = serial();
    let (source, chat) = offline(topic(&[("u1", "user", "Explain")], state(true, "", None)));
    let out = chat.project(&Stores::default());
    let mut h = harness(out, 1.)?;
    let mut text = String::from("Streaming answer. ");
    let mut times = Vec::new();
    let tokens = [
        "alpha ",
        "beta ",
        "gamma ",
        "delta\n\n",
        "```\ncode line ",
        "more ",
        "\n```\n\n",
        "epsilon ",
    ];
    for i in 0..150 {
        text.push_str(tokens[i % tokens.len()]);
        source
            .publish(topic(
                &[("u1", "user", "Explain"), ("a1", "assistant", &text)],
                state(true, "", None),
            ))
            .unwrap();
        let started = std::time::Instant::now();
        let values = chat.project(&h.session.stores);
        h.update(values)?;
        h.render()?;
        times.push(started.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    let p99 = times[times.len() * 99 / 100];
    eprintln!(
        "CHAT_STREAM_FRAME_MS p50={:.2} p99={:.2} max={:.2} (CPU renderer, project+layout+render)",
        times[times.len() / 2],
        p99,
        times[times.len() - 1]
    );
    // Budget on a quiet host is 16.7 ms; the assertion leaves room for a shared CI runner.
    if !cfg!(debug_assertions) {
        assert!(p99 < 50., "p99 {p99:.2} ms");
    }
    Ok(())
}

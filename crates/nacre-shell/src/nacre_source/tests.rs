// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;
use tokio::net::UnixListener;
struct Lab(PathBuf);
impl Lab {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "nc-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn socket(&self) -> PathBuf {
        self.0.join("daemon.sock")
    }
    fn bind(&self) -> UnixListener {
        let listener = UnixListener::bind(self.socket()).unwrap();
        std::fs::set_permissions(self.socket(), std::fs::Permissions::from_mode(0o600)).unwrap();
        listener
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn text(name: &str) -> &'static str {
    match name {
        "handshake" => include_str!("../../tests/protocol-v1/handshake.jsonl"),
        "state" => include_str!("../../tests/protocol-v1/state.jsonl"),
        "null-snapshot" => include_str!("../../tests/protocol-v1/null-snapshot.jsonl"),
        "calls" => include_str!("../../tests/protocol-v1/calls.jsonl"),
        "events" => include_str!("../../tests/protocol-v1/events.jsonl"),
        "restart" => include_str!("../../tests/protocol-v1/restart.jsonl"),
        "ui" => include_str!("../../tests/protocol-v1/ui.jsonl"),
        "menu-apps" => include_str!("../../tests/protocol-v1/menu-apps.jsonl"),
        "menu-empty" => include_str!("../../tests/protocol-v1/menu-empty.jsonl"),
        "menu-results" => include_str!("../../tests/protocol-v1/menu-results.jsonl"),
        "menu-activation" => include_str!("../../tests/protocol-v1/menu-activation.jsonl"),
        "menu-conversion" => include_str!("../../tests/protocol-v1/menu-conversion.jsonl"),
        _ => panic!("unknown fixture"),
    }
}
fn fixture(name: &str) -> Vec<Message> {
    text(name)
        .lines()
        .map(|s| framing::decode(s.as_bytes()).unwrap())
        .collect()
}
#[test]
fn committed_transcripts_match_the_pinned_rust_exports() {
    let mut count = 0;
    for (path, generated) in nacre_protocol::artifacts::transcripts() {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".jsonl");
        let actual = text(name)
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect::<Vec<_>>();
        let expected = generated
            .lines()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .collect::<Vec<_>>();
        assert!(actual == expected, "pinned transcript differs: {name}");
        for message in fixture(name) {
            let encoded = framing::encode(&message).unwrap();
            assert!(
                framing::decode(&encoded).unwrap() == message,
                "typed roundtrip: {name}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 31);
}

fn welcome() -> Message {
    fixture("handshake")[1].clone()
}

#[test]
fn committed_menu_transcripts_match_the_pinned_exports() {
    for (path, generated) in nacre_protocol::artifacts::menu_transcripts() {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".jsonl");
        let actual: Vec<Value> = text(name)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let expected: Vec<Value> = generated
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(actual, expected, "canonical menu transcript changed");
        for message in fixture(name) {
            assert_eq!(
                framing::decode(&framing::encode(&message).unwrap()).unwrap(),
                message
            );
        }
    }
}

#[tokio::test]
async fn published_menu_calls_and_errors_roundtrip_through_a_fake_daemon() {
    for name in [
        "menu-apps",
        "menu-empty",
        "menu-results",
        "menu-activation",
        "menu-conversion",
    ] {
        let lab = Lab::new();
        let listener = lab.bind();
        let trace = fixture(name);
        let server_trace = trace.clone();
        let server = tokio::spawn(async move {
            let (mut reader, mut writer) = accepted(&listener).await;
            for pair in server_trace.as_chunks::<2>().0 {
                let observed = next(&mut reader).await;
                match (&observed, &pair[0]) {
                    (
                        Message::Call { id, m, p, timeout },
                        Message::Call {
                            id: want_id,
                            m: want_m,
                            p: want_p,
                            timeout: budget,
                        },
                    ) => {
                        assert_eq!(id, want_id);
                        assert_eq!(m, want_m);
                        assert_eq!(p, want_p);
                        assert!(
                            timeout.is_some_and(|n| n > 0 && n <= budget.unwrap_or(1000)),
                            "wire deadline must count down within the requested budget"
                        );
                    }
                    _ => panic!("expected the canonical menu call"),
                }
                framing::write(&mut writer, &pair[1]).await.unwrap();
            }
        });
        let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while client.status().boot.is_none() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        for pair in trace.as_chunks::<2>().0 {
            let Message::Call { id, m, p, timeout } = pair[0].clone() else {
                panic!("golden request")
            };
            let actual = client
                .call_id(
                    Some(id),
                    m,
                    p,
                    Duration::from_millis(u64::from(timeout.unwrap_or(1000))),
                )
                .await;
            match &pair[1] {
                Message::Ret {
                    reply: nacre_protocol::Reply::Ok { ok },
                    ..
                } => assert_eq!(actual.unwrap(), *ok),
                Message::Ret {
                    reply: nacre_protocol::Reply::Err { err },
                    ..
                } => {
                    let actual = actual.unwrap_err();
                    assert_eq!(actual.message, err.message);
                    assert_eq!(actual.retry, err.retry);
                }
                _ => panic!("golden reply"),
            }
        }
        Source::stop(&client).await.unwrap();
        server.await.unwrap();
    }
}
async fn next(reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Message {
    tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
async fn accepted(
    listener: &UnixListener,
) -> (
    framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    tokio::net::unix::OwnedWriteHalf,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut r = framing::Reader::new(BufReader::new(r));
    assert!(matches!(
        next(&mut r).await,
        Message::Hello { proto: 1, .. }
    ));
    framing::write(&mut w, &welcome()).await.unwrap();
    (r, w)
}
async fn fresh(topic: &Topic, predicate: impl Fn(&Value) -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let s = topic.snapshot();
            if s.connected && predicate(&s.data) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn permissions_and_peer_identity_are_required() {
    let lab = Lab::new();
    let _listener = lab.bind();
    check_socket(&lab.socket()).unwrap();
    std::fs::set_permissions(&lab.0, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(check_socket(&lab.socket()).is_err());
    std::fs::set_permissions(&lab.0, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(lab.socket(), std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(check_socket(&lab.socket()).is_err());
    assert!(check_peer(nix::unistd::geteuid().as_raw().wrapping_add(1)).is_err());
    assert!(check_socket(std::path::Path::new("relative.sock")).is_err());
}
#[tokio::test(flavor = "current_thread")]
async fn published_state_transcript_keeps_wrong_base_then_resyncs() {
    let lab = Lab::new();
    let listener = lab.bind();
    let resync = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let server_resync = resync.clone();
    let server_release = release.clone();
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted(&listener).await;
        assert_eq!(
            next(&mut r).await,
            Message::Sub {
                topics: vec!["compositor".into()]
            }
        );
        let f = fixture("state");
        for m in &f[1..4] {
            framing::write(&mut w, m).await.unwrap();
        }
        assert_eq!(next(&mut r).await, f[4]);
        server_resync.notify_one();
        server_release.notified().await;
        framing::write(&mut w, &f[5]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let client = Client::start(Config::new(lab.socket(), vec!["compositor".into()])).unwrap();
    let topic = client.topic("compositor").unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let capture = seen.clone();
    let _sub = topic
        .subscribe(Arc::new(move |s| {
            if s.connected {
                capture.lock().unwrap().push(s.data.clone());
            }
        }))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), resync.notified())
        .await
        .unwrap();
    assert!(
        topic.snapshot().data["windows"]["w1"]["focused"] == true,
        "wrong-base patch must not mutate old data"
    );
    assert!(
        !topic.snapshot().connected,
        "gap stays stale until resync snapshot"
    );
    release.notify_one();
    fresh(&topic, |v| v["windows"]["w1"]["focused"] == false).await;
    {
        let values = seen.lock().unwrap();
        assert!(values.iter().any(|v| v["windows"]["w1"]["focused"] == true));
        assert!(values.len() >= 3);
    }
    topic.stop().await.unwrap();
    server.await.unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn restart_reconnect_resubscribes_before_accepting_sequence_zero() {
    restart_case(false).await;
}
#[tokio::test(flavor = "current_thread")]
async fn identical_topic_data_cannot_hide_a_daemon_boot_change() {
    restart_case(true).await;
}
async fn restart_case(identical_data: bool) {
    let lab = Lab::new();
    let listener = lab.bind();
    let f = fixture("restart");
    let second = f[2].clone();
    let initial = f[1].clone();
    let mut replacement = f[4].clone();
    if identical_data
        && let (Message::Snap { data, .. }, Message::Snap { data: initial, .. }) =
            (&mut replacement, &initial)
    {
        *data = initial.clone();
    }
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted(&listener).await;
        let sub = next(&mut r).await;
        framing::write(&mut w, &initial).await.unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        drop(w);
        drop(r);
        let (s, _) = listener.accept().await.unwrap();
        let (r, mut w) = s.into_split();
        let mut r = framing::Reader::new(BufReader::new(r));
        assert!(matches!(next(&mut r).await, Message::Hello { .. }));
        framing::write(&mut w, &second).await.unwrap();
        assert_eq!(next(&mut r).await, sub);
        framing::write(&mut w, &replacement).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let client = Client::start(Config::new(lab.socket(), vec!["compositor".into()])).unwrap();
    let topic = client.topic("compositor").unwrap();
    fresh(&topic, |v| v["windows"].is_object()).await;
    assert_eq!(
        topic.snapshot().epoch,
        "00000000-0000-4000-8000-000000000001"
    );
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(2), async {
        while client.status().boot.as_deref() != Some("00000000-0000-4000-8000-000000000002") {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    fresh(&topic, |v| v["windows"].is_object()).await;
    assert_eq!(
        topic.snapshot().epoch,
        "00000000-0000-4000-8000-000000000002"
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    topic.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn published_rpc_events_and_deadlines_use_the_same_client_link() {
    let lab = Lab::new();
    let listener = lab.bind();
    let f = fixture("calls");
    let events = fixture("events");
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted(&listener).await;
        let call = next(&mut r).await;
        assert!(matches!(call,Message::Call{id:Id::Integer(7),ref m,..}if m=="daemon.ping"));
        framing::write(&mut w, &f[1]).await.unwrap();
        let call = next(&mut r).await;
        assert!(
            matches!(call,Message::Call{id:Id::Text(ref id),ref m,..}if id=="missing"&&m=="example.missing")
        );
        framing::write(&mut w, &f[3]).await.unwrap();
        let call = next(&mut r).await;
        assert!(
            matches!(call,Message::Call{id:Id::Text(ref id),ref m,..}if id=="deadline"&&m=="ui.dispatch")
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        framing::write(&mut w, &f[5]).await.unwrap();
        for message in &events[..3] {
            framing::write(&mut w, message).await.unwrap();
        }
        assert_eq!(next(&mut r).await, events[3]);
        framing::write(&mut w, &events[4]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), r.next())
                .await
                .is_err(),
            "expired calls must not replay"
        );
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let capture = seen.clone();
    let mut config = Config::new(lab.socket(), vec![]);
    config.event = Some(Arc::new(move |m| capture.lock().unwrap().push(m.clone())));
    let client = Client::start(config).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        client
            .call_id(
                Some(Id::Integer(7)),
                "daemon.ping".into(),
                serde_json::json!({}),
                Duration::from_secs(1)
            )
            .await
            .unwrap(),
        Value::Null
    );
    let link = Source::snapshot(&client);
    assert!(link.connected);
    assert_eq!(link.data["boot"], serde_json::json!(client.status().boot));
    // Match the no-explicit-deadline golden semantically; the adapter always sends
    // its remaining caller deadline, which the old transcript permits optionally.
    let error = client
        .call_id(
            Some(Id::Text("missing".into())),
            "example.missing".into(),
            serde_json::json!({}),
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "unknown_method");
    let error = client
        .call_id(
            Some(Id::Text("deadline".into())),
            "ui.dispatch".into(),
            serde_json::json!({}),
            Duration::from_millis(5),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "timeout");
    let (reply, rx) = oneshot::channel();
    client
        .0
        .tx
        .try_send(Command::Wire(fixture("events")[3].clone(), reply))
        .unwrap();
    rx.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while seen.lock().unwrap().len() < 4 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    {
        let seen = seen.lock().unwrap();
        let expected = fixture("events");
        assert!(seen[..3] == expected[..3]);
        assert_eq!(seen[3], expected[4]);
    }
    server.await.unwrap();
    client.control(true).await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn published_ui_transcript_forwards_calls_to_an_injected_provider() {
    let lab = Lab::new();
    let listener = lab.bind();
    let f = fixture("ui");
    let server = tokio::spawn(async move {
        let (mut provider, mut pw) = accepted(&listener).await;
        assert_eq!(next(&mut provider).await, f[0]);
        let (mut caller, mut cw) = accepted(&listener).await;
        let request = next(&mut caller).await;
        if let Message::Call { id, m, p, .. } = &request {
            if let Message::Call {
                id: expected,
                m: em,
                p: ep,
                ..
            } = &f[1]
            {
                assert!(id == expected && m == em && p == ep);
            }
        } else {
            panic!("call expected");
        }
        framing::write(&mut pw, &f[2]).await.unwrap();
        assert_eq!(next(&mut provider).await, f[3]);
        framing::write(&mut cw, &f[4]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let mut config = Config::new(lab.socket(), vec![]);
    config.provider = Some((
        "ui".into(),
        BTreeMap::from([("ui.dispatch".into(), 1)]),
        Arc::new(|method, params| {
            assert_eq!(method, "ui.dispatch");
            assert_eq!(params["method"], "launcher");
            Box::pin(async { Ok(Value::Null) })
        }),
    ));
    let provider = Client::start(config).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while provider.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let caller = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while caller.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let Message::Call { id, m, p, .. } = fixture("ui")[1].clone() else {
        panic!("fixture call")
    };
    assert_eq!(
        caller
            .call_id(Some(id), m, p, Duration::from_secs(1))
            .await
            .unwrap(),
        Value::Null
    );
    provider.control(true).await.unwrap();
    caller.control(true).await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn partial_frame_survives_an_intervening_outbound_call() {
    use tokio::io::AsyncWriteExt;
    let lab = Lab::new();
    let listener = lab.bind();
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted(&listener).await;
        assert!(matches!(next(&mut r).await, Message::Sub { .. }));
        let line = framing::encode(&Message::Snap {
            topic: "compositor".into(),
            seq: 0,
            data: serde_json::json!({"text":"café"}),
        })
        .unwrap();
        let split = line.iter().position(|b| *b == 0xc3).unwrap() + 1;
        w.write_all(&line[..split]).await.unwrap();
        let Message::Call { id, .. } = next(&mut r).await else {
            panic!("interleaved call")
        };
        w.write_all(&line[split..]).await.unwrap();
        framing::write(
            &mut w,
            &Message::Ret {
                id,
                reply: Reply::Ok { ok: Value::Null },
            },
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let client = Client::start(Config::new(lab.socket(), vec!["compositor".into()])).unwrap();
    let topic = client.topic("compositor").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await; // reader has a partial UTF-8 frame
    client
        .call(
            "daemon.ping".into(),
            serde_json::json!({}),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    fresh(&topic, |v| v["text"] == "café").await;
    topic.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn actual_ui_provider_acknowledges_dispatch_before_a_pending_menu_reply() {
    let lab = Lab::new();
    let listener = lab.bind();
    let ui_path = lab.0.join("ui.sock");
    let handled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = handled.clone();
    let _ui = bead::services::ipc::Server::bind(ui_path.clone(), move |call| {
        assert_eq!(call.target, "nacre");
        assert_eq!(call.method, "launcher");
        assert_eq!(call.args, vec![serde_json::json!("")]);
        count.fetch_add(1, Ordering::SeqCst);
        bead::services::ipc::Reply::ok(call.id, Value::Null)
    })
    .unwrap();
    let menu = fixture("menu-apps");
    let request = menu[0].clone();
    let server = tokio::spawn(async move {
        let ui = fixture("ui");
        let (mut reader, mut writer) = accepted(&listener).await;
        assert_eq!(next(&mut reader).await, ui[0]);
        let observed = next(&mut reader).await;
        let Message::Call { id, m, p, .. } = observed else {
            panic!("outbound menu call expected")
        };
        let Message::Call {
            id: expected,
            m: method,
            p: params,
            ..
        } = &menu[0]
        else {
            panic!("golden menu call expected")
        };
        assert_eq!((&id, &m, &p), (expected, method, params));
        // Do not release the outgoing reply until this same client services
        // the incoming UI call. Waiting synchronously on the menu here would
        // deadlock. Use the published string provider ID and exact ACK.
        framing::write(&mut writer, &ui[2]).await.unwrap();
        assert_eq!(next(&mut reader).await, ui[3]);
        framing::write(&mut writer, &menu[1]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let provider = Arc::new(crate::ui_provider::start(lab.socket(), ui_path, None).unwrap());
    tokio::time::timeout(Duration::from_secs(5), async {
        while provider.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let Message::Call { id, m, p, .. } = request else {
        panic!("golden menu call expected")
    };
    let result = provider
        .call_id(Some(id), m, p, Duration::from_secs(5))
        .await
        .unwrap();
    let Message::Ret {
        reply: nacre_protocol::Reply::Ok { ok: expected },
        ..
    } = fixture("menu-apps")[1].clone()
    else {
        panic!("golden menu result expected")
    };
    assert_eq!(result, expected);
    assert_eq!(handled.load(Ordering::SeqCst), 1);
    provider.control(true).await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn full_snapshots_preserve_new_null_members() {
    let lab = Lab::new();
    let listener = lab.bind();
    let f = fixture("null-snapshot");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (r, mut w) = stream.into_split();
        let mut r = framing::Reader::new(BufReader::new(r));
        assert!(matches!(next(&mut r).await, Message::Hello { .. }));
        let Message::Welcome {
            proto,
            min,
            boot,
            methods,
            frame_ms,
            ..
        } = welcome()
        else {
            panic!("welcome")
        };
        let welcome = Message::Welcome {
            proto,
            min,
            boot,
            methods,
            frame_ms,
            topics: BTreeMap::from([("settings.desktop".into(), 1)]),
        };
        framing::write(&mut w, &welcome).await.unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Sub {
                topics: vec!["settings.desktop".into()]
            }
        );
        for message in f {
            framing::write(&mut w, &message).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let client = Client::start(Config::new(lab.socket(), vec!["settings.desktop".into()])).unwrap();
    let topic = client.topic("settings.desktop").unwrap();
    fresh(&topic, |v| {
        v.as_object().is_some_and(|o| o.contains_key("optional"))
    })
    .await;
    assert!(topic.snapshot().data["optional"].is_null());
    topic.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn stop_interrupts_a_peer_that_does_not_read_a_large_call() {
    let lab = Lab::new();
    let listener = lab.bind();
    let server = tokio::spawn(async move {
        let (_r, _w) = accepted(&listener).await;
        tokio::time::sleep(Duration::from_secs(10)).await;
    });
    let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let caller = client.clone();
    let call = tokio::spawn(async move {
        caller
            .call(
                "daemon.ping".into(),
                serde_json::json!({"large":"x".repeat(900_000)}),
                Duration::from_secs(3),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    tokio::time::timeout(Duration::from_secs(1), client.control(true))
        .await
        .unwrap()
        .unwrap();
    assert!(call.await.unwrap().is_err());
    server.abort();
    let _ = server.await;
}

#[test]
fn early_link_threads_inherit_ui_signals_without_changing_the_caller_mask() {
    use nix::sys::signal::{SigSet, Signal};
    let before = SigSet::thread_get_mask().unwrap();
    let child = {
        let _mask = super::LinkSignalMask::block().unwrap();
        std::thread::spawn(|| SigSet::thread_get_mask().unwrap())
    };
    let child = child.join().unwrap();
    assert!(child.contains(Signal::SIGINT) && child.contains(Signal::SIGTERM));
    let after = SigSet::thread_get_mask().unwrap();
    for signal in [Signal::SIGINT, Signal::SIGTERM] {
        assert_eq!(before.contains(signal), after.contains(signal));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn connection_mark_keeps_last_snapshot_and_provider_registers_after_new_boot() {
    let lab = Lab::new();
    let listener = lab.bind();
    let (drop_tx, drop_rx) = oneshot::channel();
    let (resume_tx, resume_rx) = oneshot::channel();
    let (finish_tx, finish_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut r, w) = accepted(&listener).await;
        let registration = next(&mut r).await;
        assert!(
            matches!(&registration, Message::Register { provider, methods } if provider == "ui" && methods.get("ui.dispatch") == Some(&1))
        );
        drop_rx.await.unwrap();
        drop(w);
        drop(r);
        resume_rx.await.unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let (r, mut w) = stream.into_split();
        let mut r = framing::Reader::new(BufReader::new(r));
        assert!(matches!(next(&mut r).await, Message::Hello { .. }));
        framing::write(&mut w, &fixture("restart")[2])
            .await
            .unwrap();
        assert_eq!(next(&mut r).await, registration);
        let _ = finish_rx.await;
    });
    let mut config = Config::new(lab.socket(), vec![]);
    config.provider = Some((
        "ui".into(),
        BTreeMap::from([("ui.dispatch".into(), 1)]),
        Arc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
    ));
    let client = Client::start(config).unwrap();
    let connection = client.connection();
    fresh(&connection, |v| v["connected"] == true).await;
    let last = client.snapshot();
    drop_tx.send(()).unwrap();
    fresh(&connection, |v| v["connected"] == false).await;
    assert_eq!(connection.snapshot().data["reconnecting"], false);
    fresh(&connection, |v| v["reconnecting"] == true).await;
    assert!(!client.snapshot().connected);
    assert_eq!(client.snapshot().data, last.data);
    resume_tx.send(()).unwrap();
    let ready = Instant::now();
    fresh(&connection, |v| v["connected"] == true).await;
    assert_eq!(connection.snapshot().data["reconnecting"], false);
    assert_eq!(
        client.status().boot.as_deref(),
        Some("00000000-0000-4000-8000-000000000002")
    );
    assert!(ready.elapsed() < Duration::from_secs(1));
    client.stop().await.unwrap();
    finish_tx.send(()).unwrap();
    server.await.unwrap();
}

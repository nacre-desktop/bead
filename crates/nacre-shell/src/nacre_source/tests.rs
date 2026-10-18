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
        "menu-ui-actions" => include_str!("../../tests/protocol-v1/menu-ui-actions.jsonl"),
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

#[test]
fn committed_typed_action_transcript_matches_pinned_exports_and_validates_rows() {
    let mut frames = 0;
    let mut provider_calls = 0;
    for (path, generated) in nacre_protocol::artifacts::menu_action_transcripts() {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".jsonl");
        let expected = generated
            .lines()
            .map(|line| framing::decode(line.as_bytes()).unwrap())
            .collect::<Vec<_>>();
        let actual = fixture(name);
        assert_eq!(actual, expected, "supplementary action transcript changed");
        for message in actual {
            assert_eq!(
                framing::decode(&framing::encode(&message).unwrap()).unwrap(),
                message
            );
            frames += 1;
            if let Message::Call {
                id: Id::Text(id),
                m,
                p,
                ..
            } = message
                && m == "ui.menu.activate"
            {
                assert!(id.starts_with("provider-"));
                let request: nacre_protocol::menu::MenuUiActivate =
                    serde_json::from_value(p).unwrap();
                request.validate().unwrap();
                let mut invalid = request.clone();
                invalid.row.operation.clear();
                assert!(invalid.validate().is_err());
                invalid = request;
                invalid.catalog_rev = nacre_protocol::MAX_SAFE_INTEGER + 1;
                assert!(invalid.validate().is_err());
                provider_calls += 1;
            }
        }
    }
    assert_eq!((frames, provider_calls), (19, 5));
}

#[tokio::test(flavor = "current_thread")]
async fn typed_provider_enqueues_and_acks_before_its_pending_activation_reply() {
    // This is an injected test provider, not an advertised product executor.
    // Deliberately do not consume/execute its bounded queue until the outbound
    // activation has completed: an ACK that waits for execution would deadlock.
    let lab = Lab::new();
    let listener = lab.bind();
    let golden = fixture("menu-ui-actions");
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accepted(&listener).await;
        assert_eq!(next(&mut reader).await, golden[0]);
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("activation expected")
        };
        let Message::Call {
            id: expected,
            m: method,
            p: params,
            ..
        } = &golden[3]
        else {
            panic!("golden activation")
        };
        assert_eq!((&id, &m, &p), (expected, method, params));
        framing::write(&mut writer, &golden[4]).await.unwrap();
        assert_eq!(
            next(&mut reader).await,
            golden[5],
            "exact provider-N acceptance ACK must precede reply"
        );
        framing::write(&mut writer, &golden[6]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
    let (queue, mut pending) = tokio::sync::mpsc::channel(1);
    let mut config = Config::new(lab.socket(), vec![]);
    let Message::Register { provider, methods } = fixture("menu-ui-actions")[0].clone() else {
        panic!("golden registration")
    };
    config.provider = Some((
        provider,
        methods,
        Arc::new(move |method, params| {
            let queue = queue.clone();
            Box::pin(async move {
                assert_eq!(method, "ui.menu.activate");
                let request: nacre_protocol::menu::MenuUiActivate = serde_json::from_value(params)
                    .map_err(|_| Error::new("validation", "Invalid typed UI action", false))?;
                request.validate().map_err(failure)?;
                queue
                    .try_send(request)
                    .map_err(|_| Error::new("busy", "Test UI queue full", true))?;
                Ok(serde_json::json!({"keep_open":false}))
            })
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
    let Message::Call { id, m, p, .. } = fixture("menu-ui-actions")[3].clone() else {
        panic!("golden request")
    };
    assert_eq!(
        provider
            .call_id(Some(id), m, p, Duration::from_secs(5))
            .await
            .unwrap(),
        serde_json::json!({"keep_open":false})
    );
    let queued = pending
        .try_recv()
        .expect("accepted action remains queued, unexecuted");
    let Message::Call { p, .. } = fixture("menu-ui-actions")[4].clone() else {
        panic!("golden typed row")
    };
    assert_eq!(serde_json::to_value(queued).unwrap(), p);
    assert!(pending.try_recv().is_err());
    provider.control(true).await.unwrap();
    server.await.unwrap();
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

async fn fresh_lease(topic: &LeasedTopic, value: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !topic.snapshot().connected || topic.snapshot().data["page"] != value {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

async fn accepted_dashboard(
    listener: &UnixListener,
    boot: &str,
    version: u16,
) -> (
    framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    tokio::net::unix::OwnedWriteHalf,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut r = framing::Reader::new(BufReader::new(r));
    assert!(matches!(next(&mut r).await, Message::Hello { .. }));
    let mut welcome = welcome();
    if let Message::Welcome {
        topics, boot: id, ..
    } = &mut welcome
    {
        *id = boot.into();
        topics.insert("dashboard.system".into(), version);
    }
    framing::write(&mut w, &welcome).await.unwrap();
    (r, w)
}

#[tokio::test(flavor = "current_thread")]
async fn last_page_lease_unsubscribes_releases_payload_and_preserves_link() {
    let lab = Lab::new();
    let listener = lab.bind();
    let unsubscribed = Arc::new(Notify::new());
    let done = Arc::new(Notify::new());
    let observed = unsubscribed.clone();
    let finished = done.clone();
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted_dashboard(&listener, "page-boot", 1).await;
        assert_eq!(
            next(&mut r).await,
            Message::Sub {
                topics: vec!["compositor".into()]
            }
        );
        assert_eq!(
            next(&mut r).await,
            Message::Sub {
                topics: vec!["dashboard.system".into()]
            }
        );
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "dashboard.system".into(),
                seq: 0,
                data: serde_json::json!({"page":"first", "large": "x".repeat(128 * 1024)}),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Resync {
                topics: vec!["dashboard.system".into()]
            }
        );
        // A queued delta cannot revive a page awaiting its requested snapshot.
        framing::write(
            &mut w,
            &Message::Patch {
                topic: "dashboard.system".into(),
                base: 0,
                seq: 1,
                data: serde_json::json!({"page":"should-not-display"}),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Resync {
                topics: vec!["dashboard.system".into()]
            }
        );
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "dashboard.system".into(),
                seq: 1,
                data: serde_json::json!({"page":"second", "large": "y".repeat(128 * 1024)}),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Unsub {
                topics: vec!["dashboard.system".into()]
            }
        );
        framing::write(
            &mut w,
            &Message::Ping {
                id: Id::Integer(701),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Pong {
                id: Id::Integer(701)
            }
        );
        observed.notify_one();
        finished.notified().await;
    });
    let client = Client::start(Config::new(lab.socket(), vec!["compositor".into()])).unwrap();
    assert!(client.lease_topic("compositor").is_err());
    let first = client.lease_topic("dashboard.system").unwrap();
    let first_clone = first.clone();
    let second = client.lease_topic("dashboard.system").unwrap();
    fresh_lease(&first, "first").await;
    first.stop().await.unwrap();
    assert!(!first_clone.snapshot().connected);
    assert!(first_clone.snapshot().data.as_object().unwrap().is_empty());
    assert!(
        first_clone
            .invoke("unused".into(), Value::Null)
            .await
            .is_err()
    );
    second.reconnect().await.unwrap();
    fresh_lease(&second, "second").await;
    let snapshot = second.snapshot();
    let weak = Arc::downgrade(&snapshot.data);
    drop(snapshot);
    // Retaining a subscription token must not retain the page's last snapshot.
    let _subscription = second.subscribe(Arc::new(|_| {})).unwrap();
    second.stop().await.unwrap();
    second.stop().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), unsubscribed.notified())
        .await
        .unwrap();
    assert!(
        weak.upgrade().is_none(),
        "JSON released from publisher and merge mirror"
    );
    assert_eq!(client.status().boot.as_deref(), Some("page-boot"));
    assert!(!client.0.shared.stop.load(Ordering::Acquire));
    done.notify_one();
    client.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn page_lease_resubscribes_on_boot_and_drop_unsubscribes() {
    let lab = Lab::new();
    let listener = lab.bind();
    let restart = Arc::new(Notify::new());
    let go = restart.clone();
    let done = Arc::new(Notify::new());
    let finished = done.clone();
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted_dashboard(&listener, "first-boot", 1).await;
        let sub = Message::Sub {
            topics: vec!["dashboard.system".into()],
        };
        assert_eq!(next(&mut r).await, sub);
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "dashboard.system".into(),
                seq: 0,
                data: serde_json::json!({"page":"first"}),
            },
        )
        .await
        .unwrap();
        go.notified().await;
        drop(r);
        drop(w);
        let (mut r, mut w) = accepted_dashboard(&listener, "next-boot", 1).await;
        assert_eq!(next(&mut r).await, sub);
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "dashboard.system".into(),
                seq: 0,
                data: serde_json::json!({"page":"second"}),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Unsub {
                topics: vec!["dashboard.system".into()]
            }
        );
        finished.notify_one();
        let _ = r.next().await;
    });
    let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    let page = client.lease_topic("dashboard.system").unwrap();
    fresh_lease(&page, "first").await;
    assert_eq!(page.snapshot().epoch, "first-boot");
    restart.notify_one();
    fresh_lease(&page, "second").await;
    assert_eq!(page.snapshot().epoch, "next-boot");
    drop(page);
    tokio::time::timeout(Duration::from_secs(5), done.notified())
        .await
        .unwrap();
    client.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn network_core_lease_exists_only_for_open_runtime() {
    use bead::wayland::UiEvent;
    use std::{
        path::Path,
        sync::{OnceLock, mpsc},
    };

    let lab = Lab::new();
    let listener = lab.bind();
    let link_alive = Arc::new(Notify::new());
    let observed_link = link_alive.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (r, mut w) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(r));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        let mut hello = welcome();
        if let Message::Welcome { topics, .. } = &mut hello {
            topics.insert("network".into(), 1);
        }
        framing::write(&mut w, &hello).await.unwrap();
        loop {
            match next(&mut reader).await {
                Message::Call { id, m, .. } => {
                    assert_eq!(m, "daemon.ping");
                    framing::write(
                        &mut w,
                        &Message::Ret {
                            id,
                            reply: Reply::Ok { ok: Value::Null },
                        },
                    )
                    .await
                    .unwrap();
                }
                Message::Sub { topics } => {
                    assert_eq!(topics, vec!["network"]);
                    break;
                }
                message => panic!("unexpected message before network lease: {message:?}"),
            }
        }
        let network = serde_json::json!({
            "health":{"available":true,"stale":false},
            "wireless_enabled":true,
            "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
            "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":80,"secured":true,"active":true,"connection":"wifi"}},"order":["ap0"]},
            "connections":{"items":{"wifi":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wifi"]},
            "primary_connection":"wifi"
        });
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: network.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["network".into()]
            },
            "retiring the view must release only its topic lease"
        );
        // A peer that sends after Unsub must not wake the retired view.
        framing::write(
            &mut w,
            &Message::Snap {
                topic: "network".into(),
                seq: 1,
                data: network,
            },
        )
        .await
        .unwrap();
        framing::write(
            &mut w,
            &Message::Ping {
                id: Id::Integer(703),
            },
        )
        .await
        .unwrap();
        loop {
            match next(&mut reader).await {
                Message::Pong {
                    id: Id::Integer(703),
                } => break,
                Message::Ping { id } => {
                    framing::write(&mut w, &Message::Pong { id }).await.unwrap();
                }
                message => panic!("unexpected message while checking shared link: {message:?}"),
            }
        }
        observed_link.notify_one();
        loop {
            match tokio::time::timeout(Duration::from_secs(5), reader.next()).await {
                Ok(Ok(Some(Message::Ping { id }))) => {
                    framing::write(&mut w, &Message::Pong { id }).await.unwrap();
                }
                Ok(Ok(Some(message))) => {
                    panic!("unexpected message while preserving shared link: {message:?}")
                }
                Ok(Ok(None) | Err(_)) => break,
                Err(_) => panic!("client did not stop after the lease test finished"),
            }
        }
        observed_link.notify_one();
    });

    let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let palette = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = Arc::new(bead::render::Palette::read(&palette).unwrap());
    let scale: Value =
        serde_json::from_str(include_str!("../../../bead-ui/assets/size-scale.json")).unwrap();
    let radius = scale["radii"]["medium"].as_f64().unwrap() as f32;
    let spec = crate::popouts::status(
        crate::status_views::Kind::Network,
        Arc::new(palette),
        colors,
        Arc::new(OnceLock::new()),
        Some(client.clone()),
        radius,
        false,
    )
    .unwrap();
    assert!(!client.0.shared.leases.has_topic("network"));

    let mut runtime = (spec.factory)(true).unwrap();
    let (send, receive) = mpsc::channel();
    runtime.services(Box::new(move |event| {
        let _ = send.send(event);
    }));
    let event = receive.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(&event, UiEvent::Model(_)));
    let _ = runtime.stage(event).unwrap();
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.0.shared.leases.has_topic("network") {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        receive.recv_timeout(Duration::from_millis(50)).is_err(),
        "a retired Runtime must receive no callback after its lease is released"
    );
    tokio::time::timeout(Duration::from_secs(2), link_alive.notified())
        .await
        .unwrap();
    assert!(
        client.status().boot.is_some(),
        "the shared client stays connected"
    );
    drop(spec);
    Source::stop(&client).await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn page_lease_does_not_subscribe_to_unadvertised_version() {
    let lab = Lab::new();
    let listener = lab.bind();
    let pong = Arc::new(Notify::new());
    let received = pong.clone();
    let server = tokio::spawn(async move {
        let (mut r, mut w) = accepted_dashboard(&listener, "unsupported-page", 2).await;
        framing::write(
            &mut w,
            &Message::Ping {
                id: Id::Integer(702),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut r).await,
            Message::Pong {
                id: Id::Integer(702)
            }
        );
        received.notify_one();
        assert!(
            r.next().await.unwrap().is_none(),
            "no sub for unsupported topic version"
        );
    });
    let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    let page = client.lease_topic("dashboard.system").unwrap();
    tokio::time::timeout(Duration::from_secs(5), pong.notified())
        .await
        .unwrap();
    assert!(!page.snapshot().connected);
    page.stop().await.unwrap();
    client.stop().await.unwrap();
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn page_leases_share_the_protocol_subscription_cap_and_reuse_released_slots() {
    let lab = Lab::new();
    let client = Client::start(Config::new(lab.socket(), vec!["compositor".into()])).unwrap();
    assert!(client.lease_topic("bad\0topic").is_err());
    let mut pages = (0..nacre_protocol::MAX_SUBSCRIPTIONS - 1)
        .map(|i| client.lease_topic(&format!("page-{i}")).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(client.lease_topic("overflow").err().unwrap().code, "busy");
    let shared = client.lease_topic("page-0").unwrap();
    drop(pages.remove(0));
    assert!(
        client.lease_topic("overflow").is_err(),
        "independent user still owns the slot"
    );
    drop(shared);
    let reused = client.lease_topic("replacement").unwrap();
    drop(reused);
    drop(pages);
    assert!(client.0.shared.leases.wanted().is_empty());
    client.stop().await.unwrap();
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
    actual_provider_with_pending_menu("menu-apps").await;
}

#[tokio::test(flavor = "current_thread")]
async fn actual_ui_provider_acknowledges_dispatch_before_a_pending_activation_reply() {
    actual_provider_with_pending_menu("menu-activation").await;
}

async fn actual_provider_with_pending_menu(golden: &str) {
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
    let menu = fixture(golden);
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
    } = fixture(golden)[1].clone()
    else {
        panic!("golden menu result expected")
    };
    assert_eq!(result, expected);
    assert_eq!(handled.load(Ordering::SeqCst), 1);
    provider.control(true).await.unwrap();
    server.await.unwrap();
}

fn provider_config(socket: PathBuf, action: Action) -> Config {
    let mut config = Config::new(socket, vec![]);
    config.provider = Some((
        "ui".into(),
        BTreeMap::from([("ui.dispatch".into(), 1)]),
        action,
    ));
    config
}

fn provider_call(id: &str) -> Message {
    Message::Call {
        id: Id::Text(id.into()),
        m: "ui.dispatch".into(),
        p: serde_json::json!({"target":"nacre","method":"launcher","args":[""]}),
        timeout: Some(1000),
    }
}

async fn provider_failure_exchange(
    listener: UnixListener,
    request_id: &'static str,
    invocations: Arc<std::sync::atomic::AtomicUsize>,
    request_sent: oneshot::Sender<()>,
    verified: oneshot::Sender<()>,
    shutdown: oneshot::Receiver<()>,
) {
    let (mut reader, mut writer) = accepted(&listener).await;
    assert_eq!(next(&mut reader).await, fixture("ui")[0]);
    framing::write(&mut writer, &provider_call(request_id))
        .await
        .unwrap();
    request_sent.send(()).unwrap();
    let Message::Ret {
        id: failed_id,
        reply: Reply::Err { err },
    } = next(&mut reader).await
    else {
        panic!("terminal provider failure reply expected")
    };
    assert_eq!(failed_id, Id::Text(request_id.into()));
    assert_eq!(err.code, nacre_protocol::ErrorCode::Internal);
    assert_eq!(err.message, "UI handler task failed");
    assert!(!err.retry);
    assert_eq!(err.dispatched, Some(true));
    assert_eq!(invocations.load(Ordering::SeqCst), 1);

    let ping_id = Id::Text("ping-after-handler-failure".into());
    framing::write(
        &mut writer,
        &Message::Ping {
            id: ping_id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(next(&mut reader).await, Message::Pong { id: ping_id });
    framing::write(&mut writer, &provider_call("after-failure"))
        .await
        .unwrap();
    assert_eq!(
        next(&mut reader).await,
        Message::Ret {
            id: Id::Text("after-failure".into()),
            reply: Reply::Ok { ok: Value::Null },
        }
    );
    assert_eq!(invocations.load(Ordering::SeqCst), 2);
    verified.send(()).unwrap();
    let _ = shutdown.await;
}

struct ProviderFailureHarness {
    client: Client,
    server: tokio::task::JoinHandle<()>,
    request_sent: Option<oneshot::Receiver<()>>,
    verified: oneshot::Receiver<()>,
    shutdown: Option<oneshot::Sender<()>>,
    _lab: Lab,
}

fn start_provider_failure_harness(
    request_id: &'static str,
    action: Action,
    invocations: Arc<std::sync::atomic::AtomicUsize>,
) -> ProviderFailureHarness {
    let lab = Lab::new();
    let listener = lab.bind();
    let (sent_tx, sent_rx) = oneshot::channel();
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(provider_failure_exchange(
        listener,
        request_id,
        invocations,
        sent_tx,
        verified_tx,
        shutdown_rx,
    ));
    let client = Client::start(provider_config(lab.socket(), action)).unwrap();
    ProviderFailureHarness {
        client,
        server,
        request_sent: Some(sent_rx),
        verified: verified_rx,
        shutdown: Some(shutdown_tx),
        _lab: lab,
    }
}

impl ProviderFailureHarness {
    async fn finish(self, invocations: &std::sync::atomic::AtomicUsize) {
        self.verified.await.unwrap();
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
        self.client.control(true).await.unwrap();
        let _ = self.shutdown.unwrap().send(());
        self.server.await.unwrap();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_factory_panic_returns_terminal_error_and_keeps_link() {
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invocations.clone();
    let action: Action = Arc::new(move |_, _| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("factory panic fixture");
        }
        Box::pin(async { Ok(Value::Null) })
    });
    let mut harness = start_provider_failure_harness("factory-panic", action, invocations.clone());
    harness.request_sent.take().unwrap().await.unwrap();
    harness.finish(&invocations).await;
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_future_panic_returns_terminal_error_and_keeps_link() {
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invocations.clone();
    let (started_tx, started_rx) = oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));
    let action: Action = Arc::new(move |_, _| {
        let first = count.fetch_add(1, Ordering::SeqCst) == 0;
        let started_tx = started_tx.clone();
        Box::pin(async move {
            if first {
                if let Some(tx) = started_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                panic!("future panic fixture");
            }
            Ok(Value::Null)
        })
    });
    let mut harness = start_provider_failure_harness("future-panic", action, invocations.clone());
    harness.request_sent.take().unwrap().await.unwrap();
    started_rx.await.unwrap();
    harness.finish(&invocations).await;
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_cancelled_task_returns_terminal_error_without_replay() {
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invocations.clone();
    let (started_tx, started_rx) = oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));
    let action: Action = Arc::new(move |_, _| {
        let first = count.fetch_add(1, Ordering::SeqCst) == 0;
        if first {
            let tx = started_tx.lock().unwrap().take().unwrap();
            Box::pin(async move {
                let _ = tx.send(());
                std::future::pending::<source::Result<Value>>().await
            })
        } else {
            Box::pin(async { Ok(Value::Null) })
        }
    });
    let mut harness = start_provider_failure_harness("cancelled-task", action, invocations.clone());
    harness.request_sent.take().unwrap().await.unwrap();
    started_rx.await.unwrap();
    let (abort_tx, abort_rx) = oneshot::channel();
    harness
        .client
        .0
        .tx
        .try_send(Command::AbortProviderTasks(abort_tx))
        .unwrap();
    abort_rx.await.unwrap();
    harness.finish(&invocations).await;
}

async fn accepted_with_boot(
    listener: &UnixListener,
    boot_id: &str,
) -> (
    framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    tokio::net::unix::OwnedWriteHalf,
) {
    let (stream, _) = listener.accept().await.unwrap();
    let (read, mut writer) = stream.into_split();
    let mut reader = framing::Reader::new(BufReader::new(read));
    assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
    let mut welcome = welcome();
    if let Message::Welcome { boot, .. } = &mut welcome {
        *boot = boot_id.into();
    }
    framing::write(&mut writer, &welcome).await.unwrap();
    (reader, writer)
}

struct DropNotify(Option<oneshot::Sender<()>>);
impl Drop for DropNotify {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_disconnect_discards_pending_task_metadata() {
    let lab = Lab::new();
    let listener = lab.bind();
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (started_tx, started_rx) = oneshot::channel();
    let started_tx = Arc::new(Mutex::new(Some(started_tx)));
    let (release_tx, release_rx) = oneshot::channel();
    let release_rx = Arc::new(Mutex::new(Some(release_rx)));
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let dropped_tx = Arc::new(Mutex::new(Some(dropped_tx)));
    let count = invocations.clone();
    let action: Action = Arc::new(move |_, _| {
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            let started = started_tx.lock().unwrap().take().unwrap();
            let release = release_rx.lock().unwrap().take().unwrap();
            let dropped = dropped_tx.lock().unwrap().take().unwrap();
            Box::pin(async move {
                let _drop_notify = DropNotify(Some(dropped));
                let _ = started.send(());
                let _ = release.await;
                Ok(Value::String("old-session-result".into()))
            })
        } else {
            Box::pin(async { Ok(Value::Null) })
        }
    });
    let (request_sent_tx, request_sent_rx) = oneshot::channel();
    let (disconnect_tx, disconnect_rx) = oneshot::channel();
    let (registered_tx, registered_rx) = oneshot::channel();
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut old_reader, mut old_writer) = accepted(&listener).await;
        assert_eq!(next(&mut old_reader).await, fixture("ui")[0]);
        framing::write(&mut old_writer, &provider_call("old-session-id"))
            .await
            .unwrap();
        request_sent_tx.send(()).unwrap();
        disconnect_rx.await.unwrap();
        drop(old_writer);
        drop(old_reader);

        let (mut reader, mut writer) = accepted_with_boot(&listener, "new-session-boot").await;
        assert_eq!(next(&mut reader).await, fixture("ui")[0]);
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
        registered_tx.send(()).unwrap();

        let ping_id = Id::Text("new-boot-ping".into());
        framing::write(
            &mut writer,
            &Message::Ping {
                id: ping_id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: ping_id });
        framing::write(&mut writer, &provider_call("new-session-id"))
            .await
            .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Ret {
                id: Id::Text("new-session-id".into()),
                reply: Reply::Ok { ok: Value::Null },
            }
        );
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
        verified_tx.send(()).unwrap();
        let _ = shutdown_rx.await;
    });
    let client = Client::start(provider_config(lab.socket(), action)).unwrap();
    request_sent_rx.await.unwrap();
    started_rx.await.unwrap();
    disconnect_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), dropped_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(release_tx.send(()).is_err());
    registered_rx.await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.as_deref() != Some("new-session-boot") {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(client.status().boot.as_deref(), Some("new-session-boot"));
    verified_rx.await.unwrap();
    client.control(true).await.unwrap();
    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

fn provider_slot_call(id: &str, slot: usize) -> Message {
    Message::Call {
        id: Id::Text(id.into()),
        m: "ui.dispatch".into(),
        p: serde_json::json!({"slot": slot}),
        timeout: Some(30_000),
    }
}

fn provider_error_ret(
    id: &str,
    code: nacre_protocol::ErrorCode,
    message: &str,
    retry: bool,
) -> Message {
    Message::Ret {
        id: Id::Text(id.into()),
        reply: Reply::Err {
            err: nacre_protocol::Failure::new(code, message, retry).dispatch_state(false),
        },
    }
}

type WaitingProvider = (
    Action,
    mpsc::UnboundedReceiver<usize>,
    Arc<std::sync::atomic::AtomicUsize>,
    Vec<Option<oneshot::Sender<()>>>,
);

fn waiting_provider(slots: usize) -> WaitingProvider {
    let (started_tx, started_rx) = mpsc::unbounded_channel();
    let mut release_receivers = BTreeMap::new();
    let mut release_senders = Vec::with_capacity(slots + 1);
    for slot in 0..=slots {
        let (release_tx, release_rx) = oneshot::channel();
        release_senders.push(Some(release_tx));
        release_receivers.insert(slot, release_rx);
    }
    let release_receivers = Arc::new(Mutex::new(release_receivers));
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let action_invocations = invocations.clone();
    let action_receivers = release_receivers.clone();
    let action: Action = Arc::new(move |_, params| {
        action_invocations.fetch_add(1, Ordering::SeqCst);
        let slot = params["slot"].as_u64().unwrap() as usize;
        let release = action_receivers.lock().unwrap().remove(&slot).unwrap();
        let started = started_tx.clone();
        Box::pin(async move {
            started.send(slot).unwrap();
            release.await.unwrap();
            Ok(Value::from(slot as u64))
        })
    });
    (action, started_rx, invocations, release_senders)
}

async fn fill_provider_pool(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    started: &mut mpsc::UnboundedReceiver<usize>,
    invocations: &std::sync::atomic::AtomicUsize,
) {
    let max_calls = nacre_protocol::MAX_CALLS;
    for slot in 0..max_calls {
        framing::write(writer, &provider_slot_call(&format!("slot-{slot}"), slot))
            .await
            .unwrap();
    }
    let mut started_slots = std::collections::BTreeSet::new();
    for _ in 0..max_calls {
        started_slots.insert(
            tokio::time::timeout(Duration::from_secs(5), started.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(started_slots, (0..max_calls).collect());
    assert_eq!(invocations.load(Ordering::SeqCst), max_calls);
}

async fn assert_provider_results_unordered(
    reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    expected_slots: impl IntoIterator<Item = usize>,
) {
    let expected: BTreeMap<_, _> = expected_slots
        .into_iter()
        .map(|slot| (format!("slot-{slot}"), Value::from(slot as u64)))
        .collect();
    let mut actual = BTreeMap::new();
    for _ in 0..expected.len() {
        let Message::Ret {
            id: Id::Text(id),
            reply: Reply::Ok { ok },
        } = next(reader).await
        else {
            panic!("expected successful provider reply");
        };
        assert!(actual.insert(id, ok).is_none(), "duplicate provider reply");
    }
    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_full_pool_returns_busy_without_dispatch() {
    let lab = Lab::new();
    let listener = lab.bind();
    let (action, mut started, invocations, mut releases) =
        waiting_provider(nacre_protocol::MAX_CALLS);
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accepted(&listener).await;
        assert_eq!(next(&mut reader).await, fixture("ui")[0]);
        fill_provider_pool(&mut writer, &mut started, &invocations).await;
        framing::write(
            &mut writer,
            &provider_slot_call("overload", nacre_protocol::MAX_CALLS),
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            provider_error_ret(
                "overload",
                nacre_protocol::ErrorCode::Busy,
                "UI handler request limit reached",
                true,
            )
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            nacre_protocol::MAX_CALLS
        );
        for release in releases.iter_mut().take(nacre_protocol::MAX_CALLS) {
            release.take().unwrap().send(()).unwrap();
        }
        assert_provider_results_unordered(&mut reader, 0..nacre_protocol::MAX_CALLS).await;
        verified_tx.send(()).unwrap();
        let _ = shutdown_rx.await;
    });
    let client = Client::start(provider_config(lab.socket(), action)).unwrap();
    verified_rx.await.unwrap();
    client.control(true).await.unwrap();
    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_unknown_method_is_not_dispatched_at_capacity() {
    let lab = Lab::new();
    let listener = lab.bind();
    let (action, mut started, invocations, mut releases) =
        waiting_provider(nacre_protocol::MAX_CALLS);
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accepted(&listener).await;
        assert_eq!(next(&mut reader).await, fixture("ui")[0]);
        fill_provider_pool(&mut writer, &mut started, &invocations).await;
        framing::write(
            &mut writer,
            &Message::Call {
                id: Id::Text("unknown-at-capacity".into()),
                m: "ui.not-registered".into(),
                p: serde_json::json!({}),
                timeout: Some(30_000),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            provider_error_ret(
                "unknown-at-capacity",
                nacre_protocol::ErrorCode::UnknownMethod,
                "UI method unavailable",
                false,
            )
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            nacre_protocol::MAX_CALLS
        );
        for release in releases.iter_mut().take(nacre_protocol::MAX_CALLS) {
            release.take().unwrap().send(()).unwrap();
        }
        assert_provider_results_unordered(&mut reader, 0..nacre_protocol::MAX_CALLS).await;
        verified_tx.send(()).unwrap();
        let _ = shutdown_rx.await;
    });
    let client = Client::start(provider_config(lab.socket(), action)).unwrap();
    verified_rx.await.unwrap();
    client.control(true).await.unwrap();
    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_completion_frees_one_admission_slot() {
    let lab = Lab::new();
    let listener = lab.bind();
    let (action, mut started, invocations, mut releases) =
        waiting_provider(nacre_protocol::MAX_CALLS);
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accepted(&listener).await;
        assert_eq!(next(&mut reader).await, fixture("ui")[0]);
        fill_provider_pool(&mut writer, &mut started, &invocations).await;
        framing::write(
            &mut writer,
            &provider_slot_call("rejected", nacre_protocol::MAX_CALLS),
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            provider_error_ret(
                "rejected",
                nacre_protocol::ErrorCode::Busy,
                "UI handler request limit reached",
                true,
            )
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            nacre_protocol::MAX_CALLS
        );

        releases[0].take().unwrap().send(()).unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Ret {
                id: Id::Text("slot-0".into()),
                reply: Reply::Ok {
                    ok: Value::from(0u64),
                },
            }
        );
        framing::write(
            &mut writer,
            &provider_slot_call("admitted-after-completion", nacre_protocol::MAX_CALLS),
        )
        .await
        .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), started.recv())
                .await
                .unwrap()
                .unwrap(),
            nacre_protocol::MAX_CALLS
        );
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            nacre_protocol::MAX_CALLS + 1
        );
        releases[nacre_protocol::MAX_CALLS]
            .take()
            .unwrap()
            .send(())
            .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Ret {
                id: Id::Text("admitted-after-completion".into()),
                reply: Reply::Ok {
                    ok: Value::from(nacre_protocol::MAX_CALLS as u64),
                },
            }
        );
        for release in releases.iter_mut().take(nacre_protocol::MAX_CALLS).skip(1) {
            release.take().unwrap().send(()).unwrap();
        }
        assert_provider_results_unordered(&mut reader, 1..nacre_protocol::MAX_CALLS).await;
        let ping_id = Id::Text("after-admission".into());
        framing::write(
            &mut writer,
            &Message::Ping {
                id: ping_id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: ping_id });
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            nacre_protocol::MAX_CALLS + 1
        );
        verified_tx.send(()).unwrap();
        let _ = shutdown_rx.await;
    });
    let client = Client::start(provider_config(lab.socket(), action)).unwrap();
    verified_rx.await.unwrap();
    client.control(true).await.unwrap();
    let _ = shutdown_tx.send(());
    server.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn ui_provider_missing_provider_returns_unknown_method_undispatched() {
    let lab = Lab::new();
    let listener = lab.bind();
    let (verified_tx, verified_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accepted(&listener).await;
        framing::write(&mut writer, &provider_call("no-provider"))
            .await
            .unwrap();
        assert_eq!(
            next(&mut reader).await,
            provider_error_ret(
                "no-provider",
                nacre_protocol::ErrorCode::UnknownMethod,
                "UI method unavailable",
                false,
            )
        );
        let ping_id = Id::Text("missing-provider-link".into());
        framing::write(
            &mut writer,
            &Message::Ping {
                id: ping_id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: ping_id });
        verified_tx.send(()).unwrap();
        let _ = shutdown_rx.await;
    });
    let client = Client::start(Config::new(lab.socket(), vec![])).unwrap();
    verified_rx.await.unwrap();
    client.control(true).await.unwrap();
    let _ = shutdown_tx.send(());
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

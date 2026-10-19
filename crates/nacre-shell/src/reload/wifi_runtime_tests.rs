// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Id, Message, Reply, framing};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::BufReader,
    net::UnixListener,
    sync::{mpsc, oneshot},
};

const WIFI_SWITCH_ID: &str = "status-popout.wifi-row.wifi-radio";

struct Lab(PathBuf);
impl Lab {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wifi-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn bind(&self) -> UnixListener {
        let socket = self.0.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        listener
    }
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn network(enabled: bool) -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "wireless_enabled":enabled,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
        "connections":{"items":{"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wifi-connection"]},
        "primary_connection":"wifi-connection"
    })
}
fn welcome(method: Option<(&str, u16)>, network_topic: Option<u16>) -> Message {
    let mut methods = nacre_protocol::core_methods();
    if let Some((name, version)) = method {
        methods.insert(
            name.into(),
            nacre_protocol::Method {
                v: version,
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
        topics: network_topic
            .map(|version| BTreeMap::from([("network".into(), version)]))
            .unwrap_or_default(),
        methods,
        frame_ms: None,
    }
}
async fn next(reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Message {
    tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
fn switch_center(runtime: &Runtime) -> (f32, f32) {
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id == WIFI_SWITCH_ID)
        .unwrap();
    let rect = bead::ui::rect(&node.draw);
    (rect.x + rect.w / 2., rect.y + rect.h / 2.)
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_sends_enabled_once_and_ignores_busy_duplicate()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (first_call_tx, mut first_call_rx) = mpsc::channel(2);
    let (release_ret_tx, release_ret_rx) = oneshot::channel();
    let (allow_snap_tx, allow_snap_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &welcome(Some(("network.set_wifi", 1)), Some(1)),
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["network".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: network(true),
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("Wi-Fi call expected")
        };
        first_call_tx.send((m, p)).await.unwrap();
        release_ret_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":false}),
                },
            },
        )
        .await
        .unwrap();
        allow_snap_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 1,
                data: network(false),
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("second deliberate Wi-Fi call expected")
        };
        first_call_tx.send((m, p)).await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":true}),
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["network".into()]
            }
        );
        let _ = done_tx.send(());
        let _ = shutdown_rx.await;
    });

    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while client.status().boot.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let leased = Arc::new(client.lease_topic("network")?);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !leased.snapshot().connected {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette_path = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette_path)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session = bead::ui::Session::new(
        root,
        "status/network.luau".into(),
        palette_path,
        colors,
        cold,
    );
    let mut runtime = Runtime::new(session).with_core_network_control(leased, client.clone())?;
    isolate_runtime(&mut runtime, &lab.0.join("runtime-recovery"));
    runtime.stage(UiEvent::Reload)?;
    runtime.commit();
    let (event_tx, mut event_rx) = mpsc::channel(8);
    runtime.services(Box::new(move |event| {
        let _ = event_tx.try_send(event);
    }));
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    while event_rx.try_recv().is_ok() {}
    assert_eq!(
        runtime.session.stores.get("network.wifi_enabled"),
        Some(&bead::core::Value::Bool(true))
    );
    let (x, y) = switch_center(&runtime);
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("network.wifi_busy"),
        Some(&bead::core::Value::Bool(true))
    );
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first_call_rx.recv())
            .await?
            .unwrap(),
        (
            "network.set_wifi".into(),
            serde_json::json!({"enabled":false})
        )
    );
    release_ret_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
        .await?
        .expect("missing Ret wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("network.wifi_busy"),
        Some(&bead::core::Value::Bool(false))
    );
    assert_eq!(
        runtime.session.stores.get("network.wifi_checked"),
        Some(&bead::core::Value::Bool(true)),
        "Ret is acknowledgement; Snap owns checked state"
    );
    allow_snap_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
        .await?
        .expect("missing Snap wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("network.wifi_checked"),
        Some(&bead::core::Value::Bool(false))
    );
    runtime.stage(UiEvent::Command {
        command: "tab".into(),
        shift: false,
    })?;
    runtime.commit();
    runtime.stage(UiEvent::Command {
        command: "enter".into(),
        shift: false,
    })?;
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), first_call_rx.recv())
            .await?
            .unwrap(),
        (
            "network.set_wifi".into(),
            serde_json::json!({"enabled":true})
        ),
        "deliberate click re-reads authoritative state"
    );
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv())
        .await?
        .expect("missing second Ret wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("network.wifi_busy"),
        Some(&bead::core::Value::Bool(false))
    );
    assert_eq!(
        runtime.session.stores.get("network.wifi_checked"),
        Some(&bead::core::Value::Bool(false)),
        "changed=true Ret is acknowledgement; only a later Snap changes checked state"
    );
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_requires_exact_capability_and_fresh_topic()
-> Result<(), Box<dyn std::error::Error>> {
    type Case = (&'static str, Option<(&'static str, u16)>, Option<u16>, u8);
    let cases: [Case; 9] = [
        ("method absent", None, Some(1), 0),
        (
            "misleading name",
            Some(("network.set_wifi_extra", 1)),
            Some(1),
            0,
        ),
        ("method v2", Some(("network.set_wifi", 2)), Some(1), 0),
        ("stale state", Some(("network.set_wifi", 1)), Some(1), 1),
        ("invalid state", Some(("network.set_wifi", 1)), Some(1), 2),
        ("topic v2", Some(("network.set_wifi", 1)), Some(2), 0),
        ("no topic", Some(("network.set_wifi", 1)), None, 0),
        (
            "source epoch mismatch",
            Some(("network.set_wifi", 1)),
            Some(1),
            3,
        ),
        (
            "disconnected source",
            Some(("network.set_wifi", 1)),
            Some(1),
            4,
        ),
    ];
    for (label, method, topic_version, state_case) in cases {
        let lab = Lab::new();
        let listener = lab.bind();
        let (done_tx, mut done_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
            framing::write(&mut writer, &welcome(method, topic_version))
                .await
                .unwrap();
            if topic_version == Some(1) && state_case < 3 {
                assert_eq!(
                    next(&mut reader).await,
                    Message::Sub {
                        topics: vec!["network".into()]
                    }
                );
                let mut value = network(true);
                if state_case == 1 {
                    value["health"]["stale"] = serde_json::json!(true);
                }
                if state_case == 2 {
                    value["wireless_enabled"] = serde_json::Value::Null;
                }
                framing::write(
                    &mut writer,
                    &Message::Snap {
                        topic: "network".into(),
                        seq: 0,
                        data: value,
                    },
                )
                .await
                .unwrap();
            }
            loop {
                tokio::select! {
                    _ = &mut done_rx => break,
                    incoming = reader.next() => match incoming {
                        Ok(Some(Message::Call { id, m, p, .. })) => {
                            assert_eq!(m, "daemon.ping", "{label} must never emit a Wi-Fi call");
                            assert_eq!(p, serde_json::json!({"fence":"unavailable-wifi"}));
                            framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok: serde_json::json!({"fenced":true}) } }).await.unwrap();
                        },
                        Ok(Some(_)) => {},
                        _ => break,
                    }
                }
            }
            let _ = shutdown_rx.await;
        });
        let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while client.status().boot.is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        let mut value = network(true);
        if state_case == 1 {
            value["health"]["stale"] = serde_json::json!(true);
        }
        if state_case == 2 {
            value["wireless_enabled"] = serde_json::Value::Null;
        }
        let source: Arc<dyn Source> = match state_case {
            3 => Arc::new(bead_test::source::FakeSource::new(value)?),
            4 => {
                let fake = bead_test::source::FakeSource::new(value)?;
                fake.disconnect()?;
                Arc::new(fake)
            }
            _ => Arc::new(client.lease_topic("network")?),
        };
        if topic_version == Some(1) && state_case < 3 {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !source.snapshot().connected {
                    tokio::task::yield_now().await;
                }
            })
            .await?;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette_path = root.join("palette.json");
        let colors = bead::render::Palette::read(&palette_path)?;
        let cold = crate::sound::fallback(colors.get("surface")?);
        let session = bead::ui::Session::new(
            root,
            "status/network.luau".into(),
            palette_path,
            colors,
            cold,
        );
        let mut runtime =
            Runtime::new(session).with_core_network_control(source, client.clone())?;
        isolate_runtime(&mut runtime, &lab.0.join("runtime-recovery"));
        let scene = runtime
            .stage(UiEvent::Reload)?
            .expect("controlled template must load");
        let rect = switch_rect(&scene);
        runtime.commit();
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("network.wifi_enabled"),
            Some(&bead::core::Value::Bool(false)),
            "{label}"
        );
        assert!(
            runtime
                .session
                .view_nodes()
                .iter()
                .all(|node| node.id != WIFI_SWITCH_ID),
            "{label}: unavailable control exposes no hit action"
        );
        runtime.stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })?;
        runtime.commit();
        // This ordinary call follows any accidental action command in the
        // existing actor's FIFO. Its Ret proves the peer consumed all earlier
        // wire Calls before teardown can win the done-channel select.
        assert_eq!(
            client
                .call(
                    "daemon.ping".into(),
                    serde_json::json!({"fence":"unavailable-wifi"}),
                    Duration::from_secs(2)
                )
                .await?,
            serde_json::json!({"fenced":true}),
            "{label}"
        );
        drop(runtime);
        let _ = done_tx.send(());
        Source::stop(&client).await?;
        let _ = shutdown_tx.send(());
        server.await.unwrap();
    }
    Ok(())
}

fn switch_rect(scene: &bead::render::Scene) -> bead::render::Rect {
    assert!(
        scene.primitives.iter().any(|primitive| matches!(primitive,
            bead::render::Primitive::Text { text, .. } if text == "Wi-Fi"
        )),
        "controlled template renders the Wi-Fi label"
    );
    scene
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Rounded { rect, .. }
                if (rect.w - 48.).abs() < 0.1 && (rect.h - 28.).abs() < 0.1 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .expect("controlled template renders the switch geometry")
}

fn isolate_runtime(runtime: &mut Runtime, recovery: &Path) {
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.session.enable_recovery(recovery.into());
}

fn controlled_runtime(
    client: &Client,
    recovery: &Path,
) -> Result<Runtime, Box<dyn std::error::Error>> {
    let palette = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = bead::render::Palette::read(&palette)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session = bead::ui::Session::new(
        palette.parent().unwrap().into(),
        "status/network.luau".into(),
        palette,
        colors,
        cold,
    );
    let mut runtime = Runtime::new(session)
        .with_core_network_control(Arc::new(client.lease_topic("network")?), client.clone())?;
    isolate_runtime(&mut runtime, recovery);
    runtime.services(Box::new(|_| {}));
    let scene = runtime
        .stage(UiEvent::Reload)?
        .expect("controlled template must load");
    switch_rect(&scene);
    runtime.commit();
    assert!(
        runtime.session.stores.get("network.wifi_message").is_some(),
        "controlled constructor supplies host store schema"
    );
    Ok(runtime)
}
fn project(runtime: &mut Runtime) {
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
}
fn flag(runtime: &Runtime, name: &str) -> bool {
    runtime.session.stores.get(name) == Some(&bead::core::Value::Bool(true))
}
async fn until(runtime: &mut Runtime, predicate: impl Fn(&Runtime) -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(runtime);
            if predicate(runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn click(runtime: &mut Runtime) {
    let (x, y) = switch_center(runtime);
    runtime.stage(UiEvent::Click { x, y }).unwrap();
    runtime.commit();
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_failures_disconnect_never_replay() -> Result<(), Box<dyn std::error::Error>> {
    // Each failure terminates immediately without a Snap. Ping/Pong fences all
    // preceding UI events, so an accidental retry cannot hide in a queue.
    for outcome in [
        "policy",
        "unavailable",
        "timeout",
        "malformed",
        "disconnect",
        "new-boot",
    ] {
        let lab = Lab::new();
        let listener = lab.bind();
        let (seen_tx, seen_rx) = oneshot::channel();
        let (finish_tx, finish_rx) = oneshot::channel();
        let (probe_tx, probe_rx) = oneshot::channel();
        let (snap_tx, snap_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
            let mut initial = welcome(Some(("network.set_wifi", 1)), Some(1));
            if let Message::Welcome { methods, .. } = &mut initial {
                methods.get_mut("network.set_wifi").unwrap().timeout = 100;
            }
            framing::write(&mut writer, &initial).await.unwrap();
            assert!(matches!(next(&mut reader).await, Message::Sub { .. }));
            framing::write(
                &mut writer,
                &Message::Snap {
                    topic: "network".into(),
                    seq: 0,
                    data: network(true),
                },
            )
            .await
            .unwrap();
            let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                panic!("first deliberate call expected")
            };
            assert_eq!(
                (m.as_str(), p),
                ("network.set_wifi", serde_json::json!({"enabled":false}))
            );
            seen_tx.send(()).unwrap();
            finish_rx.await.unwrap();
            if outcome == "disconnect" || outcome == "new-boot" {
                drop(reader);
                drop(writer);
                let (stream, _) = listener.accept().await.unwrap();
                let (read, write) = stream.into_split();
                reader = framing::Reader::new(BufReader::new(read));
                writer = write;
                assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
                let mut replacement = welcome(Some(("network.set_wifi", 1)), Some(1));
                if outcome == "new-boot"
                    && let Message::Welcome { boot, .. } = &mut replacement
                {
                    *boot = "wifi-boot-b".into();
                }
                framing::write(&mut writer, &replacement).await.unwrap();
                assert_eq!(
                    next(&mut reader).await,
                    Message::Sub {
                        topics: vec!["network".into()]
                    }
                );
            } else if outcome != "timeout" {
                let reply = match outcome {
                    "malformed" => Reply::Ok {
                        ok: serde_json::json!({"changed":"yes"}),
                    },
                    _ => Reply::Err {
                        err: nacre_protocol::Failure::new(
                            if outcome == "policy" {
                                nacre_protocol::ErrorCode::PermissionDenied
                            } else {
                                nacre_protocol::ErrorCode::Unavailable
                            },
                            "private error must not reach UI",
                            false,
                        ),
                    },
                };
                framing::write(&mut writer, &Message::Ret { id, reply })
                    .await
                    .unwrap();
            }
            probe_rx.await.unwrap();
            framing::write(
                &mut writer,
                &Message::Ping {
                    id: Id::Text("no-replay".into()),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                next(&mut reader).await,
                Message::Pong {
                    id: Id::Text("no-replay".into())
                }
            );
            snap_rx.await.unwrap();
            framing::write(
                &mut writer,
                &Message::Snap {
                    topic: "network".into(),
                    seq: 1,
                    data: network(false),
                },
            )
            .await
            .unwrap();
            let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                panic!("later deliberate call expected")
            };
            assert_eq!(
                (m.as_str(), p),
                ("network.set_wifi", serde_json::json!({"enabled":true}))
            );
            framing::write(
                &mut writer,
                &Message::Ret {
                    id,
                    reply: Reply::Ok {
                        ok: serde_json::json!({"changed":false}),
                    },
                },
            )
            .await
            .unwrap();
            assert_eq!(
                next(&mut reader).await,
                Message::Unsub {
                    topics: vec!["network".into()]
                }
            );
            let _ = done_tx.send(());
            let _ = shutdown_rx.await;
        });
        let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
        let mut runtime = controlled_runtime(&client, &lab.0.join("runtime-recovery"))?;
        until(&mut runtime, |r| flag(r, "network.wifi_enabled")).await;
        click(&mut runtime);
        tokio::time::timeout(Duration::from_secs(5), seen_rx).await??;
        finish_tx.send(()).unwrap();
        until(&mut runtime, |r| !flag(r, "network.wifi_busy")).await;
        if outcome != "disconnect" && outcome != "new-boot" {
            assert_eq!(
                runtime.session.stores.get("network.wifi_message"),
                Some(&bead::core::Value::Text("Wi-Fi change failed".into())),
                "{outcome}"
            );
            project(&mut runtime);
            assert_eq!(
                runtime.session.stores.get("network.wifi_message"),
                Some(&bead::core::Value::Text("Wi-Fi change failed".into()))
            );
        }
        probe_tx.send(()).unwrap();
        snap_tx.send(()).unwrap();
        until(&mut runtime, |r| {
            flag(r, "network.wifi_enabled") && !flag(r, "network.wifi_checked")
        })
        .await;
        click(&mut runtime);
        until(&mut runtime, |r| !flag(r, "network.wifi_busy")).await;
        assert_eq!(
            runtime.session.stores.get("network.wifi_message"),
            Some(&bead::core::Value::Text(String::new()))
        );
        drop(runtime);
        tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
        Source::stop(&client).await?;
        let _ = shutdown_tx.send(());
        server.await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_retire_reopen_isolates_old_reply() -> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (old_seen_tx, old_seen_rx) = oneshot::channel();
    let (retired_tx, retired_rx) = oneshot::channel();
    let (old_ret_tx, old_ret_rx) = oneshot::channel();
    let (fenced_tx, fenced_rx) = oneshot::channel();
    let (new_ret_tx, new_ret_rx) = oneshot::channel();
    let (snap_tx, snap_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &welcome(Some(("network.set_wifi", 1)), Some(1)),
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["network".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: network(true),
            },
        )
        .await
        .unwrap();
        let Message::Call { id: old_id, .. } = next(&mut reader).await else {
            panic!("old call expected")
        };
        old_seen_tx.send(()).unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["network".into()]
            }
        );
        retired_tx.send(()).unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["network".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: network(false),
            },
        )
        .await
        .unwrap();
        let Message::Call { id: new_id, p, .. } = next(&mut reader).await else {
            panic!("replacement call expected")
        };
        assert_eq!(p, serde_json::json!({"enabled":true}));
        old_ret_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id: old_id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":true}),
                },
            },
        )
        .await
        .unwrap();
        framing::write(
            &mut writer,
            &Message::Ping {
                id: Id::Text("old-reply-fence".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Pong {
                id: Id::Text("old-reply-fence".into())
            }
        );
        fenced_tx.send(()).unwrap();
        new_ret_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id: new_id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":null}),
                },
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
                data: network(true),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["network".into()]
            }
        );
        let _ = done_tx.send(());
        let _ = shutdown_rx.await;
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    let mut old = controlled_runtime(&client, &lab.0.join("old-recovery"))?;
    let old_controller = std::rc::Rc::downgrade(old.status.as_ref().unwrap());
    let callback_owner = Arc::new(());
    let callback_weak = Arc::downgrade(&callback_owner);
    let (old_event_tx, mut old_event_rx) = mpsc::unbounded_channel();
    old.services(Box::new(move |event| {
        let _keep_alive = &callback_owner;
        let _ = old_event_tx.send(event);
    }));
    until(&mut old, |r| flag(r, "network.wifi_enabled")).await;
    click(&mut old);
    tokio::time::timeout(Duration::from_secs(5), old_seen_rx).await??;
    while old_event_rx.try_recv().is_ok() {}
    drop(old);
    assert!(
        old_controller.upgrade().is_none(),
        "retirement releases controller and ticket"
    );
    tokio::time::timeout(Duration::from_secs(5), retired_rx).await??;
    let mut replacement = controlled_runtime(&client, &lab.0.join("replacement-recovery"))?;
    until(&mut replacement, |r| flag(r, "network.wifi_enabled")).await;
    assert!(!flag(&replacement, "network.wifi_checked"));
    click(&mut replacement);
    assert!(flag(&replacement, "network.wifi_busy"));
    old_ret_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), fenced_rx).await??;
    tokio::time::timeout(Duration::from_secs(5), old_event_rx.recv())
        .await?
        .expect("old completion wakes only old channel");
    assert!(
        callback_weak.upgrade().is_none(),
        "Ret releases abandoned completion callback"
    );
    project(&mut replacement);
    assert!(
        flag(&replacement, "network.wifi_busy"),
        "old Ret cannot complete new ticket"
    );
    assert!(!flag(&replacement, "network.wifi_checked"));
    new_ret_tx.send(()).unwrap();
    until(&mut replacement, |r| !flag(r, "network.wifi_busy")).await;
    assert_eq!(
        replacement.session.stores.get("network.wifi_message"),
        Some(&bead::core::Value::Text("Wi-Fi change failed".into()))
    );
    snap_tx.send(()).unwrap();
    until(&mut replacement, |r| flag(r, "network.wifi_checked")).await;
    assert_eq!(
        replacement.session.stores.get("network.wifi_message"),
        Some(&bead::core::Value::Text("Wi-Fi change failed".into())),
        "new Snap preserves new controller error"
    );
    drop(replacement);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_popout_factory_is_lazy_and_routes_only_network_control()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (probe_tx, probe_rx) = oneshot::channel();
    let (probed_tx, probed_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (called_tx, called_rx) = oneshot::channel();
    let (ignored_tx, ignored_rx) = oneshot::channel();
    let (ignored_probed_tx, ignored_probed_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &welcome(Some(("network.set_wifi", 1)), Some(1)),
        )
        .await
        .unwrap();
        probe_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ping {
                id: Id::Text("cold-factory".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Pong {
                id: Id::Text("cold-factory".into())
            },
            "constructing factories must not lease or call"
        );
        probed_tx.send(()).unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["network".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: network(true),
            },
        )
        .await
        .unwrap();
        ignored_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ping {
                id: Id::Text("ignored-input-fence".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Pong {
                id: Id::Text("ignored-input-fence".into())
            },
            "context/scroll/drag must not dispatch control"
        );
        ignored_probed_tx.send(()).unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("factory action must reach shared client")
        };
        assert_eq!(
            (m.as_str(), p),
            ("network.set_wifi", serde_json::json!({"enabled":false}))
        );
        called_tx.send(()).unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":false}),
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["network".into()]
            }
        );
        let _ = done_tx.send(());
        let _ = shutdown_rx.await;
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    let palette =
        Arc::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"));
    let colors = Arc::new(bead::render::Palette::read(&palette)?);
    let owner = Arc::new(std::sync::OnceLock::new());
    let root = palette.parent().unwrap().to_path_buf();
    let recovery_root = lab.0.join("factory-recovery");
    let next_runtime = std::sync::atomic::AtomicU64::new(0);
    let initialize: crate::popouts::StatusInit = Arc::new(move |runtime| {
        let generation = next_runtime.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        isolate_runtime(runtime, &recovery_root.join(generation.to_string()));
    });
    let spec = crate::popouts::status_at_root(
        crate::status_views::Kind::Network,
        palette.clone(),
        colors.clone(),
        owner.clone(),
        Some(client.clone()),
        (12., false),
        (root.clone(), Some(initialize.clone())),
    )?;
    let bluetooth = crate::popouts::status_at_root(
        crate::status_views::Kind::Bluetooth,
        palette.clone(),
        colors.clone(),
        owner.clone(),
        Some(client.clone()),
        (12., false),
        (root.clone(), Some(initialize.clone())),
    )?;
    let battery = crate::popouts::status_at_root(
        crate::status_views::Kind::Battery,
        palette.clone(),
        colors.clone(),
        owner.clone(),
        Some(client.clone()),
        (12., false),
        (root.clone(), Some(initialize.clone())),
    )?;
    let legacy = crate::popouts::status_at_root(
        crate::status_views::Kind::Network,
        palette,
        colors,
        owner,
        None,
        (12., false),
        (root.clone(), Some(initialize.clone())),
    )?;
    assert!(
        !spec.initial_open
            && !bluetooth.initial_open
            && !battery.initial_open
            && !legacy.initial_open
    );
    probe_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), probed_rx).await??;
    for other in [&bluetooth, &battery] {
        let mut read_only = (other.factory)(true).map_err(std::io::Error::other)?;
        read_only.stage(UiEvent::Reload)?;
        read_only.commit();
        assert!(
            read_only
                .input_regions()
                .iter()
                .all(|region| region.id != WIFI_SWITCH_ID),
            "Bluetooth does not expose the Network switch"
        );
        if std::ptr::eq(other, &bluetooth) {
            assert_eq!(
                read_only.session.stores.get("bluetooth.power_enabled"),
                Some(&bead::core::Value::Bool(false)),
                "a Bluetooth factory with the shared Client selects its capability-gated control template"
            );
        }
    }
    assert!(
        (legacy.factory)(true).is_err(),
        "legacy factory still requires hosted Desktop services"
    );
    let mut runtime = (spec.factory)(true).map_err(std::io::Error::other)?;
    runtime.services(Box::new(|_| {}));
    runtime.stage(UiEvent::Reload)?;
    runtime.commit();
    let rect = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
            runtime.commit();
            if let Some(region) = runtime
                .input_regions()
                .into_iter()
                .find(|region| region.id == WIFI_SWITCH_ID)
            {
                break region.rect;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
    for event in [
        UiEvent::ContextMenu { x, y },
        UiEvent::Scroll { x, y, delta: 1. },
        UiEvent::Drag { x, y },
    ] {
        runtime.stage(event)?;
        runtime.commit();
    }
    ignored_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), ignored_probed_rx).await??;
    runtime.stage(UiEvent::Click {
        x: rect.x + rect.w / 2.,
        y: rect.y + rect.h / 2.,
    })?;
    runtime.commit();
    tokio::time::timeout(Duration::from_secs(5), called_rx).await??;
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Message, Reply, framing};
use std::{
    collections::BTreeMap, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration,
};
use tokio::{
    io::BufReader,
    net::UnixListener,
    sync::{mpsc, oneshot},
};

struct Lab(PathBuf);
impl Lab {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bluetooth-power-{}-{}",
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
fn state(powered: bool) -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "adapters":{"items":{"adapter-A":{"name":"Office adapter","powered":powered,"discovering":false}},"order":["adapter-A"]},
        "devices":{"items":{},"order":[]}
    })
}
async fn next(reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Message {
    tokio::time::timeout(Duration::from_secs(5), reader.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
fn methods(version: Option<u16>) -> BTreeMap<String, nacre_protocol::Method> {
    let mut methods = nacre_protocol::core_methods();
    if let Some(version) = version {
        methods.insert(
            "bluetooth.set_power".into(),
            nacre_protocol::Method {
                v: version,
                class: Capability::System,
                timeout: 10_000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
    }
    methods
}
async fn assert_disabled_case(
    data: serde_json::Value,
    method_version: Option<u16>,
    disconnect_after_snap: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (fenced_tx, fenced_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-a".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(method_version),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data,
            },
        )
        .await
        .unwrap();
        let fence = nacre_protocol::Id::Text("disabled-snapshot-fence".into());
        framing::write(&mut writer, &Message::Ping { id: fence.clone() })
            .await
            .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: fence });
        let _ = fenced_tx.send(());
        if disconnect_after_snap {
            drop(writer);
            let _ = done_tx.send(());
            let _ = shutdown_rx.await;
            return;
        }
        loop {
            match next(&mut reader).await {
                Message::Unsub { topics } => {
                    assert_eq!(topics, vec!["bluetooth"]);
                    break;
                }
                Message::Call { .. } => panic!("ineligible Bluetooth control sent a call"),
                Message::Ping { .. } | Message::Pong { .. } => {}
                message => panic!("unexpected daemon message {message:?}"),
            }
        }
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut runtime = controlled_runtime(&client, root, lab.0.join("recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), fenced_rx).await??;
    if disconnect_after_snap {
        tokio::time::timeout(Duration::from_secs(5), async {
            while client.snapshot().connected {
                tokio::task::yield_now().await;
            }
        })
        .await?;
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if runtime.session.stores.get("status.secondary")
                == Some(&bead::core::Value::Text("Bluetooth on".into()))
                || runtime.session.stores.get("status.secondary")
                    == Some(&bead::core::Value::Text("Bluetooth off".into()))
                || runtime.session.stores.get("status.secondary")
                    == Some(&bead::core::Value::Text("Status data unavailable".into()))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        !runtime
            .session
            .stores
            .get("bluetooth.power_enabled")
            .is_some_and(|v| *v == bead::core::Value::Bool(true))
    );
    runtime
        .status
        .as_ref()
        .unwrap()
        .bluetooth_action(Arc::new(|| {}));
    project(&mut runtime);
    assert!(!busy(&runtime));
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}
fn controlled_runtime(
    client: &Client,
    root: PathBuf,
    recovery: PathBuf,
) -> Result<Runtime, Box<dyn std::error::Error>> {
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
    let scene = runtime
        .stage(UiEvent::Reload)?
        .expect("controlled template loads");
    assert!(scene.primitives.iter().any(|primitive| matches!(primitive, bead::render::Primitive::Rounded { rect, .. } if (rect.w - 48.).abs() < 0.1 && (rect.h - 28.).abs() < 0.1)));
    runtime.commit();
    runtime.services(Box::new(|_| {}));
    Ok(runtime)
}
fn project(runtime: &mut Runtime) {
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
}
fn checked(runtime: &Runtime) -> bool {
    runtime.session.stores.get("bluetooth.power_checked") == Some(&bead::core::Value::Bool(true))
}
fn busy(runtime: &Runtime) -> bool {
    runtime.session.stores.get("bluetooth.power_busy") == Some(&bead::core::Value::Bool(true))
}
fn click(runtime: &mut Runtime) {
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id == "status-popout.bluetooth-row.bluetooth-power")
        .unwrap();
    let rect = bead::ui::rect(&node.draw);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_targets_exact_single_adapter() -> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (call_tx, mut call_rx) = mpsc::channel(2);
    let (reply_tx, reply_rx) = oneshot::channel();
    let (snapshot_tx, snapshot_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        let mut methods = nacre_protocol::core_methods();
        methods.insert(
            "bluetooth.set_power".into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 10_000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-a".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("Bluetooth power call expected")
        };
        call_tx.send((m, p)).await.unwrap();
        reply_rx.await.unwrap();
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
        snapshot_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 1,
                data: state(false),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["bluetooth".into()]
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut runtime = controlled_runtime(&client, root, lab.0.join("recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(checked(&runtime));
    click(&mut runtime);
    assert!(busy(&runtime));
    click(&mut runtime);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), call_rx.recv())
            .await?
            .unwrap(),
        (
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter":"adapter-A","powered":false})
        )
    );
    assert!(
        call_rx.try_recv().is_err(),
        "busy click does not enqueue another call"
    );
    reply_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&runtime),
        "Ret changed:false is an acknowledgement, not readback"
    );
    snapshot_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !checked(&runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_requires_fresh_unambiguous_adapter()
-> Result<(), Box<dyn std::error::Error>> {
    let mut zero = state(true);
    zero["adapters"] = serde_json::json!({"items":{},"order":[]});
    let mut multiple = state(true);
    multiple["adapters"] = serde_json::json!({"items":{
        "adapter-A":{"name":"A","powered":true,"discovering":false},
        "adapter-B":{"name":"B","powered":false,"discovering":false}
    },"order":["adapter-A","adapter-B"]});
    let mut dangling = state(true);
    dangling["adapters"]["order"] = serde_json::json!(["missing"]);
    let mut stale = state(true);
    stale["health"]["stale"] = serde_json::json!(true);
    let mut unhealthy = state(true);
    unhealthy["health"]["available"] = serde_json::json!(false);
    let mut malformed = state(true);
    malformed["adapters"]["items"]["adapter-A"]["powered"] = serde_json::json!("unknown");
    for data in [zero, multiple, dangling, malformed, stale, unhealthy] {
        assert_disabled_case(data, Some(1), false).await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_unadvertised_method_stays_read_only()
-> Result<(), Box<dyn std::error::Error>> {
    for version in [None, Some(2)] {
        assert_disabled_case(state(true), version, false).await?;
    }
    assert_disabled_case(state(true), Some(1), true).await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_lifetime_never_replays() -> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (replacement_tx, replacement_rx) = oneshot::channel();
    let (release_old_tx, release_old_rx) = oneshot::channel();
    let (old_processed_tx, old_processed_rx) = oneshot::channel();
    let (second_call_tx, second_call_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-a".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(Some(1)),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let Message::Call {
            id: old_id, m, p, ..
        } = next(&mut reader).await
        else {
            panic!("first call expected")
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-A","powered":false})
            )
        );
        let replacement = serde_json::json!({
            "health":{"available":true,"stale":false},
            "adapters":{"items":{"adapter-B":{"name":"Replacement","powered":false,"discovering":false}},"order":["adapter-B"]},
            "devices":{"items":{},"order":[]}
        });
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 1,
                data: replacement,
            },
        )
        .await
        .unwrap();
        let _ = replacement_tx.send(());
        release_old_rx.await.unwrap();
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
        let ping_id = nacre_protocol::Id::Text("late-reply-fence".into());
        framing::write(
            &mut writer,
            &Message::Ping {
                id: ping_id.clone(),
            },
        )
        .await
        .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: ping_id });
        let _ = old_processed_tx.send(());
        let Message::Call {
            id: new_id, m, p, ..
        } = next(&mut reader).await
        else {
            panic!("deliberate replacement-target call expected")
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-B","powered":true})
            )
        );
        let _ = second_call_tx.send(());
        framing::write(
            &mut writer,
            &Message::Ret {
                id: new_id,
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
                topics: vec!["bluetooth".into()]
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut runtime = controlled_runtime(&client, root, lab.0.join("recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    click(&mut runtime);
    assert!(busy(&runtime));
    tokio::time::timeout(Duration::from_secs(5), replacement_rx).await??;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(!checked(&runtime), "replacement topic owns displayed power");
    release_old_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), old_processed_rx).await??;
    project(&mut runtime);
    assert!(
        !checked(&runtime),
        "late Ret cannot overwrite replacement topic"
    );
    assert!(
        !busy(&runtime),
        "old ticket stays terminal and is never replayed"
    );
    click(&mut runtime);
    tokio::time::timeout(Duration::from_secs(5), second_call_rx).await??;
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_terminal_failures_preserve_authoritative_state()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (calls_tx, mut calls_rx) = mpsc::channel(3);
    let (release_late_tx, release_late_rx) = oneshot::channel();
    let (late_fence_tx, late_fence_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        let mut advertised = methods(Some(1));
        advertised.get_mut("bluetooth.set_power").unwrap().timeout = 150;
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-a".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: advertised,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("first call expected")
        };
        calls_tx.send((m, p)).await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Err {
                    err: nacre_protocol::Failure::new(
                        nacre_protocol::ErrorCode::Busy,
                        "private daemon detail",
                        true,
                    )
                    .dispatch_state(true),
                },
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("only a deliberate second call is allowed")
        };
        calls_tx.send((m, p)).await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Ok {
                    ok: serde_json::json!({"changed":"malformed"}),
                },
            },
        )
        .await
        .unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("only a deliberate timeout case may send the third call")
        };
        calls_tx.send((m, p)).await.unwrap();
        release_late_rx.await.unwrap();
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
        let fence = nacre_protocol::Id::Text("uncertain-dispatch-fence".into());
        framing::write(&mut writer, &Message::Ping { id: fence.clone() })
            .await
            .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: fence });
        let _ = late_fence_tx.send(());
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("deliberate uncertain-dispatch call expected")
        };
        calls_tx.send((m, p)).await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ret {
                id,
                reply: Reply::Err {
                    err: nacre_protocol::Failure::new(
                        nacre_protocol::ErrorCode::Unavailable,
                        "outcome may be unknown",
                        true,
                    ),
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Unsub {
                topics: vec!["bluetooth".into()]
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut runtime = controlled_runtime(&client, root, lab.0.join("recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    click(&mut runtime);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv())
            .await?
            .unwrap(),
        (
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter":"adapter-A","powered":false})
        )
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&runtime),
        "Busy failure cannot flip authoritative power"
    );
    assert_eq!(
        runtime.session.stores.get("bluetooth.power_message"),
        Some(&bead::core::Value::Text(
            "Bluetooth power change failed".into()
        ))
    );
    click(&mut runtime);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv())
            .await?
            .unwrap(),
        (
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter":"adapter-A","powered":false})
        )
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime)
                && runtime.session.stores.get("bluetooth.power_message")
                    == Some(&bead::core::Value::Text(
                        "Bluetooth power change failed".into(),
                    ))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&runtime),
        "malformed CoreResult cannot flip authoritative power"
    );
    click(&mut runtime);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv())
            .await?
            .unwrap(),
        (
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter":"adapter-A","powered":false})
        )
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime)
                && runtime.session.stores.get("bluetooth.power_message")
                    == Some(&bead::core::Value::Text(
                        "Bluetooth power change failed".into(),
                    ))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&runtime),
        "timeout cannot change topic-derived checked state"
    );
    release_late_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), late_fence_rx).await??;
    project(&mut runtime);
    assert!(checked(&runtime), "uncertain late Ret cannot flip power");
    assert!(!busy(&runtime), "timeout is terminal and is not retried");
    click(&mut runtime);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv())
            .await?
            .unwrap(),
        (
            "bluetooth.set_power".into(),
            serde_json::json!({"adapter":"adapter-A","powered":false})
        )
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if !busy(&runtime) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&runtime),
        "uncertain dispatch cannot optimistically flip state"
    );
    assert_eq!(
        runtime.session.stores.get("bluetooth.power_message"),
        Some(&bead::core::Value::Text(
            "Bluetooth power change failed".into()
        ))
    );
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_queued_close_sends_nothing_and_link_recovers()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (probe_tx, probe_rx) = oneshot::channel();
    let (pong_tx, pong_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-a".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(Some(1)),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let fence = nacre_protocol::Id::Text("queued-case-snapshot".into());
        framing::write(&mut writer, &Message::Ping { id: fence.clone() })
            .await
            .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: fence });
        probe_rx.await.unwrap();
        framing::write(
            &mut writer,
            &Message::Ping {
                id: nacre_protocol::Id::Text("queued-close-progress".into()),
            },
        )
        .await
        .unwrap();
        let mut saw_pong = false;
        let mut saw_unsub = false;
        while !saw_pong || !saw_unsub {
            match next(&mut reader).await {
                Message::Pong {
                    id: nacre_protocol::Id::Text(ref id),
                } if id == "queued-close-progress" => {
                    saw_pong = true;
                }
                Message::Unsub { topics } => {
                    assert_eq!(topics, vec!["bluetooth"]);
                    saw_unsub = true;
                }
                Message::Call { .. } => {
                    panic!("queued ticket dropped before dispatch must send no Call")
                }
                Message::Ping { .. } => {}
                message => panic!("unexpected daemon message {message:?}"),
            }
        }
        let _ = pong_tx.send(());
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut runtime = controlled_runtime(&client, root, lab.0.join("recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut runtime);
            if runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let release = client.gate_for_tests().await?;
    click(&mut runtime);
    assert!(busy(&runtime));
    drop(runtime);
    drop(release);
    probe_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), pong_rx).await??;
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_retirement_drops_late_reply_for_replacement_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (first_call_tx, first_call_rx) = oneshot::channel();
    let (release_ret_tx, release_ret_rx) = oneshot::channel();
    let (fenced_tx, fenced_rx) = oneshot::channel();
    let (second_call_tx, second_call_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-retire-boot".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(Some(1)),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let Message::Call {
            id: old_id, m, p, ..
        } = next(&mut reader).await
        else {
            panic!("first runtime call expected")
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-A","powered":false})
            )
        );
        let _ = first_call_tx.send(());
        release_ret_rx.await.unwrap();
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
        let fence = nacre_protocol::Id::Text("retired-runtime-late-ret-fence".into());
        framing::write(&mut writer, &Message::Ping { id: fence.clone() })
            .await
            .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: fence });
        let _ = fenced_tx.send(());
        let (new_id, m, p) = loop {
            match next(&mut reader).await {
                Message::Call { id, m, p, .. } => break (id, m, p),
                Message::Pong { .. } => {}
                message => {
                    panic!("unexpected message before deliberate replacement call: {message:?}")
                }
            }
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-A","powered":false})
            )
        );
        let _ = second_call_tx.send(());
        framing::write(
            &mut writer,
            &Message::Ret {
                id: new_id,
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
                topics: vec!["bluetooth".into()]
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut old_runtime = controlled_runtime(&client, root.clone(), lab.0.join("retired-runtime"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut old_runtime);
            if old_runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    click(&mut old_runtime);
    assert!(busy(&old_runtime));
    tokio::time::timeout(Duration::from_secs(5), first_call_rx).await??;
    drop(old_runtime);

    let mut replacement = controlled_runtime(&client, root, lab.0.join("replacement-runtime"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut replacement);
            if replacement.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        checked(&replacement),
        "replacement runtime starts from the still-authoritative snapshot"
    );
    assert!(
        !busy(&replacement),
        "retired ticket is not inherited by a new runtime"
    );
    release_ret_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), fenced_rx).await??;
    project(&mut replacement);
    assert!(
        checked(&replacement),
        "late Ret cannot change replacement runtime state"
    );
    assert!(
        !busy(&replacement),
        "late Ret cannot attach to replacement runtime"
    );
    click(&mut replacement);
    assert!(busy(&replacement));
    tokio::time::timeout(Duration::from_secs(5), second_call_rx).await??;
    drop(replacement);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bluetooth_power_boot_replacement_does_not_replay_old_runtime_call()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (old_call_tx, old_call_rx) = oneshot::channel();
    let (close_old_tx, close_old_rx) = oneshot::channel();
    let (new_link_tx, new_link_rx) = oneshot::channel();
    let (new_call_tx, new_call_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (first, _) = listener.accept().await.unwrap();
        let (read, mut writer) = first.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-old".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(Some(1)),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: state(true),
            },
        )
        .await
        .unwrap();
        let Message::Call { m, p, .. } = next(&mut reader).await else {
            panic!("old runtime call expected")
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-A","powered":false})
            )
        );
        let _ = old_call_tx.send(());
        close_old_rx.await.unwrap();
        drop(writer);
        drop(reader);

        let (second, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let (read, mut writer) = second.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "bt-boot-new".into(),
                topics: BTreeMap::from([("bluetooth".into(), 1)]),
                methods: methods(Some(1)),
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            next(&mut reader).await,
            Message::Sub {
                topics: vec!["bluetooth".into()]
            }
        );
        let mut replacement = state(false);
        replacement["adapters"]["items"]
            .as_object_mut()
            .unwrap()
            .remove("adapter-A");
        replacement["adapters"]["items"]["adapter-B"] =
            serde_json::json!({"name":"New boot adapter","powered":false,"discovering":false});
        replacement["adapters"]["order"] = serde_json::json!(["adapter-B"]);
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "bluetooth".into(),
                seq: 0,
                data: replacement,
            },
        )
        .await
        .unwrap();
        let fence = nacre_protocol::Id::Text("new-boot-fence".into());
        framing::write(&mut writer, &Message::Ping { id: fence.clone() })
            .await
            .unwrap();
        assert_eq!(next(&mut reader).await, Message::Pong { id: fence });
        let _ = new_link_tx.send(());
        let Message::Call { id, m, p, .. } = next(&mut reader).await else {
            panic!("only a deliberate new-runtime action may call")
        };
        assert_eq!(
            (m.as_str(), p),
            (
                "bluetooth.set_power",
                serde_json::json!({"adapter":"adapter-B","powered":true})
            )
        );
        let _ = new_call_tx.send(());
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
                topics: vec!["bluetooth".into()]
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut old_runtime = controlled_runtime(&client, root.clone(), lab.0.join("old-recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut old_runtime);
            if old_runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    click(&mut old_runtime);
    assert!(busy(&old_runtime));
    tokio::time::timeout(Duration::from_secs(5), old_call_rx).await??;
    drop(old_runtime);
    close_old_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while client.status().boot.as_deref() != Some("bt-boot-new") || !client.snapshot().connected
        {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let mut new_runtime = controlled_runtime(&client, root, lab.0.join("new-recovery"))?;
    tokio::time::timeout(Duration::from_secs(5), new_link_rx).await??;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            project(&mut new_runtime);
            if new_runtime.session.stores.get("bluetooth.power_enabled")
                == Some(&bead::core::Value::Bool(true))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        !busy(&new_runtime),
        "old runtime request is not replayed on a replacement boot"
    );
    assert!(
        !checked(&new_runtime),
        "new boot topic supplies the only checked value"
    );
    click(&mut new_runtime);
    tokio::time::timeout(Duration::from_secs(5), new_call_rx).await??;
    drop(new_runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    let _ = shutdown_tx.send(());
    server.await?;
    Ok(())
}

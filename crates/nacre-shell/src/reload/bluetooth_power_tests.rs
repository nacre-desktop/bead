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

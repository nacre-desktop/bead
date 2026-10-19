// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Id, Message, Reply, framing};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{io::BufReader, net::UnixListener, sync::{mpsc, oneshot}};

struct Lab(PathBuf);
impl Lab {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("wifi-runtime-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
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
        methods.insert(name.into(), nacre_protocol::Method {
            v: version, class: Capability::System, timeout: 10_000,
            params: serde_json::json!({"type":"object"}), result: serde_json::json!({}),
        });
    }
    Message::Welcome {
        proto: 1, min: 1, boot: "wifi-boot-a".into(),
        topics: network_topic.map(|version| BTreeMap::from([("network".into(), version)])).unwrap_or_default(), methods, frame_ms: None,
    }
}
async fn next(reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Message {
    tokio::time::timeout(Duration::from_secs(5), reader.next()).await.unwrap().unwrap().unwrap()
}
fn switch_center(runtime: &Runtime) -> (f32, f32) {
    let node = runtime.session.view_nodes().iter().find(|node| node.id == "wifi-radio").unwrap();
    let rect = bead::ui::rect(&node.draw);
    (rect.x + rect.w / 2., rect.y + rect.h / 2.)
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_sends_enabled_once_and_ignores_busy_duplicate() -> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (first_call_tx, mut first_call_rx) = mpsc::channel(2);
    let (release_ret_tx, release_ret_rx) = oneshot::channel();
    let (allow_snap_tx, allow_snap_rx) = oneshot::channel();
    let (done_tx, done_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(&mut writer, &welcome(Some(("network.set_wifi", 1)), Some(1))).await.unwrap();
        assert_eq!(next(&mut reader).await, Message::Sub { topics: vec!["network".into()] });
        framing::write(&mut writer, &Message::Snap { topic: "network".into(), seq: 0, data: network(true) }).await.unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else { panic!("Wi-Fi call expected") };
        first_call_tx.send((m, p)).await.unwrap();
        release_ret_rx.await.unwrap();
        framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok: serde_json::json!({"changed":false}) } }).await.unwrap();
        allow_snap_rx.await.unwrap();
        framing::write(&mut writer, &Message::Snap { topic: "network".into(), seq: 1, data: network(false) }).await.unwrap();
        let Message::Call { id, m, p, .. } = next(&mut reader).await else { panic!("second deliberate Wi-Fi call expected") };
        first_call_tx.send((m, p)).await.unwrap();
        framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok: serde_json::json!({"changed":true}) } }).await.unwrap();
        assert_eq!(next(&mut reader).await, Message::Unsub { topics: vec!["network".into()] });
        let _ = done_tx.send(());
    });

    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    tokio::time::timeout(Duration::from_secs(5), async { while client.status().boot.is_none() { tokio::task::yield_now().await; } }).await?;
    let leased = Arc::new(client.lease_topic("network")?);
    tokio::time::timeout(Duration::from_secs(5), async { while !leased.snapshot().connected { tokio::task::yield_now().await; } }).await?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette_path = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette_path)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session = bead::ui::Session::new("shell".into(), "status/network.luau".into(), palette_path, colors, cold);
    let mut runtime = Runtime::new(session).with_core_network_control(leased, client.clone())?;
    let (event_tx, mut event_rx) = mpsc::channel(8);
    runtime.services(Box::new(move |event| { let _ = event_tx.try_send(event); }));
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    while event_rx.try_recv().is_ok() {}
    assert_eq!(runtime.session.stores.get("network.wifi_enabled"), Some(&bead::core::Value::Bool(true)));
    let (x, y) = switch_center(&runtime);
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert_eq!(runtime.session.stores.get("network.wifi_busy"), Some(&bead::core::Value::Bool(true)));
    let (x, y) = switch_center(&runtime);
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert_eq!(first_call_rx.recv().await.unwrap(), ("network.set_wifi".into(), serde_json::json!({"enabled":false})));
    release_ret_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv()).await?.expect("missing Ret wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(runtime.session.stores.get("network.wifi_busy"), Some(&bead::core::Value::Bool(false)));
    assert_eq!(runtime.session.stores.get("network.wifi_checked"), Some(&bead::core::Value::Bool(true)), "Ret is acknowledgement; Snap owns checked state");
    allow_snap_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv()).await?.expect("missing Snap wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(runtime.session.stores.get("network.wifi_checked"), Some(&bead::core::Value::Bool(false)));
    let (x, y) = switch_center(&runtime);
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert_eq!(first_call_rx.recv().await.unwrap(), ("network.set_wifi".into(), serde_json::json!({"enabled":true})), "deliberate click re-reads authoritative state");
    tokio::time::timeout(Duration::from_secs(5), event_rx.recv()).await?.expect("missing second Ret wake");
    runtime.stage(UiEvent::Model(BTreeMap::new()))?;
    runtime.commit();
    assert_eq!(runtime.session.stores.get("network.wifi_busy"), Some(&bead::core::Value::Bool(false)));
    assert_eq!(runtime.session.stores.get("network.wifi_checked"), Some(&bead::core::Value::Bool(false)), "changed=true Ret is acknowledgement; only a later Snap changes checked state");
    drop(runtime);
    tokio::time::timeout(Duration::from_secs(5), done_rx).await??;
    Source::stop(&client).await?;
    server.await?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn wifi_runtime_requires_exact_capability_and_fresh_topic() -> Result<(), Box<dyn std::error::Error>> {
    let cases: [(&str, Option<(&str, u16)>, Option<u16>, u8); 9] = [
        ("method absent", None, Some(1), 0),
        ("misleading name", Some(("network.set_wifi_extra", 1)), Some(1), 0),
        ("method v2", Some(("network.set_wifi", 2)), Some(1), 0),
        ("stale state", Some(("network.set_wifi", 1)), Some(1), 1),
        ("invalid state", Some(("network.set_wifi", 1)), Some(1), 2),
        ("topic v2", Some(("network.set_wifi", 1)), Some(2), 0),
        ("no topic", Some(("network.set_wifi", 1)), None, 0),
        ("source epoch mismatch", Some(("network.set_wifi", 1)), Some(1), 3),
        ("disconnected source", Some(("network.set_wifi", 1)), Some(1), 4),
    ];
    for (label, method, topic_version, state_case) in cases {
        let lab = Lab::new();
        let listener = lab.bind();
        let (done_tx, mut done_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
            framing::write(&mut writer, &welcome(method, topic_version)).await.unwrap();
            if topic_version == Some(1) {
                assert_eq!(next(&mut reader).await, Message::Sub { topics: vec!["network".into()] });
                let mut value = network(true);
                if state_case == 1 { value["health"]["stale"] = serde_json::json!(true); }
                if state_case == 2 { value["wireless_enabled"] = serde_json::Value::Null; }
                framing::write(&mut writer, &Message::Snap { topic: "network".into(), seq: 0, data: value }).await.unwrap();
            }
            loop {
                tokio::select! {
                    _ = &mut done_rx => break,
                    incoming = reader.next() => match incoming {
                        Ok(Some(Message::Call { .. })) => panic!("{label} must never emit a Wi-Fi call"),
                        Ok(Some(_)) => {},
                        _ => break,
                    }
                }
            }
        });
        let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
        tokio::time::timeout(Duration::from_secs(5), async { while client.status().boot.is_none() { tokio::task::yield_now().await; } }).await?;
        let mut value = network(true);
        if state_case == 1 { value["health"]["stale"] = serde_json::json!(true); }
        if state_case == 2 { value["wireless_enabled"] = serde_json::Value::Null; }
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
            tokio::time::timeout(Duration::from_secs(5), async { while !source.snapshot().connected { tokio::task::yield_now().await; } }).await?;
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette_path = root.join("palette.json");
        let colors = bead::render::Palette::read(&palette_path)?;
        let cold = crate::sound::fallback(colors.get("surface")?);
        let session = bead::ui::Session::new("shell".into(), "status/network.luau".into(), palette_path, colors, cold);
        let mut runtime = Runtime::new(session).with_core_network_control(source, client.clone())?;
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        assert_eq!(runtime.session.stores.get("network.wifi_enabled"), Some(&bead::core::Value::Bool(false)), "{label}");
        let node = runtime.session.view_nodes().iter().find(|node| node.id == "wifi-radio").unwrap();
        assert!(node.action.is_none(), "{label}: unavailable control exposes no hit action");
        let rect = bead::ui::rect(&node.draw);
        runtime.stage(UiEvent::Click { x: rect.x + rect.w / 2., y: rect.y + rect.h / 2. })?;
        runtime.commit();
        drop(runtime);
        let _ = done_tx.send(());
        Source::stop(&client).await?;
        server.await.unwrap();
    }
    Ok(())
}

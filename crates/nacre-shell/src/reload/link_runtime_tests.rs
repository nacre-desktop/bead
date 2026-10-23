// SPDX-License-Identifier: GPL-3.0-only
//! B0017: network connect/disconnect consumer tests against a fake daemon.
use super::wifi_runtime_tests::{Lab, controlled_runtime, next, project};
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::{io::BufReader, sync::mpsc};

fn topic(ap1_active: bool) -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "wireless_enabled":true,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{
            "ap0":{"device":"wifi0","name":"Home","strength":90,"secured":true,"active":!ap1_active,"connection":"c0"},
            "ap1":{"device":"wifi0","name":"Cafe","strength":60,"secured":false,"active":ap1_active},
            "ap2":{"device":"wifi0","name":"Known","strength":40,"secured":true,"active":false,"connection":"c1"}
        },"order":["ap0","ap1","ap2"]},
        "connections":{"items":{
            "c0":{"name":"Home","kind":"wireless","active":!ap1_active},
            "c1":{"name":"Known","kind":"wireless","active":false}
        },"order":["c0","c1"]},
        "primary_connection":if ap1_active { None } else { Some("c0") }
    })
}
fn welcome(methods: &[&str]) -> Message {
    let mut all = nacre_protocol::core_methods();
    for name in methods {
        all.insert(
            (*name).into(),
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
        topics: BTreeMap::from([("network".into(), 1)]),
        methods: all,
        frame_ms: None,
    }
}
fn rows(runtime: &Runtime) -> Vec<serde_json::Value> {
    match runtime.session.stores.get("network.rows") {
        Some(bead::core::Value::List(items)) => items.as_ref().clone(),
        other => panic!("rows store missing: {other:?}"),
    }
}
fn text(runtime: &Runtime, name: &str) -> String {
    match runtime.session.stores.get(name) {
        Some(bead::core::Value::Text(text)) => text.clone(),
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
        "timed out: rows {:?} message {:?}",
        rows(runtime),
        text(runtime, "network.link_message")
    );
}
fn act(runtime: &Runtime, method: &str, key: &str) {
    runtime
        .status
        .as_ref()
        .unwrap()
        .network_link_action(method, key, Arc::new(|| {}));
}
#[allow(clippy::type_complexity)]
async fn setup(
    methods: &'static [&'static str],
    script: impl Send
    + 'static
    + FnOnce(
        framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
        tokio::net::unix::OwnedWriteHalf,
    ) -> tokio::task::JoinHandle<()>,
) -> Result<(Lab, Client, Runtime, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(&mut writer, &welcome(methods))
            .await
            .unwrap();
        assert!(matches!(next(&mut reader).await, Message::Sub { .. }));
        framing::write(
            &mut writer,
            &Message::Snap {
                topic: "network".into(),
                seq: 0,
                data: topic(false),
            },
        )
        .await
        .unwrap();
        let _ = tx.send(script(reader, writer));
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    let mut runtime = controlled_runtime(&client, &lab.0.join("recovery"))?;
    let link = client.clone();
    let wanted = methods;
    until(&mut runtime, move |r| {
        rows(r).len() == 3
            && wanted
                .first()
                .is_none_or(|name| link.status().methods.contains_key(*name))
    })
    .await;
    project(&mut runtime);
    server.await?;
    Ok((lab, client, runtime, rx.await?))
}

#[test]
fn link_rows_are_bounded_and_resolve_only_fresh_targets() {
    let state: nacre_protocol::core_services::NetworkState =
        serde_json::from_value(topic(false)).unwrap();
    let keys: Vec<_> = crate::status_views::link_rows_for_test(&state);
    assert_eq!(keys, ["dev:wifi0", "ap:ap1", "ap:ap2"]);
    assert!(crate::status_views::resolve_link_for_test(&state, "ap:gone").is_none());
    assert!(crate::status_views::resolve_link_for_test(&state, "ap:ap0").is_none());
    assert!(crate::status_views::resolve_link_for_test(&state, "dev:nope").is_none());
    assert!(crate::status_views::resolve_link_for_test(&state, "bogus").is_none());
    // A saved profile is preferred over a raw access point target.
    let (method, params) = crate::status_views::resolve_link_for_test(&state, "ap:ap2").unwrap();
    assert_eq!(method, "network.connect");
    assert_eq!(
        params,
        serde_json::json!({"device":"wifi0","target":{"kind":"connection","id":"c1"}})
    );
    let (_, params) = crate::status_views::resolve_link_for_test(&state, "ap:ap1").unwrap();
    assert_eq!(
        params,
        serde_json::json!({"device":"wifi0","target":{"kind":"access_point","id":"ap1"}})
    );
    let (method, params) = crate::status_views::resolve_link_for_test(&state, "dev:wifi0").unwrap();
    assert_eq!(
        (method, params),
        ("network.disconnect", serde_json::json!({"device":"wifi0"}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn link_connect_waits_for_readback_then_errors_never_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let (calls_tx, mut calls_rx) = mpsc::channel(4);
    let (ret_tx, ret_rx) = tokio::sync::oneshot::channel::<()>();
    let (snap_tx, snap_rx) = tokio::sync::oneshot::channel::<()>();
    let (lab, client, mut runtime, script) = setup(
        &["network.connect", "network.disconnect"],
        move |mut reader, mut writer| {
            tokio::spawn(async move {
                let Message::Call { id, m, p, .. } = next(&mut reader).await else {
                    panic!("connect call expected")
                };
                calls_tx.send((m, p)).await.unwrap();
                ret_rx.await.unwrap();
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
                snap_rx.await.unwrap();
                framing::write(
                    &mut writer,
                    &Message::Snap {
                        topic: "network".into(),
                        seq: 1,
                        data: topic(true),
                    },
                )
                .await
                .unwrap();
                // Second deliberate call fails at admission; nothing may repeat it.
                let Message::Call { id, m, .. } = next(&mut reader).await else {
                    panic!("second call expected")
                };
                calls_tx.send((m, serde_json::Value::Null)).await.unwrap();
                framing::write(
                    &mut writer,
                    &Message::Ret {
                        id,
                        reply: Reply::Err {
                            err: Failure::new(ErrorCode::StaleRevision, "private text", false),
                        },
                    },
                )
                .await
                .unwrap();
                assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
            })
        },
    )
    .await?;
    assert!(rows(&runtime).iter().all(|row| row["enabled"] == true));
    let height = |r: &Runtime| r.session.stores.get("network.height").cloned();
    assert_eq!(height(&runtime), Some(bead::core::Value::Number(382.)));
    // Stale IDs send nothing.
    act(&runtime, "network.connect", "ap:vanished");
    project(&mut runtime);
    assert_eq!(
        text(&runtime, "network.link_message"),
        "Network list changed"
    );
    assert_eq!(height(&runtime), Some(bead::core::Value::Number(410.)));
    // Wrong action for the row (connect on an active device) is also stale.
    act(&runtime, "network.connect", "dev:wifi0");
    act(&runtime, "network.connect", "ap:ap1");
    project(&mut runtime);
    assert_eq!(
        calls_rx.recv().await.unwrap(),
        (
            "network.connect".into(),
            serde_json::json!({"device":"wifi0","target":{"kind":"access_point","id":"ap1"}})
        )
    );
    assert!(
        rows(&runtime).iter().all(|row| row["enabled"] == false),
        "pending blocks every row"
    );
    // Duplicate while pending is ignored.
    act(&runtime, "network.connect", "ap:ap2");
    ret_tx.send(()).unwrap();
    // Admission ack alone is not success: still pending until readback.
    until(&mut runtime, |r| {
        rows(r)
            .iter()
            .any(|row| row["id"] == "ap:ap1" && row["action"] == "Connecting…")
            && rows(r).iter().all(|row| row["enabled"] == false)
    })
    .await;
    assert!(
        rows(&runtime)
            .iter()
            .all(|row| row["active"] != true || row["id"] == "dev:wifi0")
    );
    snap_tx.send(()).unwrap();
    until(&mut runtime, |r| {
        rows(r).len() == 3 && rows(r).iter().all(|row| row["enabled"] == true)
    })
    .await;
    assert!(
        rows(&runtime)
            .iter()
            .any(|row| row["id"] == "dev:wifi0" && row["label"] == "Cafe")
    );
    // Admission error: sensible message, no retry, row usable again.
    act(&runtime, "network.connect", "ap:ap2");
    assert_eq!(calls_rx.recv().await.unwrap().0, "network.connect");
    until(&mut runtime, |r| {
        text(r, "network.link_message") == "Network list changed; try again"
    })
    .await;
    assert!(rows(&runtime).iter().all(|row| row["enabled"] == true));
    drop(runtime);
    script.await?;
    Source::stop(&client).await?;
    drop(lab);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn link_controls_are_capability_gated() -> Result<(), Box<dyn std::error::Error>> {
    // Only connect advertised: disconnect rows are disabled and send nothing.
    let (lab, client, mut runtime, script) = setup(&["network.connect"], |mut reader, writer| {
        tokio::spawn(async move {
            let _writer = writer;
            // The first message after the subscription must be the unsubscribe.
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    let by_id = |runtime: &Runtime, id: &str| {
        rows(runtime)
            .into_iter()
            .find(|row| row["id"] == id)
            .unwrap()
    };
    assert_eq!(by_id(&runtime, "dev:wifi0")["enabled"], false);
    assert_eq!(by_id(&runtime, "ap:ap1")["enabled"], true);
    act(&runtime, "network.disconnect", "dev:wifi0");
    project(&mut runtime);
    drop(runtime);
    script.await?;
    Source::stop(&client).await?;
    drop(lab);
    // Neither advertised: every row is disabled.
    let (lab, client, runtime, script) = setup(&[], |mut reader, writer| {
        tokio::spawn(async move {
            let _writer = writer;
            assert!(matches!(next(&mut reader).await, Message::Unsub { .. }));
        })
    })
    .await?;
    assert!(rows(&runtime).iter().all(|row| row["enabled"] == false));
    act(&runtime, "network.connect", "ap:ap1");
    drop(runtime);
    script.await?;
    Source::stop(&client).await?;
    drop(lab);
    Ok(())
}

// SPDX-License-Identifier: GPL-3.0-only
//! B0086: the sound popout's microphone meter leases `audio.peak` (N0061) only while the default
//! input is live and the view is shown, and shows application streams, through the real template.
use super::wifi_runtime_tests::{Lab, isolate_runtime, next, until};
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::{Value, source::Source};
use nacre_protocol::{Message, framing};
use std::{path::PathBuf, sync::Arc};
use tokio::io::BufReader;

fn audio(muted: bool) -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "default_output":"out","default_input":"mic",
        "nodes":{"order":["out","mic","sink-input:7"],"items":{
            "out":{"name":"Speakers","kind":"output","volume":0.125,"muted":false,"ready":true},
            "mic":{"name":"Microphone","kind":"input","volume":0.5,"muted":muted,"ready":true},
            "sink-input:7":{"name":"Fixture player","kind":"playback","volume":0.25,"muted":false,"ready":true}}}
    })
}
fn welcome() -> Message {
    Message::Welcome {
        proto: 1,
        min: 1,
        boot: "audio-boot-a".into(),
        topics: BTreeMap::from([("audio".into(), 1), ("audio.peak".into(), 1)]),
        methods: nacre_protocol::core_methods(),
        frame_ms: None,
    }
}
fn number(runtime: &Runtime, name: &str) -> f64 {
    match runtime.session.stores.get(name) {
        Some(Value::Number(value)) => *value,
        other => panic!("{name} missing: {other:?}"),
    }
}
fn flag(runtime: &Runtime, name: &str) -> bool {
    runtime.session.stores.get(name) == Some(&Value::Bool(true))
}
async fn expect_topics(
    reader: &mut framing::Reader<BufReader<tokio::net::unix::OwnedReadHalf>>,
    unsub: bool,
    topic: &str,
) {
    match (next(reader).await, unsub) {
        (Message::Sub { topics }, false) | (Message::Unsub { topics }, true) => {
            assert_eq!(topics, [topic.to_owned()])
        }
        other => panic!("expected the {topic} lease change, got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn the_meter_holds_the_peak_lease_only_while_the_input_is_live()
-> Result<(), Box<dyn std::error::Error>> {
    let lab = Lab::new();
    let listener = lab.bind();
    let (muted_tx, muted_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(next(&mut reader).await, Message::Hello { .. }));
        framing::write(&mut writer, &welcome()).await.unwrap();
        expect_topics(&mut reader, false, "audio").await;
        // No microphone data yet: nothing may be captured.
        let snap = |topic: &str, seq, data| Message::Snap {
            topic: topic.into(),
            seq,
            data,
        };
        framing::write(&mut writer, &snap("audio", 0, audio(false)))
            .await
            .unwrap();
        expect_topics(&mut reader, false, "audio.peak").await;
        framing::write(
            &mut writer,
            &snap(
                "audio.peak",
                0,
                serde_json::json!({"health":{"available":true,"stale":false},"level":0.5}),
            ),
        )
        .await
        .unwrap();
        muted_rx.await.unwrap();
        // A muted microphone releases the capture.
        framing::write(&mut writer, &snap("audio", 1, audio(true)))
            .await
            .unwrap();
        expect_topics(&mut reader, true, "audio.peak").await;
        expect_topics(&mut reader, true, "audio").await;
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), vec![]))?;
    let palette = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = bead::render::Palette::read(&palette)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session = bead::ui::Session::new(
        palette.parent().unwrap().into(),
        "sound/core.luau".into(),
        palette,
        colors,
        cold,
    );
    let source: Arc<dyn Source> = Arc::new(client.lease_topic("audio")?);
    // The view is opened over a topic that already has data, as the popout is in practice.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !(source.snapshot().connected && source.snapshot().data.is_object()) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let mut runtime = Runtime::panel_session(session, true)
        .with_view_edge("top")?
        .with_panel_geometry(382., 286., 0.23)?
        .with_core_audio_status(source.clone(), Some(client.clone()))?
        .with_status_controls(Some(client.clone()), vec![("audio", source)])?;
    isolate_runtime(&mut runtime, &lab.0.join("recovery"));
    runtime.services(Box::new(|_| {}));
    runtime
        .stage(UiEvent::Reload)?
        .expect("sound template loads");
    runtime.commit();
    until(&mut runtime, |r| {
        flag(r, "audio.peak_on") && number(r, "audio.peak") == 0.5
    })
    .await;
    // The application stream is a card with its name and percent, and the meter has its caption.
    let scene = runtime.stage(UiEvent::Reload)?.expect("reload paints");
    runtime.commit();
    let texts: Vec<_> = scene
        .primitives
        .iter()
        .filter_map(|p| match p {
            bead::render::Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    for wanted in ["Fixture player", "63%", "Input level"] {
        assert!(texts.iter().any(|t| t == wanted), "{wanted} in {texts:?}");
    }
    muted_tx.send(()).unwrap();
    until(&mut runtime, |r| {
        !flag(r, "audio.peak_on") && number(r, "audio.peak") == 0.
    })
    .await;
    drop(runtime);
    server.await?;
    Source::stop(&client).await?;
    drop(lab);
    Ok(())
}

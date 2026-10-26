// SPDX-License-Identifier: GPL-3.0-only
//! B0022: the real bar template routes actions through N0031 calls on a fake daemon.
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Message, Reply, framing};
use std::{os::unix::fs::PermissionsExt, sync::Arc, time::Duration};
use tokio::{
    io::BufReader,
    net::UnixListener,
    sync::{Notify, mpsc},
};

type Calls = mpsc::UnboundedReceiver<(String, serde_json::Value)>;
type Events = std::sync::mpsc::Receiver<UiEvent>;

const ALL: &[&str] = &[
    "workspace.activate",
    "workspace.scroll",
    "window.focus",
    "audio.adjust_volume",
    "audio.toggle_mute",
    "brightness.adjust",
    "tray.activate",
    "tray.secondary_activate",
    "tray.context_menu",
    "tray.scroll",
];

fn tray() -> serde_json::Value {
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/sample.svg");
    let menu = serde_json::json!({"items":{},"order":[]});
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "items":{"items":{
            "sni:menu":{"title":"Menu app","visible":true,"item_is_menu":true,"attention":false,"icon_path":icon,"menu":menu},
            "sni:plain":{"title":"Plain app","visible":true,"item_is_menu":false,"attention":false,"icon_path":icon,"menu":menu},
        },"order":["sni:menu","sni:plain"]}
    })
}

fn compositor() -> serde_json::Value {
    serde_json::json!({"available":true,"workspaces":{
        "niri-workspace:3":{"originOutput":"B","originIndex":1,"visiting":true},
        "niri-workspace:4":{"outputId":"A"},
    }})
}

/// Fake daemon that accepts reconnects. `kick` drops the current link.
async fn daemon(
    methods: &'static [&'static str],
) -> (std::path::PathBuf, Client, Calls, Arc<Notify>) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bar-routes-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls, received) = mpsc::unbounded_channel();
    let kick = Arc::new(Notify::new());
    let kicked = kick.clone();
    tokio::spawn(async move {
        let mut generation = 0;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            generation += 1;
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            if !matches!(reader.next().await, Ok(Some(Message::Hello { .. }))) {
                continue;
            }
            let mut all = nacre_protocol::core_methods();
            for name in methods {
                all.insert(
                    (*name).into(),
                    nacre_protocol::Method {
                        v: 1,
                        class: Capability::Desktop,
                        timeout: 10_000,
                        params: serde_json::json!({"type":"object"}),
                        result: serde_json::json!({}),
                    },
                );
            }
            let welcome = Message::Welcome {
                proto: 1,
                min: 1,
                boot: format!("bar-boot-{generation}"),
                topics: std::collections::BTreeMap::from([
                    ("tray".into(), 1),
                    ("compositor".into(), 1),
                    ("network".into(), 1),
                    ("bluetooth".into(), 1),
                    ("battery".into(), 1),
                    ("audio".into(), 1),
                ]),
                methods: all,
                frame_ms: None,
            };
            framing::write(&mut writer, &welcome).await.unwrap();
            loop {
                tokio::select! {
                    _ = kicked.notified() => break,
                    message = reader.next() => match message {
                        Ok(Some(Message::Call { id, m, p, .. })) => {
                            calls.send((m, p)).unwrap();
                            let reply = Reply::Ok { ok: serde_json::json!({"changed":true}) };
                            framing::write(&mut writer, &Message::Ret { id, reply }).await.unwrap();
                        }
                        Ok(Some(Message::Sub { topics })) => {
                            for topic in topics {
                                let data = if topic == "compositor" { compositor() } else { tray() };
                                let snap = Message::Snap { topic, seq: 0, data };
                                framing::write(&mut writer, &snap).await.unwrap();
                            }
                        }
                        Ok(Some(_)) => {}
                        _ => break,
                    }
                }
            }
        }
    });
    let client = Client::start(Config::new(socket, vec!["tray".into()])).unwrap();
    (dir, client, received, kick)
}

fn bar(client: &Client, output: &str) -> (Runtime, Events) {
    bar_with(client, output, false)
}
fn bar_with(client: &Client, output: &str, origins: bool) -> (Runtime, Events) {
    let palette =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session = bead::ui::Session::new(
        palette.parent().unwrap().into(),
        "bar/main.luau".into(),
        palette,
        colors,
        cold,
    );
    session.set_viewport(1920, 1200);
    session.stores = session
        .stores
        .reconcile(&crate::desktop::values(
            &bead::services::Snapshot::default(),
            "",
        ))
        .unwrap();
    // The output's own rows (B0025): index is the daemon slot, id is local.
    session.stores = session
        .stores
        .reconcile(&BTreeMap::from([
            (
                "bead.workspaces".into(),
                bead::core::Value::list(vec![
                    serde_json::json!({"id":"native-a","name":"2","index":2,"active":true,"hinted":true,"shown":1,"app1":"language","win1":"niri:1:window:7"}),
                ]),
            ),
            (
                "bead.focused_title".into(),
                bead::core::Value::Text("Title".into()),
            ),
            ("bar.tray.visible".into(), bead::core::Value::Bool(true)),
        ]))
        .unwrap();
    let mut runtime = Runtime::for_output(session, Some(output.into()))
        .with_popout_trigger()
        .unwrap();
    runtime.state_file = None;
    runtime.bar = true;
    runtime.tray_enabled = false;
    for name in [
        "backend.workspace.switch",
        "backend.window.focus",
        "services.tray",
        "services.tray.secondary",
        "services.audio",
    ] {
        runtime.actions.register(name, "bar.test").unwrap();
    }
    runtime.actions.grant("bar.test");
    let topic = Arc::new(client.topic("tray").unwrap()) as Arc<dyn bead::core::source::Source>;
    let mut runtime = runtime.with_bar_routes(client.clone(), topic).unwrap();
    if origins {
        let leased = client.lease_topic("compositor").unwrap();
        runtime = runtime.with_origins_topic(Arc::new(leased)).unwrap();
    }
    let (sender, events) = std::sync::mpsc::channel();
    let sender = std::sync::Mutex::new(sender);
    runtime.services(Box::new(move |event| {
        let _ = sender.lock().unwrap().send(event);
    }));
    runtime.stage(UiEvent::Reload).unwrap().expect("bar loads");
    runtime.commit();
    (runtime, events)
}
/// Apply service events (capabilities, tray topic) one at a time like the host
/// loop, checking `done` after each so transient states are observable.
async fn until(runtime: &mut Runtime, events: &Events, done: impl Fn(&Runtime) -> bool) {
    for _ in 0..500 {
        if done(runtime) {
            return;
        }
        match events.try_recv() {
            Ok(event) => {
                runtime.stage(event).unwrap();
                runtime.commit();
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    panic!("condition not reached");
}
fn flag(runtime: &Runtime, store: &str) -> bool {
    runtime.session.stores.get(store) == Some(&bead::core::Value::Bool(true))
}
fn node(runtime: &Runtime, action: &str, arg: Option<&str>) -> Option<(f32, f32)> {
    runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| {
            n.action.as_deref() == Some(action)
                && arg.is_none_or(|a| {
                    n.action_arg
                        .as_ref()
                        .and_then(crate::widgets::dropdown_view)
                        == Some(a)
                })
        })
        .map(|n| {
            let rect = bead::ui::rect(&n.draw);
            (rect.x + rect.w / 2., rect.y + rect.h / 2.)
        })
}
fn at(runtime: &mut Runtime, action: &str, arg: Option<&str>, make: impl Fn(f32, f32) -> UiEvent) {
    let (x, y) = node(runtime, action, arg).unwrap_or_else(|| panic!("{action} {arg:?} missing"));
    let event = make(x, y);
    let release = matches!(&event, UiEvent::Click { .. });
    runtime.stage(event).unwrap();
    runtime.commit();
    if release {
        runtime.stage(UiEvent::Release).unwrap();
        runtime.commit();
    }
}
async fn call(calls: &mut Calls, what: &str) -> (String, serde_json::Value) {
    tokio::time::timeout(Duration::from_secs(5), calls.recv())
        .await
        .unwrap_or_else(|_| panic!("no daemon call for {what}"))
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_title_pill_and_tray_actions_are_scoped_published_calls() {
    let (dir, client, mut calls, _kick) = daemon(ALL).await;
    let (mut runtime, events) = bar(&client, "right");
    until(&mut runtime, &events, |r| {
        flag(r, "bar.workspace.enabled")
            && flag(r, "bar.tray.enabled")
            && node(r, "services.tray", Some("sni:menu")).is_some()
    })
    .await;

    at(&mut runtime, "backend.workspace.switch", None, |x, y| {
        UiEvent::Click { x, y }
    });
    assert_eq!(
        call(&mut calls, "workspace click").await,
        (
            "workspace.activate".into(),
            serde_json::json!({"output":"right","workspace":2})
        )
    );
    at(&mut runtime, "backend.workspace.switch", None, |x, y| {
        UiEvent::Scroll { x, y, delta: -1. }
    });
    assert_eq!(
        call(&mut calls, "workspace scroll").await,
        (
            "workspace.scroll".into(),
            serde_json::json!({"output":"right","direction":"next"})
        )
    );
    // N0040: a window glyph focuses its window by the daemon's id, with no output.
    at(
        &mut runtime,
        "backend.window.focus",
        Some("niri:1:window:7"),
        |x, y| UiEvent::Click { x, y },
    );
    assert_eq!(
        call(&mut calls, "glyph click").await,
        (
            "window.focus".into(),
            serde_json::json!({"window":"niri-window:7"})
        )
    );
    at(&mut runtime, "bar.title", None, |x, y| UiEvent::Scroll {
        x,
        y,
        delta: 1.,
    });
    assert_eq!(
        call(&mut calls, "title scroll").await,
        (
            "audio.adjust_volume".into(),
            serde_json::json!({"delta":0.05})
        )
    );
    at(&mut runtime, "status.pill.open", Some("battery"), |x, y| {
        UiEvent::Scroll { x, y, delta: 1. }
    });
    assert_eq!(
        call(&mut calls, "battery scroll").await,
        (
            "brightness.adjust".into(),
            serde_json::json!({"output":"right","delta":5})
        )
    );
    at(&mut runtime, "status.pill.open", Some("audio"), |x, y| {
        UiEvent::SecondaryClick { x, y }
    });
    assert_eq!(
        call(&mut calls, "audio secondary").await,
        ("audio.toggle_mute".into(), serde_json::json!({}))
    );

    for (arg, make, method) in [
        (
            "sni:menu",
            (|x, y| UiEvent::Click { x, y }) as fn(f32, f32) -> UiEvent,
            "tray.context_menu",
        ),
        ("sni:plain", |x, y| UiEvent::Click { x, y }, "tray.activate"),
        (
            "sni:plain",
            |x, y| UiEvent::SecondaryClick { x, y },
            "tray.secondary_activate",
        ),
        (
            "sni:plain",
            |x, y| UiEvent::ContextMenu { x, y },
            "tray.context_menu",
        ),
    ] {
        at(&mut runtime, "services.tray", Some(arg), make);
        let (m, p) = call(&mut calls, "tray").await;
        assert_eq!((m.as_str(), p["item"].as_str()), (method, Some(arg)));
        assert!(p["x"].is_i64() && p["y"].is_i64());
    }
    at(&mut runtime, "services.tray", Some("sni:plain"), |x, y| {
        UiEvent::HorizontalScroll { x, y, delta: -1. }
    });
    assert_eq!(
        call(&mut calls, "tray scroll").await,
        (
            "tray.scroll".into(),
            serde_json::json!({"item":"sni:plain","axis":"horizontal","delta":120})
        )
    );
    // Daemon mode never falls back to the local compositor, audio or tray paths.
    assert!(calls.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn unadvertised_methods_disable_controls_and_send_nothing() {
    let (dir, client, mut calls, _kick) = daemon(&["audio.adjust_volume"]).await;
    let (mut runtime, events) = bar(&client, "left");
    let live = client.clone();
    until(&mut runtime, &events, |r| {
        live.snapshot().connected
            && live.status().boot.is_some()
            && r.session.stores.get("bar.workspace.enabled")
                == Some(&bead::core::Value::Bool(false))
            && node(r, "services.tray", Some("sni:menu")).is_none()
    })
    .await;
    assert!(!flag(&runtime, "bar.tray.enabled"));
    assert!(node(&runtime, "backend.workspace.switch", None).is_none());
    assert!(
        node(&runtime, "backend.window.focus", None).is_none(),
        "glyphs are display-only without window.focus"
    );
    // The one advertised method still works; a title click is not a scroll route.
    at(&mut runtime, "bar.title", None, |x, y| UiEvent::Click {
        x,
        y,
    });
    at(&mut runtime, "bar.title", None, |x, y| UiEvent::Scroll {
        x,
        y,
        delta: -1.,
    });
    assert_eq!(
        call(&mut calls, "tray scroll b").await,
        (
            "audio.adjust_volume".into(),
            serde_json::json!({"delta":-0.05})
        )
    );
    at(&mut runtime, "status.pill.open", Some("battery"), |x, y| {
        UiEvent::Scroll { x, y, delta: 1. }
    });
    at(&mut runtime, "status.pill.open", Some("audio"), |x, y| {
        UiEvent::SecondaryClick { x, y }
    });
    assert!(
        calls.try_recv().is_err(),
        "unadvertised methods are never guessed"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn daemon_restart_keeps_the_bar_and_gates_controls_by_link() {
    let (dir, client, mut calls, kick) = daemon(ALL).await;
    let (mut runtime, events) = bar(&client, "right");
    until(&mut runtime, &events, |r| flag(r, "bar.workspace.enabled")).await;
    kick.notify_one();
    until(&mut runtime, &events, |r| !flag(r, "bar.workspace.enabled")).await;
    // The bar is still laid out with its output's workspace; the click is refused locally.
    assert!(node(&runtime, "backend.workspace.switch", None).is_none());
    assert!(
        node(&runtime, "bar.title", None).is_some(),
        "the bar stays laid out"
    );
    until(&mut runtime, &events, |r| flag(r, "bar.workspace.enabled")).await;
    assert!(
        calls.try_recv().is_err(),
        "nothing is replayed after reconnect"
    );
    at(&mut runtime, "backend.workspace.switch", None, |x, y| {
        UiEvent::Click { x, y }
    });
    assert_eq!(
        call(&mut calls, "post-restart click").await.0,
        "workspace.activate"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn compositor_topic_origins_reach_the_bar_stores() {
    let (dir, client, _calls, _kick) = daemon(ALL).await;
    let (mut runtime, events) = bar_with(&client, "right", true);
    let expected = bead::core::Value::list(vec![
        serde_json::json!({"id":"niri-workspace:3","output":"B","index":1,"visiting":true}),
    ]);
    until(&mut runtime, &events, |r| {
        r.session.stores.get("bar.workspace.origins") == Some(&expected)
    })
    .await;
    assert_eq!(runtime.workspaces.view.origins.len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

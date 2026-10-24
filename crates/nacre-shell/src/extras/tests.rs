// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::Config;
use nacre_protocol::{Message, Reply, framing};
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};
fn fixture() -> Json {
    json!({"available":true,"capabilities":{"perOutputWorkspaces":true},"focus":{"outputId":"OUT"},
        "workspaces":{"ws1":{"index":1,"name":"Work","outputId":"OUT"},"ws2":{"index":2,"outputId":"OTHER"}},"workspaceOrder":["ws1","ws2"],
        "windows":{"win1":{"title":"Editor","wmClass":"editor","workspaceId":"ws1","outputId":"OUT"},"win2":{"title":"Other","workspaceId":"ws2","outputId":"OTHER"},"hidden":{"hidden":true}},"windowOrder":["win1","win2","hidden"]})
}
#[test]
fn overview_follows_focused_output_and_stable_ids() {
    let (windows, workspaces) = compositor_rows(&fixture());
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["id"], "win1");
    assert_eq!(windows[0]["workspace"], "ws1");
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["index"], 1);
}
struct Lab {
    dir: std::path::PathBuf,
    client: Client,
    calls: mpsc::UnboundedReceiver<(String, Json)>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn lab() -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "extras-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (tx, calls) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap(),
            Some(Message::Hello { .. })
        ));
        let mut methods = nacre_protocol::core_methods();
        for name in [
            "window.focus",
            "workspace.activate",
            "shortcuts.refresh",
            "menu.query",
            "menu.activate",
        ] {
            methods.insert(
                name.into(),
                nacre_protocol::Method {
                    v: 1,
                    class: nacre_protocol::Capability::Desktop,
                    timeout: 1000,
                    params: json!({}),
                    result: json!({}),
                },
            );
        }
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "extras-boot".into(),
                topics: BTreeMap::from([("compositor".into(), 1), ("shortcuts".into(), 1)]),
                methods,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        while let Ok(Some(msg)) = reader.next().await {
            match msg {
                Message::Sub { topics } => {
                    for topic in topics {
                        let data = if topic == "compositor" {
                            fixture()
                        } else {
                            json!({"health":{"available":true,"stale":false},"items":{"order":["k1"],"items":{"k1":{"key":"Super+K","description":"Shortcuts"}}}})
                        };
                        if framing::write(
                            &mut writer,
                            &Message::Snap {
                                topic,
                                seq: 0,
                                data,
                            },
                        )
                        .await
                        .is_err()
                        {
                            return;
                        }
                    }
                }
                Message::Call { id, m, p, .. } => {
                    let _ = tx.send((m.clone(), p.clone()));
                    let ok = if m == "menu.query" {
                        json!({"query":p["query"],"mode":"apps","route":p["route"],"catalog_rev":7,"order":["action"],"items":{"action":{"name":"Open Settings","kind":"action","description":"Preferences","activation":"opaque-ticket","disabled":false}}})
                    } else if m == "menu.activate" {
                        json!({"keep_open":false})
                    } else {
                        json!({"changed":true})
                    };
                    if framing::write(
                        &mut writer,
                        &Message::Ret {
                            id,
                            reply: Reply::Ok { ok },
                        },
                    )
                    .await
                    .is_err()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(socket, vec![])).unwrap();
    for _ in 0..200 {
        if client.snapshot().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(client.snapshot().connected);
    Lab { dir, client, calls }
}
fn stores(query: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([(
            "extras.query".into(),
            Value::Text(query.into()),
        )]))
        .unwrap()
}
async fn ready(extras: &Extras, stores: &Stores) -> BTreeMap<String, Value> {
    for _ in 0..200 {
        let model = extras.project(stores, Some(Arc::new(|| {})));
        if model["extras.available"] == Value::Bool(true) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("extras data unavailable");
}
#[tokio::test(flavor = "current_thread")]
async fn published_topics_focus_and_shortcut_refresh() {
    let mut lab = lab().await;
    let overview = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    let model = ready(&overview, &stores("")).await;
    assert!(matches!(&model["extras.windows"],Value::List(rows) if rows.len()==1));
    overview.act("focus:missing", &stores(""), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    overview.act("focus:win1", &stores(""), Arc::new(|| {}));
    let call = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(call, ("window.focus".into(), json!({"window":"win1"})));
    drop(overview);
    let keys = Extras::new(Kind::Keybindings, lab.client.clone()).unwrap();
    let model = ready(&keys, &stores("")).await;
    assert!(matches!(&model["extras.rows"],Value::List(rows) if rows[0]["key"]=="Super+K"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("shortcuts.refresh".into(), json!({}))
    );
}
#[tokio::test(flavor = "current_thread")]
async fn palette_uses_opaque_engine_activation_and_rejects_obsolete_query() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let initial = stores("");
    let model = ready(&palette, &initial).await;
    assert!(
        matches!(&model["extras.rows"],Value::List(rows) if rows[0].get("activation").is_none())
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.query".into(),
            json!({"query":"","mode":"apps","route":"actions"})
        )
    );
    palette.act("activate:action", &stores("changed"), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    palette.act("activate:action", &initial, Arc::new(|| {}));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.activate".into(),
            json!({"id":"action","activation":"opaque-ticket","catalog_rev":7})
        )
    );
}
#[test]
fn extras_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    for kind in Kind::ALL {
        for scale in [1., 1.5] {
            let (windows, workspaces) = compositor_rows(&fixture());
            let rows = if kind == Kind::Keybindings {
                vec![
                    json!({"id":"k1","key":"Super + K","description":"Keyboard shortcuts"}),
                    json!({"id":"k2","key":"Super + Tab","description":"Windows"}),
                ]
            } else {
                vec![
                    json!({"id":"settings","name":"Settings","description":"Open Settings","icon":"preferences-system"}),
                ]
            };
            let mut h = bead_test::Harness::load_with_icons(
                &root,
                std::path::Path::new("extras/main.luau"),
                &root.join("palette.json"),
                BTreeMap::from([
                    ("panel.open".into(), Value::Bool(true)),
                    ("extras.kind".into(), Value::Text(kind.name().into())),
                    ("extras.windows".into(), Value::list(windows)),
                    ("extras.workspaces".into(), Value::list(workspaces)),
                    ("extras.rows".into(), Value::list(rows)),
                ]),
                bead_test::Config {
                    width: 1280,
                    height: 1000,
                    scale,
                },
                BTreeMap::from([
                    ("editor".into(), root.join("tests/icon.svg")),
                    ("preferences-system".into(), root.join("tests/icon.svg")),
                ]),
            )?;
            let frame = h.render()?;
            let path = root.join(format!("tests/goldens/extras-{}-{scale}.png", kind.name()));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&path)?;
            }
            frame.compare(
                &path,
                Default::default(),
                &std::env::temp_dir().join(format!("extras-{}-{scale}-diff", kind.name())),
            )?;
        }
    }
    Ok(())
}

#[cfg(feature = "trace")]
pub(crate) async fn retirement() {
    let lab = lab().await;
    for kind in [Kind::Overview, Kind::Keybindings] {
        let extras = Extras::new(kind, lab.client.clone()).unwrap();
        ready(&extras, &stores("")).await;
        let subscriptions = extras.subscribe(Arc::new(|| {})).unwrap();
        let snapshot = extras.topic.as_ref().unwrap().snapshot();
        let payload = Arc::downgrade(&snapshot.data);
        drop(snapshot);
        drop(subscriptions);
        drop(extras);
        for _ in 0..100 {
            if payload.upgrade().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            payload.upgrade().is_none(),
            "closed extras must release topic data"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn keyboard_search_and_focus_use_the_actual_luau_view() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab().await;
    let extras = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    ready(&extras, &stores("")).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, true)
        .with_extras(extras)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    runtime.stage(UiEvent::Reload).unwrap().expect("view loads");
    runtime.commit();
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.label == "Editor")
    );
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "extras.query".into(),
            Value::Text("no match".into()),
        )])))
        .unwrap();
    runtime.commit();
    assert!(
        !runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.label == "Editor")
    );
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert!(lab.calls.try_recv().is_err());
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )])))
        .unwrap();
    runtime.commit();
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("window.focus".into(), json!({"window":"win1"}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_actions_require_current_available_output_and_membership() {
    let mut lab = lab().await;
    let mut overview = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    let mut data = fixture();
    data["available"] = json!(false);
    let source = Arc::new(bead_test::source::FakeSource::new(data.clone()).unwrap());
    overview.topic = Some(source.clone());
    overview.act("workspace:1", &stores(""), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    data["available"] = json!(true);
    source.publish(data).unwrap();
    overview.act("workspace:2", &stores(""), Arc::new(|| {}));
    assert!(
        lab.calls.try_recv().is_err(),
        "other output's workspace is not admitted"
    );
    overview.act("workspace:1", &stores(""), Arc::new(|| {}));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "workspace.activate".into(),
            json!({"workspace":1,"output":"OUT"})
        )
    );
}

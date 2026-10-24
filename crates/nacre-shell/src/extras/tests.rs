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
async fn lab_with_topics(requested_topics: Vec<String>) -> Lab {
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
                        // Order is deliberately not alphabetical: `order` alone is authoritative (N0058).
                        let q = p["query"].as_str().unwrap_or_default();
                        let order = if q.is_empty() {
                            json!(["action", "action-b"])
                        } else if q.trim_start_matches('>').trim().starts_with("brain:") {
                            json!(["brain"])
                        } else {
                            json!(["action", "window", "app"])
                        };
                        let all = json!({
                            "action":{"name":"Open Settings","kind":"action","activation":"opaque-ticket","disabled":false},
                            "action-b":{"name":"Calculator","kind":"action","activation":"ticket-b","disabled":false},
                            "window":{"name":"Term","kind":"window","description":"Window · workspace 1","icon":"window","activation":"ticket-w","disabled":false},
                            "app":{"name":"Zed","kind":"app","description":"Application","icon":"apps","activation":"ticket-a","disabled":false},
                            "brain":{"name":"Search brain for x","kind":"action","description":"Open the brain drawer","icon":"neurology","activation":"ticket-br","disabled":false}});
                        let items: serde_json::Map<String, Json> = order
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|id| (id.as_str().unwrap().to_owned(), all[id.as_str().unwrap()].clone()))
                            .collect();
                        json!({"query":p["query"],"mode":p["mode"],"route":p["route"],"catalog_rev":7,"order":order,"items":items})
                    } else if m == "menu.activate" {
                        json!({"keep_open":true,"view":{"route":"actions.windows","query":"edited"}})
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
    let client = Client::start(Config::new(socket, requested_topics)).unwrap();
    for _ in 0..200 {
        if client.snapshot().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(client.snapshot().connected);
    Lab { dir, client, calls }
}
async fn lab() -> Lab {
    lab_with_topics(vec![]).await
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
            json!({"query":"","mode":"palette","route":"palette"})
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
fn names(model: &BTreeMap<String, Value>) -> Vec<String> {
    match &model["extras.rows"] {
        Value::List(rows) => rows
            .iter()
            .map(|r| r["name"].as_str().unwrap_or_default().to_owned())
            .collect(),
        _ => panic!("rows missing"),
    }
}
#[tokio::test(flavor = "current_thread")]
async fn palette_mode_keeps_the_daemon_order_for_empty_typed_and_prefixed_queries() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let model = ready(&palette, &stores("")).await;
    assert_eq!(names(&model), ["Open Settings", "Calculator"]);
    for (query, expected) in [
        ("set", vec!["Open Settings", "Term", "Zed"]),
        ("> set", vec!["Open Settings", "Term", "Zed"]),
        ("brain: x", vec!["Search brain for x"]),
        (">brain:x", vec!["Search brain for x"]),
    ] {
        let model = ready(&palette, &stores(query)).await;
        assert_eq!(names(&model), expected, "query {query:?}");
        let call = loop {
            let call = lab.calls.recv().await.unwrap();
            if call.1["query"] == query {
                break call;
            }
        };
        assert_eq!(call.0, "menu.query");
        assert_eq!(call.1["mode"], "palette");
        assert_eq!(call.1["route"], "palette");
    }
}
#[tokio::test(flavor = "current_thread")]
async fn palette_activates_a_typed_row_with_its_own_ticket() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let typed = stores("set");
    ready(&palette, &typed).await;
    while lab.calls.try_recv().is_ok() {}
    palette.act("activate:window", &typed, Arc::new(|| {}));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.activate".into(),
            json!({"id":"window","activation":"ticket-w","catalog_rev":7})
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
                // The first rows QML's empty palette shows (DesktopActions order, N0058).
                [
                    ("wallpaper", "Open wallpaper picker", "wallpaper"),
                    ("calculator", "Calculator", "calculate"),
                    ("colors", "Open Nacre Colors", "palette"),
                    ("settings", "Open Nacre Settings", "settings"),
                    ("ai", "Open Nacre AI", "smart_toy"),
                    ("brain", "Open Nacre Brain", "neurology"),
                    ("files", "Open files…", "folder_open"),
                ]
                .map(|(id, name, icon)| json!({"id":id,"name":name,"description":"Action","icon":icon,"kind":"action"}))
                .to_vec()
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
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "panel.open".into(),
            Value::Bool(true),
        )])))
        .unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(true)),
        "a quick reopen must supersede the earlier close request"
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

#[tokio::test(flavor = "current_thread")]
async fn shared_bar_compositor_and_nullable_menu_fields_load_real_views() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["compositor".into()]).await;
    for kind in [Kind::Overview, Kind::Palette, Kind::Keybindings] {
        let extras = Extras::new(kind, lab.client.clone()).unwrap();
        let model = ready(&extras, &stores("")).await;
        if kind == Kind::Palette {
            assert!(
                matches!(&model["extras.rows"], Value::List(rows) if rows[0]["description"] == "" && rows[0]["icon"] == "apps")
            );
        }
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
        let scene = runtime
            .stage(UiEvent::Reload)
            .unwrap()
            .expect("real topic-backed view loads");
        // Exercise decoding too: a staged icon is not proof its bytes render.
        use bead::render::Renderer;
        bead::render::tree::TreeCpuRenderer::default()
            .prepare(&scene, 1.)
            .unwrap();
        runtime.commit();
        let label = match kind {
            Kind::Overview => "Editor",
            Kind::Palette => "Open Settings",
            Kind::Keybindings => "Super+K",
        };
        assert!(
            runtime
                .session
                .view_nodes()
                .iter()
                .any(|node| node.label.trim() == label),
            "{} must render its topic/engine rows",
            kind.name()
        );
        if kind == Kind::Keybindings {
            let close = runtime
                .session
                .view_nodes()
                .iter()
                .find(|node| node.label == "Close" && node.action.is_some())
                .unwrap()
                .id
                .clone();
            runtime
                .stage(UiEvent::Model(BTreeMap::from([(
                    "bead.focused".into(),
                    Value::Text(close),
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
                runtime.session.stores.get("panel.open"),
                Some(&Value::Bool(false)),
                "Enter activates the focused Close button"
            );
        }
        if kind == Kind::Overview {
            let node = runtime
                .session
                .view_nodes()
                .iter()
                .find(|node| node.label == "Editor" && node.action.is_some())
                .unwrap();
            let rect = bead::ui::rect(&node.draw);
            runtime
                .stage(UiEvent::Click {
                    x: rect.x + rect.w / 2.,
                    y: rect.y + rect.h / 2.,
                })
                .unwrap();
            runtime.commit();
            assert!(
                lab.calls.try_recv().is_err(),
                "press must not focus before release"
            );
            runtime.stage(UiEvent::Release).unwrap();
            runtime.commit();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                ("window.focus".into(), json!({"window":"win1"}))
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn palette_navigation_applies_engine_query_and_schedules_its_route() {
    let lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    ready(&palette, &stores("")).await;
    let woke = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = woke.clone();
    let wake: CallWake = Arc::new(move || {
        signal.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    palette.act("activate:action", &stores(""), Arc::new(|| {}));
    let mut query = None;
    for _ in 0..200 {
        let model = palette.project(&stores(""), Some(wake.clone()));
        if let Some(Value::Text(text)) = model.get("extras.query") {
            query = Some(text.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(query.as_deref(), Some("edited"));
    assert!(woke.load(std::sync::atomic::Ordering::SeqCst));
    let model = ready(&palette, &stores("edited")).await;
    assert!(!model.contains_key("panel.open"));
    assert!(
        matches!(&palette.state.borrow().result,Some((_,result)) if result.route=="actions.windows" && result.query=="edited")
    );
}

#[test]
fn long_shortcut_lists_materialize_only_the_viewport() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let rows = (0..1000).map(|index| json!({"id":format!("key{index}"),"key":format!("Mod+{index}"),"description":format!("Action {index}")})).collect();
    let mut h = bead_test::Harness::load(
        &root,
        std::path::Path::new("extras/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("keybindings".into())),
            ("extras.rows".into(), Value::list(rows)),
        ]),
        bead_test::Config {
            width: 1280,
            height: 1000,
            scale: 1.,
        },
    )?;
    h.render()?;
    assert!(
        h.scene().primitives.len() < 200,
        "off-screen rows must stay unmaterialized"
    );
    assert!(
        matches!(h.session.stores.get("extras.rows"),Some(Value::List(rows)) if rows.len()==1000)
    );
    h.update(BTreeMap::from([(
        "extras.scroll".into(),
        Value::Number(4000.),
    )]))?;
    h.render()?;
    assert!(h.scene().primitives.len() < 200);
    assert!(
        h.scene()
            .primitives
            .iter()
            .any(|p| matches!(p,bead::render::Primitive::Text{text,..} if text=="Action 86"))
    );
    Ok(())
}

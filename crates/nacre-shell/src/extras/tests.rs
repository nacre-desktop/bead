// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::Config;
use nacre_protocol::{Message, Reply, framing};
use tokio::{
    io::BufReader,
    net::UnixListener,
    sync::{mpsc, oneshot},
};
fn fixture() -> Json {
    json!({"available":true,"capabilities":{"perOutputWorkspaces":true},"focus":{"outputId":"OUT"},
        "workspaces":{"ws1":{"index":1,"name":"Work","outputId":"OUT"},"ws2":{"index":2,"outputId":"OTHER"}},"workspaceOrder":["ws1","ws2"],
        "windows":{"win1":{"title":"Editor","wmClass":"editor","workspaceId":"ws1","outputId":"OUT"},"win2":{"title":"Other","workspaceId":"ws2","outputId":"OTHER"},"hidden":{"hidden":true}},"windowOrder":["win1","win2","hidden"]})
}

#[test]
fn raw_catalogue_palette_extension_survives_typed_entry_projection() {
    let catalogue = json!({"entries":[
        {"id":"/wall/a.png","kind":"image","palette":{"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}}},
        {"id":"/wall/b.png","kind":"image"},
        {"id":"/wall/c.png","kind":"image","palette":{"mode":"dark","colours":{"background":"red","onSurface":"#ffffff","primary":"#abcdef"}}}
    ]});
    let expected = json!({"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}});
    assert_eq!(
        raw_catalogue_palette(&catalogue, "/wall/a.png"),
        Some(&expected)
    );
    assert!(raw_catalogue_palette(&catalogue, "/wall/b.png").is_none());
    assert!(raw_catalogue_palette(&catalogue, "/wall/c.png").is_none());
    assert!(raw_catalogue_palette(&catalogue, "/wall/missing.png").is_none());
    assert!(valid_catalogue_palette(&json!({
        "mode":"dark",
        "colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}
    })));
    for invalid in [
        json!({"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff"}}),
        json!({"mode":"dark","colours":{"background":"red","onSurface":"#ffffff","primary":"#abcdef"}}),
        json!({"mode":"sepia","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}}),
    ] {
        assert!(!valid_catalogue_palette(&invalid));
    }
}
#[test]
fn targeted_palette_hydration_keeps_the_retained_cache_bounded() {
    let mut entries = (0..80)
        .map(|index| {
            json!({
                "id":format!("/wall/{index}.png"),
                "active":index == 0,
                "palette":{"mode":"dark","colours":{
                    "background":"#123456","onSurface":"#ffffff","primary":"#abcdef"
                }}
            })
        })
        .collect::<Vec<_>>();
    let requested = (16..80)
        .map(|index| format!("/wall/{index}.png"))
        .collect::<Vec<_>>();
    trim_catalogue_palette_cache(&mut entries, &requested);
    let retained = entries
        .iter()
        .filter(|entry| entry.get("palette").is_some())
        .count();
    assert_eq!(retained, CATALOGUE_PALETTE_CACHE_LIMIT);
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/0.png" && entry.get("palette").is_some() })
    );
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/16.png" && entry.get("palette").is_some() })
    );
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/79.png" && entry.get("palette").is_none() })
    );
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
    activation_release: Option<oneshot::Sender<()>>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn start_with_empty_wallpaper_catalogue(extras: &Extras) {
    let mut state = extras.state.borrow_mut();
    state.catalogue = None;
    state.catalogue_boot = None;
    state.catalogue_request_boot = None;
    state.catalogue_request_ids.clear();
    state.catalogue_request_handles.clear();
    state.catalogue_result.clear();
    state.catalogue_preloaded = false;
}
async fn lab_with_topics(requested_topics: Vec<String>) -> Lab {
    lab_with_activation_behavior(requested_topics, false, false).await
}
async fn lab_with_activation_gate(
    requested_topics: Vec<String>,
    gate_first_activation: bool,
) -> Lab {
    lab_with_activation_behavior(requested_topics, gate_first_activation, false).await
}
async fn lab_with_activation_behavior(
    requested_topics: Vec<String>,
    gate_first_activation: bool,
    fail_first_activation: bool,
) -> Lab {
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
    let (activation_release, activation_wait) = oneshot::channel();
    let activation_wait = Arc::new(std::sync::Mutex::new(
        gate_first_activation.then_some(activation_wait),
    ));
    let fixture_image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (tx, calls) = mpsc::unbounded_channel();
    let mut offered_topics = BTreeMap::from([
        ("compositor".into(), 1),
        ("shortcuts".into(), 1),
        ("wallpaper.active".into(), 1),
    ]);
    for topic in &requested_topics {
        offered_topics.insert(topic.clone(), 1);
    }
    let fail_activation = Arc::new(std::sync::atomic::AtomicBool::new(fail_first_activation));
    let fail_activation_for_server = fail_activation.clone();
    let activation_wait_for_server = activation_wait.clone();
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
            "wallpaper.catalogue",
            "wallpaper.activate",
            "settings.set",
            "settings.provider",
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
        let registry_methods = methods.clone();
        let registry_topics = offered_topics.keys().cloned().collect::<Vec<_>>();
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "extras-boot".into(),
                topics: offered_topics,
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
                        let data = match topic.as_str() {
                            "compositor" => fixture(),
                            "wallpaper.active" => {
                                json!({"available":true,"stale":false,"revision":17,
                                "current":{"revision":17,"palette_revision":17,"path":"/fixture/current.jpg",
                                    "file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                                    "presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}},
                                "outputs":{}})
                            }
                            "settings.store.wallpaper-picker" => {
                                json!({"store":"wallpaper-picker","rev":3,"data":{"kind":"static","layout":"carousel","paused":true,"future":"preserve"}})
                            }
                            _ => {
                                json!({"health":{"available":true,"stale":false},"items":{"order":["k1"],"items":{"k1":{"key":"Super+K","description":"Shortcuts"}}}})
                            }
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
                    if m != "daemon.status" {
                        let _ = tx.send((m.clone(), p.clone()));
                    }
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
                            .map(|id| {
                                (
                                    id.as_str().unwrap().to_owned(),
                                    all[id.as_str().unwrap()].clone(),
                                )
                            })
                            .collect();
                        json!({"query":p["query"],"mode":p["mode"],"route":p["route"],"catalog_rev":7,"order":order,"items":items})
                    } else if m == "menu.activate" {
                        json!({"keep_open":true,"view":{"route":"actions.windows","query":"edited"}})
                    } else if m == "wallpaper.catalogue" {
                        let entries = json!([
                            {"id":"/fixture/current.jpg","kind":"image","animated":false,"title":"Current","thumbnail":"","preview":"","active":true},
                            {"id":"/fixture/next.jpg","kind":"image","animated":false,"title":"Next","thumbnail":"","preview":"","active":false},
                            {"id":"/fixture/next.mp4","kind":"video","animated":false,"title":"Moving","thumbnail":"","preview":"","active":false},
                            {"id":"/fixture/animated.gif","kind":"image","animated":true,"title":"Animated","thumbnail":"","preview":"","active":false}
                        ]);
                        let ids = p["ids"].as_array().cloned().unwrap_or_default();
                        let entries: Vec<Json> = entries
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|entry| {
                                let mut entry = entry.clone();
                                if !ids.is_empty()
                                    && ids.iter().any(|id| id.as_str() == entry["id"].as_str())
                                {
                                    entry["thumbnail"] = fixture_image.clone().into();
                                    if entry["id"] != "/fixture/next.jpg" && entry["id"] != "/fixture/next.mp4" {
                                        entry["preview"] = fixture_image.clone().into();
                                    }
                                    if entry["id"] == "/fixture/next.mp4" {
                                        entry["video_predecode"] = json!({
                                            "poster_path":fixture_image,
                                            "poster_file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                                            "source_file_identity":{"device":"1","inode":"3","size":32,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0}
                                        });
                                    }
                                }
                                entry
                            })
                            .collect();
                        json!({"entries":entries})
                    } else if m == "wallpaper.activate" {
                        let wait = activation_wait_for_server
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .take();
                        if let Some(wait) = wait {
                            let _ = wait.await;
                        }
                        let revision = p["revision"].as_u64().unwrap_or(17) + 1;
                        json!({"revision":revision,"palette_revision":revision,"path":p["id"],
                            "file_identity":{"device":"1","inode":"3","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                            "presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}})
                    } else if m == "daemon.status" {
                        json!({"boot":"extras-boot","topics":registry_topics,"methods":registry_methods})
                    } else {
                        json!({"changed":true})
                    };
                    let reply = if m == "wallpaper.activate"
                        && fail_activation_for_server
                            .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        Reply::Err {
                            err: nacre_protocol::Failure::new(
                                nacre_protocol::ErrorCode::Unavailable,
                                "fixture activation failure",
                                false,
                            ),
                        }
                    } else {
                        Reply::Ok { ok }
                    };
                    if framing::write(&mut writer, &Message::Ret { id, reply })
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
    Lab {
        dir,
        client,
        calls,
        activation_release: gate_first_activation.then_some(activation_release),
    }
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
async fn a_closed_panel_forgets_the_typed_query() {
    let lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([
            ("extras.query".into(), Value::Text("set".into())),
            ("panel.open".into(), Value::Bool(false)),
        ]))
        .unwrap();
    let model = palette.project(&closed, None);
    assert_eq!(model["extras.query"], Value::Text(String::new()));
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
#[tokio::test(flavor = "current_thread")]
async fn wallpaper_catalogue_projects_and_selection_calls_revision_fenced_activate() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let stores = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.query".into(), Value::Text(String::new())),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
            (
                "extras.wallpaper.kind".into(),
                Value::Text("dynamic".into()),
            ),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    let first = wallpaper.project(&stores, Some(wake.clone()));
    assert_eq!(first["extras.wallpaper.loading"], Value::Bool(true));
    assert_eq!(
        first["extras.available"],
        Value::Bool(true),
        "an admitted catalogue request renders the loading state instead of unavailable"
    );
    assert!(
        wallpaper.content_ready(),
        "metadata loading cannot block the first frame"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    let mut rows = Value::list(Vec::new());
    for _ in 0..200 {
        let model = wallpaper.project(&stores, Some(wake.clone()));
        rows = model["extras.rows"].clone();
        if matches!(&rows,Value::List(entries) if entries.len()==4) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(matches!(&rows,Value::List(entries) if entries.len()==4
            && entries.iter().any(|entry| entry["id"]=="/fixture/animated.gif" && entry["animated"]==true)
            && entries.iter().any(|entry| entry["id"]=="/fixture/next.mp4")));
    assert!(
        wallpaper
            .state
            .borrow()
            .catalogue_result
            .iter()
            .all(|entry| entry["thumbnail"] == "" && entry["preview"] == ""),
        "ids:[] provides all metadata without materializing cache paths"
    );
    wallpaper.act("wallpaper:activate:/fixture/next.mp4", &stores, wake);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/next.mp4"})
        )
    );

    let closed = stores
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
        .unwrap();
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    let reopened = closed
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))
        .unwrap();
    let reopened_model = wallpaper.project(&reopened, Some(Arc::new(|| {})));
    assert!(
        matches!(
            &reopened_model["extras.rows"],
            Value::List(entries) if entries.len() == 4
        ),
        "a valid catalogue remains available when the panel reopens"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a reopen with cached catalogue rows must not respawn the full catalogue scan"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_hydrates_only_visible_image_ids_and_suppresses_missing_preview_retries() {
    use bead::render::{ImageFit, Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    wallpaper.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let mut scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 8.,
        slide: None,
        primitives: vec![Primitive::Image {
            blur: 0.,
            fit: ImageFit::Fill,
            clip_polygon: None,
            rect: Rect::new(0., 0., 8., 6.),
            path: std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.mp4"),
        }],
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 8., 6.),
    };
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "visible thumbnails hydrate progressively"
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.mp4"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    {
        let state = wallpaper.state.borrow();
        assert_eq!(
            state.catalogue_result.len(),
            4,
            "hydration retains the full catalogue"
        );
        let next = state
            .catalogue_result
            .iter()
            .find(|entry| entry["id"] == "/fixture/next.mp4")
            .unwrap();
        let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../shell/sample.png")
            .canonicalize()
            .unwrap();
        let expected = expected.to_string_lossy();
        assert_eq!(next["thumbnail"].as_str(), Some(expected.as_ref()));
        assert_eq!(
            next["preview"], "",
            "the moving poster comes from the explicit predecode descriptor"
        );
        assert_eq!(next["video_predecode"]["poster_path"], expected.as_ref());
        assert_eq!(
            next["video_predecode"]["source_file_identity"]["inode"],
            "3"
        );
        assert!(
            state
                .catalogue_result
                .iter()
                .filter(|entry| entry["id"] != "/fixture/next.mp4")
                .all(|entry| entry["thumbnail"] == "" && entry["preview"] == "")
        );
    }
    let mut repeated = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 8.,
        slide: None,
        primitives: vec![Primitive::Image {
            blur: 0.,
            fit: ImageFit::Fill,
            clip_polygon: None,
            rect: Rect::new(0., 0., 8., 6.),
            path: std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.mp4"),
        }],
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 8., 6.),
    };
    wallpaper.attach_images(&mut repeated, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "settled missing previews use their hydrated thumbnail on the first frame"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a missing preview must not cause an automatic retry loop"
    );
    assert!(
        repeated.asset_data.values().any(|bytes| !bytes.is_empty()),
        "the advertised thumbnail remains a fallback"
    );
    // A cached thumbnail is ready content and does not restart a catalogue child.
    let mut cached = repeated.clone();
    if let Primitive::Image { path, .. } = &mut cached.primitives[0] {
        *path = std::path::PathBuf::from("@icon:wallpaper:thumbnail:/fixture/next.mp4");
    }
    let template = cached.clone();
    let ready_revision = wallpaper.state.borrow().image_revision;
    wallpaper.attach_images(&mut cached, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "cached thumbnails must not delay ready images"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a cached thumbnail must not respawn palette/catalogue preparation"
    );
    assert_eq!(
        wallpaper.state.borrow().image_revision,
        ready_revision,
        "a cached thumbnail must not discard prepared render content"
    );
    wallpaper.attach_images(&mut template.clone(), Some(wake.clone()));
    assert!(
        lab.calls.try_recv().is_err(),
        "a settled visible set is prepared once"
    );
    let mut other_visible = template.clone();
    other_visible.asset_data.clear();
    if let Primitive::Image { path, .. } = &mut other_visible.primitives[0] {
        *path = std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.jpg");
    }
    let mut hydrated_other = other_visible.clone();
    wallpaper.attach_images(&mut other_visible, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.jpg"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        wallpaper
            .state
            .borrow()
            .catalogue_result
            .iter()
            .any(|entry| {
                entry["id"] == "/fixture/next.mp4"
                    && entry["video_predecode"]["source_file_identity"]["inode"] == "3"
            }),
        "metadata for another requested card preserves the hydrated video's descriptor"
    );
    wallpaper.attach_images(&mut hydrated_other, Some(wake.clone()));
    assert!(wallpaper.content_ready());

    // A real refresh ticket may be pending while displayed paths remain hydrated.
    wallpaper.state.borrow_mut().catalogue =
        wallpaper.begin("wallpaper.catalogue", json!({"ids":[]}), Some(wake));
    assert!(wallpaper.state.borrow().catalogue.is_some());
    assert!(
        wallpaper.content_ready(),
        "same-boot hydrated content remains ready during refresh"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_close_discards_an_inflight_hydration_ticket_and_cache() {
    use bead::render::{ImageFit, Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    wallpaper.project(&open, Some(wake.clone()));
    let _ = lab.calls.recv().await.unwrap();
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 8.,
        slide: None,
        primitives: vec![Primitive::Image {
            blur: 0.,
            fit: ImageFit::Fill,
            clip_polygon: None,
            rect: Rect::new(0., 0., 8., 6.),
            path: std::path::PathBuf::from("@icon:wallpaper:thumbnail:/fixture/current.jpg"),
        }],
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 8., 6.),
    };
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    let call = lab.calls.recv().await.unwrap();
    assert_eq!(call.0, "wallpaper.catalogue");
    assert!(!wallpaper.state.borrow().catalogue_request_ids.is_empty());
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
        .unwrap();
    wallpaper.project(&closed, Some(wake));
    let state = wallpaper.state.borrow();
    assert!(state.catalogue.is_none());
    assert!(state.catalogue_request_ids.is_empty());
    assert!(state.catalogue_pending_handles.is_empty());
    assert_eq!(state.image_bytes, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_refreshes_deleted_cache_paths_and_settles_still_broken_paths() {
    use bead::render::{ImageFit, Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let wake: CallWake = Arc::new(|| {});
    let stale = lab.dir.join("deleted-cache.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([40, 70, 90, 255]))
        .save(&stale)
        .unwrap();
    std::fs::remove_file(&stale).unwrap();
    {
        let mut state = wallpaper.state.borrow_mut();
        state.wallpaper_open = true;
        state.wallpaper_boot = Some("extras-boot".into());
        state.catalogue_boot = Some("extras-boot".into());
        state.catalogue_result = vec![json!({
            "id":"/fixture/current.jpg","kind":"image","title":"Current",
            "thumbnail":stale.to_string_lossy(),"preview":"","active":true
        })];
    }
    let scene_for = |id: &str| Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 8.,
        slide: None,
        primitives: vec![Primitive::Image {
            blur: 0.,
            fit: ImageFit::Fill,
            clip_polygon: None,
            rect: Rect::new(0., 0., 8., 6.),
            path: std::path::PathBuf::from(format!("@icon:wallpaper:thumbnail:0:{id}")),
        }],
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 8., 6.),
    };

    let cached = lab.dir.join("cached.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([40, 70, 90, 255]))
        .save(&cached)
        .unwrap();
    wallpaper.state.borrow_mut().catalogue_result.push(json!({
        "id":"/fixture/cached.jpg","kind":"image","title":"Cached",
        "thumbnail":cached.to_string_lossy(),"preview":"","active":false
    }));
    let mut cached_scene = scene_for("/fixture/cached.jpg");
    wallpaper.attach_images(&mut cached_scene, Some(wake.clone()));
    assert!(
        cached_scene
            .asset_data
            .contains_key(&cached.canonicalize().unwrap())
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a valid persisted thumbnail is ready without a catalogue/prewarm request"
    );

    let mut scene = scene_for("/fixture/current.jpg");
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    assert!(
        scene.primitives.is_empty(),
        "deleted cache bytes are deferred while hydration runs"
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/current.jpg"]})
        )
    );
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap();
    let mut refreshed = scene_for("/fixture/current.jpg");
    wallpaper.attach_images(&mut refreshed, Some(wake.clone()));
    assert!(
        refreshed.asset_data.contains_key(&expected),
        "hydration restores an evicted/deleted thumbnail path"
    );

    let broken = lab.dir.join("still-broken.png");
    {
        let mut state = wallpaper.state.borrow_mut();
        state.catalogue_result.push(json!({
            "id":"/fixture/broken.jpg","kind":"image","title":"Broken",
            "thumbnail":broken.to_string_lossy(),"preview":"","active":false
        }));
    }
    let mut broken_scene = scene_for("/fixture/broken.jpg");
    wallpaper.attach_images(&mut broken_scene, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/broken.jpg"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut repeated = scene_for("/fixture/broken.jpg");
    wallpaper.attach_images(&mut repeated, Some(wake.clone()));
    assert!(repeated.primitives.is_empty());
    assert!(
        lab.calls.try_recv().is_err(),
        "a broken path returned by hydration is settled for the current visible set"
    );

    let mut changed_set = scene_for("/fixture/broken.jpg");
    changed_set.primitives.push(Primitive::Image {
        blur: 0.,
        fit: ImageFit::Fill,
        clip_polygon: None,
        rect: Rect::new(0., 0., 8., 6.),
        path: std::path::PathBuf::from("@icon:wallpaper:thumbnail:0:/fixture/current.jpg"),
    });
    wallpaper.attach_images(&mut changed_set, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/broken.jpg"]})
        ),
        "a changed visible set requests only the image missing from the local cache"
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut stable_changed = scene_for("/fixture/broken.jpg");
    stable_changed.primitives.push(Primitive::Image {
        blur: 0.,
        fit: ImageFit::Fill,
        clip_polygon: None,
        rect: Rect::new(0., 0., 8., 6.),
        path: std::path::PathBuf::from("@icon:wallpaper:thumbnail:0:/fixture/current.jpg"),
    });
    wallpaper.attach_images(&mut stable_changed, Some(wake));
    assert!(
        lab.calls.try_recv().is_err(),
        "the unchanged new visible set remains settled"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_latest_activation_survives_view_retirement() {
    use bead::render::{Rect, Scene};
    let mut lab = lab_with_activation_gate(vec!["wallpaper.active".into()], true).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let queue = extras.wallpaper_actions.as_ref().unwrap().clone();
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    extras.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        extras.project(&open, Some(wake.clone()));
        if extras.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        extras.state.borrow().catalogue_result.len(),
        4,
        "activation fixture catalogue is ready"
    );
    extras.act(
        "wallpaper:activate:/fixture/current.jpg",
        &open,
        wake.clone(),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/current.jpg"})
        )
    );
    extras.act("wallpaper:activate:/fixture/next.jpg", &open, wake.clone());
    assert_eq!(
        queue
            .lock()
            .unwrap()
            .inflight
            .as_ref()
            .map(|inflight| inflight.selection.id.as_str()),
        Some("/fixture/next.jpg"),
        "the latest click replaces the in-flight selection immediately"
    );

    let file = std::path::PathBuf::from("/cache/view-only.png");
    let bytes: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
    let weak = Arc::downgrade(&bytes);
    extras
        .state
        .borrow_mut()
        .images
        .insert(file.clone(), bytes.clone());
    let scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: BTreeMap::from([(file, bytes.clone())]),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 8.,
        slide: None,
        primitives: Vec::new(),
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 8., 8.),
    };
    drop(bytes);
    drop(scene);
    drop(extras);
    drop(wake);
    assert!(
        weak.upgrade().is_none(),
        "the activation queue must not retain view image data"
    );

    lab.activation_release.take().unwrap().send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/next.jpg"})
        ),
        "the new request proceeds after the serial fixture finishes the old one"
    );
    for _ in 0..200 {
        if queue.lock().unwrap().inflight.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let state = queue.lock().unwrap();
    assert!(state.inflight.is_none());
    assert_eq!(state.confirmed, Some(("extras-boot".into(), 18)));
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_activation_error_is_reported_without_retry_and_boot_change_clears_state() {
    let mut lab = lab_with_activation_behavior(vec!["wallpaper.active".into()], true, true).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let queue = extras.wallpaper_actions.as_ref().unwrap().clone();
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    extras.project(&open, Some(wake.clone()));
    let _ = lab.calls.recv().await.unwrap();
    for _ in 0..200 {
        extras.project(&open, Some(wake.clone()));
        if extras.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    extras.act(
        "wallpaper:activate:/fixture/current.jpg",
        &open,
        wake.clone(),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/current.jpg"})
        )
    );
    lab.activation_release.take().unwrap().send(()).unwrap();
    for _ in 0..200 {
        if queue.lock().unwrap().inflight.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let model = extras.project(&open, Some(wake));
    assert_eq!(
        model["extras.message"],
        Value::Text("Wallpaper selection failed".into())
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "failed activation does not retry without a newer user selection"
    );
    {
        let mut state = queue.lock().unwrap();
        assert!(state.inflight.is_none());
        state.confirmed = Some(("extras-boot".into(), 18));
        state.invalidate_boot(Some("new-boot"));
        assert!(state.confirmed.is_none());
        assert!(state.error.is_none());
    }
}

// Keep the fixture daemon schedulable while the UI performs synchronous staging.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wallpaper_runtime_search_and_next_button_dispatch_real_activation() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
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
    assert_eq!(
        runtime.session.stores.get("bead.focused"),
        Some(&Value::Text(String::new())),
        "wallpaper picker opens without assigning focus to search"
    );
    runtime.services(Box::new(|_| {}));
    assert!(runtime.viewport(1920, 1200));
    let loading_scene = runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("cold catalogue loading must publish its loading scene immediately");
    assert!(
        loading_scene.primitives.iter().any(|primitive| matches!(
            primitive,
            bead::render::Primitive::Text { text, .. } if text == "Loading wallpapers…"
        )),
        "the first cold frame must show the loading message"
    );
    assert!(runtime.commit());
    let mut first_content = None;
    for _ in 0..200 {
        if let Some(scene) = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap() {
            assert!(
                scene.asset_data.values().any(|bytes| !bytes.is_empty()),
                "first published content includes hydrated wallpaper bytes"
            );
            first_content = Some(scene);
            assert!(runtime.commit());
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let initial_scene =
        first_content.expect("hydration eventually publishes the first content frame");
    let frame = crate::placement::Frame::current();
    let usable_width = (1920. - frame.left - frame.right).max(1.);
    let usable_height = (1200. - frame.top - frame.bottom).max(1.);
    assert_eq!(
        runtime.session.stores.get("extras.viewport.width"),
        Some(&Value::Number(usable_width as f64))
    );
    assert_eq!(
        runtime.session.stores.get("extras.viewport.height"),
        Some(&Value::Number(usable_height as f64))
    );
    let carousel_root = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("close"))
        .expect("picker root has the generic close action");
    let carousel_rect = bead::ui::rect(&carousel_root.draw);
    let expected_carousel_width = 320_f32.max(1360_f32.min(usable_width - 96.));
    assert_eq!(
        (carousel_rect.w, carousel_rect.h),
        (expected_carousel_width, 320.)
    );
    assert!(
        runtime.scroll_regions().is_empty(),
        "carousel wheel ticks navigate once instead of expanding into smooth scrolling"
    );
    let motion = initial_scene
        .motion
        .as_ref()
        .expect("picker uses sliding panel motion");
    assert!(
        !motion.reveal,
        "wallpaper content slides without collapsing its children"
    );
    assert_eq!(motion.slide_distance, Some(carousel_rect.h));
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "extras.wallpaper.layout".into(),
            Value::Text("spotlight".into()),
        )]))
        .unwrap();
    let full_scene = runtime
        .session
        .relayout()
        .unwrap()
        .expect("fullscreen layout relayouts");
    runtime.session.commit(full_scene);
    assert_eq!(
        runtime.session.resolve("view.frame").unwrap(),
        Value::Bool(false),
        "Luau owns whether the spotlight joins the shell frame"
    );
    let fullscreen_root = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("close"))
        .expect("fullscreen picker root has the generic close action");
    let fullscreen_rect = bead::ui::rect(&fullscreen_root.draw);
    assert_eq!(
        (fullscreen_rect.w, fullscreen_rect.h),
        (usable_width, usable_height)
    );
    assert!(
        runtime.scroll_regions().is_empty(),
        "spotlight remains a discrete navigation surface"
    );
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "extras.wallpaper.layout".into(),
            Value::Text("hexagons".into()),
        )]))
        .unwrap();
    let hex_scene = runtime
        .session
        .relayout()
        .unwrap()
        .expect("hexagon layout relayouts");
    runtime.session.commit(hex_scene);
    assert!(
        !runtime.scroll_regions().is_empty(),
        "hexagon gallery retains its content-scroll region"
    );
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "extras.wallpaper.layout".into(),
            Value::Text("carousel".into()),
        )]))
        .unwrap();
    let carousel_scene = runtime
        .session
        .relayout()
        .unwrap()
        .expect("carousel relayouts");
    runtime.session.commit(carousel_scene);
    assert_eq!(
        runtime.session.resolve("view.frame").unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    let mut loaded = false;
    for _ in 0..200 {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        loaded = matches!(runtime.session.stores.get("extras.rows"),Some(Value::List(rows)) if rows.len()==4);
        if loaded {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        loaded,
        "the real picker must receive the asynchronous catalogue"
    );
    let first_card = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            matches!(&node.action_arg,
            Some(Value::Text(argument)) if argument.starts_with("wallpaper:select:"))
        })
        .expect("wallpaper card remains focusable");
    let first_card_rect = bead::ui::rect(&first_card.draw);
    assert!(
        !runtime.dismiss_at(
            first_card_rect.x + first_card_rect.w / 2.,
            first_card_rect.y + first_card_rect.h / 2.,
        ),
        "clicking a card must not dismiss the picker"
    );
    let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(method, "wallpaper.catalogue");
    assert!(!params["ids"].as_array().unwrap().is_empty());
    assert!(params["ids"].as_array().unwrap().len() <= 128);
    let fixture_image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap();
    let mut hydrated_scene =
        Some(initial_scene).filter(|scene| scene.asset_data.contains_key(&fixture_image));
    for _ in 0..200 {
        if hydrated_scene.is_some() {
            break;
        }
        let scene = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if scene
            .as_ref()
            .is_some_and(|scene| scene.asset_data.contains_key(&fixture_image))
        {
            hydrated_scene = scene;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        hydrated_scene.is_some(),
        "hydration must invalidate the Luau scene and attach bytes to a fresh runtime scene"
    );
    while let Ok((method, _)) = lab.calls.try_recv() {
        assert_eq!(
            method, "wallpaper.catalogue",
            "all catalogue hydration calls are settled before activation assertions"
        );
    }
    let search = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id.ends_with(".search") && node.input.is_some())
        .expect("wallpaper search input is visible");
    let search_rect = bead::ui::rect(&search.draw);
    runtime
        .stage(UiEvent::Click {
            x: search_rect.x + search_rect.w / 2.,
            y: search_rect.y + search_rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    runtime
        .stage(UiEvent::Scroll {
            x: search_rect.x + search_rect.w / 2.,
            y: search_rect.y + search_rect.h / 2.,
            delta: 15.,
        })
        .unwrap();
    runtime.commit();
    assert!(
        lab.calls.try_recv().is_err(),
        "scrolling the search toolbar does not navigate wallpapers"
    );

    runtime
        .stage(UiEvent::Key {
            text: "Next".into(),
            backspace: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("extras.query"),
        Some(&Value::Text("Next".into()))
    );
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/next.jpg"})
        )
    );

    for _ in 0..4 {
        runtime
            .stage(UiEvent::Key {
                text: String::new(),
                backspace: true,
            })
            .unwrap();
        runtime.commit();
    }
    assert_eq!(
        runtime.session.stores.get("extras.query"),
        Some(&Value::Text(String::new()))
    );
    let navigation = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("extras.next"))
        .expect("picker exposes an actionable next button");
    let rect = bead::ui::rect(&navigation.draw);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("extras.wallpaper.selection_id"),
        Some(&Value::Text("/fixture/current.jpg".into()))
    );
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":18,"id":"/fixture/current.jpg"})
        )
    );
    let card = runtime.session.view_nodes().iter().find(|node| {
        let rect = bead::ui::rect(&node.draw);
        matches!(&node.action_arg, Some(Value::Text(argument)) if argument.starts_with("wallpaper:select:"))
            && node.contains(rect.x + rect.w / 2., rect.y + rect.h / 2.)
    }).expect("a visible wallpaper card during selection travel");
    let card = bead::ui::rect(&card.draw);
    runtime
        .stage(UiEvent::Scroll {
            x: card.x + card.w / 2.,
            y: card.y + card.h / 2.,
            delta: 15.,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":19,"id":"/fixture/next.jpg"})
        ),
        "positive Wayland wheel distance moves to the next wallpaper"
    );
    let root_node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("close"))
        .expect("wallpaper root uses the generic host close action");
    let root_rect = bead::ui::rect(&root_node.draw);
    let point = [(0.05, 0.15), (0.95, 0.15), (0.05, 0.85), (0.95, 0.85)]
        .into_iter()
        .map(|(x, y)| (root_rect.x + root_rect.w * x, root_rect.y + root_rect.h * y))
        .find(|(x, y)| {
            runtime
                .session
                .view_nodes()
                .iter()
                .filter(|node| node.action.is_some() && node.action.as_deref() != Some("close"))
                .all(|node| {
                    let rect = bead::ui::rect(&node.draw);
                    *x < rect.x || *x > rect.x + rect.w || *y < rect.y || *y > rect.y + rect.h
                })
        })
        .expect("picker has a background point outside child actions");
    assert!(
        runtime.dismiss_at(point.0, point.1),
        "the empty background still dismisses the picker"
    );
    runtime
        .stage(UiEvent::Click {
            x: point.0,
            y: point.1,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(false))
    );
}

async fn next_wallpaper_runtime_call(
    runtime: &mut crate::reload::Runtime,
    calls: &mut mpsc::UnboundedReceiver<(String, Json)>,
) -> (String, Json) {
    use bead::wayland::{SceneSource, UiEvent};
    for _ in 0..200 {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if let Ok(call) = calls.try_recv() {
            if call.0 == "wallpaper.catalogue" {
                let ids = call.1["ids"]
                    .as_array()
                    .expect("hydration call carries ids");
                assert!(!ids.is_empty(), "metadata loading uses ids:[] only once");
                assert!(
                    ids.len() <= 128,
                    "hydration request stays within protocol bounds"
                );
                continue;
            }
            return call;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("wallpaper runtime did not issue its action")
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_thumbnail_cache_is_advertised_bounded_and_released_on_close() {
    use bead::render::{ImageFit, Primitive, Rect, Scene};
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let thumbnail = lab.dir.join("thumbnail.png");
    let unadvertised = lab.dir.join("unadvertised.png");
    let oversized = lab.dir.join("oversized.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([12, 120, 200, 255]))
        .save(&thumbnail)
        .unwrap();
    image::RgbaImage::from_pixel(8, 6, image::Rgba([200, 60, 30, 255]))
        .save(&unadvertised)
        .unwrap();
    image::RgbaImage::from_pixel(1601, 1, image::Rgba([20, 30, 40, 255]))
        .save(&oversized)
        .unwrap();
    extras.state.borrow_mut().catalogue_result = vec![
        json!({
            "id":"/fixture/advertised.jpg","kind":"image","title":"Advertised",
            "thumbnail":thumbnail.to_string_lossy(),"preview":"","active":false
        }),
        json!({
            "id":"/fixture/oversized.jpg","kind":"image","title":"Oversized",
            "thumbnail":oversized.to_string_lossy(),"preview":"","active":false
        }),
    ];
    {
        let mut state = extras.state.borrow_mut();
        state.wallpaper_open = true;
        state.wallpaper_boot = Some("extras-boot".into());
        state.catalogue_boot = Some("extras-boot".into());
        state.catalogue_request_boot = Some("extras-boot".into());
        state.image_revision = 9;
        // A previously visible file must not remain warm after leaving the scene.
        assert!(
            cached_wallpaper_image(&mut state, &unadvertised, 16 * bead::core::cache::MIB)
                .is_some()
        );
    }

    let logical =
        |field: &str, id: &str| std::path::PathBuf::from(format!("@icon:wallpaper:{field}:{id}"));
    let mut scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 32.,
        slide: None,
        primitives: vec![
            Primitive::Image {
                blur: 0.,
                fit: ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(0., 0., 8., 6.),
                path: logical("thumbnail", "/fixture/advertised.jpg"),
            },
            Primitive::Image {
                blur: 0.,
                fit: ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(10., 0., 8., 6.),
                path: logical("thumbnail", "/fixture/unadvertised.jpg"),
            },
            Primitive::Image {
                blur: 0.,
                fit: ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(20., 0., 8., 6.),
                path: logical("thumbnail", "/fixture/oversized.jpg"),
            },
        ],
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 32., 8.),
    };
    extras.attach_images(&mut scene, None);
    assert!(scene.asset_data.contains_key(&thumbnail));
    assert_eq!(extras.state.borrow().images.len(), 1);
    assert!(extras.image_bytes() > 0 && extras.image_bytes() <= 16 * bead::core::cache::MIB);
    let bead::render::Primitive::Image { path: loaded, .. } = &scene.primitives[0] else {
        panic!("image primitive")
    };
    assert_eq!(loaded, &thumbnail);
    assert_eq!(
        scene.primitives.len(),
        1,
        "unadvertised and invalid images are deferred out of the scene"
    );

    let opened = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.query".into(), Value::Text("Advertised".into())),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/fixture/advertised.jpg".into()),
            ),
            ("extras.wallpaper.kind".into(), Value::Text("static".into())),
        ]))
        .unwrap();
    let opened_model = extras.project(&opened, None);
    let generation = opened_model["extras.wallpaper.resource_revision"].clone();
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(false)),
            ("extras.query".into(), Value::Text("Advertised".into())),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/fixture/advertised.jpg".into()),
            ),
            ("extras.wallpaper.kind".into(), Value::Text("static".into())),
        ]))
        .unwrap();
    let closing_model = extras.project(&closed, None);
    assert!(matches!(&closing_model["extras.rows"], Value::List(rows) if rows.len()==2));
    assert_eq!(closing_model["extras.available"], Value::Bool(true));
    assert_eq!(
        closing_model["extras.wallpaper.resource_revision"],
        generation
    );
    assert!(
        !closing_model.contains_key("extras.query"),
        "query state remains unchanged during the slide-out"
    );
    assert!(!closing_model.contains_key("extras.wallpaper.selection_id"));
    assert!(
        extras.image_bytes() > 0,
        "encoded thumbnail bytes remain leased through the closing frame"
    );
    let weak = Arc::downgrade(scene.asset_data.values().next().unwrap());
    drop(scene);
    drop(extras);
    assert!(
        weak.upgrade().is_none(),
        "retiring Extras releases cached thumbnail bytes"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_scene_attachment_bytes_stay_within_the_cache_budget() {
    use bead::render::{ImageFit, Primitive, Rect, Scene};
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let shared: Arc<[u8]> = Arc::from(vec![0u8; 9 * bead::core::cache::MIB]);
    let entries: Vec<Json> = (0..3)
        .map(|index| {
            json!({
                "id":format!("/fixture/large-{index}.png"),"kind":"image",
                "title":format!("Large {index}"),"thumbnail":format!("/cache/thumb-{index}.png"),
                "preview":"","active":false
            })
        })
        .collect();
    {
        let mut state = extras.state.borrow_mut();
        state.wallpaper_open = true;
        state.catalogue_result = entries;
        for index in 0..3 {
            state.images.insert(
                std::path::PathBuf::from(format!("/cache/thumb-{index}.png")),
                shared.clone(),
            );
        }
    }
    let mut scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 24.,
        slide: None,
        primitives: (0..3)
            .map(|index| Primitive::Image {
                blur: 0.,
                fit: ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(index as f32 * 8., 0., 8., 6.),
                path: std::path::PathBuf::from(format!(
                    "@icon:wallpaper:thumbnail:/fixture/large-{index}.png"
                )),
            })
            .collect(),
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 24., 8.),
    };
    extras.attach_images(&mut scene, None);
    let attached_bytes: usize = scene.asset_data.values().map(|bytes| bytes.len()).sum();
    assert!(attached_bytes <= 16 * bead::core::cache::MIB);
    assert_eq!(
        scene.asset_data.len(),
        1,
        "the second shared large image would exceed the per-scene byte budget"
    );
}
#[test]
fn extras_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    for kind in [Kind::Overview, Kind::Palette, Kind::Keybindings] {
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
#[test]
fn wallpaper_picker_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let thumbnail = "wallpaper:thumbnail:0:/fixture/sample.png".to_owned();
    for (kind, layout) in [
        ("static", "carousel"),
        ("dynamic", "carousel"),
        ("static", "spotlight"),
        ("dynamic", "spotlight"),
        ("static", "hexagons"),
        ("dynamic", "hexagons"),
    ] {
        for scale in [1., 1.5] {
            let rows=Value::list((0..12).map(|index| json!({
                "id":format!("/fixture/wallpaper-{index:02}"),
                "kind":if index<10 {"image"} else {"video"},
                "title":format!("Fixture wallpaper {index:02}"),
                "thumbnail":thumbnail.clone(),"preview":thumbnail.clone(),"active":kind=="static" && index==5
            })).collect());
            let stores = BTreeMap::from([
                ("panel.open".into(), Value::Bool(true)),
                ("extras.kind".into(), Value::Text("wallpaper".into())),
                ("extras.available".into(), Value::Bool(true)),
                ("extras.message".into(), Value::Text(String::new())),
                ("extras.query".into(), Value::Text(String::new())),
                ("extras.rows".into(), rows),
                ("extras.wallpaper.kind".into(), Value::Text(kind.into())),
                ("extras.wallpaper.layout".into(), Value::Text(layout.into())),
                ("extras.wallpaper.loading".into(), Value::Bool(false)),
                (
                    "extras.wallpaper.selection_id".into(),
                    Value::Text(String::new()),
                ),
                ("extras.viewport.width".into(), Value::Number(1216.)),
                ("extras.viewport.height".into(), Value::Number(936.)),
            ]);
            let mut harness = bead_test::Harness::load_with_icons(
                &root,
                std::path::Path::new("extras/main.luau"),
                &palette,
                stores,
                bead_test::Config {
                    width: 1280,
                    height: 1000,
                    scale,
                },
                BTreeMap::from([(thumbnail.clone(), root.join("sample.png"))]),
            )?;
            assert!(harness.scene().primitives.iter().any(|primitive|
                matches!(primitive, bead::render::Primitive::Text { text, .. } if text == "Static")),
                "picker controls must render before approving a golden");
            assert!(harness.scene().primitives.iter().any(|primitive|
                matches!(primitive, bead::render::Primitive::Image { path, .. } if path.file_name().is_some_and(|name| name == "sample.png"))),
                "populated picker must render wallpaper thumbnails");
            if layout == "spotlight" {
                assert!(!harness.scene().primitives.iter().any(|primitive| matches!(primitive,
                    bead::render::Primitive::Image { rect, .. } if rect.w >= 1216. && rect.h >= 936.)),
                    "spotlight must leave the live desktop behind its translucent scrim");
            }
            let frame = harness.render()?;
            for primitive in &harness.scene().primitives {
                if let bead::render::Primitive::Text {
                    text,
                    rect,
                    family,
                    size,
                    ..
                } = primitive
                    && ["Static", "Dynamic", "Carousel", "Spotlight", "Hexagons"]
                        .contains(&text.as_str())
                {
                    assert_eq!(
                        bead::render::text_lines(harness.scene(), text, family, *size, rect.w)?,
                        1,
                        "toolbar label {text} must fit one visible line"
                    );
                }
            }
            let path = root.join(format!(
                "tests/goldens/wallpaper-picker-{layout}-{kind}-{scale}.png"
            ));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&path)?;
            }
            frame.compare(
                &path,
                Default::default(),
                &std::env::temp_dir()
                    .join(format!("wallpaper-picker-{layout}-{kind}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}
#[test]
fn wallpaper_picker_uses_active_middle_then_visible_query_and_pointer_selection()
-> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = bead::render::Palette::read(&root.join("palette.json"))?;
    let fallback = crate::sound::fallback(palette.get("surface")?);
    let mut session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        palette,
        fallback,
    );
    let initial = session.reload()?;
    session.check_sources()?;
    session.commit(initial);
    let rows = Value::list(vec![
        json!({"id":"/wallpapers/first.jpg","kind":"image","title":"First","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/current.jpg","kind":"image","title":"Current middle","thumbnail":"","preview":"","active":true}),
        json!({"id":"/wallpapers/garden.jpg","kind":"image","title":"Garden path","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/last.jpg","kind":"image","title":"Last","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/animated.gif","kind":"image","animated":true,"title":"Animated image","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/movie.mp4","kind":"video","animated":false,"title":"Movie","thumbnail":"","preview":"","active":false}),
    ]);
    let view = session
        .update(BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
            ("extras.available".into(), Value::Bool(true)),
            ("extras.message".into(), Value::Text(String::new())),
            ("extras.rows".into(), rows),
            ("extras.wallpaper.kind".into(), Value::Text("static".into())),
            (
                "extras.wallpaper.layout".into(),
                Value::Text("carousel".into()),
            ),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text(String::new()),
            ),
            ("extras.viewport.width".into(), Value::Number(1280.)),
            ("extras.viewport.height".into(), Value::Number(900.)),
        ]))?
        .expect("wallpaper view update");
    session.commit(view);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:activate:/wallpapers/current.jpg".into())
    );
    let query = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text("garden".into()),
        )]))?
        .expect("search update");
    session.commit(query);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:activate:/wallpapers/garden.jpg".into())
    );
    let query = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )]))?
        .expect("clear search");
    session.commit(query);
    let select =
        session.action_values("extras.select", Value::Text("/wallpapers/last.jpg".into()))?;
    let selected = session.update(select)?.expect("pointer selection update");
    session.commit(selected);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:activate:/wallpapers/last.jpg".into()),
        "pointer selection controls the next keyboard activation"
    );
    let dynamic = session
        .update(BTreeMap::from([(
            "extras.wallpaper.kind".into(),
            Value::Text("dynamic".into()),
        )]))?
        .expect("dynamic filter update");
    session.commit(dynamic);
    let behavior_path = root.join("extras/behavior.luau");
    let behavior =
        bead::ui::script::Script::load(&std::fs::read(&behavior_path)?, &behavior_path).unwrap();
    let Value::List(dynamic_rows) = behavior
        .resolve("extras.filtered", &session.stores)
        .unwrap()
    else {
        panic!("dynamic wallpaper filter returns rows")
    };
    let dynamic_ids: std::collections::BTreeSet<&str> = dynamic_rows
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(
        dynamic_ids,
        std::collections::BTreeSet::from(["/wallpapers/animated.gif", "/wallpapers/movie.mp4"])
    );
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:activate:/wallpapers/movie.mp4".into()),
        "a media filter clamps the prior ordinal when its selected/active ID is absent"
    );
    // Opening on the final item must take one forward step across the seam.
    let reset = session
        .update(BTreeMap::from([
            ("extras.wallpaper.kind".into(), Value::Text("static".into())),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/wallpapers/last.jpg".into()),
            ),
            (
                "extras.wallpaper.carousel_position".into(),
                Value::Number(1.),
            ),
        ]))?
        .unwrap();
    session.commit(reset);
    let next = session.action_values("extras.next", Value::Bool(true))?;
    assert_eq!(
        next["extras.wallpaper.carousel_position"],
        Value::Number(5.)
    );
    let next = session.update(next)?.unwrap();
    session.commit(next);
    let reset = session
        .update(BTreeMap::from([
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/wallpapers/first.jpg".into()),
            ),
            (
                "extras.wallpaper.carousel_position".into(),
                Value::Number(1.),
            ),
        ]))?
        .unwrap();
    session.commit(reset);
    let previous = session.action_values("extras.prev", Value::Bool(true))?;
    assert_eq!(
        previous["extras.wallpaper.carousel_position"],
        Value::Number(0.),
        "zero is a valid unwrapped position when moving left across the seam"
    );
    Ok(())
}

#[cfg(feature = "trace")]
pub(crate) async fn retirement() {
    // Exercise page-owned leases. A permanently requested wallpaper topic belongs
    // to the shared renderer/client and correctly survives any picker closing.
    let lab = lab().await;
    assert!(lab.client.topic("wallpaper.active").is_err());
    for kind in [Kind::Overview, Kind::Keybindings, Kind::Wallpaper] {
        let extras = Extras::new(kind, lab.client.clone()).unwrap();
        if kind == Kind::Wallpaper {
            let open = Stores::default()
                .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))
                .unwrap();
            extras.project(&open, Some(Arc::new(|| {})));
            let topic = extras.topic.as_ref().unwrap();
            let mut ready = false;
            for _ in 0..200 {
                let snapshot = topic.snapshot();
                ready = snapshot.connected && snapshot.data["revision"] == 17;
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert!(
                ready,
                "{} fixture publishes the active wallpaper topic",
                kind.name()
            );
        } else {
            ready(&extras, &stores("")).await;
        }
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
            "closed {} extras must release topic data",
            kind.name()
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
            Kind::Wallpaper => unreachable!("wallpaper uses the catalogue fixture"),
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

#[tokio::test(flavor = "current_thread")]
async fn previews_lease_only_while_open_and_with_the_setting_and_attach_the_newest_thumbnail() {
    use bead::wayland::{CaptureHub, CapturePreview};
    let lab = lab().await;
    let dir = lab.dir.join("capture");
    let hub = CaptureHub::test_hub(dir.clone());
    let extras = Extras::new(Kind::Overview, lab.client.clone())
        .unwrap()
        .with_capture(hub.clone());
    let with_open = |open: bool| {
        Stores::default()
            .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(open))]))
            .unwrap()
    };
    let wake: CallWake = Arc::new(|| {});
    let rows = |model: &BTreeMap<String, Value>| match &model["extras.windows"] {
        Value::List(rows) => rows.clone(),
        other => panic!("{other:?}"),
    };
    for _ in 0..200 {
        if extras.project(&with_open(false), None)["extras.available"] == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(hub.lease_count(), 0, "closed: nothing leased");
    let model = extras.project(&with_open(true), Some(wake.clone()));
    assert_eq!(
        hub.lease_count(),
        1,
        "open: one lease for the listed windows"
    );
    assert!(
        rows(&model)[0].get("preview").is_none(),
        "no frame yet: icon fallback"
    );
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("w1-1.png");
    std::fs::write(&path, b"x").unwrap();
    hub.publish_for_test(
        "win1",
        CapturePreview {
            path: path.clone(),
            width: 160,
            height: 100,
        },
    );
    let model = extras.project(&with_open(true), Some(wake));
    let row = &rows(&model)[0];
    assert_eq!(row["preview"], path.to_string_lossy().as_ref());
    assert_eq!(
        (row["preview_width"].clone(), row["preview_height"].clone()),
        (160.into(), 100.into())
    );
    assert_eq!(hub.lease_count(), 1, "re-projecting reuses the lease");
    extras.project(&with_open(false), None);
    assert_eq!(hub.lease_count(), 0, "closing releases the lease");
    assert!(
        hub.preview("win1").is_none() && !path.exists(),
        "thumbnails go with it"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn picker_preferences_use_provider_lane_and_preserve_the_document() {
    let mut lab = lab_with_topics(vec![crate::prefs::WALLPAPER_PICKER.into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let wake: CallWake = Arc::new(|| {});
    for _ in 0..200 {
        if wallpaper.wallpaper_preference("kind", "dynamic").is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    for (key, value) in [
        ("layout", "spotlight"),
        ("layout", "hexagons"),
        ("kind", "dynamic"),
    ] {
        wallpaper.act(
            &format!("wallpaper:{key}:{value}"),
            &Stores::default(),
            wake.clone(),
        );
        let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(method, "settings.provider");
        assert_eq!(params["store"], "wallpaper-picker");
        assert_eq!(params["rev"], 3);
        assert_eq!(params["value"]["version"], 1);
        assert_eq!(params["value"][key], value);
        assert_eq!(params["value"]["paused"], true);
        assert_eq!(params["value"]["future"], "preserve");
        wallpaper.state.borrow_mut().action = None;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn failed_activation_retires_its_early_preview_outside_the_view() {
    let mut lab = lab_with_activation_behavior(vec!["wallpaper.active".into()], false, true).await;
    let preview = Arc::new(crate::wallpaper::Preview::default());
    let generation = preview.select(
        "extras-boot".into(),
        "/fixture/next.jpg".into(),
        "image".into(),
        None,
    );
    let mut admission = WallpaperActivationQueue::new(lab.client.clone());
    admission.preview = Some(preview.clone());
    let queue = Arc::new(std::sync::Mutex::new(admission));
    let wake: CallWake = Arc::new(|| {});
    assert!(WallpaperActivationQueue::submit(
        &queue,
        WallpaperSelection {
            id: "/fixture/next.jpg".into(),
            boot: "extras-boot".into(),
            revision: 17,
            selected_at: std::time::Instant::now(),
            file_bytes: 0,
            source_width: 0,
            source_height: 0,
        },
        &wake
    ));
    assert_eq!(
        preview.generation_for("extras-boot", "/fixture/next.jpg"),
        Some(generation)
    );
    let (method, _) = lab.calls.recv().await.unwrap();
    assert_eq!(method, "wallpaper.activate");
    for _ in 0..200 {
        if preview
            .generation_for("extras-boot", "/fixture/next.jpg")
            .is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        preview.generation_for("extras-boot", "/fixture/next.jpg"),
        None
    );
    assert_eq!(
        queue.lock().unwrap().error(),
        Some("Wallpaper selection failed".into())
    );
}

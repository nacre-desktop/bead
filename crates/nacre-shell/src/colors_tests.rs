// SPDX-License-Identifier: GPL-3.0-only
//! B0060: the Colors app against fixtures and a fake daemon: the capability gate, the
//! one-at-a-time preview loop, apply/export, cache images and goldens.
use super::*;
use crate::nacre_source::Config;
use bead_test::{Config as HarnessConfig, Harness, source::FakeSource};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
/// The shell palette's colours; `shift` rotates the accent channels so styles differ.
pub(crate) fn colours(shift: u8) -> Json {
    let mut map: Map<String, Json> =
        serde_json::from_slice::<Json>(&std::fs::read(root().join("palette.json")).unwrap())
            .unwrap()["colours"]
            .as_object()
            .unwrap()
            .clone();
    for role in ["primary", "secondary", "tertiary", "primaryContainer"] {
        let hex = map[role].as_str().unwrap().to_owned();
        let turned = match shift / 30 {
            1 => format!("{}{}{}", &hex[2..4], &hex[4..6], &hex[0..2]),
            2 => format!("{}{}{}", &hex[4..6], &hex[0..2], &hex[2..4]),
            3 => format!("{0}{0}{0}", &hex[2..4]),
            _ => hex,
        };
        map.insert(role.into(), turned.into());
    }
    Json::Object(map)
}
/// A daemon-shaped preview result for `image`.
fn preview(id: &str, image: &str, poster: &str, request: &Json) -> Json {
    json!({
        "id": id, "request": request, "name": "Mountain lake", "image": image, "thumbnail": poster,
        "frames": 1, "favorite": false, "otherCoverage": 0.12,
        "imageColors": [
            {"hex": "3a6ea5", "coverage": 0.42}, {"hex": "d9a441", "coverage": 0.21},
            {"hex": "2f4f3a", "coverage": 0.14}, {"hex": "c8c8c8", "coverage": 0.06},
            {"hex": "8a3b4d", "coverage": 0.05}],
        "colours": colours(0),
        "audit": {"textPasses": true, "minimumTextContrast": 7.123, "warnings": ["Tertiary on surface is low."],
            "text": [{"foreground": "e8eaed", "background": "14161a", "ratio": 14.2}],
            "simulated": {"protan": colours(30), "deutan": colours(60), "tritan": colours(90)}},
        "comparisons": [{"personality": "natural", "colours": colours(0), "minimumContrast": 7.123},
            {"personality": "vivid", "colours": colours(30), "minimumContrast": 5.5},
            {"personality": "mist", "colours": colours(60), "minimumContrast": 4.5}, {"personality": "pearl", "colours": colours(90), "minimumContrast": 6.0}],
    })
}
pub(crate) fn cache(root: &Path, dir: &str, name: &str, w: u32, h: u32) -> String {
    let dir = root.join(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let image = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 140, 255])
    });
    image
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path.to_str().unwrap().into()
}
pub(crate) fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("colors-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn surface() -> String {
    colours(0)["surface"].as_str().unwrap().into()
}
fn doc(data: Json) -> Json {
    json!({"store": "colors", "rev": 3, "exists": true, "data": data, "dirty": false})
}
fn active(path: &str, surface: &str) -> Json {
    let mut map = colours(0);
    map["surface"] = surface.into();
    json!({"available": true, "stale": false, "revision": 1, "outputs": {},
        "current": {"revision": 1, "path": path, "palette": {"mode": "light", "colours": map}}})
}
fn sources(data: Json, active: Json) -> (Arc<FakeSource>, Arc<FakeSource>) {
    (
        Arc::new(FakeSource::new(doc(data)).unwrap()),
        Arc::new(FakeSource::new(active).unwrap()),
    )
}
fn stores(page: &str, query: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("colors.page".into(), Value::Text(page.into())),
            ("colors.query".into(), Value::Text(query.into())),
            ("colors.vision".into(), Value::Text("normal".into())),
            ("colors.fine".into(), Value::Bool(false)),
            ("colors.open".into(), Value::Text(String::new())),
        ]))
        .unwrap()
}
fn view(model: &BTreeMap<String, Value>) -> Json {
    match &model["colors.view"] {
        Value::List(items) => items[0].clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn without_the_methods_the_active_palette_is_shown_and_nothing_can_be_applied() {
    crate::i18n::init();
    let (store, active) = sources(json!({}), active("/home/me/Pictures/lake.jpg", &surface()));
    let colors = Colors::new(None).with_sources(store, active);
    let model = colors.project(&stores("studio", ""));
    let v = view(&model);
    assert_eq!(v["available"], false);
    assert_eq!(v["title"], "lake.jpg");
    assert_eq!(
        v["subtitle"],
        "Colour previews are not available from this Nacre daemon."
    );
    assert_eq!(v["mode"], "light");
    assert_eq!(v["samples"].as_array().unwrap().len(), 5);
    assert_eq!(v["samples"][0]["caption"], "primary");
    assert_eq!(v["targets"].as_array().unwrap().len(), 3);
    assert_eq!(
        (v["can_apply"].clone(), v["can_export"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(v["status"], "");
    // An action with no daemon changes nothing and never panics.
    let changes = colors.act("apply:", &stores("studio", ""));
    assert!(changes.is_empty());
    assert_eq!(
        model["colors.empty"],
        Value::Bool(false),
        "no list and no daemon is not an empty search"
    );
}

#[test]
fn page_choice_and_fine_tune_are_view_state_and_unknown_input_is_ignored() {
    crate::i18n::init();
    let (store, active) = sources(json!({}), active("/p/a.png", &surface()));
    let colors = Colors::new(None).with_sources(store, active);
    let at = stores("studio", "");
    assert_eq!(
        colors.act("page:compare", &at)["colors.page"],
        Value::Text("compare".into())
    );
    assert!(colors.act("page:nope", &at).is_empty());
    assert_eq!(colors.act("fine:", &at)["colors.fine"], Value::Bool(true));
    assert_eq!(
        colors.act("vision:tritan", &at)["colors.vision"],
        Value::Text("tritan".into())
    );
    let model = colors.project(&stores("elsewhere", ""));
    assert_eq!(model["colors.page"], Value::Text("studio".into()));
    // Hostile numbers and paths are refused before a request is built.
    assert!(colors.act("brightness:NaN", &at).is_empty());
    assert!(colors.act("sample:not-hex", &at).is_empty());
    assert!(colors.act("choose:99", &at).is_empty());
}

#[test]
fn colour_helpers_accept_only_six_digit_hex() {
    assert_eq!(rgba("#3a6ea5"), Some([0x3a, 0x6e, 0xa5, 255]));
    assert_eq!(rgba("3A6EA5"), Some([0x3a, 0x6e, 0xa5, 255]));
    assert_eq!(rgba("3a6ea"), None);
    assert_eq!(rgba("3a6eaz"), None);
    assert_eq!(hex6("#3A6EA5").as_deref(), Some("3a6ea5"));
    assert_eq!(
        colour_map(&json!({"a": "ffffff", "b": "bad"}), &[]),
        json!({"a": [255, 255, 255, 255]})
    );
}

// ---- a fake daemon ----------------------------------------------------------

struct Daemon {
    calls: tokio::sync::mpsc::UnboundedReceiver<(String, Json)>,
    /// One permit lets one `colors.preview` reply go out.
    gate: Arc<tokio::sync::Semaphore>,
    updates: tokio::sync::mpsc::UnboundedSender<(String, Json)>,
    server: tokio::task::JoinHandle<()>,
    dir: PathBuf,
    client: Client,
}
async fn daemon(name: &str, cache_root: &Path, with_methods: bool) -> Daemon {
    use nacre_protocol::{Capability, Message, Reply, framing};
    use std::os::unix::fs::PermissionsExt;
    use tokio::{io::BufReader, net::UnixListener, sync::mpsc};
    let dir = temp(name);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let (updates, mut updates_rx) = mpsc::unbounded_channel::<(String, Json)>();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let held = gate.clone();
    let applies = std::sync::atomic::AtomicUsize::new(0);
    let thumb = cache(cache_root, "nacre/shell/colors-thumbs", "t0.png", 64, 40);
    let poster = cache(cache_root, "nacre/shell/colors-preview", "p0.png", 320, 180);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        let mut methods = nacre_protocol::core_methods();
        if with_methods {
            for name in [CATALOG, PREVIEW, APPLY, EXPORT] {
                methods.insert(
                    name.into(),
                    nacre_protocol::Method {
                        v: 1,
                        class: Capability::Desktop,
                        timeout: 10_000,
                        params: json!({"type": "object"}),
                        result: json!({}),
                    },
                );
            }
        }
        let welcome = Message::Welcome {
            proto: 1,
            min: 1,
            boot: "colors-boot".into(),
            topics: [(STORE.to_string(), 1), (ACTIVE.to_string(), 1)].into(),
            methods,
            frame_ms: None,
        };
        framing::write(&mut writer, &welcome).await.unwrap();
        let mut seq = 0;
        let mut topic_data: BTreeMap<String, Json> = BTreeMap::from([
            (
                STORE.to_string(),
                doc(json!({"wallpapers": {}, "lastApplied": ""})),
            ),
            (ACTIVE.to_string(), active("/home/me/lake.jpg", &surface())),
        ]);
        // N0050: one preview at a time; a newer one cancels the waiting older (it fails `busy`).
        let mut waiting: Option<(_, Json)> = None;
        loop {
            tokio::select! {
                permit = held.clone().acquire_owned(), if waiting.is_some() => {
                    permit.unwrap().forget();
                    let (id, p) = waiting.take().unwrap();
                    let ok = if p["request"]["personality"] == "pop" {
                        let failure = nacre_protocol::Failure::new(
                            nacre_protocol::ErrorCode::Validation,
                            "Choose a local wallpaper",
                            false,
                        );
                        framing::write(&mut writer, &Message::Ret { id, reply: Reply::Err { err: failure } }).await.unwrap();
                        continue;
                    } else {
                        preview("p1", p["request"]["image"].as_str().unwrap(), &poster, &p["request"])
                    };
                    framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok } }).await.unwrap();
                }
                update = updates_rx.recv() => {
                    let Some((topic, data)) = update else { break };
                    seq += 1;
                    topic_data.insert(topic.clone(), data.clone());
                    framing::write(&mut writer, &Message::Snap { topic, seq, data }).await.unwrap();
                }
                message = reader.next() => {
                    let Ok(Some(message)) = message else { break };
                    match message {
                        Message::Sub { topics } => {
                            for topic in topics {
                                let data = topic_data.get(&topic).cloned().unwrap_or(Json::Null);
                                framing::write(&mut writer, &Message::Snap { topic, seq, data }).await.unwrap();
                            }
                        }
                        Message::Call { id, m, p, .. } => {
                            calls_tx.send((m.clone(), p.clone())).unwrap();
                            let ok = match m.as_str() {
                                CATALOG => json!({"current": "/home/me/lake.jpg", "favorites": [], "history": [], "choices": {},
                                    "entries": [{"path": "/home/me/lake.jpg", "name": "Mountain lake", "dynamic": false, "thumbnail": thumb},
                                                {"path": "/home/me/loop.mp4", "name": "Rain loop", "dynamic": true, "thumbnail": ""}]}),
                                PREVIEW => {
                                    if let Some((old, _)) = waiting.replace((id, p)) {
                                        let failure = nacre_protocol::Failure::new(nacre_protocol::ErrorCode::Busy, "replaced by a newer preview", true);
                                        framing::write(&mut writer, &Message::Ret { id: old, reply: Reply::Err { err: failure } }).await.unwrap();
                                    }
                                    continue;
                                }
                                APPLY if applies.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 => {
                                    let failure = nacre_protocol::Failure::new(nacre_protocol::ErrorCode::Busy, "another action is running", true);
                                    framing::write(&mut writer, &Message::Ret { id, reply: Reply::Err { err: failure } }).await.unwrap();
                                    continue;
                                }
                                APPLY => json!({"applied": true, "name": "Mountain lake"}),
                                EXPORT => json!({"directory": "/home/me/exports/lake", "files": ["a.css"]}),
                                _ => Json::Null,
                            };
                            framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok } }).await.unwrap();
                        }
                        _ => {}
                    }
                }
            }
        }
    });
    let mut topics = vec![STORE.to_string(), ACTIVE.to_string()];
    topics.sort();
    let client = Client::start(Config::new(socket, topics)).unwrap();
    for _ in 0..500 {
        if client.snapshot().connected && client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Daemon {
        calls,
        gate,
        updates,
        server,
        dir,
        client,
    }
}
async fn until(colors: &Colors, at: &Stores, done: impl Fn(&Json) -> bool) -> Json {
    for _ in 0..600 {
        let v = view(&colors.project(at));
        if done(&v) {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition never held: {:#}", view(&colors.project(at)));
}

/// The next call the daemon sees; projecting is what polls finished calls and sends the next one.
async fn next_call(colors: &Colors, at: &Stores, d: &mut Daemon) -> (String, Json) {
    for _ in 0..600 {
        if let Ok(call) = d.calls.try_recv() {
            return call;
        }
        colors.project(at);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no call reached the daemon");
}

#[tokio::test(flavor = "current_thread")]
async fn the_explorer_loads_previews_one_at_a_time_and_applies_over_the_wire() {
    crate::i18n::init();
    let tmp = temp("cache");
    // The adapter accepts cache images by directory name, so the fake daemon writes under a temp root.
    let mut d = daemon("wire-explorer", &tmp, true).await;
    let colors = Colors::new(Some(d.client.clone()));
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    colors.subscribe(Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let at = stores("studio", "");
    // Catalog first, then a preview of the current wallpaper with the active mode.
    let v = until(&colors, &at, |v| {
        v["busy"] == true && v["available"] == true
    })
    .await;
    assert_eq!(d.calls.recv().await.unwrap().0, CATALOG);
    let (method, params) = d.calls.recv().await.unwrap();
    assert_eq!(method, PREVIEW);
    assert_eq!(params["request"]["image"], "/home/me/lake.jpg");
    assert_eq!(params["request"]["mode"], "light");
    assert_eq!(params["request"]["personality"], "natural");
    assert_eq!(params["request"]["compare"], false);
    assert_eq!(v["subtitle"], "Preparing preview…");
    assert_eq!(v["can_apply"], false, "no apply before a matching preview");
    d.gate.add_permits(1);
    let v = until(&colors, &at, |v| {
        v["busy"] == false && v["title"] == "Mountain lake"
    })
    .await;
    assert_eq!(v["samples"].as_array().unwrap().len(), 5);
    assert_eq!(v["samples"][0]["caption"], "42%");
    assert_eq!(v["other"], "Other shades: 12%");
    assert_eq!(v["compare"][0]["contrast"], "Text contrast 7.12");
    assert_eq!(v["status"], "Preview only — Apply when you're ready");
    assert_eq!(v["can_apply"], true);
    assert_eq!(v["can_export"], true);
    assert!(v["colors"]["primary"].is_array());
    // Newest wins (N0050): every change goes out at once, the daemon cancels the older request
    // (it fails `busy`) and only the last request's reply is used.
    colors.act("style:vivid", &at);
    let (_, params) = next_call(&colors, &at, &mut d).await;
    assert_eq!(params["request"]["personality"], "vivid");
    colors.act("brightness:0.050", &at);
    let (_, params) = next_call(&colors, &at, &mut d).await;
    assert_eq!(params["request"]["brightness"], 0.05);
    colors.act("brightness:0.080", &at);
    let (_, params) = next_call(&colors, &at, &mut d).await;
    assert_eq!(params["request"]["brightness"], 0.08);
    colors.act("saturation:1.200", &at);
    let (_, params) = next_call(&colors, &at, &mut d).await;
    assert_eq!(params["request"]["saturation"], 1.2);
    assert_eq!(params["request"]["brightness"], 0.08);
    let v = until(&colors, &at, |v| v["busy"] == true).await;
    assert_eq!(
        v["status_error"], false,
        "a cancelled older preview is not an error"
    );
    d.gate.add_permits(1);
    let v = until(&colors, &at, |v| v["busy"] == false).await;
    assert_eq!(v["can_apply"], true);
    // Apply sends exactly the preview id; the status follows the reply.
    colors.act("apply:", &at);
    let (method, params) = d.calls.recv().await.unwrap();
    assert_eq!((method.as_str(), params), (APPLY, json!({"id": "p1"})));
    let v = until(&colors, &at, |v| v["status"] == "Applied to the desktop").await;
    assert_eq!(
        v["applied"], false,
        "not applied until the daemon publishes the pair"
    );
    // The daemon publishes the applied pair: the button reads In use.
    d.updates
        .send((
            STORE.into(),
            doc(json!({"wallpapers": {}, "lastApplied": "p1"})),
        ))
        .unwrap();
    d.updates
        .send((ACTIVE.into(), active("/home/me/lake.jpg", &surface())))
        .unwrap();
    let v = until(&colors, &at, |v| v["applied"] == true).await;
    assert_eq!(v["can_apply"], false);
    // A second action while one runs is refused `busy`: said, not retried.
    colors.act("apply:", &at);
    assert_eq!(d.calls.recv().await.unwrap().0, APPLY);
    let v = until(&colors, &at, |v| v["status_error"] == true).await;
    assert!(v["status"].as_str().unwrap().contains("still running"));
    colors.act("style:natural", &at);
    assert_eq!(d.calls.recv().await.unwrap().0, PREVIEW);
    d.gate.add_permits(1);
    until(&colors, &at, |v| {
        v["busy"] == false && v["status_error"] == false
    })
    .await;
    // Export reports the folder.
    colors.act("export:", &at);
    assert_eq!(d.calls.recv().await.unwrap().0, EXPORT);
    until(&colors, &at, |v| {
        v["status"] == "Exported to /home/me/exports/lake"
    })
    .await;
    // A refused preview shows the daemon's text and blocks apply; a new choice clears it.
    d.gate.add_permits(1);
    colors.act("style:pop", &at);
    let (_, params) = d.calls.recv().await.unwrap();
    assert_eq!(params["request"]["personality"], "pop");
    let v = until(&colors, &at, |v| v["status_error"] == true).await;
    assert_eq!(v["status"], "Choose a local wallpaper");
    assert_eq!(v["can_apply"], false);
    // Choosing from the list re-resolves the path from the catalog, never from the text.
    colors.act("choose:1", &at);
    let (_, params) = d.calls.recv().await.unwrap();
    assert_eq!(params["request"]["image"], "/home/me/loop.mp4");
    assert!(woken.load(std::sync::atomic::Ordering::SeqCst) > 0);
    drop(colors);
    d.client.stop().await.unwrap();
    d.server.abort();
    let _ = std::fs::remove_dir_all(&d.dir);
    let _ = std::fs::remove_dir_all(&tmp);
}

/// The Welcome demo previews only: scenes from the catalog, eight styles, no apply.
#[tokio::test(flavor = "current_thread")]
async fn the_welcome_demo_previews_wallpapers_and_styles_and_never_applies() {
    crate::i18n::init();
    let tmp = temp("cache-demo");
    let mut d = daemon("wire-demo", &tmp, true).await;
    let colors = Colors::new(Some(d.client.clone()));
    let waiting = colors.demo(false);
    assert_eq!(
        waiting["available"], false,
        "nothing is fetched while the page is hidden"
    );
    assert!(d.calls.try_recv().is_err());
    let (method, _) = next_call_demo(&colors, &mut d).await;
    assert_eq!(method, PREVIEW);
    d.gate.add_permits(1);
    let mut v = colors.demo(true);
    for _ in 0..600 {
        if v["available"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        v = colors.demo(true);
    }
    assert_eq!(v["available"], true, "{v:#}");
    assert_eq!(v["styles"].as_array().unwrap().len(), 8);
    assert_eq!(v["styles"][0]["selected"], true);
    assert_eq!(v["scenes"][0]["name"], "Mountain lake");
    assert_eq!(v["scenes"][0]["selected"], true);
    assert_eq!(v["scenes"][0]["thumb"], "colors-thumb-0");
    assert_eq!(v["swatches"].as_array().unwrap().len(), 4);
    // Tide and a second scene are previews too.
    colors.act("style:tide", &stores("studio", ""));
    let (_, params) = next_call_demo(&colors, &mut d).await;
    assert_eq!(params["request"]["personality"], "tide");
    colors.act("choose:1", &stores("studio", ""));
    let (_, params) = next_call_demo(&colors, &mut d).await;
    assert_eq!(params["request"]["image"], "/home/me/loop.mp4");
    tokio::time::sleep(Duration::from_millis(50)).await;
    while let Ok((method, _)) = d.calls.try_recv() {
        assert_ne!(method, APPLY);
    }
    drop(colors);
    d.client.stop().await.unwrap();
    d.server.abort();
    let _ = std::fs::remove_dir_all(&d.dir);
    let _ = std::fs::remove_dir_all(&tmp);
}
async fn next_call_demo(colors: &Colors, d: &mut Daemon) -> (String, Json) {
    for _ in 0..600 {
        if let Ok(call) = d.calls.try_recv() {
            if call.0 == CATALOG {
                continue;
            }
            return call;
        }
        colors.demo(true);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no call reached the daemon");
}

#[tokio::test(flavor = "current_thread")]
async fn a_daemon_without_the_methods_gets_no_calls() {
    crate::i18n::init();
    let tmp = temp("cache2");
    let mut d = daemon("wire-gate", &tmp, false).await;
    let colors = Colors::new(Some(d.client.clone()));
    let at = stores("studio", "");
    let v = until(&colors, &at, |v| v["title"] == "lake.jpg").await;
    assert_eq!(v["available"], false);
    colors.act("apply:", &at);
    colors.act("style:vivid", &at);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(d.calls.try_recv().is_err());
    drop(colors);
    d.client.stop().await.unwrap();
    d.server.abort();
    let _ = std::fs::remove_dir_all(&d.dir);
    let _ = std::fs::remove_dir_all(&tmp);
}

// ---- pictures ---------------------------------------------------------------

fn scene(slots: &[(&str, f32, f32)]) -> bead::render::Scene {
    bead::render::Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: 800.,
        slide: None,
        primitives: slots
            .iter()
            .map(|(name, w, h)| bead::render::Primitive::Image {
                blur: 0.,
                rect: bead::render::Rect::new(0., 0., *w, *h),
                path: format!("@icon:colors-{name}").into(),
            })
            .collect(),
        offset: (0., 0.),
        bounds: bead::render::Rect::new(0., 0., 800., 600.),
    }
}
/// Colours seeded with a decoded poster and one thumbnail, as a projection would leave them.
fn with_pictures(tmp: &Path) -> Colors {
    let colors = Colors::new(None);
    let poster = cache(tmp, POSTERS.dir, "p.png", 400, 200);
    let thumb = cache(tmp, THUMBS.dir, "t.png", 100, 60);
    {
        let mut pictures = colors.pictures.borrow_mut();
        pictures.poster = Some((poster.clone(), player_art::decode(&POSTERS, &poster)));
        pictures
            .thumbs
            .insert(thumb.clone(), player_art::decode(&THUMBS, &thumb));
        let mut st = colors.state.borrow_mut();
        st.catalog = Some(Catalog {
            current: String::new(),
            entries: vec![Entry {
                path: "/a.png".into(),
                name: "A".into(),
                dynamic: false,
                thumbnail: thumb,
            }],
        });
    }
    colors
}

#[test]
fn image_slots_get_pngs_sized_to_their_own_rectangles() {
    let tmp = temp("pics");
    let colors = with_pictures(&tmp);
    let mut s = scene(&[
        ("photo", 300., 168.),
        ("card", 200., 100.),
        ("thumb-0", 60., 42.),
        ("thumb-9", 60., 42.),
    ]);
    colors.attach_images(&mut s);
    let size = |i: usize| {
        let bead::render::Primitive::Image { path, .. } = &s.primitives[i] else {
            unreachable!()
        };
        let bytes = s.asset_data.get(path)?;
        let image = image::load_from_memory(bytes).unwrap();
        Some((image.width(), image.height()))
    };
    assert_eq!(
        size(0),
        Some((600, 336)),
        "photo: whole image on a canvas of the slot at 2x"
    );
    assert_eq!(size(1), Some((400, 200)));
    assert_eq!(size(2), Some((120, 84)));
    assert_eq!(
        size(3),
        None,
        "an unknown thumbnail slot is left for the icon lookup"
    );
    // Contain letterboxes: a 2:1 poster in a 600x336 slot leaves transparent bars top and bottom.
    let bead::render::Primitive::Image { path, .. } = &s.primitives[0] else {
        unreachable!()
    };
    let photo = image::load_from_memory(s.asset_data.get(path).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(photo.get_pixel(300, 0)[3], 0);
    assert_eq!(photo.get_pixel(300, 168)[3], 255);
    // Painting again reuses the PNG; a different size makes another.
    let before = colors.pictures.borrow().rendered.len();
    colors.attach_images(&mut scene(&[("photo", 300., 168.)]));
    assert_eq!(colors.pictures.borrow().rendered.len(), before);
    colors.attach_images(&mut scene(&[("photo", 100., 56.)]));
    assert_eq!(colors.pictures.borrow().rendered.len(), before + 1);
    // A click on the letterbox bars is not a pick; one on the image maps to image fractions.
    assert_eq!(colors.painted(0.5, 0.05), None);
    let (x, y) = colors.painted(0.25, 0.5).unwrap();
    assert!((x - 0.25).abs() < 1e-6 && (y - 0.5).abs() < 1e-6);
    let _ = std::fs::remove_dir_all(tmp);
}

#[test]
fn only_plain_pngs_inside_the_cache_directories_are_decoded() {
    let tmp = temp("guard");
    let good = cache(&tmp, THUMBS.dir, "ok.png", 10, 10);
    assert!(player_art::decode(&THUMBS, &good).is_some());
    // The same file under a wrong directory name, a relative path, a symlink and junk bytes.
    let other = cache(&tmp, "elsewhere", "no.png", 10, 10);
    assert!(player_art::decode(&THUMBS, &other).is_none());
    assert!(player_art::decode(&THUMBS, "colors-thumbs/ok.png").is_none());
    let link = tmp.join(THUMBS.dir).join("link.png");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    assert!(player_art::decode(&THUMBS, link.to_str().unwrap()).is_none());
    std::fs::write(tmp.join(THUMBS.dir).join("junk.png"), b"nope").unwrap();
    assert!(
        player_art::decode(
            &THUMBS,
            tmp.join(THUMBS.dir).join("junk.png").to_str().unwrap()
        )
        .is_none()
    );
    let _ = std::fs::remove_dir_all(tmp);
}

// ---- goldens ----------------------------------------------------------------

fn harness(
    model: BTreeMap<String, Value>,
    width: u32,
    height: u32,
    scale: f32,
    dir: &Path,
) -> Harness {
    crate::i18n::init();
    let icons = BTreeMap::from([
        (
            "colors-photo".to_string(),
            PathBuf::from(cache(dir, "icons", "photo.png", 600, 336)),
        ),
        (
            "colors-card".to_string(),
            PathBuf::from(cache(dir, "icons", "card.png", 320, 200)),
        ),
        (
            "colors-thumb-0".to_string(),
            PathBuf::from(cache(dir, "icons", "thumb.png", 120, 84)),
        ),
        (
            "colors-thumb-1".to_string(),
            PathBuf::from(cache(dir, "icons", "thumb1.png", 120, 84)),
        ),
    ]);
    Harness::load_with_icons(
        &root(),
        Path::new("colors/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
        icons,
    )
    .unwrap()
}
fn golden(name: &str, h: &mut Harness) {
    let frame = h.pose((0., 0.), 1.).unwrap();
    let path = root().join(format!("tests/goldens/{name}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path).unwrap();
    }
    frame
        .compare(
            &path,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-{name}-diff")),
        )
        .unwrap();
}
/// The model a connected daemon with a finished preview gives, built from fixtures.
fn explored(
    tmp: &Path,
    page: &str,
    fine: bool,
    vision: &str,
    width: u32,
) -> BTreeMap<String, Value> {
    let (store, active) = sources(
        json!({"wallpapers": {}, "lastApplied": ""}),
        active("/home/me/lake.jpg", &surface()),
    );
    let colors = Colors::new(None).with_sources(store, active);
    let poster = cache(tmp, POSTERS.dir, "p.png", 400, 224);
    let thumb = cache(tmp, THUMBS.dir, "t.png", 100, 60);
    {
        let mut st = colors.state.borrow_mut();
        let request = json!({"image": "/home/me/lake.jpg", "mode": "dark", "personality": "natural",
            "backgroundFromWallpaper": false, "overrides": {"secondary": "d9a441"}, "brightness": 0.0,
            "saturation": 1.0, "timeTint": false, "workspaceColors": false, "compare": page == "compare"});
        st.options = request.clone();
        st.preview = Some(preview("p1", "/home/me/lake.jpg", &poster, &request));
        st.catalog = Some(Catalog {
            current: "/home/me/lake.jpg".into(),
            entries: vec![
                Entry {
                    path: "/home/me/lake.jpg".into(),
                    name: "Mountain lake at dawn with long mist".into(),
                    dynamic: false,
                    thumbnail: thumb,
                },
                Entry {
                    path: "/home/me/loop.mp4".into(),
                    name: "Rain loop".into(),
                    dynamic: true,
                    thumbnail: String::new(),
                },
            ],
        });
        st.armed = Some("tertiary".into());
    }
    colors.set_viewport(width);
    let at = Stores::default()
        .reconcile(&BTreeMap::from([
            ("colors.page".into(), Value::Text(page.into())),
            ("colors.query".into(), Value::Text(String::new())),
            ("colors.vision".into(), Value::Text(vision.into())),
            ("colors.fine".into(), Value::Bool(fine)),
            ("colors.open".into(), Value::Text(String::new())),
        ]))
        .unwrap();
    let mut model = colors.project(&at);
    // A connected daemon enables the actions and the style buttons.
    let mut v = view(&model);
    for key in ["available", "can_export"] {
        v[key] = true.into();
    }
    v["can_apply"] = true.into();
    v["has_photo"] = true.into();
    v["subtitle"] = "Click the image to pick a color".into();
    v["status"] = "Preview only — Apply when you're ready".into();
    model.insert("colors.view".into(), Value::list(vec![v]));
    model
}
#[test]
fn colors_window_goldens() {
    for (name, page, fine, vision, scale, width, height) in [
        ("colors-studio-1", "studio", false, "normal", 1., 1100, 760),
        (
            "colors-studio-1.5",
            "studio",
            false,
            "normal",
            1.5,
            1100,
            760,
        ),
        (
            "colors-studio-tall-1",
            "studio",
            false,
            "normal",
            1.,
            1100,
            1100,
        ),
        ("colors-fine-1", "studio", true, "normal", 1., 1100, 1500),
        (
            "colors-compare-1",
            "compare",
            false,
            "normal",
            1.,
            1100,
            1000,
        ),
        (
            "colors-access-1",
            "accessibility",
            false,
            "protan",
            1.,
            1100,
            1000,
        ),
        ("colors-narrow-1", "studio", false, "normal", 1., 760, 1300),
    ] {
        let tmp = temp(name);
        let model = explored(&tmp, page, fine, vision, width);
        let mut h = harness(model, width, height, scale, &tmp);
        golden(name, &mut h);
        let _ = std::fs::remove_dir_all(tmp);
    }
}
#[test]
fn colors_window_without_a_daemon_golden() {
    let tmp = temp("fallback");
    let (store, active) = sources(json!({}), active("/home/me/Pictures/lake.jpg", &surface()));
    let colors = Colors::new(None).with_sources(store, active);
    let model = colors.project(&stores("studio", ""));
    let mut h = harness(model, 1100, 760, 1., &tmp);
    golden("colors-unavailable-1", &mut h);
    let _ = std::fs::remove_dir_all(tmp);
}

/// The Settings pages for wallpaper and colours are registry rows like any other; their custom
/// sections (picker, preview, app theming targets) stay placeholders and the Colors app is the
/// way to explore and apply a palette.
#[test]
fn settings_wallpaper_and_colors_pages_are_editable_registry_rows() {
    crate::i18n::init();
    let settings = crate::settings::Settings::new(None, &root()).unwrap();
    let at = |page: &str| {
        Stores::default()
            .reconcile(&BTreeMap::from([
                ("settings.page".into(), Value::Text(page.into())),
                ("settings.search".into(), Value::Text(String::new())),
            ]))
            .unwrap()
    };
    let rows = |page: &str| -> (Vec<Json>, Value) {
        let model = settings.project(&at(page));
        let Value::List(items) = &model["settings.items"] else {
            panic!()
        };
        // B0067: rows travel inside their section.
        let flat = items
            .iter()
            .flat_map(|row| {
                std::iter::once(row.clone())
                    .chain(row["rows"].as_array().cloned().unwrap_or_default())
            })
            .collect();
        (flat, model["settings.readonly"].clone())
    };
    let kind = |rows: &[Json], id: &str| {
        rows.iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing"))["kind"]
            .clone()
    };
    let (wallpaper, readonly) = rows("wallpaper");
    assert_eq!(readonly, Value::Bool(false));
    assert_eq!(
        kind(&wallpaper, "wallpaper.rotation.rotationEnabled"),
        "toggle"
    );
    assert_eq!(
        kind(&wallpaper, "wallpaper.rotation.rotationMinutes"),
        "number"
    );
    assert_eq!(kind(&wallpaper, "wallpaper.motion.motionMode"), "choice");
    let (colors, readonly) = rows("colors");
    assert_eq!(readonly, Value::Bool(false));
    assert_eq!(kind(&colors, "colors.palette.style"), "select");
    // Mode and the schedule are applied by helpers, not by a store write: shown, not editable.
    assert_eq!(kind(&colors, "colors.palette.mode"), "help");
    let (targets, _) = rows("colors/app-theming");
    assert!(
        targets
            .iter()
            .filter(|r| r["id"].as_str().is_some_and(|id| id.contains(".targets.")))
            .all(|r| r["kind"] == "help")
    );
}

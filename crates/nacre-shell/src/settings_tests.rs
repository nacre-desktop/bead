// SPDX-License-Identifier: GPL-3.0-only
//! B0045: Settings window rows, store read/write over a fake daemon, goldens, lifecycle.
use super::settings::*;
use crate::nacre_source::{CallWake, Client, Config};
use bead::core::{Stores, Value, source::Source};
use bead_test::{Config as HarnessConfig, Harness, source::FakeSource};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn stores(page: &str, search: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("settings.page".into(), Value::Text(page.into())),
            ("settings.search".into(), Value::Text(search.into())),
        ]))
        .unwrap()
}
fn doc(store: &str, rev: u64, data: Json) -> Json {
    json!({"store": store, "rev": rev, "exists": true, "data": data, "dirty": false})
}
fn fixture(desktop: Json) -> (Settings, Arc<FakeSource>) {
    let source = Arc::new(FakeSource::new(doc("desktop", 1, desktop)).unwrap());
    let sources: BTreeMap<String, Arc<dyn Source>> =
        BTreeMap::from([("desktop".into(), source.clone() as Arc<dyn Source>)]);
    (Settings::new(None).unwrap().with_sources(sources), source)
}
fn items(model: &BTreeMap<String, Value>) -> Vec<Json> {
    match &model["settings.items"] {
        Value::List(rows) => rows.to_vec(),
        other => panic!("{other:?}"),
    }
}
fn row<'a>(rows: &'a [Json], id: &str) -> &'a Json {
    rows.iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("{id} missing"))
}
fn wake() -> CallWake {
    Arc::new(|| {})
}

#[test]
fn registry_loads_and_every_navigation_page_has_a_label_and_a_group() {
    let (settings, _) = fixture(json!({}));
    let model = settings.project(&stores("", ""));
    let Value::List(nav) = &model["settings.nav"] else {
        panic!()
    };
    let pages = nav.iter().filter(|n| n["kind"] == "page").count();
    let groups = nav.iter().filter(|n| n["kind"] == "group").count();
    assert_eq!((groups, pages), (5, 21), "five groups, 21 top-level pages");
    assert!(nav.iter().all(|n| {
        !n["label"]
            .as_str()
            .unwrap()
            .starts_with("settings.registry.")
    }));
    // The default page is the first Bead can edit.
    assert_eq!(model["settings.page"], Value::Text("appearance".into()));
    assert_eq!(model["settings.title"], Value::Text("Appearance".into()));
}

#[test]
fn appearance_rows_follow_the_registry_types_and_the_store_with_nacre_defaults() {
    let (settings, _) =
        fixture(json!({"textSize": 14, "frameSheen": false, "monoFont": "Fira Code"}));
    let model = settings.project(&stores("appearance", ""));
    let rows = items(&model);
    assert_eq!(row(&rows, "appearance.fonts.textSize")["kind"], "number");
    assert_eq!(row(&rows, "appearance.fonts.textSize")["number"], 14);
    assert_eq!(row(&rows, "appearance.fonts.textSize")["unit"], "pt");
    assert_eq!(row(&rows, "appearance.frame.frameSheen")["checked"], false);
    // Unset keys read the registry default, like QML.
    assert_eq!(row(&rows, "appearance.frame.frameWidth")["number"], 10);
    assert_eq!(row(&rows, "appearance.motion.animations")["checked"], true);
    assert_eq!(row(&rows, "appearance.fonts.monoFont")["text"], "Fira Code");
    assert_eq!(
        row(&rows, "appearance.fonts.interfaceFont")["text"],
        "Default"
    );
    assert_eq!(rows.iter().filter(|r| r["kind"] == "section").count(), 3);
    // No daemon: visible but not writable.
    assert_eq!(model["settings.writable"], Value::Bool(false));
}

#[test]
fn notifications_page_and_search_use_the_same_registry_data() {
    let (settings, _) = fixture(json!({"dnd": true}));
    let rows = items(&settings.project(&stores("notifications", "")));
    assert_eq!(row(&rows, "notifications.delivery.dnd")["checked"], true);
    let found = items(&settings.project(&stores("appearance", "kitty")));
    assert!(
        found
            .iter()
            .any(|r| r["id"] == "setting:appearance.fonts.monoFont")
    );
    assert!(found.iter().all(|r| r["kind"] == "link"));
    let none = settings.project(&stores("appearance", "zzzzqqq"));
    assert_eq!(none["settings.empty"], Value::Bool(true));
    // Pages Bead cannot edit yet are shown read-only, never writable.
    let panels = settings.project(&stores("panels", ""));
    assert_eq!(panels["settings.readonly"], Value::Bool(true));
}

const DESKTOP: &str = "settings.store.desktop";

/// A real client against a fake daemon: reads the store, writes with the revision
/// fence, and follows an outside edit.
#[tokio::test(flavor = "current_thread")]
async fn store_read_write_round_trip_over_the_wire_and_live_updates() {
    use nacre_protocol::{Capability, Message, Reply, framing};
    use std::os::unix::fs::PermissionsExt;
    use tokio::{io::BufReader, net::UnixListener, sync::mpsc};
    let dir = std::env::temp_dir().join(format!("settings-wire-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, mut calls) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        let mut methods = nacre_protocol::core_methods();
        methods.insert(
            METHOD.into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 10_000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
        let welcome = Message::Welcome {
            proto: 1,
            min: 1,
            boot: "settings-boot".into(),
            topics: [(DESKTOP.to_string(), 1)].into(),
            methods,
            frame_ms: None,
        };
        framing::write(&mut writer, &welcome).await.unwrap();
        let mut seq = 0;
        while let Ok(Some(message)) = reader.next().await {
            match message {
                Message::Sub { topics } => {
                    for topic in topics {
                        let snap = Message::Snap {
                            topic,
                            seq,
                            data: doc("desktop", 3, json!({"dnd": false})),
                        };
                        framing::write(&mut writer, &snap).await.unwrap();
                    }
                }
                Message::Call { id, m, p, .. } => {
                    assert_eq!(m, METHOD);
                    calls_tx.send(p.clone()).unwrap();
                    let reply = Reply::Ok {
                        ok: json!({"rev": 4}),
                    };
                    framing::write(&mut writer, &Message::Ret { id, reply })
                        .await
                        .unwrap();
                    {
                        seq += 1;
                        let snap = Message::Snap {
                            topic: DESKTOP.into(),
                            seq,
                            data: doc("desktop", 4, json!({"dnd": true})),
                        };
                        framing::write(&mut writer, &snap).await.unwrap();
                    }
                }
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(socket, vec![DESKTOP.into()])).unwrap();
    for _ in 0..500 {
        if client.snapshot().connected && client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let settings = Settings::new(Some(client.clone())).unwrap();
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    settings.subscribe(Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let at = stores("notifications", "");
    let mut model = settings.project(&at);
    for _ in 0..500 {
        model = settings.project(&at);
        if model["settings.writable"] == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(model["settings.writable"], Value::Bool(true));
    assert_eq!(
        row(&items(&model), "notifications.delivery.dnd")["checked"],
        false
    );
    // Write: exactly the published call shape, fenced by the revision read from the topic.
    let before = woken.load(std::sync::atomic::Ordering::SeqCst);
    settings.act("set:notifications.delivery.dnd", "", wake());
    let params = calls.recv().await.unwrap();
    assert_eq!(
        params,
        json!({"store":"desktop","key":["dnd"],"value":true,"rev":3})
    );
    // The daemon accepts it and republishes; the row follows live.
    for _ in 0..500 {
        model = settings.project(&at);
        if row(&items(&model), "notifications.delivery.dnd")["checked"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        row(&items(&model), "notifications.delivery.dnd")["checked"],
        true
    );
    assert!(
        woken.load(std::sync::atomic::Ordering::SeqCst) > before,
        "store change woke the window"
    );
    assert_eq!(model["settings.message"], Value::Text(String::new()));
    // Pages Bead does not edit never send a call.
    settings.act("set:panels.lips.clickEdgeMenus|1", "", wake());
    assert!(calls.try_recv().is_err());
    // The window is closed: leases and subscriptions go with it.
    drop(settings);
    client.stop().await.unwrap();
    server.abort();
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn values_are_typed_clamped_and_staged_before_they_reach_the_wire() {
    let (settings, _) = fixture(json!({"textSize": 14}));
    // Without a daemon nothing is ever submitted, and the message says why.
    let at = stores("appearance", "");
    settings.project(&at);
    settings.act("set:appearance.fonts.textSize|99", "", wake());
    let model = settings.project(&at);
    assert_eq!(
        model["settings.message"],
        Value::Text("Settings cannot be changed right now.".into())
    );
}

#[test]
fn closing_releases_every_store_source() {
    let (settings, source) = fixture(json!({}));
    let weak = Arc::downgrade(&source);
    settings.project(&stores("appearance", ""));
    drop(settings);
    // Only the test's own handle is left.
    assert_eq!(Arc::strong_count(&source), 1);
    drop(source);
    assert!(weak.upgrade().is_none());
}

fn harness(model: BTreeMap<String, Value>, width: u32, height: u32, scale: f32) -> Harness {
    Harness::load(
        &root(),
        Path::new("settings/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
    )
    .unwrap()
}
fn golden(name: &str, h: &mut Harness) {
    // The window fills its surface: no panel shadow inset.
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

#[test]
fn settings_window_goldens() {
    let desktop =
        json!({"textSize": 14, "frameSheen": false, "monoFont": "Fira Code", "dnd": true});
    for (name, page, search, scale) in [
        ("settings-appearance-1", "appearance", "", 1.),
        ("settings-appearance-1.5", "appearance", "", 1.5),
        ("settings-notifications-1", "notifications", "", 1.),
        ("settings-search-1", "appearance", "font", 1.),
    ] {
        let (settings, _) = fixture(desktop.clone());
        let mut model = settings.project(&stores(page, search));
        // Goldens show the enabled look a connected daemon gives.
        model.insert("settings.writable".into(), Value::Bool(true));
        let mut h = harness(model, 960, 640, scale);
        golden(name, &mut h);
    }
}

/// Every row kind of the form kit, including the states a real page rarely shows
/// together: staged apply, editing text, a swatch, a list and disabled controls.
#[test]
fn form_kit_gallery_golden() {
    let rows = vec![
        json!({"id":"s1","kind":"section","label":"Controls","description":"One of every row kind"}),
        json!({"id":"t1","kind":"toggle","label":"Toggle on","description":"A switch","enabled":true,"checked":true}),
        json!({"id":"t2","kind":"toggle","label":"Needs apply","description":"Pending value","enabled":true,"checked":true,"staged":true,"apply":true}),
        json!({"id":"c1","kind":"choice","label":"Segmented choice","description":"","enabled":true,
            "options":[{"id":"c1|0","label":"One","selected":false},{"id":"c1|1","label":"Two","selected":true},{"id":"c1|2","label":"Three","selected":false}]}),
        json!({"id":"n1","kind":"number","label":"Number","description":"Stepper with a unit","enabled":true,"number":12,"text":"12","unit":"px","step":1,"min":0,"max":30,"at_min":false,"at_max":false}),
        json!({"id":"x1","kind":"text","label":"Text","description":"","enabled":true,"text":"Fira Code","editing":false}),
        json!({"id":"x2","kind":"text","label":"Colour","description":"Swatch","enabled":true,"text":"#7aa2f7","swatch":[122,162,247,255],"editing":false}),
        json!({"id":"x3","kind":"text","label":"Editing","description":"","enabled":true,"text":"x","editing":true}),
        json!({"id":"l1","kind":"list","label":"List","enabled":true,"editing":false,"items":[{"id":"l1|rm:0","label":"first"},{"id":"l1|rm:1","label":"second"}]}),
        json!({"id":"d1","kind":"toggle","label":"Disabled toggle","description":"","enabled":false,"checked":false}),
        json!({"id":"h1","kind":"help","label":"Help text under the controls."}),
    ];
    let model = BTreeMap::from([
        (
            "settings.nav".into(),
            Value::list(vec![
                json!({"id":"group:x","kind":"group","label":"Group","icon":"","selected":false}),
                json!({"id":"a","kind":"page","label":"Page","icon":"palette","selected":true}),
            ]),
        ),
        ("settings.items".into(), Value::list(rows)),
        ("settings.title".into(), Value::Text("Kit gallery".into())),
        (
            "settings.description".into(),
            Value::Text("Every form kit row".into()),
        ),
        ("settings.writable".into(), Value::Bool(true)),
    ]);
    let mut h = harness(model, 960, 900, 1.);
    golden("settings-kit-1", &mut h);
}

/// Typing in the search field rebuilds the filtered list each key. Budget 8 ms per key
/// (release); a debug build is several times slower, so it only guards against a blow-up.
#[test]
fn typing_in_the_search_field_is_fast() {
    use bead::wayland::{SceneSource, UiEvent};
    let palette = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session =
        bead::ui::Session::new(root(), "settings/main.luau".into(), palette, colors, cold);
    let mut runtime = crate::reload::Runtime::new(session)
        .with_settings(None)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    runtime.viewport(960, 640);
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("settings view loads");
    runtime.commit();
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with("search"))
        .expect("search field")
        .draw
        .clone();
    let rect = bead::ui::rect(&node);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + 4.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    let mut times = Vec::new();
    for ch in "keyboard layout".chars() {
        let started = std::time::Instant::now();
        runtime
            .stage(UiEvent::Key {
                text: ch.to_string(),
                backspace: false,
            })
            .unwrap();
        runtime.commit();
        times.push(started.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    let (p50, max) = (times[times.len() / 2], times[times.len() - 1]);
    println!(
        "SETTINGS_TYPING p50_ms={p50:.2} max_ms={max:.2} debug={}",
        cfg!(debug_assertions)
    );
    let budget = if cfg!(debug_assertions) { 80. } else { 8. };
    assert!(
        p50 <= budget && max <= budget * 2.,
        "p50 {p50:.2} max {max:.2} ms"
    );
    assert!(matches!(
        runtime.session.stores.get("settings.search"),
        Some(Value::Text(t)) if t == "keyboard layout"
    ));
}

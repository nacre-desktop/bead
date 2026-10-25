// SPDX-License-Identifier: GPL-3.0-only
//! B0049: control-center sources against fixtures, tile-store persistence against a fake
//! daemon, the Luau view's drag reorder, and goldens for the four pages.
use super::*;
use crate::launcher_quick::Quick;
use crate::nacre_source::Config;
use bead_test::{Harness, Input, source::FakeSource};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::{path::Path, sync::Mutex};
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn health() -> Json {
    json!({"available": true, "stale": false})
}
/// PipeWire `node.name`s as the daemon publishes them (N0059): dots, dashes and underscores.
const OUT: &str = "alsa_output.pci-0000_00_1f.3.analog-stereo";
const IN: &str = "alsa_input.usb-Fixture_Mic-00.mono-fallback";
fn audio() -> Json {
    json!({"health": health(), "default_output": OUT, "default_input": IN, "nodes": {"order": [OUT, IN], "items": {
        OUT: {"name": "Speakers", "kind": "output", "volume": 0.125, "muted": false, "ready": true,
            "readback": {"perceptual_volume": 0.5, "linear_volume": 0.125, "muted": false}},
        IN: {"name": "Mic", "kind": "input", "volume": 0.512, "muted": true, "ready": true}}}})
}
fn network() -> Json {
    json!({"health": health(), "wireless_enabled": true,
        "devices": {"items": {"d": {"name":"Wi-Fi","kind":"wireless","connected":true}}, "order": ["d"]}, "connections": {"items": {}, "order": []},
        "access_points": {"order": ["ap"], "items": {"ap": {"device": "d", "name": "Home", "strength": 70, "secured": true, "active": true}}}})
}
fn bluetooth() -> Json {
    json!({"health": health(),
        "adapters": {"order": ["hci0"], "items": {"hci0": {"name": "hci0", "powered": true, "discovering": false}}},
        "devices": {"order": ["d1"], "items": {"d1": {"name": "Buds", "connected": true, "adapter":"hci0", "paired":true, "trusted":true, "pairing":false}}}})
}
/// N0062 `vpn`: one connected WireGuard profile and one idle VPN-plugin profile.
fn vpn() -> Json {
    json!({"health": health(), "connections": {"order": ["home", "work"], "items": {
        "home": {"name": "Home WG", "kind": "wireguard", "uuid": "bbbbbbbb-1111-4222-8333-444444444444", "state": "connected", "active": true},
        "work": {"name": "Work VPN", "kind": "vpn", "uuid": "aaaaaaaa-1111-4222-8333-444444444444", "state": "disconnected", "active": false}}}})
}
fn power() -> Json {
    json!({"health": health(), "active": "balanced",
        "profiles": {"order": ["power-saver", "balanced", "performance"], "items": {
            "power-saver": {"name": "power-saver", "available": true},
            "balanced": {"name": "balanced", "available": true},
            "performance": {"name": "performance", "available": false}}}})
}
fn battery() -> Json {
    json!({"health": health(), "percentage": 78.0, "charging": false, "state": 2,
        "time_to_empty_seconds": 7380, "on_battery": true})
}
fn notifications() -> Json {
    json!({"health": health(), "do_not_disturb": false})
}
fn controls() -> Json {
    json!({"health": health(), "brightness": {"DP-1": 40, "eDP-1": 60}, "keyboard": {"level": 1, "max": 3},
        "airplane": {"enabled": false}, "night_light": {"enabled": false, "external": false},
        "stay_awake": {"enabled": false}, "recorder": {"recording": false}, "appearance": {"mode": "dark"}})
}
fn topic_data(name: &str) -> Json {
    match name {
        "controls" => controls(),
        "audio" => audio(),
        "network" => network(),
        "bluetooth" => bluetooth(),
        "vpn" => vpn(),
        "power.profiles" => power(),
        "battery" => battery(),
        _ => notifications(),
    }
}
fn store(tiles: Json, rev: u64) -> Json {
    json!({"store": STORE, "rev": rev, "exists": true, "data": {"version": 2, "tiles": tiles}, "dirty": false})
}
fn views() -> Vec<String> {
    [
        "network",
        "bluetooth",
        "battery",
        "session",
        "dashboard-clipboard",
        "dashboard-system",
        "dashboard-notifications",
    ]
    .map(String::from)
    .to_vec()
}
fn stores(pairs: &[(&str, Value)]) -> Stores {
    Stores::default()
        .reconcile(
            &pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
        .unwrap()
}
fn ids(list: Vec<&str>) -> Value {
    Value::list(list.into_iter().map(|id| json!({"id": id})).collect())
}
fn offline() -> ControlCenter {
    let topics = TOPICS
        .iter()
        .map(|name| {
            (
                *name,
                Arc::new(FakeSource::new(topic_data(name)).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    let saved = Arc::new(FakeSource::new(store(json!(["volume", "network", "dnd"]), 3)).unwrap());
    ControlCenter::new(None, topics, Some(saved), views())
}
fn source<'a>(out: &'a BTreeMap<String, Value>, id: &str) -> &'a Json {
    let Value::List(items) = &out["cc.src"] else {
        panic!("cc.src");
    };
    items
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("no source {id}"))
}

fn with_store(data: Json) -> BTreeMap<String, Value> {
    let topics = TOPICS
        .iter()
        .map(|name| {
            (
                *name,
                Arc::new(FakeSource::new(topic_data(name)).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    let saved = Arc::new(
        FakeSource::new(
            json!({"store": STORE, "rev": 1, "exists": true, "data": data, "dirty": false}),
        )
        .unwrap(),
    );
    ControlCenter::new(None, topics, Some(saved), views()).project(&Stores::default())
}

#[test]
fn an_earlier_default_set_is_the_unedited_default_until_revision_two() {
    // The Nacre registry default (and QML's earlier slider set) must not hide the current default tiles (B0091).
    let nine = json!([
        "volume",
        "brightness",
        "network",
        "bluetooth",
        "battery",
        "night",
        "dnd",
        "awake",
        "appearance"
    ]);
    assert_eq!(
        with_store(json!({"tiles": nine}))["cc.saved_known"],
        Value::Bool(false)
    );
    assert_eq!(
        with_store(json!({"version": 2, "tiles": nine}))["cc.saved_known"],
        Value::Bool(false)
    );
    let revised = with_store(json!({"version": 2, "defaultsRevision": 2, "tiles": nine}));
    assert_eq!(
        revised["cc.saved_known"],
        Value::Bool(true),
        "a revision-2 store is the user's own choice"
    );
    let edited = with_store(json!({"tiles": ["volume", "network"]}));
    assert_eq!(edited["cc.saved"], ids(vec!["volume", "network"]));
}

#[test]
fn sources_read_the_published_topics_and_hide_what_is_missing() {
    let out = offline().project(&Stores::default());
    let volume = source(&out, "audio.output");
    assert_eq!(volume["value"], 50.);
    assert_eq!(volume["status"], "50%");
    assert_eq!(volume["icon"], "volume_up");
    let mic = source(&out, "audio.input");
    assert_eq!(mic["on"], true, "muted input");
    assert_eq!(
        mic["value"], 80.,
        "cube root of the linear volume without readback"
    );
    assert_eq!(source(&out, "network.wifi")["status"], "Home");
    assert_eq!(source(&out, "bluetooth.power")["status"], "Buds");
    assert_eq!(source(&out, "power.profile")["status"], "Balanced");
    assert_eq!(source(&out, "notifications.dnd")["on"], false);
    // No daemon link: nothing is actionable, the tiles are read-only.
    assert_eq!(volume["toggle"], false);
    assert_eq!(volume["set"], false);
    // Contracts nobody publishes yield no source at all.
    let Value::List(all) = &out["cc.src"] else {
        panic!()
    };
    assert!(all.iter().all(|s| s["id"] != "screenshot.capture"));
    assert_eq!(source(&out, "brightness.level")["status"], "60%");
    assert_eq!(source(&out, "brightness.level")["set"], false);
    assert_eq!(out["cc.battery"], Value::Text("78%".into()));
    assert_eq!(out["cc.saved_known"], Value::Bool(true));
    assert_eq!(out["cc.saved"], ids(vec!["volume", "network", "dnd"]));
    assert_eq!(out["cc.writable"], Value::Bool(false));
}

#[test]
fn stale_or_unavailable_topics_publish_no_source() {
    let control = offline();
    let mut bad = network();
    bad["health"]["stale"] = json!(true);
    let topics: Vec<_> = TOPICS
        .iter()
        .map(|name| {
            let data = if *name == "network" {
                bad.clone()
            } else {
                topic_data(name)
            };
            (
                *name,
                Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    let out = ControlCenter::new(None, topics, None, views()).project(&Stores::default());
    let Value::List(all) = &out["cc.src"] else {
        panic!()
    };
    assert!(all.iter().all(|s| s["id"] != "network.wifi"));
    assert_eq!(out["cc.saved_known"], Value::Bool(false));
    // A hidden panel projects nothing.
    assert!(
        control
            .project(&stores(&[("panel.open", Value::Bool(false))]))
            .is_empty()
    );
}

// ---- fake daemon -----------------------------------------------------------------------

struct Lab {
    dir: std::path::PathBuf,
    client: Client,
    calls: mpsc::UnboundedReceiver<(String, Json)>,
    fail: Arc<Mutex<BTreeMap<String, ErrorCode>>>,
    unsubs: mpsc::UnboundedReceiver<Vec<String>>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
async fn lab(methods: &[&str]) -> Lab {
    lab_with(methods, controls()).await
}
async fn lab_with(methods: &[&str], published: Json) -> Lab {
    lab_with_permanent(methods, published, Vec::new()).await
}
async fn lab_with_permanent(methods: &[&str], published: Json, permanent: Vec<String>) -> Lab {
    lab_with_payloads(methods, published, permanent, notifications()).await
}
async fn lab_with_payloads(
    methods: &[&str],
    published: Json,
    permanent: Vec<String>,
    notices: Json,
) -> Lab {
    lab_full(methods, published, permanent, notices, BTreeMap::new()).await
}
/// A daemon whose named topics publish the given payloads instead of the shared fixtures.
async fn lab_topics(methods: &[&str], topics: &[(&str, Json)]) -> Lab {
    let topics = topics
        .iter()
        .map(|(n, d)| (n.to_string(), d.clone()))
        .collect();
    lab_full(methods, controls(), Vec::new(), notifications(), topics).await
}
async fn lab_full(
    methods: &[&str],
    published: Json,
    permanent: Vec<String>,
    notices: Json,
    topics: BTreeMap<String, Json>,
) -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "cc-lab-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let (unsubs_tx, unsubs) = mpsc::unbounded_channel();
    let fail: Arc<Mutex<BTreeMap<String, ErrorCode>>> = Default::default();
    let failing = fail.clone();
    let notices = Arc::new(notices);
    let overrides = Arc::new(topics);
    let advertised: Vec<String> = methods.iter().map(|m| m.to_string()).collect();
    tokio::spawn(async move {
        let mut boot = 0;
        // The store lives in the daemon: one revision, bumped by every accepted write.
        let mut rev = 3u64;
        let mut tiles = json!(["volume", "network", "dnd"]);
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            boot += 1;
            let (read, mut writer) = stream.into_split();
            let (inbox, mut incoming) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut reader = framing::Reader::new(BufReader::new(read));
                while let Ok(Some(message)) = reader.next().await {
                    if inbox.send(message).is_err() {
                        break;
                    }
                }
            });
            let Some(Message::Hello { .. }) = incoming.recv().await else {
                continue;
            };
            let method = |_: &String| nacre_protocol::Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 3000,
                params: json!({"type": "object"}),
                result: json!({}),
            };
            let mut methods = nacre_protocol::core_methods();
            methods.extend(advertised.iter().map(|m| (m.clone(), method(m))));
            framing::write(
                &mut writer,
                &Message::Welcome {
                    proto: 1,
                    min: 1,
                    boot: format!("cc-boot-{boot}"),
                    topics: TOPICS
                        .iter()
                        .chain([&STORE_TOPIC])
                        .chain([&"dashboard.updates"])
                        .map(|name| (name.to_string(), 1))
                        .collect(),
                    methods,
                    frame_ms: None,
                },
            )
            .await
            .unwrap();
            while let Some(message) = incoming.recv().await {
                match message {
                    Message::Sub { topics } => {
                        for topic in topics {
                            let data = if topic == STORE_TOPIC {
                                store(tiles.clone(), rev)
                            } else if topic == "dashboard.updates" {
                                json!({"health": health(), "count": 0, "text": "No updates"})
                            } else if topic == "controls" {
                                published.clone()
                            } else if let Some(data) = overrides.get(&topic) {
                                data.clone()
                            } else if topic == "notifications" {
                                (*notices).clone()
                            } else {
                                topic_data(&topic)
                            };
                            framing::write(
                                &mut writer,
                                &Message::Snap {
                                    topic,
                                    seq: 0,
                                    data,
                                },
                            )
                            .await
                            .unwrap();
                        }
                    }
                    Message::Unsub { topics } => {
                        let _ = unsubs_tx.send(topics);
                    }
                    Message::Call { id, m, p, .. } => {
                        let _ = calls_tx.send((m.clone(), p.clone()));
                        let code = failing.lock().unwrap().get(&m).cloned();
                        let reply = if let Some(code) = code {
                            Reply::Err {
                                err: Failure::new(code, "refused", false),
                            }
                        } else if m == SET {
                            if p["rev"] == rev {
                                rev += 1;
                                tiles = p["value"].clone();
                                Reply::Ok {
                                    ok: store(tiles.clone(), rev),
                                }
                            } else {
                                Reply::Err {
                                    err: Failure::new(ErrorCode::StaleRevision, "stale", true),
                                }
                            }
                        } else {
                            Reply::Ok {
                                ok: json!({"changed": true}),
                            }
                        };
                        framing::write(&mut writer, &Message::Ret { id, reply })
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
            }
        }
    });
    let client = Client::start(Config::new(socket, permanent)).unwrap();
    Lab {
        dir,
        client,
        calls,
        fail,
        unsubs,
    }
}
async fn control_for(lab: &Lab) -> ControlCenter {
    control_with(lab, 13).await
}
/// A control centre that waits until `sources` data sources are projected.
async fn control_with(lab: &Lab, sources: usize) -> ControlCenter {
    for _ in 0..500 {
        if lab.client.snapshot().connected && lab.client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let topics = TOPICS
        .iter()
        .map(|name| {
            (
                *name,
                Arc::new(lab.client.lease_topic(name).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    let store = Arc::new(lab.client.lease_topic(STORE_TOPIC).unwrap()) as Arc<dyn Source>;
    let control = ControlCenter::new(Some(lab.client.clone()), topics, Some(store), views());
    // Wait for the leased topics to deliver their first snapshots.
    for _ in 0..500 {
        let out = control.project(&Stores::default());
        if out["cc.saved_known"] == Value::Bool(true) && source_count(&out) >= sources {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    control
}
fn source_count(out: &BTreeMap<String, Value>) -> usize {
    match &out["cc.src"] {
        Value::List(items) => items.len(),
        _ => 0,
    }
}
async fn call(lab: &mut Lab) -> (String, Json) {
    tokio::time::timeout(Duration::from_secs(5), lab.calls.recv())
        .await
        .expect("no call reached the daemon")
        .unwrap()
}
async fn settle(control: &ControlCenter, st: &Stores) -> BTreeMap<String, Value> {
    for _ in 0..30 {
        control.project(st);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    control.project(st)
}
fn draft(list: Vec<&str>) -> Stores {
    stores(&[("cc.edited", Value::Bool(true)), ("cc.order", ids(list))])
}
const ALL: [&str; 6] = [
    "audio.set_mute",
    "audio.set_volume",
    "network.set_wifi",
    "bluetooth.set_power",
    "notifications.set_dnd",
    SET,
];

#[test]
fn launcher_quick_runtime_calls_only_on_release_and_keeps_search_focus() {
    use bead::wayland::{SceneSource, UiEvent};
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    crate::i18n::init();
    let mut lab = rt.block_on(lab(&[
        "notifications.set_dnd",
        "night_light.set",
        "stay_awake.set",
    ]));
    let h = Harness::load(
        &root(),
        Path::new("ui/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        bead_test::Config {
            width: 1920,
            height: 1200,
            scale: 1.,
        },
    )
    .unwrap();
    let mut runtime = crate::reload::Runtime::new(h.session)
        .with_launcher(None)
        .unwrap()
        .with_launcher_quick(Quick::lease(lab.client.clone()).unwrap())
        .unwrap();
    for _ in 0..500 {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if runtime
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .count()
            == 3
        {
            break;
        }
        rt.block_on(tokio::time::sleep(Duration::from_millis(10)));
    }
    for (id, method) in [
        ("actions.controls.dnd", "notifications.set_dnd"),
        ("actions.controls.night-light", "night_light.set"),
        ("actions.controls.stay-awake", "stay_awake.set"),
    ] {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|node| {
                node.action.as_deref() == Some("launcher.quick")
                    && node.action_arg == Some(Value::Text(id.into()))
            })
            .expect("published Quick button");
        let rect = bead::ui::rect(&node.draw);
        let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        runtime.stage(UiEvent::Hover { x, y }).unwrap();
        runtime.commit();
        runtime.stage(UiEvent::Click { x, y }).unwrap();
        runtime.commit();
        assert!(
            lab.calls.try_recv().is_err(),
            "press must not change daemon state"
        );
        runtime.stage(UiEvent::Release).unwrap();
        runtime.commit();
        assert_eq!(
            rt.block_on(call(&mut lab)),
            (method.into(), json!({"enabled": true}))
        );
        assert_eq!(
            runtime.session.stores.get("bead.focused"),
            Some(&Value::Text("launcher.search".into()))
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_quick_calls_published_toggles_and_releases_all_topic_leases() {
    let methods = ["notifications.set_dnd", "night_light.set", "stay_awake.set"];
    let mut controls = controls();
    controls["night_light"]["enabled"] = json!(true);
    controls["stay_awake"]["enabled"] = json!(true);
    let mut notices = notifications();
    notices["do_not_disturb"] = json!(true);
    let mut lab = lab_with_payloads(&methods, controls, Vec::new(), notices).await;
    let quick = Quick::lease(lab.client.clone()).unwrap();
    let mut projected = BTreeMap::new();
    for _ in 0..500 {
        projected = quick.project(&Stores::default());
        let Value::List(rows) = &projected["launcher.quick"] else {
            panic!("quick rows")
        };
        if rows
            .iter()
            .all(|row| row["status"].as_str().is_some_and(|s| s != "—"))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(projected["launcher.updates.count"], Value::Number(0.));
    let Value::List(rows) = &projected["launcher.quick"] else {
        panic!("quick rows")
    };
    assert!(
        rows.iter()
            .all(|row| row["on"] == true && row["enabled"] == true)
    );
    let ids = [
        (
            "actions.controls.dnd",
            "notifications.set_dnd",
            json!({"enabled": false}),
        ),
        (
            "actions.controls.night-light",
            "night_light.set",
            json!({"enabled": false}),
        ),
        (
            "actions.controls.stay-awake",
            "stay_awake.set",
            json!({"enabled": false}),
        ),
    ];
    for (index, (id, method, params)) in ids.iter().enumerate() {
        quick.act(id);
        if index == 0 {
            // The first request occupies the source's guarded in-flight slot immediately.
            quick.act(id);
        }
        assert_eq!(call(&mut lab).await, ((*method).into(), params.clone()));
        for _ in 0..30 {
            quick.project(&Stores::default());
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    assert!(
        lab.calls.try_recv().is_err(),
        "duplicate pending action was sent"
    );
    drop(quick);
    let mut released = Vec::new();
    for _ in 0..500 {
        while let Ok(topics) = lab.unsubs.try_recv() {
            released.extend(topics);
        }
        if ["controls", "notifications", "dashboard.updates"]
            .iter()
            .all(|topic| released.iter().any(|item| item == topic))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for topic in ["controls", "notifications", "dashboard.updates"] {
        assert!(
            released.iter().any(|item| item == topic),
            "missing lease release for {topic}: {released:?}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_quick_reuses_permanent_notifications_subscription() {
    let mut lab = lab_with_permanent(&[], controls(), vec!["notifications".into()]).await;
    let quick = Quick::lease(lab.client.clone()).unwrap();
    let mut projected = BTreeMap::new();
    for _ in 0..500 {
        projected = quick.project(&Stores::default());
        let Value::List(rows) = &projected["launcher.quick"] else {
            panic!("quick rows")
        };
        if rows[0]["status"]
            .as_str()
            .is_some_and(|status| status != "—")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let Value::List(rows) = &projected["launcher.quick"] else {
        panic!("quick rows")
    };
    assert_eq!(rows[0]["on"], false);
    drop(quick);
    let mut released = Vec::new();
    for _ in 0..500 {
        while let Ok(topics) = lab.unsubs.try_recv() {
            released.extend(topics);
        }
        if released.contains(&"controls".into()) && released.contains(&"dashboard.updates".into()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(released.contains(&"controls".into()));
    assert!(released.contains(&"dashboard.updates".into()));
    assert!(
        !released.contains(&"notifications".into()),
        "permanent notifications topic was unsubscribed"
    );
}

/// The UI provider owns a permanent notifications subscription in daemon mode.
/// Opening and closing either control view must share it and retain history.
#[tokio::test(flavor = "current_thread")]
async fn notification_views_share_permanent_history_and_release_only_their_leases() {
    let notices = json!({"health": health(), "do_not_disturb": false,
        "history_count": 1, "history_more": false, "history_storage": {"ready": true, "writable": true},
        "history": {"order": ["saved"], "items": {"saved": {"app_name": "Fixture", "summary": "Saved notification", "body": "From disk",
            "icon":"fixture-mail","desktop_entry":"fixture-mail.desktop","image_path":"/tmp/fixture-mail.png"}}}});
    let mut lab = lab_with_payloads(
        &["notifications.clear"],
        controls(),
        vec!["notifications".into()],
        notices,
    )
    .await;
    let control = ControlCenter::new(
        Some(lab.client.clone()),
        ["notifications", "controls"]
            .into_iter()
            .map(|name| (name, lab.client.view_topic(name).unwrap()))
            .collect(),
        None,
        Vec::new(),
    );
    let mut projected = BTreeMap::new();
    for _ in 0..500 {
        projected = control.project(&Stores::default());
        if projected.get("cc.notice_ready") == Some(&Value::Bool(true)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(projected["cc.notice_ready"], Value::Bool(true));
    assert_eq!(projected["cc.notice_count"], Value::Number(1.));
    let Value::List(rows) = &projected["cc.notices"] else {
        panic!("history rows")
    };
    assert_eq!(rows[0]["summary"], "Saved notification");
    assert_eq!(rows[0]["icon"], "fixture-mail");
    assert_eq!(rows[0]["desktop_entry"], "fixture-mail.desktop");
    assert_eq!(rows[0]["image_path"], "/tmp/fixture-mail.png");
    assert_eq!(projected["cc.notice_clear"], Value::Bool(true));
    drop(control);
    let mut released = Vec::new();
    for _ in 0..500 {
        while let Ok(topics) = lab.unsubs.try_recv() {
            released.extend(topics);
        }
        if released.contains(&"controls".into()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(released.contains(&"controls".into()));
    assert!(!released.contains(&"notifications".into()));
    let reopened = lab.client.view_topic("notifications").unwrap();
    assert_eq!(reopened.snapshot().data["history_count"], 1);
}

#[tokio::test(flavor = "current_thread")]
async fn tiles_call_only_published_methods_with_the_current_state() {
    let mut lab = lab(&ALL).await;
    let control = control_for(&lab).await;
    let out = control.project(&Stores::default());
    assert_eq!(source(&out, "network.wifi")["toggle"], true);
    assert_eq!(source(&out, "audio.output")["set"], true);
    assert_eq!(source(&out, "audio.input")["toggle"], true);
    assert_eq!(
        source(&out, "brightness.level")["set"],
        false,
        "not advertised"
    );
    assert_eq!(out["cc.writable"], Value::Bool(true));

    control.act("toggle:network.wifi");
    assert_eq!(
        call(&mut lab).await,
        ("network.set_wifi".into(), json!({"enabled": false}))
    );
    control.act("toggle:bluetooth.power");
    assert_eq!(
        call(&mut lab).await,
        (
            "bluetooth.set_power".into(),
            json!({"adapter": "hci0", "powered": false})
        )
    );
    control.act("toggle:notifications.dnd");
    assert_eq!(
        call(&mut lab).await,
        ("notifications.set_dnd".into(), json!({"enabled": true}))
    );
    control.act("toggle:audio.output");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_mute".into(),
            json!({"target": "sink", "muted": true})
        )
    );
    control.act("toggle:audio.input");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_mute".into(),
            json!({"target": "source", "muted": false})
        )
    );
    // The slider is perceptual, the daemon's level linear: the cube of the slider.
    settle(&control, &Stores::default()).await;
    control.act("set:audio.output:60");
    let (method, params) = call(&mut lab).await;
    assert_eq!(method, "audio.set_volume");
    assert_eq!(params["target"], "sink");
    assert!((params["value"].as_f64().unwrap() - 0.216).abs() < 1e-9);
    settle(&control, &Stores::default()).await;
    control.act("set:audio.input:100");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_volume".into(),
            json!({"target": "source", "value": 1.0})
        )
    );
    // Unadvertised verbs, unknown sources and garbage send nothing.
    control.act("set:brightness.level:50");
    control.act("press:screenshot.capture");
    control.act("set:audio.output:nan");
    control.act("");
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    // Absolute targets must be sent even if a possibly stale topic already matches.
    control.act("set:audio.output:50");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_volume".into(),
            json!({"target":"sink","value":0.125})
        )
    );
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    control.act("set:audio.output:100");
    let (_, params) = call(&mut lab).await;
    assert_eq!(params["value"], 1.0);
}

const CONTROLS: [&str; 12] = [
    "brightness.set",
    "keyboard_light.set",
    "power.set_profile",
    "airplane.set",
    "night_light.set",
    "stay_awake.set",
    "recorder.start",
    "recorder.stop",
    "screenshot.capture",
    "color_picker.pick",
    "wallpaper.next",
    "appearance.set_mode",
];

#[tokio::test(flavor = "current_thread")]
async fn every_n0046_tile_calls_its_method_with_absolute_values() {
    let mut lab = lab(&CONTROLS).await;
    let control = control_for(&lab).await;
    let out = control.project(&Stores::default());
    let brightness = source(&out, "brightness.level");
    assert_eq!(
        (brightness["value"].clone(), brightness["set"].clone()),
        (json!(60.), json!(true)),
        "the internal panel, not the first output"
    );
    assert_eq!(source(&out, "keyboard.backlight")["value"], 33.);
    assert_eq!(source(&out, "keyboard.backlight")["value_steps"], 3.);
    assert_eq!(source(&out, "appearance.mode")["status"], "Dark");
    assert_eq!(source(&out, "recorder.state")["status"], "Off");
    let profile = source(&out, "power.profile");
    assert_eq!(profile["choices"].as_array().unwrap().len(), 2);
    assert_eq!(profile["choices"][1]["active"], true);

    for (arg, method, params) in [
        (
            "set:brightness.level:70",
            "brightness.set",
            json!({"output": "eDP-1", "percent": 70}),
        ),
        (
            "set:keyboard.backlight:100",
            "keyboard_light.set",
            json!({"level": 3}),
        ),
        (
            "choose:power.profile:power-saver",
            "power.set_profile",
            json!({"profile": "power-saver"}),
        ),
        (
            "toggle:airplane.mode",
            "airplane.set",
            json!({"enabled": true}),
        ),
        (
            "toggle:night-light.state",
            "night_light.set",
            json!({"enabled": true}),
        ),
        (
            "toggle:stay-awake.state",
            "stay_awake.set",
            json!({"enabled": true}),
        ),
        (
            "toggle:appearance.mode",
            "appearance.set_mode",
            json!({"mode": "auto"}),
        ),
        ("press:wallpaper.next", "wallpaper.next", json!({})),
    ] {
        settle(&control, &Stores::default()).await;
        control.act(arg);
        assert_eq!(call(&mut lab).await, (method.into(), params), "{arg}");
        assert!(!control.take_close(), "{arg} needs no closed panel");
    }
    // An absolute keyboard level is sent; unknown profiles and sources send nothing.
    settle(&control, &Stores::default()).await;
    control.act("set:keyboard.backlight:33");
    assert_eq!(
        call(&mut lab).await,
        ("keyboard_light.set".into(), json!({"level":1}))
    );
    control.act("choose:power.profile:turbo");
    control.act("press:vpn.state");
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn interactive_tools_close_the_panel_first_and_call_after() {
    let mut lab = lab(&CONTROLS).await;
    let control = control_for(&lab).await;
    for (arg, method, params) in [
        (
            "press:screenshot.capture",
            "screenshot.capture",
            json!({"mode": "region"}),
        ),
        ("press:color-picker.pick", "color_picker.pick", json!({})),
        (
            "toggle:recorder.state",
            "recorder.start",
            json!({"mode": "region"}),
        ),
    ] {
        control.act(arg);
        assert!(
            control.take_close(),
            "{arg} asks the host to close the panel"
        );
        assert!(!control.take_close(), "once");
        assert!(
            lab.calls.try_recv().is_err(),
            "{arg} waits for the panel to go"
        );
        assert_eq!(call(&mut lab).await, (method.into(), params), "{arg}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_running_recording_is_stopped_without_closing_anything() {
    let mut running = controls();
    running["recorder"] = json!({"recording": true});
    let mut lab = lab_with(&CONTROLS, running).await;
    let control = control_for(&lab).await;
    let out = control.project(&Stores::default());
    assert_eq!(source(&out, "recorder.state")["on"], true);
    assert_eq!(source(&out, "recorder.state")["status"], "Recording");
    control.act("toggle:recorder.state");
    assert_eq!(call(&mut lab).await, ("recorder.stop".into(), json!({})));
    assert!(!control.take_close());
}

#[tokio::test(flavor = "current_thread")]
async fn tiles_exist_exactly_when_their_member_and_method_are_published() {
    // Members without a method: shown read-only where the member itself is the capability.
    let mut lab = lab(&["airplane.set", "wallpaper.next"]).await;
    let control = control_for(&lab).await;
    let out = control.project(&Stores::default());
    assert_eq!(source(&out, "airplane.mode")["toggle"], true);
    assert_eq!(source(&out, "stay-awake.state")["toggle"], false);
    assert_eq!(source(&out, "keyboard.backlight")["set"], false);
    assert_eq!(source(&out, "wallpaper.next")["press"], true);
    let Value::List(all) = &out["cc.src"] else {
        panic!()
    };
    assert!(
        all.iter()
            .all(|s| s["id"] != "screenshot.capture" && s["id"] != "color-picker.pick")
    );
    control.act("toggle:stay-awake.state");
    control.act("press:screenshot.capture");
    assert!(!control.take_close());
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    // A backend that is absent leaves no member: no source, no tile.
    let mut bare = controls();
    for member in [
        "keyboard",
        "airplane",
        "night_light",
        "stay_awake",
        "recorder",
        "appearance",
    ] {
        bare.as_object_mut().unwrap().remove(member);
    }
    bare["brightness"] = json!({});
    let topics = TOPICS
        .iter()
        .map(|name| {
            let data = if *name == "controls" {
                bare.clone()
            } else {
                topic_data(name)
            };
            (
                *name,
                Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    let out = ControlCenter::new(None, topics, None, views()).project(&Stores::default());
    let Value::List(all) = &out["cc.src"] else {
        panic!()
    };
    let ids: Vec<&str> = all.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(
        ids,
        [
            "audio.output",
            "audio.input",
            "network.wifi",
            "bluetooth.power",
            "vpn.state",
            "power.profile",
            "notifications.dnd"
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn the_edit_draft_is_written_to_the_tile_store_one_revision_at_a_time() {
    let mut lab = lab(&ALL).await;
    let control = control_for(&lab).await;
    // Unedited stores write nothing, however often the panel projects.
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());

    let first = draft(vec!["network", "volume", "dnd"]);
    control.project(&first);
    assert_eq!(
        call(&mut lab).await,
        (
            SET.into(),
            json!({"store": "control-center", "key": ["tiles"], "value": ["network", "volume", "dnd"], "rev": 3})
        )
    );
    // The same draft is not sent twice.
    settle(&control, &first).await;
    assert!(lab.calls.try_recv().is_err());
    // The next one uses the revision the daemon returned.
    let second = draft(vec!["network", "dnd"]);
    let out = settle(&control, &second).await;
    let (method, params) = call(&mut lab).await;
    assert_eq!(
        (method.as_str(), &params["value"], &params["rev"]),
        (SET, &json!(["network", "dnd"]), &json!(4))
    );
    assert_eq!(out["cc.message"], Value::Text(String::new()));
    // The saved order follows the store topic once the daemon publishes it.
    // Removing everything is a valid layout.
    settle(&control, &draft(vec![])).await;
    let (_, params) = call(&mut lab).await;
    assert_eq!(params["value"], json!([]));
    // Invalid ids never reach the daemon.
    settle(&control, &draft(vec!["Bad Id"])).await;
    settle(&control, &draft(vec!["../x"])).await;
    assert!(lab.calls.try_recv().is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_write_shows_the_error_and_is_not_retried_until_the_draft_changes() {
    let mut lab = lab(&ALL).await;
    let control = control_for(&lab).await;
    lab.fail
        .lock()
        .unwrap()
        .insert(SET.into(), ErrorCode::PermissionDenied);
    let first = draft(vec!["dnd"]);
    control.project(&first);
    call(&mut lab).await;
    let out = settle(&control, &first).await;
    assert_eq!(out["cc.message"], Value::Text("Request failed".into()));
    assert!(lab.calls.try_recv().is_err(), "no retry loop");
    lab.fail.lock().unwrap().clear();
    let out = settle(&control, &draft(vec!["dnd", "network"])).await;
    let (_, params) = call(&mut lab).await;
    assert_eq!(params["value"], json!(["dnd", "network"]));
    assert_eq!(out["cc.writable"], Value::Bool(true));
}

#[tokio::test(flavor = "current_thread")]
async fn a_stale_revision_is_retried_once_then_reported() {
    let mut lab = lab(&ALL).await;
    let control = control_for(&lab).await;
    lab.fail
        .lock()
        .unwrap()
        .insert(SET.into(), ErrorCode::StaleRevision);
    let first = draft(vec!["dnd"]);
    control.project(&first);
    call(&mut lab).await;
    settle(&control, &first).await;
    // One automatic retry with the topic's revision, then the message.
    call(&mut lab).await;
    let out = settle(&control, &first).await;
    assert_eq!(
        out["cc.message"],
        Value::Text("Changed elsewhere; try again".into())
    );
    assert!(lab.calls.try_recv().is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn without_the_write_route_the_editor_is_visible_but_does_not_persist() {
    let mut lab = lab(&ALL[..5]).await;
    let control = control_for(&lab).await;
    let out = settle(&control, &draft(vec!["dnd"])).await;
    assert_eq!(out["cc.writable"], Value::Bool(false));
    assert!(lab.calls.try_recv().is_err());
    assert_eq!(out["cc.saved"], ids(vec!["volume", "network", "dnd"]));
}

// ---- the Luau view ----------------------------------------------------------------------

fn base() -> BTreeMap<String, Value> {
    let mut out = offline().project(&Stores::default());
    // The offline adapter has no daemon link; the view tests want armed verbs.
    let Value::List(items) = out["cc.src"].clone() else {
        panic!()
    };
    let armed: Vec<Json> = items
        .iter()
        .map(|item| {
            let mut item = item.clone();
            let toggles = item["id"] != "power.profile";
            item["toggle"] = json!(toggles);
            item["set"] = json!(
                item["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("audio."))
            );
            item
        })
        .collect();
    out.insert("cc.src".into(), Value::list(armed));
    out.insert("cc.writable".into(), Value::Bool(true));
    out.insert("cc.max_height".into(), Value::Number(640.));
    out.insert(
        "cc.saved".into(),
        ids(vec![
            "volume",
            "mic-volume",
            "network",
            "bluetooth",
            "battery",
            "dnd",
            "mic",
            "clipboard",
            "updates",
            "brightness",
        ]),
    );
    out
}
fn harness(mut out: BTreeMap<String, Value>, page: &str, scale: f32) -> bead_test::Result<Harness> {
    out.insert("cc.page".into(), Value::Text(page.into()));
    Harness::load(
        &root(),
        Path::new("control/main.luau"),
        &root().join("palette.json"),
        out,
        bead_test::Config {
            width: 488,
            height: 704,
            scale,
        },
    )
}
fn golden(name: &str, h: &mut Harness, scale: f32) -> bead_test::Result<()> {
    let frame = h.render()?;
    let path = root().join(format!("tests/goldens/control-{name}-{scale}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("bead-control-{name}-{scale}-diff")),
    )
}
fn node_ids(h: &Harness) -> Vec<String> {
    h.session
        .view_nodes()
        .iter()
        .map(|n| n.id.clone())
        .collect()
}
fn centre(h: &Harness, id_part: &str) -> (f32, f32) {
    let node = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id.contains(id_part))
        .unwrap_or_else(|| panic!("no node {id_part} in {:?}", node_ids(h)));
    let r = bead::ui::rect(&node.draw);
    (r.x + r.w / 2., r.y + r.h / 2.)
}

#[test]
fn control_center_pages_match_goldens() -> bead_test::Result<()> {
    for scale in [1., 1.5] {
        for (name, page, detail) in [
            ("overview", "home", ""),
            ("detail", "detail", "dnd"),
            ("edit", "edit", ""),
            ("catalog", "catalog", ""),
        ] {
            let mut out = base();
            out.insert("cc.detail".into(), Value::Text(detail.into()));
            let mut h = harness(out, page, scale)?;
            let ids = node_ids(&h);
            match page {
                // Tiles whose source is published are live; the others are dimmed and inert.
                "home" => {
                    assert!(ids.iter().any(|i| i.contains("tile-volume")), "{ids:?}");
                    assert!(ids.iter().any(|i| i.contains("tile-clipboard")));
                    // An unavailable tile is drawn dimmed (as in the QML), under a veil, and inert.
                }
                "edit" => assert!(ids.iter().any(|i| i.contains(".slot-1"))),
                "catalog" => assert!(ids.iter().any(|i| i.contains("add-notifications"))),
                _ => assert!(ids.iter().any(|i| i.contains("notices-dnd"))),
            }
            golden(name, &mut h, scale)?;
        }
    }
    Ok(())
}

#[test]
fn dragging_a_tile_over_another_reorders_the_draft_and_a_new_press_starts_a_new_drag()
-> bead_test::Result<()> {
    let mut h = harness(base(), "home", 1.)?;
    let (ex, ey) = centre(&h, "status.edit");
    h.script([Input::Click { x: ex, y: ey }, Input::Release])?;
    assert_eq!(
        h.session.stores.get("cc.page"),
        Some(&Value::Text("edit".into()))
    );
    let Some(Value::List(entered)) = h.session.stores.get("cc.order").cloned() else {
        panic!()
    };
    assert_eq!(entered.len(), 10, "the whole stored order, shown or not");
    assert_eq!(
        h.session.stores.get("cc.edited"),
        Some(&Value::Bool(false)),
        "entering saves nothing"
    );
    let (ax, ay) = centre(&h, ".slot-1");
    let (bx, by) = centre(&h, ".slot-2");
    let (cx, cy) = centre(&h, ".slot-3");
    h.script([Input::Click { x: ax, y: ay }])?;
    assert_eq!(
        h.session.stores.get("cc.drag"),
        Some(&Value::Text("volume".into()))
    );
    assert_eq!(
        h.session.stores.get("cc.edited"),
        Some(&Value::Bool(false)),
        "a press alone changes nothing"
    );
    h.script([Input::Drag { x: bx, y: by }])?;
    // The draft is the shown order with `volume` moved down one place.
    let moved = h.session.stores.get("cc.order").cloned().unwrap();
    let Value::List(list) = &moved else { panic!() };
    assert_eq!(list[0]["id"], "mic-volume");
    assert_eq!(list[1]["id"], "volume");
    assert_eq!(h.session.stores.get("cc.edited"), Some(&Value::Bool(true)));
    h.script([Input::Release])?;
    // A new press on another tile starts another drag instead of moving the old one.
    let before = moved.clone();
    h.script([Input::Click { x: cx, y: cy }])?;
    assert_eq!(h.session.stores.get("cc.order"), Some(&before));
    let third = list[2]["id"].as_str().unwrap().to_owned();
    assert_eq!(h.session.stores.get("cc.drag"), Some(&Value::Text(third)));
    // Remove and reset.
    let (rx, ry) = centre(&h, ".over-1");
    h.script([
        Input::Release,
        Input::Click { x: rx, y: ry },
        Input::Release,
    ])?;
    let Some(Value::List(after)) = h.session.stores.get("cc.order") else {
        panic!()
    };
    assert!(after.iter().all(|item| item["id"] != "mic-volume"));
    Ok(())
}

#[test]
fn tiles_dispatch_host_actions_and_the_detail_page_opens_in_place() -> bead_test::Result<()> {
    let mut h = harness(base(), "home", 1.)?;
    let action = |h: &Harness, part: &str| {
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|n| n.id.contains(part))
            .unwrap_or_else(|| panic!("no node {part}"));
        (node.action.clone(), node.action_arg.clone())
    };
    // As in QML, the Wi-Fi tile opens its page (the switch lives there); a toggle tile still toggles.
    assert_eq!(
        action(&h, "tile-network.main"),
        (Some("detail".into()), Some(Value::Text("network".into())))
    );
    assert_eq!(
        action(&h, "tile-dnd.main"),
        (
            Some("cc.do".into()),
            Some(Value::Text("toggle:notifications.dnd".into()))
        )
    );
    assert_eq!(
        action(&h, "tile-clipboard.main"),
        (
            Some("cc.do".into()),
            Some(Value::Text("open:clipboard".into()))
        )
    );
    // The level slider sends percent steps for its own source.
    assert_eq!(
        action(&h, "tile-volume.level.hits.value-10"),
        (
            Some("cc.do".into()),
            Some(Value::Text("set:audio.output:50".into()))
        )
    );
    // A tile with an in-panel page opens it without leaving the panel.
    let (x, y) = centre(&h, "tile-dnd.detail-open");
    h.script([Input::Click { x, y }, Input::Release])?;
    assert_eq!(
        h.session.stores.get("cc.page"),
        Some(&Value::Text("detail".into()))
    );
    assert_eq!(
        h.session.stores.get("cc.detail"),
        Some(&Value::Text("dnd".into()))
    );
    Ok(())
}

#[test]
fn control_center_slider_drag_visual_maps_thumb_to_hit_range() -> bead_test::Result<()> {
    use bead::wayland::{SceneSource, UiEvent};

    let h = harness(base(), "home", 1.)?;
    let mut runtime = crate::reload::Runtime::new(h.session).with_control_center(offline())?;
    let scene = runtime
        .stage(UiEvent::Relayout)?
        .expect("control-center relayout scene");
    runtime.commit();

    let visuals = runtime.drag_visuals();
    let visual = visuals
        .iter()
        .find(|visual| visual.id.contains("tile-volume") && visual.id.ends_with(".thumb"))
        .expect("live volume slider exports its thumb visual");
    let layer = scene
        .layers
        .iter()
        .find(|layer| layer.id == visual.id)
        .expect("thumb has a matching scene layer");
    assert_eq!(
        layer.own.len(),
        1,
        "thumb must map to exactly one primitive"
    );
    assert_eq!(
        layer.subtree_end, layer.own.end,
        "thumb layer must have no children"
    );

    let bead::render::Primitive::Rounded { rect, fill, .. } = &scene.primitives[layer.own.start]
    else {
        panic!("kit slider thumb must be a rounded primitive");
    };
    assert!(fill[3] > 0, "the scene primitive must be visible");
    assert!(
        (rect.w - 8.).abs() < 0.01,
        "thumb width must remain 8 layout px"
    );
    assert!(
        (rect.h - 16.).abs() < 0.01,
        "thumb height must remain 16 layout px"
    );

    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id == visual.id)
        .expect("thumb view node");
    let bead::core::Value::List(records) = node.metadata.as_ref().expect("thumb metadata") else {
        panic!("thumb metadata records");
    };
    let motion = records[0]
        .get("pointer_motion")
        .expect("pointer-motion metadata");
    let key = motion
        .get("key")
        .and_then(Json::as_str)
        .expect("slider preview key");
    let fraction = motion
        .get("fraction")
        .and_then(Json::as_f64)
        .expect("slider fraction") as f32;
    let hits: Vec<_> = runtime
        .session
        .view_nodes()
        .iter()
        .filter(|node| node.preview_key == key)
        .collect();
    let first = hits.first().expect("slider hit regions");
    let first_rect = bead::ui::rect(&first.draw);
    let expected_pointer_start = first_rect.x + first.preview_inset;
    let expected_pointer_end = hits
        .iter()
        .map(|hit| {
            let rect = bead::ui::rect(&hit.draw);
            rect.x + rect.w
        })
        .fold(f32::NEG_INFINITY, f32::max)
        - first.preview_inset;
    assert!((visual.pointer.0 - expected_pointer_start).abs() < 0.01);
    assert!((visual.pointer.1 - expected_pointer_end).abs() < 0.01);

    let current_left = visual.travel.0 + fraction * (visual.travel.1 - visual.travel.0);
    assert!(
        (rect.x - current_left).abs() < 0.01,
        "thumb left must match slider fraction"
    );
    assert!(
        (rect.x + rect.w / 2.
            - (visual.pointer.0 + fraction * (visual.pointer.1 - visual.pointer.0)))
            .abs()
            < 0.01
    );

    let fill_visual = visual.fill.as_ref().expect("shared slider fill descriptor");
    assert!(fill_visual.id.ends_with(".level"));
    assert!(
        (fill_visual.rect.x + fill_visual.minimum - visual.travel.0 - rect.w / 2.).abs() < 0.001,
        "fill end and handle center share one origin"
    );

    let press_x = rect.x + rect.w / 2.;
    let press_y = visual.grab.y + visual.grab.h / 2.;
    runtime.stage(UiEvent::Click {
        x: press_x,
        y: press_y,
    })?;
    runtime.commit();
    let pressed = runtime
        .drag_visuals()
        .into_iter()
        .find(|candidate| candidate.id == visual.id)
        .expect("slider visual remains available after press");
    let target_fraction = 0.373;
    let target_x = pressed.pointer.0 + target_fraction * (pressed.pointer.1 - pressed.pointer.0);
    let target_y = pressed.grab.y + pressed.grab.h / 2.;
    let travel = pressed.travel;
    let moved_scene = runtime
        .stage(UiEvent::Drag {
            x: target_x,
            y: target_y,
        })?
        .expect("drag scene");
    runtime.commit();
    let moved = runtime
        .drag_visuals()
        .into_iter()
        .find(|candidate| candidate.id == visual.id)
        .expect("slider visual remains available after local drag feedback");
    let moved_layer = moved_scene
        .layers
        .iter()
        .find(|layer| layer.id == moved.id)
        .expect("moved layer");
    let bead::render::Primitive::Rounded {
        rect: moved_rect, ..
    } = &moved_scene.primitives[moved_layer.own.start]
    else {
        panic!("kit slider thumb remains rounded after drag");
    };
    let moved_node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id == moved.id)
        .expect("moved thumb node");
    let bead::core::Value::List(records) = moved_node.metadata.as_ref().expect("moved metadata")
    else {
        panic!("moved thumb metadata records");
    };
    let moved_fraction = records[0]
        .get("pointer_motion")
        .and_then(|value| value.get("fraction"))
        .and_then(Json::as_f64)
        .expect("moved slider fraction") as f32;
    assert!((moved_fraction - target_fraction).abs() < 0.002);
    assert!(
        moved_fraction.fract() != 0.,
        "drag feedback must retain a continuous value"
    );
    // Layout snaps independently at each pose; preserve the off-step value while
    // comparing the raster position within one logical pixel, not float epsilon.
    assert!(
        (moved_rect.x - (travel.0 + target_fraction * (travel.1 - travel.0))).abs() <= 1.,
        "thumb={} expected={}",
        moved_rect.x,
        travel.0 + target_fraction * (travel.1 - travel.0)
    );
    assert!(
        (moved_rect.x + moved_rect.w / 2. - target_x).abs() <= 1.,
        "centre={} pointer={target_x}",
        moved_rect.x + moved_rect.w / 2.
    );
    Ok(())
}

#[test]
fn the_profile_picker_and_one_shot_tiles_dispatch_their_verbs() -> bead_test::Result<()> {
    let mut out = base();
    let Value::List(items) = out["cc.src"].clone() else {
        panic!()
    };
    let mut items: Vec<Json> = items.to_vec();
    for item in &mut items {
        if item["id"] == "power.profile" {
            item["set"] = json!(false);
            item["choices"] = json!([
                {"id": "power-saver", "name": "Power-saver", "active": false},
                {"id": "balanced", "name": "Balanced", "active": true}
            ]);
        }
    }
    items.push(json!({"id": "screenshot.capture", "available": true, "press": true}));
    out.insert("cc.src".into(), Value::list(items));
    out.insert("cc.saved".into(), ids(vec!["battery", "screenshot"]));
    let mut h = harness(out.clone(), "home", 1.)?;
    let action = |h: &Harness, part: &str| {
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|n| n.id.contains(part))
            .unwrap_or_else(|| panic!("no node {part}: {:?}", node_ids(h)));
        (node.action.clone(), node.action_arg.clone())
    };
    assert_eq!(
        action(&h, "tile-screenshot"),
        (
            Some("cc.do".into()),
            Some(Value::Text("press:screenshot.capture".into()))
        )
    );
    // With a picker the tile opens it in place instead of the battery panel.
    let (x, y) = centre(&h, "tile-battery");
    h.script([Input::Click { x, y }, Input::Release])?;
    assert_eq!(
        h.session.stores.get("cc.page"),
        Some(&Value::Text("detail".into()))
    );
    assert_eq!(
        h.session.stores.get("cc.detail"),
        Some(&Value::Text("battery".into()))
    );
    assert_eq!(
        action(&h, "profiles.power-saver"),
        (
            Some("cc.do".into()),
            Some(Value::Text("choose:power.profile:power-saver".into()))
        )
    );
    Ok(())
}

#[test]
fn n0046_tiles_and_the_profile_picker_match_goldens() -> bead_test::Result<()> {
    let mut out = offline().project(&Stores::default());
    let Value::List(items) = out["cc.src"].clone() else {
        panic!()
    };
    let mut items: Vec<Json> = items.to_vec();
    for item in &mut items {
        let id = item["id"].as_str().unwrap_or("").to_owned();
        item["toggle"] = json!(!matches!(
            id.as_str(),
            "power.profile" | "brightness.level" | "keyboard.backlight"
        ));
        item["set"] = json!(matches!(
            id.as_str(),
            "audio.output" | "audio.input" | "brightness.level" | "keyboard.backlight"
        ));
        if id == "power.profile" {
            item["choices"] = json!([
                {"id": "power-saver", "name": "Power-saver", "active": false},
                {"id": "balanced", "name": "Balanced", "active": true}
            ]);
        }
        if id == "recorder.state" {
            item["on"] = json!(true);
            item["status"] = json!("Recording");
        }
    }
    for id in ["screenshot.capture", "color-picker.pick", "wallpaper.next"] {
        items.push(json!({"id": id, "available": true, "press": true}));
    }
    out.insert("cc.src".into(), Value::list(items));
    out.insert("cc.writable".into(), Value::Bool(true));
    out.insert("cc.max_height".into(), Value::Number(900.));
    out.insert(
        "cc.saved".into(),
        ids(vec![
            "volume",
            "brightness",
            "mic-volume",
            "keyboard",
            "airplane",
            "night",
            "awake",
            "appearance",
            "record",
            "battery",
            "mic",
            "screenshot",
            "picker",
            "next-wallpaper",
        ]),
    );
    let mut h = harness(out.clone(), "home", 1.)?;
    let present = node_ids(&h);
    for tile in [
        "brightness",
        "keyboard",
        "airplane",
        "night",
        "awake",
        "appearance",
        "record",
        "screenshot",
        "picker",
        "next-wallpaper",
    ] {
        assert!(
            present.iter().any(|i| i.contains(&format!("tile-{tile}"))),
            "{tile}"
        );
    }
    golden("n0046-overview", &mut h, 1.)?;
    out.insert("cc.detail".into(), Value::Text("battery".into()));
    let mut h = harness(out, "detail", 1.)?;
    golden("n0046-profiles", &mut h, 1.)
}

#[tokio::test(flavor = "current_thread")]
async fn audio_picker_rechecks_device_kind_and_uses_the_published_node_id() {
    let mut lab = lab(&["audio.set_default"]).await;
    let control = control_for(&lab).await;
    let devices = control.audio_devices();
    assert_eq!(devices.len(), 2);
    control.act(&format!("choose:audio.output:{IN}"));
    control.act("choose:audio.input:missing");
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    control.act(&format!("choose:audio.output:{OUT}"));
    assert_eq!(
        call(&mut lab).await,
        ("audio.set_default".into(), json!({"node":OUT}))
    );
}

// B0076: the real dock composes the same controlled adapters as bar popouts.
#[tokio::test(flavor = "current_thread")]
async fn docked_pages_keep_the_surface_route_actions_and_return_home() -> bead_test::Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    let mut methods = ALL.to_vec();
    methods.push("power.set_profile");
    let mut lab = lab(&methods).await;
    let control = control_for(&lab).await;
    let palette_path = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette_path)?;
    let cold = crate::sound::fallback(colors.get("surface")?);
    let session = bead::ui::Session::new(
        root(),
        "control/main.luau".into(),
        palette_path,
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::new(session)
        .with_control_center(control)?
        .with_docked_details(lab.client.clone())?;
    runtime.isolate_state(&lab.dir.join("dock-recovery"));
    runtime.services(Box::new(|_| {}));
    runtime.stage(UiEvent::Reload)?;
    runtime.commit();
    for (page, part, action, arg) in [
        ("network", "wifi-radio", "network.toggle_wifi", ""),
        ("bluetooth", "bluetooth-power", "bluetooth.toggle_power", ""),
        (
            "battery",
            "profiles.power-saver",
            "cc.do",
            "choose:power.profile:power-saver",
        ),
        ("audio", "audio.output.mute", "cc.do", "toggle:audio.output"),
    ] {
        runtime.stage(UiEvent::Model(BTreeMap::from([
            ("cc.page".into(), Value::Text("detail".into())),
            ("cc.detail".into(), Value::Text(page.into())),
            (
                "bead.focused".into(),
                Value::Text("control-center.detail.header.back".into()),
            ),
        ])))?;
        runtime.commit();
        let nodes = runtime.session.view_nodes();
        assert!(
            nodes
                .iter()
                .any(|node| node.id == "control-center.detail.header.back"),
            "{page}"
        );
        let node = nodes
            .iter()
            .find(|node| node.id.contains(part))
            .unwrap_or_else(|| {
                panic!(
                    "{page}: missing {part}: {:?}",
                    nodes.iter().map(|n| (&n.id, &n.action)).collect::<Vec<_>>()
                )
            });
        assert_eq!(node.action.as_deref(), Some(action), "{page}");
        if !arg.is_empty() {
            assert_eq!(node.action_arg, Some(Value::Text(arg.into())));
        }
        // At both reference scales the dock's new page is a reviewed golden.
        for scale in [1., 1.5] {
            let mut h = harness(runtime.session.stores.values().clone(), "detail", scale)?;
            golden(&format!("docked-{page}"), &mut h, scale)?;
        }
        let rect = bead::ui::rect(&node.draw);
        runtime.stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })?;
        runtime.commit();
        let expected = match page {
            "network" => ("network.set_wifi", json!({"enabled":false})),
            "bluetooth" => (
                "bluetooth.set_power",
                json!({"adapter":"hci0","powered":false}),
            ),
            "battery" => ("power.set_profile", json!({"profile":"power-saver"})),
            _ => ("audio.set_mute", json!({"target":"sink","muted":true})),
        };
        assert_eq!(
            call(&mut lab).await,
            (expected.0.into(), expected.1),
            "{page}"
        );
        runtime.stage(UiEvent::Command {
            command: "escape".into(),
            shift: false,
        })?;
        runtime.commit();
        assert_eq!(
            runtime.session.stores.get("cc.page"),
            Some(&Value::Text("home".into()))
        );
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(true))
        );
    }
    for event in [
        UiEvent::Model(BTreeMap::from([(
            "cc.page".into(),
            Value::Text("home".into()),
        )])),
        UiEvent::PanelVisibility(false),
    ] {
        runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([
            (
                "network.secret".into(),
                Value::Text("private wifi password".into()),
            ),
            ("bluetooth.secret".into(), Value::Text("123456".into())),
        ]))?;
        runtime.stage(event)?;
        runtime.commit();
        for key in ["network.secret", "bluetooth.secret"] {
            assert_eq!(
                runtime.session.stores.get(key),
                Some(&Value::Text(String::new()))
            );
        }
    }
    drop(runtime);
    let mut retired = std::collections::BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while retired.len() < TOPICS.len() + 1 {
            retired.extend(lab.unsubs.recv().await.expect("lease release"));
        }
    })
    .await?;
    assert!(
        TOPICS
            .iter()
            .chain([&STORE_TOPIC])
            .all(|topic| retired.contains(*topic))
    );
    Ok(())
}

#[test]
fn unsupported_tiles_hide_and_catalog_order_is_the_unsaved_default() -> bead_test::Result<()> {
    let mut out = base();
    out.insert("cc.saved_known".into(), Value::Bool(false));
    let mut h = harness(out.clone(), "home", 1.)?;
    let y = |needle: &str| {
        bead::ui::rect(
            &h.session
                .view_nodes()
                .iter()
                .find(|node| node.id.contains(needle))
                .unwrap()
                .draw,
        )
        .y
    };
    assert!(y("tile-volume.toggle") < y("tile-mic-volume.toggle"));
    golden("default-order", &mut h, 1.)?;
    let Value::List(items) = out["cc.src"].clone() else {
        panic!()
    };
    out.insert(
        "cc.src".into(),
        Value::list(
            items
                .iter()
                .filter(|item| {
                    !matches!(
                        item["id"].as_str(),
                        Some("audio.output" | "brightness.level" | "keyboard.backlight")
                    )
                })
                .cloned()
                .collect(),
        ),
    );
    let mut h = harness(out, "home", 1.)?;
    golden("unsupported", &mut h, 1.)?;
    Ok(())
}

// ---- B0082: the N0059 producers' real shapes ---------------------------------------------

const BLUEZ: &str = "bluez_output.AA_BB_CC_DD_EE_FF.1";
const USB_IN: &str = "alsa_input.usb-Other_Mic-00.analog-stereo";
/// What the daemon publishes with several devices: ids are PipeWire `node.name`s, labels are the
/// device descriptions, the Bluetooth sink is not ready yet and has no readback.
fn busy_audio() -> Json {
    json!({"health": health(), "default_output": OUT, "default_input": IN, "nodes": {
        "order": [OUT, BLUEZ, IN, USB_IN],
        "items": {
            OUT: {"name": "Built-in Audio Analog Stereo", "kind": "output", "volume": 0.125, "muted": false, "ready": true,
                "readback": {"linear_volume": 0.125, "perceptual_volume": 0.5, "muted": false}},
            BLUEZ: {"name": "Fixture Headphones", "kind": "output", "volume": 0.0, "muted": false, "ready": false, "readback": {}},
            IN: {"name": "Fixture Mic", "kind": "input", "volume": 0.512, "muted": false, "ready": true,
                "readback": {"linear_volume": 0.512, "perceptual_volume": 0.8, "muted": false}},
            USB_IN: {"name": "Other Mic", "kind": "input", "volume": 1.0, "muted": true, "ready": true,
                "readback": {"linear_volume": 1.0, "perceptual_volume": 1.0, "muted": true}}}}})
}
const AUDIO_CALLS: [&str; 3] = ["audio.set_volume", "audio.set_mute", "audio.set_default"];

#[tokio::test(flavor = "current_thread")]
async fn audio_from_the_daemon_is_live_choosable_and_addressed_by_node_name() {
    let mut lab = lab_topics(&AUDIO_CALLS, &[("audio", busy_audio())]).await;
    let control = control_with(&lab, 3).await;
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(source(&out, "audio.output")["status"], "50%");
    assert_eq!(source(&out, "audio.output")["set"], true);
    assert_eq!(source(&out, "audio.input")["value"], 80.);
    // Devices in the daemon's order, the defaults selected, a device that is not ready listed but inert.
    let devices = control.audio_devices();
    let row = |id: &str| {
        devices
            .iter()
            .find(|d| d["id"] == id)
            .unwrap_or_else(|| panic!("{id}"))
    };
    assert_eq!(
        devices
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [OUT, BLUEZ, IN, USB_IN]
    );
    assert_eq!(row(OUT)["label"], "Built-in Audio Analog Stereo");
    assert_eq!(
        (row(OUT)["selected"].clone(), row(OUT)["enabled"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(
        (
            row(BLUEZ)["selected"].clone(),
            row(BLUEZ)["enabled"].clone()
        ),
        (json!(false), json!(false))
    );
    assert_eq!(
        (
            row(USB_IN)["selected"].clone(),
            row(USB_IN)["enabled"].clone()
        ),
        (json!(false), json!(true))
    );
    control.act(&format!("choose:audio.output:{BLUEZ}"));
    control.act(&format!("choose:audio.output:{USB_IN}"));
    settle(&control, &Stores::default()).await;
    assert!(
        lab.calls.try_recv().is_err(),
        "not ready, or the wrong kind"
    );
    control.act(&format!("choose:audio.input:{USB_IN}"));
    assert_eq!(
        call(&mut lab).await,
        ("audio.set_default".into(), json!({"node": USB_IN}))
    );
    settle(&control, &Stores::default()).await;
    control.act("toggle:audio.output");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_mute".into(),
            json!({"target": "sink", "muted": true})
        )
    );
    settle(&control, &Stores::default()).await;
    control.act("set:audio.input:50");
    let (method, params) = call(&mut lab).await;
    assert_eq!(
        (method.as_str(), &params["target"]),
        ("audio.set_volume", &json!("source"))
    );
    assert!((params["value"].as_f64().unwrap() - 0.125).abs() < 1e-9);
}

// B0086 (N0061): application streams are listed from the `audio` topic and addressed by their id.
#[tokio::test(flavor = "current_thread")]
async fn application_streams_list_with_perceptual_levels_and_call_the_stream_target() {
    let mut data = busy_audio();
    for (id, node) in [
        (
            "sink-input:7",
            json!({"name": "Fixture player", "kind": "playback", "volume": 0.25, "muted": false,
            "ready": true, "icon": "no-such-theme-icon-b0086"}),
        ),
        (
            "source-output:3",
            json!({"name": "", "kind": "capture", "volume": 1.0, "muted": true, "ready": true}),
        ),
        (
            "sink-input:9",
            json!({"name": "Not ready", "kind": "playback", "volume": 0.0, "muted": false, "ready": false}),
        ),
    ] {
        data["nodes"]["order"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
        data["nodes"]["items"][id] = node;
    }
    let mut lab = lab_topics(&AUDIO_CALLS, &[("audio", data)]).await;
    let control = control_with(&lab, 3).await;
    settle(&control, &Stores::default()).await;
    let rows = control.audio_streams();
    // Devices are not streams; a stream that is not ready has no level to show.
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["sink-input:7", "source-output:3"]
    );
    assert_eq!(rows[0]["label"], "Fixture player");
    assert_eq!(
        rows[0]["icon"], "",
        "an icon the theme lacks falls back to the apps glyph"
    );
    assert_eq!(
        (rows[0]["value"].clone(), rows[0]["status"].clone()),
        (json!(63.), json!("63%"))
    );
    assert_eq!(rows[0]["enabled"], true);
    assert_eq!(
        (rows[1]["muted"].clone(), rows[1]["label"].clone()),
        (json!(true), json!("Application"))
    );
    // Mute and volume name the stream target; the slider is perceptual and the call linear, as for a device (B0095).
    control.act("toggle:stream:sink-input:7");
    assert_eq!(
        call(&mut lab).await,
        (
            "audio.set_mute".into(),
            json!({"target": "stream", "node": "sink-input:7", "muted": true})
        )
    );
    settle(&control, &Stores::default()).await;
    control.act("set:stream:source-output:3:40");
    let (method, params) = call(&mut lab).await;
    assert_eq!(method, "audio.set_volume");
    assert_eq!(
        (&params["target"], &params["node"]),
        (&json!("stream"), &json!("source-output:3"))
    );
    assert!((params["value"].as_f64().unwrap() - 0.064).abs() < 1e-9);
    settle(&control, &Stores::default()).await;
    // Unknown or malformed stream ids send nothing.
    for arg in [
        "toggle:stream:sink-input:99",
        "toggle:stream:sink:7",
        "set:stream:sink-input:7;x:50",
        "set:stream:sink-input:7:nan",
        "toggle:stream:alsa_output.fake",
    ] {
        control.act(arg);
    }
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
}

#[test]
fn audio_that_is_not_ready_or_unavailable_has_no_tiles() {
    let sources = |audio: Json| {
        let topics = TOPICS
            .iter()
            .map(|name| {
                let data = if *name == "audio" {
                    audio.clone()
                } else {
                    topic_data(name)
                };
                (
                    *name,
                    Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>,
                )
            })
            .collect();
        let control = ControlCenter::new(None, topics, None, views());
        let out = control.project(&Stores::default());
        let Value::List(items) = &out["cc.src"] else {
            panic!()
        };
        let ids = items
            .iter()
            .filter_map(|s| s["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        (ids, control.audio_devices().len())
    };
    // A default output that is not ready and has no readback is not shown as 0% or unmuted.
    let mut unready = busy_audio();
    unready["default_output"] = json!(BLUEZ);
    let (ids, devices) = sources(unready);
    assert!(!ids.iter().any(|id| id == "audio.output") && ids.iter().any(|id| id == "audio.input"));
    assert_eq!(devices, 4);
    // Only the legacy members of a ready node: the cube root of the linear volume.
    let mut legacy = busy_audio();
    legacy["nodes"]["items"][OUT]["readback"] = json!(null);
    legacy["nodes"]["items"][OUT]
        .as_object_mut()
        .unwrap()
        .remove("readback");
    assert!(sources(legacy).0.iter().any(|id| id == "audio.output"));
    // No default input (no microphone): the microphone tiles disappear, the output stays.
    let mut no_mic = busy_audio();
    no_mic.as_object_mut().unwrap().remove("default_input");
    let (ids, _) = sources(no_mic);
    assert!(ids.iter().any(|id| id == "audio.output") && !ids.iter().any(|id| id == "audio.input"));
    // The topic says the backend is unavailable (no PipeWire/pactl): nothing is offered.
    let mut down = json!({"health": {"available": false, "stale": false}, "nodes": {"items": {}, "order": []}});
    let (ids, devices) = sources(down.clone());
    assert!(!ids.iter().any(|id| id.starts_with("audio.")));
    assert_eq!(devices, 0);
    down["health"]["stale"] = json!(true);
    assert_eq!(sources(down).1, 0);
}

fn profiles_as_daemon_names_them(active: &str) -> Json {
    json!({"health": health(), "active": active, "degraded": "thermal",
        "holds": [{"profile": "balanced", "reason": "Fixture hold", "application_id": "org.example.fixture"}],
        "profiles": {"order": ["balanced", "performance", "power-saver"], "items": {
            "balanced": {"name": "Balanced", "available": true},
            "performance": {"name": "Performance", "available": false, "reason": "Unavailable on this device"},
            "power-saver": {"name": "Power saver", "available": true}}}})
}

#[tokio::test(flavor = "current_thread")]
async fn power_profiles_follow_the_daemon_ids_availability_and_degradation() {
    let mut lab = lab_topics(
        &["power.set_profile"],
        &[(
            "power.profiles",
            profiles_as_daemon_names_them("power-saver"),
        )],
    )
    .await;
    let control = control_with(&lab, 3).await;
    let out = settle(&control, &Stores::default()).await;
    let profile = source(&out, "power.profile");
    assert_eq!(
        (profile["status"].clone(), profile["icon"].clone()),
        (json!("Power saver"), json!("eco"))
    );
    assert_eq!(profile["note"], "thermal");
    // Display names do not matter, ids do; an unavailable profile is not offered.
    let choices: Vec<_> = profile["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap(),
                c["name"].as_str().unwrap(),
                c["active"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        choices,
        [
            ("balanced", "Balanced", false),
            ("power-saver", "Power saver", true)
        ]
    );
    control.act("choose:power.profile:performance");
    control.act("choose:power.profile:turbo");
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    control.act("choose:power.profile:balanced");
    assert_eq!(
        call(&mut lab).await,
        ("power.set_profile".into(), json!({"profile": "balanced"}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_system_without_a_profile_daemon_keeps_an_explained_inert_tile() {
    // The producer's unavailable state: no active profile, no entries.
    let none = json!({"health": {"available": false, "stale": false}, "profiles": {"items": {}, "order": []}});
    let lab = lab_topics(&["power.set_profile"], &[("power.profiles", none)]).await;
    let control = control_with(&lab, 3).await;
    let out = settle(&control, &Stores::default()).await;
    let profile = source(&out, "power.profile");
    assert_eq!(profile["status"], "Power profiles unavailable");
    assert_eq!(
        (
            profile["set"].clone(),
            profile["choices"].clone(),
            profile["toggle"].clone()
        ),
        (json!(false), json!([]), json!(false))
    );
    // The dimmed, stale state of a lost daemon is still no source at all.
    let mut stale = profiles_as_daemon_names_them("balanced");
    stale["health"]["stale"] = json!(true);
    let topics = TOPICS
        .iter()
        .map(|name| {
            (
                *name,
                Arc::new(
                    FakeSource::new(if *name == "power.profiles" {
                        stale.clone()
                    } else {
                        topic_data(name)
                    })
                    .unwrap(),
                ) as Arc<dyn Source>,
            )
        })
        .collect();
    let out = ControlCenter::new(None, topics, None, views()).project(&Stores::default());
    let Value::List(items) = &out["cc.src"] else {
        panic!()
    };
    assert!(items.iter().all(|s| s["id"] != "power.profile"));
}

#[test]
fn the_battery_header_reads_the_producers_state_and_estimate() {
    let header = |battery: Json| {
        let topics = TOPICS
            .iter()
            .map(|name| {
                (
                    *name,
                    Arc::new(
                        FakeSource::new(if *name == "battery" {
                            battery.clone()
                        } else {
                            topic_data(name)
                        })
                        .unwrap(),
                    ) as Arc<dyn Source>,
                )
            })
            .collect();
        let out = ControlCenter::new(None, topics, None, views()).project(&Stores::default());
        let text = |key: &str| match &out[key] {
            Value::Text(t) => t.clone(),
            other => panic!("{key} {other:?}"),
        };
        (
            text("cc.battery"),
            text("cc.battery_icon"),
            text("cc.battery_estimate"),
        )
    };
    let s = |a: &str, b: &str, c: &str| (a.to_owned(), b.to_owned(), c.to_owned());
    assert_eq!(
        header(battery()),
        s("78%", "battery_full", "2h 3m remaining")
    );
    let mut mains = battery();
    mains["on_battery"] = json!(false);
    mains["state"] = json!(1);
    mains["charging"] = json!(true);
    mains["time_to_full_seconds"] = json!(1800);
    assert_eq!(
        header(mains),
        s("78%", "battery_charging_full", "30m until full")
    );
    // Only the always-present members: the percentage is shown, time and source are unknown.
    let bare = json!({"health": health(), "percentage": 55.4});
    assert_eq!(
        header(bare),
        s("55%", "battery_charging_full", "Connected to power")
    );
    // A desktop (no battery) or a backend that is down: the QML text, no estimate.
    let desktop = json!({"health": {"available": false, "stale": false}});
    assert_eq!(
        header(desktop),
        s("Connected to power", "battery_charging_full", "")
    );
    let mut stale = battery();
    stale["health"]["stale"] = json!(true);
    assert_eq!(
        header(stale),
        s("Connected to power", "battery_charging_full", "")
    );
}

fn first_snap(text: &str) -> Json {
    match framing::decode(text.lines().next().unwrap().as_bytes()).unwrap() {
        Message::Snap { data, .. } => data,
        _ => panic!("snapshot expected"),
    }
}

/// Nacre's own audio and power transcripts, byte for byte, drive the tiles and the pickers.
#[tokio::test(flavor = "current_thread")]
async fn nacre_transcripts_drive_the_audio_and_power_tiles() {
    let audio = first_snap(include_str!("../tests/protocol-v1/core-audio.jsonl"));
    let power = first_snap(include_str!(
        "../tests/protocol-v1/core-power-profiles.jsonl"
    ));
    let mut lab = lab_topics(
        &[
            "audio.set_volume",
            "audio.set_mute",
            "audio.set_default",
            "power.set_profile",
        ],
        &[("audio", audio), ("power.profiles", power)],
    )
    .await;
    let control = control_with(&lab, 3).await;
    let out = settle(&control, &Stores::default()).await;
    // No readback members: ready nodes show their linear volume as the slider's cube root.
    assert_eq!(source(&out, "audio.output")["value"], 79.);
    assert_eq!(source(&out, "audio.input")["on"], true);
    assert_eq!(source(&out, "audio.input")["icon"], "mic_off");
    // Streams are not devices.
    let devices = control.audio_devices();
    assert_eq!(
        devices
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["output-1", "input-1"]
    );
    let power = source(&out, "power.profile");
    assert_eq!(
        (power["status"].clone(), power["note"].clone()),
        (json!("Balanced"), json!("thermal"))
    );
    control.act("choose:power.profile:power-saver");
    assert_eq!(
        call(&mut lab).await,
        (
            "power.set_profile".into(),
            json!({"profile": "power-saver"})
        )
    );
}

// ---- B0087: the VPN tile and page ------------------------------------------------------

fn row<'a>(out: &'a BTreeMap<String, Value>, id: &str) -> &'a Json {
    let Value::List(items) = &out["cc.vpn"] else {
        panic!("cc.vpn");
    };
    items
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("no vpn row {id}"))
}
fn vpn_with(state: Json, profiles: Json) -> Json {
    let mut data = vpn();
    data["connections"]["items"]["home"]["state"] = json!(state);
    data["connections"]["items"]["home"]["active"] = json!(matches!(
        state.as_str(),
        Some("connecting" | "connected" | "disconnecting")
    ));
    if !profiles.is_null() {
        data["connections"] = profiles;
    }
    data
}
fn offline_vpn(data: Json) -> ControlCenter {
    let topics = TOPICS
        .iter()
        .map(|name| {
            let data = if *name == "vpn" {
                data.clone()
            } else {
                topic_data(name)
            };
            (
                *name,
                Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>,
            )
        })
        .collect();
    ControlCenter::new(None, topics, None, views())
}

#[test]
fn the_vpn_tile_names_the_first_active_profile_and_the_page_lists_every_profile() {
    let out = offline().project(&Stores::default());
    let tile = source(&out, "vpn.state");
    assert_eq!(tile["on"], true);
    assert_eq!(tile["status"], "Home WG");
    assert_eq!(tile["icon"], "vpn_key");
    assert_eq!(
        tile["toggle"], false,
        "a navigation tile: the press opens the page"
    );
    // The daemon's order; "Connected" / "Off" as QML; nothing is switchable without a daemon link.
    let (home, work) = (row(&out, "home"), row(&out, "work"));
    assert_eq!(
        (
            home["label"].as_str(),
            home["detail"].as_str(),
            home["on"].as_bool()
        ),
        (Some("Home WG"), Some("Connected"), Some(true))
    );
    assert_eq!(
        (
            work["label"].as_str(),
            work["detail"].as_str(),
            work["on"].as_bool()
        ),
        (Some("Work VPN"), Some("Off"), Some(false))
    );
    assert_eq!(home["enabled"], false);
    assert_eq!(out["cc.vpn_message"], Value::Text(String::new()));
    // Nothing active: "Off", not on.
    let idle = offline_vpn(vpn_with(json!("disconnected"), Json::Null)).project(&Stores::default());
    assert_eq!(
        (
            source(&idle, "vpn.state")["on"].clone(),
            source(&idle, "vpn.state")["status"].clone()
        ),
        (json!(false), json!("Off"))
    );
    // A profile that is still connecting is active (QML: it has a device) and says so.
    let busy = offline_vpn(vpn_with(json!("connecting"), Json::Null)).project(&Stores::default());
    assert_eq!(source(&busy, "vpn.state")["on"], true);
    assert_eq!(row(&busy, "home")["detail"], "Connecting…");
    let leaving =
        offline_vpn(vpn_with(json!("disconnecting"), Json::Null)).project(&Stores::default());
    assert_eq!(row(&leaving, "home")["detail"], "Disconnecting…");
}

#[test]
fn the_vpn_page_says_what_is_wrong_and_the_tile_hides_when_the_topic_is_unavailable() {
    // No saved profiles: the tile stays (QML) and the page says so.
    let none = offline_vpn(vpn_with(
        json!("disconnected"),
        json!({"order": [], "items": {}}),
    ))
    .project(&Stores::default());
    assert_eq!(source(&none, "vpn.state")["status"], "Off");
    assert_eq!(
        none["cc.vpn_message"],
        Value::Text("No saved VPN profiles found.".into())
    );
    // NetworkManager gone (stale or unavailable): no source, no rows, the QML wording.
    for (available, stale) in [(false, false), (true, true)] {
        let mut data = vpn();
        data["health"] = json!({"available": available, "stale": stale});
        let out = offline_vpn(data).project(&Stores::default());
        let Value::List(all) = &out["cc.src"] else {
            panic!()
        };
        assert!(
            all.iter().all(|s| s["id"] != "vpn.state"),
            "{available} {stale}"
        );
        assert_eq!(out["cc.vpn"], Value::list(Vec::new()));
        assert_eq!(
            out["cc.vpn_message"],
            Value::Text("NetworkManager VPN controls are unavailable.".into())
        );
    }
    // A profile whose last activation failed shows the QML failure text.
    let failed = offline_vpn(vpn_with(json!("failed"), Json::Null)).project(&Stores::default());
    assert_eq!(row(&failed, "home")["detail"], "Off");
    assert_eq!(
        failed["cc.vpn_message"],
        Value::Text(
            "Could not change the VPN connection. Check its saved credentials and try again."
                .into()
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_rows_connect_or_disconnect_by_published_id_and_a_refusal_is_shown_until_the_next_try()
{
    let mut lab = lab(&["vpn.connect", "vpn.disconnect"]).await;
    let control = control_for(&lab).await;
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(row(&out, "home")["enabled"], true);
    assert_eq!(row(&out, "work")["enabled"], true);
    control.act("switch:vpn.state:work");
    assert_eq!(
        call(&mut lab).await,
        ("vpn.connect".into(), json!({"connection": "work"}))
    );
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(out["cc.vpn_message"], Value::Text(String::new()));
    control.act("switch:vpn.state:home");
    assert_eq!(
        call(&mut lab).await,
        ("vpn.disconnect".into(), json!({"connection": "home"}))
    );
    settle(&control, &Stores::default()).await;
    // Only IDs the daemon published, and only for the vpn source.
    control.act("switch:vpn.state:../x");
    control.act("switch:network.wifi:home");
    settle(&control, &Stores::default()).await;
    assert!(lab.calls.try_recv().is_err());
    // A refused change shows the QML failure text, not the generic one, and the next attempt clears it.
    lab.fail
        .lock()
        .unwrap()
        .insert("vpn.connect".into(), ErrorCode::Unavailable);
    control.act("switch:vpn.state:work");
    call(&mut lab).await;
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(
        out["cc.vpn_message"],
        Value::Text(
            "Could not change the VPN connection. Check its saved credentials and try again."
                .into()
        )
    );
    assert_eq!(
        out["cc.message"],
        Value::Text(String::new()),
        "the dock's own line is not used"
    );
    lab.fail.lock().unwrap().clear();
    control.act("switch:vpn.state:work");
    call(&mut lab).await;
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(out["cc.vpn_message"], Value::Text(String::new()));
}

#[tokio::test(flavor = "current_thread")]
async fn without_the_vpn_methods_the_rows_are_inert() {
    let lab = lab(&["network.set_wifi"]).await;
    let control = control_for(&lab).await;
    let out = settle(&control, &Stores::default()).await;
    assert_eq!(row(&out, "home")["enabled"], false);
    assert_eq!(row(&out, "work")["enabled"], false);
    drop(lab);
}

fn vpn_view(page: &str, detail: &str, scale: f32) -> bead_test::Result<Harness> {
    let mut out = base();
    let Value::List(rows) = offline().project(&Stores::default())["cc.vpn"].clone() else {
        panic!()
    };
    // The offline adapter has no daemon link; the view wants armed rows.
    let armed = rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r["enabled"] = json!(true);
            r
        })
        .collect();
    out.insert("cc.vpn".into(), Value::list(armed));
    out.insert("cc.vpn_message".into(), Value::Text(String::new()));
    out.insert("cc.detail".into(), Value::Text(detail.into()));
    out.insert(
        "cc.saved".into(),
        ids(vec![
            "volume",
            "network",
            "bluetooth",
            "vpn",
            "battery",
            "dnd",
        ]),
    );
    harness(out, page, scale)
}

#[test]
fn the_vpn_tile_opens_its_page_and_each_row_switches_its_profile() -> bead_test::Result<()> {
    let h = vpn_view("home", "", 1.)?;
    let action = |h: &Harness, part: &str| {
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|n| n.id.contains(part))
            .unwrap_or_else(|| panic!("no node {part} in {:?}", node_ids(h)));
        (node.action.clone(), node.action_arg.clone())
    };
    for part in ["tile-vpn.main", "tile-vpn.detail-open"] {
        assert_eq!(
            action(&h, part),
            (Some("detail".into()), Some(Value::Text("vpn".into())))
        );
    }
    let h = vpn_view("detail", "vpn", 1.)?;
    assert_eq!(
        action(&h, "vpn-work.main"),
        (
            Some("cc.do".into()),
            Some(Value::Text("switch:vpn.state:work".into()))
        )
    );
    assert_eq!(
        action(&h, "vpn-home.main"),
        (
            Some("cc.do".into()),
            Some(Value::Text("switch:vpn.state:home".into()))
        )
    );
    Ok(())
}

#[test]
fn the_vpn_tile_and_page_match_goldens() -> bead_test::Result<()> {
    for scale in [1., 1.5] {
        for (name, page, detail) in [
            ("vpn-overview", "home", ""),
            ("vpn-detail", "detail", "vpn"),
        ] {
            let mut h = vpn_view(page, detail, scale)?;
            golden(name, &mut h, scale)?;
        }
    }
    Ok(())
}

/// Nacre's own `vpn` transcript, byte for byte, drives the tile and the page rows and the two calls.
#[tokio::test(flavor = "current_thread")]
async fn nacre_vpn_transcript_drives_the_tile_the_rows_and_the_calls() {
    let vpn = first_snap(include_str!("../tests/protocol-v1/core-vpn.jsonl"));
    let mut lab = lab_topics(&["vpn.connect", "vpn.disconnect"], &[("vpn", vpn)]).await;
    let control = control_for(&lab).await;
    let out = settle(&control, &Stores::default()).await;
    let tile = source(&out, "vpn.state");
    assert_eq!(
        (tile["on"].clone(), tile["status"].clone()),
        (json!(true), json!("Fixture office"))
    );
    assert_eq!(row(&out, "vpn-1")["detail"], "Connected");
    assert_eq!(row(&out, "vpn-2")["detail"], "Off");
    control.act("switch:vpn.state:vpn-2");
    assert_eq!(
        call(&mut lab).await,
        ("vpn.connect".into(), json!({"connection": "vpn-2"}))
    );
    settle(&control, &Stores::default()).await;
    control.act("switch:vpn.state:vpn-1");
    assert_eq!(
        call(&mut lab).await,
        ("vpn.disconnect".into(), json!({"connection": "vpn-1"}))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn returning_to_origin_survives_a_reply_before_the_topic_echo() {
    for after_reply in [false, true] {
        let mut lab = lab(&["audio.set_volume"]).await;
        let control = control_for(&lab).await;
        // The daemon acknowledges calls but keeps its original 50% snapshot.
        control.act("set:audio.output:100");
        assert_eq!(
            call(&mut lab).await,
            (
                "audio.set_volume".into(),
                json!({"target":"sink","value":1.0})
            )
        );
        if after_reply {
            settle(&control, &Stores::default()).await;
        }
        control.act("set:audio.output:50");
        settle(&control, &Stores::default()).await;
        assert_eq!(
            call(&mut lab).await,
            (
                "audio.set_volume".into(),
                json!({"target":"sink","value":0.125})
            )
        );
        settle(&control, &Stores::default()).await;
        assert!(
            lab.calls.try_recv().is_err(),
            "a settled final target must not loop"
        );
    }
}

/// B0127: do not confuse the dock's two-row preview with the full retained list.
#[test]
fn notification_dropdown_projects_all_retained_rows_in_service_order() -> bead_test::Result<()> {
    let source = Arc::new(FakeSource::new(json!({
        "health": health(), "history_count": 4, "history_more": false,
        "active": {"order": ["active", "transient", "feedback"], "items": {
            "active": {"retained": true, "summary": "Active"},
            "transient": {"retained": false}, "feedback": {"retained": false}}},
        "history": {"order": ["new", "middle", "old"], "items": {
            "new": {"summary": "Newest"}, "middle": {"summary": "Middle"},
            "old": {"summary": "Oldest"}}},
        "history_storage": {"ready": true, "writable": true}
    }))?);
    let control = ControlCenter::new(None, vec![("notifications", source.clone())], None, vec![]);
    let out = control.project(&Stores::default());
    let Value::List(rows) = &out["cc.notices"] else {
        panic!("rows")
    };
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["active", "new", "middle", "old"]
    );
    assert_eq!(out["cc.notice_count"], Value::Number(rows.len() as f64));
    assert_eq!(out["cc.notice_note"], Value::Text(String::new()));
    let mut truncated = (*source.snapshot().data).clone();
    truncated["history_more"] = json!(true);
    truncated["history_count"] = json!(18);
    source.publish(truncated)?;
    let out = control.project(&Stores::default());
    assert_eq!(out["cc.notice_count"], Value::Number(18.));
    assert_eq!(
        out["cc.notice_note"],
        Value::Text("Newest 4 of 18 stored are shown".into())
    );
    Ok(())
}

#[test]
fn notification_dropdown_scrolls_every_published_row() -> bead_test::Result<()> {
    for scale in [1., 1.5] {
        let mut out = base();
        out.insert("view.edge".into(), Value::Text("top".into()));
        out.insert("cc.max_height".into(), Value::Number(320.));
        out.insert("cc.notice_ready".into(), Value::Bool(true));
        out.insert("cc.notices".into(), Value::list((0..18).map(|i| json!({
            "id": format!("n{i}"), "app": "Fixture", "summary": format!("Notification {}", i + 1), "body": "Retained"
        })).collect()));
        let mut h = harness(out, "home", scale)?;
        assert_eq!(h.session.scroll_regions().len(), 1);
        golden("notifications-full", &mut h, scale)?;
        let toolbar = centre(&h, "notices-dnd");
        h.script([Input::Scroll {
            x: 100.,
            y: 180.,
            delta: 2000.,
        }])?;
        assert!(matches!(h.session.stores.get("cc.scroll"), Some(Value::Number(n)) if *n > 0.));
        assert_eq!(
            centre(&h, "notices-dnd"),
            toolbar,
            "toolbar stays visible while history scrolls"
        );
        golden("notifications-scrolled", &mut h, scale)?;
    }
    Ok(())
}

#[test]
fn shared_slider_origins_match_at_fractional_scale() -> bead_test::Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    for scale in [1., 1.5, 2.] {
        let h = harness(base(), "home", scale)?;
        let mut runtime = crate::reload::Runtime::new(h.session).with_control_center(offline())?;
        runtime.stage(UiEvent::Relayout)?;
        runtime.commit();
        let visuals = runtime.drag_visuals();
        assert!(!visuals.is_empty());
        for visual in visuals {
            let fill = visual.fill.expect("kit exports its shared fill");
            assert!(
                (fill.rect.x + fill.minimum - visual.travel.0 - 4.).abs() < 0.001,
                "scale={scale}"
            );
        }
    }
    Ok(())
}

#[test]
fn local_slider_drag_defers_projection_and_releases_once() -> bead_test::Result<()> {
    use bead::wayland::{SceneSource, UiEvent};

    let h = harness(base(), "home", 1.)?;
    let mut runtime = crate::reload::Runtime::new(h.session).with_control_center(offline())?;
    let projections = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let observed = projections.clone();
    // Keep fixture controls armed, and observe projection independently of transport.
    runtime.session.set_projector(Box::new(move |_| {
        observed.set(observed.get() + 1);
        Ok(BTreeMap::new())
    }));
    let scene = runtime
        .stage(UiEvent::Relayout)?
        .expect("control-center scene");
    runtime.commit();

    let visual = runtime
        .drag_visuals()
        .into_iter()
        .find(|visual| visual.id.contains("tile-volume") && visual.id.ends_with(".thumb"))
        .expect("volume slider exports a drag visual");
    assert!(
        visual.local_feedback,
        "volume accepts local slider feedback"
    );
    let layer = scene
        .layers
        .iter()
        .find(|layer| layer.id == visual.id)
        .expect("volume thumb layer");
    let bead::render::Primitive::Rounded { rect, .. } = &scene.primitives[layer.own.start] else {
        panic!("volume thumb is rounded");
    };
    let press_x = rect.x + rect.w / 2.;
    let press_y = visual.grab.y + visual.grab.h / 2.;
    runtime.stage(UiEvent::Click {
        x: press_x,
        y: press_y,
    })?;
    runtime.commit();

    let pressed_visual = runtime
        .drag_visuals()
        .into_iter()
        .find(|candidate| candidate.id == visual.id)
        .expect("pressed slider visual");
    let release_serial = pressed_visual.release_serial;
    let node_snapshot: Vec<_> = runtime
        .session
        .view_nodes()
        .iter()
        .map(|node| (node.id.clone(), bead::ui::rect(&node.draw)))
        .collect();
    let fill_bounds = pressed_visual
        .fill
        .as_ref()
        .map(|fill| runtime.session.paint_bounds(&fill.id).expect("fill bounds"));

    let before_projection = projections.get();
    for fraction in [0.23f32, 0.57, 0.81] {
        let x = pressed_visual.pointer.0
            + fraction * (pressed_visual.pointer.1 - pressed_visual.pointer.0);
        let y = pressed_visual.grab.y + pressed_visual.grab.h / 2.;
        assert!(
            runtime.stage(UiEvent::LocalDrag { x, y })?.is_none(),
            "local motion must not produce a full scene"
        );
        let Some(bead::core::Value::Number(value)) =
            runtime.session.stores.get("bead.preview.value")
        else {
            panic!("local continuous preview value is stored");
        };
        assert!(
            ((*value as f32) - fraction * 100.).abs() < 0.2,
            "fraction={fraction}, value={value}"
        );
        let current_nodes: Vec<_> = runtime
            .session
            .view_nodes()
            .iter()
            .map(|node| (node.id.clone(), bead::ui::rect(&node.draw)))
            .collect();
        assert_eq!(
            current_nodes, node_snapshot,
            "local drag must retain the accepted node tree"
        );
        if let (Some(fill), Some(before)) = (&pressed_visual.fill, fill_bounds) {
            assert_eq!(runtime.session.paint_bounds(&fill.id), Some(before));
        }
    }

    assert!(
        runtime.stage(UiEvent::Model(BTreeMap::new()))?.is_none(),
        "service wakeups do not rebuild the held slider view"
    );
    assert_eq!(
        projections.get(),
        before_projection,
        "held values and service wakeups skip projection"
    );

    let released_scene = runtime
        .stage(UiEvent::Release)?
        .expect("release ends the pressed range and projects the latest source data");
    runtime.commit();
    assert!(
        projections.get() > before_projection,
        "release projects the source again"
    );
    let released_visual = runtime
        .drag_visuals()
        .into_iter()
        .find(|candidate| candidate.id == visual.id)
        .expect("slider descriptor survives the release frame");
    assert_eq!(
        released_visual.release_serial,
        release_serial.wrapping_add(1),
        "one physical release advances its descriptor serial once"
    );
    assert!(
        runtime.stage(UiEvent::Release)?.is_none(),
        "duplicate release is harmless"
    );
    assert_eq!(
        runtime
            .drag_visuals()
            .into_iter()
            .find(|candidate| candidate.id == visual.id)
            .expect("slider descriptor after duplicate release")
            .release_serial,
        release_serial.wrapping_add(1),
        "duplicate release must not increment the serial"
    );

    let next = runtime
        .drag_visuals()
        .into_iter()
        .find(|v| v.id == visual.id)
        .unwrap();
    let x = next.pointer.0 + 0.45 * (next.pointer.1 - next.pointer.0);
    let y = next.grab.y + next.grab.h / 2.;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert!(runtime.stage(UiEvent::LocalDrag { x, y })?.is_none());
    assert!(
        runtime
            .stage(UiEvent::Model(BTreeMap::from([(
                "panel.open".into(),
                Value::Bool(false)
            )])))?
            .is_some(),
        "an explicit close stays responsive while the gesture is held"
    );
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(false))
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    assert!(
        runtime
            .stage(UiEvent::Model(BTreeMap::from([(
                "panel.open".into(),
                Value::Bool(true)
            )])))?
            .is_some()
    );
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(true))
    );
    let _ = released_scene;

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn local_slider_ticks_send_latest_level_while_held_and_after_release() {
    async fn next_call(control: &ControlCenter, lab: &mut Lab) -> (String, Json) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(call) = lab.calls.try_recv() {
                    break call;
                }
                control.tick_calls();
                tokio::select! {
                    call=lab.calls.recv()=>break call.expect("fixture call channel closed"),
                    _=tokio::time::sleep(Duration::from_millis(5))=>{},
                }
            }
        })
        .await
        .expect("control tick did not dispatch the queued level")
    }

    let mut lab = lab(&["audio.set_volume"]).await;
    let control = control_for(&lab).await;

    // First local drag starts a request. No await occurs until the burst below has
    // queued its latest targets behind that in-flight call.
    control.act("set:audio.output:20");
    control.tick_calls();

    // Several continuous updates arrive while the first call is in flight. Each
    // event calls tick_calls, but the controller coalesces their target to the latest.
    for percent in [35, 58, 75] {
        control.tick_calls();
        control.act(&format!("set:audio.output:{percent}"));
    }
    let (method, params) = call(&mut lab).await;
    assert_eq!(method, "audio.set_volume");
    assert!((params["value"].as_f64().unwrap() - 0.008).abs() < 1e-9);
    let (method, params) = next_call(&control, &mut lab).await;
    assert_eq!(method, "audio.set_volume");
    assert!(
        (params["value"].as_f64().unwrap() - 0.421875).abs() < 1e-9,
        "only the newest in-flight target (75%) should follow the first call"
    );

    // Final motion is queued during the second write. The runtime's release flush
    // ticks call tickets again, so the final daemon write carries the release target.
    control.act("set:audio.output:82");
    control.act("set:audio.output:95");
    control.tick_calls(); // release flush polls the previous ticket and sends the final target
    let (method, params) = next_call(&control, &mut lab).await;
    assert_eq!(method, "audio.set_volume");
    assert!(
        (params["value"].as_f64().unwrap() - 0.857375).abs() < 1e-9,
        "the final 95% target must be sent after the earlier request completes: {params:?}"
    );

    // Drain the final ticket without a view projection and ensure it does not loop.
    for _ in 0..10 {
        control.tick_calls();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        lab.calls.try_recv().is_err(),
        "settled level has no duplicate write"
    );
}

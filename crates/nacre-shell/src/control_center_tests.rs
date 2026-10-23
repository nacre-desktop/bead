// SPDX-License-Identifier: GPL-3.0-only
//! B0049: control-center sources against fixtures, tile-store persistence against a fake
//! daemon, the Luau view's drag reorder, and goldens for the four pages.
use super::*;
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
fn audio() -> Json {
    json!({"health": health(), "default_output": "out", "default_input": "in", "nodes": {"order": ["out", "in"], "items": {
        "out": {"name": "Speakers", "kind": "output", "volume": 0.125, "muted": false, "ready": true,
            "readback": {"perceptual_volume": 0.5, "linear_volume": 0.125, "muted": false}},
        "in": {"name": "Mic", "kind": "input", "volume": 0.512, "muted": true, "ready": true}}}})
}
fn network() -> Json {
    json!({"health": health(), "wireless_enabled": true,
        "devices": {"items": {}, "order": []}, "connections": {"items": {}, "order": []},
        "access_points": {"order": ["ap"], "items": {"ap": {"device": "d", "name": "Home", "strength": 70, "secured": true, "active": true}}}})
}
fn bluetooth() -> Json {
    json!({"health": health(),
        "adapters": {"order": ["hci0"], "items": {"hci0": {"name": "hci0", "powered": true}}},
        "devices": {"order": ["d1"], "items": {"d1": {"name": "Buds", "connected": true}}}})
}
fn power() -> Json {
    json!({"health": health(), "active": "balanced",
        "profiles": {"order": ["power-saver", "balanced", "performance"], "items": {
            "power-saver": {"name": "power-saver", "available": true},
            "balanced": {"name": "balanced", "available": true},
            "performance": {"name": "performance", "available": false}}}})
}
fn battery() -> Json {
    json!({"health": health(), "percentage": 78.0, "charging": false})
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
    assert!(all.iter().all(|s| s["id"] != "vpn.state"));
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
    let fail: Arc<Mutex<BTreeMap<String, ErrorCode>>> = Default::default();
    let failing = fail.clone();
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
                            } else if topic == "controls" {
                                published.clone()
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
    let client = Client::start(Config::new(socket, vec![])).unwrap();
    Lab {
        dir,
        client,
        calls,
        fail,
    }
}
async fn control_for(lab: &Lab) -> ControlCenter {
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
        if out["cc.saved_known"] == Value::Bool(true) && source_count(&out) >= 13 {
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
    "bluetooth.set_enabled",
    "notifications.set_dnd",
    SET,
];

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
        ("bluetooth.set_enabled".into(), json!({"enabled": false}))
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
    // A target equal to the shown level sends nothing; a far one is one absolute call.
    control.act("set:audio.output:50");
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
    // The same keyboard step, an unknown profile and an unknown source send nothing.
    settle(&control, &Stores::default()).await;
    control.act("set:keyboard.backlight:33");
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
                // Tiles whose source is published are live; the others are dimmed.
                "home" => {
                    assert!(ids.iter().any(|i| i.contains("tile-volume")), "{ids:?}");
                    assert!(ids.iter().any(|i| i.contains("tile-clipboard")));
                    // The QML dims an unavailable tile; so does the panel (a veil over the tile).
                    assert!(ids.iter().any(|i| i.contains("dim-brightness")), "{ids:?}");
                    assert!(ids.iter().any(|i| i.contains("dim-night")), "{ids:?}");
                }
                "edit" => assert!(ids.iter().any(|i| i.contains(".slot-1"))),
                "catalog" => assert!(ids.iter().any(|i| i.contains("add-notifications"))),
                _ => assert!(ids.iter().any(|i| i.contains("switch"))),
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
    assert_eq!(
        action(&h, "tile-network.main"),
        (
            Some("cc.do".into()),
            Some(Value::Text("toggle:network.wifi".into()))
        )
    );
    assert_eq!(
        action(&h, "tile-network.detail-open"),
        (
            Some("view.open".into()),
            Some(Value::Text("network".into()))
        )
    );
    assert_eq!(
        action(&h, "tile-clipboard.main"),
        (
            Some("view.open".into()),
            Some(Value::Text("dashboard-clipboard".into()))
        )
    );
    // The level slider sends percent steps for its own source.
    assert_eq!(
        action(&h, "tile-volume.level-column.level.hits.value-10"),
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
        action(&h, "choice-power-saver"),
        (
            Some("cc.do".into()),
            Some(Value::Text("choose:power.profile:power-saver".into()))
        )
    );
    assert_eq!(
        action(&h, "details"),
        (
            Some("view.open".into()),
            Some(Value::Text("battery".into()))
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

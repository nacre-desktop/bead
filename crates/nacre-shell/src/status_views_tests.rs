// SPDX-License-Identifier: GPL-3.0-only
//! Existing hosted types, deterministic fonts, no hardware or compositor.
use crate::status_views::{Kind, Status};
use bead::core::{Value, source::Source};
use bead_test::{Config, Harness, Result, source::FakeSource};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn data() -> serde_json::Value {
    let snapshot = bead::services::Snapshot {
        network: bead::services::Network {
            available: true,
            connected: true,
            wireless_enabled: true,
            name: "Fixture network".into(),
            strength: Some(86),
        },
        bluetooth: bead::services::Bluetooth {
            available: true,
            powered: true,
            connected_devices: vec!["Fixture headphones".into(), "Fixture keyboard".into()],
        },
        battery: bead::services::Battery {
            available: true,
            percentage: Some(83.),
            charging: true,
            ..Default::default()
        },
        ..Default::default()
    };
    serde_json::to_value(snapshot).unwrap()
}
/// The join/forget stores the adapter projects (B0050), at their idle values.
fn with_flow_stores(mut stores: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    stores.extend([
        ("network.view".into(), Value::Text("list".into())),
        ("network.prompt_label".into(), Value::Text(String::new())),
        ("network.form_error".into(), Value::Text(String::new())),
        ("network.form_busy".into(), Value::Bool(false)),
        ("network.can_hidden".into(), Value::Bool(true)),
        ("network.can_forget".into(), Value::Bool(true)),
        ("network.saved".into(), Value::list(Vec::new())),
    ]);
    stores
}
fn core_network() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "wireless_enabled":true,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
        "connections":{"items":{"wired":{"name":"Ethernet","kind":"wired","active":true},"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wired","wifi-connection"]},
        "primary_connection":"wifi-connection"
    })
}
fn core_bluetooth() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "adapters":{"items":{
            "adapter-off":{"name":"Off adapter","powered":false,"discovering":false},
            "adapter-on-a":{"name":"Adapter A","powered":true,"discovering":false},
            "adapter-on-b":{"name":"Adapter B","powered":true,"discovering":false}
        },"order":["adapter-on-a","adapter-off","adapter-on-b"]},
        "devices":{"items":{
            "a-empty":{"adapter":"adapter-on-b","name":"","connected":true,"paired":true,"trusted":false,"pairing":false},
            "m-first":{"adapter":"adapter-on-a","name":"Headphones Studio","connected":true,"paired":true,"trusted":true,"pairing":false,"battery":82},
            "z-off":{"adapter":"adapter-off","name":"Off device","connected":true,"paired":true,"trusted":false,"pairing":false},
            "paired-only":{"adapter":"adapter-on-a","name":"Paired only","connected":false,"paired":true,"trusted":true,"pairing":false}
        },"order":["m-first","a-empty","z-off","paired-only"]}
    })
}
fn core_battery() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "percentage":83.0,
        "charging":true,
        "state":1,
        "time_to_full_seconds":5400,
        "on_battery":false
    })
}
fn core_audio() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "default_output":"out",
        "default_input":"mic",
        "nodes":{"items":{
            "out":{"name":"Studio speakers","kind":"output","volume":0.125,"muted":false,"ready":true,"readback":{"linear_volume":0.125,"perceptual_volume":0.5,"muted":false}},
            "mic":{"name":"Microphone","kind":"input","volume":0.0,"muted":false,"ready":true},
            "play":{"name":"Playback stream","kind":"playback","volume":1.0,"muted":false,"ready":true}
        },"order":["mic","play","out"]}
    })
}
fn core_updates() -> serde_json::Value {
    include_str!("../tests/protocol-v1/core-dashboard-updates.jsonl")
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|value| value.get("data").cloned())
        .expect("dashboard updates fixture snapshot")
}
fn text_value<'a>(values: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
    let Value::Text(value) = &values[key] else {
        panic!("{key} must be text")
    };
    value
}
#[test]
fn updates_popout_projects_the_existing_daemon_topic() -> Result<()> {
    let status = Status::core_updates(Arc::new(FakeSource::new(core_updates())?), None);
    let values = status.project();
    assert_eq!(
        text_value(&values, "status.updates_summary"),
        "3 updates available"
    );
    assert_eq!(text_value(&values, "status.updates_detail"), "");
    assert_eq!(
        text_value(&values, "status.peek.summary"),
        "3 updates available"
    );
    Ok(())
}

#[test]
fn updates_dropdown_handles_optional_packages_health_and_empty_state() -> Result<()> {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../shell/tests/fixtures/status/updates-packages.json"
    ))?;
    let source = Arc::new(FakeSource::new(data.clone())?);
    let status = Status::core_updates(source.clone(), None);
    let values = status.project();
    let Value::List(rows) = &values["status.updates_packages"] else {
        panic!("package rows")
    };
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["name"], "linux");
    assert_eq!(rows[0]["old"], "6.17.4-1");
    assert_eq!(rows[0]["new"], "6.17.5-1");
    assert_eq!(rows[2]["repo"], "AUR");
    assert!(text_value(&values, "status.updates_checked").starts_with("Last checked "));
    let mut empty = data.clone();
    empty.as_object_mut().unwrap().remove("packages");
    empty["count"] = serde_json::json!(0);
    empty["checked"] = serde_json::Value::Null;
    source.publish(empty.clone())?;
    let values = status.project();
    assert_eq!(values["status.updates_packages"], Value::list(vec![]));
    assert_eq!(text_value(&values, "status.updates_summary"), "Up to date");
    assert_eq!(
        text_value(&values, "status.updates_checked"),
        "Not checked yet"
    );
    assert_eq!(values["status.updates_can_open"], Value::Bool(false));
    empty["refreshing"] = serde_json::json!(true);
    source.publish(empty.clone())?;
    let values = status.project();
    assert_eq!(
        text_value(&values, "status.updates_health"),
        "Checking for updates…"
    );
    assert_eq!(values["status.updates_can_refresh"], Value::Bool(false));
    empty["health"] = serde_json::json!({"available":false,"stale":true,"error":{"code":"unavailable","message":"Offline","retry":true}});
    empty["refreshing"] = serde_json::json!(false);
    source.publish(empty)?;
    assert_eq!(
        text_value(&status.project(), "status.updates_health"),
        "Offline"
    );
    let mut invalid = data;
    invalid["packages"][0]["name"] = serde_json::json!("bad\nname");
    source.publish(invalid)?;
    let Value::List(rows) = &status.project()["status.updates_packages"] else {
        panic!("package rows")
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(source.calls(), 0, "projections never run package commands");
    Ok(())
}

#[test]
fn updates_dropdown_goldens_cover_count_packages_scroll_and_empty() -> Result<()> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../shell/tests/fixtures/status/updates-packages.json"
    ))?;
    for scale in [1., 1.5] {
        for case in ["count", "packages", "scroll", "empty", "unavailable"] {
            let mut data = fixture.clone();
            match case {
                "count" => {
                    data.as_object_mut().unwrap().remove("packages");
                }
                "scroll" => {
                    data["packages"] = serde_json::json!((0..40).map(|i| serde_json::json!({"name":format!("package-{i:02}"),"old":"1.0-1","new":"1.1-1","repo":"extra"})).collect::<Vec<_>>());
                }
                "empty" => {
                    data["count"] = serde_json::json!(0);
                    data["packages"] = serde_json::json!([]);
                }
                "unavailable" => {
                    data["count"] = serde_json::Value::Null;
                    data["packages"] = serde_json::json!([]);
                    data["health"] = serde_json::json!({"available":false,"stale":true});
                }
                _ => {}
            }
            let status = Status::core_updates(Arc::new(FakeSource::new(data)?), None);
            let mut values = status.project();
            // Fix the wall-clock label across time zones for deterministic pixels.
            values.insert(
                "status.updates_checked".into(),
                Value::Text("Last checked 2027-01-15 08:00".into()),
            );
            values.insert("view.width".into(), Value::Number(382.));
            if case == "scroll" {
                values.insert("status.updates_scroll".into(), Value::Number(1000.));
            }
            let mut h = Harness::load(
                &root(),
                Path::new("status/updates.luau"),
                &root().join("palette.json"),
                values,
                Config {
                    width: 446,
                    height: 500,
                    scale,
                },
            )?;
            assert!(h.scene().bounds.h < 450., "dropdown height is capped");
            if case == "count" || case == "empty" {
                assert!(h.scene().bounds.h < 200., "no blank detail space");
            }
            if case == "scroll" {
                assert_eq!(h.session.scroll_regions().len(), 1);
            }
            if case == "packages" {
                for repo in ["core", "extra", "AUR"] {
                    assert!(
                        h.scene()
                            .primitives
                            .iter()
                            .any(|primitive| matches!(primitive,
                                bead::render::Primitive::Text { text, rect, .. }
                                if text == repo && rect.w > 10. && rect.x + rect.w <= 382.
                            )),
                        "repository badge must fit inside the dropdown: {repo}"
                    );
                }
            }
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("updates-{case}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-updates-{case}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}
fn harness(scale: f32) -> Result<Harness> {
    Harness::load(
        &root(),
        Path::new("status/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 380,
            height: 300,
            scale,
        },
    )
}

#[test]
fn controlled_network_template_omits_unavailable_switch_action() -> Result<()> {
    let initial = BTreeMap::from([
        ("view.edge".into(), Value::Text("top".into())),
        ("panel.open".into(), Value::Bool(true)),
        ("status.ready".into(), Value::Bool(false)),
        ("status.active".into(), Value::Bool(false)),
        ("status.heading".into(), Value::Text("Network".into())),
        ("status.icon".into(), Value::Text("wifi".into())),
        (
            "status.primary".into(),
            Value::Text("Network unavailable".into()),
        ),
        (
            "status.secondary".into(),
            Value::Text("Status data unavailable".into()),
        ),
        ("status.detail".into(), Value::Text(String::new())),
        ("network.wifi_checked".into(), Value::Bool(false)),
        ("network.wifi_enabled".into(), Value::Bool(false)),
        ("network.wifi_busy".into(), Value::Bool(false)),
        (
            "network.wifi_message".into(),
            Value::Text("Wi-Fi control unavailable".into()),
        ),
        ("network.rows".into(), Value::list(Vec::new())),
        ("network.height".into(), Value::Number(262.)),
        ("network.link_visible".into(), Value::Bool(false)),
        ("network.link_message".into(), Value::Text(String::new())),
    ]);
    let initial = with_flow_stores(initial);
    let h = Harness::load(
        &root(),
        Path::new("status/network.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 380,
            height: 300,
            scale: 1.,
        },
    )?;
    assert!(
        h.session
            .view_nodes()
            .iter()
            .all(|node| node.id != "status-popout.wifi-row.wifi-radio"),
        "unavailable switch has no hit action"
    );
    assert!(
        h.scene()
            .primitives
            .iter()
            .any(|primitive| matches!(primitive,
                bead::render::Primitive::Text { text, .. } if text == "Wi-Fi"
            )),
        "the unavailable row remains visible"
    );
    assert!(h.scene().primitives.iter().any(|primitive| matches!(primitive,
        bead::render::Primitive::Rounded { rect, .. } if (rect.w - 38.).abs() < 0.1 && (rect.h - 22.).abs() < 0.1
    )), "the unavailable switch remains visible");
    Ok(())
}

#[test]
fn controlled_bluetooth_template_omits_unavailable_switch_action() -> Result<()> {
    let initial = BTreeMap::from([
        ("view.edge".into(), Value::Text("top".into())),
        ("panel.open".into(), Value::Bool(true)),
        ("status.ready".into(), Value::Bool(false)),
        ("status.active".into(), Value::Bool(false)),
        ("status.heading".into(), Value::Text("Bluetooth".into())),
        ("status.icon".into(), Value::Text("bluetooth".into())),
        (
            "status.primary".into(),
            Value::Text("Bluetooth unavailable".into()),
        ),
        (
            "status.secondary".into(),
            Value::Text("Status data unavailable".into()),
        ),
        ("status.detail".into(), Value::Text(String::new())),
        ("bluetooth.power_visible".into(), Value::Bool(true)),
        ("bluetooth.power_checked".into(), Value::Bool(false)),
        ("bluetooth.power_enabled".into(), Value::Bool(false)),
        ("bluetooth.power_busy".into(), Value::Bool(false)),
        (
            "bluetooth.power_message".into(),
            Value::Text("Bluetooth power control unavailable".into()),
        ),
        ("bluetooth.view".into(), Value::Text("list".into())),
        ("bluetooth.devices".into(), Value::list(Vec::new())),
        ("bluetooth.list_message".into(), Value::Text(String::new())),
        (
            "bluetooth.scan_label".into(),
            Value::Text("Find devices".into()),
        ),
        ("bluetooth.scan_enabled".into(), Value::Bool(false)),
        ("bluetooth.scanning".into(), Value::Bool(false)),
        ("bluetooth.empty_text".into(), Value::Text(String::new())),
    ]);
    let h = Harness::load(
        &root(),
        Path::new("status/bluetooth.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 380,
            height: 300,
            scale: 1.,
        },
    )?;
    assert!(
        h.session
            .view_nodes()
            .iter()
            .all(|node| node.id != "status-popout.bluetooth-row.bluetooth-power"),
        "unavailable switch has no hit action"
    );
    assert!(h.scene().primitives.iter().any(|primitive| matches!(primitive, bead::render::Primitive::Text { text, .. } if text == "Bluetooth")), "the unavailable row remains visible");
    assert!(h.scene().primitives.iter().any(|primitive| matches!(primitive, bead::render::Primitive::Rounded { rect, .. } if (rect.w - 38.).abs() < 0.1 && (rect.h - 22.).abs() < 0.1)), "the unavailable switch remains visible");
    Ok(())
}
#[test]
fn read_only_status_popouts_have_populated_goldens_at_both_scales() -> Result<()> {
    for kind in Kind::ALL {
        for scale in [1., 1.5] {
            let source = Arc::new(FakeSource::new(data())?);
            let view = Status::new(kind, source.clone());
            let mut h = harness(scale)?;
            h.session
                .set_projector(Box::new(move |_| Ok(view.project())));
            h.update(BTreeMap::new())?;
            assert_eq!(
                h.session.stores.get("status.ready"),
                Some(&Value::Bool(true))
            );
            assert!(
                h.session.view_nodes().is_empty(),
                "status-only views cannot execute service controls"
            );
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("status-{}-{scale}.png", kind.name()));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-status-{}-{scale}-diff", kind.name())),
            )?;
            assert_eq!(source.calls(), 0);
        }
    }
    Ok(())
}

#[test]
fn clicked_status_factories_use_top_popout_geometry_and_peeks_stay_small() -> Result<()> {
    use std::sync::{Arc, OnceLock};
    let palette_path = root().join("palette.json");
    let colors = Arc::new(bead::render::Palette::read(&palette_path)?);
    let palette = Arc::new(palette_path);
    let owner = Arc::new(OnceLock::new());
    let status = crate::popouts::status(
        Kind::Battery,
        palette.clone(),
        colors.clone(),
        owner.clone(),
        None,
        16.,
        false,
    )?;
    assert_eq!(status.fallback.width, 230.);
    assert!(matches!(
        status.fallback.motion.as_ref().map(|motion| motion.edge),
        Some(bead::core::Edge::Top)
    ));
    assert_eq!(
        status
            .fallback
            .motion
            .as_ref()
            .map(|motion| motion.duration),
        Some(0.23)
    );
    let position = status
        .position
        .as_ref()
        .ok_or("status popout positioner missing")?;
    let (x, y) = position(&status.fallback, None, (1920, 1200));
    assert_eq!(x + 32 + 115, 960, "status popout is centered over the bar");
    assert_eq!(y + 32, 50, "status popout begins at the usable top edge");

    let peek =
        crate::popouts::status_peek(Kind::Battery, palette, colors, owner, None, 16., false)?;
    assert_eq!(peek.fallback.width, 310.);
    assert!(matches!(
        peek.fallback.motion.as_ref().map(|motion| motion.edge),
        Some(bead::core::Edge::Top)
    ));
    assert_eq!(
        peek.fallback.motion.as_ref().map(|motion| motion.duration),
        Some(0.23)
    );
    assert!(!peek.passive, "pointer may cross into the quick-info panel");
    assert!(
        !peek.keyboard_focus,
        "hover previews do not claim keyboard focus"
    );
    Ok(())
}

#[test]
fn status_peek_projects_qml_summary_detail_and_connected_devices() -> Result<()> {
    crate::i18n::init();
    let network = Status::new(Kind::Network, Arc::new(FakeSource::new(data())?)).project();
    assert_eq!(text_value(&network, "status.peek.detail"), "Signal 86%");

    let mut wifi_off = data();
    wifi_off["network"]["connected"] = serde_json::json!(false);
    wifi_off["network"]["wireless_enabled"] = serde_json::json!(false);
    let off = Status::new(Kind::Network, Arc::new(FakeSource::new(wifi_off.clone())?)).project();
    assert_eq!(text_value(&off, "status.peek.title"), "Wi-Fi");
    assert_eq!(text_value(&off, "status.peek.summary"), "Off");
    assert_eq!(text_value(&off, "status.peek.detail"), "");
    wifi_off["network"]["available"] = serde_json::json!(false);
    let unavailable = Status::new(Kind::Network, Arc::new(FakeSource::new(wifi_off)?)).project();
    assert_eq!(
        unavailable["status.peek.summary"],
        unavailable["status.primary"]
    );
    assert_eq!(text_value(&unavailable, "status.peek.detail"), "");

    let source = Arc::new(FakeSource::new(data())?);
    let values = Status::new(Kind::Bluetooth, source).project();
    assert_eq!(text_value(&values, "status.peek.title"), "Bluetooth");
    assert_eq!(
        text_value(&values, "status.peek.summary"),
        "2 connected devices"
    );
    assert_eq!(text_value(&values, "status.peek.detail"), "");
    let Value::List(devices) = &values["status.peek.devices"] else {
        panic!("peek devices must be a list")
    };
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0]["name"], "Fixture headphones");
    assert_eq!(devices[1]["name"], "Fixture keyboard");

    let mut many = data();
    many["bluetooth"]["connected_devices"] = serde_json::json!([
        "Device 1", "Device 2", "Device 3", "Device 4", "Device 5", "Device 6"
    ]);
    let values = Status::new(Kind::Bluetooth, Arc::new(FakeSource::new(many)?)).project();
    let Value::List(devices) = &values["status.peek.devices"] else {
        panic!("peek devices must be a list")
    };
    assert_eq!(
        devices.len(),
        6,
        "the projection retains all connected devices"
    );
    assert_eq!(devices[5]["name"], "Device 6");

    let mut powered_off = data();
    powered_off["bluetooth"]["powered"] = serde_json::json!(false);
    let values = Status::new(Kind::Bluetooth, Arc::new(FakeSource::new(powered_off)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Bluetooth off");
    assert_eq!(text_value(&values, "status.peek.summary"), "Off");
    let Value::List(devices) = &values["status.peek.devices"] else {
        panic!("peek devices must be a list")
    };
    assert!(
        devices.is_empty(),
        "powered-off peeks hide stale device rows"
    );
    Ok(())
}

#[test]
fn battery_peek_rebuilds_when_profile_source_arrives_after_initial_markup() -> Result<()> {
    crate::i18n::init();
    fn scene_texts(h: &Harness) -> Vec<String> {
        h.scene()
            .primitives
            .iter()
            .filter_map(|primitive| match primitive {
                bead::render::Primitive::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
    let mut battery = core_battery();
    battery["percentage"] = serde_json::json!(63.0);
    battery["time_to_full_seconds"] = serde_json::json!(7800);
    let status = Status::core_battery(Arc::new(FakeSource::new(battery)?));
    let mut initial = status.project();
    initial.insert("status.peek.detail".into(), Value::Text(String::new()));
    initial.insert("cc.src".into(), Value::list(Vec::new()));
    let mut harness = Harness::load(
        &root(),
        Path::new("status/peek.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 310,
            height: 120,
            scale: 1.,
        },
    )?;
    assert!(
        !scene_texts(&harness)
            .iter()
            .any(|text| text.contains("until full"))
    );

    harness.update(BTreeMap::from([(
        "status.peek.detail".into(),
        Value::Text("2h 10m until full".into()),
    )]))?;
    assert!(
        scene_texts(&harness)
            .iter()
            .any(|text| text == "2h 10m until full")
    );

    harness.update(BTreeMap::from([(
        "cc.src".into(),
        Value::list(vec![serde_json::json!({
            "id": "power.profile",
            "available": true,
            "status": "Balanced"
        })]),
    )]))?;
    assert!(
        scene_texts(&harness)
            .iter()
            .any(|text| text == "2h 10m until full · Balanced")
    );
    Ok(())
}

#[test]
fn core_bluetooth_peek_distinguishes_missing_adapter_from_power_off() -> Result<()> {
    let mut state = core_bluetooth();
    state["adapters"]["items"] = serde_json::json!({});
    state["adapters"]["order"] = serde_json::json!([]);
    state["devices"]["items"] = serde_json::json!({});
    state["devices"]["order"] = serde_json::json!([]);
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(
        text_value(&values, "status.peek.summary"),
        "No Bluetooth adapter"
    );
    assert_eq!(text_value(&values, "status.primary"), "Bluetooth off");
    Ok(())
}

#[test]
fn status_peek_luau_limits_rows_and_localizes_the_overflow() -> Result<()> {
    crate::i18n::init();
    let devices = (1..=6)
        .map(|index| {
            serde_json::json!({
                "id": format!("device-{index}"),
                "name": format!("Device {index}"),
                "battery": ""
            })
        })
        .collect();
    let initial = BTreeMap::from([
        ("panel.open".into(), Value::Bool(true)),
        ("status.ready".into(), Value::Bool(true)),
        ("status.active".into(), Value::Bool(true)),
        ("status.kind".into(), Value::Text("bluetooth".into())),
        ("status.heading".into(), Value::Text("Bluetooth".into())),
        (
            "status.primary".into(),
            Value::Text("6 connected devices".into()),
        ),
        ("status.peek.title".into(), Value::Text("Bluetooth".into())),
        (
            "status.peek.summary".into(),
            Value::Text("6 connected devices".into()),
        ),
        ("status.peek.detail".into(), Value::Text(String::new())),
        ("status.peek.devices".into(), Value::list(devices)),
    ]);
    let h = Harness::load(
        &root(),
        Path::new("status/peek.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 380,
            height: 300,
            scale: 1.,
        },
    )?;
    let texts = h
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for index in 1..=4 {
        let expected = format!("Device {index}");
        assert!(texts.contains(&expected.as_str()));
    }
    assert!(!texts.contains(&"Device 5"));
    assert!(!texts.contains(&"Device 6"));
    assert!(texts.contains(&"2 more devices"));
    Ok(())
}
#[test]
fn status_source_wakes_are_noop_when_unchanged_keep_last_data_and_release_on_drop() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::{sync::mpsc, time::Duration};
    for kind in Kind::ALL {
        let source = Arc::new(FakeSource::new(data())?);
        let weak = Arc::downgrade(&source);
        let h = harness(1.)?;
        let mut runtime =
            crate::reload::Runtime::new(h.session).with_status(kind, source.clone())?;
        let (send, receive) = mpsc::channel();
        runtime.services(Box::new(move |event| {
            let _ = send.send(event);
        }));
        let scene = runtime
            .stage(receive.recv_timeout(Duration::from_millis(200))?)?
            .ok_or("Source wake did not project status")?;
        runtime.session.commit(scene);
        assert!(
            runtime.stage(UiEvent::Model(BTreeMap::new()))?.is_none(),
            "unchanged Source must not layout or raster"
        );
        let primary = runtime.session.stores.get("status.primary").cloned();
        let previous_data = Arc::downgrade(&source.snapshot().data);
        source.disconnect()?;
        assert!(
            runtime
                .stage(receive.recv_timeout(Duration::from_millis(200))?)?
                .is_none(),
            "disconnect keeps the last snapshot"
        );
        assert_eq!(
            runtime.session.stores.get("status.primary"),
            primary.as_ref()
        );
        source.publish(serde_json::json!({}))?;
        let scene = runtime
            .stage(receive.recv_timeout(Duration::from_millis(200))?)?
            .ok_or("missing data must invalidate availability")?;
        runtime.session.commit(scene);
        assert_eq!(
            runtime.session.stores.get("status.ready"),
            Some(&Value::Bool(false))
        );
        assert!(
            previous_data.upgrade().is_none(),
            "view caches must not retain replaced Source payloads"
        );
        let current_data = Arc::downgrade(&source.snapshot().data);
        assert_eq!(source.calls(), 0);
        drop(source);
        assert!(weak.upgrade().is_some());
        drop(runtime);
        assert!(
            weak.upgrade().is_none(),
            "closed worker must release its Source and subscription"
        );
        assert!(
            current_data.upgrade().is_none(),
            "closed worker must release the last payload too"
        );
    }
    Ok(())
}
#[test]
fn coarse_status_rejects_invalid_numbers_and_bounds_external_labels() -> Result<()> {
    let mut snapshot = data();
    snapshot["battery"]["percentage"] = serde_json::json!(101);
    snapshot["bluetooth"]["connected_devices"] = serde_json::json!([null]);
    snapshot["network"]["wireless_enabled"] = serde_json::Value::Null;
    for kind in Kind::ALL {
        let source = Arc::new(FakeSource::new(snapshot.clone())?);
        assert_eq!(
            Status::new(kind, source).project()["status.ready"],
            Value::Bool(false)
        );
    }
    let mut off = data();
    off["network"]["wireless_enabled"] = serde_json::json!(false);
    off["network"]["connected"] = serde_json::json!(false);
    off["bluetooth"]["powered"] = serde_json::json!(false);
    for kind in [Kind::Network, Kind::Bluetooth] {
        let source = Arc::new(FakeSource::new(off.clone())?);
        let values = Status::new(kind, source).project();
        assert_eq!(values["status.ready"], Value::Bool(true));
        assert_eq!(values["status.active"], Value::Bool(false));
    }
    let mut snapshot = data();
    snapshot["network"]["name"] = serde_json::json!("界\n".repeat(200));
    let source = Arc::new(FakeSource::new(snapshot)?);
    let projected = Status::new(Kind::Network, source).project();
    let Value::Text(label) = &projected["status.primary"] else {
        panic!("text label")
    };
    assert!(label.chars().count() <= 33 && !label.contains('\n') && label.ends_with('…'));
    Ok(())
}

#[test]
fn status_registry_keeps_only_cold_metadata_and_never_starts_another_service() -> Result<()> {
    let palette = Arc::new(root().join("palette.json"));
    let colors = Arc::new(bead::render::Palette::read(&palette)?);
    let scale: serde_json::Value =
        serde_json::from_str(include_str!("../../bead-ui/assets/size-scale.json"))?;
    let radius = scale["radii"]["medium"]
        .as_f64()
        .ok_or("shared radius missing")? as f32;
    let owner = Arc::new(std::sync::OnceLock::new());
    let specs = Kind::ALL
        .into_iter()
        .map(|kind| {
            crate::popouts::status(
                kind,
                palette.clone(),
                colors.clone(),
                owner.clone(),
                None,
                radius,
                false,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert!(owner.get().is_none());
    for spec in specs {
        assert!(!spec.initial_open && !spec.passive);
        assert!(matches!(spec.renderer, bead::render::RendererKind::Cpu));
        assert!(spec.fallback.asset_data.is_empty() && spec.fallback.font_files.is_empty());
        assert!(
            (spec.factory)(true).is_err(),
            "an absent shared owner must not start a second backend"
        );
        assert!(owner.get().is_none());
    }
    Ok(())
}

#[test]
fn each_status_source_selects_its_existing_service_before_copying_data() -> Result<()> {
    let mut snapshot: bead::services::Snapshot = serde_json::from_value(data())?;
    snapshot.tray = vec![bead::services::TrayItem {
        icon_pixmap: vec![(1024, 1024, vec![255; 4 * 1024 * 1024])],
        ..Default::default()
    }];
    for kind in Kind::ALL {
        let selected = kind.projection()(&snapshot);
        let object = selected.as_object().ok_or("object projection")?;
        assert_eq!(object.len(), 1);
        assert_eq!(selected[kind.name()], data()[kind.name()]);
        let full = Status::new(kind, Arc::new(FakeSource::new(data())?)).project();
        let subset = Status::new(kind, Arc::new(FakeSource::new(selected.clone())?)).project();
        assert_eq!(
            full, subset,
            "view bindings must not depend on unrelated services"
        );
        assert!(selected.get("tray").is_none() && selected.get("clock").is_none());
        assert!(
            serde_json::to_vec(&selected)?.len() < 1024,
            "bulk tray data must never be copied into the view Source"
        );
    }
    Ok(())
}

#[test]
fn network_core_primary_is_explicit_not_map_order() -> Result<()> {
    let mut state = core_network();
    state["primary_connection"] = serde_json::json!("wired");
    // The active Wi-Fi profile and AP are first in the explicit collection order.
    state["connections"]["order"] = serde_json::json!(["wifi-connection", "wired"]);
    let source = Arc::new(FakeSource::new(state)?);
    let values = Status::core_network(source).project();
    assert_eq!(text_value(&values, "status.primary"), "Ethernet");
    assert_eq!(text_value(&values, "status.secondary"), "Wi-Fi on");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));

    let mut no_primary = core_network();
    no_primary
        .as_object_mut()
        .unwrap()
        .remove("primary_connection");
    let values = Status::core_network(Arc::new(FakeSource::new(no_primary)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Not connected");
    assert_eq!(values["status.ready"], Value::Bool(true));
    Ok(())
}

#[test]
fn core_network_peek_uses_active_ap_when_primary_is_missing() -> Result<()> {
    let mut state = core_network();
    state.as_object_mut().unwrap().remove("primary_connection");
    let values = Status::core_network(Arc::new(FakeSource::new(state)?)).project();

    // The full status projection remains tied to primary_connection, while the
    // hover preview follows the AP that is still reported active.
    assert_eq!(text_value(&values, "status.primary"), "Not connected");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );
    assert_eq!(text_value(&values, "status.peek.summary"), "Home Wi-Fi");
    assert_eq!(text_value(&values, "status.peek.detail"), "Signal 86%");
    Ok(())
}

#[test]
fn core_network_peek_prioritizes_wired_unavailable_and_radio_off_states() -> Result<()> {
    let mut wired = core_network();
    wired["devices"]["items"]["eth0"] = serde_json::json!(
        {"name":"Ethernet","kind":"ethernet","connected":true}
    );
    wired["devices"]["order"] = serde_json::json!(["wifi0", "eth0"]);
    wired["primary_connection"] = serde_json::json!("wired");
    let values = Status::core_network(Arc::new(FakeSource::new(wired)?)).project();
    assert_eq!(text_value(&values, "status.peek.title"), "Ethernet");
    assert_eq!(
        text_value(&values, "status.peek.summary"),
        "Connected by cable"
    );
    assert_eq!(text_value(&values, "status.peek.detail"), "");
    assert_eq!(text_value(&values, "status.primary"), "Ethernet");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );

    let mut unavailable = core_network();
    unavailable["devices"]["items"] = serde_json::json!({});
    unavailable["devices"]["order"] = serde_json::json!([]);
    unavailable["access_points"]["items"] = serde_json::json!({});
    unavailable["access_points"]["order"] = serde_json::json!([]);
    let values = Status::core_network(Arc::new(FakeSource::new(unavailable)?)).project();
    assert_eq!(text_value(&values, "status.peek.title"), "Wi-Fi");
    assert_eq!(
        text_value(&values, "status.peek.summary"),
        "No Wi-Fi adapter"
    );
    assert_eq!(text_value(&values, "status.peek.detail"), "");

    let mut off = core_network();
    off["wireless_enabled"] = serde_json::json!(false);
    off.as_object_mut().unwrap().remove("primary_connection");
    let values = Status::core_network(Arc::new(FakeSource::new(off)?)).project();
    assert_eq!(text_value(&values, "status.peek.title"), "Wi-Fi");
    assert_eq!(text_value(&values, "status.peek.summary"), "Off");
    assert_eq!(text_value(&values, "status.peek.detail"), "");
    assert_eq!(text_value(&values, "status.primary"), "Not connected");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );
    Ok(())
}

#[test]
fn network_core_signal_requires_primary_ap_mapping() -> Result<()> {
    let mut state = core_network();
    state["access_points"]["items"]["ap0"]["strength"] = serde_json::json!(77);
    state["access_points"]["items"]["ap0"]["connection"] = serde_json::json!("wired");
    state["access_points"]["items"]["ap1"] = serde_json::json!({
        "device":"wifi0","name":"First matching AP","strength":32,
        "secured":true,"active":true,"connection":"wifi-connection"
    });
    state["access_points"]["order"] = serde_json::json!(["ap1", "ap0"]);
    let values = Status::core_network(Arc::new(FakeSource::new(state.clone())?)).project();
    assert_eq!(text_value(&values, "status.primary"), "First matching AP");
    assert_eq!(text_value(&values, "status.detail"), "Signal strength 32%");

    state["access_points"]["items"]
        .as_object_mut()
        .unwrap()
        .remove("ap1");
    state["access_points"]["order"] = serde_json::json!(["ap0"]);
    state["access_points"]["items"]["ap0"]["connection"] = serde_json::json!("wifi-connection");
    state["access_points"]["items"]["ap0"]["strength"] = serde_json::json!(0);
    let values = Status::core_network(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Home Wi-Fi");
    assert_eq!(text_value(&values, "status.detail"), "Signal strength 0%");
    Ok(())
}

#[test]
fn network_core_unknown_or_invalid_state_never_becomes_zero() -> Result<()> {
    let missing_topic =
        Status::core_network(Arc::new(FakeSource::new(serde_json::json!({}))?)).project();
    assert_eq!(missing_topic["status.ready"], Value::Bool(false));
    assert_eq!(
        text_value(&missing_topic, "status.primary"),
        "Network unavailable"
    );
    assert_eq!(
        text_value(&missing_topic, "status.secondary"),
        "Status data unavailable"
    );

    let mut no_primary = core_network();
    no_primary
        .as_object_mut()
        .unwrap()
        .remove("primary_connection");
    no_primary["wireless_enabled"] = serde_json::json!(false);
    let values = Status::core_network(Arc::new(FakeSource::new(no_primary)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Not connected");
    assert_eq!(text_value(&values, "status.secondary"), "Wi-Fi off");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );

    let mut invalid_strength = core_network();
    invalid_strength["access_points"]["items"]["ap0"]["strength"] = serde_json::json!(101);
    let values = Status::core_network(Arc::new(FakeSource::new(invalid_strength)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(false));
    assert_eq!(text_value(&values, "status.primary"), "Network unavailable");

    let mut invalid_order = core_network();
    invalid_order["connections"]["order"] = serde_json::json!(["wired"]);
    let values = Status::core_network(Arc::new(FakeSource::new(invalid_order)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(false));

    let mut dangling = core_network();
    dangling["primary_connection"] = serde_json::json!("missing-connection");
    let values = Status::core_network(Arc::new(FakeSource::new(dangling)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(false));

    let mut dangling_device = core_network();
    dangling_device["access_points"]["items"]["ap0"]["device"] =
        serde_json::json!("missing-device");
    let values = Status::core_network(Arc::new(FakeSource::new(dangling_device)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(false));

    let mut oversized = core_network();
    let mut items = serde_json::Map::new();
    let mut order = vec![serde_json::json!("wifi0")];
    items.insert(
        "wifi0".into(),
        serde_json::json!({"name":"Wi-Fi","kind":"wireless","connected":true}),
    );
    for index in 0..2048 {
        let id = format!("dev-{index}");
        items.insert(
            id.clone(),
            serde_json::json!({"name":"Device","kind":"wireless","connected":false}),
        );
        order.push(serde_json::json!(id));
    }
    oversized["devices"] = serde_json::json!({"items":items,"order":order});
    let values = Status::core_network(Arc::new(FakeSource::new(oversized)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(false));
    assert_eq!(text_value(&values, "status.primary"), "Network unavailable");

    let mut controls = core_network();
    controls["connections"]["items"]["wifi-connection"]["name"] =
        serde_json::json!("Home\nWi-Fi\u{0007}");
    controls["access_points"]["items"]["ap0"]["name"] = serde_json::json!("");
    let values = Status::core_network(Arc::new(FakeSource::new(controls)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Home Wi-Fi ");
    Ok(())
}

#[test]
fn network_core_disconnect_retains_last_good_until_recovery() -> Result<()> {
    let source = Arc::new(FakeSource::new(core_network())?);
    let status = Status::core_network(source.clone());
    let good = status.project();
    assert_eq!(text_value(&good, "status.primary"), "Home Wi-Fi");
    source.disconnect()?;
    let stale = status.project();
    assert_eq!(stale["status.ready"], Value::Bool(false));
    assert_eq!(stale["status.active"], Value::Bool(true));
    assert_eq!(text_value(&stale, "status.primary"), "Home Wi-Fi");
    assert_eq!(text_value(&stale, "status.detail"), "Signal strength 86%");
    assert_eq!(text_value(&stale, "status.secondary"), "Status stale");

    let mut recovered = core_network();
    recovered["primary_connection"] = serde_json::json!("wired");
    recovered["wireless_enabled"] = serde_json::json!(false);
    source.publish(recovered)?;
    let current = status.project();
    assert_eq!(current["status.ready"], Value::Bool(true));
    assert_eq!(current["status.active"], Value::Bool(true));
    assert_eq!(text_value(&current, "status.primary"), "Ethernet");
    assert_eq!(text_value(&current, "status.secondary"), "Wi-Fi off");
    Ok(())
}

#[test]
fn network_legacy_preview_remains_unchanged() -> Result<()> {
    let values = Status::new(Kind::Network, Arc::new(FakeSource::new(data())?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "Fixture network");
    assert_eq!(text_value(&values, "status.secondary"), "Wi-Fi on");
    assert_eq!(text_value(&values, "status.detail"), "Signal strength 86%");
    Ok(())
}

#[test]
fn bluetooth_core_power_and_connected_order() -> Result<()> {
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(core_bluetooth())?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "Headphones Studio");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth on");
    assert_eq!(text_value(&values, "status.detail"), "2 connected devices");

    let mut reversed = core_bluetooth();
    reversed["devices"]["order"] =
        serde_json::json!(["a-empty", "m-first", "z-off", "paired-only"]);
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(reversed)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Connected device");

    let mut empty = core_bluetooth();
    for device in empty["devices"]["items"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        device["connected"] = serde_json::json!(false);
    }
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(empty)?)).project();
    assert_eq!(
        text_value(&values, "status.primary"),
        "No connected devices"
    );
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth on");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );

    let mut off = core_bluetooth();
    off["adapters"]["items"]["adapter-on-a"]["powered"] = serde_json::json!(false);
    off["adapters"]["items"]["adapter-on-b"]["powered"] = serde_json::json!(false);
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(off)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(false));
    assert_eq!(text_value(&values, "status.primary"), "Bluetooth off");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth off");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Connection controls unavailable"
    );

    let mut controls = core_bluetooth();
    controls["devices"]["items"]["m-first"]["name"] =
        serde_json::json!("Headphones\nStudio\u{0007}");
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(controls)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Headphones Studio ");

    let mut long_name = core_bluetooth();
    long_name["devices"]["items"]["m-first"]["name"] = serde_json::json!("A".repeat(40));
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(long_name)?)).project();
    let primary = text_value(&values, "status.primary");
    assert_eq!(primary.chars().count(), 33);
    assert!(primary.ends_with('…'));
    Ok(())
}

#[test]
fn bluetooth_core_invalid_is_not_powered_off() -> Result<()> {
    let missing_topic =
        Status::core_bluetooth(Arc::new(FakeSource::new(serde_json::json!({}))?)).project();
    assert_eq!(missing_topic["status.ready"], Value::Bool(false));
    assert_eq!(
        text_value(&missing_topic, "status.primary"),
        "Bluetooth unavailable"
    );
    assert_eq!(
        text_value(&missing_topic, "status.secondary"),
        "Status data unavailable"
    );

    let mut dangling = core_bluetooth();
    dangling["devices"]["items"]["m-first"]["adapter"] = serde_json::json!("missing");
    let mut cases = vec![dangling];

    let mut oversized = core_bluetooth();
    let mut items = serde_json::Map::new();
    let mut order = Vec::new();
    for index in 0..2049 {
        let id = format!("adapter-{index}");
        items.insert(
            id.clone(),
            serde_json::json!({"name":"Adapter","powered":true,"discovering":false}),
        );
        order.push(serde_json::json!(id));
    }
    oversized["adapters"] = serde_json::json!({"items":items,"order":order});
    cases.push(oversized);

    let mut invalid_battery = core_bluetooth();
    invalid_battery["devices"]["items"]["m-first"]["battery"] = serde_json::json!(101);
    cases.push(invalid_battery);

    for state in cases {
        let values = Status::core_bluetooth(Arc::new(FakeSource::new(state)?)).project();
        assert_eq!(values["status.ready"], Value::Bool(false));
        assert_eq!(values["status.active"], Value::Bool(false));
        assert_eq!(
            text_value(&values, "status.primary"),
            "Bluetooth unavailable"
        );
    }
    Ok(())
}

#[test]
fn bluetooth_core_disconnect_retains_last_good_until_recovery() -> Result<()> {
    let source = Arc::new(FakeSource::new(core_bluetooth())?);
    let status = Status::core_bluetooth(source.clone());
    let good = status.project();
    assert_eq!(text_value(&good, "status.primary"), "Headphones Studio");

    let mut invalid = core_bluetooth();
    invalid["devices"]["items"]["m-first"]["battery"] = serde_json::json!(101);
    source.publish(invalid)?;
    let stale_invalid = status.project();
    assert_eq!(stale_invalid["status.ready"], Value::Bool(false));
    assert_eq!(
        text_value(&stale_invalid, "status.primary"),
        "Headphones Studio"
    );
    assert_eq!(
        text_value(&stale_invalid, "status.secondary"),
        "Status stale"
    );

    source.disconnect()?;
    let stale = status.project();
    assert_eq!(stale["status.ready"], Value::Bool(false));
    assert_eq!(stale["status.active"], Value::Bool(true));
    assert_eq!(text_value(&stale, "status.primary"), "Headphones Studio");
    assert_eq!(text_value(&stale, "status.detail"), "2 connected devices");
    assert_eq!(text_value(&stale, "status.secondary"), "Status stale");

    let mut recovered = core_bluetooth();
    recovered["adapters"]["items"]["adapter-on-a"]["powered"] = serde_json::json!(false);
    recovered["adapters"]["items"]["adapter-on-b"]["powered"] = serde_json::json!(false);
    source.publish(recovered)?;
    let current = status.project();
    assert_eq!(current["status.ready"], Value::Bool(true));
    assert_eq!(text_value(&current, "status.primary"), "Bluetooth off");
    assert_eq!(text_value(&current, "status.secondary"), "Bluetooth off");
    Ok(())
}

#[test]
fn bluetooth_power_legacy_preview_unchanged() -> Result<()> {
    let values = Status::new(Kind::Bluetooth, Arc::new(FakeSource::new(data())?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "2 devices connected");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth on");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Fixture headphones, Fixture keyboard"
    );
    assert!(!values.contains_key("bluetooth.power_checked"));
    assert!(!values.contains_key("bluetooth.power_enabled"));
    Ok(())
}

#[test]
fn battery_core_optional_readings_remain_unknown() -> Result<()> {
    // The fixture is on mains with 1h30m to full; the estimate follows the system's on-battery flag.
    let cases = [
        (Some(83.0), Some(true), "83%", "1h 30m until full", true),
        (Some(44.0), Some(false), "44%", "1h 30m until full", false),
        (Some(57.0), None, "57%", "1h 30m until full", false),
        (
            None,
            Some(true),
            "Battery level unknown",
            "1h 30m until full",
            true,
        ),
        (
            None,
            None,
            "Battery level unknown",
            "1h 30m until full",
            false,
        ),
    ];
    for (percentage, charging, primary, secondary, active) in cases {
        let mut state = core_battery();
        if let Some(percentage) = percentage {
            state["percentage"] = serde_json::Value::from(percentage);
        } else {
            state.as_object_mut().unwrap().remove("percentage");
        }
        if let Some(charging) = charging {
            state["charging"] = serde_json::Value::from(charging);
        } else {
            state.as_object_mut().unwrap().remove("charging");
        }
        let values = Status::core_battery(Arc::new(FakeSource::new(state)?)).project();
        assert_eq!(values["status.ready"], Value::Bool(true));
        assert_eq!(values["status.active"], Value::Bool(active));
        assert_eq!(text_value(&values, "status.primary"), primary);
        assert_eq!(text_value(&values, "status.secondary"), secondary);
        assert_eq!(
            text_value(&values, "status.detail"),
            "Power profiles unavailable"
        );
    }

    for (percentage, expected) in [(0.0, "0%"), (100.0, "100%"), (12.4, "12%"), (12.6, "13%")] {
        let mut state = core_battery();
        state["percentage"] = serde_json::json!(percentage);
        let values = Status::core_battery(Arc::new(FakeSource::new(state)?)).project();
        assert_eq!(text_value(&values, "status.primary"), expected);
    }
    Ok(())
}

#[test]
fn battery_core_invalid_is_not_zero() -> Result<()> {
    let mut missing_health = core_battery();
    missing_health.as_object_mut().unwrap().remove("health");
    let mut cases = vec![missing_health];
    for percentage in [
        serde_json::json!(-0.1),
        serde_json::json!(100.1),
        serde_json::json!("NaN"),
    ] {
        let mut state = core_battery();
        state["percentage"] = percentage;
        cases.push(state);
    }
    assert!(serde_json::Number::from_f64(f64::NAN).is_none());

    for state in cases {
        let values = Status::core_battery(Arc::new(FakeSource::new(state)?)).project();
        assert_eq!(values["status.ready"], Value::Bool(false));
        assert_eq!(values["status.active"], Value::Bool(false));
        assert_eq!(text_value(&values, "status.primary"), "Battery unavailable");
        assert_eq!(
            text_value(&values, "status.secondary"),
            "Status data unavailable"
        );
    }
    Ok(())
}

#[test]
fn battery_core_disconnect_retains_last_good_until_recovery() -> Result<()> {
    let source = Arc::new(FakeSource::new(core_battery())?);
    let status = Status::core_battery(source.clone());
    let good = status.project();
    assert_eq!(text_value(&good, "status.primary"), "83%");
    assert_eq!(text_value(&good, "status.secondary"), "1h 30m until full");
    assert_eq!(good["status.active"], Value::Bool(true));

    let mut stale = core_battery();
    stale["health"]["stale"] = serde_json::json!(true);
    let mut unavailable = core_battery();
    unavailable["health"]["available"] = serde_json::json!(false);
    let mut invalid = core_battery();
    invalid["percentage"] = serde_json::json!(101.0);
    for state in [stale, unavailable, invalid] {
        source.publish(state)?;
        let values = status.project();
        assert_eq!(values["status.ready"], Value::Bool(false));
        assert_eq!(text_value(&values, "status.primary"), "83%");
        assert_eq!(text_value(&values, "status.secondary"), "Status stale");
        assert_eq!(values["status.active"], Value::Bool(true));
    }

    source.disconnect()?;
    let disconnected = status.project();
    assert_eq!(disconnected["status.ready"], Value::Bool(false));
    assert_eq!(text_value(&disconnected, "status.primary"), "83%");
    assert_eq!(
        text_value(&disconnected, "status.secondary"),
        "Status stale"
    );
    assert_eq!(disconnected["status.active"], Value::Bool(true));

    let mut recovered = core_battery();
    recovered.as_object_mut().unwrap().remove("percentage");
    recovered.as_object_mut().unwrap().remove("charging");
    for key in ["time_to_full_seconds", "on_battery", "state"] {
        recovered.as_object_mut().unwrap().remove(key);
    }
    source.publish(recovered)?;
    let current = status.project();
    assert_eq!(current["status.ready"], Value::Bool(true));
    assert_eq!(
        text_value(&current, "status.primary"),
        "Battery level unknown"
    );
    assert_eq!(
        text_value(&current, "status.secondary"),
        "Connected to power"
    );
    assert_eq!(current["status.active"], Value::Bool(false));
    Ok(())
}

/// QML's estimate: time to empty on battery, time to full on mains, minutes below an hour,
/// and a plain state when the system gives no usable time.
#[test]
fn battery_estimate_follows_the_systems_time_and_power_source() -> Result<()> {
    let estimate = |on_battery: Option<bool>, empty: Option<u64>, full: Option<u64>| {
        let mut state = core_battery();
        let map = state.as_object_mut().unwrap();
        for key in [
            "on_battery",
            "time_to_empty_seconds",
            "time_to_full_seconds",
        ] {
            map.remove(key);
        }
        if let Some(on) = on_battery {
            map.insert("on_battery".into(), on.into());
        }
        if let Some(seconds) = empty {
            map.insert("time_to_empty_seconds".into(), seconds.into());
        }
        if let Some(seconds) = full {
            map.insert("time_to_full_seconds".into(), seconds.into());
        }
        let values = Status::core_battery(Arc::new(FakeSource::new(state).unwrap())).project();
        text_value(&values, "status.secondary").to_owned()
    };
    assert_eq!(
        estimate(Some(true), Some(7200), Some(60)),
        "2h 0m remaining"
    );
    assert_eq!(estimate(Some(true), Some(61), None), "2m remaining");
    assert_eq!(estimate(Some(true), Some(3599), None), "1h 0m remaining");
    assert_eq!(
        estimate(Some(true), None, Some(60)),
        "Remaining time unavailable"
    );
    assert_eq!(
        estimate(Some(true), Some(0), None),
        "Remaining time unavailable"
    );
    assert_eq!(
        estimate(Some(false), Some(7200), Some(1800)),
        "30m until full"
    );
    assert_eq!(
        estimate(Some(false), Some(60), Some(3601)),
        "1h 1m until full"
    );
    assert_eq!(estimate(Some(false), None, None), "Connected to power");
    assert_eq!(estimate(None, Some(60), Some(60)), "1m until full");
    Ok(())
}

#[test]
fn battery_legacy_preview_remains_unchanged() -> Result<()> {
    let values = Status::new(Kind::Battery, Arc::new(FakeSource::new(data())?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "83%");
    assert_eq!(text_value(&values, "status.secondary"), "Charging");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Power profiles unavailable"
    );
    Ok(())
}

/// A view declares its stores from the first projection (the daemon's topic starts unavailable), so a later
/// live projection must not bring a key the first one lacked ("unknown state audio.level").
#[test]
fn audio_projection_keys_do_not_change_when_the_topic_becomes_available() -> Result<()> {
    let mut down = core_audio();
    down["health"]["available"] = serde_json::json!(false);
    let before = Status::core_audio(Arc::new(FakeSource::new(down)?)).project();
    let live = Status::core_audio(Arc::new(FakeSource::new(core_audio())?)).project();
    assert_eq!(live["status.ready"], Value::Bool(true));
    let missing: Vec<_> = live.keys().filter(|k| !before.contains_key(*k)).collect();
    assert!(missing.is_empty(), "keys first seen when live: {missing:?}");
    Ok(())
}

#[test]
fn audio_core_uses_only_explicit_default_output() -> Result<()> {
    let mut state = core_audio();
    state.as_object_mut().unwrap().remove("default_output");
    let values = Status::core_audio(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "Output unavailable");
    assert_eq!(text_value(&values, "status.device"), "Output unavailable");
    Ok(())
}

#[test]
fn audio_core_readback_unknown_is_not_zero_or_unmuted() -> Result<()> {
    let mut state = core_audio();
    state["nodes"]["items"]["out"]["ready"] = serde_json::json!(false);
    state["nodes"]["items"]["out"]["readback"] = serde_json::json!({});
    let values = Status::core_audio(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Volume unknown");
    assert_eq!(text_value(&values, "status.mute"), "Mute state unknown");
    Ok(())
}

#[test]
fn audio_core_partial_readback_and_boosted_volume() -> Result<()> {
    let mut state = core_audio();
    state["nodes"]["items"]["out"]["readback"] = serde_json::json!({"perceptual_volume":1.5});
    state["nodes"]["items"]["out"]["ready"] = serde_json::json!(false);
    let values = Status::core_audio(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "150%");
    assert_eq!(text_value(&values, "status.mute"), "Mute state unknown");

    let mut overflow = core_audio();
    overflow["nodes"]["items"]["out"]["ready"] = serde_json::json!(false);
    overflow["nodes"]["items"]["out"]["readback"] =
        serde_json::json!({"perceptual_volume":f64::MAX,"muted":true});
    let values = Status::core_audio(Arc::new(FakeSource::new(overflow)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Volume unknown");
    assert_eq!(text_value(&values, "status.mute"), "Muted");
    Ok(())
}

#[test]
fn audio_core_legacy_ready_volume_uses_perceptual_scale() -> Result<()> {
    let mut state = core_audio();
    state["nodes"]["items"]["out"]
        .as_object_mut()
        .unwrap()
        .remove("readback");
    state["nodes"]["items"]["out"]["volume"] = serde_json::json!(0.125);
    let values = Status::core_audio(Arc::new(FakeSource::new(state)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "50%");
    assert_eq!(text_value(&values, "status.mute"), "Not muted");
    Ok(())
}

#[test]
fn audio_core_invalid_snapshot_stays_unavailable() -> Result<()> {
    let wrong_source =
        serde_json::json!({"health":{"available":true,"stale":false},"percentage":50.0});
    let mut invalid_range = core_audio();
    invalid_range["nodes"]["items"]["out"]["volume"] = serde_json::json!(-0.1);
    for state in [wrong_source, invalid_range] {
        let values = Status::core_audio(Arc::new(FakeSource::new(state)?)).project();
        assert_eq!(values["status.ready"], Value::Bool(false));
        assert_eq!(text_value(&values, "status.primary"), "Audio unavailable");
    }
    Ok(())
}

#[test]
fn audio_core_disconnect_retains_last_good_until_recovery() -> Result<()> {
    let source = Arc::new(FakeSource::new(core_audio())?);
    let status = Status::core_audio(source.clone());
    assert_eq!(text_value(&status.project(), "status.primary"), "50%");
    source.disconnect()?;
    let stale = status.project();
    assert_eq!(stale["status.ready"], Value::Bool(false));
    assert_eq!(text_value(&stale, "status.primary"), "50%");
    assert_eq!(text_value(&stale, "status.health"), "Status stale");
    source.publish(core_audio())?;
    assert_eq!(status.project()["status.ready"], Value::Bool(true));
    Ok(())
}

#[test]
fn audio_legacy_preview_remains_unchanged() -> Result<()> {
    crate::i18n::init();
    let initial = BTreeMap::from([
        ("panel.open".into(), Value::Bool(true)),
        (
            "bead.desktop.audio.device".into(),
            Value::Text("Speakers".into()),
        ),
        ("bead.desktop.audio.label".into(), Value::Text("50%".into())),
        ("bead.desktop.audio.percent".into(), Value::Number(50.)),
        ("bead.desktop.audio".into(), Value::Text("volume_up".into())),
    ]);
    let h = Harness::load(
        &root(),
        Path::new("sound/main.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 420,
            height: 320,
            scale: 1.,
        },
    )?;
    let actions = h
        .session
        .view_nodes()
        .iter()
        .filter_map(|node| node.action.as_deref())
        .collect::<Vec<_>>();
    assert!(actions.contains(&"services.audio"));
    assert!(actions.contains(&"services.audio.volume"));
    assert!(h.scene().primitives.iter().any(|primitive| matches!(primitive,
        bead::render::Primitive::Text { text, .. } if text == "Drag or use arrow keys to adjust volume"
    )));
    Ok(())
}

#[test]
fn core_sound_without_client_has_visible_read_only_factory() -> Result<()> {
    let palette_path = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette_path)?;
    let spec =
        crate::popouts::core_sound_with_root(root(), None, palette_path.clone(), colors, false)?;
    assert_eq!(spec.fallback.width, 382.);
    assert_eq!(spec.fallback.bounds.h, 350.);

    let mut runtime = (spec.factory)(true).map_err(std::io::Error::other)?;
    let scene = runtime
        .stage(bead::wayland::UiEvent::Reload)?
        .ok_or("unavailable Sound view did not render")?;
    for expected in [
        "Output",
        "Input",
        "Applications",
        "Microphone unavailable",
        "Applications appear here when they play or record audio.",
        "Output unavailable",
        "—",
    ] {
        assert!(
            scene.primitives.iter().any(|primitive| matches!(primitive,
                bead::render::Primitive::Text { text, .. } if text == expected
            )),
            "missing visible core Sound text: {expected}"
        );
    }

    let initial = BTreeMap::from([
        ("panel.open".into(), Value::Bool(true)),
        ("view.duration".into(), Value::Number(0.23)),
        ("status.ready".into(), Value::Bool(false)),
        ("status.active".into(), Value::Bool(false)),
        (
            "status.device".into(),
            Value::Text("Output unavailable".into()),
        ),
        (
            "status.primary".into(),
            Value::Text("Audio unavailable".into()),
        ),
        (
            "status.mute".into(),
            Value::Text("Mute state unknown".into()),
        ),
        (
            "status.health".into(),
            Value::Text("Status data unavailable".into()),
        ),
        (
            "audio.input_device".into(),
            Value::Text("Microphone unavailable".into()),
        ),
    ]);
    let h = Harness::load(
        &root(),
        Path::new("sound/core.luau"),
        &root().join("palette.json"),
        initial,
        Config {
            width: 420,
            height: 320,
            scale: 1.,
        },
    )?;
    let actions = h
        .session
        .view_nodes()
        .iter()
        .filter_map(|node| node.action.as_deref())
        .collect::<Vec<_>>();
    assert!(
        actions.is_empty(),
        "read-only audio status exposes no controls"
    );
    Ok(())
}

#[test]
fn network_popout_height_follows_rows_and_message() -> Result<()> {
    assert_eq!(crate::status_views::network_popout_height(0, false), 246.);
    assert_eq!(crate::status_views::network_popout_height(2, false), 258.);
    assert_eq!(crate::status_views::network_popout_height(5, false), 450.);
    assert_eq!(crate::status_views::network_popout_height(9, true), 478.);
    for (count, message) in [(0usize, false), (2, false), (5, false), (2, true)] {
        let rows = (0..count)
            .map(|i| {
                serde_json::json!({
                    "id": format!("ap:{i}"), "label": format!("Net {i}"),
                    "action": "Connect", "active": false, "secured": false, "enabled": true
                })
            })
            .collect();
        let expected = crate::status_views::network_popout_height(count, message);
        let initial = BTreeMap::from([
            ("view.edge".into(), Value::Text("top".into())),
            ("panel.open".into(), Value::Bool(true)),
            ("status.ready".into(), Value::Bool(true)),
            ("status.active".into(), Value::Bool(true)),
            ("status.heading".into(), Value::Text("Network".into())),
            ("status.icon".into(), Value::Text("wifi".into())),
            ("status.primary".into(), Value::Text("Home".into())),
            ("status.secondary".into(), Value::Text("Wi-Fi on".into())),
            ("status.detail".into(), Value::Text(String::new())),
            ("network.wifi_checked".into(), Value::Bool(true)),
            ("network.wifi_enabled".into(), Value::Bool(true)),
            ("network.wifi_busy".into(), Value::Bool(false)),
            ("network.wifi_message".into(), Value::Text(String::new())),
            ("network.rows".into(), Value::list(rows)),
            ("network.height".into(), Value::Number(expected)),
            ("network.link_visible".into(), Value::Bool(message)),
            (
                "network.link_message".into(),
                Value::Text(if message { "Network list changed" } else { "" }.into()),
            ),
        ]);
        let initial = with_flow_stores(initial);
        let h = Harness::load(
            &root(),
            Path::new("status/network.luau"),
            &root().join("palette.json"),
            initial,
            Config {
                width: 380,
                height: 600,
                scale: 1.,
            },
        )?;
        let panel = h
            .scene()
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                bead::render::Primitive::Rounded { rect, .. } if (rect.w - 310.).abs() < 0.1 => {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("panel is drawn");
        // B0095: the list is laid out as QML's (three groups, scrolling beyond 210 px); the body holds the header and the list.
        assert!(
            f64::from(panel.h) >= 2. * 15. + 34. + 10. + 100.,
            "{count} rows, message {message}: body includes the header, the groups and the form footer"
        );
        for node in h.session.view_nodes() {
            let rect = bead::render::tree::rect(&node.draw);
            assert!(
                rect.y + rect.h <= panel.y + panel.h - 15. + 0.1,
                "{} extends below the panel content: {:?} vs {:?}",
                node.id,
                rect,
                panel
            );
        }
    }
    Ok(())
}

/// B0050 goldens: password prompt, wrong-password re-prompt, hidden join form, saved list.
#[test]
fn network_join_pages_have_goldens_at_both_scales() -> Result<()> {
    // Install the built-in English catalog so the pages show real text.
    let _ = crate::i18n::tr("bead.panel.network-back", &[]);
    let saved = |confirming: bool| {
        Value::list(vec![
            serde_json::json!({"id":"forget:c0","label":"Home","caption":"Secured","confirming":false,"enabled":true}),
            serde_json::json!({"id":"forget:c9","label":"Old flat","caption":"Secured","confirming":confirming,"enabled":true}),
            serde_json::json!({"id":"forget:c4","label":"Guest","caption":"Open","confirming":false,"enabled":true}),
        ])
    };
    type Page = (&'static str, Vec<(&'static str, Value)>);
    let pages: [Page; 6] = [
        (
            "list",
            vec![
                (
                    "network.rows",
                    Value::list(vec![
                        serde_json::json!({"id":"dev:wifi0","label":"Home","action":"Disconnect","active":true,"secured":true,"enabled":true}),
                        serde_json::json!({"id":"ap:ap3","label":"Lab network","action":"Connect","active":false,"secured":true,"enabled":true}),
                    ]),
                ),
                ("network.height", Value::Number(342.)),
            ],
        ),
        (
            "prompt",
            vec![
                ("network.view", Value::Text("prompt".into())),
                ("network.prompt_label", Value::Text("Lab network".into())),
                ("network.secret", Value::Text("hunter2pass".into())),
            ],
        ),
        (
            "reprompt",
            vec![
                ("network.view", Value::Text("prompt".into())),
                ("network.prompt_label", Value::Text("Lab network".into())),
                (
                    "network.form_error",
                    Value::Text("Wrong password. Try again.".into()),
                ),
            ],
        ),
        (
            "hidden",
            vec![
                ("network.view", Value::Text("hidden".into())),
                ("network.hidden_ssid", Value::Text("Hidden Lab".into())),
                ("network.hidden_security", Value::Text("sae".into())),
                ("network.secret", Value::Text("hunter2pass".into())),
                ("network.reveal", Value::Bool(false)),
            ],
        ),
        (
            "saved",
            vec![
                ("network.view", Value::Text("saved".into())),
                ("network.saved", saved(false)),
            ],
        ),
        (
            "saved-confirm",
            vec![
                ("network.view", Value::Text("saved".into())),
                ("network.saved", saved(true)),
            ],
        ),
    ];
    for (name, extra) in pages {
        for scale in [1., 1.5] {
            let mut initial = with_flow_stores(BTreeMap::from([
                ("view.edge".into(), Value::Text("top".into())),
                ("panel.open".into(), Value::Bool(true)),
                ("status.ready".into(), Value::Bool(true)),
                ("status.active".into(), Value::Bool(true)),
                ("network.wifi_checked".into(), Value::Bool(true)),
                ("network.wifi_enabled".into(), Value::Bool(true)),
                ("network.wifi_busy".into(), Value::Bool(false)),
                ("network.wifi_message".into(), Value::Text(String::new())),
                ("network.rows".into(), Value::list(Vec::new())),
                ("network.height".into(), Value::Number(262.)),
                ("network.link_visible".into(), Value::Bool(false)),
                ("network.link_message".into(), Value::Text(String::new())),
            ]));
            initial.extend(extra.iter().map(|(k, v)| ((*k).into(), v.clone())));
            let mut h = Harness::load(
                &root(),
                Path::new("status/network.luau"),
                &root().join("palette.json"),
                initial,
                Config {
                    width: 380,
                    height: 400,
                    scale,
                },
            )?;
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("network-{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-network-{name}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}

/// B0055 goldens: device list and every pairing page of the Bluetooth popout.
#[test]
fn bluetooth_pairing_pages_have_goldens_at_both_scales() -> Result<()> {
    // Install the built-in English catalog so the pages show real text.
    let _ = crate::i18n::tr("bead.panel.bluetooth-pair", &[]);
    let devices = Value::list(vec![
        serde_json::json!({"id":"dev:dev_A","label":"Fixture headphones","caption":"80%","battery":"battery_full","icon":"headphones","kind":"link","action":"Disconnect","active":true,"enabled":true}),
        serde_json::json!({"id":"dev:dev_B","label":"Fixture keyboard","caption":"Ready to pair","battery":"","icon":"keyboard","kind":"pair","action":"Pair","active":false,"enabled":true}),
        serde_json::json!({"id":"dev:dev_C","label":"Fixture phone","caption":"Paired","battery":"","icon":"smartphone","kind":"link","action":"Connect","active":false,"enabled":true}),
    ]);
    // Scanning with a connection in flight: the busy row and the other buttons dimmed.
    let busy = Value::list(vec![
        serde_json::json!({"id":"dev:dev_A","label":"Fixture headphones","caption":"80%","battery":"battery_full","icon":"headphones","kind":"link","action":"Disconnect","active":true,"enabled":false}),
        serde_json::json!({"id":"dev:dev_B","label":"Fixture keyboard","caption":"Ready to pair","battery":"","icon":"keyboard","kind":"pair","action":"Pair","active":false,"enabled":false}),
        serde_json::json!({"id":"dev:dev_C","label":"Fixture phone","caption":"15%","battery":"battery_full","icon":"smartphone","kind":"link","action":"Connecting…","active":false,"enabled":false}),
    ]);
    let pair = |kind: &str, text: &str, extra: Vec<(&'static str, Value)>| {
        let mut page = vec![
            ("bluetooth.view", Value::Text("pair".into())),
            (
                "bluetooth.pair_title",
                Value::Text("Pair with Fixture keyboard".into()),
            ),
            ("bluetooth.pair_kind", Value::Text(kind.into())),
            ("bluetooth.pair_text", Value::Text(text.into())),
        ];
        page.extend(extra);
        page
    };
    let countdown = (
        "bluetooth.pair_countdown",
        Value::Text("Expires in 52 s".into()),
    );
    type Page = (&'static str, Vec<(&'static str, Value)>);
    let pages: [Page; 9] = [
        ("list", vec![("bluetooth.devices", devices)]),
        (
            "scanning",
            vec![
                ("bluetooth.devices", busy),
                ("bluetooth.scan_label", Value::Text("Scanning…".into())),
                ("bluetooth.scanning", Value::Bool(true)),
                (
                    "bluetooth.list_message",
                    Value::Text("Bluetooth is busy. Try again.".into()),
                ),
            ],
        ),
        (
            "off",
            vec![
                ("bluetooth.power_checked", Value::Bool(false)),
                ("bluetooth.scan_enabled", Value::Bool(false)),
                (
                    "bluetooth.empty_text",
                    Value::Text("Bluetooth is off".into()),
                ),
            ],
        ),
        (
            "waiting",
            pair("waiting", "Confirm pairing on your device.", vec![]),
        ),
        (
            "pin",
            pair(
                "pin",
                "Enter the pairing code from your device.",
                vec![
                    countdown.clone(),
                    ("bluetooth.secret", Value::Text("1234".into())),
                ],
            ),
        ),
        (
            "confirm",
            pair(
                "confirm",
                "Does 123456 match the code on your device?",
                vec![countdown.clone()],
            ),
        ),
        (
            "display",
            pair(
                "display",
                "Type 654321 on your device, then press Enter.",
                vec![countdown.clone()],
            ),
        ),
        (
            "authorize",
            pair(
                "authorize",
                "Allow this device to connect?",
                vec![countdown],
            ),
        ),
        (
            "error",
            pair(
                "error",
                "Wrong code. Try again.",
                vec![("bluetooth.pair_retry", Value::Bool(true))],
            ),
        ),
    ];
    for (name, extra) in pages {
        for scale in [1., 1.5] {
            let mut initial = BTreeMap::from([
                ("view.edge".into(), Value::Text("top".into())),
                ("view.width".into(), Value::Number(352.)),
                ("panel.open".into(), Value::Bool(true)),
                ("status.ready".into(), Value::Bool(true)),
                ("status.active".into(), Value::Bool(true)),
                ("status.heading".into(), Value::Text("Bluetooth".into())),
                ("status.icon".into(), Value::Text("bluetooth".into())),
                (
                    "status.primary".into(),
                    Value::Text("Fixture headphones".into()),
                ),
                (
                    "status.secondary".into(),
                    Value::Text("Bluetooth on".into()),
                ),
                (
                    "status.detail".into(),
                    Value::Text("1 device connected".into()),
                ),
                ("bluetooth.power_visible".into(), Value::Bool(true)),
                ("bluetooth.power_checked".into(), Value::Bool(true)),
                ("bluetooth.power_enabled".into(), Value::Bool(true)),
                ("bluetooth.power_busy".into(), Value::Bool(false)),
                ("bluetooth.power_message".into(), Value::Text(String::new())),
                ("bluetooth.view".into(), Value::Text("list".into())),
                ("bluetooth.devices".into(), Value::list(Vec::new())),
                ("bluetooth.list_message".into(), Value::Text(String::new())),
                (
                    "bluetooth.scan_label".into(),
                    Value::Text("Find devices".into()),
                ),
                ("bluetooth.scan_enabled".into(), Value::Bool(true)),
                ("bluetooth.scanning".into(), Value::Bool(false)),
                ("bluetooth.empty_text".into(), Value::Text(String::new())),
                ("bluetooth.pair_title".into(), Value::Text(String::new())),
                ("bluetooth.pair_kind".into(), Value::Text("waiting".into())),
                ("bluetooth.pair_text".into(), Value::Text(String::new())),
                (
                    "bluetooth.pair_countdown".into(),
                    Value::Text(String::new()),
                ),
                ("bluetooth.pair_error".into(), Value::Text(String::new())),
                ("bluetooth.pair_busy".into(), Value::Bool(false)),
                ("bluetooth.pair_retry".into(), Value::Bool(false)),
            ]);
            initial.extend(extra.iter().map(|(k, v)| ((*k).into(), v.clone())));
            let mut h = Harness::load(
                &root(),
                Path::new("status/bluetooth.luau"),
                &root().join("palette.json"),
                initial,
                Config {
                    width: 420,
                    height: if matches!(name, "list" | "scanning" | "off") {
                        520
                    } else {
                        300
                    },
                    scale,
                },
            )?;
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("bluetooth-{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-bluetooth-{name}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}

/// N0059: the battery view shows the profile daemon's degradation reason under the picker, and a
/// failed call's message takes that line instead.
#[test]
fn battery_popout_shows_the_degradation_reason_until_a_message_replaces_it() -> Result<()> {
    let line = |message: &str| -> Result<bool> {
        let mut h = Harness::load(
            &root(),
            Path::new("status/battery.luau"),
            &root().join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 380,
                height: 420,
                scale: 1.,
            },
        )?;
        let view = Status::core_battery(Arc::new(FakeSource::new(core_battery())?));
        let mut base = view.project();
        base.insert("view.edge".into(), Value::Text("top".into()));
        base.insert("panel.open".into(), Value::Bool(true));
        base.insert("cc.message".into(), Value::Text(message.into()));
        base.insert(
            "cc.src".into(),
            Value::list(vec![
                serde_json::json!({"id":"power.profile","status":"Balanced","note":"thermal",
                "choices":[{"id":"balanced","name":"Balanced","active":true}]}),
            ]),
        );
        h.update(base)?;
        Ok(h.scene()
            .primitives
            .iter()
            .any(|p| matches!(p, bead::render::Primitive::Text { text, .. } if text == "thermal")))
    };
    assert!(line("")?);
    assert!(!line("Request failed")?);
    Ok(())
}

/// B0086 goldens: the sound popout with application cards and a live microphone meter, and the
/// network popout with Scan networks enabled.
#[test]
fn sound_streams_and_network_scan_have_goldens_at_both_scales() -> Result<()> {
    let _ = crate::i18n::tr("bead.panel.audio-input-level", &[]);
    let streams = Value::list(vec![
        serde_json::json!({"id":"sink-input:7","label":"Fixture player","icon":"","value":65.0,"status":"65%","muted":false,"enabled":true}),
        serde_json::json!({"id":"source-output:3","label":"Fixture recorder","icon":"","value":30.0,"status":"30%","muted":true,"enabled":true}),
    ]);
    let sound = BTreeMap::from([
        ("panel.open".into(), Value::Bool(true)),
        ("view.duration".into(), Value::Number(0.23)),
        ("status.device".into(), Value::Text("Speakers".into())),
        (
            "audio.input_device".into(),
            Value::Text("Microphone".into()),
        ),
        ("cc.streams".into(), streams),
        ("audio.peak".into(), Value::Number(0.6)),
        ("audio.peak_on".into(), Value::Bool(true)),
    ]);
    let mut scan = with_flow_stores(BTreeMap::from([
        ("view.edge".into(), Value::Text("top".into())),
        ("panel.open".into(), Value::Bool(true)),
        ("status.ready".into(), Value::Bool(true)),
        ("status.active".into(), Value::Bool(true)),
        ("status.heading".into(), Value::Text("Network".into())),
        ("status.icon".into(), Value::Text("wifi".into())),
        ("status.primary".into(), Value::Text("Home".into())),
        ("status.secondary".into(), Value::Text("Wi-Fi on".into())),
        ("status.detail".into(), Value::Text(String::new())),
        ("network.wifi_checked".into(), Value::Bool(true)),
        ("network.wifi_enabled".into(), Value::Bool(true)),
        ("network.wifi_busy".into(), Value::Bool(false)),
        ("network.wifi_message".into(), Value::Text(String::new())),
        ("network.rows".into(), Value::list(Vec::new())),
        ("network.height".into(), Value::Number(262.)),
        ("network.link_visible".into(), Value::Bool(false)),
        ("network.link_message".into(), Value::Text(String::new())),
    ]));
    scan.extend([
        ("network.scan_enabled".into(), Value::Bool(true)),
        (
            "network.scan_label".into(),
            Value::Text("Scan networks".into()),
        ),
    ]);
    for (name, file, stores, width, height) in [
        ("sound-streams", "sound/core.luau", sound, 420u32, 640u32),
        ("network-scan", "status/network.luau", scan, 380, 400),
    ] {
        for scale in [1., 1.5] {
            let mut h = Harness::load(
                &root(),
                Path::new(file),
                &root().join("palette.json"),
                stores.clone(),
                Config {
                    width,
                    height,
                    scale,
                },
            )?;
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-{name}-{scale}-diff")),
            )?;
        }
    }
    Ok(())
}

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
        },
        ..Default::default()
    };
    serde_json::to_value(snapshot).unwrap()
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
        "charging":true
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
fn text_value<'a>(values: &'a BTreeMap<String, Value>, key: &str) -> &'a str {
    let Value::Text(value) = &values[key] else {
        panic!("{key} must be text")
    };
    value
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
        ("network.height".into(), Value::Number(222.)),
        ("network.link_visible".into(), Value::Bool(false)),
        ("network.link_message".into(), Value::Text(String::new())),
    ]);
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
        bead::render::Primitive::Rounded { rect, .. } if (rect.w - 48.).abs() < 0.1 && (rect.h - 28.).abs() < 0.1
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
    assert!(h.scene().primitives.iter().any(|primitive| matches!(primitive, bead::render::Primitive::Rounded { rect, .. } if (rect.w - 48.).abs() < 0.1 && (rect.h - 28.).abs() < 0.1)), "the unavailable switch remains visible");
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
fn clicked_status_factories_use_right_edge_control_geometry_and_peeks_stay_small() -> Result<()> {
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
    assert_eq!(status.fallback.width, 400.);
    assert!(matches!(
        status.fallback.motion.as_ref().map(|motion| motion.edge),
        Some(bead::core::Edge::Right)
    ));
    assert_eq!(
        status
            .fallback
            .motion
            .as_ref()
            .map(|motion| motion.duration),
        Some(0.28)
    );
    let position = status
        .position
        .as_ref()
        .ok_or("control panel positioner missing")?;
    let (x, _) = position(&status.fallback, None, (1920, 1200));
    assert_eq!(
        x + 32 + 400,
        1910,
        "visible control frame keeps the shell's right inset"
    );

    let peek =
        crate::popouts::status_peek(Kind::Battery, palette, colors, owner, None, 16., false)?;
    assert_eq!(peek.fallback.width, 190.);
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
    let cases = [
        (Some(83.0), Some(true), "83%", "Charging", true),
        (Some(44.0), Some(false), "44%", "Not charging", false),
        (Some(57.0), None, "57%", "Charging state unknown", false),
        (None, Some(true), "Battery level unknown", "Charging", true),
        (
            None,
            None,
            "Battery level unknown",
            "Charging state unknown",
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
            "Power controls unavailable"
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
    assert_eq!(text_value(&good, "status.secondary"), "Charging");
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
    source.publish(recovered)?;
    let current = status.project();
    assert_eq!(current["status.ready"], Value::Bool(true));
    assert_eq!(
        text_value(&current, "status.primary"),
        "Battery level unknown"
    );
    assert_eq!(
        text_value(&current, "status.secondary"),
        "Charging state unknown"
    );
    assert_eq!(current["status.active"], Value::Bool(false));
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
    assert_eq!(spec.fallback.width, 400.);
    assert_eq!(spec.fallback.bounds.h, 350.);

    let mut runtime = (spec.factory)(true).map_err(std::io::Error::other)?;
    let scene = runtime
        .stage(bead::wayland::UiEvent::Reload)?
        .ok_or("unavailable Sound view did not render")?;
    for expected in [
        "Sound output",
        "Output unavailable",
        "Audio unavailable",
        "Mute state unknown",
        "Status data unavailable",
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
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0], "close");
    Ok(())
}

#[test]
fn network_popout_height_follows_rows_and_message() -> Result<()> {
    assert_eq!(crate::status_views::network_popout_height(0, false), 222.);
    assert_eq!(crate::status_views::network_popout_height(2, false), 302.);
    assert_eq!(crate::status_views::network_popout_height(5, false), 422.);
    assert_eq!(crate::status_views::network_popout_height(9, true), 450.);
    for (count, message) in [(0usize, false), (2, false), (5, false), (2, true)] {
        let rows = (0..count)
            .map(|i| {
                serde_json::json!({
                    "id": format!("ap:{i}"), "label": format!("Net {i}"),
                    "action": "Connect", "active": false, "enabled": true
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
        assert_eq!(
            f64::from(panel.h),
            expected,
            "{count} rows, message {message}"
        );
    }
    Ok(())
}

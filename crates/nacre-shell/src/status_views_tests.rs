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
    reversed["devices"]["order"] = serde_json::json!(
        ["a-empty", "m-first", "z-off", "paired-only"]
    );
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(reversed)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "Connected device");

    let mut empty = core_bluetooth();
    for device in empty["devices"]["items"].as_object_mut().unwrap().values_mut() {
        device["connected"] = serde_json::json!(false);
    }
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(empty)?)).project();
    assert_eq!(text_value(&values, "status.primary"), "No connected devices");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth on");
    assert_eq!(text_value(&values, "status.detail"), "Connection controls unavailable");

    let mut off = core_bluetooth();
    off["adapters"]["items"]["adapter-on-a"]["powered"] = serde_json::json!(false);
    off["adapters"]["items"]["adapter-on-b"]["powered"] = serde_json::json!(false);
    let values = Status::core_bluetooth(Arc::new(FakeSource::new(off)?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(false));
    assert_eq!(text_value(&values, "status.primary"), "Bluetooth off");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth off");
    assert_eq!(text_value(&values, "status.detail"), "Connection controls unavailable");

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
    let missing_topic = Status::core_bluetooth(Arc::new(FakeSource::new(serde_json::json!({}))?))
        .project();
    assert_eq!(missing_topic["status.ready"], Value::Bool(false));
    assert_eq!(text_value(&missing_topic, "status.primary"), "Bluetooth unavailable");
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
        assert_eq!(text_value(&values, "status.primary"), "Bluetooth unavailable");
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
    assert_eq!(text_value(&stale_invalid, "status.primary"), "Headphones Studio");
    assert_eq!(text_value(&stale_invalid, "status.secondary"), "Status stale");

    source.disconnect()?;
    let stale = status.project();
    assert_eq!(stale["status.ready"], Value::Bool(false));
    assert_eq!(stale["status.active"], Value::Bool(true));
    assert_eq!(text_value(&stale, "status.primary"), "Headphones Studio");
    assert_eq!(text_value(&stale, "status.detail"), "2 connected devices");
    assert_eq!(text_value(&stale, "status.secondary"), "Status stale");

    let mut recovered = core_bluetooth();
    recovered["adapters"]["items"]["adapter-on-b"]["powered"] = serde_json::json!(false);
    source.publish(recovered)?;
    let current = status.project();
    assert_eq!(current["status.ready"], Value::Bool(true));
    assert_eq!(text_value(&current, "status.primary"), "Bluetooth off");
    assert_eq!(text_value(&current, "status.secondary"), "Bluetooth off");
    Ok(())
}

#[test]
fn bluetooth_legacy_preview_remains_unchanged() -> Result<()> {
    let values = Status::new(Kind::Bluetooth, Arc::new(FakeSource::new(data())?)).project();
    assert_eq!(values["status.ready"], Value::Bool(true));
    assert_eq!(values["status.active"], Value::Bool(true));
    assert_eq!(text_value(&values, "status.primary"), "2 devices connected");
    assert_eq!(text_value(&values, "status.secondary"), "Bluetooth on");
    assert_eq!(
        text_value(&values, "status.detail"),
        "Fixture headphones, Fixture keyboard"
    );
    Ok(())
}

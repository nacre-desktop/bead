// SPDX-License-Identifier: GPL-3.0-only
//! Exercise the actual Luau shell entries through the panel Runtime.
use super::*;

fn fake_source(data: serde_json::Value) -> std::sync::Arc<dyn bead::core::source::Source> {
    std::sync::Arc::new(bead_test::source::FakeSource::new(data).expect("fixture source"))
}

fn core_network() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},"wireless_enabled":true,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
        "connections":{"items":{"wired":{"name":"Ethernet","kind":"wired","active":true},"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wired","wifi-connection"]},
        "primary_connection":"wifi-connection"
    })
}

fn core_bluetooth() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "adapters":{"items":{"adapter":{"name":"Fixture adapter","powered":true,"discovering":false}},"order":["adapter"]},
        "devices":{"items":{"headphones":{"adapter":"adapter","name":"Fixture headphones","connected":true,"paired":true,"trusted":true,"pairing":false}},"order":["headphones"]}
    })
}

fn core_battery() -> serde_json::Value {
    serde_json::json!({"health":{"available":true,"stale":false},"percentage":83.0,"charging":true,"state":1,"time_to_full_seconds":5400,"on_battery":false})
}

fn core_audio() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},"default_output":"out","default_input":"mic",
        "nodes":{"items":{"out":{"name":"Fixture output","kind":"output","volume":0.5,"muted":false,"ready":true,"readback":{"linear_volume":0.5,"perceptual_volume":0.5,"muted":false}},"mic":{"name":"Fixture mic","kind":"input","volume":0.0,"muted":false,"ready":true}},"order":["mic","out"]}
    })
}

#[test]
fn real_shell_panel_routes_open_and_close_with_prepared_scenes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).expect("shell palette loads");
    let fallback = crate::sound::fallback(colors.get("surface").expect("surface token"));
    let routes = [
        ("dashboard", "dashboard/today/main.luau"),
        ("session", "session/main.luau"),
        ("launcher", "ui/main.luau"),
        ("network", "status/network.luau"),
        ("bluetooth", "status/bluetooth.luau"),
        ("battery", "status/battery.luau"),
        ("sound", "sound/main.luau"),
        ("sound-core", "sound/core.luau"),
        ("calendar", "popouts/main.luau"),
        ("chat", "chat/main.luau"),
        ("control", "control/main.luau"),
        ("wallpaper", "extras/main.luau"),
    ];
    let mut startup_scenes = Vec::new();

    for (name, entry) in routes {
        let session = bead::ui::Session::new(
            root.clone(),
            entry.into(),
            palette.clone(),
            colors.clone(),
            fallback.clone(),
        );
        let mut runtime = Runtime::new(session);
        runtime.state_file = None;
        let mut restored = BTreeMap::from([("panel.open".into(), bead::core::Value::Bool(false))]);
        if name == "wallpaper" {
            restored.insert(
                "extras.kind".into(),
                bead::core::Value::Text("wallpaper".into()),
            );
        }
        runtime.session.restore_on_start(restored);
        runtime = match name {
            "network" => runtime
                .with_core_network_status(fake_source(core_network()))
                .expect("network projector"),
            "bluetooth" => runtime
                .with_core_bluetooth_status(fake_source(core_bluetooth()))
                .expect("Bluetooth projector"),
            "battery" => runtime
                .with_core_battery_status(fake_source(core_battery()))
                .expect("battery projector"),
            "sound" => runtime
                .with_core_audio_status(fake_source(core_audio()), None)
                .expect("sound projector"),
            "sound-core" => runtime
                .with_view_edge("top")
                .expect("sound edge")
                .with_panel_geometry(382., 286., 0.23)
                .expect("sound geometry")
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .expect("sound frame")
                .with_core_audio_status(fake_source(core_audio()), None)
                .expect("core sound projector"),
            "control" => runtime
                .with_control_center(crate::control_center::ControlCenter::new(
                    None,
                    Vec::new(),
                    None,
                    Vec::new(),
                ))
                .expect("control-center projector"),
            _ => runtime,
        };
        if name == "network" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&BTreeMap::from([
                    ("network.wifi_enabled".into(), bead::core::Value::Bool(true)),
                    ("network.wifi_checked".into(), bead::core::Value::Bool(true)),
                    ("network.rows".into(), bead::core::Value::list(Vec::new())),
                    (
                        "network.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                    (
                        "network.wifi_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "network.link_visible".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "network.link_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    ("network.scroll".into(), bead::core::Value::Number(0.)),
                    ("network.can_hidden".into(), bead::core::Value::Bool(false)),
                    ("network.can_forget".into(), bead::core::Value::Bool(false)),
                    ("network.saved".into(), bead::core::Value::list(Vec::new())),
                    (
                        "network.scan_enabled".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "network.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                ]))
                .expect("network route state");
        }
        if name == "bluetooth" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&BTreeMap::from([
                    (
                        "bluetooth.power_visible".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "bluetooth.power_checked".into(),
                        bead::core::Value::Bool(true),
                    ),
                    (
                        "bluetooth.power_enabled".into(),
                        bead::core::Value::Bool(true),
                    ),
                    (
                        "bluetooth.power_busy".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "bluetooth.power_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.devices".into(),
                        bead::core::Value::list(Vec::new()),
                    ),
                    (
                        "bluetooth.list_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.scan_label".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.scan_enabled".into(),
                        bead::core::Value::Bool(false),
                    ),
                    ("bluetooth.scanning".into(), bead::core::Value::Bool(false)),
                    (
                        "bluetooth.empty_text".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                    (
                        "bluetooth.pair_title".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_kind".into(),
                        bead::core::Value::Text("waiting".into()),
                    ),
                    (
                        "bluetooth.pair_text".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_countdown".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_error".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    ("bluetooth.pair_busy".into(), bead::core::Value::Bool(false)),
                    (
                        "bluetooth.pair_retry".into(),
                        bead::core::Value::Bool(false),
                    ),
                ]))
                .expect("Bluetooth route state");
        }
        if name == "sound" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&crate::desktop::values(
                    &bead::services::Snapshot::default(),
                    "",
                ))
                .expect("desktop audio view state");
        }
        runtime.viewport(1920, 1200);
        runtime.services(Box::new(|_| {}));

        for (event, open) in [
            (UiEvent::Reload, None),
            (UiEvent::PanelVisibility(true), Some(true)),
            (UiEvent::PanelVisibility(false), Some(false)),
        ] {
            let action = match open {
                None => "reload",
                Some(true) => "open",
                Some(false) => "close",
            };
            let mut scene = runtime
                .stage(event)
                .unwrap_or_else(|error| panic!("{name}/{action}: stage failed: {error}"))
                .unwrap_or_else(|| panic!("{name}/{action}: stage produced no scene"));
            assert!(
                !scene.primitives.is_empty(),
                "{name}: stage produced no prepared scene"
            );
            if action == "open" {
                scene.reveal(0.);
                startup_scenes.push((name, scene.clone()));
                let warm_bytes = bead::render::tree::TreeCpuRenderer::prewarm_content(&scene, 1.5)
                    .unwrap_or_else(|error| panic!("{name}: CPU content prewarm failed: {error}"));
                eprintln!("PANEL_SMOKE_WARM route={name} bytes={warm_bytes}");
                if warm_bytes > 0 {
                    let (hits_before, _, _) = bead::render::tree::cpu_content_warm_cache_stats();
                    let mut renderer = bead::render::renderer(bead::render::RendererKind::Cpu)
                        .unwrap_or_else(|error| {
                            panic!("{name}: CPU renderer creation failed: {error}")
                        });
                    renderer
                        .prepare_on_ui(&scene, 1.5)
                        .unwrap_or_else(|error| panic!("{name}: warm CPU prepare failed: {error}"));
                    assert!(renderer.prepared_frame().is_some());
                    let (hits_after, _, _) = bead::render::tree::cpu_content_warm_cache_stats();
                    assert_eq!(
                        hits_after,
                        hits_before + 1,
                        "{name}: warm pixels were not reused"
                    );
                }
            }
            let mut cache = bead::render::tree::TreeCache::default();
            cache
                .prepare(&scene, 1.5)
                .unwrap_or_else(|error| panic!("{name}: CPU scene preparation failed: {error}"));
            assert!(cache.frame.is_some(), "{name}: CPU frame was not prepared");
            runtime.commit();
            if let Some(open) = open {
                assert_eq!(
                    runtime.session.stores.get("panel.open"),
                    Some(&bead::core::Value::Bool(open)),
                    "{name}: panel.open reflects visibility"
                );
            }
        }
    }

    let mut warmed = 0;
    let mut bytes_added = 0usize;
    let route_count = startup_scenes.len();
    for (name, scene) in startup_scenes {
        let bytes = bead::render::tree::TreeCpuRenderer::prewarm_content(&scene, 1.5)
            .unwrap_or_else(|error| panic!("{name}: startup prewarm failed: {error}"));
        warmed += usize::from(bytes > 0);
        bytes_added = bytes_added.saturating_add(bytes);
    }
    assert_eq!(warmed, route_count, "every real shell route has a startup warm entry");
    let (_, _, retained) = bead::render::tree::cpu_content_warm_cache_stats();
    assert!(retained <= 2 * bead::core::cache::MIB);
    eprintln!(
        "PANEL_SMOKE_STARTUP_PREWARM routes={route_count} warmed={warmed} bytes_added={bytes_added} cache_bytes={retained}"
    );
}

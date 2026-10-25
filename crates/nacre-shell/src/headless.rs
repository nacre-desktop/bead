// SPDX-License-Identifier: GPL-3.0-only
use std::path::Path;

#[cfg(test)]
#[path = "launcher_pixels_tests.rs"]
mod launcher_pixels_tests;
#[cfg(test)]
#[path = "service_views_tests.rs"]
mod service_views_tests;
#[cfg(test)]
#[path = "status_views_tests.rs"]
mod status_views_tests;
pub fn check(file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = file.canonicalize()?;
    let root = file
        .ancestors()
        .skip(1)
        .find(|p| p.join("palette.json").is_file())
        .ok_or("UI bundle needs ancestor palette.json")?;
    let palette_path = root.join("palette.json");
    let fixture = initial_models()?;
    let mut harness = bead_test::Harness::load(
        root,
        file.strip_prefix(root)?,
        &palette_path,
        fixture,
        bead_test::Config {
            width: 1280,
            height: 900,
            scale: 1.,
        },
    )?;
    harness.render()?;
    println!("CHECK_OK {}", harness.session.resource_counts());
    Ok(())
}
fn initial_models()
-> Result<std::collections::BTreeMap<String, bead::core::Value>, Box<dyn std::error::Error>> {
    crate::i18n::init();
    let mut session = bead::core::Stores::default();
    session = session.reconcile(&crate::notifications::values(&serde_json::Value::Null))?;
    session = session.reconcile(&crate::desktop::values(&Default::default(), ""))?;
    session = session.reconcile(&std::collections::BTreeMap::from([
        ("bead.workspaces".into(), bead::core::Value::list(vec![])),
        ("bead.windows".into(), bead::core::Value::list(vec![])),
        (
            "bead.focused_title".into(),
            bead::core::Value::Text(String::new()),
        ),
    ]))?;
    Ok(session.values().clone())
}
#[cfg(test)]
mod tests {
    use super::*;
    use bead::core::Value;
    use std::path::PathBuf;
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell")
    }
    #[test]
    fn shell_views_use_headless_harness() -> Result<(), Box<dyn std::error::Error>> {
        for entry in [
            "ui/main.luau",
            "bar/main.luau",
            "notifications/main.luau",
            "osd/main.luau",
        ] {
            check(&root().join(entry))?;
        }
        Ok(())
    }
    #[test]
    fn reconnecting_mark_is_conditional_and_does_not_capture_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut h = bead_test::Harness::load(
            &root(),
            Path::new("bar/main.luau"),
            &root().join("palette.json"),
            initial_models()?,
            bead_test::Config {
                width: 1344,
                height: 114,
                scale: 1.,
            },
        )?;
        let has_mark = |h: &bead_test::Harness| {
            h.scene().primitives.iter().any(
                |p| matches!(p, bead::render::Primitive::Text{text,..} if text == "Reconnecting"),
            )
        };
        assert!(!has_mark(&h));
        h.update(std::collections::BTreeMap::from([(
            "bar.reconnecting".into(),
            Value::Bool(true),
        )]))?;
        assert!(has_mark(&h));
        assert!(
            !h.session
                .view_nodes()
                .iter()
                .any(|node| node.id.ends_with("reconnecting"))
        );
        h.update(std::collections::BTreeMap::from([(
            "bar.reconnecting".into(),
            Value::Bool(false),
        )]))?;
        assert!(!has_mark(&h));
        Ok(())
    }
    #[test]
    fn notification_osd_populated_goldens_and_pose_counter()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = serde_json::json!({"rows":[{"id":"1","app":"Example","summary":"Foundation ready","body":"Private fixture","time_str":"now","expanded":true,"urgency":1,"actions":[{"id":"1|open","label":"Open"}]}],"popup_count":1,"history_count":1,"history_ready":true,"dnd":false,"inhibited":false,"error":"","timers":[],"osd":{"channel":"volume","value":0.75,"muted":false}});
        for (name, entry, config) in [
            (
                "notification",
                "notifications/main.luau",
                bead_test::Config {
                    width: 464,
                    height: 400,
                    scale: 1.,
                },
            ),
            (
                "osd",
                "osd/main.luau",
                bead_test::Config {
                    width: 404,
                    height: 144,
                    scale: 1.,
                },
            ),
        ] {
            let mut fixture = initial_models()?;
            fixture.extend(crate::notifications::values(&state));
            fixture.insert("panel.open".into(), Value::Bool(true));
            let mut harness = bead_test::Harness::load_with_icons(
                &root(),
                Path::new(entry),
                &root().join("palette.json"),
                fixture,
                config,
                std::collections::BTreeMap::from([(
                    "dialog-information".into(),
                    root().join("tests/icon.svg"),
                )]),
            )?;
            let frame = harness.render()?;
            if name == "osd" {
                let scene = harness.scene();
                let mut label_right = None;
                let mut status_left = None;
                for primitive in &scene.primitives {
                    if let bead::render::Primitive::Text { rect, text, .. } = primitive {
                        assert!(
                            rect.x >= scene.bounds.x
                                && rect.x + rect.w <= scene.bounds.x + scene.bounds.w,
                            "OSD text must fit within its panel"
                        );
                        if text == "Volume" {
                            label_right = Some(rect.x + rect.w);
                        }
                        if text == "75%" {
                            status_left = Some(rect.x);
                        }
                    }
                }
                assert!(
                    label_right
                        .zip(status_left)
                        .is_some_and(|(right, left)| right <= left),
                    "OSD label and status must occupy separate space"
                );
            }
            assert!(
                frame
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[3] > 0)
                    .count()
                    > 4000,
                "populated golden must not be blank"
            );
            let golden = root().join("tests/goldens").join(format!("{name}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                std::fs::create_dir_all(golden.parent().ok_or("golden parent")?)?;
                frame.save(&golden)?;
            }
            let artifacts = std::env::var_os("BEAD_TEST_ARTIFACTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("bead-shell-goldens"))
                .join(name);
            frame.compare(&golden, bead_test::Tolerance::default(), &artifacts)?;
            let offset = harness.scene().offset;
            harness.pose(offset, 0.99)?;
            let before = harness.counters();
            for i in 0..30 {
                harness.pose((offset.0, offset.1 + i as f32 * 0.2), 0.5)?;
            }
            assert_eq!(harness.counters(), before);
        }
        Ok(())
    }
    /// B0051: one golden and the visible texts per `osd` event kind (glyph, title, value, detail).
    #[test]
    fn osd_kinds_show_their_glyph_title_value_and_detail() -> Result<(), Box<dyn std::error::Error>>
    {
        // (name, kind, value, muted, label, device, texts that must be drawn)
        #[allow(clippy::type_complexity)]
        let cases: [(&str, &str, f64, bool, &str, &str, &[&str]); 11] = [
            (
                "volume",
                "volume",
                0.5,
                false,
                "Speakers",
                "alsa_output.x",
                &["volume_up", "Volume", "50%", "Speakers"],
            ),
            (
                "volume-low",
                "volume",
                0.2,
                false,
                "",
                "",
                &["volume_down", "20%", "Default output"],
            ),
            (
                "volume-muted",
                "volume",
                0.5,
                true,
                "Speakers",
                "",
                &["volume_off", "Muted"],
            ),
            (
                "volume-clamped",
                "volume",
                1.4,
                false,
                "Speakers",
                "",
                &["volume_up", "100%"],
            ),
            (
                "output",
                "output",
                0.3,
                false,
                "Headphones",
                "bluez_output.1",
                &["headphones", "Audio output", "Headphones"],
            ),
            (
                "microphone",
                "microphone",
                0.0,
                true,
                "Built-in microphone",
                "",
                &["mic_off", "Microphone", "Muted", "Built-in microphone"],
            ),
            (
                "brightness",
                "brightness",
                0.75,
                false,
                "",
                "eDP-1",
                &["brightness_high", "Display brightness", "75%"],
            ),
            (
                "keyboard",
                "keyboard",
                0.5,
                false,
                "",
                "",
                &[
                    "keyboard",
                    "Keyboard brightness",
                    "50%",
                    "Keyboard backlight",
                ],
            ),
            (
                "night-light",
                "night_light",
                1.,
                false,
                "",
                "",
                &["bedtime", "Night light", "On"],
            ),
            (
                "power-profile",
                "power_profile",
                0.,
                false,
                "power-saver",
                "",
                &["eco", "Power mode", "Power saver"],
            ),
            (
                "capslock",
                "capslock",
                0.,
                false,
                "",
                "",
                &["keyboard_capslock", "Caps Lock", "Off"],
            ),
        ];
        for (name, kind, value, muted, label, device, texts) in cases {
            let mut fixture = initial_models()?;
            fixture.insert("panel.open".into(), Value::Bool(true));
            for (key, v) in [
                ("osd.open", Value::Bool(true)),
                ("osd.kind", Value::Text(kind.into())),
                ("osd.value", Value::Number(value)),
                ("osd.muted", Value::Bool(muted)),
                ("osd.label", Value::Text(label.into())),
                ("osd.device", Value::Text(device.into())),
            ] {
                fixture.insert(key.into(), v);
            }
            let mut harness = bead_test::Harness::load(
                &root(),
                Path::new("osd/main.luau"),
                &root().join("palette.json"),
                fixture,
                bead_test::Config {
                    width: 404,
                    height: 144,
                    scale: 1.,
                },
            )?;
            let frame = harness.render()?;
            let drawn: Vec<String> = harness
                .scene()
                .primitives
                .iter()
                .filter_map(|p| match p {
                    bead::render::Primitive::Text { text, .. } => Some(text.to_string()),
                    _ => None,
                })
                .collect();
            for want in texts {
                assert!(
                    drawn.iter().any(|t| t == want),
                    "{name}: {want} in {drawn:?}"
                );
            }
            let golden = root()
                .join("tests/goldens")
                .join(format!("osd-{name}-1.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                bead_test::Tolerance::default(),
                &std::env::temp_dir().join(format!("bead-osd-{name}-diff")),
            )?;
        }
        Ok(())
    }
    /// Manual renderer CPU comparison; excluded from CI timing assertions.
    #[test]
    #[ignore = "manual release CPU benchmark; no compositor needed"]
    fn opaque_composition_measurement() -> Result<(), Box<dyn std::error::Error>> {
        use bead::render::{
            PixelFormat, Renderer, ShmTarget,
            tree::{OpaqueComposition, TreeCpuRenderer},
        };
        for entry in ["bar/main.luau", "sound/main.luau"] {
            let mut fixture = initial_models()?;
            fixture.insert("panel.open".into(), Value::Bool(true));
            fixture.insert("bead.workspaces".into(),Value::list((1..=6).map(|i|serde_json::json!({"id":i.to_string(),"name":i.to_string(),"index":i,"active":i==1,"urgent":false,"mru":false})).collect()));
            fixture.insert(
                "bead.desktop.audio.device".into(),
                Value::Text("Private sound output".into()),
            );
            fixture.insert("bead.desktop.audio.label".into(), Value::Text("75%".into()));
            fixture.insert("bead.desktop.audio.percent".into(), Value::Number(75.));
            // Contents-sized surfaces on a 2880×1800 output at scale1.5.
            let (logical_w, logical_h) = if entry.starts_with("bar/") {
                (1920, 50)
            } else {
                (464, 350)
            };
            let (width, height) = (logical_w * 3 / 2, logical_h * 3 / 2);
            let harness = bead_test::Harness::load(
                &root(),
                Path::new(entry),
                &root().join("palette.json"),
                fixture,
                bead_test::Config {
                    width: logical_w,
                    height: logical_h,
                    scale: 1.5,
                },
            )?;
            let mut scene = harness.scene().clone();
            scene.opacity = 1.;
            if entry.starts_with("bar/") {
                // Match Runtime::propose: bars have no generic panel shadow inset.
                scene.offset = (0., 0.);
                scene.width = logical_w as f32;
                scene.bounds = bead::render::Rect::new(0., 0., logical_w as f32, logical_h as f32);
            }
            let origin = scene.offset;
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            for repeat in 0..5 {
                let modes = if repeat % 2 == 0 {
                    [OpaqueComposition::Direct, OpaqueComposition::Cached]
                } else {
                    [OpaqueComposition::Cached, OpaqueComposition::Direct]
                };
                for mode in modes {
                    let mut renderer = TreeCpuRenderer::with_opaque_composition(mode);
                    renderer.prepare(&scene, 1.5)?;
                    let mut samples = Vec::with_capacity(240);
                    for i in 0..272 {
                        let previous = scene.bounds();
                        let delta = (i % 24) as f32 / 1.5;
                        scene.offset = if entry.starts_with("bar/") {
                            (origin.0, origin.1 - delta)
                        } else {
                            (origin.0 + delta, origin.1)
                        };
                        let damage = previous.union(scene.bounds()).pixels(1.5, width, height);
                        let began = bead::core::profile::process_cpu_ns();
                        renderer.render(
                            &scene,
                            ShmTarget {
                                bytes: &mut pixels,
                                width,
                                height,
                                scale: 1.5,
                                format: PixelFormat::Rgba,
                            },
                            damage,
                        )?;
                        if i >= 32 {
                            samples
                                .push((bead::core::profile::process_cpu_ns() - began) as f64 / 1e6);
                        }
                    }
                    samples.sort_by(f64::total_cmp);
                    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
                    println!(
                        "OPAQUE_COMPOSITION entry={entry} repeat={repeat} mode={mode:?} cpu_mean_ms={mean:.6} cpu_p50_ms={:.6} cpu_p99_ms={:.6} direct={} groups={} bounds={}x{} target={width}x{height} monitor=2880x1800 scale=1.5",
                        samples[samples.len() / 2],
                        samples[(samples.len() * 99 / 100).min(samples.len() - 1)],
                        renderer.direct_composites,
                        renderer.opacity_builds,
                        scene.bounds.w,
                        scene.bounds.h
                    );
                    assert_eq!(
                        renderer.opacity_builds,
                        usize::from(mode == OpaqueComposition::Cached)
                    );
                    assert_eq!(
                        renderer.direct_composites,
                        if mode == OpaqueComposition::Direct {
                            272
                        } else {
                            0
                        }
                    );
                }
            }
        }
        Ok(())
    }
    #[test]
    fn launcher_source_fixtures_cover_grid_results_and_prefixes()
    -> Result<(), Box<dyn std::error::Error>> {
        use bead::core::Value;
        use serde_json::json;
        let apps: Vec<_>=(0..320).map(|i|json!({"id":format!("app-{i:04}"),"kind":"app","label":format!("Application {i:04}"),"icon":"fixture","description":"Application fixture"})).collect();
        let mut data = json!({"queries":{
            "":{"rows":apps},"a":{"rows":apps[..160]},"ap":{"rows":apps[160..]},
            "=1+1":{"rows":[{"id":"calc-2","kind":"calculator","label":"2","description":"1 + 1"}]},
            "emoji smile":{"rows":[{"id":"emoji-smile","kind":"emoji","label":"Smiling face","emoji":"☺"}]},
            "clipboard item":{"rows":[{"id":"clip-1","kind":"clipboard","label":"Copied fixture"}]},
            ": fixture":{"rows":[{"id":"file-1","kind":"file","label":"Fixture document"}]},
            "# fixture":{"rows":[{"id":"brain-1","kind":"brain","label":"Fixture note"}]},
            "> fixture":{"rows":[{"id":"action-1","kind":"action","label":"Fixture action"}]}
        }});
        for (query, reply) in data["queries"].as_object_mut().unwrap() {
            let rows = reply["rows"].take().as_array().unwrap().clone();
            *reply = crate::launcher::fixture_reply(query, rows);
        }
        let source = std::sync::Arc::new(crate::launcher::Fixture::new(data)?);
        let menu = std::rc::Rc::new(crate::launcher::Menu::new(Some(source))?);
        let mut h = bead_test::Harness::load_with_icons(
            &root(),
            Path::new("ui/main.luau"),
            &root().join("palette.json"),
            initial_models()?,
            bead_test::Config {
                width: 1164,
                height: 654,
                scale: 1.5,
            },
            std::collections::BTreeMap::from([
                ("fixture".into(), root().join("tests/icon.svg")),
                (
                    "application-x-executable".into(),
                    root().join("tests/icon.svg"),
                ),
            ]),
        )?;
        let projector = menu.clone();
        h.session
            .set_projector(Box::new(move |stores| projector.project(stores)));
        let wait_for_launcher =
            |h: &mut bead_test::Harness| -> Result<(), Box<dyn std::error::Error>> {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                loop {
                    h.update(std::collections::BTreeMap::new())?;
                    if h.session.stores.get("launcher.pending") == Some(&Value::Bool(false)) {
                        return Ok(());
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err("launcher query did not settle".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            };
        h.reload()?;
        wait_for_launcher(&mut h)?;
        h.render()?;
        h.assert_accessible("Application 0000", accesskit::Role::Button)?;
        for (query, label) in [
            ("a", "Application 0000"),
            ("ap", "Application 0160"),
            ("a", "Application 0000"),
            ("=1+1", "2"),
            ("emoji smile", "Smiling face"),
            ("clipboard item", "Copied fixture"),
            (": fixture", "Fixture document"),
            ("# fixture", "Fixture note"),
            ("> fixture", "Fixture action"),
        ] {
            h.update(std::collections::BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(query.into()),
            )]))?;
            wait_for_launcher(&mut h)?;
            h.render()?;
            h.assert_accessible(label, accesskit::Role::Button)?;
            let visible_rows = h
                .session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("select_app"))
                .count();
            assert!(
                visible_rows <= 12,
                "320-row catalogue should materialize only viewport rows plus overscan, got {visible_rows}"
            );
        }
        h.reload()?;
        h.assert_accessible("Fixture action", accesskit::Role::Button)?;
        assert!(
            menu.activate("action-1").is_err(),
            "fixture must never invoke external actions"
        );
        h.update(std::collections::BTreeMap::from([(
            "launcher.search.text".into(),
            Value::Text(String::new()),
        )]))?;
        wait_for_launcher(&mut h)?;
        h.render()?;
        h.assert_accessible("Application 0000", accesskit::Role::Button)?;
        assert!(matches!(
            h.session.stores.get("launcher.result_count"),
            Some(Value::Number(count)) if *count == 320.
        ));
        Ok(())
    }

    #[test]
    fn launcher_conversion_and_provider_guidance_have_populated_goldens()
    -> Result<(), Box<dyn std::error::Error>> {
        for (name, key, value) in [
            (
                "launcher-conversion",
                "launcher.conversion.value",
                "10 km = 6.213711922 mi",
            ),
            (
                "launcher-guidance",
                "launcher.message",
                "Install qalc for unit conversion",
            ),
            (
                "launcher-conversion-error",
                "launcher.conversion.error",
                "Conversion is unavailable",
            ),
        ] {
            let mut fixture = initial_models()?;
            fixture.insert(
                "launcher.search.text".into(),
                Value::Text("= 10 km to miles".into()),
            );
            fixture.insert("launcher.groups".into(), crate::launcher::fixture_groups());
            fixture.insert(key.into(), Value::Text(value.into()));
            let mut h = bead_test::Harness::load(
                &root(),
                Path::new("ui/main.luau"),
                &root().join("palette.json"),
                fixture,
                bead_test::Config {
                    width: 1164,
                    height: 654,
                    scale: 1.,
                },
            )?;
            let frame = h.render()?;
            assert!(
                h.scene()
                    .primitives
                    .iter()
                    .any(|p| matches!(p,bead::render::Primitive::Text{text,..}if text==value))
            );
            assert!(
                !h.scene().primitives.iter().any(
                    |p| matches!(p,bead::render::Primitive::Text{text,..}if text=="No results")
                )
            );
            assert!(
                h.accessibility()
                    .nodes
                    .iter()
                    .any(|(_, n)| n.value() == Some("= 10 km to miles"))
            );
            let golden = root().join("tests/goldens").join(format!("{name}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            let artifacts = std::env::temp_dir()
                .join("bead-launcher-detail-goldens")
                .join(name);
            frame.compare(&golden, bead_test::Tolerance::default(), &artifacts)?;
        }
        Ok(())
    }

    #[test]
    fn notification_stream_windows_variable_height_cards_and_reaches_both_edges()
    -> Result<(), Box<dyn std::error::Error>> {
        let rows: Vec<_> = (0..120)
            .map(|i| {
                serde_json::json!({
                    "id":i.to_string(), "app":"Example", "summary":format!("Notice {i}"),
                    "body":if i % 2 == 0 { "Details\nSecond line" } else { "" },
                    "time_str":"now", "expanded":i % 3 == 0, "urgency":1, "actions":[]
                })
            })
            .collect();
        let mut fixture = initial_models()?;
        fixture.extend(crate::notifications::values(
            &serde_json::json!({"rows":rows,
            "popup_count":120,"history_count":120,"dnd":false,"inhibited":false,"timers":[]}),
        ));
        let mut h = bead_test::Harness::load_with_icons(
            &root(),
            Path::new("notifications/main.luau"),
            &root().join("palette.json"),
            fixture,
            bead_test::Config {
                width: 464,
                height: 784,
                scale: 1.,
            },
            std::collections::BTreeMap::from([(
                "dialog-information".into(),
                root().join("tests/icon.svg"),
            )]),
        )?;
        for (name, delta, expected) in [
            ("top", 0., "Notice 0"),
            ("bottom", 1_000_000., "Notice 119"),
            ("reversed", -1_000_000., "Notice 0"),
        ] {
            if delta != 0. {
                h.script([bead_test::Input::Scroll {
                    x: 200.,
                    y: 350.,
                    delta,
                }])?;
            }
            let scene = h.scene();
            let visible = scene.primitives.iter().filter(|p| matches!(p, bead::render::Primitive::Text {text,..} if text.starts_with("Notice "))).count();
            assert!(
                visible < 12,
                "only viewport cards plus overscan should materialize: {visible}"
            );
            assert!(
                scene.primitives.iter().any(
                    |p| matches!(p, bead::render::Primitive::Text {text,..} if text==expected)
                )
            );
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("notification-scroll-{name}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                bead_test::Tolerance::default(),
                &std::env::temp_dir().join(format!("notification-scroll-{name}")),
            )?;
        }
        Ok(())
    }

    #[test]
    fn notification_controls_follow_current_qml_visibility()
    -> Result<(), Box<dyn std::error::Error>> {
        for (body, actions, details) in [
            (
                "",
                serde_json::json!([{"id":"1|default","label":"Implicit default"}]),
                false,
            ),
            (
                "Details",
                serde_json::json!([{"id":"1|default","label":"Implicit default"},{"id":"1|open","label":"Open"}]),
                true,
            ),
        ] {
            let snapshot = serde_json::json!({"rows":[{"id":"1","app":"Example","summary":"Controls","body":body,"time_str":"now","expanded":true,"urgency":1,"actions":actions}],"popup_count":1,"history_count":1,"dnd":false,"inhibited":false,"timers":[]});
            let mut fixture = initial_models()?;
            fixture.extend(crate::notifications::values(&snapshot));
            let mut h = bead_test::Harness::load_with_icons(
                &root(),
                Path::new("notifications/main.luau"),
                &root().join("palette.json"),
                fixture,
                bead_test::Config {
                    width: 464,
                    height: 400,
                    scale: 1.,
                },
                std::collections::BTreeMap::from([(
                    "dialog-information".into(),
                    root().join("tests/icon.svg"),
                )]),
            )?;
            h.render()?;
            h.assert_accessible("Dismiss notification", accesskit::Role::Button)?;
            let tree = h.accessibility();
            assert!(
                !tree
                    .nodes
                    .iter()
                    .any(|(_, n)| n.label() == Some("Implicit default")),
                "default action is not a footer button"
            );
            assert_eq!(
                tree.nodes
                    .iter()
                    .any(|(_, n)| n.label() == Some("Collapse notification")),
                details
            );
            if details {
                h.assert_accessible("Open", accesskit::Role::Button)?;
            }
        }
        Ok(())
    }
}

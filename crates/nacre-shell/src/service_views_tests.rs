// SPDX-License-Identifier: GPL-3.0-only
//! Explicit date fixtures; no compositor, wall clock or hardware service.
use bead::core::Value;
use bead_test::{Config, Harness, Input, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

#[test]
fn idle_bar_minute_model_changes_damage_the_real_clock_widget() -> Result<()> {
    use bead::render::{damage::FrameDamage, tree::TreeCache};
    for scale in [1., 1.5] {
        let mut values = super::initial_models()?;
        values.insert("bead.desktop.clock".into(), Value::Text("10:11".into()));
        let h = Harness::load(
            &root(),
            Path::new("bar/main.luau"),
            &root().join("palette.json"),
            values,
            Config {
                width: 1920,
                height: 70,
                scale,
            },
        )?;
        let mut cache = TreeCache::default();
        let mut damage = FrameDamage::default();
        let width = (1920. * scale) as u32;
        let height = (70. * scale) as u32;
        cache.prepare(h.scene(), scale)?;
        damage.update(cache.frame.as_ref().unwrap(), (0., 0.), 1., width, height)?;
        let mut session = h.session;
        for minute in ["10:12", "10:13", "11:14", "09:15"] {
            let scene = session
                .update_model(BTreeMap::from([(
                    "bead.desktop.clock".into(),
                    Value::Text(minute.into()),
                )]))?
                .ok_or("idle clock update was filtered out")?;
            assert!(scene.primitives.iter().any(|p| matches!(p,
                bead::render::Primitive::Text {text,..} if text == minute)));
            cache.prepare(&scene, scale)?;
            let regions =
                damage.update(cache.frame.as_ref().unwrap(), (0., 0.), 1., width, height)?;
            assert!(!regions.is_empty(), "changed minute needs surface damage");
            session.commit(scene);
            assert!(
                session
                    .update_model(BTreeMap::from([(
                        "bead.desktop.clock".into(),
                        Value::Text(minute.into())
                    ),]))?
                    .is_none(),
                "unchanged minute must stay idle"
            );
        }
    }
    Ok(())
}

fn settle_harness_animations(harness: &mut Harness) -> Result<usize> {
    let mut frames = 0;
    while let Some(deadline) = harness.session.preview_deadline() {
        if frames >= 64 {
            return Err("harness animations did not settle within 64 frames".into());
        }
        let now = std::time::Instant::now();
        if deadline > now {
            std::thread::sleep(deadline.duration_since(now));
        }
        harness.update(BTreeMap::new())?;
        frames += 1;
    }
    Ok(frames)
}
fn calendar(year: u16, month: u8, first: u8, scale: f32) -> Result<Harness> {
    Harness::load(
        &root(),
        Path::new("popouts/main.luau"),
        &root().join("palette.json"),
        BTreeMap::from([
            ("calendar.ready".into(), Value::Bool(true)),
            ("calendar.year".into(), Value::Number(year.into())),
            ("calendar.month".into(), Value::Number(month.into())),
            ("calendar.first_weekday".into(), Value::Number(first.into())),
            ("calendar.today".into(), Value::Text("2024-02-29".into())),
        ]),
        Config {
            width: 380,
            height: 380,
            scale,
        },
    )
}
fn dates(h: &Harness) -> BTreeMap<String, String> {
    h.scene()
        .layers
        .iter()
        .filter_map(|layer| {
            let (parent, tail) = layer.id.split_once("dates.k")?;
            let hex = tail.split('.').next()?;
            let bytes = hex
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                .collect::<Option<Vec<_>>>()?;
            let date = String::from_utf8(bytes).ok()?;
            Some((date, format!("{parent}dates.k{hex}")))
        })
        .collect()
}
fn month(h: &mut Harness, label: &str) -> Result<()> {
    let node = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.label == label)
        .ok_or("month button missing")?;
    let r = bead::ui::rect(&node.draw);
    h.script([Input::Click {
        x: r.x + r.w / 2.,
        y: r.y + r.h / 2.,
    }])
}
#[test]
fn calendar_fixture_pixels_and_navigation_keep_stable_date_ids() -> Result<()> {
    for scale in [1., 1.5] {
        let mut h = calendar(2024, 2, 1, scale)?;
        let before = dates(&h);
        assert_eq!(before.len(), 42);
        assert_eq!(
            before.keys().filter(|k| k.starts_with("2024-02-")).count(),
            29
        );
        assert!(before.contains_key("2024-01-29") && before.contains_key("2024-03-10"));
        assert_eq!(
            h.session.view_nodes().len(),
            2,
            "dates are static; only month buttons act"
        );
        let frame = h.render()?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("calendar-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        let artifacts = std::env::temp_dir().join(format!("bead-calendar-golden-{scale}"));
        frame.compare(&golden, bead_test::Tolerance::default(), &artifacts)?;
        month(&mut h, "Next month")?;
        assert_eq!(
            h.session.stores.get("calendar.month"),
            Some(&Value::Number(3.))
        );
        assert_eq!(dates(&h).get("2024-02-29"), before.get("2024-02-29"));
        month(&mut h, "Previous month")?;
        assert_eq!(dates(&h), before);
    }
    Ok(())
}
#[test]
fn calendar_century_leaps_week_start_and_year_bounds() -> Result<()> {
    for (year, days, first) in [(1900, 28, 0), (2000, 29, 1), (2100, 28, 1), (2400, 29, 0)] {
        let h = calendar(year, 2, first, 1.)?;
        let prefix = format!("{year}-02-");
        assert_eq!(
            dates(&h).keys().filter(|k| k.starts_with(&prefix)).count(),
            days
        );
    }
    let sunday = calendar(2024, 2, 0, 1.)?;
    assert_eq!(
        dates(&sunday).keys().next().map(String::as_str),
        Some("2024-01-28")
    );
    let mut lower = calendar(1, 1, 1, 1.)?;
    month(&mut lower, "Previous month")?;
    assert_eq!(
        lower.session.stores.get("calendar.year"),
        Some(&Value::Number(1.))
    );
    assert_eq!(
        lower.session.stores.get("calendar.month"),
        Some(&Value::Number(1.))
    );
    let mut upper = calendar(9999, 12, 0, 1.)?;
    month(&mut upper, "Next month")?;
    assert_eq!(
        upper.session.stores.get("calendar.year"),
        Some(&Value::Number(9999.))
    );
    assert_eq!(
        upper.session.stores.get("calendar.month"),
        Some(&Value::Number(12.))
    );
    Ok(())
}
#[test]
fn calendar_missing_source_is_unavailable_with_no_navigation() -> Result<()> {
    let h = Harness::load(
        &root(),
        Path::new("popouts/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 380,
            height: 380,
            scale: 1.,
        },
    )?;
    assert!(h.session.view_nodes().is_empty());
    assert!(dates(&h).is_empty());
    Ok(())
}

#[test]
fn calendar_today_changes_do_not_reset_the_browsed_month_or_date_identity() -> Result<()> {
    let mut h = calendar(2024, 2, 1, 1.5)?;
    let before = dates(&h);
    let pixels = h.render()?.pixels;
    h.update(BTreeMap::from([(
        "calendar.today".into(),
        Value::Text("2024-03-01".into()),
    )]))?;
    assert_eq!(dates(&h), before);
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(2.))
    );
    assert_ne!(
        h.render()?.pixels,
        pixels,
        "today highlight must follow the supplied date"
    );
    month(&mut h, "Previous month")?;
    h.update(BTreeMap::from([(
        "calendar.today".into(),
        Value::Text("2024-03-02".into()),
    )]))?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(1.))
    );
    Ok(())
}

#[test]
fn calendar_source_recovers_without_overwriting_a_browsed_month() -> Result<()> {
    use bead::core::source::Source;
    use bead_test::source::FakeSource;
    use std::{rc::Rc, sync::Arc};
    let source = Arc::new(FakeSource::new(
        serde_json::json!({"date":{"year":2024,"month":2,"day":29}}),
    )?);
    let calendar = Rc::new(crate::calendar::Calendar::new(source.clone()));
    let mut h = Harness::load(
        &root(),
        Path::new("popouts/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 380,
            height: 380,
            scale: 1.5,
        },
    )?;
    let projector = calendar.clone();
    h.session
        .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.today"),
        Some(&Value::Text("2024-02-29".into()))
    );
    assert_eq!(dates(&h).len(), 42);
    month(&mut h, "Previous month")?;
    source.publish(serde_json::json!({"date":{"year":2024,"month":3,"day":1}}))?;
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(1.))
    );
    assert_eq!(
        h.session.stores.get("calendar.today"),
        Some(&Value::Text("2024-03-01".into()))
    );
    let before = h.render()?.pixels;
    source.disconnect()?;
    assert!(!source.snapshot().connected);
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.render()?.pixels,
        before,
        "disconnect keeps the last date snapshot"
    );
    source.publish(serde_json::json!({"date":{"year":1900,"month":2,"day":29}}))?;
    h.update(BTreeMap::new())?;
    assert!(
        h.session.view_nodes().is_empty(),
        "invalid source date must not become a usable calendar"
    );
    source.publish(serde_json::json!({"date":{"year":2000,"month":2,"day":29}}))?;
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(1.))
    );
    assert_eq!(
        h.session.stores.get("calendar.year"),
        Some(&Value::Number(2024.))
    );
    assert_eq!(
        source.calls(),
        0,
        "calendar month navigation is view state, not a service mutation"
    );
    Ok(())
}

#[test]
fn real_bar_calendar_action_enqueues_a_view_and_audio_scroll_does_not_toggle_it() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::sync::{Arc, Mutex};
    let mut values = super::initial_models()?;
    values.insert("bar.popouts".into(), Value::Bool(true));
    let h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 70,
            scale: 1.5,
        },
    )?;
    let center = |label: &str| -> Result<(f32, f32)> {
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|node| node.label == label)
            .ok_or("bar action missing")?;
        let r = bead::ui::rect(&node.draw);
        Ok((r.x + r.w / 2., r.y + r.h / 2.))
    };
    let calendar = center("Open calendar")?;
    let audio = center("Open sound panel")?;
    let notifications = center("Open notifications")?;
    let clock = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.label == "Open calendar")
        .ok_or("clock button missing")?;
    let clock = bead::ui::rect(&clock.draw);
    let time = h
        .scene()
        .layers
        .iter()
        .find(|layer| layer.id.ends_with(".clock.time"))
        .ok_or("clock time must remain visible")?;
    let time = h.scene().primitives[time.own.clone()]
        .iter()
        .find_map(|p| {
            if let bead::render::Primitive::Text { rect, .. } = p {
                Some(*rect)
            } else {
                None
            }
        })
        .ok_or("clock text geometry missing")?;
    assert!(
        time.y >= clock.y && time.y + time.h <= clock.y + clock.h,
        "time must fit vertically inside the bar clock button"
    );
    let calendar_icon = h
        .scene()
        .layers
        .iter()
        .find(|layer| layer.id.ends_with(".clock.calendar.ink"))
        .and_then(|layer| {
            h.scene().primitives[layer.own.clone()]
                .iter()
                .find_map(|p| {
                    if let bead::render::Primitive::Text { rect, .. } = p {
                        Some(*rect)
                    } else {
                        None
                    }
                })
        })
        .ok_or("calendar icon geometry missing")?;
    assert!(
        time.x + 0.01 >= calendar_icon.x + calendar_icon.w,
        "time must follow the calendar icon horizontally"
    );
    let host = bead::services::Desktop::start_config(bead::services::Config {
        audio: false,
        bus: false,
    });
    let mut runtime = crate::reload::Runtime::new(h.session)
        .with_shared_desktop(host.clone(), true)?
        .with_popout_trigger()?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    runtime.services(Box::new(move |event| output.lock().unwrap().push(event)));
    let regions = runtime.input_regions();
    let prepare = regions
        .iter()
        .find(|r| r.prepare_view.as_deref() == Some("calendar"))
        .ok_or("calendar hover preparation missing")?;
    assert!(prepare.prepare_panel);
    assert_eq!(prepare.prepare_center, Some(calendar.0));
    assert!(
        regions
            .iter()
            .any(|r| r.prepare_view.as_deref() == Some("sound")
                && r.prepare_panel
                && r.prepare_center == Some(audio.0))
    );
    let notification_anchor = regions
        .iter()
        .find(|r| r.prepare_view.as_deref() == Some("notifications"))
        .and_then(|r| r.prepare_center)
        .ok_or("notification hover anchor missing")?;
    assert_eq!(
        notification_anchor, notifications.0,
        "anchor uses the combined icon/count hit area, as in QML"
    );
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Click {
        x: calendar.0,
        y: calendar.1,
    })?;
    runtime.commit();
    assert!(
        events.lock().unwrap().is_empty(),
        "the draggable click waits for release"
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    assert!(
        matches!(events.lock().unwrap().as_slice(),[UiEvent::CompanionSelect{view,toggle:false,center:Some(x),..}]if view=="calendar" && *x==calendar.0)
    );
    let error = runtime
        .stage(UiEvent::Scroll {
            x: audio.0,
            y: audio.1,
            delta: 1.,
        })
        .err()
        .ok_or("fixture has no audio endpoint")?;
    assert!(error.to_string().contains("audio unavailable"));
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "scroll must not open or toggle a popout"
    );
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Click {
        x: notifications.0,
        y: notifications.1,
    })?;
    runtime.commit();
    assert!(
        events.lock().unwrap().is_empty(),
        "the draggable click waits for release"
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    let received = events.lock().unwrap();
    assert!(matches!(received.as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, center:Some(center), .. }]
            if view == "notifications" && *center == notification_anchor));
    drop(received);
    host.stop();
    Ok(())
}

#[test]
fn real_bar_equal_width_title_spinner_patches_only_the_text_primitive() -> Result<()> {
    let mut values = super::initial_models()?;
    values.insert("bead.focused_title".into(), Value::Text("◐ …".into()));
    let mut h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 70,
            scale: 1.5,
        },
    )?;
    let before = h.scene().primitives.clone();
    let before_rect = before
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text == "◐ …" => Some(*rect),
            _ => None,
        })
        .ok_or("initial bar title primitive missing")?;
    let node_geometry = |h: &Harness| {
        h.session
            .view_nodes()
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    (bead::ui::rect(&node.draw), node.label.clone()),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before_nodes = node_geometry(&h);
    let before_layouts = h.counters().layouts;
    assert!(
        h.session
            .update_model(BTreeMap::from([(
                "bead.focused_title".into(),
                Value::Text("◐ …".into()),
            )]))?
            .is_none()
    );
    h.update(BTreeMap::from([(
        "bead.focused_title".into(),
        Value::Text("◑ …".into()),
    )]))?;
    assert_eq!(
        h.counters().layouts,
        before_layouts,
        "spinner ran Taffy layout"
    );
    let after = h.scene();
    let after_rect = after
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text == "◑ …" => Some(*rect),
            _ => None,
        })
        .ok_or("updated bar title primitive missing")?;
    assert_eq!(
        after_rect, before_rect,
        "equal-width spinner moved the title"
    );
    assert_eq!(node_geometry(&h), before_nodes);
    let mut expected = before;
    for primitive in &mut expected {
        if let bead::render::Primitive::Text { text, .. } = primitive
            && text == "◐ …"
        {
            *text = "◑ …".into();
        }
    }
    assert_eq!(
        serde_json::to_value(&after.primitives)?,
        serde_json::to_value(&expected)?
    );
    assert!(before_nodes.keys().any(|id| id.ends_with(".title-band")));
    Ok(())
}

#[test]
fn bar_counts_share_icon_hit_areas_and_centered_line_boxes() -> Result<()> {
    for scale in [1., 1.5] {
        let mut values = super::initial_models()?;
        values.extend(BTreeMap::from([
            ("bar.popouts".into(), Value::Bool(true)),
            ("bar.launcher".into(), Value::Bool(true)),
            ("bar.updates.count".into(), Value::Number(9.)),
            ("bar.notifications.count".into(), Value::Number(15.)),
            ("bead.desktop.battery.value".into(), Value::Text("56".into())),
            ("bead.desktop.clock".into(), Value::Text("11:02".into())),
            ("bead.desktop.battery.value.width".into(), Value::Number(30.)),
            ("bead.workspaces".into(), Value::list(vec![
                serde_json::json!({"id":"one","index":1,"label":"1","active":true,"hinted":true,"shown":1,"app1":"terminal"}),
                serde_json::json!({"id":"two","index":2,"label":"2","occupied":true}),
                serde_json::json!({"id":"three","index":3,"label":"3"}),
            ])),
        ]));
        let mut h = Harness::load(
            &root(),
            Path::new("bar/main.luau"),
            &root().join("palette.json"),
            values,
            Config {
                width: 1920,
                height: 50,
                scale,
            },
        )?;
        settle_harness_animations(&mut h)?;
        assert!(
            h.session.preview_deadline().is_none(),
            "bar geometry must be at its authored animation endpoint before golden capture"
        );
        // The pill's buttons tile its full width, while their glyphs retain the old inset.
        let mut targets: Vec<_> = h
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.id.contains("system-pill") && node.action.is_some())
            .filter_map(|node| {
                let rect = bead::ui::rect(&node.draw);
                let expected_action = node.action.as_deref()?;
                h.session
                    .action_at(rect.x + rect.w / 2., rect.y + rect.h / 2.)
                    .filter(|(_, action, _)| action == expected_action)
                    .map(|(_, _, argument)| {
                        assert!(
                            !crate::widgets::dropdown_hover(&argument),
                            "click is the default"
                        );
                        (argument, rect)
                    })
            })
            .collect();
        targets.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
        targets.dedup_by(|a, b| a.1.x == b.1.x);
        assert_eq!(targets.len(), 8, "every status widget owns one hit target");
        let pill = h
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id.ends_with("system-pill"))
            .ok_or("status pill layer missing")?;
        let pill_rect = h.scene().primitives[pill.own.clone()]
            .iter()
            .find_map(|p| match p {
                bead::render::Primitive::Rounded { rect, .. } => Some(*rect),
                _ => None,
            })
            .ok_or("status pill background missing")?;
        assert!(
            (targets[0].1.x - pill_rect.x).abs() < 0.01,
            "first hit area starts at pill left edge"
        );
        let last = targets.last().ok_or("status targets missing")?.1;
        assert!(
            (last.x + last.w - (pill_rect.x + pill_rect.w)).abs() < 0.01,
            "last hit area ends at pill right edge"
        );
        {
            for pair in targets.windows(2) {
                assert!(
                    (pair[0].1.x + pair[0].1.w - pair[1].1.x).abs() < 0.01,
                    "pill buttons must not leave a gap: {pair:?}"
                );
                let x = pair[1].1.x;
                assert!(
                    h.session.action_at(x, 25.).is_some(),
                    "the seam between two pill buttons is a hit target"
                );
            }
            assert!(
                h.session.action_at(pill_rect.x - 2., 25.).is_none(),
                "space outside the pill is inert"
            );
        }
        // Old glyph x = old target x + 3 px padding + optical-centering offset.
        // New hit targets start 6 px earlier, with 9 px padding, so this is the
        // same position while the hit rect grows over the old gap/end inset.
        let optical_width = |text: &str| match text {
            "package_2" | "volume_up" | "volume_mute" | "volume_off" | "calendar_month" => {
                Some(44. / 3.)
            }
            "wifi" => Some(16.),
            "wifi_off" => Some(56. / 3.),
            "bluetooth" | "bluetooth_disabled" => Some(10.),
            "battery_5_bar" | "battery_full" | "battery_charging_full" => Some(8.),
            "battery_unknown" => Some(16.),
            "notifications" => Some(40. / 3.),
            "power_settings_new" => Some(52. / 3.),
            _ => None,
        };
        let mut status_glyphs = Vec::new();
        for layer in &h.scene().layers {
            for primitive in &h.scene().primitives[layer.own.clone()] {
                if let bead::render::Primitive::Text {
                    rect: glyph, text, ..
                } = primitive
                    && optical_width(text).is_some()
                {
                    status_glyphs.push((text.clone(), *glyph));
                }
            }
        }
        status_glyphs.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
        assert_eq!(status_glyphs.len(), 8, "every status glyph is present");
        for ((text, glyph), (_, target)) in status_glyphs.iter().zip(&targets) {
            let ink = optical_width(text).expect("recognized status glyph");
            let hit_inset = 9.;
            let expected = target.x + hit_inset + (ink - 24.) / 2.;
            assert!(
                (glyph.x - expected).abs() < 1.,
                "status glyph {text} moved from its pre-648f0dd6 x position: expected {expected}, got {}",
                glyph.x,
            );
        }
        for (suffix, action) in [
            ("updates.count", "status.pill.open"),
            ("battery.battery-value", "status.pill.open"),
            ("notifications.notifications-count", "status.pill.open"),
        ] {
            let layer = h
                .scene()
                .layers
                .iter()
                .find(|l| l.id.ends_with(suffix))
                .ok_or("count layer missing")?;
            let rect = h.scene().primitives[layer.own.clone()]
                .iter()
                .find_map(|p| {
                    if let bead::render::Primitive::Text { rect, .. } = p {
                        Some(*rect)
                    } else {
                        None
                    }
                })
                .ok_or("count text missing")?;
            let (_, name, _) = h
                .session
                .action_at(rect.x + rect.w / 2., rect.y + rect.h / 2.)
                .ok_or("count hit missing")?;
            assert_eq!(name, action);
            assert!(
                (rect.y + rect.h / 2. - 25.).abs() < 0.1,
                "count line box must be centered in pill: {rect:?}"
            );
        }
        assert_eq!(h.session.pointer_cursor(960., 25.), "pointer");
        assert!(h.session.allows_edge_hover(960., 25.));
        let frame = h.pose((0., 0.), 1.)?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("bar-parity-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance::default(),
            &std::env::temp_dir().join(format!("bead-bar-parity-{scale}")),
        )?;
        h.script([Input::Pointer { x: 960., y: 25. }])?;
        assert_eq!(
            h.session.stores.get("bead.hovered"),
            Some(&Value::Text("bar.title-band".into()))
        );
        // The host feeds the dwell fill: half way, cue fully faded in.
        h.update(std::collections::BTreeMap::from([
            ("bead.edge.dwell".into(), Value::Number(0.5)),
            ("bead.edge.fade".into(), Value::Number(1.)),
        ]))?;
        let trace = h
            .scene()
            .layers
            .iter()
            .find(|l| l.id.ends_with("title-trace"))
            .ok_or("title underline missing")?;
        assert!(
            !trace.own.is_empty(),
            "the title underline must paint while the dwell fills"
        );
        let hover = h.pose((0., 0.), 1.)?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("bar-parity-hover-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            hover.save(&golden)?;
        }
        hover.compare(
            &golden,
            bead_test::Tolerance::default(),
            &std::env::temp_dir().join(format!("bead-bar-hover-{scale}")),
        )?;

        let first = targets[0].1;
        h.script([Input::Pointer {
            x: first.x + first.w / 2.,
            y: first.y + first.h / 2.,
        }])?;
        let status_hover = h.pose((0., 0.), 1.)?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("status-pill-hover-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            status_hover.save(&golden)?;
        }
        status_hover.compare(
            &golden,
            bead_test::Tolerance::default(),
            &std::env::temp_dir().join(format!("bead-status-pill-hover-{scale}")),
        )?;
    }
    Ok(())
}

#[test]
fn workspace_icon_cells_share_one_bar_pill_at_both_scales() -> Result<()> {
    for scale in [1., 1.5] {
        let mut values = super::initial_models()?;
        values.insert(
            "bead.workspaces".into(),
            Value::list(vec![
                serde_json::json!({"id":"one","index":1,"label":"1","active":true,"icon_mode":true}),
                serde_json::json!({"id":"apps","index":2,"label":"","icon":"apps","icon_mode":true}),
                serde_json::json!({"id":"two","index":3,"label":"2","icon_mode":true}),
            ]),
        );
        let mut h = Harness::load(
            &root(),
            Path::new("bar/main.luau"),
            &root().join("palette.json"),
            values,
            Config {
                width: 1920,
                height: 50,
                scale,
            },
        )?;
        settle_harness_animations(&mut h)?;
        assert!(
            h.scene()
                .layers
                .iter()
                .any(|layer| layer.id.ends_with("active-capsule")),
            "workspace row must expose one shared active capsule"
        );
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("backend.workspace.switch"))
                .count(),
            3,
            "each workspace remains independently clickable"
        );
        let frame = h.pose((0., 0.), 1.)?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("workspaces-pill-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance::default(),
            &std::env::temp_dir().join(format!("bead-workspaces-pill-{scale}")),
        )?;

        let mut targets: Vec<_> = h
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("backend.workspace.switch"))
            .map(|node| bead::ui::rect(&node.draw))
            .collect();
        targets.sort_by(|a, b| a.x.total_cmp(&b.x));
        let rect = *targets.get(1).ok_or("workspace hover target missing")?;
        h.script([Input::Pointer {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        }])?;
        let hover = h.pose((0., 0.), 1.)?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("workspaces-pill-hover-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            hover.save(&golden)?;
        }
        hover.compare(
            &golden,
            bead_test::Tolerance::default(),
            &std::env::temp_dir().join(format!("bead-workspaces-pill-hover-{scale}")),
        )?;

        let capsule_bounds = |h: &Harness| -> Result<(f32, f32)> {
            let layer = h
                .scene()
                .layers
                .iter()
                .find(|layer| layer.id.ends_with("active-capsule"))
                .ok_or("active workspace capsule layer missing")?;
            h.scene().primitives[layer.own.clone()]
                .iter()
                .find_map(|primitive| match primitive {
                    bead::render::Primitive::Rounded { rect, .. } => Some((rect.x, rect.w)),
                    _ => None,
                })
                .ok_or_else(|| "active workspace capsule geometry missing".into())
        };
        let pill_width = |h: &Harness| -> Result<f32> {
            let layer = h
                .scene()
                .layers
                .iter()
                .find(|layer| {
                    layer.id.ends_with("capsule") && !layer.id.ends_with("active-capsule")
                })
                .ok_or("workspace pill layer missing")?;
            h.scene().primitives[layer.own.clone()]
                .iter()
                .find_map(|primitive| match primitive {
                    bead::render::Primitive::Rounded { rect, .. } => Some(rect.w),
                    _ => None,
                })
                .ok_or_else(|| "workspace pill geometry missing".into())
        };
        let move_capsule = |h: &mut Harness, model: Vec<serde_json::Value>| -> Result<()> {
            h.update(BTreeMap::from([(
                "bead.workspaces".into(),
                Value::list(model),
            )]))?;
            assert!(
                h.session.preview_deadline().is_some(),
                "workspace position or width change must animate"
            );
            Ok(())
        };
        let before_switch = capsule_bounds(&h)?;
        move_capsule(
            &mut h,
            vec![
                serde_json::json!({"id":"one","index":1,"label":"1","occupied":true,"icon_mode":false}),
                serde_json::json!({"id":"two","index":2,"label":"2","active":true,"icon_mode":false}),
                serde_json::json!({"id":"three","index":3,"label":"3","icon_mode":false}),
            ],
        )?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.update(BTreeMap::new())?;
        let switch_mid = capsule_bounds(&h)?;
        assert!(
            switch_mid.0 > before_switch.0 && switch_mid.0 < before_switch.0 + 34.,
            "active capsule must travel between workspace cells: {before_switch:?} -> {switch_mid:?}"
        );
        assert!(
            switch_mid.1 < before_switch.1 && switch_mid.1 > 32.,
            "active capsule width must resize with changing window hints"
        );
        settle_harness_animations(&mut h)?;

        let before_add = capsule_bounds(&h)?;
        let pill_before_add = pill_width(&h)?;
        move_capsule(
            &mut h,
            vec![
                serde_json::json!({"id":"one","index":1,"label":"1","icon_mode":false}),
                serde_json::json!({"id":"two","index":2,"label":"2","icon_mode":false}),
                serde_json::json!({"id":"three","index":3,"label":"3","icon_mode":false}),
                serde_json::json!({"id":"four","index":4,"label":"4","active":true,"icon_mode":false}),
            ],
        )?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.update(BTreeMap::new())?;
        let add_mid = capsule_bounds(&h)?;
        let pill_add_mid = pill_width(&h)?;
        assert!(
            add_mid.0 > before_add.0 + 1.,
            "new workspace must not make the active capsule jump directly to its slot"
        );
        assert!(
            pill_add_mid > pill_before_add + 1.,
            "outer pill width must animate as a workspace is added"
        );
        settle_harness_animations(&mut h)?;
        let pill_after_add = pill_width(&h)?;
        assert!(
            pill_after_add > pill_add_mid,
            "outer pill finishes its width expansion"
        );
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("backend.workspace.switch"))
                .count(),
            4,
            "new workspace cell appears in the repeated row"
        );

        let before_remove = capsule_bounds(&h)?;
        let pill_before_remove = pill_width(&h)?;
        move_capsule(
            &mut h,
            vec![
                serde_json::json!({"id":"one","index":1,"label":"1","occupied":true,"shown":1,"hinted":true,"app1":"apps","icon_mode":false}),
                serde_json::json!({"id":"two","index":2,"label":"2","active":true,"icon_mode":false}),
            ],
        )?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        h.update(BTreeMap::new())?;
        let remove_mid = capsule_bounds(&h)?;
        let pill_remove_mid = pill_width(&h)?;
        assert!(
            remove_mid.0 < before_remove.0 && remove_mid.0 > before_remove.0 - 68.,
            "removing workspaces moves the active capsule smoothly to a remaining cell"
        );
        assert!(
            pill_remove_mid < pill_before_remove,
            "outer pill width must begin shrinking as workspaces are removed"
        );
        settle_harness_animations(&mut h)?;
        assert!(
            pill_width(&h)? < pill_remove_mid,
            "outer pill finishes its width contraction"
        );
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("backend.workspace.switch"))
                .count(),
            2,
            "removed workspace cell leaves the repeated row"
        );
    }
    Ok(())
}

#[test]
fn status_pill_click_routes_switch_escape_and_reopen() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::sync::{Arc, Mutex};
    let mut values = super::initial_models()?;
    values.insert("bar.popouts".into(), Value::Bool(true));
    let h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 70,
            scale: 1.5,
        },
    )?;
    let target = |label: &str| -> Result<(f32, f32)> {
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|node| node.label == label)
            .ok_or("status target missing")?;
        let rect = bead::ui::rect(&node.draw);
        Ok((rect.x + rect.w / 2., rect.y + rect.h / 2.))
    };
    let network = target("Open network status")?;
    let bluetooth = target("Open Bluetooth status")?;
    let host = bead::services::Desktop::start_config(bead::services::Config {
        audio: false,
        bus: false,
    });
    let mut runtime = crate::reload::Runtime::new(h.session)
        .with_shared_desktop(host.clone(), true)?
        .with_popout_trigger()?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    runtime.services(Box::new(move |event| output.lock().unwrap().push(event)));
    events.lock().unwrap().clear();

    runtime.stage(UiEvent::Click {
        x: network.0,
        y: network.1,
    })?;
    runtime.commit();
    assert!(
        events.lock().unwrap().is_empty(),
        "the draggable click waits for release"
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    assert!(matches!(events.lock().unwrap().as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, center:Some(center), .. }]
            if view == "network" && *center == network.0));
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Click {
        x: bluetooth.0,
        y: bluetooth.1,
    })?;
    runtime.commit();
    assert!(
        events.lock().unwrap().is_empty(),
        "the draggable click waits for release"
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    assert!(matches!(events.lock().unwrap().as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, center:Some(center), .. }]
            if view == "bluetooth" && *center == bluetooth.0));
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Command {
        command: "escape".into(),
        shift: false,
    })?;
    assert!(matches!(
        events.lock().unwrap().as_slice(),
        [UiEvent::CompanionVisibility(false)]
    ));
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Click {
        x: network.0,
        y: network.1,
    })?;
    runtime.commit();
    assert!(
        events.lock().unwrap().is_empty(),
        "the draggable click waits for release"
    );
    runtime.stage(UiEvent::Release)?;
    runtime.commit();
    assert!(matches!(events.lock().unwrap().as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, .. }] if view == "network"));
    host.stop();
    Ok(())
}

#[test]
fn legacy_state_distinguishes_named_views_on_one_host() -> Result<()> {
    use bead::wayland::{CommandContext, Remote, SceneSource, UiEvent};
    let h = calendar(2024, 2, 1, 1.)?;
    let mut runtime = crate::reload::Runtime::new(h.session)
        .with_launcher_trigger(Default::default())?
        .with_popout_trigger()?;
    for (view, open, launcher, edge) in [
        ("calendar", true, false, "calendar"),
        ("sound", true, false, "sound"),
        ("launcher", true, true, ""),
        ("network", true, false, "network"),
        ("bluetooth", true, false, "bluetooth"),
        ("battery", true, false, "battery"),
        ("calendar", false, false, ""),
        ("dashboard-today", true, false, ""),
        ("dashboard-weather", true, false, ""),
        ("dashboard-media", true, false, ""),
        ("dashboard-system", true, false, ""),
        ("dashboard-media", false, false, ""),
    ] {
        let (reply, received) = std::sync::mpsc::sync_channel(1);
        runtime.stage(UiEvent::Remote(std::sync::Arc::new(Remote {
            call: bead::services::ipc::Call {
                version: 1,
                id: 1,
                target: "nacre".into(),
                method: "state".into(),
                args: vec![],
                output: None,
            },
            reply,
            context: CommandContext {
                panel_open: open,
                panel_configured: true,
                panel_view: Some(view.into()),
                hidden: false,
            },
        })))?;
        let response = received.recv_timeout(std::time::Duration::from_secs(1))?;
        assert!(response.error.is_none());
        let state: serde_json::Value = serde_json::from_str(
            response
                .result
                .as_ref()
                .and_then(serde_json::Value::as_str)
                .ok_or("legacy state should be JSON text")?,
        )?;
        let page = crate::dashboard::Page::from_view(view);
        assert_eq!(state["dashboard"], open && page.is_some());
        assert_eq!(state["tab"], page.map(|p| p.tab()).unwrap_or(0));
        assert_eq!(state["launcher"], launcher);
        assert_eq!(state["edgeMenu"], edge);
        assert_eq!(state["bead"]["panelOpen"], open);
        assert_eq!(
            state["bead"]["available"],
            serde_json::json!([
                "bar",
                "launcher",
                "sound-output",
                "calendar",
                "network-status",
                "bluetooth-status",
                "battery-status",
                "dashboard"
            ])
        );
    }
    Ok(())
}

#[test]
fn owner_key_routes_open_the_launcher_with_routes_and_toggle_like_qml() -> Result<()> {
    use bead::wayland::{CommandContext, Remote, SceneSource, UiEvent};
    use std::sync::{Arc, Mutex};
    let h = calendar(2024, 2, 1, 1.)?;
    let memory = crate::launcher::Memory::default();
    let mut runtime = crate::reload::Runtime::new(h.session)
        .with_launcher_trigger(memory.clone())?
        .with_popout_trigger()?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let output = events.clone();
    runtime.services(Box::new(move |event| output.lock().unwrap().push(event)));
    events.lock().unwrap().clear();
    let mut send = |target: &str,
                    method: &str,
                    args: &[&str],
                    open: bool|
     -> Result<bead::services::ipc::Reply> {
        let (reply, received) = std::sync::mpsc::sync_channel(1);
        runtime.stage(UiEvent::Remote(std::sync::Arc::new(Remote {
            call: crate::commands::from_strings(
                target.into(),
                method.into(),
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )?,
            reply,
            context: CommandContext {
                panel_open: open,
                panel_configured: true,
                panel_view: Some("launcher".into()),
                hidden: false,
            },
        })))?;
        Ok(received.recv_timeout(std::time::Duration::from_secs(1))?)
    };
    let route =
        |memory: &crate::launcher::Memory| memory.lock().unwrap()["launcher.engine_route"].clone();
    // Super+X: menu route, never a toggle.
    for open in [false, true] {
        events.lock().unwrap().clear();
        assert!(send("nacre", "menu", &["power", ""], open)?.error.is_none());
        assert!(matches!(
            events.lock().unwrap().as_slice(),
            [
                UiEvent::ShellHidden(Some(false)),
                UiEvent::CompanionVisibility(true),
                UiEvent::CompanionModel(_)
            ]
        ));
        assert_eq!(route(&memory), Value::Text("power".into()));
    }
    // Extras now leave the launcher family in the command map before Runtime admission.
    for (mode, view) in [
        ("overview", "overview"),
        ("palette", "palette"),
        ("keys", "keybindings"),
    ] {
        let mapped = crate::commands::map_views(crate::commands::from_strings(
            "nacre".into(),
            "mode".into(),
            &[mode.into()],
        )?)
        .unwrap();
        assert_eq!(mapped.target, "bead.panel");
        assert_eq!(mapped.method, "toggle");
        assert_eq!(mapped.args, vec![serde_json::json!(view)]);
        assert_eq!(route(&memory), Value::Text("power".into()));
    }
    // Super+Space twice: the second identical press closes the open launcher.
    events.lock().unwrap().clear();
    assert!(send("nacre", "launcher", &[""], false)?.error.is_none());
    assert!(send("nacre", "launcher", &[""], true)?.error.is_none());
    assert!(matches!(
        events.lock().unwrap().as_slice(),
        [
            UiEvent::ShellHidden(Some(false)),
            UiEvent::CompanionVisibility(true),
            UiEvent::CompanionModel(_),
            UiEvent::CompanionVisibility(false)
        ]
    ));
    // Super+V is the clipboard route and a different request replaces the launcher.
    events.lock().unwrap().clear();
    assert!(send("nacre", "clipboard", &[], true)?.error.is_none());
    assert_eq!(route(&memory), Value::Text("actions.clipboard".into()));
    assert!(matches!(
        events.lock().unwrap().as_slice(),
        [
            UiEvent::ShellHidden(Some(false)),
            UiEvent::CompanionVisibility(true),
            UiEvent::CompanionModel(_)
        ]
    ));
    // Refused or unpublished routes fail explicitly and leave the launcher alone.
    events.lock().unwrap().clear();
    for (target, method, args) in [
        ("nacre", "mode", &["legacy"][..]),
        ("nacre", "left", &[][..]),
        ("keyboardLight", "step", &["up"][..]),
        ("brightness", "step", &["up"][..]),
    ] {
        let reply = send(target, method, args, false)?;
        assert_eq!(
            reply.error.as_ref().map(|e| e.code.as_str()),
            Some("unavailable")
        );
    }
    assert!(events.lock().unwrap().is_empty());
    Ok(())
}

/// A wallpaper-driven palette reaches panel executors too: `bead-wayland` stages
/// the same `WallpaperPalette` event into an open or newly opened panel.
#[test]
fn live_palette_overlay_recolors_panel_views_without_reload_or_file_write() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    const SURFACE: [u8; 4] = [1, 2, 3, 255];
    let palette_file = root().join("palette.json");
    let before = std::fs::read(&palette_file)?;
    let entries = [
        ("popouts/main.luau", BTreeMap::new()),
        ("session/main.luau", BTreeMap::new()),
    ];
    for (entry, fixture) in entries {
        let h = Harness::load(
            &root(),
            Path::new(entry),
            &palette_file,
            fixture,
            Config {
                width: 380,
                height: 380,
                scale: 1.,
            },
        )?;
        let fills = |scene: &bead::render::Scene| {
            scene.primitives.iter().any(
                |p| matches!(p, bead::render::Primitive::Rounded { fill, .. } if *fill == SURFACE),
            )
        };
        assert!(!fills(h.scene()), "{entry} starts with the file palette");
        let mut runtime = crate::reload::Runtime::new(h.session);
        let scene = runtime
            .stage(UiEvent::WallpaperPalette(BTreeMap::from([(
                "surface".to_string(),
                SURFACE,
            )])))?
            .ok_or("palette stage produced no scene")?;
        assert!(
            fills(&scene),
            "{entry} did not take the live palette: {:?}",
            scene
                .primitives
                .iter()
                .filter_map(|p| match p {
                    bead::render::Primitive::Rounded { fill, .. } => Some(*fill),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        // A later reload keeps the overlay; the committed file is never rewritten.
        assert!(runtime.commit());
        let reloaded = runtime
            .stage(UiEvent::Reload)?
            .ok_or("reload produced no scene")?;
        assert!(fills(&reloaded), "{entry} lost the live palette on reload");
    }
    assert_eq!(std::fs::read(&palette_file)?, before);
    Ok(())
}

#[test]
fn panel_retirement_releases_launcher_mirror_but_bar_does_not() -> Result<()> {
    let memory = crate::launcher::Memory::default();
    let initial = BTreeMap::from([(
        "launcher.search.text".into(),
        bead::core::Value::Text("panel query".into()),
    )]);
    *memory.lock().unwrap() = initial.clone();
    let h = calendar(2026, 10, 0, 1.)?;
    let mut panel = crate::reload::Runtime::new(h.session)
        .with_launcher(None)?
        .with_launcher_memory(memory.clone())?;
    let panel_values = BTreeMap::from([
        (
            "launcher.search.text".into(),
            bead::core::Value::Text("kept query".into()),
        ),
        (
            "launcher.selection".into(),
            bead::core::Value::Text("app-id".into()),
        ),
        (
            "launcher.mode".into(),
            bead::core::Value::Text("apps".into()),
        ),
        (
            "launcher.route".into(),
            bead::core::Value::Text("apps".into()),
        ),
        (
            "launcher.engine_route".into(),
            bead::core::Value::Text(String::new()),
        ),
        ("launcher.scroll.y".into(), bead::core::Value::Number(42.)),
        (
            "launcher.apps".into(),
            bead::core::Value::list(vec![
                serde_json::json!({"id":"large","payload":"not retained"}),
            ]),
        ),
    ]);
    panel.session.stores = panel.session.stores.reconcile(&panel_values)?;
    panel.session.stores.commit(panel_values)?;
    let h = calendar(2026, 10, 0, 1.)?;
    let bar = crate::reload::Runtime::new(h.session).with_launcher_trigger(memory.clone())?;
    drop(bar);
    assert_eq!(
        *memory.lock().unwrap(),
        initial,
        "bar does not own panel state"
    );
    drop(panel);
    let saved = memory.lock().unwrap();
    assert_eq!(
        saved["launcher.search.text"],
        bead::core::Value::Text("kept query".into())
    );
    assert_eq!(
        saved["launcher.selection"],
        bead::core::Value::Text("app-id".into())
    );
    assert_eq!(saved["launcher.scroll.y"], bead::core::Value::Number(42.));
    assert!(
        !saved.contains_key("launcher.apps"),
        "row payload is released after worker retirement"
    );
    Ok(())
}

#[test]
fn bar_bundle_does_not_load_closed_panel_modules_or_assets() -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut values = super::initial_models()?;
    values.insert("bar.popouts".into(), Value::Bool(true));
    values.insert("bar.launcher".into(), Value::Bool(true));
    let stores = bead::core::Stores::default().reconcile(&values)?;
    let (bar, _) =
        bead::ui::Generation::load(&root, std::path::Path::new("bar/main.luau"), &stores, 1)?;
    for path in bar.sources.keys() {
        assert!(
            !["popouts", "sound", "ui", "status"]
                .iter()
                .any(|prefix| path.starts_with(prefix)),
            "closed panel content eagerly loaded: {}",
            path.display()
        );
    }
    assert!(!bar.sources.is_empty());
    Ok(())
}

#[test]
fn calendar_runtime_consumes_source_wakes_for_date_readiness_and_recovery() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use bead_test::source::FakeSource;
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let source = Arc::new(FakeSource::new(serde_json::json!({
        "date":{"year":2026,"month":12,"day":31}
    }))?);
    let h = calendar(2024, 2, 1, 1.)?;
    let mut runtime = crate::reload::Runtime::new(h.session).with_calendar(source.clone())?;
    let (send, receive) = mpsc::channel();
    runtime.services(Box::new(move |event| {
        send.send(event).unwrap();
    }));
    let event = receive.recv_timeout(Duration::from_millis(200))?;
    assert!(matches!(&event, UiEvent::Model(values) if values.is_empty()));
    let scene = runtime
        .stage(event)?
        .ok_or("calendar wake did not stage changed Source data")?;
    // Commit the headless session without writing the runtime's durable UI file
    // or invoking presentation/recovery side effects.
    runtime.session.commit(scene);
    assert_eq!(
        runtime.session.stores.get("calendar.today"),
        Some(&Value::Text("2026-12-31".into()))
    );
    assert_eq!(
        runtime.session.stores.get("calendar.year"),
        Some(&Value::Number(2026.))
    );
    source.publish(serde_json::json!({"date":{"year":1900,"month":2,"day":29}}))?;
    let event = receive.recv_timeout(Duration::from_millis(200))?;
    let scene = runtime
        .stage(event)?
        .ok_or("invalid date did not invalidate the view")?;
    runtime.session.commit(scene);
    assert_eq!(
        runtime.session.stores.get("calendar.ready"),
        Some(&Value::Bool(false))
    );
    source.publish(serde_json::json!({"date":{"year":2027,"month":1,"day":1}}))?;
    let event = receive.recv_timeout(Duration::from_millis(200))?;
    let scene = runtime
        .stage(event)?
        .ok_or("recovered date did not restore the view")?;
    runtime.session.commit(scene);
    assert_eq!(
        runtime.session.stores.get("calendar.ready"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        runtime.session.stores.get("calendar.today"),
        Some(&Value::Text("2027-01-01".into()))
    );
    assert_eq!(
        runtime.session.stores.get("calendar.year"),
        Some(&Value::Number(2027.))
    );
    assert_eq!(
        runtime.session.stores.get("calendar.month"),
        Some(&Value::Number(1.))
    );
    assert!(
        runtime.stage(UiEvent::Model(BTreeMap::new()))?.is_none(),
        "unchanged external data must not stage another layout or raster"
    );
    assert_eq!(source.calls(), 0);
    Ok(())
}

#[test]
fn calendar_follows_today_until_the_user_browses_then_keeps_that_month() -> Result<()> {
    use bead_test::source::FakeSource;
    use std::sync::Arc;
    let source = Arc::new(FakeSource::new(serde_json::json!({
        "date":{"year":2024,"month":2,"day":29}
    }))?);
    let projector = crate::calendar::Calendar::new(source.clone());
    let mut h = calendar(2024, 2, 1, 1.)?;
    h.session
        .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
    h.update(BTreeMap::new())?;
    source.publish(serde_json::json!({"date":{"year":2024,"month":3,"day":1}}))?;
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(3.))
    );
    month(&mut h, "Previous month")?;
    source.publish(serde_json::json!({"date":{"year":2025,"month":1,"day":1}}))?;
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(2.))
    );
    assert_eq!(
        h.session.stores.get("calendar.year"),
        Some(&Value::Number(2024.))
    );
    assert_eq!(
        h.session.stores.get("calendar.today"),
        Some(&Value::Text("2025-01-01".into()))
    );
    assert_eq!(source.calls(), 0);
    Ok(())
}

#[test]
fn calendar_old_bundle_without_follow_flag_keeps_its_navigation_behavior() -> Result<()> {
    use bead_test::source::FakeSource;
    use std::sync::Arc;
    let source = Arc::new(FakeSource::new(serde_json::json!({
        "date":{"year":2025,"month":1,"day":1}
    }))?);
    let projector = crate::calendar::Calendar::new(source.clone());
    let stores = bead::core::Stores::default().reconcile(&BTreeMap::from([
        ("calendar.initialized".into(), Value::Bool(true)),
        ("calendar.year".into(), Value::Number(2024.)),
        ("calendar.month".into(), Value::Number(2.)),
    ]))?;
    let changes = projector.project(&stores);
    assert!(!changes.contains_key("calendar.year"));
    assert!(!changes.contains_key("calendar.month"));
    assert_eq!(changes["calendar.today"], Value::Text("2025-01-01".into()));
    // Also preserve an already-open older view when the NEW bundle supplies
    // its optional follow flag. Calendar month/year are not durable page state.
    let mut h = calendar(2024, 2, 1, 1.)?;
    h.update(BTreeMap::from([(
        "calendar.initialized".into(),
        Value::Bool(true),
    )]))?;
    let projector = crate::calendar::Calendar::new(source);
    h.session
        .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
    h.update(BTreeMap::new())?;
    assert_eq!(
        h.session.stores.get("calendar.year"),
        Some(&Value::Number(2024.))
    );
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(2.))
    );
    assert_eq!(
        h.session.stores.get("calendar.today"),
        Some(&Value::Text("2025-01-01".into()))
    );
    Ok(())
}

/// B0042: session menu idle, inline confirmation, and disabled (route not advertised).
#[test]
fn session_menu_goldens_idle_confirming_and_disabled() -> Result<()> {
    crate::i18n::init();
    let states = [
        ("idle", true, "", ""),
        ("confirm", true, "logout", "Confirm log out"),
        ("disabled", false, "", ""),
    ];
    for scale in [1., 1.5] {
        for (name, enabled, action, question) in states {
            let mut h = Harness::load(
                &root(),
                Path::new("session/main.luau"),
                &root().join("palette.json"),
                BTreeMap::from([
                    ("panel.open".into(), Value::Bool(true)),
                    ("session.enabled".into(), Value::Bool(enabled)),
                    ("session.confirm".into(), Value::Text(question.into())),
                    ("session.confirm_action".into(), Value::Text(action.into())),
                ]),
                Config {
                    width: 490,
                    height: 500,
                    scale,
                },
            )?;
            let acting = h
                .session
                .view_nodes()
                .iter()
                .filter(|n| n.action.is_some())
                .count();
            // Five power actions; asking adds Cancel and confirmation; unavailable is inert.
            let expected = match (enabled, action) {
                (true, "") => 5,
                (true, _) => 7,
                (false, _) => 0,
            };
            assert_eq!(acting, expected, "{name}");
            let frame = h.render()?;
            let golden = root()
                .join("tests/goldens")
                .join(format!("session-{name}-{scale}.png"));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            let artifacts =
                std::env::temp_dir().join(format!("bead-session-golden-{name}-{scale}"));
            frame.compare(&golden, bead_test::Tolerance::default(), &artifacts)?;
        }
    }
    Ok(())
}

#[test]
fn widget_preset_roundtrip_live_move_unknown_preservation_and_atomic_rejection() -> Result<()> {
    use serde_json::{Value as Json, json};
    let mut values = super::initial_models()?;
    values.insert("bead.desktop.clock".into(), Value::Text("11:02".into()));
    let mut h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 50,
            scale: 1.5,
        },
    )?;
    settle_harness_animations(&mut h)?;
    let baseline = serde_json::to_value(&h.scene().primitives)?;
    let preset: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let roundtrip: Json = serde_json::from_str(&serde_json::to_string(&preset)?)?;
    let apply = |doc: Json| BTreeMap::from([("widgets.layout".into(), Value::list(vec![doc]))]);
    h.update(apply(roundtrip))?;
    settle_harness_animations(&mut h)?;
    assert_eq!(
        serde_json::to_value(&h.scene().primitives)?,
        baseline,
        "explicit preset must reproduce the implicit preset"
    );
    let mut edited = preset.clone();
    edited["hosts"]["top_bar"]["instances"][0]["settings"]["future_setting"] =
        json!({"opaque": [1, 2]});
    let instances = edited["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap();
    let mut clock = instances[3]["children"].as_array_mut().unwrap().remove(6);
    clock["host"] = "top_bar".into();
    clock["slot"] = json!({"region":"left","start":2,"span":1});
    instances.push(clock);
    instances.push(json!({"iid":"future","widget":"future-package","host":"top_bar","slot":{"region":"right","start":3,"span":1},"settings":{"opaque":[1,{"x":true}]}}));
    h.update(apply(edited.clone()))?;
    let moved_clock = h
        .scene()
        .layers
        .iter()
        .find(|layer| layer.id.starts_with("bar.left.") && layer.id.ends_with(".clock.time"))
        .ok_or("moved clock's time content is rendered in the left region")?;
    assert!(
        moved_clock.bounds.x > 0.,
        "the moved clock keeps its left-slot geometry"
    );
    assert!(
        h.scene()
            .primitives
            .iter()
            .any(|primitive| matches!(primitive,
            bead::render::Primitive::Text {text,..} if text=="11:02")),
        "the moved clock keeps rendering its live text"
    );
    assert_eq!(
        h.session.stores.get("widgets.layout"),
        Some(&Value::list(vec![edited.clone()]))
    );
    let transition_frame = serde_json::to_value(&h.scene().primitives)?;
    let settling = h.session.preview_deadline().is_some();
    settle_harness_animations(&mut h)?;
    assert!(h.session.preview_deadline().is_none());
    let accepted = serde_json::to_value(&h.scene().primitives)?;
    if settling {
        assert_ne!(
            transition_frame, accepted,
            "the accepted widget move should reach its animated layout endpoint"
        );
    }
    for invalid in [
        {
            let mut d = edited.clone();
            d["version"] = json!(99);
            d
        },
        {
            let mut d = edited.clone();
            d["hosts"]["top_bar"]["instances"][0]["iid"] = json!("clock");
            d
        },
        {
            let mut d = edited.clone();
            d["hosts"]["top_bar"]["instances"][0]["settings"]["icon"] = json!("invalid");
            d
        },
        {
            let mut d = edited.clone();
            d["hosts"]["top_bar"]["instances"][0]["slot"]["span"] = json!(61);
            d
        },
    ] {
        assert!(h.update(apply(invalid)).is_err());
        assert_eq!(serde_json::to_value(&h.scene().primitives)?, accepted);
        assert_eq!(
            h.session.stores.get("widgets.layout"),
            Some(&Value::list(vec![edited.clone()]))
        );
    }
    h.update(apply(preset))?;
    let returning_frame = serde_json::to_value(&h.scene().primitives)?;
    let returning = h.session.preview_deadline().is_some();
    settle_harness_animations(&mut h)?;
    assert!(h.session.preview_deadline().is_none());
    let restored = serde_json::to_value(&h.scene().primitives)?;
    if returning {
        assert_ne!(
            returning_frame, restored,
            "returning the widget must settle its animated layout endpoint"
        );
    }
    assert_eq!(
        restored, baseline,
        "moving the clock back must restore the bar"
    );
    Ok(())
}

#[test]
fn widget_launcher_settings_select_dashboard_and_custom_view_routes() -> Result<()> {
    let mut values = super::initial_models()?;
    values.insert("bar.launcher".into(), Value::Bool(true));
    let mut h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 50,
            scale: 1.5,
        },
    )?;
    let mut preset: serde_json::Value =
        serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    for (action, route, expected) in [
        ("dashboard", "", "dashboard-today"),
        ("custom", "calendar", "calendar"),
    ] {
        preset["hosts"]["top_bar"]["instances"][0]["settings"]["action"] = action.into();
        preset["hosts"]["top_bar"]["instances"][0]["settings"]["route"] = route.into();
        h.update(BTreeMap::from([(
            "widgets.layout".into(),
            Value::list(vec![preset.clone()]),
        )]))?;
        let node = h
            .session
            .view_nodes()
            .iter()
            .find(|n| n.id == "bar.left.brand")
            .ok_or("launcher widget missing")?;
        assert_eq!(node.action.as_deref(), Some("view.open"));
        assert_eq!(node.action_arg, Some(Value::Text(expected.into())));
    }
    Ok(())
}

#[test]
fn widget_editor_transactions_keep_pills_unknown_data_and_cancel_is_not_a_write() -> Result<()> {
    use serde_json::{Value as Json, json};
    let preset: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let mut h = Harness::load(
        &root(),
        Path::new("widgets/editor/main.luau"),
        &root().join("palette.json"),
        super::initial_models()?,
        Config {
            width: 1920,
            height: 1080,
            scale: 1.5,
        },
    )?;
    let apply = |h: &mut Harness, mut request: Json| -> Result<()> {
        request["document"] = preset.clone();
        request["initial"] = preset.clone();
        request["rev"] = json!(7);
        let changes = h
            .session
            .action_values("widgets.edit", Value::list(vec![request]))?;
        h.update(changes)
    };
    let doc = |h: &Harness| -> Json {
        match h.session.stores.get("widgets.draft").unwrap() {
            Value::List(items) => items[0].clone(),
            _ => panic!("draft must be a document list"),
        }
    };
    apply(
        &mut h,
        json!({"op":"move","iid":"clock","host":"top_bar","region":"left","start":2}),
    )?;
    assert!(
        doc(&h)["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock" && item["host"] == "top_bar")
    );
    apply(
        &mut h,
        json!({"op":"setting","iid":"launcher","key":"future","value":{"opaque":[1,2],"empty_object":{},"empty_array":[],"nothing":null,"large":18446744073709551615_u64}}),
    )?;
    let accepted = doc(&h);
    assert_eq!(
        accepted["hosts"]["top_bar"]["instances"][0]["settings"]["future"],
        json!({"opaque":[1,2],"empty_object":{},"empty_array":[],"nothing":null,"large":18446744073709551615_u64})
    );
    assert!(
        apply(
            &mut h,
            json!({"op":"setting","iid":"launcher","key":"icon","value":"invalid"})
        )
        .is_err()
    );
    assert_eq!(
        doc(&h),
        accepted,
        "invalid settings must preserve the accepted draft"
    );
    apply(
        &mut h,
        json!({"op":"move","iid":"clock","host":"status-pill","region":"right","start":0}),
    )?;
    let restored = doc(&h);
    let pill = restored["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    assert!(
        pill["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock" && item["host"] == "status-pill")
    );
    apply(
        &mut h,
        json!({"op":"move","iid":"status-pill","host":"status-pill","region":"left","start":0}),
    )?;
    assert_eq!(doc(&h), restored, "cyclic moves must be refused");
    apply(
        &mut h,
        json!({"op":"add","widget":"clock","host":"top_bar","region":"left","start":2,"span":1}),
    )?;
    let first = doc(&h);
    apply(
        &mut h,
        json!({"op":"add","widget":"clock","host":"top_bar","region":"left","start":3,"span":1}),
    )?;
    assert_ne!(doc(&h), first, "copies need unique instance IDs");
    apply(&mut h, json!({"op":"undo"}))?;
    assert_eq!(doc(&h), first);
    apply(&mut h, json!({"op":"done"}))?;
    assert_eq!(
        h.session.stores.get("widgets.write"),
        Some(&Value::list(vec![first]))
    );
    // Cancel a fresh editing session: it never creates an external-write request.
    h.update(BTreeMap::from([(
        "widgets.write".into(),
        Value::list(Vec::new()),
    )]))?;
    apply(&mut h, json!({"op":"cancel"}))?;
    assert_eq!(
        h.session.stores.get("widgets.write"),
        Some(&Value::list(Vec::new()))
    );
    assert_eq!(
        h.session.stores.get("widgets.draft"),
        Some(&Value::list(Vec::new()))
    );
    Ok(())
}

#[test]
fn widget_canvas_move_roundtrip_snaps_preserves_ids_and_rejects_duplicates() -> Result<()> {
    use serde_json::{Value as Json, json};
    let preset: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let mut h = Harness::load(
        &root(),
        Path::new("widgets/editor/main.luau"),
        &root().join("palette.json"),
        super::initial_models()?,
        Config {
            width: 1920,
            height: 1080,
            scale: 1.5,
        },
    )?;
    let apply = |h: &mut Harness, mut request: Json| -> Result<()> {
        request["document"] = preset.clone();
        request["canvas_width"] = json!(1920);
        request["canvas_height"] = json!(1080);
        let values = h
            .session
            .action_values("widgets.edit", Value::list(vec![request]))?;
        h.update(values)
    };
    apply(
        &mut h,
        json!({"op":"move","iid":"clock","host":"canvas","x":100,"y":100,"w":88,"h":34,"snap":32}),
    )?;
    let Value::List(draft) = h.session.stores.get("widgets.draft").unwrap() else {
        panic!("missing draft")
    };
    let document = draft[0].clone();
    assert_eq!(document["hosts"]["canvas"]["instances"][0]["iid"], "clock");
    assert_eq!(
        document["hosts"]["canvas"]["instances"][0]["slot"],
        json!({"x":96,"y":96,"w":88,"h":34})
    );
    let mut idle = super::initial_models()?;
    idle.insert("widgets.layout".into(), Value::list(vec![document.clone()]));
    idle.insert("widgets.editing".into(), Value::Bool(false));
    let mut canvas = Harness::load(
        &root(),
        Path::new("widgets/editor/canvas.luau"),
        &root().join("palette.json"),
        idle,
        Config {
            width: 1920,
            height: 1080,
            scale: 1.5,
        },
    )?;
    assert!(
        canvas
            .scene()
            .layers
            .iter()
            .any(|layer| layer.id.contains("clock-desktop")),
        "the moved clock must render its desktop form"
    );
    assert!(
        canvas
            .session
            .view_nodes()
            .iter()
            .all(|node| node.action.is_none() && node.input.is_none()),
        "passive fixture canvas cannot capture keyboard input"
    );
    let mut invalid = document.clone();
    invalid["hosts"]["canvas"]["instances"]
        .as_array_mut()
        .unwrap()[0]["iid"] = json!("launcher");
    assert!(
        canvas
            .update(BTreeMap::from([(
                "widgets.layout".into(),
                Value::list(vec![invalid])
            )]))
            .is_err()
    );
    assert_eq!(
        canvas.session.stores.get("widgets.layout"),
        Some(&Value::list(vec![document]))
    );
    apply(
        &mut h,
        json!({"op":"move","iid":"clock","host":"top_bar","region":"left","start":2,"span":1}),
    )?;
    let Value::List(draft) = h.session.stores.get("widgets.draft").unwrap() else {
        panic!("missing draft")
    };
    assert!(
        draft[0]["hosts"]["canvas"]["instances"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        draft[0]["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock" && item["host"] == "top_bar")
    );
    Ok(())
}

#[test]
fn canvas_resize_handles_follow_the_widgets_app_open_state() -> Result<()> {
    use serde_json::{Value as Json, json};
    let mut document: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    document["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[{
        "iid":"desktop-clock","widget":"clock","host":"canvas",
        "slot":{"x":96,"y":96,"w":240,"h":96},"settings":{}
    }]});
    let mut values = super::initial_models()?;
    values.insert("widgets.layout".into(), Value::list(vec![document]));
    values.insert("bead.desktop.clock".into(), Value::Text("11:02".into()));
    values.insert("bead.hovered".into(), Value::Text("desktop-clock".into()));
    let mut canvas = Harness::load(
        &root(),
        Path::new("widgets/editor/canvas.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 1080,
            scale: 1.5,
        },
    )?;
    assert_eq!(
        canvas.session.stores.get("widgets.app.open"),
        Some(&Value::Bool(false)),
        "canvas behavior seed declares app-open state even when the app is absent"
    );

    let has_resize_handle = |h: &Harness| {
        h.session
            .view_nodes()
            .iter()
            .any(|node| node.id.contains("resize-desktop-clock"))
    };
    assert!(
        !has_resize_handle(&canvas),
        "closed app hides canvas resize handles"
    );
    canvas.update(BTreeMap::from([(
        "widgets.app.open".into(),
        Value::Bool(true),
    )]))?;
    assert!(
        has_resize_handle(&canvas),
        "open app exposes the hovered canvas handle"
    );
    canvas.update(BTreeMap::from([(
        "widgets.app.open".into(),
        Value::Bool(false),
    )]))?;
    assert!(
        !has_resize_handle(&canvas),
        "closing app removes canvas resize handles"
    );
    Ok(())
}

#[test]
fn panel_hover_lifecycle_does_not_add_undeclared_popout_state() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    for entry in ["sound/main.luau", "status/main.luau"] {
        let h = Harness::load(
            &root(),
            Path::new(entry),
            &root().join("palette.json"),
            super::initial_models()?,
            Config {
                width: 1920,
                height: 1200,
                scale: 1.5,
            },
        )?;
        let mut runtime = crate::reload::Runtime::new(h.session);
        for (opened, anchor) in [
            (Some(false), false),
            (Some(true), true),
            (None, false),
            (None, true),
        ] {
            runtime.stage(UiEvent::PanelHover { opened, anchor })?;
            runtime.commit();
            assert!(runtime.session.stores.get("panel.anchor_hovered").is_none());
            assert!(runtime.session.stores.get("panel.hover_opened").is_none());
        }
    }
    Ok(())
}

#[test]
fn configured_form_styles_are_validated_and_seconds_demand_follows_the_visible_host() -> Result<()>
{
    use serde_json::{Value as Json, json};
    fn clock_style(document: &mut Json, style: &str) {
        document["hosts"]["top_bar"]["instances"][3]["children"][6]["style"] = style.into();
    }
    fn seconds(h: &Harness) -> bool {
        h.session.view_nodes().iter().any(|node| {
            matches!(&node.metadata, Some(Value::List(records))
                if records.first().is_some_and(|seed| seed["widget_clock_seconds"] == true))
        })
    }
    let mut document: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let mut values = super::initial_models()?;
    values.insert("bead.desktop.clock".into(), Value::Text("12:34".into()));
    values.insert(
        "bead.desktop.clock.seconds".into(),
        Value::Text("12:34:56".into()),
    );
    values.insert("widgets.layout".into(), Value::list(vec![document.clone()]));
    let config = Config {
        width: 1920,
        height: 1080,
        scale: 1.5,
    };
    let mut bar = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values.clone(),
        config,
    )?;
    assert!(!seconds(&bar));
    clock_style(&mut document, "seconds");
    bar.update(BTreeMap::from([(
        "widgets.layout".into(),
        Value::list(vec![document.clone()]),
    )]))?;
    assert!(seconds(&bar));
    assert!(bar.scene().primitives.iter().any(|p| matches!(p,
        bead::render::Primitive::Text {text,..} if text == "12:34:56")));
    let accepted = document.clone();
    clock_style(&mut document, "nonexistent");
    assert!(
        bar.update(BTreeMap::from([(
            "widgets.layout".into(),
            Value::list(vec![document])
        )]))
        .is_err()
    );
    assert_eq!(
        bar.session.stores.get("widgets.layout"),
        Some(&Value::list(vec![accepted.clone()]))
    );
    document = accepted;
    clock_style(&mut document, "time");
    document["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[{
        "iid":"desktop-clock","widget":"clock","host":"canvas","style":"seconds",
        "settings":{},"slot":{"x":96,"y":96,"w":240,"h":96}}]});
    bar.update(BTreeMap::from([(
        "widgets.layout".into(),
        Value::list(vec![document.clone()]),
    )]))?;
    assert!(
        !seconds(&bar),
        "desktop-only seconds must not lease the bar clock"
    );
    values.insert("widgets.layout".into(), Value::list(vec![document.clone()]));
    values.insert("widgets.editing".into(), Value::Bool(false));
    let mut canvas = Harness::load(
        &root(),
        Path::new("widgets/editor/canvas.luau"),
        &root().join("palette.json"),
        values,
        config,
    )?;
    assert!(seconds(&canvas));
    document["hosts"]["canvas"]["instances"][0]["widget"] = "workspaces".into();
    document["hosts"]["canvas"]["instances"][0]["style"] = "pearls".into();
    assert!(
        canvas
            .update(BTreeMap::from([(
                "widgets.layout".into(),
                Value::list(vec![document])
            )]))
            .is_err(),
        "a widget without a desktop form cannot become a floating card"
    );
    Ok(())
}

/// B0189: analog clocks do not animate between ticks, ease the minute hand for a fraction
/// of a second once a minute, and give each hand its own tight damage box.
#[test]
fn analog_clock_redraws_only_on_ticks_and_hands_hug_their_pixels() -> Result<()> {
    use serde_json::{Value as Json, json};
    fn analog(style: &str) -> Json {
        let mut document: Json =
            serde_json::from_str(include_str!("../tests/widgets-default.json")).unwrap();
        document["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[{
            "iid":"desktop-analog","widget":"clock","host":"canvas","style":style,
            "settings":{},"slot":{"x":96,"y":96,"w":420,"h":300}}]});
        document
    }
    fn seconds(h: &Harness) -> bool {
        h.session.view_nodes().iter().any(|node| {
            matches!(&node.metadata, Some(Value::List(records))
                if records.first().is_some_and(|seed| seed["widget_clock_seconds"] == true))
        })
    }
    fn clock(time: &str, second: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("bead.desktop.clock".into(), Value::Text(time.into())),
            (
                "bead.desktop.clock.seconds".into(),
                Value::Text(second.into()),
            ),
            (
                "bead.desktop.clock.date".into(),
                Value::Text("2026-10-29".into()),
            ),
        ])
    }
    let config = Config {
        width: 1920,
        height: 1080,
        scale: 1.5,
    };
    let load = |style: &str| -> Result<Harness> {
        let mut values = super::initial_models()?;
        values.extend(clock("12:34", "12:34:56"));
        values.insert("widgets.layout".into(), Value::list(vec![analog(style)]));
        values.insert("widgets.editing".into(), Value::Bool(false));
        Harness::load(
            &root(),
            Path::new("widgets/editor/canvas.luau"),
            &root().join("palette.json"),
            values,
            config,
        )
    };
    let mut plain = load("analog")?;
    assert!(
        !seconds(&plain),
        "hour and minute hands need no seconds ticks"
    );
    assert!(
        plain.session.preview_deadline().is_none(),
        "a settled analog clock schedules no frames"
    );
    plain.update(clock("12:35", "12:35:00"))?;
    let eased = plain
        .session
        .preview_deadline()
        .expect("the minute hand eases after a tick");
    assert!(
        eased.saturating_duration_since(std::time::Instant::now())
            <= std::time::Duration::from_millis(450),
        "the minute ease lasts a fraction of a second"
    );
    let hands: Vec<_> = plain
        .scene()
        .primitives
        .iter()
        .filter_map(|p| match p {
            bead::render::Primitive::Polyline { rect, .. } if rect.w < 300. && rect.h < 300. => {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert!(
        hands.iter().all(|r| r.w * r.h < 0.5 * 284. * 284.),
        "each hand's box is smaller than half of the dial: {hands:?}"
    );
    let ticking = load("analog-seconds")?;
    assert!(seconds(&ticking), "only the seconds style leases the clock");
    Ok(())
}

/// B0193: the digital clock's ink is the palette role that reads on the wallpaper under it
/// (luminance grid from the host), with a translucent scrim only when neither role reaches
/// the contrast floor, and it follows a wallpaper change.
#[test]
fn desktop_clock_ink_follows_wallpaper_luminance() -> Result<()> {
    use serde_json::{Value as Json, json};
    // Relative luminance of the dark palette's onSurface (ecebe8) and surface (12120f).
    const ON_SURFACE: f64 = 0.83;
    const SURFACE: f64 = 0.006;
    const ON: [u8; 4] = [0xec, 0xeb, 0xe8, 255];
    const SURF: [u8; 4] = [0x12, 0x12, 0x0f, 255];
    fn luma(cell: f64) -> Value {
        Value::list(vec![json!({"cols":16,"rows":9,"on_surface":ON_SURFACE,"surface":SURFACE,
            "cells":(0..16 * 9).map(|_| cell).collect::<Vec<_>>()})])
    }
    fn ink(h: &Harness) -> ([u8; 4], bool) {
        let ink = h
            .scene()
            .primitives
            .iter()
            .find_map(|p| match p {
                bead::render::Primitive::Text { text, color, .. } if text == "12:34" => {
                    Some(*color)
                }
                _ => None,
            })
            .expect("the time text");
        let scrim = h.scene().primitives.iter().any(|p| {
            matches!(p, bead::render::Primitive::Rounded { rect, fill, .. }
                if rect.w >= 419. && rect.h >= 189. && fill[3] > 0 && fill[3] < 255)
        });
        (ink, scrim)
    }
    let mut document: Json =
        serde_json::from_str(include_str!("../tests/widgets-default.json")).unwrap();
    document["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[{
        "iid":"desktop-digital","widget":"clock","host":"canvas","style":"digital",
        "settings":{},"slot":{"x":96,"y":96,"w":420,"h":190}}]});
    let mut values = super::initial_models()?;
    values.insert("bead.desktop.clock".into(), Value::Text("12:34".into()));
    values.insert(
        "bead.desktop.clock.date".into(),
        Value::Text("2026-10-29".into()),
    );
    values.insert("widgets.layout".into(), Value::list(vec![document]));
    values.insert("widgets.editing".into(), Value::Bool(false));
    let mut canvas = Harness::load(
        &root(),
        Path::new("widgets/editor/canvas.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 1080,
            scale: 1.5,
        },
    )?;
    let set = |canvas: &mut Harness, cell: f64| {
        canvas.update(BTreeMap::from([(
            "bead.desktop.wallpaper.luma".into(),
            luma(cell),
        )]))
    };
    assert_eq!(ink(&canvas), (ON, false), "no luminance yet: the theme ink");
    set(&mut canvas, 0.02)?;
    assert_eq!(ink(&canvas), (ON, false), "dark wallpaper keeps light ink");
    set(&mut canvas, 0.9)?;
    assert_eq!(ink(&canvas), (SURF, false), "bright wallpaper switches to dark ink");
    set(&mut canvas, 0.2)?;
    assert!(ink(&canvas).1, "mid luminance reads on neither role: scrim");
    set(&mut canvas, 0.02)?;
    assert_eq!(ink(&canvas), (ON, false), "a wallpaper change brings the light ink back");
    Ok(())
}

#[test]
fn title_width_reservation_uses_configured_instance_settings() -> Result<()> {
    let mut document: serde_json::Value =
        serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    document["hosts"]["top_bar"]["instances"][2]["settings"] =
        serde_json::json!({"min_width":0,"max_width":128});
    let mut values = super::initial_models()?;
    values.insert("widgets.layout".into(), Value::list(vec![document]));
    let bar = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 70,
            scale: 1.5,
        },
    )?;
    let width = bar
        .session
        .view_nodes()
        .iter()
        .find_map(|node| {
            let Some(Value::List(records)) = &node.metadata else {
                return None;
            };
            records.first()?["reservations"]
                .as_array()?
                .iter()
                .find(|reservation| reservation["iid"] == "title")?["width"]
                .as_f64()
        })
        .ok_or("title reservation missing")?;
    assert!(
        (128. ..200.).contains(&width),
        "configured title reserved {width} px"
    );
    Ok(())
}

#[test]
fn status_value_styles_keep_icons_and_values_on_the_same_bar_line() -> Result<()> {
    let mut document: serde_json::Value =
        serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    for index in [1, 2, 3] {
        document["hosts"]["top_bar"]["instances"][3]["children"][index]["style"] =
            "icon+value".into();
    }
    let mut values = super::initial_models()?;
    values.extend(BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![document])),
        ("bar.popouts".into(), Value::Bool(true)),
        ("bead.desktop.audio".into(), Value::Text("volume_up".into())),
        ("bead.desktop.audio.label".into(), Value::Text("45%".into())),
        ("bead.desktop.network".into(), Value::Text("wifi".into())),
        (
            "bead.desktop.bluetooth".into(),
            Value::Text("bluetooth".into()),
        ),
    ]));
    let bar = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width: 1920,
            height: 70,
            scale: 1.5,
        },
    )?;
    let text_rect = |value: &str| {
        bar.scene()
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                bead::render::Primitive::Text { text, rect, .. } if text == value => Some(*rect),
                _ => None,
            })
            .expect("status text missing")
    };
    for (icon, value) in [
        ("volume_up", "45%"),
        ("wifi", "Connected"),
        ("bluetooth", "On"),
    ] {
        let icon = text_rect(icon);
        let value = text_rect(value);
        assert!(value.x > icon.x);
        assert!((value.y + value.h / 2. - icon.y - icon.h / 2.).abs() < 2.5);
    }
    Ok(())
}

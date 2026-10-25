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
            .any(|r| r.prepare_view.as_deref() == Some("peek-audio")
                && r.prepare_panel
                && r.prepare_center == Some(audio.0))
    );
    let notification_anchor = regions
        .iter()
        .find(|r| r.prepare_view.as_deref() == Some("peek-notifications"))
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
    assert!(
        matches!(events.lock().unwrap().as_slice(),[UiEvent::CompanionSelect{view,toggle:true,center:Some(x),..}]if view=="calendar" && *x==calendar.0)
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
    let received = events.lock().unwrap();
    assert!(matches!(received.as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, center:Some(center), .. }]
            if view == "notifications" && *center == notification_anchor));
    drop(received);
    host.stop();
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
    assert!(matches!(events.lock().unwrap().as_slice(),
        [UiEvent::CompanionSelect { view, toggle:false, center:Some(center), .. }]
            if view == "network" && *center == network.0));
    events.lock().unwrap().clear();
    runtime.stage(UiEvent::Click {
        x: bluetooth.0,
        y: bluetooth.1,
    })?;
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
            // Four tiles and the back arrow; asking adds Cancel and the confirmation; disabled leaves only the back arrow.
            let expected = match (enabled, action) {
                (true, "") => 5,
                (true, _) => 7,
                (false, _) => 1,
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
    let mut h = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        super::initial_models()?,
        Config {
            width: 1920,
            height: 50,
            scale: 1.5,
        },
    )?;
    let baseline = serde_json::to_value(&h.scene().primitives)?;
    let preset: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let roundtrip: Json = serde_json::from_str(&serde_json::to_string(&preset)?)?;
    let apply = |doc: Json| BTreeMap::from([("widgets.layout".into(), Value::list(vec![doc]))]);
    h.update(apply(roundtrip))?;
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
    assert!(
        h.scene()
            .layers
            .iter()
            .any(|l| l.id == "bar.left.clock.time")
    );
    assert_eq!(
        h.session.stores.get("widgets.layout"),
        Some(&Value::list(vec![edited.clone()]))
    );
    let accepted = serde_json::to_value(&h.scene().primitives)?;
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
    assert_eq!(
        serde_json::to_value(&h.scene().primitives)?,
        baseline,
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

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
    assert!(
        time.x >= clock.x + 24.,
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
                && r.prepare_center.is_none())
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
                "battery-status"
            ])
        );
    }
    Ok(())
}

#[test]
fn panel_retirement_releases_launcher_mirror_but_bar_does_not() -> Result<()> {
    let memory = crate::launcher::Memory::default();
    let h = calendar(2026, 10, 0, 1.)?;
    let panel = crate::reload::Runtime::new(h.session)
        .with_launcher(None)?
        .with_launcher_memory(memory.clone())?;
    memory.lock().unwrap().insert(
        "launcher.search.text".into(),
        bead::core::Value::Text("x".repeat(65536)),
    );
    let h = calendar(2026, 10, 0, 1.)?;
    let bar = crate::reload::Runtime::new(h.session).with_launcher_trigger(memory.clone())?;
    drop(bar);
    assert_eq!(
        memory.lock().unwrap().len(),
        1,
        "bar does not own panel state"
    );
    drop(panel);
    assert!(
        memory.lock().unwrap().is_empty(),
        "worker retirement releases all mirrored values"
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

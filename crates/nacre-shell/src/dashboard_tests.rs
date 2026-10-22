// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead_test::{Config, Harness, Result, source::FakeSource};

pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
pub(super) fn fixture(name: &str) -> Json {
    let name = match name {
        "today" => "calendar",
        "devices" => "bluetooth",
        "customize" => "settings",
        other => other,
    };
    let folder = if matches!(
        name,
        "notifications"
            | "bluetooth"
            | "audio"
            | "network"
            | "focus"
            | "clipboard"
            | "screenshots"
            | "ai-usage"
            | "brain"
            | "settings"
            | "player"
    ) {
        "dashboard-extras"
    } else {
        "dashboard"
    };
    serde_json::from_slice(
        &std::fs::read(root().join(format!("tests/fixtures/{folder}/{name}.json"))).unwrap(),
    )
    .unwrap()
}
fn view(page: Page, data: Json, updates: Option<Json>) -> Dashboard {
    Dashboard::new(
        page,
        Arc::new(FakeSource::new(data).unwrap()),
        updates.map(|data| Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>),
        true,
    )
}

#[test]
fn published_dashboard_frames_match_and_merge_to_valid_typed_states() -> Result<()> {
    use bead::core::source::{Applied, Mirror};
    use nacre_protocol::{Message, framing};
    use std::time::Instant;
    let generated = nacre_protocol::artifacts::generate();
    let mut count = 0;
    for (name, text) in [
        (
            "calendar",
            include_str!("../tests/protocol-v1/core-dashboard-calendar.jsonl"),
        ),
        (
            "weather",
            include_str!("../tests/protocol-v1/core-dashboard-weather.jsonl"),
        ),
        (
            "updates",
            include_str!("../tests/protocol-v1/core-dashboard-updates.jsonl"),
        ),
        (
            "media",
            include_str!("../tests/protocol-v1/core-dashboard-media.jsonl"),
        ),
        (
            "system",
            include_str!("../tests/protocol-v1/core-dashboard-system.jsonl"),
        ),
    ] {
        // serde_json map ordering depends on feature unification in the host.
        // Keep committed upstream bytes; compare decoded JSON object contents.
        let frames = |text: &str| {
            text.lines()
                .map(serde_json::from_str::<Json>)
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        assert_eq!(
            frames(&generated[&format!("protocol/fixtures/core-dashboard-{name}.jsonl")])?,
            frames(text)?
        );
        let mut mirror = Mirror::new(Instant::now());
        mirror.reconnected("dashboard-golden", Instant::now())?;
        for line in text.lines() {
            let frame = framing::decode(line.as_bytes()).map_err(|e| e.message)?;
            assert_eq!(
                framing::decode(&framing::encode(&frame).map_err(|e| e.message)?)
                    .map_err(|e| e.message)?,
                frame
            );
            match frame {
                Message::Snap { topic, seq, data } => {
                    assert!(checked(&topic, &data)?.valid());
                    mirror.accept(
                        Arc::new(Snapshot {
                            epoch: "dashboard-golden".into(),
                            revision: seq,
                            data: Arc::new(data),
                            connected: true,
                        }),
                        Instant::now(),
                    )?;
                }
                Message::Patch {
                    topic,
                    base,
                    seq,
                    data,
                } => {
                    assert_eq!(
                        mirror.patch("dashboard-golden", base, seq, &data, Instant::now())?,
                        Applied::Updated
                    );
                    assert!(checked(&topic, &mirror.snapshot().unwrap().data)?.valid());
                }
                // Reserved refresh calls are contract examples, not permission
                // to send them to a daemon that does not advertise the method.
                Message::Call { .. } | Message::Ret { .. } => {}
                _ => panic!("unexpected dashboard transcript frame"),
            }
            count += 1;
        }
    }
    assert_eq!(count, 16);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn preview_fixtures_use_canonical_types_and_cannot_call_services() -> Result<()> {
    let choice = Choice {
        client: None,
        fixtures: Some(root().join("tests/fixtures/dashboard")),
    };
    for page in Page::ALL {
        let source = choice.open(page)?;
        assert!(checked(&page.topic(), &source.snapshot().data)?.valid());
        let error = source
            .invoke("audio.set_volume".into(), json!({"volume":1}))
            .await
            .unwrap_err();
        assert_eq!(error.code, "unavailable");
        assert!(!error.retry);
    }
    assert!(checked("dashboard.updates", &fixture("updates"))?.valid());
    let mut malformed = fixture("system");
    malformed["processes"]["order"] = json!(["missing-entity"]);
    assert!(checked("dashboard.system", &malformed).is_err());
    malformed = fixture("weather");
    malformed["current"]["temperature"] = json!("warm");
    assert!(checked("dashboard.weather", &malformed).is_err());
    Ok(())
}

#[test]
fn unknown_weather_updates_and_hardware_values_never_become_zero() -> Result<()> {
    let unavailable = Dashboard::new(
        Page::System,
        Arc::new(ReadOnly::unavailable()?),
        None,
        false,
    )
    .project();
    assert_eq!(unavailable["dashboard.ready"], Value::Bool(false));
    assert_eq!(
        unavailable["dashboard.stale"],
        Value::Bool(false),
        "unknown is not old data"
    );
    assert_eq!(unavailable["dashboard.metric"], Value::Text("—".into()));
    let mut weather = fixture("weather");
    weather["current"]["temperature"] = Json::Null;
    weather["current"]["feels_like"] = Json::Null;
    let projection = view(Page::Weather, weather, None).project();
    assert_eq!(projection["dashboard.metric"], Value::Text("—".into()));
    let mut updates = fixture("updates");
    updates.as_object_mut().unwrap().remove("count");
    updates["health"]["stale"] = json!(true);
    let projection = view(Page::System, fixture("system"), Some(updates)).project();
    assert_eq!(
        projection["dashboard.updates"],
        Value::Text("Updates unavailable".into())
    );
    assert_eq!(
        projection["dashboard.updates_detail"],
        Value::Text("Stale data".into())
    );
    assert_eq!(projection["dashboard.metric"], Value::Text("—".into()));
    Ok(())
}

#[test]
fn dashboard_four_tabs_have_complete_state_defaults() -> Result<()> {
    for page in Page::ALL {
        let harness = Harness::load(
            &root(),
            Path::new(&format!("dashboard/{}/main.luau", page.name())),
            &root().join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 1048,
                height: 714,
                scale: 1.,
            },
        )?;
        let defaults = harness.session.stores.values();

        let unavailable =
            Dashboard::new(page, Arc::new(ReadOnly::unavailable()?), None, false).project();
        assert_eq!(unavailable["dashboard.ready"], Value::Bool(false));
        assert_eq!(unavailable["dashboard.stale"], Value::Bool(false));
        for key in unavailable
            .keys()
            .filter(|key| key.starts_with("dashboard."))
        {
            assert!(
                defaults.contains_key(key),
                "{} unavailable state key {key}",
                page.name()
            );
        }

        let primary_data = fixture(page.name());
        assert!(checked(&page.topic(), &primary_data)?.valid());
        let secondary = match page {
            Page::Today => {
                let data = fixture("weather");
                assert!(checked("dashboard.weather", &data)?.valid());
                Some(Arc::new(FakeSource::new(data)?) as Arc<dyn Source>)
            }
            Page::System => {
                let data = fixture("updates");
                assert!(checked("dashboard.updates", &data)?.valid());
                Some(Arc::new(FakeSource::new(data)?) as Arc<dyn Source>)
            }
            _ => None,
        };
        let dashboard = Dashboard::new(
            page,
            Arc::new(FakeSource::new(primary_data)?),
            secondary,
            true,
        );
        let projected = if page == Page::Today {
            let clock = Arc::new(FakeSource::new(
                json!({"clock":"10:00","date":{"year":2024,"month":2,"day":29}}),
            )?);
            let stores = bead::core::Stores::default().reconcile(&BTreeMap::from([
                ("calendar.initialized".into(), Value::Bool(true)),
                ("calendar.follow_today".into(), Value::Bool(false)),
                ("calendar.year".into(), Value::Number(2023.)),
                ("calendar.month".into(), Value::Number(12.)),
            ]))?;
            let projected = dashboard
                .with_clock(Some(clock))
                .project_with_stores(&stores);
            assert_eq!(projected["dashboard.clock"], Value::Text("10:00".into()));
            assert_eq!(
                projected["calendar.today"],
                Value::Text("2024-02-29".into())
            );
            projected
        } else {
            dashboard.project()
        };
        assert_eq!(projected["dashboard.ready"], Value::Bool(true));
        assert_eq!(projected["dashboard.stale"], Value::Bool(false));
        for key in projected.keys().filter(|key| key.starts_with("dashboard.")) {
            assert!(
                defaults.contains_key(key),
                "{} ready state key {key}",
                page.name()
            );
        }
        if page == Page::Today {
            assert!(defaults.contains_key("dashboard.clock"));
        } else {
            assert!(
                !unavailable.contains_key("dashboard.clock")
                    && !projected.contains_key("dashboard.clock"),
                "{} must not project Today-only clock state",
                page.name()
            );
            assert!(!defaults.contains_key("dashboard.clock"));
        }
    }
    Ok(())
}

#[test]
fn dashboard_navigation_routes_every_core_tab() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};

    for (index, page) in Page::ALL.into_iter().enumerate() {
        let route = format!("dashboard-{}", page.name());
        assert_eq!(Page::from_view(&route), Some(page));
        assert_eq!(page.tab(), index);

        let legacy =
            crate::commands::from_strings("nacre".into(), "tab".into(), &[index.to_string()])?;
        let mapped = crate::commands::map_views(legacy).expect("valid core dashboard tab route");
        assert_eq!(mapped.target, "bead.panel");
        assert_eq!(mapped.method, "open");
        assert_eq!(mapped.args, vec![json!(route.clone())]);
        assert_eq!(
            Page::from_view(mapped.args[0].as_str().unwrap()),
            Some(page)
        );

        let model = Dashboard::new(page, Arc::new(ReadOnly::unavailable()?), None, false).project();
        let harness = Harness::load(
            &root(),
            Path::new(&format!("dashboard/{}/main.luau", page.name())),
            &root().join("palette.json"),
            model,
            Config {
                width: 1048,
                height: 714,
                scale: 1.,
            },
        )?;
        for tab in Page::ALL {
            if tab == page {
                assert!(
                    harness.scene().primitives.iter().any(|primitive| {
                        matches!(primitive, bead::render::Primitive::Text { text, .. } if text == tab.title())
                    }),
                    "selected {} tab must render its label",
                    tab.name()
                );
                assert!(
                    !harness.session.view_nodes().iter().any(|node| {
                        node.action.as_deref() == Some("view.open")
                            && node.action_arg == Some(Value::Text(route.clone()))
                    }),
                    "selected tab is not an action"
                );
            } else {
                assert!(
                    harness.session.view_nodes().iter().any(|node| {
                        node.action.as_deref() == Some("view.open")
                            && node.action_arg
                                == Some(Value::Text(format!("dashboard-{}", tab.name())))
                    }),
                    "unselected {} tab must open its dashboard route",
                    tab.name()
                );
            }
        }

        let mut runtime = crate::reload::Runtime::new(harness.session);
        if let Some(scene) = runtime.stage(UiEvent::PanelVisibility(false))? {
            runtime.session.commit(scene);
        }
        assert_eq!(
            runtime.session.stores.get("panel.open"),
            Some(&Value::Bool(false)),
            "closing a core tab hides the panel and returns to the bar"
        );
    }
    Ok(())
}

#[test]
fn dashboard_disconnect_keeps_last_good_stale() -> Result<()> {
    let unavailable = Dashboard::new(
        Page::Weather,
        Arc::new(ReadOnly::unavailable()?),
        None,
        false,
    )
    .project();
    assert_eq!(unavailable["dashboard.ready"], Value::Bool(false));
    assert_eq!(unavailable["dashboard.stale"], Value::Bool(false));
    assert_eq!(unavailable["dashboard.metric"], Value::Text("—".into()));

    for page in Page::ALL {
        let data = fixture(page.name());
        assert!(checked(&page.topic(), &data)?.valid());
        let source = Arc::new(FakeSource::new(data)?);
        let secondary = match page {
            Page::Today => {
                let data = fixture("weather");
                assert!(checked("dashboard.weather", &data)?.valid());
                Some(Arc::new(FakeSource::new(data)?) as Arc<dyn Source>)
            }
            Page::System => {
                let data = fixture("updates");
                assert!(checked("dashboard.updates", &data)?.valid());
                Some(Arc::new(FakeSource::new(data)?) as Arc<dyn Source>)
            }
            _ => None,
        };
        let dashboard = Dashboard::new(page, source.clone(), secondary, true);
        let before = dashboard.project();
        assert_eq!(
            before["dashboard.ready"],
            Value::Bool(true),
            "{} ready",
            page.name()
        );
        assert_eq!(
            before["dashboard.stale"],
            Value::Bool(false),
            "{} fresh",
            page.name()
        );

        source.disconnect()?;
        let after = dashboard.project();
        let mut expected = before;
        expected.insert("dashboard.stale".into(), Value::Bool(true));
        assert_eq!(after, expected, "{} lost last-good projection", page.name());
    }
    Ok(())
}

#[test]
fn dashboard_long_rows_scroll_without_clipping() -> Result<()> {
    use bead_test::Input;

    let mut calendar = fixture("calendar");
    calendar["events"]["items"] = json!({});
    calendar["events"]["order"] = json!([]);
    for index in 0..24 {
        let id = format!("event-{index}");
        calendar["events"]["items"][&id] = json!({
            "title": format!("Event {index}"),
            "start": 1_800_000_000 + index,
            "time": "08:00",
            "date": "2027-01-15"
        });
        calendar["events"]["order"]
            .as_array_mut()
            .ok_or("calendar event order must be an array")?
            .push(json!(id));
    }
    let mut model = view(Page::Today, calendar, Some(fixture("weather"))).project();
    model.extend(BTreeMap::from([
        ("calendar.initialized".into(), Value::Bool(true)),
        ("calendar.ready".into(), Value::Bool(true)),
        ("calendar.today".into(), Value::Text("2024-02-29".into())),
        ("calendar.year".into(), Value::Number(2024.)),
        ("calendar.month".into(), Value::Number(2.)),
    ]));
    let mut today = Harness::load(
        &root(),
        Path::new("dashboard/today/main.luau"),
        &root().join("palette.json"),
        model,
        Config {
            width: 1048,
            height: 714,
            scale: 1.,
        },
    )?;
    today.render()?;
    let scroll = today
        .session
        .view_nodes()
        .iter()
        .find(|node| node.scroll_store.as_deref() == Some("dashboard.agenda.scroll.y"))
        .ok_or("Today agenda scroll region missing")?;
    let rect = bead::ui::rect(&scroll.draw);
    today.script((0..3).map(|_| Input::Scroll {
        x: rect.x + rect.w / 2.,
        y: rect.y + rect.h / 2.,
        delta: 400.,
    }))?;
    let (event_index, event_rect) = today
        .scene()
        .primitives
        .iter()
        .enumerate()
        .find_map(|(index, primitive)| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text == "Event 23" => {
                Some((index, rect))
            }
            _ => None,
        })
        .ok_or("last bounded agenda event is not rendered after scrolling")?;
    let clip = *today
        .scene()
        .clips
        .get(&event_index)
        .ok_or("agenda row must carry its viewport clip")?;
    assert!(
        event_rect.y < clip.y + clip.h && event_rect.y + event_rect.h > clip.y,
        "last agenda event must intersect the scrolled viewport"
    );

    let mut system_data = fixture("system");
    for index in 0..5 {
        system_data["processes"]["items"][format!("process-{index}")]["name"] =
            json!(format!("Process {index}"));
    }
    let system_model = view(Page::System, system_data, Some(fixture("updates"))).project();
    let mut system = Harness::load(
        &root(),
        Path::new("dashboard/system/main.luau"),
        &root().join("palette.json"),
        system_model,
        Config {
            width: 1048,
            height: 714,
            scale: 1.,
        },
    )?;
    system.render()?;
    let process_rect = system
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text == "Process 4" => Some(rect),
            _ => None,
        })
        .ok_or("last bounded process row is absent")?;
    assert!(process_rect.y >= 342. && process_rect.y + process_rect.h <= 554.);

    let weather_model = view(Page::Weather, fixture("weather"), None).project();
    let mut weather = Harness::load(
        &root(),
        Path::new("dashboard/weather/main.luau"),
        &root().join("palette.json"),
        weather_model,
        Config {
            width: 1048,
            height: 714,
            scale: 1.,
        },
    )?;
    weather.render()?;
    let forecast_rect = weather
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text == "2024-03-06" => Some(rect),
            _ => None,
        })
        .ok_or("last bounded daily forecast row is absent")?;
    assert!(forecast_rect.y >= 68. && forecast_rect.y + forecast_rect.h <= 630.);
    Ok(())
}

#[test]
fn dashboard_closed_page_releases_sources() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};

    for page in Page::ALL {
        let main = Arc::new(FakeSource::new(fixture(page.name()))?);
        let secondary = match page {
            Page::Today => Some(Arc::new(FakeSource::new(fixture("weather"))?)),
            Page::System => Some(Arc::new(FakeSource::new(fixture("updates"))?)),
            _ => None,
        };
        let clock = (page == Page::Today)
            .then(|| Arc::new(FakeSource::new(json!({"clock":"10:00"})).unwrap()));
        let retained_before = (
            Arc::strong_count(&main),
            secondary.as_ref().map(Arc::strong_count),
            clock.as_ref().map(Arc::strong_count),
        );
        let mut harness = Harness::load(
            &root(),
            Path::new(&format!("dashboard/{}/main.luau", page.name())),
            &root().join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 1048,
                height: 714,
                scale: 1.,
            },
        )?;
        harness.render()?;
        let mut runtime = crate::reload::Runtime::new(harness.session).with_dashboard(
            page,
            main.clone(),
            secondary
                .as_ref()
                .map(|source| source.clone() as Arc<dyn Source>),
            clock
                .as_ref()
                .map(|source| source.clone() as Arc<dyn Source>),
            false,
        )?;
        let events = Arc::new(AtomicUsize::new(0));
        let callback_events = events.clone();
        runtime.services(Box::new(move |_: UiEvent| {
            callback_events.fetch_add(1, Ordering::Relaxed);
        }));
        let scene = runtime
            .session
            .update_model(BTreeMap::new())?
            .ok_or("page Sources must materialize the page state")?;
        runtime.session.commit(scene);
        assert_eq!(
            runtime.session.stores.get("dashboard.ready"),
            Some(&Value::Bool(true)),
            "{page:?} fixture state must be live before retirement"
        );
        let subscribed = events.load(Ordering::Relaxed);
        assert!(
            subscribed > 0,
            "{page:?} page acquired source subscriptions"
        );

        // Keep every source alive while explicitly retiring the view. A shared
        // provider therefore cannot be mistaken for a leaked page owner.
        drop(runtime);
        assert_eq!(
            Arc::strong_count(&main),
            retained_before.0,
            "{page:?} page must release its primary Source lease"
        );
        assert_eq!(
            secondary.as_ref().map(Arc::strong_count),
            retained_before.1,
            "{page:?} page must release its secondary Source lease"
        );
        assert_eq!(
            clock.as_ref().map(Arc::strong_count),
            retained_before.2,
            "{page:?} page must release its clock Source lease"
        );
        main.publish(fixture(page.name()))?;
        if let Some(source) = &secondary {
            source.publish(source.snapshot().data.as_ref().clone())?;
        }
        if let Some(source) = &clock {
            source.publish(json!({"clock":"10:01"}))?;
        }
        assert_eq!(
            events.load(Ordering::Relaxed),
            subscribed,
            "{page:?} page callbacks must stop after Runtime retirement"
        );
        assert_eq!(main.calls(), 0, "dashboard pages are read-only");
    }
    Ok(())
}

#[test]
fn process_projection_preserves_ids_and_uses_memory_not_fake_cpu_percent() {
    let mut data = fixture("system");
    data["processes"]["order"] = json!([
        "process-4",
        "process-3",
        "process-2",
        "process-1",
        "process-0"
    ]);
    let projection = view(Page::System, data, None).project();
    let Value::List(rows) = &projection["dashboard.rows"] else {
        panic!("process rows");
    };
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0]["id"], "process-0");
    assert_eq!(rows[0]["value"], "320.0 MiB");
    assert_eq!(rows[0]["detail"], "1.0 s CPU");
    assert!(!serde_json::to_string(rows).unwrap().contains('%'));
}

#[test]
fn dashboard_pages_have_deterministic_pixels_and_only_navigation_actions() -> Result<()> {
    for page in Page::ALL {
        for scale in [1., 1.5] {
            let mut model = view(
                page,
                fixture(page.name()),
                match page {
                    Page::Today => Some(fixture("weather")),
                    Page::System => Some(fixture("updates")),
                    _ => None,
                },
            )
            .project();
            if page == Page::Today {
                let source = Arc::new(FakeSource::new(
                    json!({"clock":"10:00","date":{"year":2024,"month":2,"day":29}}),
                )?);
                let stores = bead::core::Stores::default().reconcile(&BTreeMap::from([(
                    "calendar.initialized".into(),
                    Value::Bool(false),
                )]))?;
                model.extend(crate::calendar::Calendar::new(source).project(&stores));
                model.insert("dashboard.clock".into(), Value::Text("10:00".into()));
            }
            let mut h = Harness::load(
                &root(),
                Path::new(&format!("dashboard/{}/main.luau", page.name())),
                &root().join("palette.json"),
                model,
                Config {
                    width: 1048,
                    height: 714,
                    scale,
                },
            )?;
            for node in h.session.view_nodes() {
                assert!(
                    matches!(
                        node.action.as_deref(),
                        None | Some("view.open" | "calendar_month")
                    ),
                    "no hidden device/update/playback executor"
                );
            }
            assert_eq!(
                h.session
                    .view_nodes()
                    .iter()
                    .filter(|node| node.action.is_some())
                    .count(),
                if page == Page::Today { 5 } else { 3 },
                "three other core pages, plus two calendar controls on Today"
            );
            assert!(
                h.session
                    .resource_counts()
                    .split_whitespace()
                    .any(|part| part
                        == if page == Page::Today {
                            "sources=7"
                        } else {
                            "sources=6"
                        }),
                "four selected-page modules plus two shared root image snapshots"
            );
            let frame = h.render()?;
            let rect = h
                .scene()
                .primitives
                .iter()
                .find_map(|p| match p {
                    bead::render::Primitive::Text { text, rect, .. } if text == "Preview data" => {
                        Some(rect)
                    }
                    _ => None,
                })
                .ok_or("fixture data must be visibly labeled")?;
            let (dx, dy) = h.scene().offset;
            let left = ((rect.x + dx) * scale).max(0.) as u32;
            let top = ((rect.y + dy) * scale).max(0.) as u32;
            let right = (((rect.x + rect.w + dx) * scale).ceil() as u32).min(frame.width);
            let bottom = (((rect.y + rect.h + dy) * scale).ceil() as u32).min(frame.height);
            let width = frame.width;
            let visible_ink = (top..bottom)
                .flat_map(|y| (left..right).map(move |x| (y * width + x) as usize * 4))
                .filter(|&i| frame.pixels[i..i + 3].iter().all(|&c| c > 100))
                .count();
            assert!(
                visible_ink > 20,
                "fixture label must have actual visible pixels"
            );
            let golden = root().join(format!(
                "tests/goldens/dashboard-{}-{scale}.png",
                page.name()
            ));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-dashboard-{}-{scale}-diff", page.name())),
            )?;
        }
    }
    Ok(())
}

#[test]
fn dashboard_source_wakes_keep_last_good_data_and_drop_with_page() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::{sync::mpsc, time::Duration};
    for page in Page::ALL {
        let source = Arc::new(FakeSource::new(fixture(page.name()))?);
        let weak = Arc::downgrade(&source);
        let mut h = Harness::load(
            &root(),
            Path::new(&format!("dashboard/{}/main.luau", page.name())),
            &root().join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 1048,
                height: 714,
                scale: 1.,
            },
        )?;
        h.render()?;
        let mut runtime = crate::reload::Runtime::new(h.session).with_dashboard(
            page,
            source.clone(),
            None,
            None,
            true,
        )?;
        let (tx, rx) = mpsc::channel();
        runtime.services(Box::new(move |event| {
            let _ = tx.send(event);
        }));
        if let Some(scene) = runtime.stage(rx.recv_timeout(Duration::from_millis(200))?)? {
            runtime.session.commit(scene);
        }
        assert!(
            runtime.stage(UiEvent::Model(BTreeMap::new()))?.is_none(),
            "unchanged source is no layout/bake"
        );
        source.disconnect()?;
        if let Some(scene) = runtime.stage(rx.recv_timeout(Duration::from_millis(200))?)? {
            runtime.session.commit(scene);
        }
        assert_eq!(
            runtime.session.stores.get("dashboard.stale"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            source.calls(),
            0,
            "views do not poll or infer service calls"
        );
        drop(source);
        drop(runtime);
        assert!(
            weak.upgrade().is_none(),
            "page/projector/subscription must not keep source alive"
        );
    }
    Ok(())
}

#[test]
fn today_uses_published_calendar_identity_and_keeps_unknown_and_empty_distinct() -> Result<()> {
    let mut data = fixture("calendar");
    let original = data["events"]["order"].as_array().unwrap().clone();
    let projection = view(Page::Today, data.clone(), Some(fixture("weather"))).project();
    let Value::List(rows) = &projection["dashboard.rows"] else {
        panic!("agenda rows")
    };
    assert_eq!(
        rows.iter().map(|r| r["id"].clone()).collect::<Vec<_>>(),
        original
    );
    assert_eq!(
        projection["dashboard.heading"],
        Value::Text("Upcoming events".into())
    );
    data["events"] = json!({"items":{},"order":[]});
    data["configured"] = json!(false);
    let projection = view(Page::Today, data.clone(), None).project();
    assert_eq!(
        projection["dashboard.detail"],
        Value::Text("Calendar is not configured".into())
    );
    data["configured"] = json!(true);
    let projection = view(Page::Today, data.clone(), None).project();
    assert_eq!(
        projection["dashboard.detail"],
        Value::Text("No upcoming events".into())
    );
    data["events"]["order"] = json!(["missing"]);
    assert!(checked("dashboard.calendar", &data).is_err());
    Ok(())
}

#[test]
fn today_clock_and_secondary_source_drop_with_page_and_preserve_browsed_month() -> Result<()> {
    let clock = Arc::new(FakeSource::new(
        json!({"clock":"10:00","date":{"year":2024,"month":2,"day":29}}),
    )?);
    let weather = Arc::new(FakeSource::new(fixture("weather"))?);
    let weak_clock = Arc::downgrade(&clock);
    let weak_weather = Arc::downgrade(&weather);
    let page = Dashboard::new(
        Page::Today,
        Arc::new(FakeSource::new(fixture("calendar"))?),
        Some(weather.clone()),
        true,
    )
    .with_clock(Some(clock.clone()));
    let stores = bead::core::Stores::default().reconcile(&BTreeMap::from([
        ("calendar.initialized".into(), Value::Bool(true)),
        ("calendar.follow_today".into(), Value::Bool(false)),
        ("calendar.year".into(), Value::Number(2023.)),
        ("calendar.month".into(), Value::Number(12.)),
    ]))?;
    let values = page.project_with_stores(&stores);
    assert_eq!(values["dashboard.clock"], Value::Text("10:00".into()));
    assert_eq!(values["calendar.today"], Value::Text("2024-02-29".into()));
    assert!(
        !values.contains_key("calendar.month"),
        "clock cannot reset a browsed month"
    );
    assert!(!values.contains_key("calendar.year"));
    weather.disconnect()?;
    assert_eq!(
        page.project_with_stores(&stores)["dashboard.stale"],
        Value::Bool(true)
    );
    let subscriptions = page.subscribe(Arc::new(|_| {}))?;
    drop(clock);
    drop(weather);
    drop(subscriptions);
    drop(page);
    assert!(weak_clock.upgrade().is_none());
    assert!(weak_weather.upgrade().is_none());
    Ok(())
}

#[test]
fn today_calendar_navigation_and_agenda_scrolling_use_real_input() -> Result<()> {
    let mut data = fixture("calendar");
    use bead_test::Input;
    data["events"]["items"] = json!({});
    data["events"]["order"] = json!([]);
    for i in 0..40 {
        let id = format!("event-{i}");
        data["events"]["items"][&id] = json!({"title":format!("Event {i}"),"start":1800000000+i,"time":"08:00","date":"2027-01-15"});
        data["events"]["order"]
            .as_array_mut()
            .unwrap()
            .push(json!(id));
    }
    let mut model = view(Page::Today, data, None).project();
    model.extend(BTreeMap::from([
        ("calendar.initialized".into(), Value::Bool(true)),
        ("calendar.ready".into(), Value::Bool(true)),
        ("calendar.today".into(), Value::Text("2024-02-29".into())),
        ("calendar.year".into(), Value::Number(2024.)),
        ("calendar.month".into(), Value::Number(2.)),
    ]));
    let mut h = Harness::load(
        &root(),
        Path::new("dashboard/today/main.luau"),
        &root().join("palette.json"),
        model,
        Config {
            width: 1048,
            height: 714,
            scale: 1.,
        },
    )?;
    h.render()?;
    let next = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.label == "Next month")
        .ok_or("next month")?;
    let r = bead::ui::rect(&next.draw);
    h.script([Input::Click {
        x: r.x + r.w / 2.,
        y: r.y + r.h / 2.,
    }])?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(3.))
    );
    assert_eq!(
        h.session.stores.get("calendar.today"),
        Some(&Value::Text("2024-02-29".into()))
    );
    let previous = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.label == "Previous month")
        .ok_or("previous month")?;
    let r = bead::ui::rect(&previous.draw);
    h.script([Input::Click {
        x: r.x + r.w / 2.,
        y: r.y + r.h / 2.,
    }])?;
    assert_eq!(
        h.session.stores.get("calendar.month"),
        Some(&Value::Number(2.))
    );
    let scroll = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.scroll_store.as_deref() == Some("dashboard.agenda.scroll.y"))
        .ok_or("agenda scrolling")?;
    let r = bead::ui::rect(&scroll.draw);
    h.script([Input::Scroll {
        x: r.x + r.w / 2.,
        y: r.y + r.h / 2.,
        delta: 200.,
    }])?;
    assert!(
        matches!(h.session.stores.get("dashboard.agenda.scroll.y"),Some(Value::Number(value)) if *value>0.)
    );
    // Calendar/date/palette is presentation only; events cannot execute labels.
    assert!(
        h.session
            .view_nodes()
            .iter()
            .filter_map(|n| n.action.as_deref())
            .all(|a| matches!(a, "view.open" | "calendar_month"))
    );
    Ok(())
}

fn extra_secondary(page: Page) -> Option<Json> {
    (page == Page::Devices).then(|| fixture("audio"))
}
pub(super) fn extra_harness(page: Page, model: BTreeMap<String, Value>, scale: f32) -> Result<Harness> {
    Harness::load(
        &root(),
        Path::new(&format!("dashboard/{}/main.luau", page.name())),
        &root().join("palette.json"),
        model,
        Config {
            width: 1048,
            height: 714,
            scale,
        },
    )
}

#[test]
fn dashboard_extras_map_nacre_indices_and_refuse_unpublished_pages() -> Result<()> {
    for (index, page) in [
        (5, Page::Notifications),
        (7, Page::Devices),
        (8, Page::Network),
    ] {
        assert_eq!(Page::from_tab(index), Some(page));
        assert_eq!(page.tab(), index);
        assert_eq!(
            Page::from_view(&format!("dashboard-{}", page.name())),
            Some(page)
        );
        let call =
            crate::commands::from_strings("nacre".into(), "tab".into(), &[index.to_string()])?;
        let mapped = crate::commands::map_views(call).expect("published extra routes");
        assert_eq!(
            mapped.args,
            vec![json!(format!("dashboard-{}", page.name()))]
        );
    }
    // Only the twelve registry slots exist; the B0027 pages are covered separately.
    for index in [12, 13] {
        assert_eq!(Page::from_tab(index), None);
        let call =
            crate::commands::from_strings("nacre".into(), "tab".into(), &[index.to_string()])?;
        assert!(crate::commands::map_views(call).is_err());
    }
    Ok(())
}

#[test]
fn dashboard_extras_have_complete_state_defaults_and_project_rows() -> Result<()> {
    for page in Page::EXTRAS {
        let harness = extra_harness(page, BTreeMap::new(), 1.)?;
        let defaults = harness.session.stores.values();
        let unavailable =
            Dashboard::new(page, Arc::new(ReadOnly::unavailable()?), None, false).project();
        assert_eq!(unavailable["dashboard.ready"], Value::Bool(false));
        assert!(!unavailable.contains_key("dashboard.clock"));

        let data = fixture(page.name());
        assert!(checked(&page.topic(), &data)?.valid());
        let secondary = extra_secondary(page);
        if let Some(audio) = &secondary {
            assert!(checked("audio", audio)?.valid());
        }
        let projected = view(page, data, secondary).project();
        assert_eq!(projected["dashboard.ready"], Value::Bool(true));
        assert_eq!(projected["dashboard.stale"], Value::Bool(false));
        for key in projected.keys().filter(|key| key.starts_with("dashboard.")) {
            assert!(
                defaults.contains_key(key),
                "{} state key {key}",
                page.name()
            );
        }
        let rows = |key: &str| match &projected[key] {
            Value::List(list) => list.len(),
            other => panic!("{other:?}"),
        };
        match page {
            // Active first, then history in service order.
            Page::Notifications => assert_eq!(rows("dashboard.rows"), 3),
            // Unpaired devices are hidden; connected devices sort first.
            Page::Devices => {
                assert_eq!(rows("dashboard.rows"), 2);
                assert_eq!(rows("dashboard.secondary"), 2, "outputs and inputs only");
                assert_eq!(
                    projected["dashboard.summary"],
                    Value::Text("Powered on".into())
                );
            }
            Page::Network => {
                assert_eq!(rows("dashboard.rows"), 2);
                assert_eq!(rows("dashboard.secondary"), 2);
                assert_eq!(projected["dashboard.metric"], Value::Text("72%".into()));
                assert_eq!(
                    projected["dashboard.heading"],
                    Value::Text("Preview network".into())
                );
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[test]
fn dashboard_extras_keep_unknown_and_invalid_data_unavailable_and_stale_on_disconnect() -> Result<()>
{
    for page in Page::EXTRAS {
        let invalid = view(
            page,
            json!({"health":{"available":true,"stale":false}}),
            None,
        )
        .project();
        assert_eq!(invalid["dashboard.ready"], Value::Bool(false), "{page:?}");
        let mut stale = fixture(page.name());
        stale["health"]["stale"] = json!(true);
        let projected = view(page, stale, extra_secondary(page)).project();
        assert_eq!(projected["dashboard.stale"], Value::Bool(true), "{page:?}");
        let source = Arc::new(FakeSource::new(fixture(page.name()))?);
        let dashboard = Dashboard::new(page, source.clone(), None, true);
        let before = dashboard.project();
        source.disconnect()?;
        let mut after = dashboard.project();
        assert_eq!(
            after.insert("dashboard.stale".into(), Value::Bool(false)),
            Some(Value::Bool(true))
        );
        assert_eq!(before, after, "{page:?} keeps the last good projection");
    }
    // Network without a primary connection or active AP never invents a signal.
    let mut data = fixture("network");
    data["primary_connection"] = Json::Null;
    data["access_points"]["items"]["ap1"]["active"] = json!(false);
    let projected = view(Page::Network, data, None).project();
    assert_eq!(projected["dashboard.metric"], Value::Text("—".into()));
    assert_eq!(
        projected["dashboard.heading"],
        Value::Text("Not connected".into())
    );
    Ok(())
}

#[test]
fn dashboard_extras_release_sources_on_retirement() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    use std::sync::atomic::{AtomicUsize, Ordering};
    for page in Page::EXTRAS {
        let main = Arc::new(FakeSource::new(fixture(page.name()))?);
        let secondary = extra_secondary(page).map(|d| Arc::new(FakeSource::new(d).unwrap()));
        let before = (
            Arc::strong_count(&main),
            secondary.as_ref().map(Arc::strong_count),
        );
        let mut harness = extra_harness(page, BTreeMap::new(), 1.)?;
        harness.render()?;
        let mut runtime = crate::reload::Runtime::new(harness.session).with_dashboard(
            page,
            main.clone(),
            secondary.as_ref().map(|s| s.clone() as Arc<dyn Source>),
            None,
            false,
        )?;
        let events = Arc::new(AtomicUsize::new(0));
        let counter = events.clone();
        runtime.services(Box::new(move |_: UiEvent| {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        let scene = runtime
            .session
            .update_model(BTreeMap::new())?
            .ok_or("page state")?;
        runtime.session.commit(scene);
        assert_eq!(
            runtime.session.stores.get("dashboard.ready"),
            Some(&Value::Bool(true))
        );
        let subscribed = events.load(Ordering::Relaxed);
        assert!(subscribed > 0);
        drop(runtime);
        assert_eq!(
            (
                Arc::strong_count(&main),
                secondary.as_ref().map(Arc::strong_count)
            ),
            before
        );
        main.publish(fixture(page.name()))?;
        assert_eq!(
            events.load(Ordering::Relaxed),
            subscribed,
            "{page:?} callbacks stop"
        );
        assert_eq!(main.calls(), 0, "extras are read-only");
    }
    Ok(())
}

#[test]
fn dashboard_extras_have_deterministic_pixels_and_only_navigation_actions() -> Result<()> {
    for page in Page::EXTRAS {
        for scale in [1., 1.5] {
            let model = view(page, fixture(page.name()), extra_secondary(page)).project();
            let mut h = extra_harness(page, model, scale)?;
            for node in h.session.view_nodes() {
                assert!(
                    matches!(node.action.as_deref(), None | Some("view.open")),
                    "extras execute nothing"
                );
            }
            assert_eq!(
                h.session
                    .view_nodes()
                    .iter()
                    .filter(|n| n.action.is_some())
                    .count(),
                4,
                "four core tabs; the selected extra tab is not an action"
            );
            let frame = h.render()?;
            assert!(h.scene().primitives.iter().any(|p| matches!(
                p, bead::render::Primitive::Text { text, .. } if text == page.title())));
            let golden = root().join(format!(
                "tests/goldens/dashboard-{}-{scale}.png",
                page.name()
            ));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&golden)?;
            }
            frame.compare(
                &golden,
                Default::default(),
                &std::env::temp_dir().join(format!("bead-dashboard-{}-{scale}-diff", page.name())),
            )?;
        }
    }
    Ok(())
}

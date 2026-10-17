// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead_test::{Config, Harness, Result, source::FakeSource};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn fixture(name: &str) -> Json {
    serde_json::from_slice(
        &std::fs::read(root().join(format!("tests/fixtures/dashboard/{name}.json"))).unwrap(),
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
    assert_eq!(count, 12);
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
            let model = view(
                page,
                fixture(page.name()),
                if page == Page::System {
                    Some(fixture("updates"))
                } else {
                    None
                },
            )
            .project();
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
                assert_eq!(
                    node.action.as_deref(),
                    Some("view.open"),
                    "no hidden device/update/playback executor"
                );
            }
            assert_eq!(
                h.session.view_nodes().len(),
                2,
                "two other available pages; Today is unimplemented"
            );
            assert!(
                h.session
                    .resource_counts()
                    .split_whitespace()
                    .any(|part| part == "sources=6"),
                "four selected-page modules plus two shared root image snapshots"
            );
            let frame = h.render()?;
            let rect = h
                .scene()
                .primitives
                .iter()
                .find_map(|p| match p {
                    bead::render::Primitive::Text { text, rect, .. } if text == "Preview data" => {
                        Some(*rect)
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
            view(page, fixture(page.name()), None).project(),
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

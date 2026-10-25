// SPDX-License-Identifier: GPL-3.0-only
use super::tests::{fixture, root};
use super::*;
use bead_test::{Config, Harness, Result, source::FakeSource};

#[test]
fn today_companion_fallback_surface_matches_joined_frame_geometry() -> Result<()> {
    use bead::render::{Palette, Primitive};
    use bead::wayland::{SceneSource, UiEvent};

    let palette_path = root().join("palette.json");
    let colors = Arc::new(Palette::read(&palette_path)?);
    let harness = Harness::load(
        &root(),
        Path::new("dashboard/today/main.luau"),
        &palette_path,
        BTreeMap::new(),
        Config {
            width: 1048,
            height: 714,
            scale: 1.5,
        },
    )?;
    let dashboard = Dashboard::new(
        Page::Today,
        Arc::new(FakeSource::new(fixture("calendar"))?),
        None,
        true,
    );
    let mut runtime = crate::reload::Runtime::new(harness.session)
        .with_dashboard_view(dashboard)?
        .with_frame(
            crate::placement::Attachment::Attached(bead::core::Edge::Top),
            &colors,
        )?;
    let prepared = runtime
        .stage(UiEvent::Reload)?
        .ok_or("decorated Today runtime should produce its first scene")?;
    let spec = super::spec(
        Page::Today,
        Arc::new(palette_path),
        colors,
        Choice {
            client: None,
            fixtures: None,
            prefs: None,
        },
        Arc::new(std::sync::OnceLock::new()),
        false,
    )?;

    let fallback = &spec.fallback;
    assert_eq!(fallback.width, prepared.width);
    assert_eq!(fallback.bounds, prepared.bounds);
    assert_eq!(fallback.offset, prepared.offset);
    let fallback_surface_height = fallback.bounds.y + fallback.bounds.h + fallback.offset.1;
    let prepared_surface_height = prepared.bounds.y + prepared.bounds.h + prepared.offset.1;
    assert_eq!(fallback_surface_height, prepared_surface_height);
    let fallback_body = fallback
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            Primitive::Rounded { rect, .. } if rect.x == 0. && rect.y == 0. => Some(*rect),
            _ => None,
        });
    assert_eq!(fallback_body, Some(crate::placement::body(&prepared)));
    assert_eq!(fallback_surface_height, 714.);
    assert_eq!(crate::placement::body(&prepared).h, 650.);
    Ok(())
}

#[test]
fn today_projects_host_storage_and_releases_overview_sources_and_poster() -> Result<()> {
    use nacre_protocol::wallpaper::{ActiveWallpaper, WallpaperFileIdentity};
    let dir = std::env::temp_dir().join(format!("bead-today-poster-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let image_path = dir.join("poster.png");
    image::RgbaImage::from_pixel(32, 32, image::Rgba([80, 130, 180, 255])).save(&image_path)?;
    let mut pair: ActiveWallpaper = serde_json::from_value(
        json!({"revision":1,"palette_revision":1,
        "path":image_path,"file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":0,"mtime_nsec":0,"ctime_sec":0,"ctime_nsec":0},
        "presentation":"crop","palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}}),
    )?;
    pair.file_identity = WallpaperFileIdentity::from_metadata(&std::fs::metadata(&image_path)?);
    pair.presentation = nacre_protocol::wallpaper::Presentation::Fit;
    let wallpaper = Arc::new(FakeSource::new(
        json!({"available":true,"stale":false,"revision":1,
        "current":pair,"outputs":{"fixture":pair}}),
    )?);
    let mut system_data = fixture("system");
    system_data["host"] =
        json!({"user":"Fixture user","os":"Fixture Linux","kernel":"6.10","uptime_s":3660});
    let system = Arc::new(FakeSource::new(system_data)?);
    let updates = Arc::new(FakeSource::new(fixture("updates"))?);
    let dashboard = Dashboard::new(
        Page::Today,
        Arc::new(FakeSource::new(fixture("calendar"))?),
        None,
        true,
    )
    .with_overview(
        Some(system.clone()),
        Some(wallpaper.clone()),
        Some(updates.clone()),
    );
    let mut h = Harness::load(
        &root(),
        Path::new("dashboard/today/main.luau"),
        &root().join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 1048,
            height: 714,
            scale: 1.5,
        },
    )?;
    let values = dashboard.project_with_stores(&h.session.stores);
    assert_eq!(
        values["dashboard.wallpaper"],
        Value::Bool(false),
        "first projection returns the layout without waiting for image decoding"
    );
    let finish = |stores: &bead::core::Stores| {
        dashboard
            .poster_job
            .borrow_mut()
            .as_mut()
            .unwrap()
            .thread
            .take()
            .unwrap()
            .join()
            .unwrap();
        dashboard.project_with_stores(stores)
    };
    let values = finish(&h.session.stores);
    assert_eq!(values["dashboard.wallpaper"], Value::Bool(true));
    let held = dashboard.poster.borrow();
    let poster = held.as_ref().unwrap();
    assert_eq!(
        poster.0.presentation,
        nacre_protocol::wallpaper::Presentation::Fit
    );
    let pixels = image::load_from_memory(poster.3.as_ref().unwrap())?.to_rgba8();
    assert_eq!(
        pixels.get_pixel(0, 0).0,
        [80, 130, 180, 255],
        "the hero crops a square poster instead of inheriting desktop fit letterboxing"
    );
    drop(held);
    let Value::List(host) = &values["dashboard.host"] else {
        panic!("host rows")
    };
    assert!(host.iter().any(|row| row["text"] == "Fixture Linux"));
    let Value::List(gauges) = &values["dashboard.gauges"] else {
        panic!("gauges")
    };
    assert_eq!(gauges[2]["id"], "storage");
    assert_eq!(gauges[2]["label"], "Storage");
    let mut visible = values.clone();
    visible.insert("dashboard.wallpaper".into(), Value::Bool(false));
    let scene = h.session.update_model(visible)?.ok_or("Today projection")?;
    let text_y = |wanted: &str| {
        scene
            .primitives
            .iter()
            .find_map(|p| match p {
                bead::render::Primitive::Text { text, rect, .. } if text == wanted => Some(rect.y),
                _ => None,
            })
            .expect("host text in layout")
    };
    let Value::Text(update_text) = &values["dashboard.updates"] else {
        panic!("updates text")
    };
    assert!(
        text_y(update_text) < text_y("6.10"),
        "updates precede the kernel in the QML host card"
    );

    let bytes = Arc::downgrade(
        dashboard
            .poster
            .borrow()
            .as_ref()
            .unwrap()
            .3
            .as_ref()
            .unwrap(),
    );
    dashboard.project_with_stores(&h.session.stores);
    assert_eq!(
        bytes.strong_count(),
        1,
        "repeat projection reuses the bounded poster"
    );
    wallpaper.disconnect()?;
    assert_eq!(
        dashboard.project_with_stores(&h.session.stores)["dashboard.wallpaper"],
        Value::Bool(false)
    );
    assert!(bytes.upgrade().is_none(), "disconnect releases the poster");
    wallpaper.publish((*wallpaper.snapshot().data).clone())?;
    dashboard.project_with_stores(&h.session.stores);
    finish(&h.session.stores);
    let closing = Arc::downgrade(
        dashboard
            .poster
            .borrow()
            .as_ref()
            .unwrap()
            .3
            .as_ref()
            .unwrap(),
    );
    drop(dashboard);
    assert!(
        closing.upgrade().is_none(),
        "closing releases a connected poster"
    );
    assert_eq!(Arc::strong_count(&system), 1);
    assert_eq!(Arc::strong_count(&wallpaper), 1);
    assert_eq!(Arc::strong_count(&updates), 1);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

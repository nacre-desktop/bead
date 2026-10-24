// SPDX-License-Identifier: GPL-3.0-only
use super::tests::{fixture, root};
use super::*;
use bead_test::{Config, Harness, Result, source::FakeSource};

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

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
    let h = Harness::load(
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
    let Value::List(host) = &values["dashboard.host"] else {
        panic!("host rows")
    };
    assert!(host.iter().any(|row| row["text"] == "Fixture Linux"));
    let Value::List(gauges) = &values["dashboard.gauges"] else {
        panic!("gauges")
    };
    assert_eq!(gauges[2]["id"], "storage");
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

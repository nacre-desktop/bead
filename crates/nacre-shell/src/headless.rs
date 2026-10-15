// SPDX-License-Identifier: GPL-3.0-only
use bead::render::{Palette, Rect, Scene};
use std::path::Path;
pub fn check(file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = file.canonicalize()?;
    let root = file
        .ancestors()
        .skip(1)
        .find(|p| p.join("palette.json").is_file())
        .ok_or("UI bundle needs ancestor palette.json")?;
    let palette_path = root.join("palette.json");
    let palette = Palette::read(&palette_path)?;
    let fallback = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: 1.,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 1., 1.),
        primitives: vec![],
    };
    let mut session = bead::ui::Session::new(
        root.into(),
        file.strip_prefix(root)?.into(),
        palette_path,
        palette,
        fallback,
    );
    session.stores = session
        .stores
        .reconcile(&crate::desktop::values(&Default::default(), ""))?;
    session.stores = session
        .stores
        .reconcile(&std::collections::BTreeMap::from([
            ("launcher.catalog".into(), bead::core::Value::List(vec![])),
            ("bead.workspaces".into(), bead::core::Value::List(vec![])),
            (
                "bead.focused_title".into(),
                bead::core::Value::Text(String::new()),
            ),
        ]))?;
    let scene = session.reload()?;
    session.check_sources()?;
    session.commit(scene);
    println!("CHECK_OK {}", session.resource_counts());
    Ok(())
}

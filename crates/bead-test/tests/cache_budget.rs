// SPDX-License-Identifier: MIT
//! Actual UI/value/raster churn plus cache admission: no compositor or driver.
use bead_core::{Value, cache};
use bead_render::{CpuRenderer, Primitive, Rect, Renderer};
use bead_test::{Config, Harness, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn bounded() -> usize {
    let r = cache::report();
    assert_eq!(
        r.used_bytes,
        r.caches.iter().map(|c| c.used_bytes).sum::<usize>()
    );
    assert!(
        r.used_bytes <= r.limit_bytes,
        "process cache admission failed"
    );
    assert!(
        r.caches.iter().all(|c| c.used_bytes <= c.limit_bytes),
        "owner cache admission failed"
    );
    r.caches
        .iter()
        .filter(|c| c.name == "legacy-cpu-textures")
        .map(|c| c.used_bytes)
        .max()
        .unwrap_or(0)
}
#[test]
fn real_ui_and_legacy_rasters_remain_byte_bounded() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut h = Harness::load(
        &root,
        Path::new("main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([("query".into(), Value::Text("fixture".into()))]),
        Config {
            width: 384,
            height: 244,
            scale: 1.,
        },
    )?;
    for i in 0..32 {
        h.update(BTreeMap::from([(
            "query".into(),
            Value::Text(format!("query {i}")),
        )]))?;
        h.render()?;
        bounded();
    }
    let mut scene = h.scene().clone();
    scene.layers.clear();
    scene.prepared.clear();
    scene.clips.clear();
    scene.primitives = vec![Primitive::Text {
        weight: 400,
        align: None,
        rect: Rect::new(0., 0., 512., 256.),
        text: "fixture".into(),
        family: "Bead Test Sans".into(),
        size: 16.,
        color: [255; 4],
    }];
    let mut renderer = CpuRenderer::default();
    let mut high = 0;
    for i in 0..160 {
        if let Primitive::Text { text, .. } = &mut scene.primitives[0] {
            *text = format!("fixture {i}");
        }
        renderer.prepare(&scene, 1.)?;
        high = high.max(bounded());
    }
    assert!(
        high > 60 * cache::MIB,
        "fixture must fill and evict the legacy byte cap"
    );
    drop(renderer);
    bounded();
    Ok(())
}
#[test]
fn byte_lru_preserves_recent_values_under_repeated_pressure() {
    let mut lru = cache::Lru::new("stress-lru", 4096);
    for i in 0..256 {
        lru.insert(i, vec![0; 1024], 1024).unwrap();
        assert!(lru.get(&i).is_some());
        assert!(lru.bytes() <= 4096);
        if i >= 4 {
            assert!(lru.get(&(i - 4)).is_none());
        }
        bounded();
    }
    lru.get(&252);
    lru.insert(256, vec![0; 1024], 1024).unwrap();
    assert!(lru.get(&252).is_some());
    assert!(lru.get(&253).is_none());
}

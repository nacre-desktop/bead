// SPDX-License-Identifier: MIT
use bead_core::Value;
use bead_test::{Budget, Config, FrameTime, Harness, Input, Result, Tolerance};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn test() -> Result<Harness> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Harness::load(
        &root,
        Path::new("main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([("query".into(), Value::Text("fixture".into()))]),
        Config {
            width: 384,
            height: 244,
            scale: 1.,
        },
    )
}
#[test]
fn png_input_accessibility_and_immutable_golden() -> Result<()> {
    let mut test = test()?;
    test.assert_accessible("Apply", accesskit::Role::Button)?;
    assert_eq!(
        test.session.stores.get("query"),
        Some(&Value::Text("fixture".into()))
    );
    let search = test
        .session
        .view_nodes()
        .iter()
        .find(|n| n.input.is_some())
        .ok_or("missing input")?;
    let r = bead_ui::rect(&search.draw);
    test.script([
        Input::Pointer {
            x: r.x + 2.,
            y: r.y + 2.,
        },
        Input::Click {
            x: r.x + 2.,
            y: r.y + 2.,
        },
        Input::Text("!".into()),
        Input::Backspace,
    ])?;
    assert_eq!(
        test.session.stores.get("query"),
        Some(&Value::Text("fixture".into()))
    );
    let button = test
        .session
        .view_nodes()
        .iter()
        .find(|n| n.action.is_some())
        .ok_or("missing button")?;
    let r = bead_ui::rect(&button.draw);
    test.script([
        Input::Click {
            x: r.x + 2.,
            y: r.y + 2.,
        },
        Input::Tab { backwards: true },
    ])?;
    assert_eq!(
        test.session.stores.get("count"),
        Some(&Value::Text("1".into()))
    );
    let tree = test.accessibility();
    let focused = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == tree.focus)
        .ok_or("missing focused node")?;
    assert_eq!(focused.1.role(), accesskit::Role::TextInput);
    assert_eq!(focused.1.value(), Some("fixture"));
    assert!(focused.1.supports_action(accesskit::Action::SetValue));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let frame = test.render()?;
    let golden = root.join("tests/goldens/controls.png");
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&golden)?;
    }
    let artifacts = std::env::var_os("BEAD_TEST_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("bead-test-controls"));
    frame.compare(&golden, Tolerance::default(), &artifacts)?;
    Ok(())
}
#[test]
fn transform_opacity_replay_reuses_layout_and_rasters() -> Result<()> {
    let mut test = test()?;
    let offset = test.scene().offset;
    test.pose(offset, 0.999)?; // one group composite at transition start
    let before = test.counters();
    for i in 0..120 {
        let t = i as f32 / 120.;
        test.pose((offset.0 + t * 12., offset.1 + t * 6.), 0.1 + t * 0.8)?;
    }
    assert_eq!(
        test.counters(),
        before,
        "presentation poses must not layout or bake"
    );
    println!(
        "COUNTER_PROOF frames=120 layout_delta={} raster_delta={} warm_rasters={}",
        test.counters().layouts - before.layouts,
        test.counters().rasters - before.rasters,
        before.rasters
    );
    test.update(BTreeMap::from([(
        "query".into(),
        Value::Text("changed content".into()),
    )]))?;
    assert!(
        test.counters().rasters > before.rasters,
        "the counter must detect actual text rasterization"
    );
    Ok(())
}
#[test]
fn golden_diff_and_frame_replay_fail_for_real_regressions() -> Result<()> {
    let mut test = test()?;
    let frame = test.render()?;
    let dir = std::env::temp_dir().join(format!("bead-diff-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let golden = dir.join("expected.png");
    frame.save(&golden)?;
    let mut wrong = test.render()?;
    wrong.pixels.fill(255);
    assert!(wrong.compare(&golden, Tolerance::default(), &dir).is_err());
    assert!(dir.join("diff.png").is_file());
    assert!(dir.join("actual.png").is_file());
    std::fs::remove_dir_all(dir)?;
    let good = [FrameTime {
        input_ns: 1_000_000,
        presented_ns: 9_000_000,
        cpu_ns: 2_000_000,
    }; 100];
    let budget = Budget {
        presented_p99_ms: 16.7,
        cpu_per_frame_ms: 8.,
    };
    bead_test::replay(&good, budget)?;
    let mut bad = good;
    bad[0].cpu_ns = 900_000_000;
    assert!(bead_test::replay(&bad, budget).is_err());
    bad = good;
    bad[0].presented_ns = 30_000_000;
    bad[1].presented_ns = 30_000_000;
    assert!(bead_test::replay(&bad, budget).is_err());
    bad = good;
    bad[0].presented_ns = 0;
    assert!(bead_test::replay(&bad, budget).is_err());
    assert!(bead_test::replay(&[], budget).is_err());
    Ok(())
}
#[test]
fn opaque_transform_audit_reports_direct_composition_without_new_content_bakes() -> Result<()> {
    let mut test = test()?;
    let offset = test.scene().offset;
    let before = test.counters();
    for i in 0..120 {
        test.pose((offset.0 + i as f32 * 0.05, offset.1), 1.)?;
    }
    let after = test.counters();
    assert_eq!(after.layouts, before.layouts);
    assert_eq!(after.rasters, before.rasters);
    assert_eq!(after.direct_composites - before.direct_composites, 120);
    println!(
        "OPAQUE_TRANSFORM_AUDIT frames=120 layout_delta=0 content_bakes=0 direct_composites=120"
    );
    Ok(())
}

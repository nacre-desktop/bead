// SPDX-License-Identifier: MIT
use bead_core::Value;
use bead_test::{Budget, Config, FrameTime, Harness, Input, Result, Tolerance};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
fn test() -> Result<Harness> {
    test_at_scale(1.)
}
fn test_at_scale(scale: f32) -> Result<Harness> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Harness::load(
        &root,
        Path::new("main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([("query".into(), Value::Text("fixture".into()))]),
        Config {
            width: 384,
            height: 244,
            scale,
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
    test.presentation_motion(true);
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

#[test]
fn opaque_motion_uses_measured_paths_and_releases_panel_group() -> Result<()> {
    let mut test = test_at_scale(1.5)?;
    let offset = test.scene().offset;
    let direct = test.pose(offset, 1.)?;
    test.opaque_composition(bead_render::tree::OpaqueComposition::Cached); // explicit comparison path
    let cached = test.pose(offset, 1.)?;
    let max = direct
        .pixels
        .iter()
        .zip(&cached.pixels)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(max <= 2, "opaque group pixel difference {max}");
    let warm = test.counters();
    for horizontal in [false, true] {
        for i in 0..120 {
            let delta = (i % 8) as f32 / 1.5;
            let pose = if horizontal {
                (offset.0 + delta, offset.1)
            } else {
                (offset.0, offset.1 + delta)
            };
            test.pose(pose, 1.)?;
        }
        assert_eq!(
            test.counters(),
            warm,
            "opaque poses must reuse the group without layout/content baking"
        );
    }
    test.opaque_composition(bead_render::tree::OpaqueComposition::Auto);
    test.pose(offset, 1.)?; // default direct path releases the temporary group
    let resting = test.counters();
    assert_eq!(resting.rasters, warm.rasters);
    assert_eq!(resting.direct_composites, warm.direct_composites + 1);
    test.opaque_composition(bead_render::tree::OpaqueComposition::Cached);
    test.pose(offset, 1.)?;
    assert_eq!(
        test.counters().rasters,
        resting.rasters + 1,
        "idle group must have been released"
    );
    Ok(())
}

#[test]
fn static_text_has_stable_accesskit_content_without_pointer_or_focus_nodes() -> Result<()> {
    let mut h = test()?;
    h.assert_accessible("Bead foundation", accesskit::Role::Label)?;
    let before = h.accessibility();
    let count = before
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Label && n.value() == Some("0"))
        .ok_or("missing static count")?
        .0;
    let heading = h
        .scene()
        .primitives
        .iter()
        .find_map(|p| match p {
            bead_render::Primitive::Text { rect, text, .. } if text == "Bead foundation" => {
                Some(*rect)
            }
            _ => None,
        })
        .ok_or("missing heading")?;
    h.script([Input::Click {
        x: heading.x + 1.,
        y: heading.y + 1.,
    }])?;
    assert_eq!(
        h.session.stores.get("count"),
        Some(&Value::Text("0".into()))
    );
    assert!(
        !h.session
            .view_nodes()
            .iter()
            .any(|n| n.label == "Bead foundation")
    );
    let button = h
        .session
        .view_nodes()
        .iter()
        .find(|n| n.action.is_some())
        .ok_or("button")?;
    let r = bead_ui::rect(&button.draw);
    h.script([Input::Click {
        x: r.x + 2.,
        y: r.y + 2.,
    }])?;
    let after = h.accessibility();
    assert!(
        after
            .nodes
            .iter()
            .any(|(id, n)| *id == count && n.value() == Some("1"))
    );
    for (_, node) in after
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == accesskit::Role::Label)
    {
        assert!(!node.supports_action(accesskit::Action::Focus));
        assert!(!node.supports_action(accesskit::Action::Click));
    }
    let ids: std::collections::BTreeSet<_> = after.nodes.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids.len(), after.nodes.len());
    let children: Vec<_> = after
        .nodes
        .iter()
        .flat_map(|(_, n)| n.children().iter().copied())
        .collect();
    assert!(children.iter().all(|id| ids.contains(id)));
    assert_eq!(
        children
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        children.len()
    );
    Ok(())
}

#[test]
fn static_text_bounds_follow_clip_and_offset_and_decorative_text_is_omitted() -> Result<()> {
    let h = test()?;
    let mut scene = h.scene().clone();
    let (index, r) = scene
        .primitives
        .iter()
        .enumerate()
        .find_map(|(i, p)| match p {
            bead_render::Primitive::Text { rect, text, .. } if text == "Bead foundation" => {
                Some((i, *rect))
            }
            _ => None,
        })
        .ok_or("heading")?;
    let clip = bead_render::Rect::new(r.x + 3., r.y + 2., r.w / 2., r.h / 2.);
    scene.clips.insert(index, clip);
    scene.offset = (5., 7.);
    let tree =
        bead_ui::accessibility::tree_with_scene(h.session.view_nodes(), &h.session.stores, &scene);
    let label = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Label && n.value() == Some("Bead foundation"))
        .ok_or("label")?;
    assert_eq!(
        label.1.bounds(),
        Some(accesskit::Rect {
            x0: f64::from(clip.x + 5.),
            y0: f64::from(clip.y + 7.),
            x1: f64::from(clip.x + clip.w + 5.),
            y1: f64::from(clip.y + clip.h + 7.)
        })
    );
    assert!(
        !tree
            .nodes
            .iter()
            .any(|(_, n)| n.role() == accesskit::Role::Label && n.value() == Some("Apply")),
        "control's own label duplicated"
    );
    for mode in 0..3 {
        let mut hidden = scene.clone();
        if let bead_render::Primitive::Text { color, family, .. } = &mut hidden.primitives[index] {
            if mode == 0 {
                color[3] = 0;
            } else if mode == 1 {
                *family = "Material Symbols Rounded".into();
            }
        }
        if mode == 2 {
            hidden
                .clips
                .insert(index, bead_render::Rect::new(r.x + r.w + 10., r.y, 1., 1.));
        }
        let tree = bead_ui::accessibility::tree_with_scene(
            h.session.view_nodes(),
            &h.session.stores,
            &hidden,
        );
        assert!(!tree.nodes.iter().any(
            |(_, n)| n.role() == accesskit::Role::Label && n.value() == Some("Bead foundation")
        ));
    }
    Ok(())
}

// SPDX-License-Identifier: GPL-3.0-only
//! Real launcher widgets against accepted-source goldens, not a DSL imitation.
use bead::core::Value;
use bead_test::{Config, Harness, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn harness(query: &str, scroll: f64) -> Result<Harness> {
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let selected = std::env::var_os("BEAD_ROW_REFERENCE_BUNDLE");
    if selected.is_some() && !update {
        return Err("reference bundle requires explicit golden capture".into());
    }
    let root = selected.map(PathBuf::from).unwrap_or_else(root);
    let rows=(0..320).map(|i|serde_json::json!({"id":format!("app-{i}"),"name":format!("Example application {i}"),"description":format!("Description {i}"),"icon":"fixture","selected":i==0,"disabled":i%3==2,"emoji":if i%4==0 {"A"}else{""}})).collect();
    Harness::load_with_icons(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("launcher.apps".into(), Value::list(rows)),
            ("launcher.result_count".into(), Value::Number(320.)),
            ("launcher.search.text".into(), Value::Text(query.into())),
            ("launcher.scroll.y".into(), Value::Number(scroll)),
        ]),
        Config {
            width: 1920,
            height: 1200,
            scale: 1.5,
        },
        BTreeMap::from([("fixture".into(), root.join("tests/icon.svg"))]),
    )
}

#[test]
fn real_launcher_grid_results_and_scrolled_pixels_match_accepted_markup() -> Result<()> {
    for (name, query, scroll) in [
        ("grid", "", 0.),
        ("results", "a", 0.),
        ("scrolled", "a", 500.),
    ] {
        let mut h = harness(query, scroll)?;
        let source = h.render()?;
        let width = (h.scene().bounds.w * 1.5).ceil() as u32;
        let height = (h.scene().bounds.h * 1.5).ceil() as u32;
        assert!(width <= source.width && height <= source.height);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in source
            .pixels
            .chunks_exact(source.width as usize * 4)
            .take(height as usize)
        {
            pixels.extend_from_slice(&row[..width as usize * 4]);
        }
        let frame = bead_test::Frame {
            width,
            height,
            pixels,
        };
        let golden = root()
            .join("tests/goldens")
            .join(format!("launcher-rows-{name}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance {
                channel: 0,
                changed_fraction: 0.,
            },
            &std::env::temp_dir().join("bead-launcher-rows"),
        )?;
        let actions = h
            .session
            .view_nodes()
            .iter()
            .filter(|n| n.action.as_deref() == Some("select_app"))
            .collect::<Vec<_>>();
        assert!(
            actions.len() >= 7,
            "populated fixture must exercise actual rows"
        );
        let ids = actions
            .iter()
            .map(|n| match &n.action_arg {
                Some(Value::Text(id)) => id.clone(),
                _ => panic!("row action lost its item ID"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actions.len(),
            ids.len(),
            "each visible row must retain its own action ID"
        );
    }
    Ok(())
}

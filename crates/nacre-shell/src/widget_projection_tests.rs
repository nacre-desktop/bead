// SPDX-License-Identifier: GPL-3.0-only
//! Shared tiled widget layouts project onto each output without rewriting the saved document.
use bead::core::Value;
use bead_test::{Config, Harness, Result};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

fn projected_bar(width: u32, document: Json) -> Result<Harness> {
    let mut values = super::initial_models()?;
    values.extend(BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![document])),
        ("widgets.editing".into(), Value::Bool(false)),
        (
            "bead.focused_title".into(),
            Value::Text("A cross-output title longer than four tiles".into()),
        ),
    ]));
    let mut harness = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width,
            height: 70,
            scale: 1.,
        },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(240));
    harness.update(BTreeMap::new())?;
    Ok(harness)
}

fn implicit_bar(width: u32) -> Result<Harness> {
    let mut values = super::initial_models()?;
    values.insert(
        "bead.focused_title".into(),
        Value::Text("A cross-output title longer than four tiles".into()),
    );
    let mut harness = Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        values,
        Config {
            width,
            height: 70,
            scale: 1.,
        },
    )?;
    std::thread::sleep(std::time::Duration::from_millis(240));
    harness.update(BTreeMap::new())?;
    Ok(harness)
}

fn title_text_width(harness: &Harness) -> f32 {
    harness
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text.starts_with("A ") => {
                Some(rect.w)
            }
            _ => None,
        })
        .expect("projected title text should render")
}

#[test]
fn tiled_widget_layout_projects_region_positions_and_preserves_saved_json() -> Result<()> {
    let mut document: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    document["preset"] = json!("Shared tiled test");
    document["future"] = json!({"large": 9007199254740993_u64, "empty_object": {}});
    document["hosts"]["top_bar"]["mode"] = json!("tiles");
    document["hosts"]["top_bar"]["tiles"] = json!(90);
    for region in ["left", "centre", "right"] {
        document["hosts"]["top_bar"]["regions"][region]["start"] = json!(0);
        document["hosts"]["top_bar"]["regions"][region]["span"] = json!(90);
    }
    let instances = document["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap();
    let title = instances
        .iter_mut()
        .find(|item| item["iid"] == "title")
        .unwrap();
    title["slot"]["start"] = json!(45);
    title["slot"]["span"] = json!(4);
    title["settings"]["behavior"] = json!("none");
    instances
        .iter_mut()
        .find(|item| item["iid"] == "launcher")
        .unwrap()["slot"]["start"] = json!(75);
    instances
        .iter_mut()
        .find(|item| item["iid"] == "workspaces")
        .unwrap()["slot"]["start"] = json!(78);
    instances.push(json!({
        "iid":"future-left", "widget":"future-package", "host":"top_bar",
        "slot":{"region":"left","start":81,"span":1}, "settings":{"opaque":{"empty":{}}}
    }));
    for (iid, start) in [
        ("future-centre-a", 75),
        ("future-centre-b", 80),
        ("future-centre-c", 85),
    ] {
        instances.push(json!({
            "iid":iid, "widget":"future-package", "host":"top_bar",
            "slot":{"region":"centre","start":start,"span":1}, "settings":{"opaque":{}}
        }));
    }
    let canonical = Value::list(vec![document.clone()]);

    let wide = projected_bar(2880, document.clone())?;
    let narrow = projected_bar(640, document.clone())?;
    let squeezed = projected_bar(192, document.clone())?;
    let wide_width = title_text_width(&wide);
    let narrow_width = title_text_width(&narrow);
    assert!(wide_width > 0. && wide_width < 128.);
    assert!(
        (wide_width - narrow_width).abs() < 1.,
        "32 dp tile spans should keep title width stable across output widths: wide={wide_width}, narrow={narrow_width}"
    );
    for harness in [&wide, &narrow, &squeezed] {
        assert_eq!(
            harness.session.stores.get("widgets.layout"),
            Some(&canonical)
        );
    }
    assert!(
        title_text_width(&squeezed) > 0.,
        "overloaded regions must repack without rejecting widgets"
    );
    Ok(())
}

#[test]
fn default_content_preset_projection_keeps_exact_pixels_at_narrow_width() -> Result<()> {
    let preset: Json = serde_json::from_str(include_str!("../tests/widgets-default.json"))?;
    let explicit = projected_bar(640, preset)?;
    let implicit = implicit_bar(640)?;
    assert_eq!(
        serde_json::to_value(&explicit.scene().primitives)?,
        serde_json::to_value(&implicit.scene().primitives)?,
        "projecting the default content preset must preserve its rendered pixels"
    );
    Ok(())
}

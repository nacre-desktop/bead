// SPDX-License-Identifier: GPL-3.0-only
//! Widget layouts project onto each output without rewriting the saved document.
use bead::core::Value;
use bead_test::{Config, Harness, Result};
use serde_json::Value as Json;
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

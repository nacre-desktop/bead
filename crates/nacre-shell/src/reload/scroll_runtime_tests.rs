// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead::core::Value;

fn runtime_with_wheel_step(step: Option<f64>) -> Runtime {
    let palette_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let palette = bead::render::Palette::read(&palette_path).unwrap();
    let fallback = crate::sound::fallback(palette.get("surface").unwrap());
    let session = bead::ui::Session::new(
        palette_path.parent().unwrap().into(),
        "missing-scroll-test.luau".into(),
        palette_path,
        palette,
        fallback,
    );
    let mut runtime = Runtime::new(session);
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    if let Some(step) = step {
        runtime.session.stores = runtime
            .session
            .stores
            .reconcile(&BTreeMap::from([(
                "bead.scroll.wheel_step".into(),
                Value::Number(step),
            )]))
            .unwrap();
    }
    runtime
}

#[test]
fn wheel_step_defaults_to_one_hundred_for_missing_or_nonpositive_store() {
    assert_eq!(runtime_with_wheel_step(None).wheel_step(), 100.);
    assert_eq!(runtime_with_wheel_step(Some(0.)).wheel_step(), 100.);
    assert_eq!(runtime_with_wheel_step(Some(-25.)).wheel_step(), 100.);
}

#[test]
fn explicit_luau_wheel_step_is_used() {
    assert_eq!(runtime_with_wheel_step(Some(250.)).wheel_step(), 250.);
}

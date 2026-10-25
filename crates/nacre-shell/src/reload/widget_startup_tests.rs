// SPDX-License-Identifier: GPL-3.0-only
use super::*;
#[test]
fn a_widget_file_callback_before_view_load_keeps_the_initial_layout() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let fallback = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(root, "bar/main.luau".into(), palette, colors, fallback);
    let mut runtime = Runtime::new(session).with_widget_file();
    runtime.state_file = None;
    let document: serde_json::Value = serde_json::from_str(include_str!("../../tests/widgets-default.json")).unwrap();
    let layout = bead::core::Value::list(vec![document]);
    assert!(runtime.stage(UiEvent::Model(BTreeMap::from([
        ("widgets.layout".into(), layout.clone()),
    ]))).unwrap().is_none());
    assert_eq!(runtime.session.stores.get("widgets.layout"), Some(&layout));
}

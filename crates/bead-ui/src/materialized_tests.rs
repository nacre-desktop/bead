// SPDX-License-Identifier: MIT
use super::*;
use crate::tests::Fixture;

fn markup_fixture(content: &str) -> (Fixture, Session) {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.0.join("ui")).unwrap();
    fs::write(
        fixture.0.join("ui/behavior.luau"),
        "return {stores={open=true},actions={},computed={}}",
    )
    .unwrap();
    fs::write(fixture.0.join("ui/main.luau"), content).unwrap();
    let mut session = fixture.session();
    session.entry = "ui/main.luau".into();
    let first = session.reload().unwrap();
    session.commit(first);
    (fixture, session)
}

#[test]
fn root_panel_open_binding_patches_motion_without_relayout_or_luau() {
    let (_fixture, mut session) = markup_fixture(
        "local ui=require('bead.ui')\nreturn ui.Panel{id='panel',width=200,height=120,open=ui.bind('open'),ui.Text{id='label',text='fixed'}}",
    );
    assert!(session.current.motion.as_ref().unwrap().open);
    frontend::reset_eval_calls();
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();

    for open in [false, true, false, true] {
        let scene = session
            .update(BTreeMap::from([("open".into(), Value::Bool(open))]))
            .unwrap()
            .unwrap();
        assert_eq!(scene.motion.as_ref().unwrap().open, open);
        session.commit(scene);
    }

    assert_eq!(
        frontend::eval_calls(),
        0,
        "motion changes must not evaluate Luau"
    );
    #[cfg(feature = "test-counters")]
    assert_eq!(
        layout::layout_passes(),
        passes,
        "motion changes must reuse layout"
    );
}

#[test]
fn root_panel_open_binding_falls_back_when_content_reads_same_store() {
    let (_fixture, mut session) = markup_fixture(
        "local ui=require('bead.ui')\nreturn ui.Panel{id='panel',width=200,height=120,open=ui.bind('open'),ui.Text{id='label',text='fixed',visible=ui.bind('open')}}",
    );
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();

    let scene = session
        .update(BTreeMap::from([("open".into(), Value::Bool(false))]))
        .unwrap()
        .unwrap();
    assert!(!scene.motion.as_ref().unwrap().open);
    #[cfg(feature = "test-counters")]
    assert!(
        layout::layout_passes() > passes,
        "structural use must relayout"
    );
}

#[cfg(feature = "test-counters")]
#[test]
fn root_panel_open_cache_misses_on_palette_and_viewport_changes() {
    let (_fixture, mut session) = markup_fixture(
        "local ui=require('bead.ui')\nreturn ui.Panel{id='panel',width=200,height=120,open=ui.bind('open'),ui.Text{id='label',text='fixed'}}",
    );
    let initial = layout::layout_passes();
    let overlay = Palette::parse(br#"{"formatVersion":1,"colours":{"primary":"ff0000"}}"#).unwrap();
    let scene = session.set_runtime_palette(overlay).unwrap().unwrap();
    session.commit(scene);
    let palette_passes = layout::layout_passes();
    assert!(
        palette_passes > initial,
        "palette changes must rebuild cached colors"
    );

    assert!(session.set_viewport(900, 700));
    let scene = session
        .update(BTreeMap::from([("open".into(), Value::Bool(false))]))
        .unwrap()
        .unwrap();
    assert!(!scene.motion.as_ref().unwrap().open);
    assert!(
        layout::layout_passes() > palette_passes,
        "viewport changes must rebuild layout"
    );
}

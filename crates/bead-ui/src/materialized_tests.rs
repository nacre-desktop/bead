// SPDX-License-Identifier: MIT
use super::*;
use crate::{materialized::patch_hits, tests::Fixture};

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

/// A session over `content` with a fuller palette and the given behaviour stores.
fn interaction_fixture(content: &str, behavior: &str) -> (Fixture, Session) {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"101010","onSurface":"f0f0f0","primary":"8ab4f8","surfaceContainer":"202020","onSurfaceVariant":"a0a0a0","outlineVariant":"404040"}}"#,
    )
    .unwrap();
    fs::create_dir_all(fixture.0.join("ui")).unwrap();
    fs::write(fixture.0.join("ui/behavior.luau"), behavior).unwrap();
    fs::write(fixture.0.join("ui/main.luau"), content).unwrap();
    let mut session = fixture.session();
    session.entry = "ui/main.luau".into();
    let first = session.reload().unwrap();
    session.commit(first);
    (fixture, session)
}
fn node<'a>(session: &'a Session, id: &str) -> &'a Node {
    session
        .view_nodes()
        .iter()
        .find(|n| n.id.rsplit('.').next() == Some(id))
        .unwrap_or_else(|| panic!("no node {id}"))
}
/// The middle of the node's visible (clipped) box.
fn center(session: &Session, id: &str) -> (f32, f32) {
    let node = node(session, id);
    let r = rect(&node.draw);
    let r = node.clip.map_or(r, |clip| clip.intersect(r));
    (r.x + r.w / 2., r.y + r.h / 2.)
}
/// The scoped view id of the node authored as `id` (empty stays empty).
fn scoped(session: &Session, id: &str) -> Value {
    Value::Text(if id.is_empty() {
        String::new()
    } else {
        node(session, id).id.clone()
    })
}
/// Primitives, clips and node boxes of a scene as comparable data, without the
/// scrollbar feedback a staged scroll reveals and a fresh reload does not.
fn painted(scene: &Scene, nodes: &[Node]) -> (serde_json::Value, Vec<(usize, Rect)>, Vec<Rect>) {
    let end = scene
        .layers
        .iter()
        .find(|l| l.id.starts_with("@scrollbar:"))
        .map_or(scene.primitives.len(), |l| l.own.start);
    (
        serde_json::to_value(&scene.primitives[..end]).unwrap(),
        scene
            .clips
            .iter()
            .filter(|(i, _)| **i < end)
            .map(|(i, r)| (*i, *r))
            .collect(),
        nodes.iter().map(|n| rect(&n.draw)).collect(),
    )
}
/// The same stores laid out from scratch by a new generation.
fn rebuilt(session: &mut Session, changes: BTreeMap<String, Value>) -> Scene {
    session.stores.commit(changes).unwrap();
    let scene = session.reload().unwrap();
    session.commit(scene.clone());
    scene
}
const HOVER_VIEW: &str = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=300,fill='surface',
 ui.Button{id='ghost',text='Ghost',width=180,height=30,fill='transparent',hover_fill='primary',on_activate=ui.action('noop')},
 ui.Button{id='solid',text='Solid',width=180,height=30,fill='surfaceContainer',hover_fill='primary',color='onSurfaceVariant',on_activate=ui.action('noop')},
 ui.Button{id='plain',text='Plain',width=180,height=30,on_activate=ui.action('noop')},
 ui.Button{id='bound',text=ui.bind('label'),width=180,height=30,fill='surfaceContainer',hover_fill='primary',on_activate=ui.action('noop')},
 ui.TextInput{id='field',text=ui.bind('field'),placeholder='Type',width=180,height=30},
 ui.TextInput{id='inked',text=ui.bind('field'),placeholder='Type',width=180,height=30,color='onSurface'},
 ui.Button{id='bare',width=180,height=30,fill='transparent',hover_fill='primary',on_activate=ui.action('noop'),ui.Box{id='dot',width=10,height=10,fill='primary'}},
 ui.Scroll{id='list',scroll=ui.bind('scroll'),width=180,height=40,
  ui.Column{id='rows',width=180,
   ui.Button{id='row1',text='Row 1',width=180,height=30,fill='transparent',hover_fill='primary',on_activate=ui.action('noop')},
   ui.Button{id='row2',text='Row 2',width=180,height=30,fill='transparent',hover_fill='primary',on_activate=ui.action('noop')}}}}"#;
const HOVER_BEHAVIOR: &str =
    "return {stores={label='Bound',field='',scroll=0},actions={noop=function(s) end},computed={}}";

#[test]
fn hover_patch_recolours_in_place_and_equals_a_full_rebuild() {
    let (_fixture, mut session) = interaction_fixture(HOVER_VIEW, HOVER_BEHAVIOR);
    let (_reference_fixture, mut reference) = interaction_fixture(HOVER_VIEW, HOVER_BEHAVIOR);
    let mut hits = patch_hits();
    // (target, patched): a box present in both states is recoloured; a box only one state
    // draws is inserted in front of the view's label or removed. An uncoloured input's label
    // depends on its emptiness and a label-less transparent button cannot prove it was not
    // culled, so hovering those rebuilds (leaving `bare` only removes its box).
    for (id, patched) in [
        ("ghost", true),
        ("solid", true),
        ("inked", true),
        ("bound", true),
        ("", true),
        ("plain", true),
        ("field", false),
        ("ghost", false),
        ("", true),
        ("solid", true),
        ("bare", false),
        ("", true),
        ("row1", true),
        ("row2", true),
        ("ghost", true),
        ("", true),
    ] {
        let (x, y) = if id.is_empty() {
            (-5., -5.)
        } else {
            center(&session, id)
        };
        let scene = session.hover(x, y).unwrap().unwrap();
        session.commit(scene.clone());
        let expected = rebuilt(
            &mut reference,
            BTreeMap::from([("bead.hovered".into(), scoped(&session, id))]),
        );
        assert_eq!(
            painted(&scene, session.view_nodes()),
            painted(&expected, reference.view_nodes()),
            "hover {id} must paint exactly like a rebuild"
        );
        assert_eq!(
            patch_hits(),
            hits + usize::from(patched),
            "hover {id} patched={patched}"
        );
        hits = patch_hits();
    }
    // A scrolled list keeps an inserted box clipped and placed with its row.
    for (changes, patched) in [
        (
            BTreeMap::from([("scroll".into(), Value::Number(15.))]),
            true,
        ),
        (
            BTreeMap::from([("bead.hovered".into(), scoped(&session, "row2"))]),
            true,
        ),
        (
            BTreeMap::from([
                ("scroll".into(), Value::Number(0.)),
                ("bead.hovered".into(), scoped(&session, "row1")),
            ]),
            true,
        ),
    ] {
        let scene = session.update(changes.clone()).unwrap().unwrap();
        session.commit(scene.clone());
        let expected = rebuilt(&mut reference, changes);
        assert_eq!(
            painted(&scene, session.view_nodes()),
            painted(&expected, reference.view_nodes())
        );
        assert_eq!(patch_hits(), hits + usize::from(patched));
        hits = patch_hits();
    }
    // A text change while hovered keeps the hover colour of the patched label.
    let (x, y) = center(&session, "bound");
    let scene = session.hover(x, y).unwrap().unwrap();
    session.commit(scene);
    let scene = session
        .update(BTreeMap::from([(
            "label".into(),
            Value::Text("Next".into()),
        )]))
        .unwrap()
        .unwrap();
    session.commit(scene.clone());
    let expected = rebuilt(
        &mut reference,
        BTreeMap::from([
            ("bead.hovered".into(), scoped(&session, "bound")),
            ("label".into(), Value::Text("Next".into())),
        ]),
    );
    assert_eq!(
        painted(&scene, session.view_nodes()),
        painted(&expected, reference.view_nodes())
    );
    // A cache built while hovered keeps that label's hover colour through a text patch.
    let scene = session.reload().unwrap();
    session.commit(scene);
    let scene = session
        .update(BTreeMap::from([(
            "label".into(),
            Value::Text("Last".into()),
        )]))
        .unwrap()
        .unwrap();
    let expected = rebuilt(
        &mut reference,
        BTreeMap::from([("label".into(), Value::Text("Last".into()))]),
    );
    assert_eq!(
        painted(&scene, session.view_nodes()),
        painted(&expected, reference.view_nodes())
    );
}

#[test]
fn hover_bound_in_markup_stays_structural() {
    let view = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=100,fill='surface',
 ui.Button{id='a',text='A',width=180,height=30,fill='transparent',hover_fill='primary',on_activate=ui.action('noop')},
 ui.Text{id='who',text='who',aria_label=ui.bind('bead.hovered'),width=180,height=30}}"#;
    let (_fixture, mut session) = interaction_fixture(view, HOVER_BEHAVIOR);
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();
    let hits = patch_hits();
    let (x, y) = center(&session, "a");
    let scene = session.hover(x, y).unwrap().unwrap();
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Rounded { fill, .. } if fill == &[138, 180, 248, 255]))
    );
    assert_eq!(patch_hits(), hits, "a markup read of bead.hovered rebuilds");
    #[cfg(feature = "test-counters")]
    assert_eq!(layout::layout_passes(), passes + 1);
}

#[test]
fn focus_ring_patch_moves_between_controls_and_rebuilds_when_it_appears_or_leaves() {
    let view = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=200,fill='surface',
 ui.Button{id='a',text='A',width=180,height=30,fill='surfaceContainer',on_activate=ui.action('noop')},
 ui.Scroll{id='list',scroll=ui.bind('scroll'),width=180,height=60,
  ui.Column{id='rows',width=180,
   ui.Button{id='b',text='B',width=180,height=30,fill='surfaceContainer',on_activate=ui.action('noop')},
   ui.Button{id='c',text='C',width=180,height=30,fill='surfaceContainer',on_activate=ui.action('noop')},
   ui.Button{id='d',text='D',width=180,height=30,fill='surfaceContainer',on_activate=ui.action('noop')}}}}"#;
    let behavior = "return {stores={scroll=0},actions={noop=function(s) end},computed={}}";
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    let (_reference_fixture, mut reference) = interaction_fixture(view, behavior);
    let mut hits = patch_hits();
    // (focused, focus_visible, patched)
    let steps = [
        ("a", true, false),
        ("b", true, true),
        ("c", true, true),
        ("c", false, false),
        ("d", false, true),
        ("d", true, false),
        ("a", true, true),
        ("", true, false),
    ];
    for (id, visible, patched) in steps {
        let changes = BTreeMap::from([
            ("bead.focused".into(), scoped(&session, id)),
            ("bead.focus_visible".into(), Value::Bool(visible)),
        ]);
        let scene = session.update(changes.clone()).unwrap().unwrap();
        session.commit(scene.clone());
        let expected = rebuilt(&mut reference, changes);
        assert_eq!(
            painted(&scene, session.view_nodes()),
            painted(&expected, reference.view_nodes()),
            "focus {id} visible={visible}"
        );
        assert_eq!(
            patch_hits(),
            hits + usize::from(patched),
            "focus {id} visible={visible}"
        );
        hits = patch_hits();
    }
    // Scrolling moves the ring with its control inside the clipped list.
    let changes = BTreeMap::from([
        ("bead.focused".into(), scoped(&session, "c")),
        ("bead.focus_visible".into(), Value::Bool(true)),
    ]);
    let scene = session.update(changes.clone()).unwrap().unwrap();
    session.commit(scene);
    rebuilt(&mut reference, changes);
    let scroll = BTreeMap::from([("scroll".into(), Value::Number(20.))]);
    let scene = session.update(scroll.clone()).unwrap().unwrap();
    session.commit(scene.clone());
    assert_eq!(patch_hits(), hits + 1, "the scroll step patches");
    let expected = rebuilt(&mut reference, scroll);
    assert_eq!(
        painted(&scene, session.view_nodes()),
        painted(&expected, reference.view_nodes()),
        "a scrolled focus ring follows its control"
    );
}

#[test]
fn animation_frames_rebuild_and_the_raw_cache_hits_once_the_motion_ends() {
    let view = r#"local ui=require('bead.ui')
return ui.Row{id='root',width=300,height=40,fill='surface',
 ui.Box{id='bar',width=ui.animate('w',ui.resolve_for_check(ui.bind('target')),0.05,'linear'),height=20,fill='primary'}}"#;
    let behavior = "return {stores={target=10},actions={},computed={}}";
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(5));
    let width = |scene: &Scene| {
        match scene.primitives.iter().find(|p| matches!(p, Primitive::Rounded { fill, .. } if fill[3] == 255 && fill != &[16, 16, 16, 255])) {
        Some(Primitive::Rounded { rect, .. }) => rect.w,
        _ => panic!("no bar"),
    }
    };
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(200.))]))
        .unwrap()
        .unwrap();
    let mut last = width(&scene);
    assert!(last < 200.);
    session.commit(scene);
    frontend::reset_eval_calls();
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let next = width(&scene);
        assert!(next >= last, "animation advances monotonically");
        last = next;
        session.commit(scene);
        frames += 1;
        assert!(frames < 100);
    }
    assert_eq!(last, 200., "the motion settles on its target");
    assert_eq!(
        frontend::eval_calls(),
        frames,
        "each frame evaluates Luau once"
    );
    assert!(session.preview_deadline().is_none());
    // Retargeting restarts the motion from the settled value; an unrelated wakeup hits the cache.
    assert!(session.update_model(BTreeMap::new()).unwrap().is_none());
    assert_eq!(frontend::eval_calls(), frames);
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(50.))]))
        .unwrap()
        .unwrap();
    assert!(width(&scene) <= 200. && width(&scene) > 50.);
    assert!(session.preview_deadline().is_some());
}

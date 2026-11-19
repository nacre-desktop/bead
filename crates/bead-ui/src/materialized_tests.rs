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

#[test]
fn sampled_markers_patch_paint_properties_without_luau_or_relayout() {
    let view = r#"local ui=require('bead.ui')
local m=ui.motion
local target=ui.resolve_for_check(ui.bind('target'))
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Box{id='box',width=100,height=40,fill=ui.alpha('primary',ui.animated('a',target/200,m.duration.open,m.curve.out_cubic)),
  radius=ui.animated('r',target/10,0.2,'out-quint'),translate_x=ui.animated('x',target,0.2,'out-cubic'),
  ui.Text{id='label',text='Hi',width=50,height=20,color=ui.alpha('onSurface',ui.animated('t',target/200,0.2,'linear'))},
  ui.Button{id='hit',text='Go',width=40,height=20,on_activate=ui.action('noop')}}}"#;
    let behavior = "return {stores={target=0},actions={noop=function(s) end},computed={}}";
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(4));
    let box_ = |scene: &Scene| match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Rounded { rect, .. } if rect.w == 100.))
    {
        Some(Primitive::Rounded {
            rect, radius, fill, ..
        }) => (rect.x, *radius, fill[3]),
        _ => panic!("no box"),
    };
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(200.))]))
        .unwrap()
        .unwrap();
    let start = box_(&scene);
    assert!(start.0 < 200. && start.1 < 20. && start.2 < 255);
    session.commit(scene);
    let hit_start = rect(&node(&session, "hit").draw).x - start.0;
    frontend::reset_eval_calls();
    let hits = patch_hits();
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();
    let mut last = start;
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let next = box_(&scene);
        assert!(next.0 >= last.0 && next.1 >= last.1 && next.2 >= last.2);
        // Every frame is one patch of the cached scene: no Luau, no layout.
        assert_eq!(frontend::eval_calls(), 0);
        last = next;
        session.commit(scene);
        assert_eq!(
            rect(&node(&session, "hit").draw).x - next.0,
            hit_start,
            "node boxes move with the view"
        );
        frames += 1;
        assert!(frames < 200);
    }
    assert!(frames > 3);
    assert_eq!(last, (200., 20., 255), "exact final geometry");
    assert_eq!(patch_hits(), hits + frames);
    #[cfg(feature = "test-counters")]
    assert_eq!(layout::layout_passes(), passes);
    // A retarget continues from the sampled value: one Luau run, no jump.
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(100.))]))
        .unwrap()
        .unwrap();
    let evaluated = frontend::eval_calls();
    assert!(evaluated >= 1);
    assert_eq!(box_(&scene), (200., 20., 255));
    session.commit(scene);
    std::thread::sleep(std::time::Duration::from_millis(250));
    let scene = session.update_model(BTreeMap::new()).unwrap().unwrap();
    assert_eq!(box_(&scene), (100., 10., 128));
    assert_eq!(
        frontend::eval_calls(),
        evaluated,
        "frames after a retarget run no Luau"
    );
}

#[test]
fn opacity_marker_patches_an_image_without_relayout() {
    let view = r#"local ui=require('bead.ui')
local target=ui.resolve_for_check(ui.bind('target'))
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Image{id='pic',icon='bell',width=20,height=20,opacity=ui.animated('o',target,0.05,'linear')}}"#;
    let behavior = "return {stores={target=0},actions={},computed={}}";
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(4));
    let opacity = |scene: &Scene| {
        scene
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Image { opacity, .. } => Some(*opacity),
                _ => None,
            })
            .expect("image")
    };
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(1.))]))
        .unwrap()
        .unwrap();
    let mut last = opacity(&scene);
    session.commit(scene);
    frontend::reset_eval_calls();
    let hits = patch_hits();
    let mut changed = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let next = opacity(&scene);
        assert!(next >= last);
        changed += usize::from(next > last);
        last = next;
        session.commit(scene);
        assert_eq!(frontend::eval_calls(), 0);
    }
    assert_eq!(last, 1.);
    assert!(changed > 1, "samples change the image opacity");
    assert!(patch_hits() > hits, "the image marker patches");
    // Image alone: patched frames equal a full rebuild at the same sample.
    let alone = r#"local ui=require('bead.ui')
local target=ui.resolve_for_check(ui.bind('target'))
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Image{id='pic',icon='bell',width=20,height=20,opacity=ui.animated('o',target,0.05,'linear')}}"#;
    let (_f, mut session) = interaction_fixture(alone, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(4));
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(1.))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    frontend::reset_eval_calls();
    let hits = patch_hits();
    let mut frames = 0;
    let mut previous = 0.;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let sampled = opacity(&scene);
        assert!(sampled >= previous);
        previous = sampled;
        session.commit(scene.clone());
        frames += 1;
        assert_eq!(frontend::eval_calls(), 0);
        assert_eq!(patch_hits(), hits + frames, "one patch per frame");
        assert!(frames < 200);
    }
    assert!(frames > 1);
    assert_eq!(previous, 1.);
    // The settled patched scene equals a full materialize of the same sample.
    let patched = session.update_model(BTreeMap::new()).unwrap();
    assert!(patched.is_none());
    let full = session.reload().unwrap();
    assert_eq!(opacity(&full), 1.);
}

/// B0349: the alphas of the group's fill, text and image, and the image opacity.
fn group_alphas(scene: &Scene) -> (u8, u8, f32) {
    let (mut fill, mut text, mut image) = (0, 0, 0.);
    for p in &scene.primitives {
        match p {
            Primitive::Rounded { rect, fill: f, .. } if rect.w == 100. => fill = f[3],
            Primitive::Text { color, .. } => text = color[3],
            Primitive::Image { opacity, .. } => image = *opacity,
            _ => {}
        }
    }
    (fill, text, image)
}
/// Run the motion of a fixture whose target store goes to `target` to its end: one patch per frame.
fn settle_opacity(
    view: &str,
    target: f64,
    mut each: impl FnMut(&Scene),
) -> (Fixture, Session, Scene) {
    let behavior = "return {stores={target=0},actions={},computed={}}";
    let (fixture, mut session) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(4));
    let mut scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(target))]))
        .unwrap()
        .unwrap();
    session.commit(scene.clone());
    frontend::reset_eval_calls();
    let hits = patch_hits();
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(next) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        frames += 1;
        assert_eq!(frontend::eval_calls(), 0, "no Luau");
        assert_eq!(
            patch_hits(),
            hits + frames,
            "one patch per frame, no relayout"
        );
        each(&next);
        session.commit(next.clone());
        scene = next;
        assert!(frames < 300);
    }
    assert!(frames > 1);
    (fixture, session, scene)
}
fn json(scene: &Scene) -> String {
    serde_json::to_string(&scene.primitives).unwrap()
}

#[test]
fn opacity_marker_on_a_box_dims_its_subtree_and_equals_a_full_build() {
    let view = r#"local ui=require('bead.ui')
local target=ui.resolve_for_check(ui.bind('target'))
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Box{id='group',width=100,height=40,fill='primary',opacity=ui.animated('o',target,0.05,'linear'),
  ui.Text{id='label',text='Hi',width=50,height=20},
  ui.Image{id='pic',icon='bell',width=20,height=20}}}"#;
    let mut seen = vec![];
    let (_fixture, mut session, last) =
        settle_opacity(view, 0.5, |scene| seen.push(group_alphas(scene)));
    assert_eq!(group_alphas(&last), (128, 128, 0.5));
    assert!(seen.windows(2).any(|w| w[0].2 < w[1].2), "samples change");
    // Each sample: every child alpha is its opaque base times the sample.
    for (fill, text, image) in seen {
        assert_eq!(fill, (255. * image).round() as u8);
        assert_eq!(text, (255. * image).round() as u8);
    }
    // The settled patched scene equals a full build at the same sample.
    assert_eq!(json(&session.reload().unwrap()), json(&last));
}

#[test]
fn opacity_multiplies_with_parents_and_alpha_markers_without_compounding() {
    let view = r#"local ui=require('bead.ui')
local target=ui.resolve_for_check(ui.bind('target'))
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Box{id='outer',width=100,height=40,fill='primary',opacity=0.5,
  ui.Box{id='inner',width=60,height=30,fill=ui.alpha('primary',ui.animated('a',target,0.05,'linear')),opacity=ui.animated('o',target,0.05,'linear')}}}"#;
    let inner = |scene: &Scene| match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Rounded { rect, .. } if rect.w == 60.))
    {
        Some(Primitive::Rounded { fill, .. }) => fill[3],
        _ => panic!("no inner"),
    };
    let mut seen = vec![];
    let (_fixture, mut session, last) = settle_opacity(view, 1., |scene| seen.push(inner(scene)));
    // Settled: base alpha times the outer 0.5 only. Mid-motion the alpha, the opacity and the
    // outer factor all multiply, so the sequence rises monotonically and never exceeds that.
    assert_eq!(inner(&last), 128);
    assert!(seen.iter().all(|a| *a <= 128));
    assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    assert!(seen.iter().any(|a| *a < 64), "alpha and opacity both apply");
    assert_eq!(json(&session.reload().unwrap()), json(&last));
}

#[test]
fn static_opacity_one_changes_nothing() {
    let plain = r#"local ui=require('bead.ui')
return ui.Row{id='root',width=300,height=60,fill='surface',
 ui.Box{id='group',width=100,height=40,fill='primary',{{O}}ui.Text{id='label',text='Hi',width=50,height=20}}}"#;
    let scene = |o: &str| {
        let (_f, mut session) = interaction_fixture(
            &plain.replace("{{O}}", o),
            "return {stores={},actions={},computed={}}",
        );
        session.reload().unwrap()
    };
    assert_eq!(json(&scene("")), json(&scene("opacity=1.0,")));
}

#[test]
fn sampled_layout_markers_relayout_without_luau_until_the_motion_ends() {
    let view = r#"local ui=require('bead.ui')
return ui.Row{id='root',width=300,height=40,fill='surface',
 ui.Box{id='bar',width=ui.animated('w',ui.resolve_for_check(ui.bind('target')),0.05,'linear'),height=20,fill='primary'}}"#;
    let behavior = "return {stores={target=10},actions={},computed={}}";
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(5));
    let width = |scene: &Scene| {
        match scene.primitives.iter().find(
        |p| matches!(p, Primitive::Rounded { fill, .. } if fill[3] == 255 && fill != &[16, 16, 16, 255]),
    ) {
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
    let hits = patch_hits();
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let next = width(&scene);
        assert!(next >= last);
        last = next;
        session.commit(scene);
        frames += 1;
        assert!(frames < 100);
    }
    assert_eq!(last, 200.);
    assert!(frames > 1);
    assert_eq!(frontend::eval_calls(), 0, "no frame evaluates Luau");
    assert_eq!(patch_hits(), hits, "a width motion relayouts");
    assert!(session.preview_deadline().is_none());
    // Settled: the next unrelated wakeup hits the raw cache and the scene patch.
    assert!(session.update_model(BTreeMap::new()).unwrap().is_none());
}

/// Spec 0280: a palette with the roles the kit's notification row and card read.
fn kit_fixture(content: &str, behavior: &str) -> (Fixture, Session) {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"101010","onSurface":"f0f0f0","primary":"8ab4f8","surfaceContainer":"202020","surfaceContainerHigh":"282828","surfaceContainerHighest":"303030","onSurfaceVariant":"a0a0a0","outlineVariant":"404040","outline":"606060"}}"#,
    )
    .unwrap();
    fs::create_dir_all(fixture.0.join("ui")).unwrap();
    fs::write(fixture.0.join("ui/behavior.luau"), behavior).unwrap();
    fs::write(fixture.0.join("ui/main.luau"), content).unwrap();
    let mut session = fixture.session();
    session.entry = "ui/main.luau".into();
    let first = session.reload().unwrap();
    session.commit(first);
    session.set_frame_interval(std::time::Duration::from_millis(5));
    (fixture, session)
}
/// Commit frames until no motion is due; returns how many it took.
fn settle(session: &mut Session) -> usize {
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        session.commit(scene);
        frames += 1;
        assert!(frames < 400, "motion settles");
    }
    frames
}
fn layer_bounds(scene: &Scene, suffix: &str) -> Rect {
    scene
        .layers
        .iter()
        .find(|l| l.id.ends_with(suffix))
        .unwrap_or_else(|| panic!("no layer {suffix}"))
        .bounds
}
/// The first text primitive drawing `text`.
fn text_of<'a>(scene: &'a Scene, text: &str) -> (&'a Rect, f32, [u8; 4]) {
    scene
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Text {
                rect,
                text: t,
                size,
                color,
                ..
            } if t == text => Some((rect, *size, *color)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {text}"))
}
const NOTICE_VIEW: &str = r#"local ui=require('bead.ui')
local kit=require('bead.kit')
return ui.Column{id='root',width=420,height=200,fill='surface',gap=8,
 kit.NoticeRow{id='n',width=380,caption='Mail',title='Hello',body='Body',on_dismiss=ui.action('noop')},
 ui.Button{id='other',text='Other',width=100,height=30,on_activate=ui.action('noop')}}"#;

#[test]
fn hover_only_child_fades_in_with_its_group_and_keyboard_focus_without_luau_or_layout() {
    let (_fixture, mut session) = kit_fixture(
        NOTICE_VIEW,
        "return {stores={},actions={noop=function(s) end},computed={}}",
    );
    let alpha = |session: &Session| text_of(&session.current, "close").2[3];
    assert_eq!(alpha(&session), 0, "the dismiss button is hidden at rest");
    // Still a node: the pointer, keyboard and accessibility reach it.
    assert!(node(&session, "dismiss").action.is_some());
    frontend::reset_eval_calls();
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();
    // The row's plain background is a hover target for its group.
    let row = layer_bounds(&session.current, "notice-n");
    let scene = session.hover(row.x + 4., row.y + 4.).unwrap().unwrap();
    session.commit(scene);
    let Some(Value::Text(hovered)) = session.stores.get("bead.hovered").cloned() else {
        panic!("hover id")
    };
    assert!(hovered.ends_with("notice-n"), "hovered {hovered}");
    assert!(settle(&mut session) > 1, "the reveal fades over frames");
    assert_eq!(alpha(&session), 255);
    // Over the button itself (inside the group) it stays; outside the group it fades out.
    let (x, y) = center(&session, "dismiss");
    let scene = session.hover(x, y).unwrap().unwrap();
    session.commit(scene);
    settle(&mut session);
    assert_eq!(alpha(&session), 255);
    let (x, y) = center(&session, "other");
    let scene = session.hover(x, y).unwrap().unwrap();
    session.commit(scene);
    settle(&mut session);
    assert_eq!(alpha(&session), 0);
    // Keyboard focus inside the group reveals it; pointer focus (focus_visible off) does not.
    let dismiss = scoped(&session, "dismiss");
    for (visible, expected) in [(true, 255), (false, 0)] {
        let scene = session
            .update(BTreeMap::from([
                ("bead.focused".into(), dismiss.clone()),
                ("bead.focus_visible".into(), Value::Bool(visible)),
            ]))
            .unwrap()
            .unwrap();
        session.commit(scene);
        settle(&mut session);
        assert_eq!(alpha(&session), expected, "focus_visible={visible}");
    }
    assert_eq!(frontend::eval_calls(), 0, "hover reveal runs no Luau");
    #[cfg(feature = "test-counters")]
    assert!(
        layout::layout_passes() <= passes + 2,
        "only the focus-ring changes relayout"
    );
}

#[test]
fn fade_scales_an_untinted_image_opacity_and_a_shadow_alpha_but_leaves_a_tint_to_its_colour() {
    let rect = bead_render::Rect::new(0., 0., 8., 8.);
    let mut image = Primitive::image(rect, "a.png");
    if let Primitive::Image { opacity, .. } = &mut image {
        *opacity = 0.5;
    }
    crate::materialized::fade(&mut image, 0.5);
    assert!(matches!(image, Primitive::Image { opacity, .. } if opacity == 0.25));
    let mut tinted = Primitive::image(rect, "a.png");
    if let Primitive::Image { tint, .. } = &mut tinted {
        *tint = Some([10, 20, 30, 200]);
    }
    crate::materialized::fade(&mut tinted, 0.5);
    assert!(
        matches!(tinted, Primitive::Image { tint: Some(c), opacity, .. } if c[3] == 100 && opacity == 1.)
    );
    let mut shadow = Primitive::Shadow {
        rect,
        radius: 2.,
        blur: 4.,
        spread: 0.,
        dy: 0.,
        alpha: 0.8,
    };
    crate::materialized::fade(&mut shadow, 0.5);
    assert!(matches!(shadow, Primitive::Shadow { alpha, .. } if (alpha - 0.4).abs() < 1e-6));
}

#[test]
fn fade_scales_every_radial_stop_alpha_with_the_fill() {
    let stops = vec![(0., [255, 0, 0, 200]), (1., [0, 0, 255, 100])];
    let layer = bead_render::RadialLayer {
        cx: 0.5,
        cy: 0.5,
        rx: 0.5,
        ry: 0.5,
        stops,
    };
    let mut primitive =
        Primitive::rounded(bead_render::Rect::new(0., 0., 8., 8.), 2., [9, 9, 9, 100]);
    if let Primitive::Rounded { radial, .. } = &mut primitive {
        *radial = Some(std::sync::Arc::from(vec![layer]));
    }
    crate::materialized::fade(&mut primitive, 0.5);
    let Primitive::Rounded { fill, radial, .. } = primitive else {
        unreachable!()
    };
    assert_eq!(fill[3], 50);
    let stops = &radial.unwrap()[0].stops;
    assert_eq!(
        (stops[0].1, stops[1].1),
        ([255, 0, 0, 100], [0, 0, 255, 50])
    );
}

#[test]
fn pressed_view_scales_and_deepens_its_fill_until_release_without_luau() {
    let view = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=100,fill='surface',
 ui.Button{id='chip',text='Chip',width=100,height=40,fill='surfaceContainer',hover_fill='primary',pressed_fill='onSurface',press_scale=0.9,on_activate=ui.action('noop')}}"#;
    let (_fixture, mut session) = kit_fixture(
        view,
        "return {stores={},actions={noop=function(s) end},computed={}}",
    );
    let chip = rect(&node(&session, "chip").draw);
    let boxed = |session: &Session| {
        match session.current.primitives.iter().find(
        |p| matches!(p, Primitive::Rounded { rect, .. } if rect.w > 50. && rect.w < 150. && rect.h > 20.),
    ) {
        Some(Primitive::Rounded { rect, fill, .. }) => (*rect, *fill),
        _ => panic!("no chip box"),
    }
    };
    let size = text_of(&session.current, "Chip").1;
    let (x, y) = center(&session, "chip");
    let scene = session.hover(x, y).unwrap().unwrap();
    session.commit(scene);
    frontend::reset_eval_calls();
    let scene = session.click(x, y).unwrap().unwrap();
    session.commit(scene);
    assert!(settle(&mut session) > 1, "the press runs over frames");
    let (pressed, fill) = boxed(&session);
    assert!((pressed.w - 90.).abs() < 0.01 && (pressed.h - 36.).abs() < 0.01);
    assert!(
        (pressed.x - (chip.x + 5.)).abs() < 0.01,
        "scaled about the centre"
    );
    assert_eq!(fill, [240, 240, 240, 255], "pressed_fill");
    assert!((text_of(&session.current, "Chip").1 - size * 0.9).abs() < 0.01);
    let scene = session.release().unwrap().unwrap();
    session.commit(scene);
    settle(&mut session);
    let (released, fill) = boxed(&session);
    assert_eq!(released, chip);
    assert_eq!(fill, [138, 180, 248, 255], "back to the hover fill");
    assert_eq!(frontend::eval_calls(), 0, "press feedback runs no Luau");
    // Node boxes never scale: input keeps the laid-out geometry.
    assert_eq!(rect(&node(&session, "chip").draw), chip);
}

#[test]
fn reflow_children_slide_from_their_drawn_position_without_luau_or_layout_per_frame() {
    let view = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=300,fill='surface',
 ui.Column{id='stack',width=200,gap=4,reflow=0.08,
  ui.Repeat{id='items',model=ui.bind('rows'),key='id',render=function(item)
   return ui.Box{id='card',width=200,height=40,fill='primary'} end}}}"#;
    let behavior = "return {stores={rows={{id='a'},{id='b'},{id='c'}}},actions={},computed={}}";
    let (_fixture, mut session) = kit_fixture(view, behavior);
    // `b` is key 62 (hex); its box is the first primitive of its layer.
    let drawn_y = |scene: &Scene| {
        let layer = scene
            .layers
            .iter()
            .find(|l| l.id.contains(".k62.") && l.id.ends_with("card"))
            .expect("b");
        (rect(&scene.primitives[layer.own.start]).y, layer.bounds.y)
    };
    let (start, _) = drawn_y(&session.current);
    assert_eq!(start, 44.);
    let rows = |ids: &[&str]| {
        Value::list(
            ids.iter()
                .map(|id| serde_json::json!({ "id": id }))
                .collect(),
        )
    };
    let scene = session
        .update(BTreeMap::from([("rows".into(), rows(&["b", "c"]))]))
        .unwrap()
        .unwrap();
    let (y, rest) = drawn_y(&scene);
    assert!(
        rest == 0. && y > 40.,
        "the first frame draws b where it was: {y}"
    );
    session.commit(scene);
    frontend::reset_eval_calls();
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();
    let mut last = y;
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        let (y, _) = drawn_y(&scene);
        assert!(y <= last, "b slides up monotonically");
        last = y;
        session.commit(scene);
        frames += 1;
        assert!(frames < 400);
    }
    assert!(frames > 1);
    assert_eq!(last, 0., "b settles at its new place");
    assert_eq!(frontend::eval_calls(), 0, "slide frames run no Luau");
    #[cfg(feature = "test-counters")]
    assert_eq!(layout::layout_passes(), passes, "slide frames reuse layout");
    // A new item appears in place; the others slide down from where they are drawn.
    let scene = session
        .update(BTreeMap::from([("rows".into(), rows(&["a", "b", "c"]))]))
        .unwrap()
        .unwrap();
    let (y, rest) = drawn_y(&scene);
    assert!(rest == 44. && y < 4., "b starts its slide down from 0: {y}");
}

#[test]
fn shadow_on_any_surface_and_a_hover_box_inserted_behind_it_paints_like_a_rebuild() {
    let view = r#"local ui=require('bead.ui')
local kit=require('bead.kit')
return ui.Column{id='root',width=300,height=300,fill='surface',padding=40,gap=40,
 kit.Card{id='card',width=120,height=50,shadow=true},
 ui.Button{id='lift',text='Lift',width=100,height=30,fill='transparent',hover_fill='primary',shadow=true,on_activate=ui.action('noop')}}"#;
    let behavior = "return {stores={},actions={noop=function(s) end},computed={}}";
    let (_fixture, mut session) = kit_fixture(view, behavior);
    let (_reference_fixture, mut reference) = kit_fixture(view, behavior);
    let card = layer_bounds(&session.current, "card");
    assert!(session.current.primitives.iter().any(|p| matches!(p,
        Primitive::Shadow { rect, blur, .. } if *rect == Rect::new(card.x - 30., card.y - 30., card.w + 60., card.h + 60.) && *blur == 10.)));
    let hits = patch_hits();
    let (x, y) = center(&session, "lift");
    let scene = session.hover(x, y).unwrap().unwrap();
    session.commit(scene.clone());
    assert_eq!(patch_hits(), hits + 1, "hovering patches");
    let expected = rebuilt(
        &mut reference,
        BTreeMap::from([("bead.hovered".into(), scoped(&session, "lift"))]),
    );
    assert_eq!(
        painted(&scene, session.view_nodes()),
        painted(&expected, reference.view_nodes())
    );
    // Not on leaves: a text has no surface to shadow.
    let raw = layout::RawNode {
        kind: layout::Widget::Text,
        props: serde_json::from_value(serde_json::json!({"text":"t","shadow":true})).unwrap(),
        children: vec![],
        source: layout::Source {
            file: "test.luau".into(),
            line: 1,
        },
        scope: None,
    };
    assert!(layout::ViewSpec::from_raw(&raw, &|_| Ok(Value::Bool(false))).is_err());
}

/// B0307: a close-on-leave view closes `leave_ms` after the pointer leaves unless it authors `panel.leave_delay`.
#[test]
fn close_on_leave_defaults_to_the_profile_delay_and_an_authored_one_wins() {
    let delay = |stores: &str| {
        let behavior = format!(
            "return {{stores={{[\"panel.close_on_leave\"]=true,{stores}}},actions={{}},computed={{}}}}"
        );
        let (_fixture, mut session) = interaction_fixture(
            "local ui=require('bead.ui')\nreturn ui.Panel{id='root',width=100,height=60}",
            &behavior,
        );
        session.pointer_region(true).unwrap();
        let left = std::time::Instant::now();
        session.pointer_region(false).unwrap();
        session.hover_close_deadline().unwrap() - left
    };
    let profile = std::time::Duration::from_millis(bead_core::animation::PANEL.leave_ms);
    let default = delay("");
    assert!(default >= profile && default < profile * 2, "{default:?}");
    let authored = delay("[\"panel.leave_delay\"]=1");
    assert!(
        authored >= std::time::Duration::from_secs(1),
        "{authored:?}"
    );
}

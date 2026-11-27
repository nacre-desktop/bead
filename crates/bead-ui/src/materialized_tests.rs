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

#[test]
fn the_accepted_runtime_palette_again_runs_nothing_but_a_pending_change_still_does() {
    let (_fixture, mut session) = markup_fixture(
        "local ui=require('bead.ui')\nreturn ui.Panel{id='panel',width=200,height=120,ui.Text{id='label',text='fixed'}}",
    );
    let palette = |hex: &str| {
        let json = format!(r#"{{"formatVersion":1,"colours":{{"primary":"{hex}"}}}}"#);
        Palette::parse(json.as_bytes()).unwrap()
    };
    let scene = session
        .set_runtime_palette(palette("ff0000"))
        .unwrap()
        .unwrap();
    session.commit(scene);
    #[cfg(feature = "test-counters")]
    let passes = layout::layout_passes();
    // B0410: a parked view's resume restages the accepted palette.
    assert!(
        session
            .set_runtime_palette(palette("ff0000"))
            .unwrap()
            .is_none()
    );
    #[cfg(feature = "test-counters")]
    assert_eq!(
        layout::layout_passes(),
        passes,
        "no pass for the same palette"
    );
    // A change staged but not yet committed: returning to the accepted palette still runs.
    assert!(
        session
            .set_runtime_palette(palette("00ff00"))
            .unwrap()
            .is_some()
    );
    assert!(
        session
            .set_runtime_palette(palette("ff0000"))
            .unwrap()
            .is_some()
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

/// B0416 risk 1: a parked view bakes its closed scene with a new palette in the background. Its
/// open scene must then build no raster: a reveal and a slide panel, tinted text and image, CPU
/// and upload policies. The control is today's parked view, which bakes only at its open.
#[test]
fn a_closed_panel_bake_warms_every_raster_its_open_scene_needs() {
    use bead_render::tree::{ImagePolicy, TreeCache};
    let palette = |hex: &str| {
        let json = format!(r#"{{"formatVersion":1,"colours":{{"primary":"{hex}"}}}}"#);
        Palette::parse(json.as_bytes()).unwrap()
    };
    let open = |value| BTreeMap::from([("open".into(), Value::Bool(value))]);
    for motion in ["reveal=true,edge='top'", "slide_distance=140,edge='right'"] {
        for policy in [ImagePolicy::Dense, ImagePolicy::UploadOnly] {
            let fixture = Fixture::new();
            fs::write(
                fixture.0.join("palette.json"),
                r#"{"formatVersion":1,"colours":{"surface":"101010","primary":"808080"}}"#,
            )
            .unwrap();
            fs::create_dir_all(fixture.0.join("ui")).unwrap();
            fs::write(
                fixture.0.join("ui/dot.svg"),
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#fff"/></svg>"##,
            )
            .unwrap();
            fs::write(
                fixture.0.join("ui/behavior.luau"),
                "return {stores={open=true},actions={},computed={}}",
            )
            .unwrap();
            fs::write(
                fixture.0.join("ui/main.luau"),
                format!(
                    "local ui=require('bead.ui')\nreturn ui.Panel{{id='panel',width=220,height=140,open=ui.bind('open'),{motion},body='body',\
                     ui.Column{{id='body',width=200,height=120,fill='surface',padding=8,\
                     ui.Text{{id='label',text='Tinted',font_size=14,height=20,color='primary'}},\
                     ui.Image{{id='dot',path='ui/dot.svg',width=16,height=16,tint='primary'}}}}}}"
                ),
            )
            .unwrap();
            let mut session = fixture.session();
            session.entry = "ui/main.luau".into();
            let first = session.reload().unwrap();
            session.commit(first);
            let open_a = session.set_runtime_palette(palette("ff0000")).unwrap();
            session.commit(open_a.clone().unwrap());
            let closed_a = session.update(open(false)).unwrap();
            session.commit(closed_a.clone().unwrap());
            let closed_b = session.set_runtime_palette(palette("00ff00")).unwrap();
            session.commit(closed_b.clone().unwrap());
            let open_b = session.update(open(true)).unwrap().unwrap();
            let [open_a, closed_a, closed_b] = [open_a, closed_a, closed_b].map(Option::unwrap);
            assert!(
                !closed_b.motion.as_ref().unwrap().open && open_b.motion.as_ref().unwrap().open
            );
            // Both views opened with palette A and closed: the worker's idle trim keeps the closed bake.
            let (mut today, mut parked) = (
                TreeCache::default().with_image_policy(policy),
                TreeCache::default().with_image_policy(policy),
            );
            for cache in [&mut today, &mut parked] {
                cache.prepare(&open_a, 1.).unwrap();
                cache.prepare(&closed_a, 1.).unwrap();
                cache.trim_idle();
            }
            parked.prepare(&closed_b, 1.).unwrap();
            parked.trim_idle();
            // Held, so an upload payload built again could not reuse the background one's address.
            let background = parked.frame.clone().unwrap();
            let payload = |frame: &bead_render::tree::Frame| {
                frame
                    .items
                    .iter()
                    .find(|item| matches!(item.primitive, Primitive::Image { .. }))
                    .and_then(|item| item.image.as_ref().map(std::sync::Arc::as_ptr))
            };
            assert_eq!(
                payload(&background).is_some(),
                policy == ImagePolicy::UploadOnly
            );
            let (built_today, built_parked) = (today.builds, parked.builds);
            today.prepare(&open_b, 1.).unwrap();
            parked.prepare(&open_b, 1.).unwrap();
            let label = format!("{motion} policy_dense={}", policy == ImagePolicy::Dense);
            let today_builds = today.builds - built_today;
            println!(
                "B0416_RISK1 {label} open_builds_today={today_builds} open_builds_parked={}",
                parked.builds - built_parked
            );
            assert!(
                today_builds >= if policy == ImagePolicy::Dense { 2 } else { 1 },
                "{label}: the control bakes the tinted layers at its open ({today_builds})"
            );
            assert_eq!(
                parked.builds, built_parked,
                "{label}: the closed bake warmed every raster the open scene needs"
            );
            assert_eq!(
                payload(parked.frame.as_ref().unwrap()),
                payload(&background),
                "{label}: and the tinted upload payload"
            );
        }
    }
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
fn a_motion_build_skips_the_unusable_cache_and_the_settled_build_restores_it() {
    let view = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=300,height=80,fill='surface',
 ui.Box{id='bar',width=ui.animate('w',ui.resolve_for_check(ui.bind('target')),0.05,'linear'),height=20,fill='primary'},
 ui.Button{id='btn',text='Go',width=100,height=30,fill='surfaceContainer',hover_fill='primary',on_activate=ui.action('noop')}}"#;
    let behavior = "return {stores={target=10},actions={noop=function(s) end},computed={}}";
    let target = BTreeMap::from([("target".into(), Value::Number(200.))]);
    let (_fixture, mut session) = interaction_fixture(view, behavior);
    let (_reference_fixture, mut reference) = interaction_fixture(view, behavior);
    session.set_frame_interval(std::time::Duration::from_millis(5));
    let built = crate::generation::cached_builds();
    let scene = session.update(target.clone()).unwrap().unwrap();
    session.commit(scene);
    // Every build while the motion runs is a build whose raw nothing keeps.
    let mut frames = 0;
    while let Some(deadline) = session.preview_deadline() {
        assert_eq!(
            crate::generation::cached_builds(),
            built,
            "no Cached mid-motion"
        );
        std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
        let Some(scene) = session.update_model(BTreeMap::new()).unwrap() else {
            break;
        };
        session.commit(scene);
        frames += 1;
        assert!(frames < 100);
    }
    assert!(frames > 1);
    // The settling build is not animating: a Cached exists again and hover patches.
    assert!(crate::generation::cached_builds() > built);
    rebuilt(&mut reference, target);
    let hits = patch_hits();
    let (x, y) = center(&session, "btn");
    let scene = session.hover(x, y).unwrap().unwrap();
    assert_eq!(patch_hits(), hits + 1, "the settled cache patches hover");
    let expected = rebuilt(
        &mut reference,
        BTreeMap::from([("bead.hovered".into(), scoped(&session, "btn"))]),
    );
    assert_eq!(
        painted(&scene, session.view_nodes()),
        painted(&expected, reference.view_nodes())
    );
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
fn fade_scales_an_image_opacity_tinted_or_not_and_a_shadow_alpha() {
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
        matches!(tinted, Primitive::Image { tint: Some(c), opacity, .. } if c[3] == 200 && opacity == 0.5)
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

/// B0364: the sampler both the layout and the patch use, tracks included.
#[test]
fn track_markers_map_the_driver_through_their_keys_linearly_and_clamp() {
    use crate::layout::Marker;
    let marker = |extra: serde_json::Value| {
        let mut marker = serde_json::json!({"from": 0., "to": 1., "start": 0, "duration": 1.,
            "curve": "linear", "loop": false});
        let object = marker.as_object_mut().unwrap();
        object.extend(extra.as_object().unwrap().clone());
        Marker::parse(&marker)
    };
    let at = |seconds: f64| (seconds * 1e9) as u64;
    let track = marker(serde_json::json!({"keys": [0.25, 0.5, 0.75], "values": [10, 30, 20.]}))
        .expect("a track");
    assert_eq!(track.points(), 3);
    for (t, value) in [
        (0.25, 10.),
        (0.5, 30.),
        (0.75, 20.),
        (0.375, 20.),
        (0.625, 25.),
        (0., 10.),
        (0.1, 10.),
        (0.9, 20.),
        (1., 20.),
        (5., 20.),
    ] {
        assert_eq!(track.sample(at(t)), value, "t={t}");
    }
    // Keys are in the driver's value space, after its curve; a loop ping-pongs the driver.
    let eased = marker(
        serde_json::json!({"from": 2., "to": 4., "curve": "in-cubic",
        "keys": [2, 4], "values": [0, 100]}),
    )
    .unwrap();
    let d = 2. + 2. * f64::from(bead_core::animation::Curve::InCubic.ease(0.5));
    assert_eq!(eased.sample(at(0.5)), (d - 2.) * 50.);
    let looping =
        marker(serde_json::json!({"loop": true, "keys": [0, 1], "values": [0, 8]})).unwrap();
    assert_eq!(looping.sample(at(1.5)), looping.sample(at(0.5)));
    assert_eq!(looping.sample(at(0.5)), 4.);
    // Without keys the marker samples its driver, as before.
    let plain = marker(serde_json::json!({"values": [5, 6]})).unwrap();
    assert_eq!((plain.points(), plain.sample(at(0.25))), (0, 0.25));
    let many = |n: usize| (0..n).collect::<Vec<_>>();
    for bad in [
        serde_json::json!({"keys": [0, 1], "values": [0]}),
        serde_json::json!({"keys": [0], "values": [0]}),
        serde_json::json!({"keys": many(129), "values": many(129)}),
        serde_json::json!({"keys": [0, 0], "values": [1, 2]}),
        serde_json::json!({"keys": [1, 0], "values": [1, 2]}),
        serde_json::json!({"keys": [0, f64::NAN], "values": [1, 2]}),
        serde_json::json!({"keys": [0, 1], "values": [1, f64::INFINITY]}),
        serde_json::json!({"keys": [0, 1]}),
        serde_json::json!({"keys": 1, "values": [1, 2]}),
    ] {
        assert!(marker(bad.clone()).is_none(), "{bad}");
    }
    assert_eq!(
        marker(serde_json::json!({"keys": many(128), "values": many(128)}))
            .unwrap()
            .points(),
        128
    );
}

/// A hand-built track over a linear 0..1 driver of one second from clock 0 (B0364).
const TRACK: &str = "local function track(keys, values) return {__animate={from=0,to=1,start=0,\
duration=1,curve='linear',loop=false,keys=keys,values=values}} end\n";
/// Equal up to float rounding: a patch moves the cached scene, a layout computes it afresh.
fn assert_close(a: &serde_json::Value, b: &serde_json::Value, at: &str) {
    use serde_json::Value as J;
    match (a, b) {
        (J::Number(x), J::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() < 1e-3, "{at}: {x} != {y}");
        }
        (J::Array(x), J::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{at}");
            for (i, (x, y)) in x.iter().zip(y).enumerate() {
                assert_close(x, y, &format!("{at}/{i}"));
            }
        }
        (J::Object(x), J::Object(y)) => {
            assert!(x.keys().eq(y.keys()), "{at}");
            for (key, x) in x {
                assert_close(x, &y[key], &format!("{at}.{key}"));
            }
        }
        _ => assert_eq!(a, b, "{at}"),
    }
}
/// What a frame paints and hit-tests: primitives, clips, node boxes and clips, layer bounds.
fn frame(session: &Session, scene: &Scene) -> serde_json::Value {
    let nodes = session.view_nodes();
    let layers: Vec<_> = scene
        .layers
        .iter()
        .filter(|l| !l.id.starts_with("@scrollbar:"))
        .map(|l| l.bounds)
        .collect();
    let clips: Vec<_> = nodes.iter().map(|n| n.clip).collect();
    serde_json::to_value((painted(scene, nodes), clips, layers)).unwrap()
}
fn pin(seconds: f64) {
    crate::layout::PINNED_CLOCK.set(Some((seconds * 1e9) as u64));
}

#[test]
fn track_patches_equal_a_full_layout_through_nested_scale_translate_clip_and_scroll() {
    let view = format!(
        "{TRACK}{}",
        r#"local ui=require('bead.ui')
return ui.Column{id='root',width=400,height=300,fill='surface',
 ui.Box{id='outer',width=240,height=200,fill='primary',radius=8,border_color='onSurface',border_width=2,
  scale=track({0,0.5,1},{1,1.5,0.5}),translate_x=track({0,1},{0,32}),opacity=track({0,1},{1,0.5}),
  ui.Box{id='inner',width=120,height=60,fill='surfaceContainer',radius=4,clip_children=true,
   translate_y=track({0,0.5,1},{0,16,-16}),scale=track({0,1},{1,2}),
   ui.Text{id='label',text='Hi',width=50,height=20,font_size=12},
   ui.Box{id='deep',width=40,height=40,fill='onSurface',translate_x=track({0,1},{0,24})}},
  ui.Scroll{id='list',scroll=ui.bind('scroll'),width=120,height=40,
   ui.Column{id='rows',width=120,
    ui.Button{id='row1',text='Row 1',width=120,height=30,on_activate=ui.action('noop')},
    ui.Button{id='row2',text='Row 2',width=120,height=30,on_activate=ui.action('noop')}}}}}"#
    );
    let behavior = "return {stores={scroll=0},actions={noop=function(s) end},computed={}}";
    pin(0.);
    let (_fixture, mut session) = interaction_fixture(&view, behavior);
    let scroll = |offset: f64| BTreeMap::from([("scroll".into(), Value::Number(offset))]);
    for t in [0.25, 0.5, 0.8, 1.] {
        pin(0.);
        rebuilt(&mut session, scroll(0.));
        pin(t);
        let hits = patch_hits();
        let patched = session.update(scroll(10.)).unwrap().unwrap();
        assert_eq!(patch_hits(), hits + 1, "t={t}: the frame is a patch");
        session.commit(patched.clone());
        let patched = frame(&session, &patched);
        let full = rebuilt(&mut session, scroll(10.));
        assert_close(&patched, &frame(&session, &full), &format!("t={t}"));
    }
    // At t = 1 the subtree is really placed: outer at half size, inner twice its own.
    let inner = layer_bounds(&session.current, "inner");
    assert!(
        (inner.w - 120.).abs() < 1e-3 && (inner.h - 60.).abs() < 1e-3,
        "{inner:?}"
    );
    crate::layout::PINNED_CLOCK.set(None);
}

/// B0372: under a scale track a `bake_size` image keeps its baked radius, so every patched frame
/// reuses its raster; an image without `bake_size` rasters per rect and its radius scales.
#[test]
fn a_scale_track_reuses_a_baked_images_raster_on_every_patched_frame() {
    let view = format!(
        "{TRACK}{}",
        r#"local ui=require('bead.ui')
return ui.Row{id='root',width=300,height=120,fill='surface',
 ui.Box{id='pearl',width=60,height=60,scale=track({0,1},{0.8,1.2}),
  ui.Image{id='baked',position='absolute',left=0,top=0,width=60,height=60,path='ui/dot.svg',bake_size={w=90,h=90}},
  ui.Image{id='exact',position='absolute',left=0,top=0,width=60,height=60,path='ui/dot.svg'}}}"#
    );
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.0.join("ui")).unwrap();
    fs::write(
        fixture.0.join("ui/dot.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#f80"/></svg>"##,
    )
    .unwrap();
    fs::write(
        fixture.0.join("ui/behavior.luau"),
        "return {stores={},actions={},computed={}}",
    )
    .unwrap();
    fs::write(fixture.0.join("ui/main.luau"), view).unwrap();
    pin(0.);
    let mut session = fixture.session();
    session.entry = "ui/main.luau".into();
    let first = session.reload().unwrap();
    session.commit(first);
    let generation = session.generation.as_ref().unwrap();
    let mut cache = bead_render::tree::TreeCache::default()
        .with_image_policy(bead_render::tree::ImagePolicy::UploadOnly);
    let mut baked_payload = None;
    for t in [0.25, 0.5, 0.8, 1.] {
        pin(t);
        let hits = patch_hits();
        let (scene, _) = generation
            .materialize(&session.stores, &session.palette, session.viewport)
            .unwrap();
        assert_eq!(patch_hits(), hits + 1, "t={t}: the frame is a patch");
        let k = 0.8 + 0.4 * t as f32;
        for primitive in &scene.primitives {
            if let Primitive::Image {
                rect,
                radius,
                bake_size,
                ..
            } = primitive
            {
                assert!((rect.w - 60. * k).abs() < 1e-3, "t={t}: {rect:?}");
                let want = if bake_size.is_some() { 9. } else { 9. * k };
                assert!(
                    (radius - want).abs() < 1e-4,
                    "t={t}: radius {radius} != {want}"
                );
            }
        }
        cache.prepare(&scene, 1.).unwrap();
        let payload = cache
            .frame
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| {
                matches!(
                    item.primitive,
                    Primitive::Image {
                        bake_size: Some(_),
                        ..
                    }
                )
            })
            .and_then(|item| item.image.clone())
            .expect("the baked image has a payload");
        match &baked_payload {
            None => baked_payload = Some(payload),
            Some(previous) => assert!(
                std::sync::Arc::ptr_eq(previous, &payload),
                "t={t}: the baked raster is reused, not baked again"
            ),
        }
    }
    crate::layout::PINNED_CLOCK.set(None);
}

/// B0365: the markers `ui.track` builds are what the sampler reads, through layout and the patch.
#[test]
fn ui_track_markers_move_and_scale_a_box_through_layout_and_the_patch() {
    let view = r#"local ui=require('bead.ui')
local drive=ui.animated('d',1,60,'linear',0)
return ui.Row{id='root',width=300,height=120,fill='surface',
 ui.Box{id='b',width=100,height=40,fill='primary',translate_x=ui.track(drive,{0,.5,1},{0,40,100}),scale=ui.track(drive,{0,1},{1,2})}}"#;
    let (_fixture, mut session) =
        interaction_fixture(view, "return {stores={},actions={},computed={}}");
    // The driver runs 60 s from the build, so the live build clock is its start to a few ms (< 0.3 px).
    let base = bead_core::animation::clock_ns();
    let primary = |scene: &Scene| match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Rounded { fill, .. } if fill == &[138, 180, 248, 255]))
    {
        Some(Primitive::Rounded { rect, .. }) => *rect,
        _ => panic!("no box"),
    };
    for d in [0., 0.25, 0.5, 1_f64] {
        let (tx, scale) = (80. * d.min(0.5) + 120. * (d - 0.5).max(0.), 1. + d);
        crate::layout::PINNED_CLOCK.set(Some(base + (d * 60. * 1e9) as u64));
        let hits = patch_hits();
        let patched = session.relayout().unwrap().unwrap();
        assert_eq!(patch_hits(), hits + 1, "d={d}: the patch path");
        session.commit(patched.clone());
        let full = session.reload().unwrap();
        session.commit(full.clone());
        for (path, scene) in [("patch", &patched), ("layout", &full)] {
            let r = primary(scene);
            let (w, h) = (100. * scale, 40. * scale);
            let want = [50. + tx - w / 2., 20. - h / 2., w, h];
            for (got, want) in [r.x, r.y, r.w, r.h].map(f64::from).into_iter().zip(want) {
                assert!((got - want).abs() < 0.3, "d={d} {path}: {r:?} vs {want:?}");
            }
        }
    }
    crate::layout::PINNED_CLOCK.set(None);
}

/// B0370: a view whose box moves `to` px at a 30 s pin (the driver runs 60 s), optionally in a memo.
fn track_view(to: u32, memo: bool) -> String {
    let b = format!(
        "ui.Box{{id='b',width=100,height=40,fill='primary',translate_x=ui.track(drive,{{0,1}},{{0,{to}}})}}"
    );
    let b = if memo {
        format!("ui.memo('m',{{}},function() return {b} end)")
    } else {
        b
    };
    format!(
        "local ui=require('bead.ui')\nlocal drive=ui.animated('d',1,60,'linear',0)\nlocal _=ui.resolve_for_check(ui.bind('n'))\nreturn ui.Row{{id='root',width=300,height=120,fill='surface',{b}}}"
    )
}
fn box_x(scene: &Scene) -> f32 {
    match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Rounded { fill, .. } if fill == &[138, 180, 248, 255]))
    {
        Some(Primitive::Rounded { rect, .. }) => rect.x,
        _ => panic!("no box"),
    }
}
const N_STORE: &str = "return {stores={n=0},actions={},computed={}}";
fn set_n(session: &mut Session, n: u32) -> Scene {
    let scene = session
        .update(BTreeMap::from([("n".into(), Value::Number(n.into()))]))
        .unwrap()
        .unwrap();
    session.commit(scene.clone());
    scene
}

/// B0370: a kept raw re-lays out from its tokens after other views built on the thread.
#[test]
fn tracks_of_a_kept_raw_outlive_other_views_builds() {
    let (_a, mut first) = interaction_fixture(&track_view(80, false), N_STORE);
    let (_b, mut other) = interaction_fixture(&track_view(40, false), N_STORE);
    crate::layout::PINNED_CLOCK.set(Some(bead_core::animation::clock_ns() + 30_000_000_000));
    assert!((box_x(&set_n(&mut first, 1)) - 40.).abs() < 0.3);
    for n in 1..=4 {
        set_n(&mut other, n);
    }
    frontend::reset_eval_calls();
    let overlay =
        Palette::parse(br#"{"formatVersion":1,"colours":{"onSurfaceVariant":"ff0000"}}"#).unwrap();
    let after = first.set_runtime_palette(overlay).unwrap().unwrap();
    assert_eq!(
        frontend::eval_calls(),
        0,
        "the kept raw is served without Luau"
    );
    assert!((box_x(&after) - 40.).abs() < 0.3);
    crate::layout::PINNED_CLOCK.set(None);
}

/// B0370: equal points share a token while held (so an unchanged view keeps its raw identity);
/// a dead token resolves to nothing.
#[test]
fn equal_tracks_share_a_token_while_held_and_expire_after() {
    use crate::layout::{hand_over_track, tracks_begin_build};
    let token = |h: &crate::layout::TrackHandle| serde_json::to_value(h).unwrap();
    let held = hand_over_track(vec![[0., 1.], [1., 2.]]);
    let same = hand_over_track(vec![[0., 1.], [1., 2.]]);
    assert_eq!(token(&held), token(&same));
    assert_ne!(
        token(&held),
        token(&hand_over_track(vec![[0., 1.], [1., 3.]]))
    );
    let marker = |t| serde_json::json!({"from":0,"to":1,"start":0,"duration":1,"curve":"linear","loop":false,"track":t});
    assert!(layout::Marker::parse(&marker(token(&held))).is_some());
    let gone = token(&held);
    drop((held, same));
    tracks_begin_build();
    assert!(
        layout::Marker::parse(&marker(gone)).is_none(),
        "a dead token is an invalid marker"
    );
}

/// B0370: a track inside `ui.memo` stays in the JSON, so a memo hit keeps sampling it.
#[test]
fn memo_tracks_sample_on_every_rebuild() {
    let (_f, mut session) = interaction_fixture(&track_view(80, true), N_STORE);
    crate::layout::PINNED_CLOCK.set(Some(bead_core::animation::clock_ns() + 30_000_000_000));
    for n in 1..=3 {
        assert!((box_x(&set_n(&mut session, n)) - 40.).abs() < 0.3, "n={n}");
    }
    crate::layout::PINNED_CLOCK.set(None);
}

#[test]
fn scale_prop_and_marker_scale_text_and_radius_but_not_strokes() {
    let view = format!(
        "{TRACK}{}",
        r#"local ui=require('bead.ui')
return ui.Row{id='root',width=300,height=120,fill='surface',
 ui.Box{id='group',width=100,height=60,fill='primary',radius=6,border_color='onSurface',border_width=2,{{S}}
  ui.Text{id='label',text='Hi',width=50,height=20,font_size=10},
  ui.Arc{id='arc',width=20,height=20,stroke_width=3,color='primary'}}}"#
    );
    let behavior = "return {stores={},actions={},computed={}}";
    pin(0.5);
    let scene = |s: &str| {
        let (fixture, mut session) = interaction_fixture(&view.replace("{{S}}", s), behavior);
        let full = session.reload().unwrap();
        session.commit(full.clone());
        (fixture, session, full)
    };
    let (_plain, _, plain) = scene("");
    let (fixture, mut session, scaled) = scene("scale=1.5,");
    let (_sampled, _, sampled) = scene("scale=track({0,1},{1.5,1.5}),");
    assert_eq!(
        json(&scaled),
        json(&sampled),
        "a static scale equals a marker's"
    );
    assert_eq!(json(&scene("scale=1,").2), json(&plain));
    let group = |scene: &Scene| match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Rounded { fill, .. } if fill == &[138, 180, 248, 255]))
    {
        Some(Primitive::Rounded {
            rect,
            radius,
            border,
            ..
        }) => (*rect, *radius, border.unwrap().1),
        _ => panic!("no group box"),
    };
    let arc = |scene: &Scene| match scene
        .primitives
        .iter()
        .find(|p| matches!(p, Primitive::Arc { .. }))
    {
        Some(Primitive::Arc {
            rect, stroke_width, ..
        }) => (*rect, *stroke_width),
        _ => panic!("no arc"),
    };
    let ((rect, radius, border), (box_rect, ..)) = (group(&scaled), group(&plain));
    assert_eq!((rect.w, rect.h, radius, border), (150., 90., 9., 2.));
    let centre = |r: Rect| [r.x + r.w / 2., r.y + r.h / 2.];
    assert_close(
        &serde_json::json!(centre(rect)),
        &serde_json::json!(centre(box_rect)),
        "about the laid-out centre",
    );
    assert_eq!(text_of(&scaled, "Hi").1, 15.);
    assert_eq!((arc(&scaled).0.w, arc(&scaled).1), (30., 3.));
    // The patch of a statically scaled view lands on the same scene.
    let hits = patch_hits();
    let patched = session.relayout().unwrap().unwrap();
    assert_eq!(patch_hits(), hits + 1);
    assert_close(
        &serde_json::to_value(&patched.primitives).unwrap(),
        &serde_json::to_value(&scaled.primitives).unwrap(),
        "patch",
    );
    for bad in ["scale=0,", "scale=9,", "scale=-1,"] {
        fs::write(fixture.0.join("ui/main.luau"), view.replace("{{S}}", bad)).unwrap();
        let error = session.reload().err().expect("an invalid scale");
        assert!(
            error
                .message
                .contains("scale must be above 0 and at most 8"),
            "{bad}: {error}"
        );
    }
    crate::layout::PINNED_CLOCK.set(None);
}

#[test]
fn a_view_over_the_track_point_cap_fails_with_a_diagnostic() {
    let view = |boxes: usize| {
        let track = serde_json::json!({"__animate": {"from": 0., "to": 1., "start": 0,
            "duration": 1., "curve": "linear", "loop": false,
            "keys": (0..128).collect::<Vec<_>>(), "values": vec![0.; 128]}});
        let source = serde_json::json!({"file": "ui/main.luau", "line": 1});
        let child = |i: usize| {
            serde_json::json!({"kind": "box", "props": {"id": format!("b{i}"), "translate_x": track},
                "children": [], "source": source, "scope": null})
        };
        let raw: crate::layout::RawNode = serde_json::from_value(serde_json::json!({
            "kind": "column", "props": {"id": "root"},
            "children": (0..boxes).map(child).collect::<Vec<_>>(), "source": source, "scope": null}))
        .unwrap();
        crate::layout::ViewSpec::from_raw(&raw, &|_: &str| Ok(Value::Bool(false))).map(|_| ())
    };
    assert!(view(256).is_ok(), "exactly 32768 points fit");
    let error = view(257).expect_err("over the cap");
    assert!(error.message.contains("more than 32768 points"), "{error}");
}

/// B0364 rough timing, not asserted: a ~190-node view with ~130 track markers (16 subtrees,
/// 9-46 keys) patched per frame. `SCALE=1` adds scale tracks (absent before B0364).
#[test]
#[ignore = "timing probe: cargo test --release -p bead-ui --lib patch_timing_probe -- --ignored --nocapture"]
fn patch_timing_probe() {
    let scale = std::env::var("SCALE").is_ok_and(|v| v == "1");
    let view = r#"local ui=require('bead.ui')
local d=ui.animated('d',ui.resolve_for_check(ui.bind('target')),60,'linear')
local function track(n,base,amp)
 local a=d.__animate
 local m={from=a.from,to=a.to,start=a.start,duration=a.duration,curve=a.curve,loop=a.loop,keys={},values={}}
 for i=1,n do m.keys[i]=(i-1)/(n-1) m.values[i]=base+amp*math.sin(i) end
 return {__animate=m}
end
local root={id='root',width=800,height=600,fill='surface'}
for p=1,16 do
 local n=9+(p*7)%38
 local pearl={id='p'..p,width=40,height=40,fill='primary',radius=20,
  translate_x=track(n,0,20),translate_y=track(n,0,30),opacity=track(n,0.5,0.4){{SCALE}}}
 for c=1,11 do
  local leaf={width=3,height=3,fill='onSurface'}
  if c<=5 then leaf.translate_y=track(n,0,4) end
  pearl[c]=ui.Box(leaf)
 end
 root[p]=ui.Box(pearl)
end
return ui.Row(root)"#
        .replace("{{SCALE}}", if scale { ",scale=track(n,1,0.2)" } else { "" });
    let behavior = "return {stores={target=0},actions={},computed={}}";
    let (_fixture, mut session) = interaction_fixture(&view, behavior);
    let scene = session
        .update(BTreeMap::from([("target".into(), Value::Number(1.))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let generation = session.generation.as_ref().unwrap();
    let (hits, mut times) = (patch_hits(), vec![]);
    for _ in 0..400 {
        let start = std::time::Instant::now();
        let (scene, _) = generation
            .materialize(&session.stores, &session.palette, session.viewport)
            .unwrap();
        times.push(start.elapsed().as_secs_f64() * 1e6);
        std::hint::black_box(scene);
    }
    let patched = patch_hits() - hits;
    times.sort_by(f64::total_cmp);
    println!(
        "PATCH_PROBE scale={scale} layers={} frames=400 patched={patched} min_us={:.1} median_us={:.1} p90_us={:.1}",
        session.current.layers.len(),
        times[0],
        times[200],
        times[360]
    );
}

/// B0369 move-build timing, not asserted: every iteration moves the bound target, so markup re-runs,
/// the patch misses and view_ir, layout and the cache copy all run. A generic view of items, dots and
/// a ring on one `ui.animated` driver. `SHAPE=tap`: 12 tracked items (+9 static), 109 tracks, ~930
/// points, ~240 nodes; `SHAPE=jump`: 35 items, 362 tracks, ~8.3k points, ~360 nodes. `TRACKS=0`: the
/// same view with static props; `bare`: compute every track's keys and values, draw static props;
/// `only`: also call `ui.track`, drop the track. Stage spans follow `MOVE_PROBE warm` with
/// `BEAD_PROFILE_INPUT=1`.
#[test]
#[ignore = "timing probe: BEAD_PROFILE_INPUT=1 SHAPE=jump cargo test -p bead-ui --lib move_build_probe -- --ignored --nocapture"]
fn move_build_probe() {
    const VIEW: &str = r#"local ui=require('bead.ui')
local t=ui.resolve_for_check(ui.bind('target'))
local d=ui.animated('d',t+SPAN,0.5,'out-quint',t)
local a,b=ui.range(d)
local lo,hi,count=math.min(a,b)-0.5,math.max(a,b)+0.5,0
-- A catenary-like curve over the driver position x for the thing at slot s.
local function curve(x,s) local u=(x-s)/8 return math.cosh(u)*0.6-math.sqrt(1+u*u)*0.4 end
local function tx(c) return 30*c end
local function ty(c) return 12*c end
local function sc(c) return math.clamp(1+0.1*c,0.5,2) end
local function op(c) return math.clamp(0.7+0.3*c,0,1) end
local function prop(live,s,fn)
 local static=fn(curve(hi,s))
 if MODE=='0' or not live then return static end
 count+=1
 local n,keys,values=KLO+(count*7)%KW,{},{}
 for i=1,n do local x=lo+(hi-lo)*(i-1)/(n-1) keys[i]=x values[i]=fn(curve(x,s)) end
 if MODE=='bare' then return static end
 local track=ui.track(d,keys,values)
 return MODE=='only' and static or track
end
local function points() local p={} for i=1,256 do p[i]={(i-1)/255,0.5+0.4*math.sin(i/20)} end return p end
local deco={gap=4}
for r=1,10 do local row={gap=2} for c=1,6 do row[c]=ui.Box{width=20,height=20,fill='primary',radius=4} end deco[r]=ui.Row(row) end
local strip={gap=8,height=120}
for j=1,ITEMS+IDLE do
 local s,live=j-6,j<=ITEMS
 local dist=math.max(0,-s,s-SPAN)
 strip[j]=ui.Button{id='i'..j,width=44,height=44,on_activate=ui.action('noop'),
  translate_x=prop(live,s,tx),translate_y=prop(live,s,ty),scale=prop(live,s,sc),opacity=prop(live,s,op),
  ui.Box{width=40,height=40,
   ui.Box{width=40,height=40,fill='primary',opacity=prop(live,s,op)},
   ui.Box{width=40,height=40,fill='onSurface',opacity=prop(live and dist<2.5,s,op)},
   ui.Box{width=40,height=40,fill='onSurface',opacity=prop(live and dist<2.5,s,op)},
   ui.Box{width=40,height=40,fill='surface',opacity=prop(live and dist<0.7,s,op)},
   ui.Box{width=40,height=40,border_color='outlineVariant',border_width=1}}}
end
for k=1,DOTS do
 local s=k-5.5
 strip[ITEMS+IDLE+k]=ui.Box{width=6,height=6,radius=3,fill='primary',
  translate_x=prop(true,s,tx),translate_y=prop(true,s,ty),opacity=prop(true,s,op)}
end
local ring={width=120,height=120,scale=prop(true,0,sc),opacity=prop(true,0,op)}
for r=1,5 do
 ring[r]=ui.Arc{position='absolute',left=0,top=0,width=120,height=120,start_angle=r*72-90,sweep_angle=60,stroke_width=3,color='primary'}
end
return ui.Column{id='root',width=2400,height=900,fill='surface',
 ui.Polyline{width=1600,height=160,points=points()},ui.Polyline{width=1600,height=160,points=points()},
 ui.Column(deco),ui.Row(strip),ui.Box(ring)}"#;
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.into());
    let (shape, mode) = (env("SHAPE", "tap"), env("TRACKS", "1"));
    // Tracked items, static items, dots, travel in slots, keys per track (lowest, spread).
    let [items, idle, dots, span, klo, kw] = match shape.as_str() {
        "jump" => [35, 0, 34, 24, 2, 43],
        _ => [12, 9, 11, 1, 6, 6],
    };
    let view = format!(
        "local ITEMS,IDLE,DOTS,SPAN,KLO,KW,MODE={items},{idle},{dots},{span},{klo},{kw},'{mode}'\n{VIEW}"
    );
    let behavior = "return {stores={target=0},actions={noop=function(s) end},computed={}}";
    let (_fixture, mut session) = interaction_fixture(&view, behavior);
    let (hits, mut times) = (patch_hits(), vec![]);
    for i in 0..45_u32 {
        if i == 5 {
            println!("MOVE_PROBE warm");
        }
        let target = Value::Number(1. + f64::from(i) * 0.01);
        let start = std::time::Instant::now();
        let scene = session
            .update(BTreeMap::from([("target".into(), target)]))
            .unwrap()
            .unwrap();
        times.push(start.elapsed().as_secs_f64() * 1e3);
        assert_eq!(patch_hits(), hits, "a move build must miss the patch");
        session.commit(scene);
    }
    let mut times = times.split_off(5);
    times.sort_by(f64::total_cmp);
    println!(
        "MOVE_PROBE shape={shape} tracks={mode} nodes={} layers={} iters=40 median_ms={:.3} p90_ms={:.3}",
        session.view_nodes().len(),
        session.current.layers.len(),
        times[20],
        times[36]
    );
}

/// B0419: a 200 px Scroll over 1000 px of content at offset 100; its text sits 150 px below the clip.
const PLANE_VIEW: &str = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=300,fill='surface',
 ui.Scroll{id='list',scroll=ui.bind('scroll'),width=180,height=200,
  ui.Column{id='rows',width=180,height=1000,shrink=0,
   ui.Box{id='gap',width=180,height=450,shrink=0},
   ui.Text{id='far',text='Far row',width=180,height=20,shrink=0}}}}"#;
const PLANE_BEHAVIOR: &str = "return {stores={scroll=100},actions={},computed={}}";
fn plane_of(scene: &Scene) -> (String, bead_render::ScrollPlane) {
    let layer = scene.layers.iter().find(|l| l.scroll.is_some());
    let layer = layer.expect("a scroll plane");
    (layer.id.clone(), layer.scroll.unwrap())
}
fn scrolling(id: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([("bead.scrolling".into(), Value::Text(id.into()))])
}

#[test]
fn scroll_plane_geometry_at_rest_and_while_moving() {
    let (_fixture, mut session) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (id, rest) = plane_of(&session.current);
    let clip = Rect::new(0., 0., 180., 200.);
    assert_eq!(
        (
            rest.offset,
            rest.limit,
            rest.clip,
            rest.interest,
            rest.ready
        ),
        (100., 800., clip, clip, (100., 300.)),
        "at rest the interest is the clip and ready its content span"
    );
    let moving = session.update(scrolling(&id)).unwrap().unwrap();
    let (_, plane) = plane_of(&moving);
    // One viewport of overscan each way, clamped to the content's top (at -100).
    assert_eq!(
        (plane.offset, plane.clip, plane.interest, plane.ready),
        (100., clip, Rect::new(0., -100., 180., 500.), (0., 500.))
    );
    // `overscan` is a Scroll prop within 0..3.
    for (view, message) in [
        (
            PLANE_VIEW.replace("height=200,", "height=200,overscan=4,"),
            "overscan must be 0..3",
        ),
        (
            PLANE_VIEW.replace("id='gap',", "id='gap',overscan=1,"),
            "overscan needs a Scroll",
        ),
        (
            PLANE_VIEW.replace("id='gap',", "id='gap',ready={from=0,to=1},"),
            "ready needs a Scroll",
        ),
        (
            PLANE_VIEW.replace("height=200,", "height=200,ready={from=9,to=1},"),
            "ready must be {from=, to=}",
        ),
        (
            PLANE_VIEW.replace("height=200,", "height=200,ready=3,"),
            "ready must be {from=, to=}",
        ),
    ] {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("ui")).unwrap();
        fs::write(fixture.0.join("ui/behavior.luau"), PLANE_BEHAVIOR).unwrap();
        fs::write(fixture.0.join("ui/main.luau"), view).unwrap();
        let mut session = fixture.session();
        session.entry = "ui/main.luau".into();
        let error = session.reload().err().expect("a markup error");
        assert!(error.message.contains(message), "{}", error.message);
    }
}

#[test]
fn scroll_plane_rasters_the_text_below_its_clip_only_while_moving() {
    use bead_render::tree::TreeCache;
    let (_fixture, mut session) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let far = |scene: &Scene| {
        let text = |p: &Primitive| matches!(p, Primitive::Text { text, .. } if text == "Far row");
        let far = scene.primitives.iter().position(text);
        far.expect("the far text is in the scene")
    };
    let rasters = |cache: &mut TreeCache, scene: &Scene| {
        cache.prepare(scene, 1.).unwrap();
        cache.frame.as_ref().unwrap().items[far(scene)]
            .raster
            .is_some()
    };
    let rest = session.current.clone();
    assert_eq!(
        crate::rect(&rest.primitives[far(&rest)]).y,
        350.,
        "150 px below the clip"
    );
    let mut cache = TreeCache::default();
    assert!(!rasters(&mut cache, &rest), "at rest only the clip rasters");
    let (id, _) = plane_of(&rest);
    let moving = session.update(scrolling(&id)).unwrap().unwrap();
    session.commit(moving.clone());
    assert_eq!(moving.primitives.len(), rest.primitives.len());
    assert!(
        rasters(&mut cache, &moving),
        "while bead.scrolling names the plane, its overscan text rasters"
    );
    let settled = session.update(scrolling("")).unwrap().unwrap();
    assert!(!rasters(&mut cache, &settled), "the settle drops it again");
}

#[test]
fn scroll_plane_follows_a_scroll_patch_with_no_luau() {
    let (_fixture, mut session) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (_reference_fixture, mut reference) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (id, _) = plane_of(&session.current);
    let moving = session.update(scrolling(&id)).unwrap().unwrap();
    session.commit(moving);
    frontend::reset_eval_calls();
    let hits = patch_hits();
    let step = BTreeMap::from([("scroll".into(), Value::Number(300.))]);
    let patched = session.update(step.clone()).unwrap().unwrap();
    assert_eq!(patch_hits(), hits + 1, "the scroll step is a patch");
    assert_eq!(frontend::eval_calls(), 0, "and runs no Luau");
    let (_, plane) = plane_of(&patched);
    assert_eq!(
        (plane.offset, plane.interest, plane.ready),
        (300., Rect::new(0., -200., 180., 600.), (100., 700.))
    );
    let mut stores = step;
    stores.extend(scrolling(&id));
    assert_eq!(
        plane_of(&rebuilt(&mut reference, stores)).1,
        plane,
        "a full layout of the same stores places the same plane"
    );
}

#[test]
fn scroll_settled_clears_the_named_plane_with_no_luau_and_ignores_another() {
    let (_fixture, mut session) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (id, _) = plane_of(&session.current);
    let moving = session.update(scrolling(&id)).unwrap().unwrap();
    session.commit(moving);
    assert!(session.scroll_settled("other").unwrap().is_none());
    assert_eq!(
        session.stores.get("bead.scrolling"),
        Some(&Value::Text(id.clone()))
    );
    frontend::reset_eval_calls();
    let hits = patch_hits();
    let rest = session.scroll_settled(&id).unwrap().unwrap();
    assert_eq!(patch_hits(), hits + 1, "the settle is a patch");
    assert_eq!(frontend::eval_calls(), 0, "and runs no Luau");
    let (_, plane) = plane_of(&rest);
    assert_eq!(plane.interest, plane.clip);
    session.commit(rest);
    assert_eq!(
        session.stores.get("bead.scrolling"),
        Some(&Value::Text(String::new()))
    );
}

/// B0419 U5a2: `on_settle` runs once per settle of a viewport that moved, with the final offset.
#[test]
fn on_settle_runs_once_with_the_final_offset_after_a_burst() {
    let view = PLANE_VIEW.replace("height=200,", "height=200,on_settle=ui.action('landed'),");
    let behavior = "return {stores={scroll=100,landed=-1,count=0},\
        actions={landed=function(s,offset) s.landed=offset;s.count=s.count+1 end},computed={}}";
    let (_fixture, mut session) = interaction_fixture(&view, behavior);
    let (id, _) = plane_of(&session.current);
    let viewports = session.scroll_viewports();
    assert_eq!(viewports.len(), 1);
    let v = &viewports[0];
    assert_eq!(
        (v.id.as_str(), v.rect, v.horizontal, v.offset, v.settle),
        (
            id.as_str(),
            Rect::new(0., 0., 180., 200.),
            false,
            100.,
            true
        )
    );
    let landed = |s: &Session| {
        (
            s.stores.get("landed").cloned(),
            s.stores.get("count").cloned(),
        )
    };
    let number = |n: f64| Some(Value::Number(n));
    assert!(
        session.scroll_settled(&id).unwrap().is_none(),
        "nothing moved"
    );
    // Worker path: three steps, then the settle.
    for delta in [30., 30., 30.] {
        let scene = session.scroll(90., 100., delta, false).unwrap().unwrap();
        session.commit(scene);
    }
    assert_eq!(landed(&session), (number(-1.), number(0.)), "not per step");
    let scene = session.scroll_settled(&id).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(landed(&session), (number(190.), number(1.)));
    assert!(session.scroll_settled(&id).unwrap().is_none(), "once");
    // Compositing path: main named the plane; the settle clears it and lands in one stage.
    let moving = session.update(scrolling(&id)).unwrap().unwrap();
    session.commit(moving);
    let scene = session.scroll(90., 100., -40., false).unwrap().unwrap();
    session.commit(scene);
    let scene = session.scroll_settled(&id).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(landed(&session), (number(150.), number(2.)));
    assert_eq!(
        session.stores.get("bead.scrolling"),
        Some(&Value::Text(String::new()))
    );
    for (view, message) in [
        (
            PLANE_VIEW.replace("id='gap',", "id='gap',on_settle=ui.action('landed'),"),
            "on_settle needs a Scroll",
        ),
        (
            PLANE_VIEW.replace("height=200,", "height=200,on_settle='landed',"),
            "on_settle requires ui.action",
        ),
    ] {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("ui")).unwrap();
        fs::write(fixture.0.join("ui/behavior.luau"), behavior).unwrap();
        fs::write(fixture.0.join("ui/main.luau"), view).unwrap();
        let mut session = fixture.session();
        session.entry = "ui/main.luau".into();
        let error = session.reload().err().expect("a markup error");
        assert!(error.message.contains(message), "{}", error.message);
    }
}

#[test]
fn scroll_plane_at_rest_adds_no_primitive_or_raster() {
    use bead_render::tree::TreeCache;
    let (_fixture, mut session) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (_reference_fixture, mut reference) = interaction_fixture(PLANE_VIEW, PLANE_BEHAVIOR);
    let (id, _) = plane_of(&session.current);
    for changes in [scrolling(&id), scrolling("")] {
        let scene = session.update(changes).unwrap().unwrap();
        session.commit(scene);
    }
    let rest = session.current.clone();
    let (_, plane) = plane_of(&rest);
    assert_eq!(plane.interest, plane.clip);
    // Today's scene is the same layout without planes.
    let mut today = rest.clone();
    for layer in &mut today.layers {
        layer.scroll = None;
    }
    let rasters = |scene: &Scene| {
        let mut cache = TreeCache::default();
        cache.prepare(scene, 1.).unwrap();
        let items = &cache.frame.as_ref().unwrap().items;
        items
            .iter()
            .map(|item| {
                let raster = item.raster.as_ref();
                let raster = raster.map(|r| (r.key, r.pixels.data().len()));
                (item.id.clone(), item.clip, raster)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rasters(&rest), rasters(&today));
    let full = rebuilt(&mut reference, BTreeMap::new());
    assert_eq!(
        painted(&rest, session.view_nodes()),
        painted(&full, reference.view_nodes()),
        "a settled plane paints exactly like a fresh layout"
    );
}

#[test]
fn scroll_plane_patch_follows_an_ancestor_mid_animation() {
    let view = format!(
        "{TRACK}{}",
        r#"local ui=require('bead.ui')
return ui.Column{id='root',width=300,height=400,fill='surface',
 ui.Box{id='slide',width=200,height=300,clip_children=true,translate_y=track({0,1},{0,40}),
  ui.Scroll{id='list',scroll=ui.bind('scroll'),width=180,height=200,
   ui.Column{id='rows',width=180,height=1000,shrink=0,
    ui.Text{id='row',text='Row',width=180,height=20,shrink=0}}}}}"#
    );
    pin(0.);
    let behavior = "return {stores={scroll=0},actions={},computed={}}";
    let (_fixture, mut session) = interaction_fixture(&view, behavior);
    let (id, _) = plane_of(&session.current);
    let stores = |offset: f64| {
        let mut stores = scrolling(&id);
        stores.insert("scroll".into(), Value::Number(offset));
        stores
    };
    let json = |p: bead_render::ScrollPlane| {
        serde_json::to_value((p.offset, p.limit, p.clip, p.interest, p.ready)).unwrap()
    };
    for t in [0.25, 0.5, 1.] {
        pin(0.);
        rebuilt(&mut session, stores(0.));
        pin(t);
        let hits = patch_hits();
        let patched = session.update(stores(50.)).unwrap().unwrap();
        assert_eq!(patch_hits(), hits + 1, "t={t}: the frame is a patch");
        session.commit(patched.clone());
        let (_, plane) = plane_of(&patched);
        let y = 40. * t as f32;
        assert!((plane.clip.y - y).abs() < 1e-3, "t={t}: {plane:?}");
        let full = rebuilt(&mut session, stores(50.));
        assert_close(&json(plane), &json(plane_of(&full).1), &format!("t={t}"));
    }
    crate::layout::PINNED_CLOCK.set(None);
}

#[test]
fn scroll_plane_ready_is_the_declared_span_within_the_interest() {
    let ready = |span: &str| {
        let view = PLANE_VIEW.replace("height=200,", &format!("height=200,ready={span},"));
        let (_fixture, mut session) = interaction_fixture(&view, PLANE_BEHAVIOR);
        let (id, rest) = plane_of(&session.current);
        let moving = session.update(scrolling(&id)).unwrap().unwrap();
        (rest.ready, plane_of(&moving).1.ready)
    };
    // Rest: the clip spans 100..300; moving: 0..500 (the content's top clamps the overscan).
    assert_eq!(ready("{from=120,to=260}"), ((120., 260.), (120., 260.)));
    assert_eq!(ready("{from=-50,to=2000}"), ((100., 300.), (0., 500.)));
}

/// B0419 U5a: an x Scroll (`strip`, 180 px over 1000 px, at 100) at the top of a y Scroll.
const AXIS_VIEW: &str = r#"local ui=require('bead.ui')
return ui.Column{id='root',width=200,height=300,fill='surface',
 ui.Scroll{id='page',scroll=ui.bind('sy'),width=200,height=200,
  ui.Column{id='body',width=200,height=600,shrink=0,
   ui.Scroll{id='strip',axis='x',scroll=ui.bind('sx'),width=180,height=100,shrink=0,
    ui.Row{id='cells',width=1000,height=100,shrink=0,
     ui.Box{id='gap',width=450,height=100,shrink=0},
     ui.Text{id='far',text='Far cell',width=100,height=20,shrink=0}}}}}}"#;
const AXIS_BEHAVIOR: &str = "return {stores={sx=100,sy=0},actions={},computed={}}";

#[test]
fn axis_x_places_content_by_the_x_offset_and_takes_only_x_deltas() {
    let (_fixture, mut session) = interaction_fixture(AXIS_VIEW, AXIS_BEHAVIOR);
    let (_reference_fixture, mut reference) = interaction_fixture(AXIS_VIEW, AXIS_BEHAVIOR);
    let far = |scene: &Scene| *text_of(scene, "Far cell").0;
    let node = |s: &Session, id: &str| {
        let n = s.view_nodes().iter().find(|n| n.id.ends_with(id)).unwrap();
        (n.scroll_limit, n.scroll_horizontal, n.scrollbar)
    };
    let store = |s: &Session, id: &str| s.stores.get(id).cloned();
    assert_eq!(node(&session, "strip"), (Some(820.), true, false));
    assert_eq!(node(&session, "page"), (Some(400.), false, true));
    let plane = |s: &Session, id: &str| {
        let layer = s.current.layers.iter().find(|l| l.id.ends_with(id));
        layer.unwrap().scroll.is_some()
    };
    assert!(!plane(&session, "strip"), "no x plane until main steps x");
    assert!(plane(&session, "page"));
    assert_eq!(
        far(&session.current).x,
        350.,
        "450 px in, scrolled 100 along x"
    );
    let y = far(&session.current).y;
    // An x delta over the strip moves it, as a patch that equals a full layout.
    let hits = patch_hits();
    let scene = session.scroll(90., 50., 50., true).unwrap().unwrap();
    assert_eq!(patch_hits(), hits + 1, "the x step is a patch");
    session.commit(scene.clone());
    assert_eq!((far(&scene).x, far(&scene).y), (300., y));
    assert_eq!(store(&session, "sx"), Some(Value::Number(150.)));
    let full = rebuilt(
        &mut reference,
        BTreeMap::from([("sx".into(), Value::Number(150.))]),
    );
    assert_close(
        &frame(&session, &scene),
        &frame(&reference, &full),
        "x patch",
    );
    let x_scroll = |s: &Session| {
        let tree = s.accessibility_tree();
        tree.nodes.iter().find_map(|(_, n)| n.scroll_x())
    };
    assert_eq!(x_scroll(&session), Some(150.));
    // A y delta over the strip passes to the page; an x delta over the page alone moves nothing.
    let scene = session.scroll(90., 50., 30., false).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(
        (store(&session, "sx"), store(&session, "sy")),
        (Some(Value::Number(150.)), Some(Value::Number(30.)))
    );
    assert!(session.scroll(100., 150., 40., true).unwrap().is_none());
    assert_eq!(store(&session, "sy"), Some(Value::Number(30.)));
    // Vertical keys over the strip move the page.
    session
        .scroll_key("page-down", false, Some((90., 50.)))
        .unwrap();
    assert_eq!(session.keyboard_scroll.as_ref().unwrap().store, "sy");
    for (view, message) in [
        (AXIS_VIEW.replace("axis='x'", "axis='z'"), "axis must be"),
        (
            AXIS_VIEW.replace("id='gap',", "id='gap',axis='x',"),
            "axis needs a Scroll",
        ),
    ] {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("ui")).unwrap();
        fs::write(fixture.0.join("ui/behavior.luau"), AXIS_BEHAVIOR).unwrap();
        fs::write(fixture.0.join("ui/main.luau"), view).unwrap();
        let mut session = fixture.session();
        session.entry = "ui/main.luau".into();
        let error = session.reload().err().expect("a markup error");
        assert!(error.message.contains(message), "{}", error.message);
    }
}

/// B0419 U4: bands of half a viewport. Luau runs once per band crossing, not per step.
fn band_evals(main: &str, steps: &[f64]) -> (Fixture, Session, Vec<usize>) {
    let behavior = "local rows={};for i=1,400 do rows[i]={id=tostring(i)} end;\
        return {stores={scroll=0,rows=rows},actions={select=function()end},computed={}}";
    let (fixture, mut session) = interaction_fixture(main, behavior);
    let evals = steps
        .iter()
        .map(|&offset| {
            frontend::reset_eval_calls();
            let next = session
                .update(BTreeMap::from([("scroll".into(), Value::Number(offset))]))
                .unwrap();
            if let Some(scene) = next {
                session.commit(scene);
            }
            frontend::eval_calls()
        })
        .collect();
    (fixture, session, evals)
}
const LIST_FIXED: &str = r#"local ui=require('bead.ui');local list=require('bead.widgets.kit.virtual-list')
return list{id='list',width=200,height=100,row_height=32,gap=0,scroll=ui.bind('scroll'),model=ui.bind('rows'),key='id',render=function(item)return ui.Button{id='row',text=item.id,width='fill',height=32,on_activate=ui.action('select',item.id)}end}"#;
const LIST_VARIABLE: &str = r#"local ui=require('bead.ui');local list=require('bead.widgets.kit.virtual-list')
return list{id='list',width=200,height=100,row_height=function(item)return 20+tonumber(item.id)%3*10 end,gap=2,scroll=ui.bind('scroll'),model=ui.bind('rows'),key='id',render=function(item)return ui.Button{id='row',text=item.id,width='fill',height=20+tonumber(item.id)%3*10,on_activate=ui.action('select',item.id)}end}"#;
const GRID: &str = r#"local ui=require('bead.ui');local kit=require('bead.kit')
return ui.Panel{id='panel',width=200,height=300,kit.VirtualGrid{id='grid',width=200,height=100,min_cell_width=100,gap_x=0,gap_y=0,row_height=40,scroll=ui.bind('scroll'),model=ui.bind('rows'),key='id',render=function(item)return ui.Button{id='row',text=item.id,width='fill',height=40,on_activate=ui.action('select',item.id)}end}}"#;

/// Bands are 50 px (half the 100 px viewport); the window index trails by one band.
fn assert_band_evals(main: &str) {
    let (_fixture, _session, evals) = band_evals(main, &[10., 40., 60., 90., 100., 110., 90.]);
    assert_eq!(evals, [0, 0, 0, 0, 1, 0, 1]);
}
#[test]
fn kit_fixed_list_reruns_luau_only_when_a_step_crosses_a_band() {
    assert_band_evals(LIST_FIXED);
}
#[test]
fn kit_variable_list_reruns_luau_only_when_a_step_crosses_a_band() {
    assert_band_evals(LIST_VARIABLE);
}
#[test]
fn kit_grid_reruns_luau_only_when_a_step_crosses_a_band() {
    assert_band_evals(GRID);
}

#[test]
fn kit_lists_at_rest_show_the_same_rows_at_the_same_positions() {
    let rows = |main: &str, offset: f64| {
        let (_fixture, session, _) = band_evals(main, &[offset]);
        let nodes = session.view_nodes();
        let top = nodes
            .iter()
            .find(|n| n.scrollable)
            .map(|n| crate::rect(&n.draw).y)
            .unwrap();
        let mut ys: Vec<f32> = nodes
            .iter()
            .filter(|n| n.id.ends_with(".row"))
            .map(|n| crate::rect(&n.draw).y - top)
            .filter(|y| *y > -32. && *y < 100.)
            .collect();
        ys.sort_by(f32::total_cmp);
        ys
    };
    assert_eq!(rows(LIST_FIXED, 0.), [0., 32., 64., 96.]);
    assert_eq!(rows(LIST_FIXED, 500.), [-20., 12., 44., 76.]);
}

// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::tests::Fixture;
fn fixture() -> Fixture {
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("ui/components")).unwrap();
    fs::write(f.0.join("ui/behavior.luau"),r#"return {
 stores={ query="", scroll=0, selected="b", rows={{id="a",name="Alpha"},{id="b",name="Beta"},{id="c",name="Gamma"}} },
 actions={select=function(s,id) s.selected=id end}, computed={}
}"#).unwrap();
    fs::write(f.0.join("ui/components/card.luau"),r#"local ui=require("bead.ui")
return function(p) return ui.Button {id="tile",text=p.item.name,on_activate=ui.action("select",p.item.id)} end"#).unwrap();
    fs::write(f.0.join("ui/main.luau"),r#"local ui=require("bead.ui")
local Card=require("./components/card")
return ui.Panel {id="panel",width=400,height=240,padding=12,
 ui.Column {gap=8,
  ui.TextInput{id="search",text=ui.bind("query")},
  ui.Row {grow=1,gap=12,
   ui.Column {width="25%",ui.Button{id="nav",text="Home",on_activate=ui.action("select","a")}},
   ui.Scroll{id="scroll",grow=1,scroll=ui.bind("scroll"),
    ui.Grid{columns=2,gap=8,row_height=48,
     ui.Repeat{id="items",model=ui.bind("rows"),key="id",render=function(item)return Card{item=item}end}
    }
   }
  }
 }
}"#).unwrap();
    f
}

#[test]
fn polyline_parser_rejects_out_of_range_and_oversized_points() {
    let invalid = [
        "{{-0.1,0.5},{1,1}}".to_string(),
        format!(
            "{{{}}}",
            (0..257).map(|_| "{0.5,0.5}").collect::<Vec<_>>().join(",")
        ),
    ];
    for points in invalid {
        let f = fixture();
        fs::write(f.0.join("ui/main.luau"), format!("local ui=require('bead.ui')\nreturn ui.Polyline{{id='chart',width=100,height=40,points={points}}}" )).unwrap();
        let mut session = f.session();
        session.entry = "ui/main.luau".into();
        assert!(session.reload().is_err());
    }
    for points in ["{}", "{{0.5,0.5}}"] {
        let f = fixture();
        fs::write(f.0.join("ui/main.luau"), format!("local ui=require('bead.ui')\nreturn ui.Polyline{{id='chart',width=100,height=40,points={points}}}")).unwrap();
        let mut session = f.session();
        session.entry = "ui/main.luau".into();
        assert!(session.reload().is_ok());
    }
}
fn session(f: &Fixture) -> Session {
    let mut session = f.session();
    session.entry = "ui/main.luau".into();
    let scene = session.reload().unwrap();
    session.check_sources().unwrap();
    session.commit(scene);
    session
}

#[test]
fn normalized_clip_polygon_masks_image_and_button_hit_region() {
    let f = fixture();
    fs::create_dir_all(f.0.join("ui/assets")).unwrap();
    fs::write(f.0.join("ui/assets/hex.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#fff"/></svg>"##).unwrap();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
local hex={{0.25,0},{0.75,0},{1,0.5},{0.75,1},{0.25,1},{0,0.5}}
return ui.Panel{id="panel",width=120,height=120,ui.Button{id="hex",width=100,height=100,clip_polygon=hex,on_activate=ui.action("select","hex"),
 ui.Image{id="image",path="ui/assets/hex.svg",width=100,height=100,image_fit="cover",clip_polygon=hex}}}"#).unwrap();
    let s = session(&f);
    let node = s
        .current_nodes
        .iter()
        .find(|n| n.id.ends_with("hex"))
        .unwrap();
    assert!(node.contains(50., 50.));
    assert!(!node.contains(1., 1.));
    assert!(s.current.primitives.iter().any(|p| matches!(p,Primitive::Image{fit:bead_render::ImageFit::Cover,clip_polygon:Some(points),..} if points.len()==6)));
}

fn deferred_fixture() -> (Fixture, Session) {
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("ui")).unwrap();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"return {
 stores={count=0,last=""},
 actions={select=function(s,id) s.count=s.count+1;s.last=id end}, computed={}
}"#,
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=240,height=120,
 ui.Button{id="deferred",width=120,height=40,text="Deferred",on_activate=ui.action("select","deferred"),activate_on_release=true,
  ui.Box{id="switch-visual",width=100,height=24,ui.Box{id="switch-track",width=28,height=16},ui.Text{id="switch-label",text="DND"}}},
 ui.Button{id="normal",width=120,height=40,text="Normal",on_activate=ui.action("select","normal")}}
"#).unwrap();
    let s = session(&f);
    (f, s)
}
fn node_center(s: &Session, id: &str) -> (f32, f32) {
    let node = s
        .current_nodes
        .iter()
        .find(|node| node.id.ends_with(id))
        .unwrap();
    let Primitive::Rounded { rect, .. } = &node.draw else {
        panic!("button node geometry")
    };
    (rect.x + rect.w / 2., rect.y + rect.h / 2.)
}

#[test]
fn measured_labels_center_the_shaped_advance_and_react_to_text_changes() {
    let f = fixture();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
local text=ui.resolve_for_check(ui.bind("query"))
return ui.Row{id="row",width=400,height=40,justify="center",
 ui.Text{id="label",text=text,width=ui.text_width(text,"DejaVu Sans",16),height=24,font_size=16,family="DejaVu Sans"}}"#).unwrap();
    let mut s = session(&f);
    let mut widths = Vec::new();
    for text in ["WWW", "iii", "Café 世界"] {
        let scene = s
            .update(BTreeMap::from([("query".into(), Value::Text(text.into()))]))
            .unwrap()
            .unwrap();
        let rect = scene
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Text {
                    rect, text: drawn, ..
                } if drawn == text => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!((rect.x + rect.w / 2. - 200.).abs() < 0.1);
        assert!(rect.w > 0.);
        widths.push(rect.w);
        s.commit(scene);
    }
    assert!(
        widths[0] > widths[1] * 2.,
        "equal byte counts have different advances"
    );
}

#[test]
fn text_measurement_uses_each_sessions_own_font_picker() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let make_session = |calls: Arc<AtomicUsize>| {
        let f = fixture();
        fs::write(
            f.0.join("ui/main.luau"),
            "local ui=require('bead.ui')\nreturn ui.Panel{id='root',width=ui.text_width('A','IBM Plex Sans',16),height=40}",
        )
        .unwrap();
        let mut session = f.session();
        session.entry = "ui/main.luau".into();
        session.set_font_picker_if_missing(Arc::new(move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        }));
        let scene = session.reload().unwrap();
        session.commit(scene);
        (f, session)
    };

    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let (_first_fixture, mut first) = make_session(first_calls.clone());
    let (_second_fixture, _second) = make_session(second_calls.clone());
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);

    let replacement_calls = Arc::new(AtomicUsize::new(0));
    let replacement_counter = replacement_calls.clone();
    first.set_font_picker_if_missing(Arc::new(move |_| {
        replacement_counter.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }));
    let reloaded = first.reload().unwrap();
    first.commit(reloaded);
    assert_eq!(first_calls.load(Ordering::SeqCst), 2);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    assert_eq!(replacement_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn navigation_loads_the_new_schema_atomically_and_keeps_the_last_good_entry() {
    let f = fixture();
    fs::create_dir_all(f.0.join("other")).unwrap();
    fs::write(
        f.0.join("other/behavior.luau"),
        "return {stores={query='',counter=7},actions={},computed={}}",
    )
    .unwrap();
    fs::write(f.0.join("other/main.luau"), r#"local ui=require("bead.ui")
return ui.Box{id="other",width=400,height=100,
 ui.Text{id="label",text=ui.bind("query"),width=200,height=24},
 ui.Text{id="counter",text=tostring(ui.resolve_for_check(ui.bind("counter"))),width=100,height=24}}"#).unwrap();
    let mut s = session(&f);
    s.enable_recovery(f.0.join("recovery/current.json"));
    let before = s.stores.get("query").cloned();
    let next = s
        .navigate(
            "other/main.luau".into(),
            BTreeMap::from([("query".into(), Value::Text("kept".into()))]),
        )
        .unwrap();
    assert_eq!(s.entry, Path::new("ui/main.luau"));
    assert_eq!(s.stores.get("query").cloned(), before);
    s.check_sources().unwrap();
    s.commit(next);
    assert_eq!(s.entry, Path::new("other/main.luau"));
    assert_eq!(s.stores.get("query"), Some(&Value::Text("kept".into())));
    assert_eq!(s.stores.get("counter"), Some(&Value::Number(7.)));
    fs::write(f.0.join("other/bad.luau"), "invalid syntax !").unwrap();
    assert!(
        s.navigate("other/bad.luau".into(), BTreeMap::new())
            .is_err()
    );
    assert_eq!(s.entry, Path::new("other/main.luau"));
    assert_eq!(s.stores.get("query"), Some(&Value::Text("kept".into())));
    let scene = s
        .update(BTreeMap::from([(
            "query".into(),
            Value::Text("still live".into()),
        )]))
        .unwrap()
        .unwrap();
    assert!(scene.primitives.iter().any(|p| matches!(p,
        Primitive::Text { text, .. } if text == "still live")));
    s.commit(scene);
}

#[test]
fn clickable_region_can_opt_into_edge_hover() {
    let f = fixture();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
return ui.Button{id="target",text="Target",width=100,height=40,edge_hover=true,on_activate=ui.action("select","a")}"#).unwrap();
    let s = session(&f);
    assert_eq!(s.pointer_cursor(20., 20.), "pointer");
    assert!(s.allows_edge_hover(20., 20.));
    assert!(
        s.action_at(20., 20.).is_some(),
        "click and wheel target remains available"
    );
}

#[test]
fn an_update_in_the_same_event_keeps_the_staged_hover() {
    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "deferred");
    // Hover is staged but not yet committed when a service value arrives in the same event.
    assert!(s.hover(x, y).unwrap().is_some());
    let scene = s
        .update(BTreeMap::from([("count".into(), Value::Number(7.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text("panel.deferred".into()))
    );
    assert_eq!(s.stores.get("count"), Some(&Value::Number(7.)));
    // An unchanged value adds nothing to paint: the staged hover scene stands.
    assert!(s.hover(x, y).unwrap().is_none());
}

#[test]
fn release_activated_button_defers_luau_focus_and_host_action_until_exactly_once() {
    let (_f, mut s) = deferred_fixture();
    if let Some(scene) = s.focus_id("panel.normal").unwrap() {
        s.commit(scene);
    }
    let focused = s.stores.get("bead.focused").cloned();
    let (x, y) = node_center(&s, "deferred");
    assert_eq!(s.press_action_at(x, y), None);
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text("panel.deferred".into())),
        "a deferred press records its hover target even without a prior hover commit"
    );
    assert_eq!(s.stores.get("count"), Some(&Value::Number(0.)));
    assert_eq!(s.stores.get("bead.focused"), focused.as_ref());
    assert_eq!(s.stores.get("bead.pressed"), Some(&Value::Bool(true)));
    assert_eq!(
        s.release_action(),
        Some((
            "panel.deferred".into(),
            "select".into(),
            Value::Text("deferred".into())
        ))
    );
    if let Some(scene) = s.release().unwrap() {
        s.commit(scene);
    }
    assert_eq!(s.stores.get("count"), Some(&Value::Number(1.)));
    assert_eq!(s.stores.get("last"), Some(&Value::Text("deferred".into())));
    assert_eq!(
        s.stores.get("bead.focused"),
        Some(&Value::Text("panel.deferred".into()))
    );
    assert_eq!(s.stores.get("bead.pressed"), Some(&Value::Bool(false)));
    assert_eq!(s.release_action(), None);
    assert!(s.release().unwrap().is_none());
    assert_eq!(s.stores.get("count"), Some(&Value::Number(1.)));
}

#[test]
fn release_activation_cancels_when_pointer_leaves_or_model_removes_target() {
    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "deferred");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s.drag(230., 110.).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.release_action(), None);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text(String::new()))
    );
    let (normal_x, normal_y) = node_center(&s, "normal");
    let scene = s.drag(normal_x, normal_y).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text("panel.normal".into()))
    );
    assert_eq!(
        s.release_action(),
        None,
        "re-entering another button does not revive activation"
    );
    let scene = s.hover(-1., -1.).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text(String::new()))
    );
    assert!(
        s.release().unwrap().is_none(),
        "the canceled gesture has nothing to release"
    );
    assert_eq!(s.stores.get("count"), Some(&Value::Number(0.)));

    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "deferred");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s.hover(-1., -1.).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.release_action(), None, "leave outside cancels the press");
    assert_eq!(s.stores.get("bead.pressed"), Some(&Value::Bool(false)));

    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "deferred");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s.reload().unwrap();
    s.commit(scene);
    assert_eq!(
        s.release_action(),
        None,
        "reload cancels the old generation's pending action"
    );

    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "deferred");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s
        .update(BTreeMap::from([(
            "bead.pressed".into(),
            Value::Bool(false),
        )]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.release_action(),
        None,
        "a pending action requires a live pressed state"
    );
}

#[test]
fn model_argument_change_or_disable_cancels_pending_release() {
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("ui")).unwrap();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"return {
 stores={count=0,rows={{id="one",enabled=true,arg="first"}}},
 actions={select=function(s,id) s.count=s.count+1;s.selected=id end}, computed={}
}"#,
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=240,height=120,ui.Repeat{id="items",model=ui.bind("rows"),key="id",render=function(item)
 return ui.Button{id="tile",width=120,height=40,text="Tile",on_activate=item.enabled and ui.action("select",item.arg) or nil,activate_on_release=true}
end}}
"#).unwrap();
    let mut s = session(&f);
    let (x, y) = node_center(&s, "tile");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s
        .update_model(BTreeMap::from([(
            "rows".into(),
            Value::list(vec![
                serde_json::json!({"id":"one","enabled":true,"arg":"second"}),
            ]),
        )]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.release_action(),
        None,
        "changed action arguments cancel the original press"
    );
    let (x, y) = node_center(&s, "tile");
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    let scene = s
        .update_model(BTreeMap::from([(
            "rows".into(),
            Value::list(vec![
                serde_json::json!({"id":"one","enabled":false,"arg":"second"}),
            ]),
        )]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.release_action(),
        None,
        "disabled model row cancels the pending press"
    );
    assert_eq!(s.stores.get("count"), Some(&Value::Number(0.)));
}

#[test]
fn default_button_press_and_keyboard_enter_remain_immediate() {
    let (_f, mut s) = deferred_fixture();
    let (x, y) = node_center(&s, "normal");
    assert_eq!(
        s.press_action_at(x, y),
        Some((
            "panel.normal".into(),
            "select".into(),
            Value::Text("normal".into())
        ))
    );
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("count"), Some(&Value::Number(1.)));
    assert_eq!(s.stores.get("last"), Some(&Value::Text("normal".into())));

    let (_f, mut s) = deferred_fixture();
    if let Some(scene) = s.focus_id("panel.deferred").unwrap() {
        s.commit(scene);
    }
    let scene = s.activate_focused().unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("count"), Some(&Value::Number(1.)));
    assert_eq!(s.stores.get("last"), Some(&Value::Text("deferred".into())));
}

#[test]
fn focus_on_pointer_false_preserves_focus_but_keeps_keyboard_activation() {
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("ui")).unwrap();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"return {
 stores={count=0,last=""},
 actions={select=function(s,id) s.count=s.count+1;s.last=id end}, computed={}
}"#,
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"), r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=240,height=120,
 ui.Button{id="previous",width=120,height=40,text="Previous",on_activate=ui.action("select","previous")},
 ui.Button{id="quiet-focus",width=120,height=40,text="Quick",on_activate=ui.action("select","quick"),activate_on_release=true,focus_on_pointer=false}}
"#).unwrap();
    let mut s = session(&f);
    if let Some(scene) = s.focus_id("panel.previous").unwrap() {
        s.commit(scene);
    }
    let (x, y) = node_center(&s, "quiet-focus");
    assert_eq!(s.press_action_at(x, y), None);
    let scene = s.click(x, y).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.focused"),
        Some(&Value::Text("panel.previous".into()))
    );
    let scene = s.release().unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("count"), Some(&Value::Number(1.)));
    assert_eq!(
        s.stores.get("bead.focused"),
        Some(&Value::Text("panel.previous".into()))
    );

    if let Some(scene) = s.focus_id("panel.quiet-focus").unwrap() {
        s.commit(scene);
    }
    let scene = s.activate_focused().unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("count"), Some(&Value::Number(2.)));
    assert_eq!(s.stores.get("last"), Some(&Value::Text("quick".into())));
}

#[test]
fn text_input_focus_ring_defaults_on_and_can_be_suppressed() {
    for (property, expected) in [("", true), (",focus_ring=false", false)] {
        let f = fixture();
        fs::write(
            f.0.join("ui/main.luau"),
            format!(
                "local ui=require(\"bead.ui\"); return ui.TextInput{{id=\"search\",text=ui.bind(\"query\"){property}}}"
            ),
        )
        .unwrap();
        let mut s = session(&f);
        let node = s
            .current_nodes
            .iter()
            .find(|node| node.id == "search")
            .unwrap();
        let rect = rect(&node.draw);
        let scene = s.focus_id("search").unwrap().unwrap();
        s.commit(scene);
        let scene = s.key("x", false).unwrap().unwrap();
        let primary = s.palette.get("primary").unwrap_or([200, 200, 200, 255]);
        let has_focus_ring = scene.primitives.iter().any(|primitive| {
            matches!(
                primitive,
                Primitive::Rounded {
                    rect: ring_rect,
                    border: Some((color, width)),
                    ..
                } if *ring_rect == rect && *color == primary && *width == 1.
            )
        });
        assert_eq!(has_focus_ring, expected, "focus_ring property: {property}");
    }
}

#[test]
fn panel_transition_selects_cubic_in_out_without_changing_legacy_curves() {
    use bead_core::animation::Curve;
    for (transition, curve, spring) in [
        ("ease-in-out", Some(Curve::EaseInOut), false),
        ("ease", None, false),
        ("spring", None, true),
    ] {
        let f = fixture();
        fs::write(f.0.join("ui/main.luau"), format!(
            "local ui=require(\"bead.ui\"); return ui.Panel{{id=\"preview\",width=200,height=100,open=true,duration=0.5,transition={transition:?}}}"
        )).unwrap();
        let s = session(&f);
        let motion = s.current.motion.as_ref().unwrap();
        assert_eq!(motion.curve, curve);
        assert_eq!(motion.spring, spring);
        assert_eq!(motion.duration, 0.5);
    }
}
#[test]
fn sdk_brush_helpers_preserve_values_coercion_and_reject_bad_arguments() {
    use crate::frontend::{FrontEnd, LuauFrontEnd, Sources};
    let entry = Path::new("ui/main.luau");
    let build = |expression: &str| {
        let sources = Sources::from([(
            entry.to_path_buf(),
            format!(
                "local ui=require(\"bead.ui\")\nreturn ui.Box{{id=\"test\",fill={expression}}}"
            )
            .into_bytes(),
        )]);
        LuauFrontEnd::new(entry, &sources)
            .unwrap()
            .build(entry, &|_| {
                Err(Diagnostic::new(entry, 1, 1, "unexpected binding"))
            })
    };
    for (expression, expected) in [
        (
            "ui.alpha(\"surface\", .5)",
            serde_json::json!({"role":"surface","alpha":0.5}),
        ),
        (
            "ui.alpha(42, \".5\")",
            serde_json::json!({"role":"42","alpha":0.5}),
        ),
        (
            "ui.tint(\"surface\", 5, \".25\")",
            serde_json::json!({"base":"surface","over":"5","alpha":0.25}),
        ),
    ] {
        assert_eq!(build(expression).unwrap().props["fill"], expected);
    }
    for expression in [
        "ui.alpha({}, .5)",
        "ui.alpha(\"surface\", {})",
        "ui.tint(\"surface\", false, .5)",
        "ui.tint(\"surface\", \"primary\", false)",
    ] {
        let error = build(expression).unwrap_err();
        assert!(error.file.ends_with("ui/main.luau"));
        assert_eq!(
            error.line, 2,
            "brush errors must identify their caller: {error}"
        );
    }
}
#[test]
fn sdk_bind_action_helpers_keep_markers_shared_arguments_and_nil_false_distinct() {
    use crate::frontend::{FrontEnd, LuauFrontEnd, Sources};
    let entry = Path::new("ui/main.luau");
    let build = |body: &str| {
        let sources = Sources::from([(
            entry.to_path_buf(),
            format!("local ui=require(\"bead.ui\")\n{body}").into_bytes(),
        )]);
        LuauFrontEnd::new(entry, &sources)
            .unwrap()
            .build(entry, &|_| {
                Err(Diagnostic::new(entry, 1, 1, "unexpected binding"))
            })
    };
    let raw=build("local argument={flag=false,values={1,'two',true}};local action=ui.action(17,argument);argument.flag=true;return ui.Box{id='test',text=ui.bind(42),on_activate=action}").unwrap();
    assert_eq!(raw.props["text"], serde_json::json!({"__bind":"42"}));
    assert_eq!(
        raw.props["on_activate"],
        serde_json::json!({"name":"17","argument":{"flag":true,"values":[1,"two",true]}})
    );
    for (expression, expected) in [
        ("ui.action('toggle')", serde_json::json!({"name":"toggle"})),
        (
            "ui.action('toggle',false)",
            serde_json::json!({"name":"toggle","argument":false}),
        ),
    ] {
        assert_eq!(
            build(&format!(
                "return ui.Box{{id='test',on_activate={expression}}}"
            ))
            .unwrap()
            .props["on_activate"],
            expected
        );
    }
    for (field, expression) in [("text", "ui.bind({})"), ("on_activate", "ui.action(false)")] {
        let error = build(&format!("return ui.Box{{id='test',{field}={expression}}}")).unwrap_err();
        assert!(error.file.ends_with("ui/main.luau"));
        assert_eq!(error.line, 2);
    }
}
#[test]
fn luau_components_imports_taffy_and_relative_sizes() {
    let f = fixture();
    let session = session(&f);
    assert_eq!(session.current.width, 400.);
    let nav = session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(".nav"))
        .unwrap();
    assert!(
        (rect(&nav.draw).w - 94.).abs() <= 1.,
        "sidebar is25% of376 content pixels"
    );
    let cards: Vec<_> = session
        .view_nodes()
        .iter()
        .filter(|n| n.id.ends_with(".tile"))
        .collect();
    assert_eq!(cards.len(), 3);
    assert_eq!(rect(&cards[0].draw).y, rect(&cards[1].draw).y);
    assert!(rect(&cards[2].draw).y > rect(&cards[0].draw).y);
    assert!(!session.current.clips.is_empty());
}
#[test]
fn keyed_models_reject_duplicates_atomically_and_deliver_action_arguments() {
    let f = fixture();
    let mut session = session(&f);
    let card = session
        .view_nodes()
        .iter()
        .find(|n| n.id.contains("k61.tile"))
        .unwrap();
    let r = rect(&card.draw);
    let scene = session.click(r.x + 2., r.y + 2.).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("selected"),
        Some(&Value::Text("a".into()))
    );
    let initial = session.state_summary();
    let changes = BTreeMap::from([(
        "rows".into(),
        Value::list(vec![
            serde_json::json!({"id":"a","name":"A"}),
            serde_json::json!({"id":"a","name":"Again"}),
        ]),
    )]);
    assert!(
        session
            .update(changes)
            .err()
            .unwrap()
            .message
            .contains("duplicate key")
    );
    assert_eq!(session.state_summary(), initial);
}
#[test]
fn imported_error_has_real_file_line_and_old_generation_survives() {
    let f = fixture();
    let mut session = session(&f);
    let initial = session.state_summary();
    fs::write(f.0.join("ui/components/card.luau"),"local ui=require('bead.ui')\nreturn function(p) return ui.Button{id='tile', text=p.item.name, widht=1} end").unwrap();
    let error = session.reload().err().unwrap();
    assert!(error.message.contains("widht"));
    assert!(error.file.contains("card.luau"), "{error}");
    assert_eq!(error.line, 2);
    assert_eq!(session.state_summary(), initial);
    let scene = session.key("still works", false).unwrap();
    assert!(scene.is_none());
}
#[test]
fn import_cycles_and_deleted_modules_keep_previous_ui() {
    let f = fixture();
    let mut session = session(&f);
    fs::write(
        f.0.join("ui/components/card.luau"),
        "return require('./card')",
    )
    .unwrap();
    assert!(session.reload().err().unwrap().message.contains("cycle"));
    fs::remove_file(f.0.join("ui/components/card.luau")).unwrap();
    assert!(session.reload().err().unwrap().message.contains("tracked"));
}

#[test]
fn keyed_reorder_insert_delete_keep_focus_selection_scroll_and_input() {
    let f = fixture();
    let mut session = session(&f);
    let rows: Vec<_> = (0..20)
        .map(|i| serde_json::json!({"id":format!("{i}"),"name":format!("Item{i}")}))
        .collect();
    let scene = session
        .update(BTreeMap::from([("rows".into(), Value::list(rows.clone()))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let card = session
        .view_nodes()
        .iter()
        .find(|node| node.id.contains("k30.tile"))
        .unwrap();
    let r = rect(&card.draw);
    let scene = session.click(r.x + 2., r.y + 2.).unwrap().unwrap();
    session.commit(scene);
    let focused = session.stores.get("bead.focused").cloned();
    let scene = session
        .update(BTreeMap::from([
            ("query".into(), Value::Text("saved😀".into())),
            ("scroll".into(), Value::Number(18.)),
        ]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let mut moved = rows;
    moved.swap(0, 3);
    moved.insert(1, serde_json::json!({"id":"new","name":"New"}));
    moved.retain(|row| row["id"] != "5");
    let scene = session
        .update(BTreeMap::from([("rows".into(), Value::list(moved))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    assert_eq!(session.stores.get("bead.focused"), focused.as_ref());
    assert_eq!(
        session.stores.get("selected"),
        Some(&Value::Text("0".into()))
    );
    assert_eq!(session.stores.get("scroll"), Some(&Value::Number(18.)));
    assert_eq!(
        session.stores.get("query"),
        Some(&Value::Text("saved😀".into()))
    );
    assert!(
        session
            .view_nodes()
            .iter()
            .any(|node| Some(&Value::Text(node.id.clone())) == focused.as_ref())
    );
    let scene = session.reload().unwrap();
    session.commit(scene);
    assert_eq!(session.stores.get("bead.focused"), focused.as_ref());
}
#[test]
fn runtime_component_properties_reject_unknown_and_wrong_types() {
    let f = fixture();
    let mut session = session(&f);
    fs::write(f.0.join("ui/components/card.luau"),r#"local ui=require('bead.ui')
local Card=ui.component('Card',{item={id='string',name='string'}},function(p)return ui.Button{id='tile',text=p.item.name,on_activate=ui.action('select',p.item.id)}end)
return function(p)return Card{item=p.item,typo=true}end"#).unwrap();
    assert!(
        session
            .reload()
            .err()
            .unwrap()
            .message
            .contains("unknown property typo")
    );
    fs::write(f.0.join("ui/components/card.luau"),r#"local ui=require('bead.ui')
local Card=ui.component('Card',{item={id='string',name='string'}},function(p)return ui.Button{id='tile',text=p.item.name,on_activate=ui.action('select',p.item.id)}end)
return function(p)return Card{item={id=p.item.id,name=42}}end"#).unwrap();
    assert!(
        session
            .reload()
            .err()
            .unwrap()
            .message
            .contains("Card.item.name must be string")
    );
    fs::write(f.0.join("ui/components/card.luau"),r#"local ui=require('bead.ui')
local Card=ui.component('Card',{item={id='string',name='string'}},function(p)return ui.Button{id='tile',text=p.item.name,on_activate=ui.action('select',p.item.id)}end)
return function(p)return Card{item={id=p.item.id,name=ui.bind('query')}}end"#).unwrap();
    let scene = session.reload().unwrap();
    session.commit(scene);
    fs::write(f.0.join("ui/components/card.luau"),r#"local ui=require('bead.ui')
local Card=ui.component('Card',{item={id='string',name='string'}},function(p)return ui.Button{id='tile',text=p.item.name,on_activate=ui.action('select',p.item.id)}end)
return function(p)return Card{item={id=p.item.id,name=ui.bind('scroll')}}end"#).unwrap();
    assert!(
        session
            .reload()
            .err()
            .unwrap()
            .message
            .contains("Card.item.name must be string")
    );
}
#[test]
fn keyboard_selection_preedit_commits_and_accessibility_survive_reload() {
    let f = fixture();
    let mut session = session(&f);
    let search = session
        .view_nodes()
        .iter()
        .find(|n| n.input.is_some())
        .unwrap()
        .id
        .clone();
    let scene = session.focus_id(&search).unwrap().unwrap();
    session.commit(scene);
    let scene = session.key("A👩‍🚀é", false).unwrap().unwrap();
    session.commit(scene);
    let scene = session
        .edit_key(input::EditKey::Left, false)
        .unwrap()
        .unwrap();
    session.commit(scene);
    let scene = session.key("", true).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(session.stores.get("query"), Some(&Value::Text("Aé".into())));
    let scene = session.ime("に".into(), None, 0, 0).unwrap().unwrap();
    session.commit(scene);
    let scene = session.reload().unwrap();
    session.commit(scene);
    assert_eq!(session.editors[&search].preedit, "に");
    let scene = session
        .ime("".into(), Some("日本語".into()), 0, 0)
        .unwrap()
        .unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("query"),
        Some(&Value::Text("A日本語é".into()))
    );
    let scene = session
        .edit_key(input::EditKey::SelectAll, false)
        .unwrap()
        .unwrap();
    session.commit(scene);
    let scene = session.key("replaced", false).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("query"),
        Some(&Value::Text("replaced".into()))
    );
    let tree = session.accessibility_tree();
    assert_eq!(tree.focus, accessibility::id(&search));
    let editable = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == accessibility::id(&search))
        .unwrap();
    assert_eq!(editable.1.role(), accesskit::Role::TextInput);
    assert_eq!(editable.1.value(), Some("replaced"));
    let scene = session
        .accessibility_action(
            accessibility::id(&search).0,
            "set-value",
            Some("screen reader".into()),
        )
        .unwrap()
        .unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("query"),
        Some(&Value::Text("screen reader".into()))
    );
}
#[test]
fn removing_focused_key_picks_a_visible_fallback_without_changing_search_scroll() {
    let f = fixture();
    let mut session = session(&f);
    let node = session
        .view_nodes()
        .iter()
        .find(|n| n.id.contains("k61.tile"))
        .unwrap()
        .id
        .clone();
    let scene = session.focus_id(&node).unwrap().unwrap();
    session.commit(scene);
    let scene = session
        .update(BTreeMap::from([(
            "rows".into(),
            Value::list(vec![serde_json::json!({"id":"b","name":"B"})]),
        )]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let Some(Value::Text(focus)) = session.stores.get("bead.focused") else {
        panic!("no focus");
    };
    assert_ne!(focus, &node);
    assert!(session.view_nodes().iter().any(|n| &n.id == focus));
}

#[test]
fn overflowing_scroll_grid_keeps_rows_visible_beyond_its_assigned_container_box() {
    let f = fixture();
    let mut session = session(&f);
    let rows = (0..60)
        .map(|i| serde_json::json!({"id":format!("row-{i}"),"name":format!("Item {i}")}))
        .collect();
    let scene = session
        .update(BTreeMap::from([("rows".into(), Value::list(rows))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let scroll = session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(".scroll"))
        .unwrap();
    let r = rect(&scroll.draw);
    let scene = session
        .scroll(r.x + r.w / 2., r.y + r.h / 2., 350.)
        .unwrap()
        .unwrap();
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text.starts_with("Item ")))
    );
}

#[test]
fn framework_kit_shared_scale_edges_and_independent_tile_actions() {
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("kit")).unwrap();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(
        f.0.join("sample.png"),
        include_bytes!("../../../shell/sample.png"),
    )
    .unwrap();
    fs::write(
        f.0.join("kit/behavior.luau"),
        include_str!("../../../shell/kit/behavior.luau"),
    )
    .unwrap();
    let source = include_str!("../../../shell/kit/main.luau");
    for edge in ["left", "right"] {
        fs::write(
            f.0.join("kit/main.luau"),
            source.replace("edge=\"left\"", &format!("edge=\"{edge}\"")),
        )
        .unwrap();
        let mut session = f.session();
        session.entry = "kit/main.luau".into();
        let candidate = session.reload().unwrap();
        assert_eq!(
            candidate.motion.as_ref().unwrap().edge,
            if edge == "left" {
                bead_core::Edge::Left
            } else {
                bead_core::Edge::Right
            }
        );
        session.commit(candidate);
        for (suffix, store, expected) in [
            ("tile.toggle", "enabled", Value::Bool(true)),
            ("tile.detail", "details", Value::Number(1.)),
            ("slider.hits.value-10", "volume", Value::Number(50.)),
            ("segments.normal", "choice", Value::Text("normal".into())),
        ] {
            let node = session
                .view_nodes()
                .iter()
                .find(|n| n.id.ends_with(suffix))
                .unwrap();
            let r = rect(&node.draw);
            let scene = session
                .click(r.x + r.w / 2., r.y + r.h / 2.)
                .unwrap()
                .unwrap();
            session.commit(scene);
            assert_eq!(session.stores.get(store), Some(&expected));
        }
        let r = rect(
            &session
                .view_nodes()
                .iter()
                .find(|n| n.id.ends_with("slider.hits.value-10"))
                .unwrap()
                .draw,
        );
        let scene = session
            .click(r.x + r.w / 2., r.y + r.h / 2.)
            .unwrap()
            .unwrap();
        session.commit(scene);
        let r = rect(
            &session
                .view_nodes()
                .iter()
                .find(|n| n.id.ends_with("slider.hits.value-15"))
                .unwrap()
                .draw,
        );
        let scene = session
            .drag(r.x + r.w / 2., r.y + r.h / 2.)
            .unwrap()
            .unwrap();
        session.commit(scene);
        assert_eq!(
            session.stores.get("volume"),
            Some(&Value::Number(75.5)),
            "the hit-cell centre maps to an exact pointer value, not its 75% keyboard step"
        );
        assert_eq!(session.range_action(true).unwrap().2, Value::Number(70.));
        assert_eq!(
            session
                .drag_action(r.x + r.w / 2., r.y + r.h / 2.)
                .unwrap()
                .2,
            Value::Number(75.)
        );
        assert_eq!(
            session.drag_action(-1., -1.).unwrap().2,
            Value::Number(0.),
            "a held range clamps outside its row"
        );
        let scene = session.step_range(true).unwrap().unwrap();
        session.commit(scene);
        assert_eq!(session.stores.get("volume"), Some(&Value::Number(70.)));
        assert!(
            session
                .current
                .primitives
                .iter()
                .any(|p| matches!(p,Primitive::Image{blur,..} if *blur == 6.))
        );
    }
    let scale: serde_json::Value =
        serde_json::from_str(include_str!("../assets/size-scale.json")).unwrap();
    assert_eq!(scale["spacing"]["md"], 12);
    assert_eq!(scale["radii"]["medium"], 16);
}
#[test]
fn edge_motion_opens_inward_and_top_bar_stays_horizontal() {
    assert!(bead_core::Edge::Left.closed_offset(100., 200., 0.).0 < 0.);
    assert!(bead_core::Edge::Right.closed_offset(100., 200., 0.).0 > 0.);
    assert_eq!(
        bead_core::Edge::Left.closed_offset(100., 200., 1.),
        (0., 0.)
    );
    let f = fixture();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"),r#"local ui=require("bead.ui"); local kit=require("bead.kit")
return kit.Bar{id="bar",edge="top",width=400,height=50,kit.Button{id="a",text="A",on_activate=ui.action("select","a")},kit.Button{id="b",text="B",on_activate=ui.action("select","b")}}"#).unwrap();
    let session = session(&f);
    let nodes = session.view_nodes();
    let a = rect(&nodes.iter().find(|n| n.id.ends_with(".a")).unwrap().draw);
    let b = rect(&nodes.iter().find(|n| n.id.ends_with(".b")).unwrap().draw);
    assert!(b.x > a.x);
    assert_eq!(a.y, b.y);
}

#[test]
fn restored_panel_state_is_staged_and_commits_only_after_validation() {
    let f = fixture();
    let fallback = f.session().current.clone();
    let mut session = Session::new(
        f.0.clone(),
        "ui/main.luau".into(),
        f.0.join("palette.json"),
        Palette::read(&f.0.join("palette.json")).unwrap(),
        fallback,
    );
    session.restore_on_start(BTreeMap::from([
        ("selected".into(), Value::Text("c".into())),
        ("obsolete.open".into(), Value::Bool(false)),
    ]));
    let scene = session.reload().unwrap();
    assert!(session.stores.get("selected").is_none());
    session.check_sources().unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("selected"),
        Some(&Value::Text("c".into()))
    );
    assert!(session.stores.get("obsolete.open").is_none());
}

#[test]
fn compiled_markup_keeps_evaluations_fresh_and_replaces_changed_imports() {
    let f = fixture();
    let path = f.0.join("ui/components/card.luau");
    let source = r#"local ui=require("bead.ui")
local count=0
return function(p) count+=1; return ui.Button{id="tile",text=p.item.name..count} end"#;
    fs::write(&path, source).unwrap();
    let mut session = session(&f);
    let labels = |s: &Session| {
        s.current
            .primitives
            .iter()
            .filter_map(|p| {
                if let Primitive::Text { text, .. } = p {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .filter(|s| s.starts_with("Alpha") || s.starts_with("Beta") || s.starts_with("Gamma"))
            .collect::<Vec<_>>()
    };
    let before = labels(&session);
    assert!(before.iter().any(|s| s == "Alpha1"));
    let scene = session
        .update(BTreeMap::from([(
            "query".into(),
            Value::Text("typed".into()),
        )]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    assert_eq!(
        labels(&session),
        before,
        "no Lua module state may survive an evaluation"
    );
    fs::write(
        &path,
        source.replace("p.item.name..count", "p.item.name..'-new'..count"),
    )
    .unwrap();
    let scene = session.reload().unwrap();
    session.check_sources().unwrap();
    session.commit(scene);
    assert!(labels(&session).iter().any(|s| s == "Alpha-new1"));
    assert_eq!(
        session.stores.get("query"),
        Some(&Value::Text("typed".into()))
    );
}

#[test]
fn arc_constructor_builds_thematic_signed_stroke_and_rejects_out_of_range_angles() {
    let f = fixture();
    fs::write(
        f.0.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"101010","secondary":"4499cc"}}"#,
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Box{id="root",width=120,height=120,
 ui.Arc{id="track",position="absolute",left=4,top=4,width=112,height=112,
   start_angle=-90,sweep_angle=-180,stroke_width=5,color="secondary",round_caps=true}}"#,
    )
    .unwrap();
    let mut session = session(&f);
    let arc = session
        .current
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Arc {
                rect,
                start_angle,
                sweep_angle,
                stroke_width,
                color,
                round_caps,
            } => Some((
                *rect,
                *start_angle,
                *sweep_angle,
                *stroke_width,
                *color,
                *round_caps,
            )),
            _ => None,
        })
        .expect("Arc constructor emits a render primitive");
    assert_eq!(arc.0, Rect::new(4., 4., 112., 112.));
    assert_eq!((arc.1, arc.2, arc.3), (-90., -180., 5.));
    assert_eq!(
        arc.4,
        Palette::read(&f.0.join("palette.json"))
            .unwrap()
            .get("secondary")
            .unwrap()
    );
    assert!(arc.5);

    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Arc{id="invalid",width=20,height=20,sweep_angle=361}"#,
    )
    .unwrap();
    assert!(
        session
            .reload()
            .err()
            .expect("out of range Arc angles are rejected")
            .message
            .contains("Arc angles")
    );
}

#[test]
fn closed_error_overlay_replaces_retained_body_without_stale_layer_ranges() {
    for prepared in [false, true] {
        let f = fixture();
        let mut session = session(&f);
        assert!(!session.current.layers.is_empty());
        session.current.motion = Some(bead_render::Motion {
            id: "panel".into(),
            open: false,
            duration: 0.2,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Right,
            slide_distance: None,
            fade: true,
            reveal: false,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });
        let mut cache = bead_render::tree::TreeCache::default();
        if prepared {
            cache.prepare(&session.current, 1.).unwrap();
            session
                .current
                .prepared
                .insert(1_f32.to_bits(), cache.frame.clone().unwrap());
        }
        let count = session.current.primitives.len();
        let state = session.state_summary();
        let overlay = session.reject("reload deadline".into());
        cache.prepare(&overlay, 1.).unwrap();
        assert_eq!(
            cache.frame.as_ref().unwrap().items.len(),
            2,
            "only error background and text remain"
        );
        assert_eq!(
            session.current.primitives.len(),
            count,
            "last good body is retained separately"
        );
        assert_eq!(session.state_summary(), state);
    }
}

#[test]
fn unread_host_model_stays_current_and_becomes_live_when_a_view_binds_it() {
    let f = fixture();
    let behavior = f.0.join("ui/behavior.luau");
    fs::write(
        &behavior,
        fs::read_to_string(&behavior)
            .unwrap()
            .replace("stores={ query=", "stores={ background=\"old\", query="),
    )
    .unwrap();
    let mut session = session(&f);
    assert!(
        session
            .update_model(BTreeMap::from([(
                "background".into(),
                Value::Text("latest".into())
            )]))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        session.stores.get("background"),
        Some(&Value::Text("latest".into()))
    );
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=400,height=240,ui.Text{id="host",text=ui.bind("background")}}"#,
    )
    .unwrap();
    let scene = session.reload().unwrap();
    session.check_sources().unwrap();
    session.commit(scene);
    assert!(
        session
            .current
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text=="latest"))
    );
    let scene = session
        .update_model(BTreeMap::from([(
            "background".into(),
            Value::Text("newer".into()),
        )]))
        .unwrap()
        .unwrap();
    assert_eq!(
        session.stores.get("background"),
        Some(&Value::Text("latest".into())),
        "visible model change must wait for commit"
    );
    session.commit(scene);
    assert!(
        session
            .current
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text=="newer"))
    );
}
#[test]
fn indirect_model_reads_survive_rejected_visibility_branches() {
    let f = fixture();
    let behavior = f.0.join("ui/behavior.luau");
    let source=fs::read_to_string(&behavior).unwrap().replace("stores={ query=", "stores={ background=\"old\", query=").replace("computed={}",r#"computed={shown=function(s) if s.query=="" then return s.background end return "hidden" end}"#);
    fs::write(&behavior, source).unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=400,height=240,ui.Text{id="host",text=ui.bind("shown")}}"#,
    )
    .unwrap();
    let mut session = session(&f);
    let scene = session
        .update_model(BTreeMap::from([(
            "background".into(),
            Value::Text("new".into()),
        )]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    session
        .update(BTreeMap::from([(
            "query".into(),
            Value::Text("hide".into()),
        )]))
        .unwrap()
        .unwrap();
    session.reject("candidate rejected".into());
    let scene = session
        .update_model(BTreeMap::from([(
            "background".into(),
            Value::Text("last".into()),
        )]))
        .unwrap()
        .unwrap();
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..} if text=="last")),
        "committed dependency must survive rejected candidate"
    );
}

#[test]
fn model_count_tracks_empty_growing_and_replaced_store_models() {
    let f = fixture();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=400,height=240,
    ui.Text{id="count",text=tostring(ui.model_count(ui.bind("rows")))}}"#,
    )
    .unwrap();
    let mut s = session(&f);
    for count in [320, 1, 0, 3] {
        let rows = (0..count)
            .map(|i| serde_json::json!({"id":i.to_string(),"name":"Fixture"}))
            .collect();
        let next = s
            .update(BTreeMap::from([("rows".into(), Value::list(rows))]))
            .unwrap()
            .expect("a count-only binding must track its model dependency");
        assert!(next.primitives.iter().any(|p| matches!(p,
            bead_render::Primitive::Text{text,..} if text == &count.to_string())));
        s.commit(next);
    }
    let before = s.state_summary();
    assert!(
        s.update(BTreeMap::from([(
            "rows".into(),
            Value::Text("not a model".into())
        )]))
        .is_err()
    );
    assert_eq!(
        s.state_summary(),
        before,
        "a failed count projection must keep the committed view"
    );
}

#[test]
fn windowed_repeat_keeps_stable_ids_extent_and_validates_offscreen_keys() {
    let f = fixture();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"local rows={};for i=1,320 do rows[i]={id=tostring(i),name="Application "..i} end
return {stores={scroll=0,rows=rows},actions={select=function()end},computed={}}"#,
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"),r#"local ui=require("bead.ui");local kit=require("bead.kit")
return ui.Panel{id="panel",width=400,height=240,kit.VirtualGrid{id="grid",width=400,height=240,min_cell_width=100,gap_x=0,gap_y=0,row_height=48,scroll=ui.bind("scroll"),model=ui.bind("rows"),key="id",render=function(item)return ui.Button{id="tile",text=item.name,on_activate=ui.action("select",item.id)}end}}"#).unwrap();
    let mut s = session(&f);
    assert!(s.view_nodes().len() < 35);
    let first = s
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(".tile"))
        .unwrap()
        .id
        .clone();
    let next = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(1000.))]))
        .unwrap()
        .unwrap();
    s.commit(next);
    assert!(s.view_nodes().len() < 35);
    assert!(!s.view_nodes().iter().any(|n| n.id == first));
    // Invalid keys outside the viewport still reject the candidate.
    let mut rows = if let Some(Value::List(rows)) = s.stores.get("rows") {
        rows.as_ref().clone()
    } else {
        panic!()
    };
    rows[319]["id"] = rows[318]["id"].clone();
    assert!(
        s.update(BTreeMap::from([("rows".into(), Value::list(rows))]))
            .is_err()
    );
}

#[test]
fn grid_list_flip_builds_only_the_window_and_returns_the_same_tiles() {
    let f = fixture();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"local rows={};for i=1,320 do rows[i]={id=tostring(i),name="Application "..i} end
return {stores={mode="grid",scroll=0,rows=rows},actions={select=function()end},computed={}}"#,
    )
    .unwrap();
    fs::write(f.0.join("ui/main.luau"),r#"local ui=require("bead.ui");local kit=require("bead.kit")
local grid=ui.resolve_for_check(ui.bind("mode"))=="grid"
return ui.Panel{id="panel",width=400,height=240,kit.VirtualGrid{id="grid",width=400,height=100,min_cell_width=grid and 100 or 400,gap_x=0,gap_y=0,row_height=40,scroll=ui.bind("scroll"),model=ui.bind("rows"),key="id",render=function(item)return ui.Button{id="tile",text=item.name,on_activate=ui.action("select",item.id)}end}}"#).unwrap();
    let mut s = session(&f);
    let tiles = |s: &Session| {
        s.view_nodes()
            .iter()
            .filter(|n| n.id.ends_with(".tile"))
            .map(|n| n.id.clone())
            .collect::<Vec<_>>()
    };
    let set = |s: &mut Session, name: &str, value: Value| {
        let next = s
            .update(BTreeMap::from([(name.into(), value)]))
            .unwrap()
            .unwrap();
        s.commit(next);
    };
    // The grid claims a 100 px viewport inside a 240 px scroll area, so every built row is
    // on screen and countable. Top of the model: 4 columns x 4 rows (2.5 visible + 1 below).
    let grid = tiles(&s);
    assert_eq!(grid.len(), 4 * 4);
    set(&mut s, "mode", Value::Text("list".into()));
    let list = tiles(&s);
    assert_eq!(list.len(), 4);
    set(&mut s, "mode", Value::Text("grid".into()));
    assert_eq!(
        tiles(&s),
        grid,
        "grid -> list -> grid returns the same tiles"
    );
    // Mid-scroll: the window moves and never exceeds viewport + 2 rows.
    set(&mut s, "scroll", Value::Number(1000.));
    let mid = tiles(&s);
    assert!(!mid.is_empty() && mid.len() <= 4 * 5);
    assert!(mid.iter().all(|id| !grid.contains(id)));
}
#[test]
fn text_patch_updates_pixels_label_and_structural_bindings_still_relayout() {
    let f = fixture();
    let mut s = session(&f);
    let next = s
        .update(BTreeMap::from([(
            "query".into(),
            Value::Text("changed".into()),
        )]))
        .unwrap()
        .unwrap();
    s.commit(next);
    assert!(
        s.current
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..}if text=="changed"))
    );
    assert!(
        s.view_nodes()
            .iter()
            .any(|n| n.input.as_deref() == Some("query") && n.label == "changed")
    );
    let next = s
        .update(BTreeMap::from([("query".into(), Value::Text("".into()))]))
        .unwrap()
        .unwrap();
    s.commit(next);
    assert!(
        !s.current
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..}if text=="changed"))
    );
    let next = s
        .update(BTreeMap::from([(
            "selected".into(),
            Value::Text("a".into()),
        )]))
        .unwrap()
        .unwrap();
    s.commit(next);
    // A structural text/size binding cannot take the text-only shortcut.
    let source = fs::read_to_string(f.0.join("ui/main.luau")).unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        source.replace("width=400", "width=ui.bind(\"scroll\")"),
    )
    .unwrap();
    let next = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(400.))]))
        .unwrap()
        .unwrap();
    s.commit(next);
    let next = s.reload().unwrap();
    s.commit(next);
    let next = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(500.))]))
        .unwrap()
        .unwrap();
    assert_eq!(next.width, 500.);
}

#[test]
fn widget_constructors_preserve_empty_children_and_callsite_diagnostics() {
    use crate::frontend::{FrontEnd, LuauFrontEnd};
    let entry = PathBuf::from("ui/main.luau");
    let sources = BTreeMap::from([(
        entry.clone(),
        br#"local ui=require('bead.ui')
return ui.Panel{id='root',width=120,height=80,
 ui.Text{id='label',text='Hello'},
 ui.Box{id='empty'}
}"#
        .to_vec(),
    )]);
    let frontend = LuauFrontEnd::new(&entry, &sources).unwrap();
    let resolve = |name: &str| Err(Diagnostic::new(&entry, 1, 1, name));
    let raw = frontend.build(&entry, &resolve).unwrap();
    assert_eq!(raw.source.file, "ui/main.luau");
    assert!((2..=5).contains(&raw.source.line));
    assert_eq!(raw.children.len(), 2);
    assert_eq!(raw.children[0].source.line, 3);
    assert_eq!(raw.children[1].source.line, 4);
    assert!(raw.children.iter().all(|node| node.children.is_empty()));
    for (body, message, line) in [
        (
            "local ui=require('bead.ui')\nreturn ui.Box{id='bad',typo=3}",
            "unknown Box property typo",
            2,
        ),
        (
            "local ui=require('bead.ui')\nreturn ui.Box{[false]='bad'}",
            "node properties need names or ordered children",
            2,
        ),
    ] {
        let sources = BTreeMap::from([(entry.clone(), body.as_bytes().to_vec())]);
        let frontend = LuauFrontEnd::new(&entry, &sources).unwrap();
        let error = frontend.build(&entry, &resolve).unwrap_err();
        assert!(error.message.contains(message), "{}", error.message);
        assert_eq!(error.file, entry.display().to_string());
        assert_eq!(error.line, line);
    }
}

#[test]
fn constructors_snapshot_plain_frozen_and_protected_properties_with_sparse_children() {
    use crate::frontend::{FrontEnd, LuauFrontEnd};
    let entry = PathBuf::from("ui/main.luau");
    let sources = BTreeMap::from([(
        entry.clone(),
        br#"local ui=require('bead.ui')
local p={id='first',text='Before'}
local first=ui.Text(p)
p.id='second';p.text='After'
local second=ui.Text(p)
local frozen=table.freeze({id='frozen',text='Frozen'})
local protected=setmetatable({id='protected'},{__metatable=false,__index={text='Ghost',[1]=first}})
return ui.Panel{id='root',width=200,height=120,
 first,second,ui.Text(frozen),ui.Text(protected),
 ui.Box{id='sparse',[1]=ui.Text{id='kept',text='Kept'},[3]=ui.Text{id='ignored',text='Ignored'}}}
"#
        .to_vec(),
    )]);
    let frontend = LuauFrontEnd::new(&entry, &sources).unwrap();
    let raw = frontend
        .build(&entry, &|name| Err(Diagnostic::new(&entry, 1, 1, name)))
        .unwrap();
    assert_eq!(raw.children[0].props["id"], "first");
    assert_eq!(raw.children[0].props["text"], "Before");
    assert_eq!(raw.children[1].props["text"], "After");
    assert_eq!(raw.children[2].props["text"], "Frozen");
    assert!(!raw.children[3].props.contains_key("text"));
    assert!(raw.children[3].children.is_empty());
    assert_eq!(raw.children[4].children.len(), 1);
    assert_eq!(raw.children[4].children[0].props["id"], "kept");
}

static CATALOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn texts(session: &Session) -> Vec<String> {
    session
        .current
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn tr_reaches_markup_and_behaviour_and_a_catalog_change_redraws_on_reload() {
    use crate::i18n::{self, Entries, Message};
    let _lock = CATALOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    i18n::set_builtin(
        r#"{"test.hello":"Hello {name}","test.items":{"one":"{count} item","other":"{count} items"},"test.only-english":"English only"}"#,
    );
    i18n::clear();
    let f = fixture();
    fs::write(
        f.0.join("ui/behavior.luau"),
        r#"return {stores={}, actions={}, computed={["label"]=function(_) return tr("test.items",{count=2}) end}}"#,
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=400,height=240,padding=12,ui.Column{gap=8,
 ui.Text{id="a",text=ui.tr("test.hello",{name="Ada"}),height=20},
 ui.Text{id="b",text=ui.tr("test.items",{count=1}),height=20},
 ui.Text{id="c",text=ui.bind("label"),height=20},
 ui.Text{id="d",text=ui.tr("test.only-english"),height=20},
 ui.Text{id="e",text=ui.tr("test.missing"),height=20}}}"#,
    )
    .unwrap();
    let mut s = session(&f);
    assert_eq!(
        texts(&s),
        [
            "Hello Ada",
            "1 item",
            "2 items",
            "English only",
            "test.missing"
        ]
    );
    // Language switch en -> pseudo: new text for two ids; the third falls back to English.
    let pseudo = Entries::from([
        (
            "test.hello".into(),
            Message::Text("[!! Hello {name} !!]".into()),
        ),
        (
            "test.items".into(),
            Message::Plural {
                one: "[!! {count} item !!]".into(),
                other: "[!! {count} items !!]".into(),
            },
        ),
    ]);
    let changed = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let seen = changed.clone();
    let _subscription = i18n::on_change(move || {
        seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    i18n::install(2, pseudo.clone());
    i18n::install(2, pseudo);
    assert_eq!(changed.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(i18n::revision(), 2);
    let scene = s.reload().unwrap();
    s.commit(scene);
    assert_eq!(
        texts(&s),
        [
            "[!! Hello Ada !!]",
            "[!! 1 item !!]",
            "[!! 2 items !!]",
            "English only",
            "test.missing"
        ]
    );
    i18n::clear();
    assert_eq!(i18n::resident_bytes(), 0);
    let scene = s.reload().unwrap();
    s.commit(scene);
    assert_eq!(texts(&s)[0], "Hello Ada");
}
#[test]
fn multiline_input_breaks_lines_moves_vertically_and_draws_a_wrapped_caret() {
    let f = fixture();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=300,height=200,padding=0,
 ui.TextInput{id="box",multiline=true,text=ui.bind("query"),width=280,height=120}}"#,
    )
    .unwrap();
    let mut s = session(&f);
    let scene = s.focus_id("panel.box").unwrap().unwrap();
    s.commit(scene);
    // A single-line input ignores the line-break request; this one honours it.
    assert!(s.focused_multiline());
    let scene = s.key("ab", false).unwrap().unwrap();
    s.commit(scene);
    let scene = s.input_newline().unwrap().unwrap();
    s.commit(scene);
    let scene = s.key("cd", false).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("query"), Some(&Value::Text("ab\ncd".into())));
    // Up moves to the first line: typing there lands before the break.
    let scene = s.input_vertical(true, false).unwrap().unwrap();
    s.commit(scene);
    let scene = s.key("X", false).unwrap().unwrap();
    s.commit(scene);
    let Some(Value::Text(text)) = s.stores.get("query") else {
        panic!()
    };
    assert!(
        text.starts_with("ab") && text.contains('X') && text.ends_with("\ncd"),
        "{text:?}"
    );
    // The caret is drawn below the first line for text on the second.
    let mut scene = s.current.clone();
    s.decorate_inputs(&mut scene).unwrap();
    let carets = scene
        .primitives
        .iter()
        .filter(|p| matches!(p, Primitive::Rounded { rect, .. } if rect.w == 1.))
        .count();
    assert_eq!(carets, 1);
}
#[test]
fn masked_input_draws_bullets_and_never_exposes_its_value() {
    let f = fixture();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Panel{id="panel",width=300,height=200,padding=0,
 ui.TextInput{id="box",masked=true,text=ui.bind("query"),placeholder="Password",width=280,height=40}}"#,
    )
    .unwrap();
    let mut s = session(&f);
    let scene = s.focus_id("panel.box").unwrap().unwrap();
    s.commit(scene);
    let scene = s.key("pässw0rd", false).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("query"), Some(&Value::Text("pässw0rd".into())));
    assert!(s.focused_masked());
    // Drawn as one bullet per character, never the characters.
    assert!(texts(&s).iter().any(|t| t == "••••••••"), "{:?}", texts(&s));
    assert!(texts(&s).iter().all(|t| !t.contains("ssw0rd")));
    // Out of copy, IME context, accessibility, traces.
    assert_eq!(s.selected_text(), None);
    assert_eq!(s.surrounding_text(), Some((String::new(), 0, 0)));
    let tree = format!("{:?}", s.accessibility_tree());
    assert!(!tree.contains("ssw0rd"), "{tree}");
    // The caret sits after the last bullet, not at the end of the real text.
    let mut scene = s.current.clone();
    s.decorate_inputs(&mut scene).unwrap();
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Rounded { rect, .. } if rect.w == 1.))
    );
}

#[test]
fn memo_tracks_store_reads_inputs_and_repeat_identity() {
    let f = fixture();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"
local ui=require("bead.ui")
return ui.Panel{id="panel",width=400,height=240,
 ui.Repeat{id="items",model=ui.bind("rows"),key="id",render=function(item)
  return ui.memo("row",item,function(p)
   return ui.memo("inner",p,function(q)
    return ui.Button{id="tile",text=ui.resolve_for_check(ui.bind("query"))..q.name,
      on_activate=ui.action("select",q.id),width=100,height=30}
   end)
  end)
 end}}
"#,
    )
    .unwrap();
    let mut s = session(&f);
    let ids = s
        .view_nodes()
        .iter()
        .filter(|n| n.action.is_some())
        .map(|n| n.id.clone())
        .collect::<Vec<_>>();
    for query in ["first ", "second "] {
        let scene = s
            .update(BTreeMap::from([(
                "query".into(),
                Value::Text(query.into()),
            )]))
            .unwrap()
            .unwrap();
        assert!(
            scene
                .primitives
                .iter()
                .any(|p| matches!(p,Primitive::Text{text,..}if text==&format!("{query}Alpha")))
        );
        s.commit(scene);
        assert_eq!(
            s.view_nodes()
                .iter()
                .filter(|n| n.action.is_some())
                .map(|n| n.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
    }
    let scene = s
        .update(BTreeMap::from([(
            "rows".into(),
            Value::list(vec![serde_json::json!({"id":"a","name":"Replaced"})]),
        )]))
        .unwrap()
        .unwrap();
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p,Primitive::Text{text,..}if text=="second Replaced"))
    );
    s.commit(scene);
    assert_eq!(
        s.view_nodes().iter().filter(|n| n.action.is_some()).count(),
        1
    );
}

#[test]
fn memo_expansion_is_bounded_and_a_rejected_reload_keeps_the_view() {
    let f = fixture();
    let mut s = session(&f);
    let before = s.state_summary();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"
local ui=require("bead.ui")
local function large()
 local nodes={}
 for i=1,64 do nodes[i]=ui.Text{id="text"..i,text=string.rep("x",1024),height=1} end
 return ui.Column{id="chunk",table.unpack(nodes)}
end
local nodes={}
for i=1,200 do nodes[i]=ui.memo("large",{},large) end
error("memo loop escaped early admission")
return ui.Panel{id="panel",width=400,height=240,table.unpack(nodes)}
"#,
    )
    .unwrap();
    let error = s.reload().err().expect("oversized memo must fail");
    assert!(error.message.contains("exceeds 8 MiB"), "{error:?}");
    assert_eq!(s.state_summary(), before);
}

#[test]
fn pointer_preview_precedes_service_echo_and_keyboard_focus_restores_ring() {
    let f = fixture();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
local shown=ui.preview_value("volume",ui.resolve_for_check(ui.bind("scroll")))
return ui.Row{id="row",width=200,height=40,
 ui.Button{id="target",width=100,text=tostring(shown),preview_key="volume",preview_value=75,
  on_activate=ui.action("select","a")}}
"#,
    )
    .unwrap();
    let mut s = session(&f);
    let candidate = s
        .pointer_focus_id("row.target", true, None)
        .unwrap()
        .unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.focus_visible"),
        Some(&Value::Bool(false))
    );
    assert!(
        s.current
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "75"))
    );
    // An older topic echo cannot pull the knob back before the latest request settles.
    let candidate = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(30.))]))
        .unwrap()
        .unwrap();
    s.commit(candidate);
    assert!(
        s.current
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "75"))
    );
    let _ = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(42.))]))
        .unwrap();
    let candidate = s.release().unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(s.stores.get("scroll"), Some(&Value::Number(42.)));
    s.preview_deadline = Some(std::time::Instant::now());
    let candidate = s.update_model(BTreeMap::new()).unwrap().unwrap();
    s.commit(candidate);
    assert!(
        s.current
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "42"))
    );
    let candidate = s.focus_id("row.target").unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(s.stores.get("bead.focus_visible"), Some(&Value::Bool(true)));
    let pointer = s.pointer_input();
    let candidate = s.focus_id("row.target").unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.focus_visible"),
        Some(&Value::Bool(false))
    );
    drop(pointer);
    let candidate = s.focus_id("row.target").unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(s.stores.get("bead.focus_visible"), Some(&Value::Bool(true)));
}

#[test]
fn stationary_hover_does_not_reproject_services() {
    let f = fixture();
    let mut s = session(&f);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    s.set_projector(Box::new(move |_| {
        count.set(count.get() + 1);
        Ok(BTreeMap::new())
    }));
    let scene = s.hover(20., 20.).unwrap().unwrap();
    s.commit(scene);
    calls.set(0);
    assert!(s.hover(21., 20.).unwrap().is_none());
    assert_eq!(calls.get(), 0);
}

#[test]
fn local_feedback_skips_projector_until_the_next_model_wakeup() {
    let f = fixture();
    fs::write(
        f.0.join("ui/behavior.luau"),
        "return {stores={external=\"\"},actions={noop=function(s) end},computed={}}",
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
return ui.Column{id="root",width=200,height=100,
 ui.Button{id="target",width=100,height=40,text="Target",on_activate=ui.action("noop")},
 ui.Text{id="external",text=ui.bind("external")}}
"#,
    )
    .unwrap();
    let mut s = session(&f);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    s.set_projector(Box::new(move |_| {
        count.set(count.get() + 1);
        Ok(BTreeMap::from([(
            "external".into(),
            Value::Text("fresh model".into()),
        )]))
    }));

    let scene = s
        .with_local_feedback(|s| {
            if let Some(scene) = s.hover(20., 20.)? {
                s.commit(scene);
            }
            s.pointer_focus_id("root.target", true, None)
        })
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(calls.get(), 0);
    assert_eq!(
        s.stores.get("bead.hovered"),
        Some(&Value::Text("root.target".into()))
    );
    assert_eq!(
        s.stores.get("bead.focused"),
        Some(&Value::Text("root.target".into()))
    );

    let scene = s.update_model(BTreeMap::new()).unwrap().unwrap();
    s.commit(scene);
    assert_eq!(calls.get(), 1);
    assert!(
        s.current
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "fresh model"))
    );
}

#[test]
fn local_feedback_restores_projector_after_invalid_focus() {
    let f = fixture();
    let mut s = session(&f);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    s.set_projector(Box::new(move |_| {
        count.set(count.get() + 1);
        Ok(BTreeMap::new())
    }));

    let result = s
        .with_local_feedback(|s| s.focus_id("missing"))
        .err()
        .expect("invalid focus must fail");
    assert!(result.message.contains("focus"));
    assert_eq!(calls.get(), 0);
    assert!(s.update_model(BTreeMap::new()).unwrap().is_none());
    assert_eq!(calls.get(), 1);
}

#[test]
fn kit_slider_feedback_is_continuous_and_scoped_to_its_instance() {
    let f = fixture();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
local kit=require("bead.kit")
return ui.Column{id="ranges",width=400,height=80,
 ui.Row{id="row",width=400,height=40,
  kit.Slider{id="a",value=0,width=200,on_change="select"},
  kit.Slider{id="b",value=0,width=200,on_change="select"}},
 ui.Button{id="deferred",width=100,height=40,text="Hold",focus_on_pointer=false,activate_on_release=true,on_activate=ui.action("select","hold")}}
"#,
    )
    .unwrap();
    fs::write(
        f.0.join("ui/behavior.luau"),
        "return {stores={},actions={select=function(_)end},computed={}}",
    )
    .unwrap();
    let mut s = session(&f);
    let node = s
        .current_nodes
        .iter()
        .find(|n| n.id.ends_with("a.hits.value-5"))
        .unwrap();
    let id = node.id.clone();
    let r = rect(&node.draw);
    let x = r.x + r.w / 2.;
    let candidate = s.pointer_focus_id(&id, true, Some(x)).unwrap().unwrap();
    s.commit(candidate);
    let thumb = |suffix: &str| {
        let layer = s
            .current
            .layers
            .iter()
            .find(|layer| layer.id.ends_with(suffix))
            .unwrap();
        rect(&s.current.primitives[layer.own.start]).x + 4.
    };
    assert!(
        (thumb("a.thumb") - x).abs() <= 0.501,
        "thumb={} pointer={x} preview={:?}; layout rounds to whole logical pixels",
        thumb("a.thumb"),
        s.stores.get("bead.preview.value")
    );
    assert!(
        (thumb("b.thumb") - 210.).abs() < 0.001,
        "a peer with the same action must keep its own value"
    );
    assert!(
        s.drag_action(1000., -100.)
            .unwrap()
            .0
            .ends_with("a.hits.value-20")
    );
    let candidate = s.drag(1000., -100.).unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.preview.value"),
        Some(&Value::Number(100.))
    );
    let candidate = s.drag(-1000., -100.).unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(s.stores.get("bead.preview.value"), Some(&Value::Number(0.)));
    let candidate = s.drag(x, -100.).unwrap().unwrap();
    s.commit(candidate);
    let released = s.stores.get("bead.preview.value").cloned();
    let candidate = s.release().unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.preview.value"),
        released.as_ref(),
        "release keeps the exact pointer target"
    );
    assert!(
        s.drag_action(x, 15.).is_none(),
        "a released range has no grab"
    );
    let first = s
        .current_nodes
        .iter()
        .find(|node| node.id.ends_with("a.hits.value-0"))
        .unwrap()
        .id
        .clone();
    let candidate = s.focus_id(&first).unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.preview.value"),
        released.as_ref(),
        "Tab focus alone must not change a level"
    );
    let next = s.range_action(false).unwrap().0;
    let candidate = s.range_step_id(&next).unwrap().unwrap();
    s.commit(candidate);
    assert_eq!(
        s.stores.get("bead.preview.value"),
        Some(&Value::Number(5.)),
        "keyboard activation gets immediate discrete feedback"
    );
    assert_eq!(s.stores.get("bead.focus_visible"), Some(&Value::Bool(true)));
    let candidate = s.click(50., 60.).unwrap().unwrap();
    s.commit(candidate);
    assert!(
        s.drag_action(x, 15.).is_none(),
        "a deferred button press must not inherit a slider's old focus"
    );
}

#[test]
fn kit_slider_exact_pointer_arguments_and_source_acknowledgement() {
    let f = fixture();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Slider{id="slider",width=200,value=ui.bind("scroll"),step=5,
 on_change="select",change_arg=function(n) return string.format("level:%.5f",n) end}
"#,
    )
    .unwrap();
    let mut s = session(&f);
    // Start off-centre within a stepped hit cell, then jump faster than a
    // display frame across several cells in both directions and outside the row.
    let scene = s.click(43.25, 15.).unwrap().unwrap();
    s.commit(scene);
    for x in [123.75, 48.25, 187.5, 102.75] {
        let scene = s.drag(x, -50.).unwrap().unwrap();
        s.commit(scene);
        let Some(Value::Number(value)) = s.stores.get("bead.preview.value") else {
            panic!("numeric preview")
        };
        assert!((value - (f64::from(x) - 10.) / 180. * 100.).abs() < 0.00001);
        let Some(Value::Text(argument)) = s.stores.get("selected") else {
            panic!("mapped argument")
        };
        let sent: f64 = argument.strip_prefix("level:").unwrap().parse().unwrap();
        assert!(
            (sent - value).abs() < 0.00001,
            "argument must use this motion, not the preceding frame or cell"
        );
        let layer = s
            .current
            .layers
            .iter()
            .find(|layer| layer.id.ends_with("thumb"))
            .unwrap();
        let thumb = rect(&s.current.primitives[layer.own.start]);
        assert!(
            (thumb.x + thumb.w / 2. - x).abs() <= 0.501,
            "absolute pointer tracking, including vertical escape"
        );
    }
    let target = s.stores.get("bead.preview.value").cloned().unwrap();
    let scene = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(20.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("bead.preview.value"), Some(&target));
    let scene = s.release().unwrap().unwrap();
    s.commit(scene);
    assert_eq!(s.stores.get("bead.preview.value"), Some(&target));
    assert!(
        s.preview_deadline().is_none(),
        "no timeout may restore an old topic value"
    );
    let scene = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(35.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert!(!matches!(s.stores.get("bead.preview.key"),Some(Value::Text(key)) if key.is_empty()));
    let Value::Number(target) = target else {
        unreachable!()
    };
    let scene = s
        .update(BTreeMap::from([(
            "scroll".into(),
            Value::Number(target.round()),
        )]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.preview.key"),
        Some(&Value::Text(String::new())),
        "only convergence retires feedback"
    );
    let scene = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(80.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    let next = s.range_action(false).unwrap().0;
    let scene = s.range_step_id(&next).unwrap().unwrap();
    let Some(Value::Text(argument)) = s.pointer_argument(&next) else {
        panic!("keyboard preview")
    };
    assert_eq!(
        argument, "level:55.00000",
        "keyboard still advances by the declared five-unit step"
    );
    s.commit(scene);
}

#[test]
fn kit_slider_release_before_frame_acceptance_and_quantized_readback() {
    let f = fixture();
    fs::write(
        f.0.join("palette.json"),
        include_str!("../../../shell/palette.json"),
    )
    .unwrap();
    fs::write(
        f.0.join("ui/main.luau"),
        r#"local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Slider{id="slider",width=200,value=ui.bind("scroll"),step=5,on_change="select",
 change_arg=function(n) return tostring(n) end,
 echo_value=function(n) return math.floor(n/100*3+.5)/3*100 end}
"#,
    )
    .unwrap();
    let mut s = session(&f);
    let scene = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(33.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    let id = s
        .current_nodes
        .iter()
        .find(|n| n.id.ends_with("hits.value-10"))
        .unwrap()
        .id
        .clone();
    assert!(s.pointer_focus_id(&id, true, Some(100.)).unwrap().is_some());
    assert_eq!(
        s.stores.get("bead.pressed"),
        Some(&Value::Bool(false)),
        "the renderer has not accepted the press yet"
    );
    let release = s
        .release()
        .unwrap()
        .expect("release must consume the staged gesture");
    s.commit(release);
    assert_eq!(s.stores.get("bead.pressed"), Some(&Value::Bool(false)));
    assert_eq!(
        s.stores.get("bead.preview.value"),
        Some(&Value::Number(50.))
    );
    assert!(
        !matches!(s.stores.get("bead.preview.key"),Some(Value::Text(key)) if key.is_empty()),
        "the old 33% cannot acknowledge a target that rounds to 67%"
    );
    let scene = s
        .update(BTreeMap::from([("scroll".into(), Value::Number(67.))]))
        .unwrap()
        .unwrap();
    s.commit(scene);
    assert_eq!(
        s.stores.get("bead.preview.key"),
        Some(&Value::Text(String::new())),
        "a coarse device acknowledges its real quantized readback"
    );
}

#[test]
fn scroll_notches_clamp_at_both_content_edges() {
    let f = fixture();
    let mut session = session(&f);
    let rows = (0..60)
        .map(|i| serde_json::json!({"id":format!("row-{i}"),"name":format!("Item {i}")}))
        .collect();
    let scene = session
        .update(BTreeMap::from([("rows".into(), Value::list(rows))]))
        .unwrap()
        .unwrap();
    session.commit(scene);
    let scroll = session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with(".scroll"))
        .unwrap();
    let r = rect(&scroll.draw);
    let key = scroll.scroll_store.clone().unwrap();
    let limit = scroll.scroll_limit.unwrap();
    assert!(limit > 0.);
    for (delta, expected) in [(100_000., limit), (-100_000., 0.)] {
        let scene = session
            .scroll(r.x + r.w / 2., r.y + r.h / 2., delta)
            .unwrap()
            .unwrap();
        session.commit(scene);
        assert_eq!(session.stores.get(&key), Some(&Value::Number(expected)));
    }
}

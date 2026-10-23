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
fn session(f: &Fixture) -> Session {
    let mut session = f.session();
    session.entry = "ui/main.luau".into();
    let scene = session.reload().unwrap();
    session.check_sources().unwrap();
    session.commit(scene);
    session
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
        assert_eq!(session.stores.get("volume"), Some(&Value::Number(75.)));
        assert_eq!(session.range_action(true).unwrap().2, Value::Number(70.));
        assert_eq!(
            session
                .drag_action(r.x + r.w / 2., r.y + r.h / 2.)
                .unwrap()
                .2,
            Value::Number(75.)
        );
        assert!(session.drag_action(-1., -1.).is_none());
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

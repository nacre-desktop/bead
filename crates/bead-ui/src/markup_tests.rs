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
            .contains("must be string")
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

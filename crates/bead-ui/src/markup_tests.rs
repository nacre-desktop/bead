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
        Value::List(vec![
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

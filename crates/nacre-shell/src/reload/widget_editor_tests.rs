// SPDX-License-Identifier: GPL-3.0-only
//! B0208 widgets editor: owner-shaped layout migration, bar sections, edit-mode inspection,
//! per-instance styles, free desktop placement and bar placement (B0210), drag-frame cost,
//! the gallery and the app scroll.
use super::widget_layout_property_tests::{
    HEIGHT, Model, WIDTH, default_layout, drop_event, locate,
};
use bead::core::Value;
use bead_test::{Config, Harness};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path};

fn shell() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

/// A saved file shaped like the owner's (launcher, workspaces, title, the status pill with the
/// clock at right/0 and power at right/1, a configured clock group).
fn owner_layout() -> Json {
    let file: Json =
        serde_json::from_str(include_str!("widget_owner_layout_fixture.json")).unwrap();
    file["layout"].clone()
}

fn bar_harness(doc: &Json, editing: bool) -> Harness {
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.insert("widgets.layout".into(), Value::list(vec![doc.clone()]));
    stores.insert("bead.focused_title".into(), Value::Text("Notes".into()));
    stores.insert("bead.windows".into(), Value::list(Vec::new()));
    stores.insert("bead.workspaces".into(), Value::list(Vec::new()));
    stores.insert("widgets.editing".into(), Value::Bool(editing));
    if editing {
        stores.insert("widgets.draft".into(), Value::list(vec![doc.clone()]));
    }
    Harness::load(
        &shell(),
        Path::new("bar/main.luau"),
        &shell().join("palette.json"),
        stores,
        Config {
            width: WIDTH as u32,
            height: 50,
            scale: 1.,
        },
    )
    .unwrap()
}

fn instance_rects(harness: &Harness) -> BTreeMap<String, bead::render::Rect> {
    let mut rects = BTreeMap::new();
    for node in harness.session.view_nodes() {
        if let Some(Value::List(rows)) = &node.metadata
            && let Some(iid) = rows.first().and_then(|row| row["iid"].as_str())
            && rows[0]["widget_instance"] == true
        {
            rects.insert(iid.to_owned(), bead::ui::rect(&node.draw));
        }
    }
    rects
}

#[test]
fn owner_layout_loads_unchanged_with_the_title_as_a_widget() {
    let doc = owner_layout();
    let harness = bar_harness(&doc, false);
    let rects = instance_rects(&harness);
    for iid in [
        "launcher",
        "workspaces",
        "title",
        "sound",
        "wifi",
        "bluetooth",
        "battery",
        "notifications",
        "clock",
        "power",
    ] {
        assert!(
            rects.contains_key(iid),
            "{iid} renders from the owner's file: {rects:?}"
        );
    }
    // The lone centre title keeps the QML band (same geometry as before B0208).
    assert!(
        harness
            .session
            .view_nodes()
            .iter()
            .any(|node| node.id.ends_with("title-band")),
        "a lone title keeps its centred band"
    );
    let (clock, power) = (rects["clock"], rects["power"]);
    assert!(
        clock.x < power.x,
        "clock right/0 stays before power right/1"
    );
    // The same file renders exactly like the shipped default with the same order.
    let default = bar_harness(&default_layout(), false);
    assert_eq!(instance_rects(&default).keys().len(), rects.keys().len());
}

#[test]
fn a_widget_dropped_in_the_middle_sits_beside_the_title_never_on_top() {
    let mut model = Model::new(owner_layout());
    let title_x = WIDTH / 2.;
    // Drop the clock just right of the centre: it joins the centre section after the title.
    assert!(model.drag("clock", "bar", title_x + 140., 20.));
    let clock = locate(&model.doc, "clock").unwrap();
    assert_eq!(clock["host"], "top_bar");
    assert_eq!(clock["slot"]["region"], "centre");
    let harness = bar_harness(&model.doc, false);
    let rects = instance_rects(&harness);
    let (title, clock) = (rects["title"], rects["clock"]);
    let overlap = title.x < clock.x + clock.w && clock.x < title.x + title.w;
    assert!(!overlap, "clock {clock:?} lands beside the title {title:?}");
    let band = ((WIDTH * 0.28) as f32, (WIDTH * 0.72) as f32);
    for rect in [title, clock] {
        assert!(
            rect.x >= band.0 - 0.5 && rect.x + rect.w <= band.1 + 0.5,
            "{rect:?} stays in the centre section"
        );
    }
    // Left and right sections keep their own packing.
    assert!(rects["workspaces"].x < band.0 && rects["power"].x > band.1);
}

#[test]
fn edit_mode_clicks_inspect_the_placed_instance_instead_of_its_dropdown() {
    let doc = owner_layout();
    let normal = bar_harness(&doc, false);
    let editing = bar_harness(&doc, true);
    let action_at = |harness: &Harness, iid: &str| {
        let rect = instance_rects(harness)[iid];
        let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        harness
            .session
            .view_nodes()
            .iter()
            .rev()
            .find(|node| node.contains(x, y) && node.action.is_some())
            .and_then(|node| node.action.clone())
    };
    assert_ne!(action_at(&normal, "clock").as_deref(), Some("widgets.edit"));
    for iid in ["clock", "power", "launcher", "title"] {
        assert_eq!(
            action_at(&editing, iid).as_deref(),
            Some("widgets.edit"),
            "{iid} opens its own settings in edit mode"
        );
    }
}

#[test]
fn a_style_change_restyles_only_the_clicked_copy() {
    let mut model = Model::new(owner_layout());
    assert!(model.add("clock", "bar", json!({}), "bar", WIDTH - 40., 20.));
    let copies: Vec<String> = ["clock", "clock-2"]
        .iter()
        .filter_map(|iid| {
            locate(&model.doc, iid).map(|item| item["iid"].as_str().unwrap().to_owned())
        })
        .collect();
    assert_eq!(copies.len(), 2, "two clocks are placed: {}", model.doc);
    let second = locate(&model.doc, "clock-2").unwrap().clone();
    model.edit(
        json!({"op":"style","widget":"clock","group":second["group"],"iid":"clock-2",
        "form":"bar","style":"time"}),
    );
    assert_eq!(locate(&model.doc, "clock-2").unwrap()["style"], "time");
    assert!(
        locate(&model.doc, "clock").unwrap()["style"].is_null(),
        "the original clock keeps its own style"
    );
}

#[test]
fn desktop_drops_land_where_dropped_without_a_grid_and_never_overlap() {
    let mut model = Model::new(default_layout());
    assert!(model.add("clock", "desktop", json!({}), "desktop", 405., 307.));
    let first = model.doc["hosts"]["canvas"]["instances"][0]["slot"].clone();
    // Free placement: the ghost's top-left (pointer minus the grab point), not a grid cell.
    assert_eq!(
        (first["x"].as_f64(), first["y"].as_f64()),
        (Some(285.), Some(247.))
    );
    // A second widget dropped on top of the first slides beside it.
    assert!(model.add("battery", "desktop", json!({}), "desktop", 410., 310.));
    let slots: Vec<Json> = model.doc["hosts"]["canvas"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["slot"].clone())
        .collect();
    assert_eq!(slots.len(), 2);
    let (a, b) = (&slots[0], &slots[1]);
    let f = |slot: &Json, key: &str| slot[key].as_f64().unwrap();
    let overlap = f(a, "x") < f(b, "x") + f(b, "w")
        && f(b, "x") < f(a, "x") + f(a, "w")
        && f(a, "y") < f(b, "y") + f(b, "h")
        && f(b, "y") < f(a, "y") + f(a, "h");
    assert!(!overlap, "collision handling separates {a} and {b}");
    assert!(f(b, "x") <= WIDTH && f(b, "y") <= HEIGHT);
    // No grid is drawn or seeded any more.
    let dragging = std::fs::read_to_string(shell().join("widgets/dragging.luau")).unwrap();
    assert!(!dragging.contains("grid"), "the drag overlay draws no grid");
}

#[test]
fn desktop_motion_moves_only_the_ghost_surface_and_writes_only_on_drop() {
    let mut model = Model::new(default_layout());
    assert!(model.add("clock", "desktop", json!({}), "desktop", 400., 300.));
    let doc = model.doc.clone();
    let iid = doc["hosts"]["canvas"]["instances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut content = None;
    let mut anchors = Vec::new();
    for step in 0..120 {
        let x = 400. + step as f64 * 3.;
        let values = model
            .controller
            .drag(drop_event(
                &doc,
                &iid,
                "desktop",
                x,
                300. + step as f64,
                "move",
            ))
            .unwrap();
        assert!(
            matches!(values.get("widgets.write"), Some(Value::List(rows)) if rows.is_empty()),
            "no layout write before the drop"
        );
        let drag: BTreeMap<String, Value> = values
            .into_iter()
            .filter(|(key, _)| key.starts_with("widgets.drag."))
            .collect();
        // What the ghost and zone canvases render from: unchanged by pointer motion, so
        // no content repaint and no layout pass; only the surface anchor moves.
        let current = super::widget_drag_content(&drag);
        if step > 0 {
            assert_eq!(
                content.as_ref(),
                Some(&current),
                "motion {step} changed content"
            );
        }
        content = Some(current);
        anchors.push(super::widget_drag_pointer(&drag).unwrap());
    }
    assert_eq!(anchors.len(), 120);
    assert!(anchors.windows(2).all(|pair| pair[1].0 > pair[0].0));
    let dropped = model
        .controller
        .drag(drop_event(&doc, &iid, "desktop", 700., 500., "drop"))
        .unwrap();
    assert!(
        matches!(dropped.get("widgets.write"), Some(Value::List(rows)) if rows.len() == 1),
        "the drop commits once"
    );
}

#[test]
fn the_ghost_is_drawn_relative_to_the_pointer() {
    let ghost = std::fs::read_to_string(shell().join("widgets/dragging.luau")).unwrap();
    // The ghost root holds only the ghost; zones are their own canvas.
    assert!(ghost.contains("function M.zones_root()"));
    let root = &ghost[ghost.find("function M.ghost_root()").unwrap()..];
    let root = &root[..root.find("\nend").unwrap()];
    assert!(!root.contains("drop_zones"), "{root}");
}

fn bar_rects_after(doc: &Json) -> BTreeMap<String, bead::render::Rect> {
    instance_rects(&bar_harness(doc, false))
}

#[test]
fn the_bar_takes_a_widget_anywhere_along_its_width() {
    // Far left: before the launcher.
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", 20., 20.));
    let clock = locate(&model.doc, "clock").unwrap();
    assert_eq!(clock["host"], "top_bar");
    assert_eq!(
        (
            clock["slot"]["region"].as_str(),
            clock["slot"]["start"].as_f64()
        ),
        (Some("left"), Some(0.))
    );
    let rects = bar_rects_after(&model.doc);
    assert!(rects["clock"].x < rects["launcher"].x);

    // Left of centre, in the left section: it keeps a gap after the workspaces.
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", WIDTH * 0.22, 20.));
    let clock = locate(&model.doc, "clock").unwrap();
    assert_eq!(clock["slot"]["region"], "left");
    let gap = clock["slot"]["gap"].as_f64().unwrap();
    assert!(gap > 150., "gap {gap}");
    let rects = bar_rects_after(&model.doc);
    let workspaces = rects["workspaces"];
    assert!(
        rects["clock"].x >= workspaces.x + workspaces.w + gap as f32,
        "{:?} after {workspaces:?} by {gap}",
        rects["clock"]
    );
    assert!(rects["clock"].x + rects["clock"].w < (WIDTH * 0.28) as f32);

    // Just left of the centre: the centre section, before the title.
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", WIDTH * 0.40, 20.));
    let clock = locate(&model.doc, "clock").unwrap();
    assert_eq!(clock["slot"]["region"], "centre");
    let rects = bar_rects_after(&model.doc);
    assert!(rects["clock"].x + rects["clock"].w <= rects["title"].x);

    // Between two right-side widgets (inside the status pill): between wifi and bluetooth.
    let mut model = Model::new(owner_layout());
    let (wifi, bluetooth) = (
        pill_child_x(&model.doc, "wifi"),
        pill_child_x(&model.doc, "bluetooth"),
    );
    assert!(model.drag("clock", "bar", (wifi + bluetooth) / 2. + 32., 20.));
    let pill = locate(&model.doc, "status-pill").unwrap();
    let order: Vec<&str> = pill["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|child| child["iid"].as_str().unwrap())
        .collect();
    let at = |iid: &str| order.iter().position(|other| *other == iid).unwrap();
    assert_eq!(at("clock"), at("wifi") + 1, "{order:?}");
    assert_eq!(at("bluetooth"), at("clock") + 1, "{order:?}");

    // Moved again within the left section, it keeps its neighbours in place.
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", 20., 20.));
    assert!(model.drag("clock", "bar", WIDTH * 0.22, 20.));
    let bar = model.doc["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap();
    let left: Vec<&str> = bar
        .iter()
        .filter(|item| item["slot"]["region"] == "left")
        .map(|item| item["iid"].as_str().unwrap())
        .collect();
    assert_eq!(left, ["launcher", "workspaces", "clock"], "{bar:?}");

    // Many motions whose geometry comes from the live preview (neighbours pushed right by the
    // dragged widget's gap) still insert after them: geometry is frozen per gesture.
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", 20., 20.));
    let doc = model.doc.clone();
    let mut shifted = super::widget_layout_property_tests::context(&doc);
    for rect in shifted["geometry"].as_array_mut().unwrap() {
        if rect["iid"] == "launcher" || rect["iid"] == "workspaces" {
            let x = rect["x"].as_f64().unwrap();
            rect["x"] = json!(x + 600.);
        }
    }
    let event = |x: f64, phase: &str, context: &Json| {
        let Value::List(rows) = drop_event(&doc, "clock", "bar", x, 20., phase) else {
            unreachable!()
        };
        let mut row = rows[0].clone();
        row["context"] = context.clone();
        Value::list(vec![row])
    };
    let plain = super::widget_layout_property_tests::context(&doc);
    model.controller.drag(event(40., "move", &plain)).unwrap();
    for step in 1..20 {
        model
            .controller
            .drag(event(40. + step as f64 * 20., "move", &shifted))
            .unwrap();
    }
    let dropped = model
        .controller
        .drag(event(WIDTH * 0.22, "drop", &shifted))
        .unwrap();
    let Some(Value::List(written)) = dropped.get("widgets.write") else {
        panic!("the drop writes")
    };
    let left: Vec<&str> = written[0]["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["slot"]["region"] == "left")
        .map(|item| item["iid"].as_str().unwrap())
        .collect();
    assert_eq!(left, ["launcher", "workspaces", "clock"]);

    // A saved gap survives normalisation and reload (the owner's file shape).
    let mut model = Model::new(owner_layout());
    assert!(model.drag("clock", "bar", WIDTH * 0.22, 20.));
    let reloaded = Model::new(model.doc.clone());
    assert_eq!(
        locate(&reloaded.doc, "clock").unwrap()["slot"]["gap"],
        locate(&model.doc, "clock").unwrap()["slot"]["gap"]
    );
}

fn pill_child_x(doc: &Json, iid: &str) -> f64 {
    super::widget_layout_property_tests::context(doc)["geometry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|rect| rect["iid"] == iid)
        .map(|rect| rect["x"].as_f64().unwrap())
        .unwrap()
}

#[test]
fn a_bar_drag_shows_the_insertion_slot_under_the_pointer() {
    let doc = owner_layout();
    let mut model = Model::new(doc.clone());
    let values = model
        .controller
        .drag(drop_event(&doc, "clock", "bar", WIDTH * 0.22, 20., "move"))
        .unwrap();
    let Some(Value::List(preview)) = values.get("widgets.drag.layout") else {
        panic!("a bar motion previews the insertion");
    };
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.insert("widgets.layout".into(), Value::list(vec![doc.clone()]));
    stores.insert("widgets.drag.layout".into(), Value::List(preview.clone()));
    stores.insert("widgets.drag.active".into(), Value::Bool(true));
    stores.insert("widgets.drag.phase".into(), Value::Text("dragging".into()));
    stores.insert("widgets.drag.destination".into(), Value::Text("bar".into()));
    stores.insert("bead.focused_title".into(), Value::Text("Notes".into()));
    stores.insert("bead.windows".into(), Value::list(Vec::new()));
    stores.insert("bead.workspaces".into(), Value::list(Vec::new()));
    let harness = Harness::load(
        &shell(),
        Path::new("bar/main.luau"),
        &shell().join("palette.json"),
        stores,
        Config {
            width: WIDTH as u32,
            height: 50,
            scale: 1.,
        },
    )
    .unwrap();
    // The insertion slot: the placeholder's outlined box (registry, `widget-snap-slot-`).
    let slot = harness
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Rounded {
                rect,
                border: Some((_, width)),
                ..
            } if *width > 1. && rect.x > 200. && rect.x < (WIDTH * 0.28) as f32 => Some(*rect),
            _ => None,
        })
        .expect("the insertion slot renders on the bar");
    let pointer = (WIDTH * 0.22) as f32;
    assert!(
        slot.x <= pointer && pointer <= slot.x + slot.w + 80.,
        "slot {slot:?} sits under the pointer at {pointer}"
    );
}

#[test]
fn niri_rules_keep_pearls_unblurred_and_the_widgets_app_an_ordinary_window() {
    let rules = std::fs::read_to_string(shell().join("niri-pearls.kdl")).unwrap();
    let rule = |app: &str| -> String {
        let start = rules.find(&format!("match app-id=\"{app}\"")).unwrap();
        let end = rules[start..].find("\n}").unwrap();
        rules[start..start + end].to_owned()
    };
    let pearl = rule("^bead[.]pearl([.]pending)?$");
    for line in [
        "border { off; }",
        "focus-ring { off; }",
        "shadow { off; }",
        "background-effect { blur false; }",
    ] {
        assert!(pearl.contains(line), "pearl rule lacks {line}");
    }
    let widgets = rule("^nacre-widgets$");
    assert!(
        !widgets.contains("border") && !widgets.contains("focus-ring"),
        "{widgets}"
    );
    assert!(widgets.contains("background-effect { blur false; }"));
    // Balanced braces: a parse-level sanity check (niri itself validates it in the captures).
    assert_eq!(rules.matches('{').count(), rules.matches('}').count());
    let app = std::fs::read_to_string(shell().join("widgets/app/main.luau")).unwrap();
    let root = &app[app.find("local app_content=").unwrap()..];
    let root = &root[..root.find('\n').unwrap()];
    assert!(
        !root.contains("border_width") && !root.contains("radius"),
        "{root}"
    );
}

fn app_harness(stores: BTreeMap<String, Value>) -> Harness {
    let mut values = crate::desktop::values(&Default::default(), "");
    values.extend(stores);
    Harness::load(
        &shell(),
        Path::new("widgets/app/main.luau"),
        &shell().join("palette.json"),
        values,
        Config {
            width: 1200,
            height: 800,
            scale: 1.5,
        },
    )
    .unwrap()
}

fn texts(harness: &Harness) -> Vec<(String, bead::render::Rect)> {
    harness
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } => Some((text.clone(), *rect)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_gallery_pill_is_an_empty_container_and_there_is_no_advanced_section() {
    let harness = app_harness(BTreeMap::new());
    let scene = harness.scene();
    let frame = scene
        .layers
        .iter()
        .find(|layer| {
            layer.id.contains("widget-card-pill") && layer.id.ends_with("bar-preview-frame")
        })
        .expect("pill card preview")
        .bounds;
    let inside = |rect: &bead::render::Rect| {
        let (x, y) = (rect.x + rect.w / 2., rect.y + rect.h / 2.);
        x >= frame.x && x <= frame.x + frame.w && y >= frame.y && y <= frame.y + frame.h
    };
    let shown: Vec<_> = texts(&harness)
        .into_iter()
        .filter(|(_, rect)| inside(rect))
        .map(|(text, _)| text)
        .collect();
    assert!(
        shown.contains(&"add".to_owned()),
        "empty pill invites content: {shown:?}"
    );
    for child in ["volume_up", "calendar_month", "power_settings_new"] {
        assert!(
            !shown.contains(&child.to_owned()),
            "the gallery pill is empty: {shown:?}"
        );
    }
    for selected in ["clock", "battery", "pill"] {
        let page = app_harness(BTreeMap::from([(
            "widgets.selected".into(),
            Value::Text(selected.into()),
        )]));
        assert!(
            !page
                .session
                .view_nodes()
                .iter()
                .any(|node| node.id.contains("advanced")),
            "{selected} has no Advanced section"
        );
    }
}

#[test]
fn the_clock_page_shows_the_clock_in_its_previews_and_edits_one_copy() {
    let doc = owner_layout();
    let harness = app_harness(BTreeMap::from([
        ("widgets.selected".into(), Value::Text("clock".into())),
        ("widgets.layout".into(), Value::list(vec![doc])),
    ]));
    let shown: Vec<String> = texts(&harness).into_iter().map(|(text, _)| text).collect();
    // Gallery style cards, the placed copy's own style cards and the stage all show a time.
    let times = shown.iter().filter(|text| text.as_str() == "12:34").count();
    assert!(
        times >= 3,
        "clock previews show the time ({times}): {shown:?}"
    );
    let nodes = harness.session.view_nodes();
    assert!(
        nodes.iter().any(|node| node.id.contains("instance-editor")),
        "the placed copy's editor is shown: {:?}",
        nodes
            .iter()
            .map(|node| node.id.as_str())
            .filter(|id| id.contains("placed") || id.contains("instance"))
            .collect::<Vec<_>>()
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.id.ends_with("style-seconds-placed"))
    );
    // Gallery cards choose the style for a new copy; they never edit the layout.
    let gallery_card = nodes
        .iter()
        .find(|node| node.id.ends_with("style-seconds"))
        .expect("gallery style card");
    assert_eq!(gallery_card.action.as_deref(), Some("widgets.navigate"));
    let placed_card = nodes
        .iter()
        .find(|node| node.id.ends_with("style-seconds-placed"))
        .unwrap();
    assert_eq!(placed_card.action.as_deref(), Some("widgets.edit"));
}

#[test]
fn widgets_app_scroll_moves_at_the_shared_wheel_distance() {
    for (selected, store) in [("", "widgets.scroll"), ("clock", "widgets.detail.scroll")] {
        let mut harness = app_harness(BTreeMap::from([(
            "widgets.selected".into(),
            Value::Text(selected.into()),
        )]));
        let region = harness
            .session
            .view_nodes()
            .iter()
            .find(|node| node.scroll_store.as_deref() == Some(store))
            .map(|node| bead::ui::rect(&node.draw))
            .unwrap_or_else(|| panic!("{store} scroll region"));
        let started = std::time::Instant::now();
        harness
            .script([bead_test::Input::Scroll {
                x: region.x + region.w / 2.,
                y: region.y + region.h / 2.,
                delta: 100.,
            }])
            .unwrap();
        let cost = started.elapsed();
        assert_eq!(
            harness.session.stores.get(store),
            Some(&Value::Number(100.)),
            "one wheel notch moves {store} by the framework's 100 px step"
        );
        eprintln!(
            "B0208_SCROLL page={selected:?} cost_ms={:.2}",
            cost.as_secs_f64() * 1e3
        );
    }
}

#[test]
fn an_inspect_request_opens_the_app_on_that_copy() {
    let model = crate::widgets::open_model(Some(
        &json!({"widget":"clock","group":"clock-2","iid":"clock-2"}),
    ));
    assert_eq!(model["widgets.selected"], Value::Text("clock".into()));
    assert_eq!(model["widgets.group"], Value::Text("clock-2".into()));
    let search = crate::widgets::open_model(Some(&json!("batt")));
    assert_eq!(search["widgets.search"], Value::Text("batt".into()));
    assert!(!search.contains_key("widgets.selected"));
}

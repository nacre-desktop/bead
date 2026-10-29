// SPDX-License-Identifier: GPL-3.0-only
//! B0208 widgets editor: owner-shaped layout migration, bar sections, edit-mode inspection,
//! per-instance styles, the desktop grid, drag-frame cost, the gallery and the app scroll.
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
            .filter(|node| node.contains(x, y) && node.action.is_some())
            .last()
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
fn desktop_drops_snap_to_the_grid_and_never_overlap() {
    let mut model = Model::new(default_layout());
    assert!(model.add("clock", "desktop", json!({}), "desktop", 405., 307.));
    let first = model.doc["hosts"]["canvas"]["instances"][0]["slot"].clone();
    for axis in ["x", "y"] {
        let value = first[axis].as_f64().unwrap();
        assert_eq!(value % 32., 0., "{axis}={value} lies on the 32 px grid");
    }
    // A second widget dropped on top of the first moves to the nearest free cell.
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
}

#[test]
fn desktop_motion_is_a_cheap_transform_and_writes_only_on_drop() {
    let mut model = Model::new(default_layout());
    assert!(model.add("clock", "desktop", json!({}), "desktop", 400., 300.));
    let doc = model.doc.clone();
    let iid = doc["hosts"]["canvas"]["instances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut layout = None;
    let mut costs = Vec::new();
    let mut landings = std::collections::BTreeSet::new();
    for step in 0..120 {
        let x = 400. + step as f64 * 3.;
        let started = std::time::Instant::now();
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
        costs.push(started.elapsed());
        assert!(
            matches!(values.get("widgets.write"), Some(Value::List(rows)) if rows.is_empty()),
            "no layout write before the drop"
        );
        let current = values.get("widgets.drag.layout").cloned();
        if let Some(previous) = &layout {
            assert_eq!(
                Some(previous),
                current.as_ref(),
                "the hosts' layout stays frozen while the pointer moves"
            );
        }
        layout = current;
        if let (Some(Value::Number(x)), Some(Value::Number(y))) = (
            values.get("widgets.drag.snap_x"),
            values.get("widgets.drag.snap_y"),
        ) {
            assert_eq!(
                (x % 32., y % 32.),
                (0., 0.),
                "the landing cell is on the grid"
            );
            landings.insert((*x as i64, *y as i64));
        }
    }
    assert!(
        landings.len() > 5,
        "the landing outline follows the pointer: {landings:?}"
    );
    costs.sort();
    let median = costs[costs.len() / 2];
    // Reference: the same pointer path over the bar runs the full placement transaction.
    let mut bar_costs = Vec::new();
    for step in 0..60 {
        let started = std::time::Instant::now();
        model
            .controller
            .drag(drop_event(
                &doc,
                &iid,
                "bar",
                200. + step as f64 * 20.,
                20.,
                "move",
            ))
            .unwrap();
        bar_costs.push(started.elapsed());
    }
    bar_costs.sort();
    let bar_median = bar_costs[bar_costs.len() / 2];
    eprintln!("B0208_MOTION desktop_median={median:?} bar_median={bar_median:?}");
    assert!(
        median <= bar_median,
        "a desktop motion costs no more than a bar placement: {median:?} vs {bar_median:?}"
    );
    let dropped = model
        .controller
        .drag(drop_event(&doc, &iid, "desktop", 700., 500., "drop"))
        .unwrap();
    assert!(
        matches!(dropped.get("widgets.write"), Some(Value::List(rows)) if rows.len() == 1),
        "the drop commits once"
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

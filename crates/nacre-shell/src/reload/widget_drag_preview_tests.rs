// SPDX-License-Identifier: GPL-3.0-only
//! Cross-host drag preview is based on one normalized snapshot and commits only on drop.
use super::{
    Runtime, SceneSource, UiEvent, suppress_widget_drag_hover, widget_drag_source_bounds,
    widget_drag_targets_output,
};
use crate::widgets::widget_drag::Controller;
use bead::core::Value;
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, HashSet};

fn document() -> Json {
    json!({
        "version":1,
        "hosts":{"top_bar":{"mode":"content","tiles":60,"regions":{
            "left":{"start":0,"span":20},"centre":{"start":20,"span":20},"right":{"start":40,"span":20}
        },"instances":[
            {"iid":"clock-main","group":"clock-main","widget":"clock","host":"top_bar","style":"time","slot":{"region":"left","start":0,"span":1},"settings":{"future":"kept"}},
            {"iid":"network-main","group":"network-main","widget":"network","host":"top_bar","style":"icon","slot":{"region":"left","start":1,"span":1},"settings":{}}
        ]}}
    })
}

#[test]
fn prepared_source_bounds_clip_before_padding_at_output_edges() {
    let bounds =
        |x, y, width, height| widget_drag_source_bounds(x, y, width, height, 1920., 1080.).unwrap();
    assert_eq!(
        bounds(100., 100., 88., 34.),
        bead::render::Rect::new(100., 100., 89., 35.)
    );
    assert_eq!(
        bounds(1832., 100., 88., 34.),
        bead::render::Rect::new(1832., 100., 88., 35.)
    );
    assert_eq!(
        bounds(100., 1046., 88., 34.),
        bead::render::Rect::new(100., 1046., 89., 34.)
    );
    assert_eq!(
        bounds(-10., -5., 88., 34.),
        bead::render::Rect::new(0., 0., 79., 30.)
    );
    assert!(widget_drag_source_bounds(1920., 20., 10., 10., 1920., 1080.).is_none());
}

#[test]
fn dropped_ghost_consumes_leave_event_before_rearming_hover() {
    let mut suppressed = true;
    assert!(suppress_widget_drag_hover(&mut suppressed, true, false));
    assert!(
        suppressed,
        "timer refreshes while over the target must not rebuild the ghost"
    );
    assert!(suppress_widget_drag_hover(&mut suppressed, true, true));
    assert!(
        suppressed,
        "hovering the drop target must not rebuild the ghost"
    );
    assert!(suppress_widget_drag_hover(&mut suppressed, false, false));
    assert!(suppressed, "non-pointer updates do not prove pointer leave");
    assert!(suppress_widget_drag_hover(&mut suppressed, false, true));
    assert!(
        !suppressed,
        "leaving clears suppression but consumes the pointer event"
    );
    assert!(!suppress_widget_drag_hover(&mut suppressed, true, true));
}

fn catalogue() -> Json {
    json!([
        {"id":"clock","defaults":{},"forms":{"bar":{"default_style":"time","styles":{"time":{"min_span":1,"max_span":4,"preferred_span":2}}}}},
        {"id":"network","defaults":{},"forms":{"bar":{"default_style":"icon","styles":{"icon":{"min_span":1,"max_span":2,"preferred_span":2}}}}}
    ])
}
fn bar_style_catalogue() -> Json {
    json!([
        {"id":"notifications","defaults":{},"forms":{"bar":{"default_style":"icon","styles":{
            "icon":{"min_span":1,"max_span":8,"preferred_span":1},
            "icon+value":{"min_span":1,"max_span":8,"preferred_span":2},
            "icon+text":{"min_span":1,"max_span":8,"preferred_span":3}
        }}}},
        {"id":"pill","defaults":{},"forms":{"bar":{"default_style":"pill","styles":{
            "pill":{"min_span":1,"max_span":10,"preferred_span":1}
        }}}}
    ])
}
fn bar_style_document(
    child_span: u32,
    saved_span: u32,
    pill_span: u32,
    region_span: u32,
    live: bool,
) -> Json {
    let notifications = json!({"iid":"notifications","group":"notifications","widget":"notifications",
        "host":"status-pill","style":"icon","slot":{"region":"centre","start":0,"span":child_span},"settings":{}});
    let wifi = json!({"iid":"wifi","widget":"wifi","host":"status-pill","style":"icon",
        "slot":{"region":"centre","start":child_span,"span":1},"settings":{}});
    let mut children = vec![wifi];
    if live {
        children.insert(0, notifications);
    }
    json!({
        "version":1,
        "widgets":{"notifications":{"widget":"notifications","settings":{"future":"keep"},"forms":{
            "bar":{"enabled":live,"style":"icon","placement":{"host":"status-pill",
                "future_placement":"keep","slot":{"region":"centre","start":0,"span":saved_span,"future_slot":"keep"}}},
            "desktop":{"enabled":false,"style":"summary","placement":{}}}}},
        "hosts":{"top_bar":{"mode":"tiles","tiles":10,"regions":{
            "left":{"start":0,"span":region_span},"centre":{"start":0,"span":10},"right":{"start":0,"span":10}},
            "instances":[
                {"iid":"status-pill","widget":"pill","host":"top_bar","style":"pill",
                    "slot":{"region":"left","start":0,"span":pill_span},"settings":{},"children":children},
                {"iid":"bar-neighbor","widget":"title","host":"top_bar","style":"default",
                    "slot":{"region":"left","start":pill_span,"span":1},"settings":{}}
            ]}}
    })
}
fn style_controller(doc: &Json) -> Controller {
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![doc.clone()])),
            ("widgets.rev".into(), Value::Number(1.)),
        ]))
        .unwrap();
    controller
}
fn apply_bar_style(
    controller: &mut Controller,
    doc: &Json,
    style: &str,
) -> BTreeMap<String, Value> {
    controller
        .edit(Value::list(vec![
            json!({"op":"style","widget":"notifications","group":"notifications",
        "form":"bar","style":style,"document":doc,"catalogue":bar_style_catalogue()}),
        ]))
        .unwrap()
}
fn context() -> Json {
    json!({"catalogue":catalogue(),"screen_width":1920,"screen_height":1080,
        "geometry":[{"iid":"clock-main","x":0,"y":0,"w":64,"h":40},{"iid":"network-main","x":64,"y":0,"w":64,"h":40}],
        "reservations":[{"iid":"clock-main","width":64},{"iid":"network-main","width":64}]})
}
fn pill_context() -> Json {
    let mut packages = catalogue().as_array().unwrap().clone();
    packages.push(json!({"id":"pill","defaults":{},"forms":{"bar":{"default_style":"pill","styles":{"pill":{"min_span":1,"max_span":60,"preferred_span":1}}}}}));
    json!({"catalogue":packages,"screen_width":1920,"screen_height":1080,"snap_distance":16,
        "geometry":[{"iid":"clock-main","x":0,"y":0,"w":64,"h":40},
            {"iid":"network-main","x":64,"y":0,"w":64,"h":40},
            {"iid":"status-pill","x":1664,"y":0,"w":192,"h":40}],
        "reservations":[{"iid":"clock-main","width":64},{"iid":"network-main","width":64},{"iid":"status-pill","width":192}]})
}
fn pill_document(child: bool) -> Json {
    let mut doc = document();
    doc["hosts"]["top_bar"]["instances"][1]["settings"] = json!({"future":"keep"});
    doc["hosts"]["top_bar"]["instances"][1]["opaque"] = json!({"nested":[1,"preserved"]});
    let mut pill = json!({"iid":"status-pill","group":"status-pill","widget":"pill","host":"top_bar","style":"pill",
        "slot":{"region":"right","start":0,"span":6},"settings":{"opaque":true},"future_field":{"keep":7},"children":[]});
    if child {
        let mut moved = doc["hosts"]["top_bar"]["instances"][1].clone();
        moved["host"] = json!("status-pill");
        moved["slot"] = json!({"region":"centre","start":0,"span":2});
        pill["children"] = json!([moved]);
        doc["hosts"]["top_bar"]["instances"]
            .as_array_mut()
            .unwrap()
            .remove(1);
    }
    doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .push(pill);
    doc
}

fn clock_pill_document() -> Json {
    let mut doc = document();
    doc["preset"] = json!("Custom layout");
    doc["hosts"]["top_bar"]["mode"] = json!("content");
    doc["hosts"]["top_bar"]["instances"][1]["widget"] = json!("wifi");
    let mut clock = doc["hosts"]["top_bar"]["instances"][0].clone();
    clock["host"] = json!("status-pill");
    clock["slot"] = json!({"region":"centre","start":0,"span":2});
    doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let pill = json!({"iid":"status-pill","group":"status-pill","widget":"pill","host":"top_bar","style":"pill",
        "slot":{"region":"right","start":0,"span":6},"settings":{},"children":[clock]});
    doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .push(pill);
    doc
}

fn empty_clock_pill_document() -> Json {
    let mut doc = clock_pill_document();
    doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["widget"] == "pill")
        .unwrap()["children"] = json!([]);
    doc
}
fn pill_transfer(phase: &str, child: bool) -> Value {
    let (x, y) = if child { (300., 20.) } else { (1700., 20.) };
    Value::list(vec![
        json!({"phase":phase,"source":[{"iid":"network-main","group":"network-main","widget":"network",
        "host":"top_bar","form":"bar","style":"icon","settings":{"future":"keep"}}],
        "destination":"bar","x":x,"y":y,"global_x":x,"global_y":y,"start_x":96,"start_y":20,
        "width":64,"height":40,"grab_x":16,"grab_y":8,"screen_width":1920,"screen_height":1080,
        "target_origin_x":0,"target_origin_y":0,"context":pill_context()}),
    ])
}
fn desktop_catalogue_context() -> Json {
    let mut packages = catalogue().as_array().unwrap().clone();
    packages[0]["forms"]["desktop"] = json!({"default_style":"card","styles":{"card":{"min_span":120,"max_span":480,"preferred_span":240,"preferred_height":120}}});
    json!({"catalogue":packages,"screen_width":1920,"screen_height":1080,"snap_distance":16,
        "geometry":[{"iid":"clock-main","x":0,"y":0,"w":64,"h":40}],
        "reservations":[{"iid":"clock-main","width":64}]})
}
fn desktop_transfer(destination: &str, form: &str, x: f64) -> Value {
    Value::list(vec![
        json!({"phase":"drop","source":[{"iid":"clock-main","group":"clock-main","widget":"clock",
        "host":if form=="desktop" {"canvas"} else {"top_bar"},"form":form,"style":if form=="desktop" {"card"} else {"time"},
        "settings":{"future":"keep"}}],"destination":destination,"x":x,"y":200,"global_x":x,"global_y":200,
        "screen_width":1920,"screen_height":1080,"width":64,"height":40,"grab_x":16,"grab_y":8,
        "target_origin_x":0,"target_origin_y":0,"context":desktop_catalogue_context()}),
    ])
}
fn default_widget_fixture() -> Json {
    let nested = |iid: &str, widget: &str, region: &str, start: u32, settings: Json| {
        json!({"iid":iid,"widget":widget,"host":"status-pill",
            "slot":{"region":region,"start":start,"span":1},"settings":settings})
    };
    let clock = nested(
        "clock",
        "clock",
        "right",
        0,
        json!({"clock_fixture_sentinel":"keep-clock-setting"}),
    );
    json!({
        "version":1,"preset":"Nacre default",
        "owner_fixture_sentinel":{"tag":"widgets-drag-capture","version":1},
        "widgets":{
            "clock":{"widget":"clock","settings":{"clock_fixture_sentinel":"keep-clock-setting"},"forms":{
                "bar":{"enabled":true,"style":"time","placement":{"host":"status-pill","slot":{"region":"right","start":0,"span":1}}},
                "desktop":{"enabled":false,"style":"time","placement":{"host":"canvas","slot":{"x":320,"y":1080,"w":220,"h":120}}}}},
            "launcher":{"widget":"launcher","settings":{"icon":"nacre","custom_path":"","action":"launcher","route":""},"forms":{
                "bar":{"enabled":true,"style":"nacre","placement":{"host":"top_bar","slot":{"region":"left","start":0,"span":1}}},
                "desktop":{"enabled":false,"style":"","placement":{}}}}
        },
        "hosts":{"top_bar":{"mode":"content","tiles":60,"regions":{
            "left":{"start":0,"span":60},"centre":{"start":0,"span":60},"right":{"start":0,"span":60}},
            "instances":[
                {"iid":"launcher","widget":"launcher","host":"top_bar","slot":{"region":"left","start":0,"span":1},"settings":{"icon":"nacre","custom_path":"","action":"launcher","route":""}},
                {"iid":"workspaces","widget":"workspaces","host":"top_bar","slot":{"region":"left","start":1,"span":1},"settings":{}},
                {"iid":"title","widget":"title","host":"top_bar","slot":{"region":"centre","start":0,"span":1},"settings":{}},
                {"iid":"status-pill","widget":"pill","host":"top_bar","slot":{"region":"right","start":0,"span":1},"settings":{},"children":[
                    {"iid":"updates","widget":"updates","host":"status-pill","slot":{"region":"left","start":0,"span":1},"settings":{}},
                    {"iid":"sound","widget":"sound","host":"status-pill","slot":{"region":"centre","start":0,"span":1},"settings":{}},
                    {"iid":"wifi","widget":"wifi","host":"status-pill","slot":{"region":"centre","start":1,"span":1},"settings":{}},
                    {"iid":"bluetooth","widget":"bluetooth","host":"status-pill","slot":{"region":"centre","start":2,"span":1},"settings":{}},
                    {"iid":"battery","widget":"battery","host":"status-pill","slot":{"region":"centre","start":3,"span":1},"settings":{}},
                    {"iid":"notifications","widget":"notifications","host":"status-pill","slot":{"region":"centre","start":4,"span":1},"settings":{}},
                    clock,
                    {"iid":"power","widget":"power","host":"status-pill","slot":{"region":"right","start":1,"span":1},"settings":{}}
                ]}
            ]}}
    })
}
fn default_widget_context() -> Json {
    let mut packages = catalogue().as_array().unwrap().clone();
    packages[0]["forms"]["desktop"] = json!({"default_style":"time","styles":{"time":{"min_span":120,"max_span":480,"preferred_span":220,"preferred_height":120}}});
    packages.push(json!({"id":"pill","defaults":{},"forms":{"bar":{"default_style":"pill","styles":{"pill":{"min_span":1,"max_span":60,"preferred_span":1}}}}}));
    json!({"catalogue":packages,"canvas_width":1920,"canvas_height":1080,"screen_width":1920,"screen_height":1080,
        "geometry":[{"iid":"status-pill","x":1600,"y":0,"w":288,"h":34},{"iid":"clock","x":1812,"y":0,"w":64,"h":34}],
        "reservations":[{"iid":"status-pill","width":288},{"iid":"clock","width":64}]})
}
fn default_widget_transfer(phase: &str) -> Value {
    Value::list(vec![
        json!({"phase":phase,"source":[{"iid":"clock","group":"clock","widget":"clock",
        "host":"top_bar","form":"bar","style":"time","settings":{"clock_fixture_sentinel":"keep-clock-setting"}}],
        "destination":"desktop","destination_x":320,"destination_y":1104,"x":320,"y":1104,
        "global_x":320,"global_y":1104,"output_origin_x":0,"output_origin_y":0,
        "screen_width":1920,"screen_height":1200,"width":64,"height":40,"grab_x":16,"grab_y":8,
        "target_origin_x":0,"target_origin_y":0,"context":default_widget_context()}),
    ])
}
fn default_widget_bar_return_transfer_at(phase: &str, x: f64) -> Value {
    let mut event = json!({
        "phase":phase,"source":[{"iid":"clock","group":"clock","widget":"clock",
        "host":"canvas","form":"desktop","style":"time","settings":{"clock_fixture_sentinel":"keep-clock-setting"}}],
        "destination":"bar","x":x,"y":20,"global_x":x,"global_y":20,
        "screen_width":1920,"screen_height":1200,"width":220,"height":120,"grab_x":16,"grab_y":8,
        "target_origin_x":0,"target_origin_y":0,"context":default_widget_context()
    });
    event["context"]["geometry"][0]["x"] = json!(1600);
    event["context"]["geometry"][0]["w"] = json!(352);
    event["context"]["reservations"][0]["width"] = json!(352);
    Value::list(vec![event])
}
fn default_widget_bar_return_transfer(phase: &str) -> Value {
    default_widget_bar_return_transfer_at(phase, 400.)
}
#[test]
fn desktop_clock_dropped_on_occupied_pill_slot_writes_at_82_and_90_percent() {
    for x in [1920. * 0.82, 1920. * 0.90] {
        let mut original = default_widget_fixture();
        let pill = &mut original["hosts"]["top_bar"]["instances"][3];
        pill["children"]
            .as_array_mut()
            .unwrap()
            .retain(|child| child["iid"] != "clock");
        pill["slot"]["span"] = json!(11);
        original["hosts"]["canvas"] = json!({"width":1920,"height":1200,"instances":[{
            "iid":"clock","group":"clock","widget":"clock","host":"canvas","style":"time",
            "settings":{"clock_fixture_sentinel":"keep-clock-setting"},
            "slot":{"x":1700,"y":1080,"w":220,"h":120}
        }]});
        original["widgets"]["clock"]["forms"]["bar"]["enabled"] = json!(false);
        original["widgets"]["clock"]["forms"]["bar"]["placement"]["slot"]["span"] = json!(3);
        original["widgets"]["clock"]["forms"]["desktop"]["enabled"] = json!(true);

        let mut controller = Controller::new().unwrap();
        controller
            .sync(&BTreeMap::from([
                ("widgets.layout".into(), Value::list(vec![original])),
                ("widgets.screen_width".into(), Value::Number(1920.)),
                ("widgets.screen_height".into(), Value::Number(1200.)),
            ]))
            .unwrap();
        controller
            .drag(default_widget_bar_return_transfer_at("move", x))
            .unwrap();
        let dropped = controller
            .drag(default_widget_bar_return_transfer_at("drop", x))
            .unwrap();
        assert!(
            matches!(&dropped["widgets.write"],Value::List(rows) if rows.len()==1),
            "clock drop at {x}px must commit instead of rejecting the occupied pill slot: {dropped:?}"
        );
        let written = doc_from(&dropped, "widgets.write");
        let pill = written["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["iid"] == "status-pill")
            .unwrap();
        assert!(
            pill["children"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["iid"] == "clock")
        );
        assert!(pill["slot"]["span"].as_f64().unwrap() >= 13.);
        assert!(
            written["hosts"]["canvas"]["instances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
fn assert_unique_iids(document: &Json) {
    fn visit(items: &[Json], seen: &mut HashSet<String>) {
        for item in items {
            let iid = item["iid"]
                .as_str()
                .expect("fixture item has a stable IID")
                .to_owned();
            assert!(
                seen.insert(iid.clone()),
                "duplicate IID in widget document: {iid}"
            );
            if let Some(children) = item["children"].as_array() {
                visit(children, seen);
            }
        }
    }
    let mut seen = HashSet::new();
    for host in document["hosts"].as_object().unwrap().values() {
        if let Some(items) = host["instances"].as_array() {
            visit(items, &mut seen);
        }
    }
}
fn event(phase: &str, x: f64) -> Value {
    Value::list(vec![
        json!({"phase":phase,"source":[{"iid":"clock-main","group":"clock-main","widget":"clock","host":"top_bar","form":"bar","style":"time","settings":{"future":"kept"}}],
        "destination":"bar","x":x,"y":20,"global_x":x,"global_y":20,"start_x":64,"start_y":20,
        "width":64,"height":40,"grab_x":16,"grab_y":8,"screen_width":1920,"screen_height":1080,
        "target_origin_x":0,"target_origin_y":0,"context":context()}),
    ])
}
fn doc_from(values: &BTreeMap<String, Value>, key: &str) -> Json {
    let Value::List(rows) = &values[key] else {
        panic!("{key} is a document list")
    };
    rows.first().cloned().expect("document is present")
}

#[test]
fn nested_bar_style_growth_reflows_children_and_reserved_pill() {
    let original = bar_style_document(1, 1, 2, 10, true);
    let mut controller = style_controller(&original);
    let values = apply_bar_style(&mut controller, &original, "icon+value");
    let updated = doc_from(&values, "widgets.draft");
    assert_unique_iids(&updated);
    let items = updated["hosts"]["top_bar"]["instances"].as_array().unwrap();
    let pill = items
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    assert_eq!(
        pill["slot"]["span"], 3,
        "pill reservation grows with its child"
    );
    let children = pill["children"].as_array().unwrap();
    let notifications = children
        .iter()
        .find(|item| item["iid"] == "notifications")
        .unwrap();
    let wifi = children.iter().find(|item| item["iid"] == "wifi").unwrap();
    assert_eq!(notifications["style"], "icon+value");
    assert_eq!(notifications["slot"]["span"], 2);
    assert_eq!(
        wifi["slot"]["start"], 2,
        "following pill child shifts out of the grown slot"
    );
    let neighbor = items
        .iter()
        .find(|item| item["iid"] == "bar-neighbor")
        .unwrap();
    assert_eq!(
        neighbor["slot"]["start"], 3,
        "following bar root shifts with the pill reservation"
    );
    let placement = &updated["widgets"]["notifications"]["forms"]["bar"]["placement"];
    assert_eq!(placement["slot"]["span"], 2);
    assert_eq!(placement["future_placement"], "keep");
    assert_eq!(placement["slot"]["future_slot"], "keep");
}

#[test]
fn nested_style_change_preserves_a_larger_explicit_bar_span() {
    let original = bar_style_document(4, 4, 5, 10, true);
    let mut controller = style_controller(&original);
    let values = apply_bar_style(&mut controller, &original, "icon+value");
    let updated = doc_from(&values, "widgets.draft");
    let items = updated["hosts"]["top_bar"]["instances"].as_array().unwrap();
    let pill = items
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    let child = pill["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["iid"] == "notifications")
        .unwrap();
    assert_eq!(child["slot"]["span"], 4);
    assert_eq!(pill["slot"]["span"], 5);
    assert_eq!(
        items
            .iter()
            .find(|item| item["iid"] == "bar-neighbor")
            .unwrap()["slot"]["start"],
        5
    );
    assert_eq!(
        updated["widgets"]["notifications"]["forms"]["bar"]["placement"]["slot"]["span"],
        4
    );
}

#[test]
fn disabled_bar_style_change_grows_its_saved_preferred_span() {
    let original = bar_style_document(1, 1, 2, 10, false);
    let mut controller = style_controller(&original);
    let values = apply_bar_style(&mut controller, &original, "icon+value");
    let updated = doc_from(&values, "widgets.draft");
    let form = &updated["widgets"]["notifications"]["forms"]["bar"];
    assert_eq!(form["enabled"], false);
    assert_eq!(form["style"], "icon+value");
    assert_eq!(form["placement"]["slot"]["span"], 2);
    assert_eq!(form["placement"]["future_placement"], "keep");
    assert_eq!(form["placement"]["slot"]["future_slot"], "keep");
}

#[test]
fn failed_nested_style_growth_keeps_the_canonical_layout_unchanged() {
    let original = bar_style_document(1, 1, 4, 5, true);
    let mut controller = style_controller(&original);
    let values = apply_bar_style(&mut controller, &original, "icon+value");
    assert_eq!(
        values.get("widgets.error"),
        Some(&Value::Text("bar has no room for this style".into()))
    );
    assert_eq!(
        values.get("widgets.layout"),
        Some(&Value::list(vec![original]))
    );
    assert_eq!(values.get("widgets.draft"), Some(&Value::list(Vec::new())));
    assert_eq!(values.get("widgets.write"), Some(&Value::list(Vec::new())));
}

#[test]
fn private_drag_feedback_ignores_other_pid_and_stale_epoch() {
    assert!(super::widget_drag_feedback_matches(41, Some(8), 41, 8));
    assert!(!super::widget_drag_feedback_matches(41, Some(8), 42, 8));
    assert!(!super::widget_drag_feedback_matches(41, Some(9), 41, 8));
    assert!(!super::widget_drag_feedback_matches(41, None, 41, 8));
}

#[test]
fn parent_history_feedback_accepts_topic_before_or_after_and_rejects_stale_revisions() {
    let previous = json!({"version":1,"hosts":{"top_bar":{"instances":[]}}});
    let accepted = json!({"version":1,"hosts":{"top_bar":{"instances":[{"iid":"from-parent"}]}}});
    let newer = json!({"version":1,"hosts":{"top_bar":{"instances":[{"iid":"newer-edit"}]}}});
    let lagging = json!({"version":1,"hosts":{"top_bar":{"instances":[{"iid":"old-projection"}]}}});
    assert!(
        super::widget_history_feedback_matches(2, 0, 4, 5, Some(&lagging), &previous, &accepted),
        "a projection older than the native write advances to its authoritative feedback"
    );
    assert!(
        !super::widget_history_feedback_matches(2, 5, 4, 5, Some(&lagging), &previous, &accepted),
        "a lagging projection still cannot append duplicate history"
    );
    assert!(
        super::widget_history_feedback_matches(4, 0, 4, 5, Some(&previous), &previous, &accepted),
        "feedback arriving before topic publication matches the old canonical snapshot"
    );
    assert!(
        super::widget_history_feedback_matches(5, 0, 4, 5, Some(&accepted), &previous, &accepted),
        "feedback arriving after topic publication matches the accepted snapshot"
    );
    assert!(
        !super::widget_history_feedback_matches(6, 0, 4, 5, Some(&newer), &previous, &accepted),
        "a newer revision must not be overwritten"
    );
    assert!(
        !super::widget_history_feedback_matches(5, 5, 4, 5, Some(&accepted), &previous, &accepted),
        "duplicate operation revisions must not append history twice"
    );
    assert!(
        !super::widget_history_feedback_matches(4, 0, 4, 5, Some(&newer), &previous, &accepted),
        "same revision with different content is stale"
    );
}

#[test]
fn first_parent_remove_uses_unpersisted_default_for_undo_in_either_topic_order() {
    let default = json!({"version":1,"hosts":{"top_bar":{"instances":[
        {"iid":"launcher-main","widget":"launcher","host":"top_bar","slot":{"region":"left","start":0,"span":2}}
    ]}}});
    let removed = json!({"version":1,"hosts":{"top_bar":{"instances":[]}}});
    let seed = json!({"widget_layout":true,"document":default,"initial":default});
    let values = BTreeMap::from([
        ("widgets.layout".into(), Value::list(Vec::new())),
        ("widgets.initial".into(), Value::list(vec![default.clone()])),
        ("widgets.rev".into(), Value::Number(0.)),
    ]);
    let previous = super::widget_previous_document(&values, Some(&seed))
        .expect("default is the revision-zero source");
    assert_eq!(previous, default);

    let before =
        super::widget_history_current_document(None, Some(default.clone()), Some(&seed), 0, 0);
    assert!(
        super::widget_history_feedback_matches(0, 0, 0, 1, before.as_ref(), &previous, &removed),
        "feedback before publication compares against the known initial document"
    );
    let after = super::widget_history_current_document(
        Some(removed.clone()),
        Some(default.clone()),
        Some(&seed),
        1,
        0,
    );
    assert!(
        super::widget_history_feedback_matches(1, 0, 0, 1, after.as_ref(), &previous, &removed),
        "feedback after publication compares against the accepted canonical document"
    );
    assert!(
        super::widget_history_current_document(None, Some(default.clone()), Some(&seed), 1, 0)
            .is_none(),
        "a missing layout at a later revision must not fall back to stale defaults"
    );

    let mut controller = Controller::new().unwrap();
    controller.sync(&values).unwrap();
    let committed = controller
        .apply_parent_history(previous.clone(), removed.clone(), 1)
        .unwrap();
    let Value::List(undo) = &committed["widgets.undo"] else {
        panic!("history is a list")
    };
    assert_eq!(undo.len(), 1);
    assert_eq!(undo[0]["document"], default);
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![removed.clone()])),
            ("widgets.rev".into(), Value::Number(1.)),
        ]))
        .unwrap();
    let undone = controller
        .edit(Value::list(vec![json!({"op":"undo","document":removed})]))
        .unwrap();
    assert_eq!(
        doc_from(&undone, "widgets.draft"),
        previous,
        "the first parent-owned remove can be undone back to the unpersisted default"
    );
}

#[test]
fn parent_drag_or_native_edit_feedback_adds_one_undo_before_or_after_topic() {
    let previous =
        json!({"version":1,"hosts":{"top_bar":{"instances":[]}},"unknown":{"kept":true}});
    let accepted = json!({"version":1,"hosts":{"top_bar":{"instances":[{"iid":"parent-edit"}]}},"unknown":{"kept":true}});
    let mut before = Controller::new().unwrap();
    before
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![previous.clone()])),
            ("widgets.rev".into(), Value::Number(4.)),
        ]))
        .unwrap();
    let committed = before
        .apply_parent_history(previous.clone(), accepted.clone(), 5)
        .unwrap();
    assert_eq!(doc_from(&committed, "widgets.layout"), accepted);
    let Value::List(undo) = &committed["widgets.undo"] else {
        panic!("history is a list")
    };
    assert_eq!(undo.len(), 1);
    assert_eq!(undo[0]["document"], previous);

    let mut after = Controller::new().unwrap();
    after
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![accepted.clone()])),
            ("widgets.rev".into(), Value::Number(5.)),
        ]))
        .unwrap();
    after
        .apply_parent_history(previous.clone(), accepted.clone(), 5)
        .unwrap();
    after
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![accepted.clone()])),
            ("widgets.rev".into(), Value::Number(5.)),
        ]))
        .unwrap();
    let committed = after
        .edit(Value::list(vec![json!({"op":"select","widget":"clock"})]))
        .unwrap();
    let Value::List(undo) = &committed["widgets.undo"] else {
        panic!("history is a list")
    };
    assert_eq!(
        undo.len(),
        1,
        "topic publication after feedback preserves the transported Undo snapshot"
    );
    assert_eq!(undo[0]["document"], previous);
}

#[test]
fn floating_policy_targets_the_unique_active_widgets_window_only_once() {
    let pid = 51;
    let generation = 7;
    let window_id = bead::backends::Id("niri:12:window:44".into());
    let mut capabilities = bead::backends::Capabilities::empty();
    capabilities
        .features
        .insert("floatingPlacement".into(), true);
    let backend = bead::backends::Snapshot {
        connected: true,
        capabilities,
        windows: vec![bead::backends::Window {
            id: window_id.clone(),
            title: "Widgets".into(),
            app_id: "nacre-widgets".into(),
            workspace: None,
            output: None,
            focused: false,
            floating: false,
            fullscreen: None,
            urgent: false,
        }],
        ..bead::backends::Snapshot::default()
    };
    let compositor = bead::core::source::Snapshot {
        epoch: "fake".into(),
        revision: 1,
        connected: true,
        data: std::sync::Arc::new(
            json!({"available":true,"windows":{"44":{"id":44,"pid":pid,"appId":"nacre-widgets"}}}),
        ),
    };
    let mut request = Some(super::WidgetFloatingRequest {
        pid,
        generation,
        enabled: true,
        attempted: false,
    });
    let mut floating = false;
    let mut actions = 0;
    let changed = super::apply_widget_floating(
        &mut request,
        Some((pid, generation)),
        &backend,
        &compositor,
        |action| {
            actions += 1;
            match action {
                bead::backends::Action::SetFloating {
                    window,
                    floating: value,
                } => {
                    assert_eq!(window, window_id);
                    floating = value;
                    Ok(())
                }
                other => panic!("unexpected action: {other:?}"),
            }
        },
    );
    assert!(changed);
    assert!(floating);
    assert_eq!(actions, 1);
    assert!(!super::apply_widget_floating(
        &mut request,
        Some((pid, generation)),
        &backend,
        &compositor,
        |_| {
            actions += 1;
            Ok(())
        }
    ));
    assert_eq!(
        actions, 1,
        "a later retile or backend snapshot does not reapply the policy"
    );

    let mut stale = Some(super::WidgetFloatingRequest {
        pid,
        generation,
        enabled: true,
        attempted: false,
    });
    assert!(!super::apply_widget_floating(
        &mut stale,
        Some((pid, generation + 1)),
        &backend,
        &compositor,
        |_| {
            actions += 1;
            Ok(())
        }
    ));
    let mut closed = None;
    assert!(!super::apply_widget_floating(
        &mut closed,
        None,
        &backend,
        &compositor,
        |_| {
            actions += 1;
            Ok(())
        }
    ));
    assert_eq!(
        actions, 1,
        "stale generations and closed app state do not affect any window"
    );
}

#[test]
fn parent_runtime_policy_tracks_open_and_matching_close_generations() {
    let dir = std::env::temp_dir().join(format!("widget-float-runtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    let state = |pid: u32, generation: u64, open: bool| {
        BTreeMap::from([
            ("widgets.app.open".into(), Value::Bool(open)),
            ("widgets.editing".into(), Value::Bool(open)),
            ("widgets.policy.pid".into(), Value::Number(f64::from(pid))),
            (
                "widgets.policy.generation".into(),
                Value::Number(generation as f64),
            ),
            ("widgets.policy.open".into(), Value::Bool(open)),
            ("widgets.policy.floating".into(), Value::Bool(open)),
        ])
    };
    runtime.stage(UiEvent::Model(state(51, 7, true))).unwrap();
    assert!(
        runtime
            .widget_floating_request
            .is_some_and(|request| request.pid == 51
                && request.generation == 7
                && request.enabled
                && !request.attempted)
    );
    runtime.stage(UiEvent::Model(state(51, 6, false))).unwrap();
    assert!(
        runtime
            .widget_floating_request
            .is_some_and(|request| request.generation == 7),
        "a stale close cannot clear the current parent Runtime request"
    );
    runtime.stage(UiEvent::Model(state(51, 7, false))).unwrap();
    assert!(runtime.widget_floating_request.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn motion_reflows_neighbors_without_write_and_drop_commits_once_from_frozen_base() {
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![document()])),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let first = controller.drag(event("move", 96.)).unwrap();
    let preview = doc_from(&first, "widgets.drag.layout");
    assert_unique_iids(&preview);
    assert_eq!(preview["hosts"]["top_bar"]["mode"], "tiles");
    let instances = preview["hosts"]["top_bar"]["instances"].as_array().unwrap();
    assert!(instances.iter().any(|item| item["iid"] == "clock-main"));
    assert!(
        instances
            .iter()
            .any(|item| item["drag_placeholder"] == true)
    );
    assert_eq!(
        instances
            .iter()
            .find(|item| item["iid"] == "network-main")
            .unwrap()["slot"]["start"],
        5
    );
    assert!(matches!(&first["widgets.write"],Value::List(rows) if rows.is_empty()));
    assert!(matches!(&first["widgets.undo"],Value::List(rows) if rows.is_empty()));

    let second = controller.drag(event("move", 128.)).unwrap();
    let second_doc = doc_from(&second, "widgets.drag.layout");
    assert_unique_iids(&second_doc);
    assert_eq!(
        second_doc["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["iid"] == "network-main")
            .unwrap()["slot"]["start"],
        2,
        "the retained source placeholder occupies its slot while neighbors reflow from the gesture base"
    );
    let mut fresh = Controller::new().unwrap();
    fresh
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![document()])),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let direct = fresh.drag(event("move", 128.)).unwrap();
    assert_unique_iids(&doc_from(&direct, "widgets.drag.layout"));
    assert_eq!(
        second_doc,
        doc_from(&direct, "widgets.drag.layout"),
        "motion path must not accumulate edits"
    );
    let epoch = controller.drag_epoch();
    let mut drop = event("drop", 128.);
    let Value::List(records) = &mut drop else {
        unreachable!()
    };
    std::sync::Arc::make_mut(records)[0]["phase"] = "drop".into();
    let committed = controller.drag(drop).unwrap();
    let written = doc_from(&committed, "widgets.write");
    assert_unique_iids(&written);
    assert_eq!(written["hosts"]["top_bar"]["mode"], "tiles");
    assert!(
        !written["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["drag_placeholder"] == true)
    );
    assert!(matches!(&committed["widgets.undo"],Value::List(rows) if rows.len()==1));

    let _new_gesture = controller.drag(event("move", 96.)).unwrap();
    let new_epoch = controller.drag_epoch();
    assert!(new_epoch > epoch);
    let stale_retire = Value::list(vec![json!({"phase":"retire","epoch":epoch})]);
    let still_active = controller.drag(stale_retire).unwrap();
    assert_eq!(
        still_active.get("widgets.drag.active"),
        Some(&Value::Bool(true))
    );
    let retire = Value::list(vec![json!({"phase":"retire","epoch":new_epoch})]);
    let retired = controller.drag(retire).unwrap();
    assert_eq!(
        retired.get("widgets.drag.active"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn drag_into_pill_preserves_item_data_and_writes_only_on_drop() {
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![pill_document(false)]),
            ),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let preview = controller.drag(pill_transfer("move", false)).unwrap();
    assert!(matches!(&preview["widgets.write"],Value::List(rows) if rows.is_empty()));
    let moved = controller.drag(pill_transfer("drop", false)).unwrap();
    let written = doc_from(&moved, "widgets.write");
    let pill = written["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    let child = pill["children"][0].clone();
    assert_eq!(child["iid"], "network-main");
    assert_eq!(child["host"], "status-pill");
    assert_eq!(child["settings"]["future"], "keep");
    assert_eq!(child["opaque"]["nested"][1], "preserved");
    assert_eq!(pill["future_field"]["keep"], 7);
    assert!(pill["slot"]["span"].as_f64().unwrap() >= 8.);
    assert_eq!(
        written["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["iid"] == "network-main")
            .count(),
        0
    );
    assert!(matches!(&moved["widgets.write"],Value::List(rows) if rows.len()==1));
    assert!(matches!(&moved["widgets.undo"],Value::List(rows) if rows.len()==1));
}

#[test]
fn default_eight_child_pill_drag_to_desktop_keeps_unique_ids_and_real_output_bounds() {
    let original = default_widget_fixture();
    assert_eq!(
        original["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        original["hosts"]["top_bar"]["instances"][3]["children"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![original.clone()])),
            ("widgets.rev".into(), Value::Number(4.)),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1200.)),
        ]))
        .unwrap();

    let Value::List(move_events) = default_widget_transfer("move") else {
        unreachable!()
    };
    let mut move_events = move_events.as_ref().clone();
    move_events[0]["source_origin_valid"] = json!(true);
    let preview = controller.drag(Value::list(move_events)).unwrap();
    assert!(matches!(&preview["widgets.write"],Value::List(rows) if rows.is_empty()));
    assert_eq!(preview["widgets.rev"], Value::Number(4.));
    let preview_doc = doc_from(&preview, "widgets.drag.layout");
    assert_unique_iids(&preview_doc);

    let Value::List(drop_events) = default_widget_transfer("drop") else {
        unreachable!()
    };
    let mut drop_events = drop_events.as_ref().clone();
    drop_events[0]["source_origin_valid"] = json!(true);
    let dropped = controller.drag(Value::list(drop_events)).unwrap();
    assert_eq!(dropped["widgets.rev"], Value::Number(4.));
    let written = doc_from(&dropped, "widgets.write");
    assert_unique_iids(&written);
    let top_items = written["hosts"]["top_bar"]["instances"].as_array().unwrap();
    assert_eq!(
        top_items.len(),
        4,
        "resizing the existing pill must not append it again"
    );
    let pills: Vec<_> = top_items
        .iter()
        .filter(|item| item["widget"] == "pill")
        .collect();
    assert_eq!(pills.len(), 1, "the status pill IID remains unique");
    assert_eq!(pills[0]["slot"]["span"], 7);
    let children = pills[0]["children"].as_array().unwrap();
    assert_eq!(children.len(), 7);
    assert!(!children.iter().any(|item| item["widget"] == "clock"));
    let canvas = &written["hosts"]["canvas"];
    assert_eq!(canvas["width"], 1920);
    assert_eq!(
        canvas["height"], 1200,
        "event output height wins over stale 1080p seed metadata"
    );
    let clocks = canvas["instances"].as_array().unwrap();
    assert_eq!(clocks.len(), 1);
    assert_eq!(clocks[0]["widget"], "clock");
    assert_eq!(clocks[0]["slot"]["y"], 1080);
    assert_eq!(clocks[0]["slot"]["h"], 120);
    assert_eq!(
        clocks[0]["settings"]["clock_fixture_sentinel"],
        "keep-clock-setting"
    );
    assert_eq!(written["owner_fixture_sentinel"]["version"], 1);
    assert!(matches!(&dropped["widgets.undo"],Value::List(rows) if rows.len()==1));
    assert!(matches!(&dropped["widgets.write"],Value::List(rows) if rows.len()==1));
}

#[test]
fn default_clock_returns_with_geometry_materialized_bar_span() {
    let mut original = default_widget_fixture();
    original["hosts"]["top_bar"]["instances"][1]["group"] = json!("opaque-workspaces-group");
    original["widgets"]["opaque-workspaces-group"] = json!({"forms":17,"future":[1,{"keep":true}]});
    original["widgets"]["clock"]["forms"]["bar"]["placement"]["future_placement"] =
        json!("preserved");
    original["widgets"]["clock"]["forms"]["bar"]["placement"]["slot"]["future_slot"] =
        json!("preserved");
    let mut context = default_widget_context();
    context["geometry"][1]["w"] = json!(68);
    context["reservations"][1]["width"] = json!(68);
    let mut to_desktop = default_widget_transfer("drop");
    let Value::List(events) = &mut to_desktop else {
        unreachable!()
    };
    let events = std::sync::Arc::make_mut(events);
    events[0]["context"] = context;
    events[0]["source_origin_valid"] = json!(true);

    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![original])),
            ("widgets.rev".into(), Value::Number(4.)),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1200.)),
        ]))
        .unwrap();
    let desktop = controller.drag(to_desktop).unwrap();
    assert!(
        matches!(&desktop["widgets.write"],Value::List(rows) if rows.len()==1),
        "desktop conversion rejected: error={:?}, message={:?}, phase={:?}",
        desktop.get("widgets.error"),
        desktop.get("widgets.message"),
        desktop.get("widgets.drag.phase")
    );
    let desktop_doc = doc_from(&desktop, "widgets.write");
    assert_eq!(
        desktop_doc["widgets"]["clock"]["forms"]["bar"]["placement"]["slot"]["span"], 3,
        "content-mode geometry and reservation materialize the clock's bar span"
    );
    assert_eq!(
        desktop_doc["widgets"]["clock"]["forms"]["bar"]["placement"]["future_placement"],
        "preserved"
    );
    assert_eq!(
        desktop_doc["widgets"]["clock"]["forms"]["bar"]["placement"]["slot"]["future_slot"],
        "preserved"
    );
    assert_eq!(
        desktop_doc["widgets"]["opaque-workspaces-group"]["future"][1]["keep"],
        true
    );
    assert_unique_iids(&desktop_doc);

    controller
        .sync(&BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![desktop_doc.clone()]),
            ),
            ("widgets.draft".into(), Value::list(Vec::<Json>::new())),
            ("widgets.rev".into(), Value::Number(5.)),
        ]))
        .unwrap();
    controller
        .drag(default_widget_bar_return_transfer("move"))
        .unwrap();
    let returned = controller
        .drag(default_widget_bar_return_transfer("drop"))
        .unwrap();
    assert!(
        matches!(&returned["widgets.write"],Value::List(rows) if rows.len()==1),
        "bar return rejected: error={:?}, message={:?}, phase={:?}, layout={:?}",
        returned.get("widgets.error"),
        returned.get("widgets.message"),
        returned.get("widgets.drag.phase"),
        returned.get("widgets.drag.layout")
    );
    let returned_doc = doc_from(&returned, "widgets.write");
    assert_unique_iids(&returned_doc);
    let bar_items = returned_doc["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap();
    let clock = bar_items
        .iter()
        .find(|item| item["iid"] == "clock")
        .expect("clock returns as a top-level bar widget");
    assert_eq!(clock["host"], "top_bar");
    assert_eq!(clock["slot"]["span"], 3);
    assert_eq!(
        clock["settings"]["clock_fixture_sentinel"],
        "keep-clock-setting"
    );
    let pill = bar_items
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    assert!(
        !pill["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock")
    );
    assert_eq!(
        returned_doc["widgets"]["clock"]["settings"]["clock_fixture_sentinel"],
        "keep-clock-setting"
    );
    assert_eq!(
        returned_doc["widgets"]["opaque-workspaces-group"]["future"][1]["keep"],
        true
    );
}

#[test]
fn drag_out_of_pill_restores_top_level_and_preserves_opaque_settings() {
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![pill_document(true)]),
            ),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let preview = controller.drag(pill_transfer("move", true)).unwrap();
    assert!(matches!(&preview["widgets.write"],Value::List(rows) if rows.is_empty()));
    let moved = controller.drag(pill_transfer("drop", true)).unwrap();
    let written = doc_from(&moved, "widgets.write");
    let instances = written["hosts"]["top_bar"]["instances"].as_array().unwrap();
    let child = instances
        .iter()
        .find(|item| item["iid"] == "network-main")
        .unwrap();
    let pill = instances
        .iter()
        .find(|item| item["iid"] == "status-pill")
        .unwrap();
    assert_eq!(child["host"], "top_bar");
    assert_eq!(child["settings"]["future"], "keep");
    assert_eq!(child["opaque"]["nested"][1], "preserved");
    assert!(pill["children"].as_array().unwrap().is_empty());
    assert_eq!(pill["slot"]["span"].as_f64().unwrap(), 4.);
    assert!(matches!(&moved["widgets.write"],Value::List(rows) if rows.len()==1));
    assert!(matches!(&moved["widgets.undo"],Value::List(rows) if rows.len()==1));
}

#[test]
fn configured_bar_span_survives_desktop_form_roundtrip() {
    let mut doc = document();
    doc["widgets"] = json!({"clock-main":{"widget":"clock","settings":{"future":"keep"},"forms":{
        "bar":{"enabled":true,"style":"time","placement":{"host":"top_bar","slot":{"region":"left","start":0,"span":4}}},
        "desktop":{"enabled":false,"style":"card","placement":{"host":"canvas","slot":{"x":240,"y":160,"w":240,"h":120}}}
    }}});
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![doc])),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let desktop = controller
        .drag(desktop_transfer("desktop", "bar", 500.))
        .unwrap();
    let desktop_doc = doc_from(&desktop, "widgets.write");
    let canvas = desktop_doc["hosts"]["canvas"]["instances"]
        .as_array()
        .unwrap();
    assert_eq!(canvas.len(), 1);
    assert_eq!(canvas[0]["iid"], "clock-main");
    assert_eq!(canvas[0]["slot"]["w"].as_f64().unwrap(), 240.);
    assert_eq!(canvas[0]["slot"]["h"].as_f64().unwrap(), 120.);
    assert_eq!(canvas[0]["settings"]["future"], "keep");
    assert!(
        !desktop_doc["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock-main")
    );

    // The host acknowledges the first write before the next pointer gesture.
    controller
        .sync(&BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![desktop_doc.clone()]),
            ),
            ("widgets.draft".into(), Value::list(Vec::<Json>::new())),
            ("widgets.rev".into(), Value::Number(1.)),
        ]))
        .unwrap();
    let Value::List(events) = desktop_transfer("bar", "desktop", 400.) else {
        unreachable!()
    };
    let mut start = events[0].clone();
    start["phase"] = json!("move");
    controller.drag(Value::list(vec![start])).unwrap();
    let back = controller
        .drag(desktop_transfer("bar", "desktop", 400.))
        .unwrap();
    let back_doc = doc_from(&back, "widgets.write");
    let clock = back_doc["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["iid"] == "clock-main")
        .unwrap();
    assert_eq!(clock["slot"]["span"].as_f64().unwrap(), 4.);
    assert_eq!(clock["settings"]["future"], "keep");
    assert!(
        !back_doc["hosts"]["canvas"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "clock-main")
    );
    assert!(matches!(&back["widgets.write"],Value::List(rows) if rows.len()==1));
}

#[test]
fn cancelled_desktop_drag_returns_to_its_desktop_host() {
    let mut doc = document();
    doc["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[
        {"iid":"clock-desktop","group":"clock-main","widget":"clock","host":"canvas","style":"card",
            "slot":{"x":100,"y":120,"w":240,"h":96},"settings":{"future":"keep"}}
    ]});
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![doc.clone()])),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    let source = json!({"iid":"clock-desktop","group":"clock-main","widget":"clock","host":"canvas","form":"desktop","style":"card","settings":{"future":"keep"}});
    let move_event = json!({"phase":"move","source":[source.clone()],"destination":"desktop","x":180,"y":200,"global_x":180,"global_y":200,
        "start_x":180,"start_y":200,"width":240,"height":96,"grab_x":12,"grab_y":8,
        "screen_width":1920,"screen_height":1080,"context":context()});
    controller.drag(Value::list(vec![move_event])).unwrap();
    let cancelled = controller
        .drag(Value::list(vec![
            json!({"phase":"cancel","source":[source]}),
        ]))
        .unwrap();
    assert_eq!(
        cancelled.get("widgets.drag.destination"),
        Some(&Value::Text("desktop".into()))
    );
    assert_eq!(
        cancelled.get("widgets.drag.phase"),
        Some(&Value::Text("return".into()))
    );
    assert_eq!(
        cancelled.get("widgets.drag.snap_x"),
        Some(&Value::Number(168.))
    );
    assert_eq!(
        doc_from(&cancelled, "widgets.drag.layout")["hosts"]["canvas"]["instances"][0]["slot"]["x"],
        100
    );
    assert!(matches!(&cancelled["widgets.write"],Value::List(rows) if rows.is_empty()));
}

#[test]
fn remote_drop_routes_to_only_the_target_output_controller() {
    let mut left = Controller::new().unwrap();
    let mut right = Controller::new().unwrap();
    for controller in [&mut left, &mut right] {
        controller
            .sync(&BTreeMap::from([
                ("widgets.layout".into(), Value::list(vec![document()])),
                ("widgets.screen_width".into(), Value::Number(1920.)),
                ("widgets.screen_height".into(), Value::Number(1080.)),
            ]))
            .unwrap();
    }
    let mut transfer = event("drop", 128.);
    let Value::List(records) = &mut transfer else {
        unreachable!()
    };
    std::sync::Arc::make_mut(records)[0]["private_remote"] = json!(true);
    std::sync::Arc::make_mut(records)[0]["target_output_id"] = json!("right");
    let mut writes = 0;
    for (output, controller) in [("left", &mut left), ("right", &mut right)] {
        if !widget_drag_targets_output(&transfer, Some(output)) {
            continue;
        }
        let values = controller.drag(transfer.clone()).unwrap();
        if matches!(&values["widgets.write"],Value::List(rows) if !rows.is_empty()) {
            writes += 1;
        }
    }
    assert_eq!(writes, 1);
}

#[test]
fn cancel_drops_transient_layout_and_never_writes() {
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![document()])),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    controller.drag(event("move", 96.)).unwrap();
    let cancelled = controller.drag(event("cancel", 96.)).unwrap();
    let layout = doc_from(&cancelled, "widgets.drag.layout");
    assert_eq!(doc_from(&cancelled, "widgets.layout"), document());
    assert_eq!(layout["hosts"]["top_bar"]["mode"], "tiles");
    assert!(matches!(&cancelled["widgets.write"],Value::List(rows) if rows.is_empty()));
    assert!(matches!(&cancelled["widgets.undo"],Value::List(rows) if rows.is_empty()));
}

#[test]
fn drag_rejects_a_canonical_revision_change_and_next_gesture_uses_new_document() {
    let original = document();
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![original])),
            ("widgets.draft".into(), Value::list(Vec::new())),
            ("widgets.rev".into(), Value::Number(4.)),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();

    let moving = controller.drag(event("move", 96.)).unwrap();
    assert_eq!(moving["widgets.drag.revision"], Value::Number(4.));
    assert!(matches!(&moving["widgets.write"],Value::List(rows) if rows.is_empty()));

    let mut external = document();
    external["external_revision"] = json!({"writer":"another output","rev":5});
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![external.clone()])),
            ("widgets.draft".into(), Value::list(Vec::new())),
            ("widgets.rev".into(), Value::Number(5.)),
        ]))
        .unwrap();
    let rejected = controller.drag(event("drop", 128.)).unwrap();
    assert_eq!(doc_from(&rejected, "widgets.layout"), external);
    assert_eq!(
        doc_from(&rejected, "widgets.drag.layout"),
        external,
        "the return preview follows the newer canonical document"
    );
    assert_eq!(rejected["widgets.rev"], Value::Number(5.));
    assert!(matches!(&rejected["widgets.draft"],Value::List(rows) if rows.is_empty()));
    assert!(matches!(&rejected["widgets.undo"],Value::List(rows) if rows.is_empty()));
    assert!(
        matches!(&rejected["widgets.write"],Value::List(rows) if rows.is_empty()),
        "a stale gesture does not write or add history"
    );
    assert!(
        matches!(&rejected["widgets.error"],Value::Text(message) if message.contains("changed during drag"))
    );

    controller.drag(event("move", 96.)).unwrap();
    assert!(controller.drag_epoch() > 0);
    let accepted = controller.drag(event("drop", 128.)).unwrap();
    let committed = doc_from(&accepted, "widgets.write");
    assert_eq!(
        committed["external_revision"],
        json!({"writer":"another output","rev":5}),
        "a new gesture starts from and preserves the latest canonical document"
    );
    assert!(matches!(&accepted["widgets.write"],Value::List(rows) if rows.len()==1));
}

#[test]
fn stale_cancel_uses_materialized_default_when_canonical_layout_is_null() {
    let default = document();
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![default.clone()])),
            ("widgets.initial".into(), Value::list(Vec::new())),
            ("widgets.draft".into(), Value::list(Vec::new())),
            ("widgets.rev".into(), Value::Number(7.)),
            ("widgets.screen_width".into(), Value::Number(1920.)),
            ("widgets.screen_height".into(), Value::Number(1080.)),
        ]))
        .unwrap();
    controller.drag(event("move", 96.)).unwrap();
    controller
        .sync(&BTreeMap::from([
            ("widgets.layout".into(), Value::list(Vec::new())),
            ("widgets.draft".into(), Value::list(Vec::new())),
            ("widgets.rev".into(), Value::Number(8.)),
        ]))
        .unwrap();
    let mut cancel = event("cancel", 96.);
    let Value::List(records) = &mut cancel else {
        unreachable!()
    };
    let event = std::sync::Arc::make_mut(records).first_mut().unwrap();
    event["context"]["document"] = default.clone();
    event["context"]["initial"] = default.clone();
    let cancelled = controller.drag(cancel).unwrap();
    assert!(
        matches!(&cancelled["widgets.layout"],Value::List(rows) if rows.is_empty()),
        "the canonical null remains null"
    );
    assert_eq!(cancelled["widgets.rev"], Value::Number(8.));
    assert_eq!(
        doc_from(&cancelled, "widgets.drag.layout"),
        default,
        "the return preview uses current materialized default context, not the stale gesture base"
    );
    assert!(matches!(&cancelled["widgets.write"],Value::List(rows) if rows.is_empty()));
    assert!(matches!(&cancelled["widgets.undo"],Value::List(rows) if rows.is_empty()));
}

fn real_bar_runtime(file: std::path::PathBuf) -> Runtime {
    real_bar_runtime_configured(file, false)
}

fn real_widget_canvas_publisher() -> Runtime {
    let shell = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = shell.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session = bead::ui::Session::new(
        shell,
        "widgets/editor/canvas.luau".into(),
        palette,
        colors,
        cold,
    );
    session.set_viewport(1920, 1080);
    let mut runtime = Runtime::new(session);
    runtime.session.restore_on_start(BTreeMap::new());
    runtime = runtime.with_widget_editor(None, true).unwrap();
    runtime.state_file = None;
    runtime.widgets_file = None;
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("the canvas publisher loads its declared source schema");
    runtime.commit();
    runtime
}

fn real_bar_runtime_with_popouts(
    file: std::path::PathBuf,
) -> (Runtime, std::sync::Arc<std::sync::Mutex<Vec<UiEvent>>>) {
    let mut runtime = real_bar_runtime_configured(file, true);
    let routes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = routes.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    (runtime, routes)
}

fn real_bar_runtime_configured(file: std::path::PathBuf, popouts: bool) -> Runtime {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session = bead::ui::Session::new(root, "bar/main.luau".into(), palette, colors, cold);
    session.set_viewport(1920, 1080);
    let mut stores = crate::desktop::values(&bead::services::Snapshot::default(), "");
    stores.extend([
        ("bead.focused_title".into(), Value::Text(String::new())),
        ("bead.windows".into(), Value::list(Vec::new())),
        ("bead.workspaces".into(), Value::list(Vec::new())),
    ]);
    stores.extend([
        ("widgets.outputs".into(),Value::list(vec![json!({"name":"right","x":0,"y":0,"width":1920,"height":1080,"logical_width":1920,"logical_height":1080,"scale":1})])),
        ("widgets.screen_width".into(),Value::Number(1920.)),
        ("widgets.screen_height".into(),Value::Number(1080.)),
    ]);
    session.stores = session.stores.reconcile(&stores).unwrap();
    let mut runtime = Runtime::for_output(session, Some("right".into()));
    runtime.state_file = None;
    runtime.bar = true;
    if popouts {
        runtime = runtime.with_popout_trigger().unwrap();
    }
    runtime.widgets_file = Some(file);
    runtime.register_widget_drag_action().unwrap();
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("real bar loads");
    runtime.commit();
    runtime
}

#[test]
fn bar_press_prepares_the_exact_drag_source_without_starting_the_gesture() {
    let dir = std::env::temp_dir().join(format!("widget-drag-prepare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    let nodes = runtime.session.view_nodes();
    let source = nodes
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("clock")
        })
        .expect("the real bar exposes a draggable clock inside its status pill");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));

    if let Some(scene) = runtime.stage(UiEvent::Click { x, y }).unwrap() {
        runtime.commit();
        let _ = scene;
    }
    let events = events.lock().unwrap();
    let prepared = events.iter().find_map(|event| match event {
        UiEvent::CanvasPrepare {
            view,
            output,
            values,
            initial_bounds,
        } if view == "widgets-drag" => Some((output, values, initial_bounds)),
        _ => None,
    });
    let Some((output, values, initial_bounds)) = prepared else {
        panic!("pressing a real widget prepares its passive drag preview before threshold");
    };
    assert_eq!(output.as_deref(), Some("right"));
    let initial_bounds = initial_bounds.expect("prepare carries source-form geometry");
    let number = |key: &str| match values.get(key) {
        Some(Value::Number(value)) => *value as f32,
        _ => panic!("{key} is present in the prepared source geometry"),
    };
    let source_left =
        bounds.x + number("widgets.drag.source_origin_x") - number("widgets.drag.target_origin_x");
    let source_top =
        bounds.y + number("widgets.drag.source_origin_y") - number("widgets.drag.target_origin_y");
    assert!((initial_bounds.w - (bounds.w.max(1.).ceil() + 1.)).abs() < 0.01);
    assert!((initial_bounds.h - (bounds.h.max(1.).ceil() + 1.)).abs() < 0.01);
    assert!((initial_bounds.x - source_left.floor()).abs() < 0.01);
    assert!((initial_bounds.y - source_top.floor()).abs() < 0.01);
    assert_eq!(
        values.get("widgets.drag.phase"),
        Some(&Value::Text("prepare".into()))
    );
    assert_eq!(
        values.get("widgets.drag.destination"),
        Some(&Value::Text("desktop".into()))
    );
    assert_eq!(values.get("widgets.drag.active"), Some(&Value::Bool(true)));
    assert_eq!(
        values.get("widgets.drag.source_origin_valid"),
        Some(&Value::Bool(true))
    );
    let Some(Value::List(items)) = values.get("widgets.drag.source") else {
        panic!("prepared source preserves the captured widget payload");
    };
    assert!(items.first().is_some_and(|item| {
        item["widget"] == "clock" && item["host"] == "top_bar" && item["form"] == "bar"
    }));
    assert_ne!(
        runtime.session.stores.get("widgets.drag.active"),
        Some(&Value::Bool(true)),
        "the prewarm must not start the transfer before its existing movement threshold"
    );
    drop(events);
    let _ = std::fs::remove_dir_all(&dir);
}

fn hover_prewarm_document() -> Json {
    let mut document = document();
    document["preset"] = json!("Hover prewarm");
    document["widgets"] = json!({
        "clock-main": {
            "widget":"clock",
            "settings":{"dropdown_trigger":"default"},
            "forms":{
                "bar":{"enabled":true,"style":"time","placement":{"host":"top_bar","slot":{"region":"left","start":0,"span":1}}},
                "desktop":{"enabled":false,"style":"time","placement":{"host":"canvas","slot":{"x":40,"y":50,"w":360,"h":100}}}
            }
        }
    });
    document["hosts"]["canvas"] = json!({
        "width":1920,"height":1080,"instances":[
            {"iid":"clock-desktop","group":"clock-main","widget":"clock","host":"canvas","style":"time","slot":{"x":40,"y":50,"w":360,"h":100},"settings":{}}
        ]
    });
    document
}

#[test]
fn bar_hover_prewarms_once_retires_on_leave_and_reuses_the_warm_destination() {
    let dir =
        std::env::temp_dir().join(format!("widget-drag-hover-prepare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    runtime = runtime
        .with_shared_desktop(
            bead::services::Desktop::start_config(bead::services::Config {
                audio: false,
                bus: false,
            }),
            true,
        )
        .unwrap();
    let initial_document = hover_prewarm_document();
    if runtime
        .stage(UiEvent::Model(BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![initial_document.clone()]),
            ),
            ("widgets.rev".into(), Value::Number(1.)),
        ])))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    let source = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("clock")
        })
        .expect("the real bar exposes a draggable clock inside its status pill");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let candidate = runtime
        .session
        .transfer_candidate_at(x, y)
        .expect("the hovered real widget has a passive transfer candidate");
    let Value::List(candidate_items) = &candidate.payload else {
        panic!("widget drag payload is a list");
    };
    assert!(candidate_items.first().is_some_and(|item| {
        item["widget"] == "clock"
            && item["form"] == "bar"
            && item["style"] == "time"
            && item["host"] == "top_bar"
            && item["settings"]["dropdown_trigger"] == "default"
    }));
    let layout_before = runtime.session.stores.get("widgets.layout").cloned();
    let focused_before = runtime.session.stores.get("bead.focused").cloned();
    let pressed_before = runtime.session.stores.get("bead.pressed").cloned();
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    let hover = |runtime: &mut Runtime, x, y| {
        if runtime.stage(UiEvent::Hover { x, y }).unwrap().is_some() {
            runtime.commit();
        }
    };

    hover(&mut runtime, x, y);
    hover(&mut runtime, x + 1., y);
    assert!(runtime.widget_drag_controller.is_some());
    assert!(runtime.widget_drag_controller_primed);
    assert_eq!(
        runtime.session.stores.get("widgets.layout").cloned(),
        layout_before
    );
    assert_eq!(
        runtime.session.stores.get("bead.focused").cloned(),
        focused_before
    );
    assert_eq!(
        runtime.session.stores.get("bead.pressed").cloned(),
        pressed_before
    );
    assert!(runtime.session.transfer_candidate().is_none());
    assert!(!file.exists(), "hover does not persist widget layout");

    {
        let events = events.lock().unwrap();
        let prepares = events
            .iter()
            .filter_map(|event| match event {
                UiEvent::CanvasPrepare {
                    view,
                    output,
                    values,
                    initial_bounds,
                } if view == "widgets-drag" => Some((output, values, initial_bounds)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            prepares.len(),
            1,
            "pointer movement over a widget is deduplicated"
        );
        assert_eq!(prepares[0].0.as_deref(), Some("right"));
        assert!(prepares[0].2.is_some());
        assert_eq!(
            prepares[0].1.get("widgets.drag.source"),
            Some(&candidate.payload)
        );
        assert_eq!(
            prepares[0].1.get("widgets.drag.destination"),
            Some(&Value::Text("desktop".into()))
        );
        assert_eq!(
            prepares[0].1.get("widgets.drag.phase"),
            Some(&Value::Text("prepare".into()))
        );
    }

    runtime
        .widget_hover_preview
        .as_mut()
        .expect("hover preview owns a bounded renewal deadline")
        .prepared_at = std::time::Instant::now() - std::time::Duration::from_secs(2);
    assert!(
        runtime.wakeup_at().is_some(),
        "stationary hover renews via the source deadline"
    );
    if runtime
        .stage(UiEvent::Model(BTreeMap::new()))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, UiEvent::CanvasPrepare { view, .. } if view == "widgets-drag"))
            .count(),
        2,
        "a stationary pointer renews the same passive warm target before its two-second expiry"
    );
    assert!(
        !events.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::CanvasOffer { view, open: false, .. } if view == "widgets-drag")),
        "renewal does not close or replace the retained worker"
    );

    let bar_payload = runtime
        .session
        .transfer_candidate_at(x, y)
        .expect("the source still has its configured bar transfer payload")
        .payload;
    let mut target_config = initial_document.clone();
    target_config["widgets"]["clock-main"]["forms"]["desktop"]["style"] = json!("time+date");
    if runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![target_config])),
            ("widgets.rev".into(), Value::Number(2.)),
        ])))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    assert_eq!(
        runtime
            .session
            .transfer_candidate_at(x, y)
            .expect("source remains at the pointer")
            .payload,
        bar_payload,
        "changing the configured desktop form leaves the bar payload unchanged"
    );
    hover(&mut runtime, x, y);
    let target_preview = events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|event| match event {
            UiEvent::CanvasPrepare { view, values, .. } if view == "widgets-drag" => {
                Some(values.clone())
            }
            _ => None,
        });
    assert!(
        matches!(target_preview.as_ref().and_then(|values| values.get("widgets.layout")),
        Some(Value::List(documents))
            if documents[0]["widgets"]["clock-main"]["forms"]["desktop"]["style"] == "time+date")
    );
    assert_eq!(
        target_preview
            .as_ref()
            .and_then(|values| values.get("widgets.drag.source")),
        Some(&bar_payload)
    );

    hover(&mut runtime, -1., -1.);
    assert!(runtime.widget_drag_controller.is_none());
    assert!(!runtime.widget_drag_controller_primed);
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::CanvasOffer { view, output: Some(output), open: false, .. }
            if view == "widgets-drag" && output == "right")));
    hover(&mut runtime, x, y);
    hover(&mut runtime, x + 1., y);
    let prepares = events
        .lock()
        .unwrap()
        .iter()
        .filter(
            |event| matches!(event, UiEvent::CanvasPrepare { view, .. } if view == "widgets-drag"),
        )
        .count();
    assert_eq!(
        prepares, 4,
        "re-entry reuses the retained destination instead of restarting it"
    );

    runtime.output = Some("left".into());
    hover(&mut runtime, x, y);
    hover(&mut runtime, x + 1., y);
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::CanvasOffer { view, output: Some(output), open: false, .. }
            if view == "widgets-drag" && output == "right")));
    let prepares = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(event, UiEvent::CanvasPrepare { view, output: Some(output), .. }
            if view == "widgets-drag" && output == "left")
        })
        .count();
    assert_eq!(
        prepares, 1,
        "a changed output gets one prepared destination"
    );
    hover(&mut runtime, -1., -1.);
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::CanvasOffer { view, output: Some(output), open: false, .. }
            if view == "widgets-drag" && output == "left")));
    hover(&mut runtime, x, y);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(
                |event| matches!(event, UiEvent::CanvasPrepare { view, output: Some(output), .. }
                if view == "widgets-drag" && output == "left")
            )
            .count(),
        2,
        "re-enter within the host retention window prepares on the same output"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn completed_drag_retirement_stops_stationary_hover_renewal_until_reentry() {
    let dir = std::env::temp_dir().join(format!("widget-drag-hover-retire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    runtime = runtime
        .with_shared_desktop(
            bead::services::Desktop::start_config(bead::services::Config {
                audio: false,
                bus: false,
            }),
            true,
        )
        .unwrap();
    let document = hover_prewarm_document();
    if runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![document])),
            ("widgets.rev".into(), Value::Number(1.)),
        ])))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    let source = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("clock")
        })
        .expect("the real bar exposes a draggable clock inside its status pill");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));

    if runtime.stage(UiEvent::Hover { x, y }).unwrap().is_some() {
        runtime.commit();
    }
    assert!(runtime.widget_hover_preview.is_some());
    assert!(runtime.widget_drag_controller.is_some());
    assert!(runtime.widget_drag_controller_primed);

    runtime.widget_drag_model = Some(BTreeMap::from([(
        "widgets.drag.active".into(),
        Value::Bool(false),
    )]));
    runtime.widget_drag_retire_cleanup = true;
    runtime.commit();

    assert!(runtime.widget_drag_controller.is_none());
    assert!(!runtime.widget_drag_controller_primed);
    assert!(runtime.widget_drag_model.is_none());
    assert!(runtime.widget_hover_preview.is_none());
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::CanvasOffer { view, open: false, .. } if view == "widgets-drag")));

    if runtime
        .stage(UiEvent::Model(BTreeMap::new()))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    let prepares = || {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(event, UiEvent::CanvasPrepare { view, .. } if view == "widgets-drag")
            })
            .count()
    };
    assert_eq!(
        prepares(),
        1,
        "empty-model wakeups do not rewarm the retired view"
    );

    if runtime.stage(UiEvent::Hover { x, y }).unwrap().is_some() {
        runtime.commit();
    }
    assert_eq!(prepares(), 2, "a fresh pointer hover may prewarm again");
    assert!(runtime.widget_hover_preview.is_some());
    assert!(runtime.widget_drag_controller_primed);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bar_drag_threshold_offers_ghost_before_root_reflow_commit() {
    let dir = std::env::temp_dir().join(format!(
        "widget-drag-threshold-offer-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    let canonical = runtime
        .session
        .stores
        .get("widgets.layout")
        .cloned()
        .expect("bar has canonical widget layout");
    let draft = runtime
        .session
        .stores
        .get("widgets.draft")
        .cloned()
        .expect("bar has draft state");
    let source = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("clock")
        })
        .expect("the real bar exposes a draggable clock inside its status pill");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    runtime.prewarm_widget_drag_hover(x, y, true);
    assert!(runtime.widget_drag_controller_primed);
    if runtime.stage(UiEvent::Click { x, y }).unwrap().is_some() {
        runtime.commit();
    }
    events.lock().unwrap().clear();

    assert!(
        runtime
            .stage(UiEvent::Drag { x: x + 8., y })
            .unwrap()
            .is_some()
    );
    assert!(runtime.pending.is_some(), "root bar reflow remains pending");
    assert!(
        !runtime.widget_drag_controller_primed,
        "the real gesture claims and reuses the passively primed controller"
    );
    let events = events.lock().unwrap();
    let offer = events
        .iter()
        .find_map(|event| match event {
            UiEvent::CanvasOffer {
                view,
                output,
                open: true,
                values,
            } if view == "widgets-drag" => Some((output, values)),
            _ => None,
        })
        .expect("threshold immediately offers the passive ghost");
    assert_eq!(offer.0.as_deref(), Some("right"));
    assert_eq!(
        offer.1.get("widgets.drag.phase"),
        Some(&Value::Text("dragging".into()))
    );
    assert!(
        matches!(offer.1.get("widgets.drag.layout"),Some(Value::List(rows)) if !rows.is_empty())
    );
    assert_eq!(offer.1.get("widgets.layout"), Some(&canonical));
    assert_eq!(offer.1.get("widgets.draft"), Some(&draft));
    assert!(
        !events.iter().any(|event| matches!(event,
            UiEvent::CanvasOffer { view, .. } | UiEvent::CanvasModel { view, .. }
                if view == "widgets-canvas")),
        "the early path publishes only the passive ghost; canonical canvas reflow stays on its existing commit path"
    );
    assert!(
        matches!(runtime.widget_drag_model.as_ref().and_then(|model| model.get("widgets.write")),
            Some(Value::List(rows)) if rows.is_empty()),
        "threshold preview never writes or persists the candidate layout"
    );
    assert!(
        !file.exists(),
        "drag motion does not create the layout file"
    );
    assert_eq!(
        runtime.session.stores.get("widgets.layout"),
        Some(&canonical)
    );
    drop(events);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejected_threshold_transaction_returns_and_retires_ghost() {
    let dir = std::env::temp_dir().join(format!(
        "widget-drag-threshold-reject-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    let source = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("clock")
        })
        .expect("the real bar exposes a draggable clock inside its status pill");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    if runtime.stage(UiEvent::Click { x, y }).unwrap().is_some() {
        runtime.commit();
    }
    events.lock().unwrap().clear();

    let declared = runtime.session.stores.values().clone();
    let mut invalid = declared.clone();
    invalid.insert(
        "widgets.drag.layout".into(),
        Value::Text("wrong type".into()),
    );
    runtime.session.stores = bead::core::Stores::default().reconcile(&invalid).unwrap();
    let _ = runtime.stage(UiEvent::Drag { x: x + 8., y }).unwrap();
    let token = runtime.widget_drag_epoch;
    {
        let events = events.lock().unwrap();
        assert!(
            events.iter().any(|event| matches!(event,
            UiEvent::CanvasOffer { view, open: true, values, .. }
                if view == "widgets-drag"
                    && values.get("widgets.drag.phase")==Some(&Value::Text("dragging".into())))),
            "the threshold starts a speculative ghost before root preflight"
        );
        assert!(
            events.iter().any(|event| matches!(event,
            UiEvent::CanvasModel { view, values, .. }
                if view == "widgets-drag"
                    && values.get("widgets.drag.phase")==Some(&Value::Text("return".into())))),
            "rejected root preflight returns the speculative ghost"
        );
    }

    runtime.session.stores = bead::core::Stores::default().reconcile(&declared).unwrap();
    let _ = runtime.stage(UiEvent::Timer { token }).unwrap();
    if runtime.pending.is_some() {
        runtime.commit();
    }
    assert!(
        events.lock().unwrap().iter().any(|event| matches!(event,
            UiEvent::CanvasOffer { view, open: false, .. } if view == "widgets-drag")),
        "the rejected candidate's returned ghost is eventually retired"
    );
    assert!(!file.exists(), "rejected drag does not persist layout");
    drop(events);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn canvas_press_prepares_the_configured_bar_destination_with_global_origin() {
    let mut runtime = real_widget_canvas_publisher();
    let layout = canvas_transfer_document();
    let host_values = BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![layout])),
        ("widgets.rev".into(), Value::Number(4.)),
        ("widgets.editing".into(), Value::Bool(true)),
        ("widgets.draft".into(), Value::list(Vec::new())),
        ("widgets.screen_origin_x".into(), Value::Number(1000.)),
        ("widgets.screen_origin_y".into(), Value::Number(40.)),
        ("bead.output.name".into(), Value::Text("right".into())),
        ("bead.desktop.clock".into(), Value::Text("12:34".into())),
    ]);
    if runtime
        .stage(UiEvent::Model(host_values))
        .unwrap()
        .is_some()
    {
        runtime.commit();
    }
    let nodes = runtime.session.view_nodes();
    let source = nodes
        .iter()
        .find(|node| {
            node.drag_action.as_deref() == Some("widgets.drag") && node.id.contains("canvas-clock")
        })
        .expect("the desktop clock exposes a native drag source");
    let bounds = bead::ui::rect(&source.draw);
    let (x, y) = (bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.);
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));

    if runtime.stage(UiEvent::Click { x, y }).unwrap().is_some() {
        runtime.commit();
    }
    let events = events.lock().unwrap();
    let values = events.iter().find_map(|event| match event {
        UiEvent::CanvasPrepare {
            view,
            output: Some(output),
            values,
            initial_bounds,
        } if view == "widgets-drag" && output == "right" => Some((values, initial_bounds)),
        _ => None,
    });
    let Some((values, initial_bounds)) = values else {
        panic!("a native canvas source prepares its bar-destination ghost before threshold");
    };
    let bounds = initial_bounds.expect("canvas source prepare carries output-local geometry");
    assert!(bounds.x < 1000., "warm bounds are local to the output");
    assert!(bounds.y < 1200., "warm bounds are local to the output");
    assert_eq!(
        values.get("widgets.drag.destination"),
        Some(&Value::Text("bar".into()))
    );
    assert_eq!(
        values.get("widgets.drag.source_origin_x"),
        Some(&Value::Number(1000.))
    );
    assert_eq!(
        values.get("widgets.drag.source_origin_y"),
        Some(&Value::Number(40.))
    );
    assert!(matches!(
        values.get("widgets.drag.source"),
        Some(Value::List(items)) if items.first().is_some_and(|item| {
            item["widget"] == "clock" && item["host"] == "canvas" && item["form"] == "desktop"
        })
    ));
    assert_ne!(
        runtime.session.stores.get("widgets.drag.active"),
        Some(&Value::Bool(true)),
        "preparing the return preview does not activate a drag before threshold"
    );
}

#[test]
fn canvas_public_projection_renders_clock_and_routes_launcher_visibility_to_host() {
    let dir = std::env::temp_dir().join(format!("widget-canvas-runtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut bar = real_bar_runtime(dir.join("widgets.json"));
    let layout = json!({
        "version":1,"preset":"Canvas fixture",
        "hosts":{
            "top_bar":{"mode":"tiles","tiles":60,"regions":{
                "left":{"start":0,"span":20},"centre":{"start":20,"span":20},"right":{"start":40,"span":20}
            },"instances":[]},
            "canvas":{"width":1920,"height":1080,"instances":[
                {"iid":"canvas-clock","group":"canvas-clock","widget":"clock","host":"canvas","style":"time",
                    "slot":{"x":40,"y":50,"w":280,"h":100},"settings":{}},
                {"iid":"canvas-launcher","group":"canvas-launcher","widget":"launcher","host":"canvas","style":"nacre",
                    "slot":{"x":360,"y":50,"w":96,"h":96},"settings":{"action":"launcher"}}
            ]}
        }
    });
    let mut host_values = BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![layout.clone()])),
        ("widgets.rev".into(), Value::Number(4.)),
        ("widgets.drag.active".into(), Value::Bool(false)),
        ("bead.desktop.clock".into(), Value::Text("12:34".into())),
        (
            "bead.desktop.clock.seconds".into(),
            Value::Text("12:34:56".into()),
        ),
        (
            "bead.desktop.clock.date".into(),
            Value::Text("2026-10-26".into()),
        ),
    ]);
    for key in [
        "widgets.outputs",
        "widgets.screen_width",
        "widgets.screen_height",
    ] {
        host_values.insert(key.into(), bar.session.stores.get(key).unwrap().clone());
    }
    bar.stage(UiEvent::Model(host_values)).unwrap();
    bar.commit();
    bar.widget_drag_model = Some(BTreeMap::from([
        ("widgets.drag.revision".into(), Value::Number(91.)),
        ("widgets.drag.active".into(), Value::Bool(false)),
    ]));
    let offers = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = offers.clone();
    bar.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    bar.widgets_canvas_wanted = false;
    bar.widget_canvas_projection = None;
    bar.publish_widget_models();
    let projection = offers
        .lock()
        .unwrap()
        .iter()
        .find_map(|event| match event {
            UiEvent::CanvasOffer {
                view,
                open: true,
                values,
                ..
            } if view == "widgets-canvas" => Some(values.clone()),
            _ => None,
        })
        .expect("the bar offers the configured canvas to its host");
    assert_eq!(projection["widgets.layout"], Value::list(vec![layout]));
    assert_eq!(
        projection["bead.desktop.clock"],
        Value::Text("12:34".into())
    );
    assert_eq!(
        projection["widgets.outputs"],
        bar.session.stores.get("widgets.outputs").unwrap().clone()
    );
    assert!(
        !projection.contains_key("widgets.drag.revision"),
        "controller revision is private and is not sent to the canvas script"
    );

    let shell = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = shell.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session = bead::ui::Session::new(
        shell,
        "widgets/editor/canvas.luau".into(),
        palette,
        colors,
        cold,
    );
    session.set_viewport(1920, 1080);
    let mut canvas = Runtime::panel_session(session, true)
        .with_widget_editor(None, true)
        .unwrap();
    canvas.state_file = None;
    canvas.widgets_file = None;
    let canvas_host_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = canvas_host_events.clone();
    canvas.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    canvas
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("the real canvas behavior initializes from its declared stores");
    canvas.commit();
    let first_scene = canvas
        .stage(UiEvent::Model(projection.clone()))
        .unwrap()
        .expect("the host model is accepted by the actual canvas runtime");
    canvas.commit();
    let rendered_text = first_scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, .. } if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        rendered_text.iter().any(|text| text == "12:34"),
        "the projected desktop clock is painted with live host text; rendered={rendered_text:?}, clock={:?}",
        canvas.session.stores.get("bead.desktop.clock")
    );
    let clock_region = canvas
        .input_regions()
        .into_iter()
        .find(|region| region.id.contains("canvas-clock"))
        .expect("the rendered clock exposes a native canvas hit region");
    assert!(clock_region.include_in_bounds);
    assert!(clock_region.rect.w > 0. && clock_region.rect.h > 0.);
    let mut changed_clock = projection.clone();
    changed_clock.insert("bead.desktop.clock".into(), Value::Text("12:35".into()));
    let changed_scene = canvas
        .stage(UiEvent::Model(changed_clock))
        .unwrap()
        .expect("a later host model update is applied without changing the canvas behavior");
    canvas.commit();
    assert!(
        changed_scene.primitives.iter().any(|primitive| matches!(
            primitive,
            bead::render::Primitive::Text { text, .. } if text == "12:35"
        )),
        "live desktop model changes update the painted clock text"
    );
    assert!(
        canvas_host_events
            .lock()
            .unwrap()
            .iter()
            .all(|event| !matches!(
                event,
                UiEvent::CanvasOffer { view, .. } | UiEvent::CanvasModel { view, .. }
                    if view == "widgets-canvas"
            )),
        "the canvas consumes its host's document and live models without echoing them back as host events"
    );
    let launcher_region = canvas
        .input_regions()
        .into_iter()
        .find(|region| region.id.contains("canvas-launcher"))
        .expect("the canvas launcher exposes an activatable input region");
    assert!(launcher_region.include_in_bounds);
    assert!(launcher_region.rect.w > 0. && launcher_region.rect.h > 0.);
    let launcher_node = canvas
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id.contains("canvas-launcher") && node.drag_action.is_some())
        .expect("the canvas widget keeps its native drag source metadata");
    assert!(launcher_node.drag_action.is_some());

    let visibility = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = visibility.clone();
    canvas.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    canvas.stage(UiEvent::Click { x: 408., y: 98. }).unwrap();
    canvas.commit();
    canvas.stage(UiEvent::Release).unwrap();
    canvas.commit();
    assert!(
        visibility
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, UiEvent::CompanionVisibility(true))),
        "canvas activation emits a host lifecycle event instead of staging it in Luau"
    );
    canvas.stage(UiEvent::Click { x: 408., y: 98. }).unwrap();
    canvas.commit();
    canvas.stage(UiEvent::Drag { x: 416., y: 98. }).unwrap();
    canvas.commit();
    assert_eq!(
        canvas.session.stores.get("widgets.drag.active"),
        Some(&Value::Bool(true)),
        "a native canvas drag is delivered to the editor's drag action"
    );
    canvas.stage(UiEvent::Release).unwrap();
    canvas.commit();
    let _ = std::fs::remove_dir_all(dir);
}

fn status_widget_point(runtime: &Runtime, draggable: bool) -> (f32, f32) {
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.action.as_deref() == Some("status.pill.open")
                && (!draggable || node.drag_action.is_some())
        })
        .expect("the real bar has a status widget with the requested input metadata");
    let rect = bead::ui::rect(&node.draw);
    (rect.x + rect.w / 2., rect.y + rect.h / 2.)
}

fn canvas_transfer_document() -> Json {
    let mut doc = document();
    doc["preset"] = json!("Canvas model contract");
    doc["hosts"]["canvas"] = json!({"width":1920,"height":1080,"instances":[
        {"iid":"canvas-clock","group":"canvas-clock","widget":"clock","host":"canvas","style":"time",
            "slot":{"x":40,"y":50,"w":280,"h":100},"settings":{}}
    ]});
    doc
}

fn canvas_transfer_drag_model(document: &Json, x: f64) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("widgets.drag.active".into(), Value::Bool(true)),
        (
            "widgets.drag.source".into(),
            Value::list(vec![json!({
            "iid":"clock-main","group":"clock-main","widget":"clock","host":"top_bar",
            "form":"bar","style":"time","settings":{}})]),
        ),
        (
            "widgets.drag.destination".into(),
            Value::Text("desktop".into()),
        ),
        ("widgets.drag.phase".into(), Value::Text("dragging".into())),
        ("widgets.drag.epoch".into(), Value::Number(17.)),
        (
            "widgets.drag.layout".into(),
            Value::list(vec![document.clone()]),
        ),
        (
            "widgets.drag.base".into(),
            Value::list(vec![document.clone()]),
        ),
        (
            "widgets.drag.original".into(),
            Value::list(vec![document.clone()]),
        ),
        ("widgets.drag.x".into(), Value::Number(x)),
        ("widgets.drag.y".into(), Value::Number(120.)),
        ("widgets.drag.start_x".into(), Value::Number(100.)),
        ("widgets.drag.start_y".into(), Value::Number(70.)),
        ("widgets.drag.width".into(), Value::Number(64.)),
        ("widgets.drag.height".into(), Value::Number(40.)),
        ("widgets.drag.grab_x".into(), Value::Number(16.)),
        ("widgets.drag.grab_y".into(), Value::Number(8.)),
        ("widgets.drag.snap_x".into(), Value::Number(400.)),
        ("widgets.drag.snap_y".into(), Value::Number(100.)),
        ("widgets.drag.source_origin_x".into(), Value::Number(0.)),
        ("widgets.drag.source_origin_y".into(), Value::Number(0.)),
        ("widgets.drag.source_origin_valid".into(), Value::Bool(true)),
        ("widgets.drag.target_origin_x".into(), Value::Number(0.)),
        ("widgets.drag.target_origin_y".into(), Value::Number(0.)),
        ("widgets.drag.app_origin_x".into(), Value::Number(0.)),
        ("widgets.drag.app_origin_y".into(), Value::Number(0.)),
        ("widgets.drag.app_origin_valid".into(), Value::Bool(true)),
        ("widgets.drag.app_width".into(), Value::Number(0.)),
        ("widgets.drag.app_height".into(), Value::Number(0.)),
        (
            "widgets.drag.target_output_id".into(),
            Value::Text("right".into()),
        ),
        // Controller-only state deliberately has no receiver in either surface.
        ("widgets.drag.revision".into(), Value::Number(91.)),
    ])
}

fn widget_surface_source(
    passive_drag_preview: bool,
    initial_model: BTreeMap<String, Value>,
    host_events: std::sync::Arc<std::sync::Mutex<Vec<UiEvent>>>,
) -> Box<dyn SceneSource> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let inputs = crate::widgets::Inputs {
        client: None,
        desktop: std::sync::Arc::new(std::sync::OnceLock::new()),
        backend: std::sync::Arc::new(std::sync::OnceLock::new()),
        prefs: None,
    };
    let spec = crate::widgets::canvas_spec_with_root(
        root,
        palette,
        colors,
        inputs,
        false,
        if passive_drag_preview {
            "widgets/drag/ghost.luau"
        } else {
            "widgets/editor/canvas.luau"
        },
        passive_drag_preview,
    )
    .unwrap();
    assert_eq!(spec.passive, passive_drag_preview);
    let mut source =
        (spec.factory)(true).expect("the real canvas/ghost factory creates its Runtime");
    let sent = host_events;
    source.services(Box::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    source.watch(Box::new(|_| {})).unwrap();
    source.initial_model(initial_model).unwrap();
    source.viewport(1920, 1080);
    if source
        .stage(UiEvent::WallpaperPalette(BTreeMap::new()))
        .unwrap()
        .is_some()
    {
        source.validate().unwrap();
        assert!(source.commit());
    }
    if source
        .stage(UiEvent::OutputName("right".into()))
        .unwrap()
        .is_some()
    {
        source.validate().unwrap();
        assert!(source.commit());
    }
    source
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("the real canvas/ghost accepts the host startup sequence");
    source.validate().unwrap();
    assert!(source.commit());
    source
}

fn deliver_widget_surface_models<'a>(
    events: &[UiEvent],
    canvas: &'a mut dyn SceneSource,
    ghost: &'a mut dyn SceneSource,
) -> usize {
    let mut delivered = 0;
    for event in events {
        let (view, open, values) = match event {
            UiEvent::CanvasOffer {
                view,
                open: true,
                values,
                ..
            } => (view.as_str(), true, values),
            UiEvent::CanvasOffer { .. } => continue,
            UiEvent::CanvasModel { view, values, .. } => (view.as_str(), true, values),
            _ => continue,
        };
        if !open || !matches!(view, "widgets-canvas" | "widgets-drag") {
            continue;
        }
        assert!(!values.contains_key("widgets.base_rev"));
        assert!(!values.contains_key("widgets.drag.revision"));
        assert!(values.contains_key("widgets.drag.app_origin_valid"));
        assert_eq!(values.get("bar.workspace.hints"), Some(&Value::Bool(false)));
        assert!(values.contains_key("bar.workspace.origins"));
        let target = if view == "widgets-canvas" {
            &mut *canvas
        } else {
            &mut *ghost
        };
        let scene = target
            .stage(UiEvent::Model(values.clone()))
            .unwrap_or_else(|error| panic!("{view} rejected its declared host model: {error}"));
        target.validate().unwrap();
        if scene.is_some() {
            assert!(target.commit(), "{view} commits its real Model update");
        }
        delivered += 1;
    }
    delivered
}

fn widget_host_effect(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::CanvasPrepare { .. }
            | UiEvent::CanvasOffer { .. }
            | UiEvent::CanvasModel { .. }
            | UiEvent::CompanionOffer { .. }
            | UiEvent::CompanionVisibility(_)
            | UiEvent::CompanionSelect { .. }
            | UiEvent::CompanionSelectAt { .. }
            | UiEvent::CompanionAction { .. }
            | UiEvent::CompanionMotionAction { .. }
            | UiEvent::CompanionModel(_)
            | UiEvent::HostModel(_)
            | UiEvent::PanelDismissed
            | UiEvent::PanelNavigated(_)
            | UiEvent::PanelSelected(_)
            | UiEvent::PanelDetached(_)
            | UiEvent::NativePearlMotion(_)
            | UiEvent::NativePearlPlaced { .. }
            | UiEvent::ShellHidden(_)
            | UiEvent::LivePalette(_)
            | UiEvent::ControlChanged
    )
}

fn largest_rounded_size(scene: &bead::render::Scene) -> (f32, f32) {
    scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                Some((rect.w, rect.h))
            }
            _ => None,
        })
        .max_by(|left, right| (left.0 * left.1).total_cmp(&(right.0 * right.1)))
        .expect("the passive clock ghost paints a rounded card")
}

#[test]
fn passive_prepare_bakes_desktop_clock_without_input_and_seeds_bar_sized_morph() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let inputs = crate::widgets::Inputs {
        client: None,
        desktop: std::sync::Arc::new(std::sync::OnceLock::new()),
        backend: std::sync::Arc::new(std::sync::OnceLock::new()),
        prefs: None,
    };
    let spec = crate::widgets::canvas_spec_with_root(
        root,
        palette,
        colors,
        inputs,
        false,
        "widgets/drag/ghost.luau",
        true,
    )
    .unwrap();
    assert!(spec.passive);
    assert!(spec.retain_when_closed);
    let mut ghost = (spec.factory)(true).unwrap();
    ghost.viewport(1920, 1200);
    let mut layout = document();
    layout["widgets"] = json!({
        "clock-main":{"widget":"clock","settings":{},"forms":{
            "bar":{"enabled":true,"style":"time","placement":{"host":"top_bar",
                "slot":{"region":"left","start":0,"span":1}}},
            "desktop":{"enabled":false,"style":"seconds","placement":{"host":"canvas",
                "slot":{"x":480,"y":240,"w":240,"h":120}}}
        }}
    });
    let source = json!({
        "iid":"clock-main","group":"clock-main","widget":"clock",
        "host":"top_bar","form":"bar","style":"time","settings":{}
    });
    let prepare = BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![layout.clone()])),
        ("widgets.drag.active".into(), Value::Bool(true)),
        (
            "widgets.drag.source".into(),
            Value::list(vec![source.clone()]),
        ),
        ("widgets.drag.phase".into(), Value::Text("prepare".into())),
        (
            "widgets.drag.destination".into(),
            Value::Text("desktop".into()),
        ),
        ("widgets.drag.x".into(), Value::Number(120.)),
        ("widgets.drag.y".into(), Value::Number(64.)),
        ("widgets.drag.width".into(), Value::Number(64.)),
        ("widgets.drag.height".into(), Value::Number(40.)),
        ("widgets.drag.grab_x".into(), Value::Number(16.)),
        ("widgets.drag.grab_y".into(), Value::Number(8.)),
        ("widgets.drag.source_origin_valid".into(), Value::Bool(true)),
        ("widgets.drag.source_origin_x".into(), Value::Number(0.)),
        ("widgets.drag.source_origin_y".into(), Value::Number(0.)),
        (
            "widgets.drag.target_output_id".into(),
            Value::Text("right".into()),
        ),
        ("bead.desktop.clock".into(), Value::Text("12:34".into())),
        (
            "bead.desktop.clock.seconds".into(),
            Value::Text("12:34:56".into()),
        ),
    ]);
    let host_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = host_events.clone();
    ghost.services(Box::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    ghost.watch(Box::new(|_| {})).unwrap();
    ghost.initial_model(prepare.clone()).unwrap();
    if ghost
        .stage(UiEvent::OutputName("right".into()))
        .unwrap()
        .is_some()
    {
        ghost.validate().unwrap();
        assert!(ghost.commit());
    }
    let prepared = ghost
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("the passive ghost prepares its target scene");
    ghost.validate().unwrap();
    assert!(ghost.commit());
    assert!(ghost.input_regions().is_empty());
    assert!(prepared.primitives.iter().any(|primitive| matches!(
        primitive,
        bead::render::Primitive::Text { text, .. } if text == "12:34:56"
    )));
    let prepared_size = largest_rounded_size(&prepared);
    assert!(prepared_size.0 > 64. && prepared_size.1 > 40.);
    assert!(
        host_events
            .lock()
            .unwrap()
            .iter()
            .all(|event| !widget_host_effect(event))
    );

    let mut visible_model = prepare.clone();
    visible_model.insert("widgets.drag.phase".into(), Value::Text("dragging".into()));
    visible_model.insert("widgets.drag.destination".into(), Value::Text("bar".into()));
    visible_model.insert("widgets.drag.layout".into(), Value::list(vec![layout]));
    visible_model.insert("widgets.drag.x".into(), Value::Number(124.));
    visible_model.insert("widgets.drag.y".into(), Value::Number(68.));
    let source_frame = ghost
        .stage(UiEvent::Model(visible_model.clone()))
        .unwrap()
        .expect("the first active Model renders the source before destination entry");
    ghost.validate().unwrap();
    assert!(ghost.commit());
    assert!(ghost.input_regions().is_empty());
    assert!(source_frame.primitives.iter().any(|primitive| matches!(
        primitive,
        bead::render::Primitive::Text { text, .. } if text == "12:34"
    )));
    let source_size = largest_rounded_size(&source_frame);
    assert!(
        source_size.0 < prepared_size.0 * 0.75 && source_size.1 < prepared_size.1 * 0.75,
        "the prepared target scene remains larger than the visible bar source: target={prepared_size:?}, source={source_size:?}"
    );

    visible_model.insert(
        "widgets.drag.destination".into(),
        Value::Text("desktop".into()),
    );
    let desktop_frame = ghost
        .stage(UiEvent::Model(visible_model))
        .unwrap()
        .expect("the live destination update begins the same-size desktop morph");
    ghost.validate().unwrap();
    assert!(ghost.commit());
    assert!(ghost.input_regions().is_empty());
    assert!(desktop_frame.primitives.iter().any(|primitive| matches!(
        primitive,
        bead::render::Primitive::Text { text, .. } if text == "12:34:56"
    )));
    assert!(
        host_events
            .lock()
            .unwrap()
            .iter()
            .all(|event| !widget_host_effect(event))
    );
}

fn initial_canvas_offer_values(events: &[UiEvent], view: &str) -> BTreeMap<String, Value> {
    events
        .iter()
        .find_map(|event| match event {
            UiEvent::CanvasOffer {
                view: offered,
                open: true,
                values,
                ..
            } if offered == view => Some(values.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the bar publishes initial values for {view}"))
}

#[test]
fn canvas_and_passive_ghost_use_distinct_declared_host_model_contracts() {
    let dir = std::env::temp_dir().join(format!(
        "widget-canvas-ghost-model-contract-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let layout = canvas_transfer_document();
    let drag_model = canvas_transfer_drag_model(&layout, 420.);
    let source_values = BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![layout.clone()])),
        ("widgets.rev".into(), Value::Number(8.)),
        ("widgets.editing".into(), Value::Bool(false)),
        ("bar.workspace.hints".into(), Value::Bool(false)),
        (
            "bar.workspace.origins".into(),
            Value::list(vec![json!({"id":"workspace-1","x":18})]),
        ),
        ("bead.desktop.clock".into(), Value::Text("12:34".into())),
    ]);

    // The bar emits the initial canvas/ghost offers and later Model envelopes.
    let mut bar = real_bar_runtime(dir.join("widgets.json"));
    bar.session.stores.commit(source_values.clone()).unwrap();
    bar.widget_drag_model = Some(drag_model.clone());
    let bar_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = bar_events.clone();
    bar.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    bar.widgets_canvas_wanted = false;
    bar.widget_canvas_projection = None;
    bar.widget_drag_wanted = false;
    bar.widget_drag_projection = None;
    bar.publish_widget_models();
    let mut later_drag = drag_model.clone();
    later_drag.insert("widgets.drag.x".into(), Value::Number(432.));
    bar.widget_drag_model = Some(later_drag);
    bar.publish_widget_models();
    let bar_events = bar_events.lock().unwrap().clone();
    assert!(bar_events.iter().any(|event| matches!(event,
        UiEvent::CanvasOffer {view,open:true,..} if view=="widgets-canvas")));
    assert!(bar_events.iter().any(|event| matches!(event,
        UiEvent::CanvasOffer {view,open:true,..} if view=="widgets-drag")));
    assert!(bar_events.iter().any(|event| matches!(event,
        UiEvent::CanvasModel {view,..} if view=="widgets-canvas")));
    assert!(bar_events.iter().any(|event| matches!(event,
        UiEvent::CanvasModel {view,..} if view=="widgets-drag")));

    // A desktop canvas can publish its passive drag ghost, but the bar remains
    // the owner that publishes the canvas document and live model.
    let mut canvas_publisher = real_widget_canvas_publisher();
    let mut canvas_source_values = source_values.clone();
    canvas_source_values.insert("widgets.base_rev".into(), Value::Number(19.));
    canvas_publisher
        .session
        .stores
        .commit(canvas_source_values.clone())
        .unwrap();
    canvas_publisher.widget_drag_model = Some(drag_model.clone());
    let canvas_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent = canvas_events.clone();
    canvas_publisher.wakeup = Some(std::sync::Arc::new(move |event| {
        sent.lock().unwrap().push(event);
    }));
    canvas_publisher.publish_widget_models();
    let mut later_drag = drag_model;
    later_drag.insert("widgets.drag.x".into(), Value::Number(448.));
    canvas_publisher.widget_drag_model = Some(later_drag);
    canvas_publisher.publish_widget_models();
    let canvas_events = canvas_events.lock().unwrap().clone();
    assert!(
        !canvas_events.iter().any(|event| matches!(event,
        UiEvent::CanvasOffer {view,..} | UiEvent::CanvasModel {view,..} if view=="widgets-canvas")),
        "the canvas consumes its owner's live model instead of echoing it back to all canvases"
    );
    assert!(canvas_events.iter().any(|event| matches!(event,
        UiEvent::CanvasOffer {view,open:true,..} if view=="widgets-drag")));
    assert!(canvas_events.iter().any(|event| matches!(event,
        UiEvent::CanvasModel {view,values,..} if view=="widgets-drag"
            && values.get("widgets.drag.x")==Some(&Value::Number(448.)))));

    let ghost_host_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let canvas_host_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut canvas = widget_surface_source(
        false,
        initial_canvas_offer_values(&bar_events, "widgets-canvas"),
        canvas_host_events.clone(),
    );
    let mut ghost = widget_surface_source(
        true,
        initial_canvas_offer_values(&bar_events, "widgets-drag"),
        ghost_host_events.clone(),
    );
    assert!(
        !ghost_host_events
            .lock()
            .unwrap()
            .iter()
            .any(widget_host_effect),
        "the passive ghost's initial reload publishes no host effects"
    );
    let bar_delivered = deliver_widget_surface_models(&bar_events, &mut *canvas, &mut *ghost);
    let canvas_delivered = deliver_widget_surface_models(&canvas_events, &mut *canvas, &mut *ghost);
    assert!(
        bar_delivered >= 4,
        "bar offers and updates reach both surfaces"
    );
    assert!(
        canvas_delivered >= 2,
        "canvas drag ghost models reach the passive surface"
    );
    assert!(
        !canvas_host_events
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event,
        UiEvent::CanvasOffer {view,..} | UiEvent::CanvasModel {view,..} if view=="widgets-canvas")),
        "the actual canvas Runtime accepts bar Models without echoing its own document back to the host"
    );
    assert!(
        !ghost_host_events
            .lock()
            .unwrap()
            .iter()
            .any(widget_host_effect),
        "the passive ghost consumes host Models but publishes no host effects"
    );
    assert!(canvas_source_values.contains_key("widgets.base_rev"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn native_status_release_opens_once_and_clears_press_but_drag_does_not_open() {
    let dir = std::env::temp_dir().join(format!("widget-host-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let (mut click, click_routes) = real_bar_runtime_with_popouts(dir.join("click-widgets.json"));
    let (x, y) = status_widget_point(&click, false);
    click.stage(UiEvent::Click { x, y }).unwrap().unwrap();
    click.commit();
    assert_eq!(
        click.session.stores.get("bead.pressed"),
        Some(&Value::Bool(true))
    );
    click
        .stage(UiEvent::Release)
        .unwrap()
        .expect("release feedback is rendered");
    click.commit();
    assert_eq!(
        click.session.stores.get("bead.pressed"),
        Some(&Value::Bool(false))
    );
    assert!(
        click.status_pill.panel().is_some(),
        "the registered host route opens the status dropdown once"
    );
    assert!(
        click_routes.lock().unwrap().iter().any(|event| matches!(
            event,
            UiEvent::CompanionSelect { .. } | UiEvent::CompanionSelectAt { .. }
        )),
        "the registered host route requests the production dropdown panel"
    );

    let (mut drag, _) = real_bar_runtime_with_popouts(dir.join("drag-widgets.json"));
    let (x, y) = status_widget_point(&drag, true);
    drag.stage(UiEvent::Click { x, y }).unwrap().unwrap();
    drag.commit();
    drag.stage(UiEvent::Drag { x: x + 8., y }).unwrap();
    drag.commit();
    drag.stage(UiEvent::Release).unwrap();
    drag.commit();
    assert!(
        drag.status_pill.panel().is_none(),
        "a transfer must not invoke the click route"
    );
    assert_eq!(
        drag.session.stores.get("bead.pressed"),
        Some(&Value::Bool(false))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn animated_pill_clips_child_hit_region_and_keeps_its_settled_width() {
    use std::{thread, time::Duration};

    let dir = std::env::temp_dir().join(format!("widget-pill-clip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    std::fs::write(
        &file,
        serde_json::to_vec(&empty_clock_pill_document()).unwrap(),
    )
    .unwrap();
    let (mut runtime, routes) = real_bar_runtime_with_popouts(file);
    runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.editing".into(), Value::Bool(true)),
            (
                "widgets.draft".into(),
                Value::list(vec![empty_clock_pill_document()]),
            ),
            ("bead.desktop.clock".into(), Value::Text("1:00".into())),
        ])))
        .unwrap();
    runtime.commit();
    thread::sleep(Duration::from_millis(220));
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
    let pill_width = |runtime: &Runtime| {
        let pill = runtime
            .session
            .view_nodes()
            .iter()
            .find(|node| node.id.ends_with("system-pill"))
            .expect("the status pill is materialized");
        bead::ui::rect(&pill.draw).w
    };
    let initial_width = pill_width(&runtime);

    let mut long_clock = clock_pill_document();
    long_clock["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["iid"] == "status-pill")
        .unwrap()["children"][0]["slot"]["span"] = json!(4);
    runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.draft".into(), Value::list(vec![long_clock])),
            ("bead.desktop.clock".into(), Value::Text("23:59".into())),
            ("bar.clock.twelve".into(), Value::Bool(true)),
        ])))
        .unwrap();
    runtime.commit();
    assert!(runtime.session.preview_deadline().is_some());
    thread::sleep(Duration::from_millis(40));
    let animated_scene = runtime
        .stage(UiEvent::Model(BTreeMap::new()))
        .unwrap()
        .expect("the active pill animation publishes an intermediate frame");
    runtime.commit();
    assert!(
        runtime.session.preview_deadline().is_some(),
        "the pill is still animating"
    );

    let pill = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id.ends_with("system-pill"))
        .expect("the animated pill remains materialized");
    let pill_rect = bead::ui::rect(&pill.draw);
    assert!(
        pill_rect.w > initial_width,
        "the pill is growing toward its natural width (initial={initial_width}, current={})",
        pill_rect.w
    );
    let rendered_text = animated_scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let text_index = animated_scene
        .primitives
        .iter()
        .enumerate()
        .find_map(|(index, primitive)| match primitive {
            bead::render::Primitive::Text { text, .. } if text.starts_with("11:59") => Some(index),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the long 12-hour clock text is painted; got {rendered_text:?}"));
    let status = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.action.as_deref() == Some("status.pill.open") && node.id.contains("system-pill")
        })
        .expect("the clock child remains an interactive status item");
    let status_clip = status
        .clip
        .expect("the status hit node inherits its pill content clip");
    assert!(
        status_clip.x >= pill_rect.x
            && status_clip.y >= pill_rect.y
            && status_clip.x + status_clip.w <= pill_rect.x + pill_rect.w
            && status_clip.y + status_clip.h <= pill_rect.y + pill_rect.h,
        "status paint and hit testing stay inside the animated pill: child={status_clip:?}, pill={pill_rect:?}"
    );
    let pill_right = pill_rect.x + pill_rect.w;
    let text_clip = animated_scene
        .clips
        .get(&text_index)
        .expect("clock text inherits a paint clip");
    assert!(
        text_clip.x >= pill_rect.x
            && text_clip.y >= pill_rect.y
            && text_clip.x + text_clip.w <= pill_rect.x + pill_rect.w
            && text_clip.y + text_clip.h <= pill_rect.y + pill_rect.h,
        "clock text paint stays inside the animated pill: text={text_clip:?}, pill={pill_rect:?}"
    );
    let outside_x = if pill_right + 4. < 1920. {
        pill_right + 4.
    } else {
        pill_rect.x - 4.
    };
    let outside_y = pill_rect.y + pill_rect.h / 2.;
    assert!(
        (0. ..1920.).contains(&outside_x) && (outside_x < pill_rect.x || outside_x > pill_right)
    );
    assert!(
        !status.contains(outside_x, outside_y),
        "the status child cannot be hit outside its parent pill"
    );
    runtime
        .stage(UiEvent::Click {
            x: outside_x,
            y: outside_y,
        })
        .unwrap();
    runtime.commit();
    assert!(runtime.status_pill.panel().is_none());
    assert!(
        routes.lock().unwrap().is_empty(),
        "the clipped child does not dispatch its status action"
    );

    thread::sleep(Duration::from_millis(220));
    let settled_scene = runtime
        .stage(UiEvent::Model(BTreeMap::new()))
        .unwrap()
        .expect("the settled natural clock width publishes its final frame");
    runtime.commit();
    let settled_clock_text = settled_scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, rect, .. } if text.starts_with("11:59") => {
                Some((text.clone(), *rect))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let clock_reservation = runtime.widget_seed().and_then(|seed| {
        seed["reservations"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["iid"] == "clock"))
            .cloned()
    });
    assert!(
        settled_scene
            .primitives
            .iter()
            .any(|primitive| matches!(primitive,
        bead::render::Primitive::Text {text,..} if text=="11:59 PM")),
        "the full 12-hour clock value fits once the natural-width animation settles; text={settled_clock_text:?}, reservation={clock_reservation:?}"
    );
    assert!(runtime.session.preview_deadline().is_none());
    let settled = pill_width(&runtime);
    assert!(
        pill_rect.w < settled,
        "the final width follows the longer natural content"
    );
    assert!(
        settled > initial_width,
        "the pill reaches its longer natural width"
    );
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
    assert_eq!(
        pill_width(&runtime),
        settled,
        "settled restoration does not change the final width"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn editing_keeps_centered_title_geometry_and_places_nonzero_remove_markers() {
    let dir = std::env::temp_dir().join(format!(
        "widget-title-edit-stability-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    let rect_for = |runtime: &Runtime, suffix: &str| {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|node| node.id.ends_with(suffix))
            .unwrap_or_else(|| panic!("node ending in {suffix} is present"));
        bead::ui::rect(&node.draw)
    };
    let before = rect_for(&runtime, "title-band");
    runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.editing".into(), Value::Bool(true)),
            ("bead.hovered".into(), Value::Text(String::new())),
        ])))
        .unwrap();
    assert!(runtime.commit());
    let after = rect_for(&runtime, "title-band");
    assert!(
        (before.x - after.x).abs() < 0.01
            && (before.w - after.w).abs() < 0.01
            && (before.y - after.y).abs() < 0.01
            && (before.h - after.h).abs() < 0.01,
        "opening Widgets editing must leave the default title centered and the same size"
    );
    let content = rect_for(&runtime, "title-content");
    assert!(
        content.w > 0. && content.h > 0.,
        "the title drag target remains materialized in the Group"
    );

    for hover in ["title", "launcher"] {
        runtime
            .stage(UiEvent::Model(BTreeMap::from([(
                "bead.hovered".into(),
                Value::Text(hover.into()),
            )])))
            .unwrap();
        assert!(runtime.commit());
        let marker = rect_for(&runtime, "remove");
        assert!(
            marker.w > 0. && marker.h > 0.,
            "remove affordance for {hover} has visible geometry"
        );
        if hover == "launcher" {
            let launcher = runtime.session.view_nodes().iter().find_map(|node| {
                matches!(&node.metadata,Some(Value::List(records))
                    if records.iter().any(|record|record["widget_instance"]==true && record["iid"]=="launcher"))
                    .then(||bead::ui::rect(&node.draw))
            }).expect("visible launcher source geometry");
            assert!(
                marker.x + marker.w / 2. <= launcher.x + launcher.w,
                "normal widget remove affordance stays clear of the following widget center"
            );
        }
    }
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "bead.hovered".into(),
            Value::Text("status-pill.clock".into()),
        )])))
        .unwrap();
    assert!(runtime.commit());
    let ids: Vec<_> = runtime
        .session
        .view_nodes()
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    assert!(
        !ids.iter()
            .any(|id| id.ends_with("widget-status-pill.remove")),
        "hovering a child suppresses its parent pill remove affordance"
    );
    assert!(
        ids.iter().any(|id| id.ends_with("widget-clock.remove")),
        "the deepest hovered widget keeps its own remove affordance"
    );
    let _ = std::fs::remove_dir_all(dir);
}
fn widget_app_runtime() -> Runtime {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let mut session = bead::ui::Session::new(
        root.clone(),
        "widgets/app/main.luau".into(),
        palette,
        colors,
        cold,
    );
    session.set_viewport(1200, 800);
    let mut runtime = Runtime::new(session).with_widget_app(None, &root).unwrap();
    runtime.state_file = None;
    runtime.widgets_file = None;
    runtime
}

#[test]
fn enable_entry_source_requires_real_origin_and_uses_the_marked_form_preview() {
    let mut runtime = widget_app_runtime();
    runtime.stage(UiEvent::Reload).unwrap();
    runtime.commit();
    let mut values = BTreeMap::from([
        ("widgets.selected".into(), Value::Text("clock".into())),
        ("widgets.form".into(), Value::Text("desktop".into())),
        ("widgets.screen_width".into(), Value::Number(1920.)),
        ("widgets.screen_height".into(), Value::Number(1080.)),
        ("widgets.drag.app_origin_valid".into(), Value::Bool(false)),
        ("widgets.drag.app_origin_x".into(), Value::Number(40.)),
        ("widgets.drag.app_origin_y".into(), Value::Number(60.)),
        ("widgets.drag.app_width".into(), Value::Number(1200.)),
        ("widgets.drag.app_height".into(), Value::Number(800.)),
        (
            "widgets.drag.app_output_id".into(),
            Value::Text("DP-1".into()),
        ),
    ]);
    runtime.stage(UiEvent::Model(values.clone())).unwrap();
    runtime.commit();
    let argument = Value::list(vec![
        json!({"op":"toggle-form","widget":"clock","group":"clock",
        "form":"desktop","enabled":true,"document":{"version":1,"hosts":{}}}),
    ]);
    assert!(
        runtime.widget_entry_source(&argument).is_none(),
        "an unknown app origin preserves the enable action without producing a flight source"
    );

    values.insert("widgets.drag.app_origin_valid".into(), Value::Bool(true));
    runtime.stage(UiEvent::Model(values)).unwrap();
    runtime.commit();
    let source = runtime
        .widget_entry_source(&argument)
        .expect("the marked desktop preview has a measured source pose");
    assert_eq!(source["form"], "desktop");
    assert_eq!(source["widget"], "clock");
    assert!(source["rect"]["w"].as_f64().unwrap() > 0.);
    assert!(source["rect"]["h"].as_f64().unwrap() > 0.);
    assert_eq!(source["app_output_id"], "DP-1");

    let bar_document = json!({"version":1,"preset":"Custom","hosts":{"top_bar":{"mode":"content","tiles":60,
        "regions":{"left":{"start":0,"span":20},"centre":{"start":20,"span":20},"right":{"start":40,"span":20}},"instances":[]}}});
    runtime
        .stage(UiEvent::Model(BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![bar_document.clone()]),
            ),
            ("widgets.selected".into(), Value::Text("title".into())),
            ("widgets.group".into(), Value::Text("title".into())),
            ("widgets.form".into(), Value::Text("bar".into())),
        ])))
        .unwrap();
    runtime.commit();
    let bar_request = Value::list(vec![
        json!({"op":"toggle-form","widget":"title","group":"title",
        "form":"bar","enabled":true,"document":bar_document}),
    ]);
    let bar_source = runtime
        .widget_entry_source(&bar_request)
        .expect("the configured bar stage candidate has a measured source pose");
    assert_eq!(bar_source["form"], "bar");
    assert_eq!(bar_source["widget"], "title");
    assert!(bar_source["rect"]["w"].as_f64().unwrap() > 0.);
}

#[test]
fn enable_entry_preview_places_clock_in_its_configured_pill_slot() {
    let mut runtime = widget_app_runtime();
    runtime.stage(UiEvent::Reload).unwrap();
    runtime.commit();
    let mut document = clock_pill_document();
    document["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["iid"] == "status-pill")
        .unwrap()["children"] = json!([]);
    document["widgets"] = json!({"clock-main":{"widget":"clock","settings":{},"forms":{
        "bar":{"enabled":false,"style":"time","placement":{"host":"status-pill","slot":{"region":"centre","start":2,"span":2}}}
    }}});
    runtime
        .stage(UiEvent::Model(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![document.clone()])),
            ("widgets.selected".into(), Value::Text("clock".into())),
            ("widgets.group".into(), Value::Text("clock-main".into())),
            ("widgets.form".into(), Value::Text("bar".into())),
            ("widgets.drag.app_origin_valid".into(), Value::Bool(true)),
            ("widgets.drag.app_origin_x".into(), Value::Number(40.)),
            ("widgets.drag.app_origin_y".into(), Value::Number(60.)),
            ("widgets.drag.app_width".into(), Value::Number(1200.)),
            ("widgets.drag.app_height".into(), Value::Number(800.)),
            (
                "widgets.drag.app_output_id".into(),
                Value::Text("DP-1".into()),
            ),
        ])))
        .unwrap();
    runtime.commit();

    let request = Value::list(vec![
        json!({"op":"toggle-form","widget":"clock","group":"clock-main",
        "form":"bar","enabled":true,"document":document}),
    ]);
    let source = runtime
        .widget_entry_source(&request)
        .expect("the configured pill child renders a measured source node");
    assert_eq!(source["widget"], "clock");
    assert!(source["rect"]["w"].as_f64().unwrap() > 0.);
    assert!(source["rect"]["h"].as_f64().unwrap() > 0.);
    let nodes = runtime.session.view_nodes();
    let source_node = nodes
        .iter()
        .find(|node| {
            node.metadata.as_ref().is_some_and(|metadata| {
                matches!(metadata,Value::List(records)
                    if records.iter().any(|record| record["widget_entry_source"]==true
                        && record["group"]=="clock-main"))
            })
        })
        .unwrap_or_else(|| {
            let candidates = nodes
                .iter()
                .filter(|node| node.id.contains("pill"))
                .map(|node| (node.id.as_str(), bead::ui::rect(&node.draw)))
                .collect::<Vec<_>>();
            panic!(
                "the configured clock source has an actual rendered node; candidates={candidates:?}"
            )
        });
    assert!(
        source_node.id.contains("system-pill"),
        "the measured clock source path identifies its configured pill: {}",
        source_node.id
    );
    let source_node_rect = bead::ui::rect(&source_node.draw);
    let source_rect = &source["rect"];
    assert!((source_rect["x"].as_f64().unwrap() - f64::from(source_node_rect.x)).abs() < 0.01);
    assert!((source_rect["y"].as_f64().unwrap() - f64::from(source_node_rect.y)).abs() < 0.01);
    assert!((source_rect["w"].as_f64().unwrap() - f64::from(source_node_rect.w)).abs() < 0.01);
    assert!((source_rect["h"].as_f64().unwrap() - f64::from(source_node_rect.h)).abs() < 0.01);
}

#[test]
fn entry_ack_and_timer_guards_reject_stale_write_closed_app_and_new_gesture() {
    let document = json!({"version":1,"hosts":{"top_bar":{"instances":[]}}});
    let entry = super::WidgetEntryWrite {
        event: json!({"accepted_revision":9}),
        document: document.clone(),
        previous_revision: 8,
    };
    assert!(super::widget_entry_ack_matches(
        &entry,
        true,
        9,
        Some(&document)
    ));
    assert!(!super::widget_entry_ack_matches(
        &entry,
        false,
        9,
        Some(&document)
    ));
    assert!(!super::widget_entry_ack_matches(
        &entry,
        true,
        10,
        Some(&document)
    ));
    assert!(!super::widget_entry_ack_matches(
        &entry,
        true,
        9,
        Some(&json!({}))
    ));

    let flight = super::WidgetEntryFlight {
        event: json!({"entry_epoch":12}),
        app: (41, 7),
        token: 12,
        phase: super::WidgetEntryTimerPhase::Seed,
    };
    assert!(super::widget_entry_flight_continues(
        &flight,
        Some((41, 7)),
        Some(12)
    ));
    assert!(!super::widget_entry_flight_continues(
        &flight,
        None,
        Some(12)
    ));
    assert!(!super::widget_entry_flight_continues(
        &flight,
        Some((41, 7)),
        Some(13)
    ));
    assert!(super::widget_entry_outputs_match(Some("DP-1"), "DP-1"));
    assert!(!super::widget_entry_outputs_match(Some("DP-2"), "DP-1"));

    let runtime = widget_app_runtime();
    assert!(
        !runtime.schedule_widget_entry_timer(1, std::time::Duration::ZERO),
        "missing wakeup rejects the animation timer so the presentation can be retired"
    );
}

#[test]
fn entry_seed_flight_and_retire_reuse_drag_overlay_without_writing_or_history() {
    let canonical = document();
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([
            (
                "widgets.layout".into(),
                Value::list(vec![canonical.clone()]),
            ),
            ("widgets.rev".into(), Value::Number(4.)),
        ]))
        .unwrap();
    let entry = json!({
        "phase":"entry-seed","private_remote":true,"target_iid":"clock-main",
        "target_form":"bar","target_output_id":"right","target_width":72,"target_height":50,
        "app_origin_x":40,"app_origin_y":60,"app_origin_valid":true,"app_width":1200,"app_height":800,
        "app_output_id":"right","source_origin_x":40,"source_origin_y":60,"source_origin_valid":true,
        "x":180,"y":80,"width":48,"height":24,"grab_x":24,"grab_y":12,"snap_x":960,"snap_y":0,
        "document":canonical,
        "source":[{"app":true,"kind":"entry","widget":"clock","group":"clock-main","form":"bar",
            "style":"time","settings":{"future":"kept"}}]
    });
    let seeded = controller.drag(Value::list(vec![entry.clone()])).unwrap();
    assert_eq!(seeded["widgets.drag.active"], Value::Bool(true));
    assert_eq!(seeded["widgets.drag.phase"], Value::Text("dragging".into()));
    assert_eq!(
        doc_from(&seeded, "widgets.drag.layout")["hosts"]["top_bar"]["instances"][0]["drag_placeholder"],
        true
    );
    assert_eq!(doc_from(&seeded, "widgets.layout"), canonical);
    assert!(matches!(&seeded["widgets.write"],Value::List(rows) if rows.is_empty()));
    assert!(matches!(&seeded["widgets.undo"],Value::List(rows) if rows.is_empty()));

    let Value::Number(epoch) = &seeded["widgets.drag.epoch"] else {
        panic!("drag epoch is numeric")
    };
    let epoch = *epoch;
    let mut flight = entry;
    flight["phase"] = json!("entry-flight");
    flight["entry_epoch"] = json!(epoch);
    let settling = controller.drag(Value::list(vec![flight])).unwrap();
    assert_eq!(settling["widgets.drag.phase"], Value::Text("settle".into()));
    assert_eq!(
        settling["widgets.drag.entry.target_width"],
        Value::Number(72.)
    );
    assert!(matches!(&settling["widgets.write"],Value::List(rows) if rows.is_empty()));
    let retired = controller
        .drag(Value::list(vec![json!({"phase":"retire","epoch":epoch})]))
        .unwrap();
    assert_eq!(retired["widgets.drag.active"], Value::Bool(false));
    assert!(matches!(&retired["widgets.write"],Value::List(rows) if rows.is_empty()));
}

#[test]
fn temporary_widget_warm_runtime_is_side_effect_free_and_real_commit_announces_once() {
    let mut warm = widget_app_runtime();
    warm.warm_frame().unwrap();
    assert!(
        !warm.widget_drag_app_announced,
        "the temporary warm runtime does not publish open state"
    );
    drop(warm);

    let mut actual = widget_app_runtime();
    let accepted = actual.warm_frame().unwrap();
    assert!(!actual.widget_drag_app_announced);
    actual.pending = Some(accepted.clone());
    assert!(actual.commit());
    assert!(
        actual.widget_drag_app_announced,
        "the actual accepted scene announces the app"
    );
    actual.pending = Some(accepted);
    assert!(actual.commit());
    assert!(
        actual.widget_drag_app_announced,
        "subsequent commits keep the single open lifecycle"
    );
}
fn launcher_transfer(phase: &str, x: f64) -> Json {
    json!({"phase":phase,"private_remote":true,"target_output_id":"right",
        "source_origin_valid":true,"source_origin_x":0,"source_origin_y":0,"app_width":0,"app_height":0,
        "global_x":x,"global_y":20,"x":x,"y":20,"screen_width":1920,"screen_height":1080,
        "start_x":x,"start_y":20,"width":32,"height":36,"grab_x":8,"grab_y":8,
        "source":[{"app":true,"iid":"launcher","group":"launcher","widget":"launcher","host":"top_bar","form":"bar","style":"nacre",
            "settings":{"icon":"nacre","custom_path":"","action":"launcher","route":""}}]})
}

#[test]
fn real_bar_session_receives_live_transient_neighbor_reflow() {
    let dir = std::env::temp_dir().join(format!("widget-drag-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    let transfer = launcher_transfer("move", 300.);
    let result = runtime.stage(UiEvent::MotionAction {
        name: "widgets.drag".into(),
        argument: Value::list(vec![transfer]),
    });
    assert!(
        result.is_ok(),
        "actual bar view must accept the transient document before any write: {}",
        result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );
    assert!(
        result.unwrap().is_some(),
        "reflow must stage a visible frame"
    );
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("widgets.drag.phase"),
        Some(&Value::Text("dragging".into()))
    );
    let Some(Value::List(documents)) = runtime.session.stores.get("widgets.drag.layout") else {
        panic!("transient document projected into bar session")
    };
    let instances = documents[0]["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap();
    assert!(instances.iter().any(|item| item["iid"] == "launcher"));
    assert!(
        instances
            .iter()
            .any(|item| item["drag_placeholder"] == true)
    );
    assert!(
        !dir.join("widgets.json").exists(),
        "motion never writes the canonical layout"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn canvas_retire_projection_keeps_current_host_document_and_live_data() {
    let dir = std::env::temp_dir().join(format!("widget-canvas-retire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut runtime = real_bar_runtime(dir.join("widgets.json"));
    let accepted = json!({"version":1,"hosts":{"top_bar":{"mode":"tiles","tiles":60,"regions":{
        "left":{"start":0,"span":20},"centre":{"start":20,"span":20},"right":{"start":40,"span":20}
    },"instances":[]},"canvas":{"width":1920,"height":1080,"instances":[
        {"iid":"clock-desktop","widget":"clock","host":"canvas","slot":{"x":200,"y":100,"w":240,"h":96}}
    ]}}});
    let stale = document();
    runtime
        .session
        .stores
        .commit(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![accepted.clone()])),
            ("widgets.rev".into(), Value::Number(9.)),
            ("widgets.draft".into(), Value::list(Vec::new())),
            ("widgets.editing".into(), Value::Bool(false)),
            ("bead.desktop.clock".into(), Value::Text("14:32".into())),
        ]))
        .unwrap();
    runtime.widget_drag_model = Some(BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![stale.clone()])),
        ("widgets.rev".into(), Value::Number(2.)),
        ("widgets.draft".into(), Value::list(vec![stale])),
        ("widgets.error".into(), Value::Text("stale model".into())),
        ("bead.desktop.clock".into(), Value::Text("09:41".into())),
        ("widgets.drag.active".into(), Value::Bool(false)),
        ("widgets.drag.phase".into(), Value::Text(String::new())),
        ("widgets.drag.layout".into(), Value::list(Vec::new())),
    ]));
    runtime.widgets_canvas_wanted = true;
    runtime.widget_canvas_projection = Some(BTreeMap::new());
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = events.clone();
    runtime.wakeup = Some(std::sync::Arc::new(move |event| {
        sink.lock().unwrap().push(event)
    }));
    runtime.publish_widget_models();
    let events = events.lock().unwrap();
    assert!(!events.iter().any(|event|matches!(event,UiEvent::CanvasOffer{view,open:false,..} if view=="widgets-canvas")),
        "retiring an old controller model does not close/reopen the accepted canvas");
    let canvas = events
        .iter()
        .find_map(|event| match event {
            UiEvent::CanvasModel { view, values, .. } if view == "widgets-canvas" => Some(values),
            _ => None,
        })
        .expect("current canvas state is published");
    assert_eq!(doc_from(canvas, "widgets.layout"), accepted);
    assert_eq!(canvas["widgets.rev"], Value::Number(9.));
    assert!(matches!(&canvas["widgets.draft"],Value::List(rows) if rows.is_empty()));
    assert_eq!(canvas["bead.desktop.clock"], Value::Text("14:32".into()));
    assert!(
        canvas.get("widgets.error") == Some(&Value::Text(String::new())),
        "the host's empty error is retained instead of stale controller feedback"
    );
    drop(events);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn native_widget_edit_action_uses_the_canonical_save_path() {
    let dir = std::env::temp_dir().join(format!("widget-edit-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "widgets.editing".into(),
            Value::Bool(true),
        )]))
        .unwrap();
    let request = json!({"op":"toggle-form","widget":"launcher","group":"launcher","form":"bar","enabled":false});
    runtime
        .stage(UiEvent::Action {
            name: "widgets.edit".into(),
            argument: Value::list(vec![request]),
        })
        .unwrap();
    let saved: Json = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(
        !saved["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["iid"] == "launcher")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn synchronous_native_remove_commits_saved_layout_into_the_real_bar_session() {
    let dir = std::env::temp_dir().join(format!("widget-sync-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "widgets.editing".into(),
            Value::Bool(true),
        )]))
        .unwrap();
    runtime.stage(UiEvent::Action{name:"widgets.edit".into(),argument:Value::list(vec![json!({
        "op":"toggle-form","widget":"launcher","group":"launcher","form":"bar","enabled":false
    })])}).unwrap();
    assert!(
        runtime.commit(),
        "the accepted post-write scene becomes the committed frame"
    );
    let layout = doc_from(runtime.session.stores.values(), "widgets.layout");
    assert!(
        !layout["hosts"]["top_bar"]["instances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["widget"] == "launcher")
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.draft"),Some(Value::List(rows)) if rows.is_empty())
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.undo"),Some(Value::List(rows)) if rows.len()==1)
    );
    assert_eq!(
        serde_json::from_slice::<Json>(&std::fs::read(file).unwrap()).unwrap(),
        layout
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn synchronous_native_remove_save_failure_restores_visible_default_and_reports_error() {
    let dir = std::env::temp_dir().join(format!("widget-sync-failure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let blocker = dir.join("not-a-directory");
    std::fs::write(&blocker, b"x").unwrap();
    let expected_error = std::fs::create_dir_all(&blocker).unwrap_err().to_string();
    let file = blocker.join("widgets.json");
    let mut runtime = real_bar_runtime(file);
    runtime
        .session
        .stores
        .commit(BTreeMap::from([(
            "widgets.editing".into(),
            Value::Bool(true),
        )]))
        .unwrap();
    runtime.stage(UiEvent::Action{name:"widgets.edit".into(),argument:Value::list(vec![json!({
        "op":"toggle-form","widget":"launcher","group":"launcher","form":"bar","enabled":false
    })])}).unwrap();
    assert!(
        runtime.commit(),
        "the rollback/error scene is committed after the filesystem rejects the write"
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.layout"),Some(Value::List(rows)) if rows.is_empty()),
        "the prior unpersisted canonical state remains empty"
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.draft"),Some(Value::List(rows)) if rows.is_empty())
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.undo"),Some(Value::List(rows)) if rows.is_empty()),
        "the rejected provisional edit does not leak into history"
    );
    assert!(
        matches!(runtime.session.stores.get("widgets.error"),Some(Value::Text(error)) if error==&expected_error),
        "the actual filesystem error is visible"
    );
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .any(|node| node.id == "brand" || node.id.ends_with(".brand")),
        "the launcher remains rendered from the initial layout"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_real_bar_preview_never_reaches_file_persistence() {
    let dir = std::env::temp_dir().join(format!("widget-drag-preflight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    let seed = runtime.widget_seed().unwrap();
    let descriptors = seed["catalogue"]
        .as_array()
        .expect("native host seed includes JSON-safe widget descriptors");
    let launcher = descriptors
        .iter()
        .find(|item| item["id"] == "launcher")
        .expect("launcher descriptor");
    assert!(
        launcher["forms"]["bar"]["styles"]
            [launcher["forms"]["bar"]["default_style"].as_str().unwrap()]["preferred_span"]
            .is_number()
    );
    let mut invalid = seed["document"].clone();
    invalid["hosts"]["top_bar"]["instances"][1]["style"] = json!("missing-style");
    runtime
        .session
        .stores
        .commit(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![invalid.clone()])),
            ("widgets.draft".into(), Value::list(vec![invalid])),
        ]))
        .unwrap();
    let transfer = launcher_transfer("drop", 300.);
    let _ = runtime.stage(UiEvent::Action {
        name: "widgets.drag".into(),
        argument: Value::list(vec![transfer]),
    });
    assert!(
        !file.exists(),
        "view validation must finish before any layout file write"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn desktop_resize_previews_then_commits_one_edit_and_cancel_restores_geometry() {
    let mut initial = document();
    initial["hosts"]["canvas"] = json!({"width":1280,"height":720,"instances":[{
        "iid":"clock-desktop","group":"clock-main","widget":"clock","host":"canvas","style":"time",
        "slot":{"x":100,"y":100,"w":240,"h":100},"settings":{"future":"kept"}
    }]});
    let mut context = context();
    context["catalogue"][0]["forms"]["desktop"] = json!({"default_style":"time","styles":{
        "time":{"min_span":80,"max_span":600,"preferred_span":240}
    }});
    let source = json!({"kind":"resize","iid":"clock-desktop","group":"clock-main","widget":"clock",
        "host":"canvas","form":"desktop","style":"time","min_height":32,"max_height":720,
        "slot":{"x":100,"y":100,"w":240,"h":100}});
    let transfer = |phase: &str| {
        Value::list(vec![json!({"phase":phase,"source":[source.clone()],
        "destination":"desktop","x":300,"y":250,"start_x":200,"start_y":200,
        "width":24,"height":24,"context":context.clone()})])
    };
    let mut controller = Controller::new().unwrap();
    controller
        .sync(&BTreeMap::from([(
            "widgets.layout".into(),
            Value::list(vec![initial.clone()]),
        )]))
        .unwrap();
    let preview = controller.drag(transfer("move")).unwrap();
    let resized = doc_from(&preview, "widgets.drag.layout");
    assert_eq!(
        resized["hosts"]["canvas"]["instances"][0]["slot"],
        json!({"x":100,"y":100,"w":340,"h":150})
    );
    assert!(matches!(&preview["widgets.write"],Value::List(rows) if rows.is_empty()));
    let saved = controller.drag(transfer("drop")).unwrap();
    let written = doc_from(&saved, "widgets.write");
    assert_eq!(
        written["hosts"]["canvas"]["instances"][0]["slot"],
        resized["hosts"]["canvas"]["instances"][0]["slot"]
    );
    assert_eq!(
        written["hosts"]["canvas"]["instances"][0]["settings"]["future"],
        "kept"
    );
    assert!(matches!(&saved["widgets.undo"],Value::List(rows) if rows.len()==1));
    let mut cancelled = Controller::new().unwrap();
    cancelled
        .sync(&BTreeMap::from([(
            "widgets.layout".into(),
            Value::list(vec![initial]),
        )]))
        .unwrap();
    cancelled.drag(transfer("move")).unwrap();
    let returned = cancelled.drag(transfer("cancel")).unwrap();
    assert_eq!(
        doc_from(&returned, "widgets.drag.layout")["hosts"]["canvas"]["instances"][0]["slot"]["w"],
        240
    );
    assert!(matches!(&returned["widgets.write"],Value::List(rows) if rows.is_empty()));
}

#[test]
fn invalid_native_edit_never_reaches_file_persistence() {
    let dir = std::env::temp_dir().join(format!("widget-edit-preflight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("widgets.json");
    let mut runtime = real_bar_runtime(file.clone());
    let mut invalid = runtime.widget_seed().unwrap()["document"].clone();
    invalid["hosts"]["top_bar"]["instances"][1]["style"] = json!("missing-style");
    runtime
        .session
        .stores
        .commit(BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![invalid.clone()])),
            ("widgets.draft".into(), Value::list(vec![invalid])),
        ]))
        .unwrap();
    let _ = runtime.stage(UiEvent::Action {
        name: "widgets.edit".into(),
        argument: Value::list(vec![json!({
            "op":"toggle-form","widget":"launcher","group":"launcher","form":"bar","enabled":false
        })]),
    });
    assert!(
        !file.exists(),
        "native edit validation must finish before any file write"
    );
    assert!(runtime.widgets_write.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn native_widget_write_resolution_does_not_send_app_busy_to_bar() {
    let changes = super::widget_write_resolution(None, None, None, None, BTreeMap::new(), false);
    assert!(!changes.contains_key("widgets.busy"));

    let dir = std::env::temp_dir().join(format!("widget-write-resolution-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut bar = real_bar_runtime(dir.join("widgets.json"));
    bar.session
        .update(changes)
        .expect("native write completion values match the real bar schema");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn entry_preview_projection_keeps_controller_target_size() {
    let dir = std::env::temp_dir().join(format!("widget-entry-projection-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut bar = real_bar_runtime(dir.join("widgets.json"));
    bar.widget_drag_model = Some(BTreeMap::from([
        (
            "widgets.drag.entry.target_width".into(),
            Value::Number(320.),
        ),
        (
            "widgets.drag.entry.target_height".into(),
            Value::Number(96.),
        ),
        ("widgets.drag.revision".into(), Value::Number(91.)),
        ("widgets.base_rev".into(), Value::Number(19.)),
        (
            "widgets.drag.app_output_id".into(),
            Value::Text("private".into()),
        ),
    ]));
    let values = bar.widget_drag_preview_values();
    assert_eq!(
        values["widgets.drag.entry.target_width"],
        Value::Number(320.)
    );
    assert_eq!(
        values["widgets.drag.entry.target_height"],
        Value::Number(96.)
    );
    for key in [
        "widgets.drag.revision",
        "widgets.base_rev",
        "widgets.drag.app_output_id",
    ] {
        assert!(
            !values.contains_key(key),
            "private controller field leaked: {key}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn resized_widget_app_gallery_launcher_accepts_a_real_pointer_click() {
    let mut runtime = widget_app_runtime();
    runtime.stage(UiEvent::Reload).unwrap();
    runtime.commit();
    runtime.viewport(926, 1108);
    runtime.stage(UiEvent::Relayout).unwrap();
    runtime.commit();
    let region = runtime
        .input_regions()
        .into_iter()
        .find(|region| region.id.ends_with("widget-card-launcher"))
        .expect("launcher card is present after window configure");
    let rect = region
        .clip
        .map_or(region.rect, |clip| region.rect.intersect(clip));
    let x = rect.x + rect.w / 2.;
    let y = rect.y + rect.h / 2.;
    runtime.stage(UiEvent::Hover { x, y }).unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Click { x, y }).unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("widgets.selected"),
        Some(&Value::Text("launcher".into())),
        "a card click must select the launcher in the real app Runtime"
    );
}

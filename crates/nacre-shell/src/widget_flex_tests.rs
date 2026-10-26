// SPDX-License-Identifier: GPL-3.0-only
//! Flexible bar widget sizing stays stable within reservations and clips narrow content.
use bead::core::Value;
use bead_test::{Config, Harness};
use serde_json::{Value as Json, json};
use std::{
    collections::BTreeMap,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

fn document(mode: &str) -> Json {
    let mut pill = json!({
        "iid":"status-pill","widget":"pill","host":"top_bar","style":"pill",
        "slot":{"region":"right","start":0,"span":1},"settings":{},
        "children":[
            {"iid":"updates-in-pill","widget":"updates","host":"status-pill","style":"icon+value",
                "slot":{"region":"left","start":0,"span":2},"settings":{}},
            {"iid":"clock-in-pill","widget":"clock","host":"status-pill","style":"seconds",
                "slot":{"region":"right","start":0,"span":2},"settings":{}}
        ]
    });
    if mode == "tiles" {
        pill["slot"]["span"] = json!(12);
    }
    let title_span = if mode == "tiles" { 2 } else { 1 };
    let notifications_span = if mode == "tiles" { 2 } else { 1 };
    let clock_left_start = notifications_span;
    json!({
        "version":1,"preset":"Custom","dropdown_trigger":"click",
        "hosts":{"top_bar":{"mode":mode,"tiles":60,"regions":{
            "left":{"start":0,"span":60},"centre":{"start":0,"span":60},"right":{"start":0,"span":60}
        },"instances":[
            {"iid":"notifications-main","widget":"notifications","host":"top_bar","style":"icon+value",
                "slot":{"region":"left","start":0,"span":notifications_span},"settings":{}},
            {"iid":"clock-left","widget":"clock","host":"top_bar","style":"time",
                "slot":{"region":"left","start":clock_left_start,"span":4},"settings":{}},
            {"iid":"title-main","widget":"title","host":"top_bar","style":"window-title",
                "slot":{"region":"centre","start":0,"span":title_span},
                "settings":{"min_width":0,"max_width":240,"display":"window title","behavior":"none"}},
            pill
        ]}}
    })
}

fn harness(doc: Json, title: &str, count: u64) -> Harness {
    crate::i18n::init();
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.extend(crate::notifications::values(&json!({"rows":[]})));
    stores.extend(BTreeMap::from([
        ("widgets.layout".into(), Value::list(vec![doc])),
        ("widgets.draft".into(), Value::list(Vec::<Json>::new())),
        (
            "widgets.geometry".into(),
            Value::list(vec![
                json!({"iid":"status-pill","x":1600,"y":0,"w":200,"h":34}),
                json!({"iid":"updates-in-pill","x":1600,"y":0,"w":40,"h":34}),
                json!({"iid":"clock-in-pill","x":1760,"y":0,"w":60,"h":34}),
                json!({"iid":"title-main","x":800,"y":0,"w":80,"h":34}),
                json!({"iid":"notifications-main","x":0,"y":0,"w":50,"h":34}),
            ]),
        ),
        ("widgets.editing".into(), Value::Bool(false)),
        ("widgets.rev".into(), Value::Number(1.)),
        ("widgets.selected".into(), Value::Text(String::new())),
        ("widgets.preview".into(), Value::list(Vec::<Json>::new())),
        ("bar.height".into(), Value::Number(50.)),
        ("bar.reconnecting".into(), Value::Bool(false)),
        ("bar.launcher".into(), Value::Bool(false)),
        ("bar.popouts".into(), Value::Bool(false)),
        ("bar.workspace.enabled".into(), Value::Bool(false)),
        ("bar.workspace.count".into(), Value::Number(6.)),
        (
            "bar.workspace.icons".into(),
            Value::list(Vec::<Json>::new()),
        ),
        ("bar.tray.enabled".into(), Value::Bool(false)),
        ("bar.tray.visible".into(), Value::Bool(false)),
        ("bar.dashboard.enabled".into(), Value::Bool(false)),
        ("bar.window.focus.enabled".into(), Value::Bool(false)),
        ("bar.updates.enabled".into(), Value::Bool(true)),
        ("bar.updates.count".into(), Value::Number(count as f64)),
        ("bar.updates.text".into(), Value::Text(count.to_string())),
        (
            "bar.notifications.count".into(),
            Value::Number(count as f64),
        ),
        (
            "bar.notifications.text".into(),
            Value::Text(count.min(999).to_string()),
        ),
        ("bar.clock.twelve".into(), Value::Bool(false)),
        ("bar.clock.text".into(), Value::Text("09:08".into())),
        ("bar.title".into(), Value::Text(title.into())),
        ("bar.title.cue.visible".into(), Value::Bool(false)),
        ("bar.title.cue.fade".into(), Value::Number(0.)),
        ("bar.title.cue.halo".into(), Value::Number(0.)),
        ("bar.title.cue.body".into(), Value::Number(0.)),
        ("bar.title.cue.border".into(), Value::Number(0.)),
        ("bead.focused_title".into(), Value::Text(title.into())),
        ("bead.desktop.titlevisible".into(), Value::Bool(true)),
        ("bead.desktop.clock".into(), Value::Text("09:08".into())),
        (
            "bead.desktop.clock.seconds".into(),
            Value::Text("09:08:07".into()),
        ),
        (
            "bead.desktop.clock.date".into(),
            Value::Text("2026-10-25".into()),
        ),
        ("bead.desktop.menu".into(), Value::list(Vec::<Json>::new())),
        ("bead.desktop.tray".into(), Value::list(Vec::<Json>::new())),
        ("bead.edge.dwell".into(), Value::Number(0.)),
        ("bead.edge.fade".into(), Value::Number(0.)),
        ("bead.windows".into(), Value::list(Vec::<Json>::new())),
        ("bead.workspaces".into(), Value::list(Vec::<Json>::new())),
        ("bead.focused_title".into(), Value::Text(title.into())),
    ]));
    Harness::load(
        &root(),
        Path::new("bar/main.luau"),
        &root().join("palette.json"),
        stores,
        Config {
            width: 1920,
            height: 70,
            scale: 1.,
        },
    )
    .expect("bar harness loads")
}

fn seed_value(harness: &Harness, key: &str) -> Json {
    harness
        .session
        .view_nodes()
        .iter()
        .find_map(|node| {
            let Some(Value::List(records)) = &node.metadata else {
                return None;
            };
            records.first().map(|seed| seed[key].clone())
        })
        .expect("bar Host publishes widget seed metadata")
}

fn reservations(harness: &Harness) -> BTreeMap<String, f64> {
    let values = seed_value(harness, "reservations");
    let Json::Array(values) = values else {
        panic!("reservations are a list")
    };
    values
        .iter()
        .filter_map(|row| Some((row["iid"].as_str()?.to_owned(), row["width"].as_f64()?)))
        .collect()
}

fn preferred_clock_seconds_span(harness: &Harness) -> f64 {
    let catalogue = seed_value(harness, "catalogue");
    let Json::Array(packages) = catalogue else {
        panic!("catalogue is a list")
    };
    packages
        .iter()
        .find(|package| package["id"] == "clock")
        .and_then(|package| package["forms"]["bar"]["styles"]["seconds"]["preferred_span"].as_f64())
        .expect("clock seconds form publishes a preferred span")
}

fn text_rect(harness: &Harness, label: &str) -> bead::render::Rect {
    harness
        .scene()
        .primitives
        .iter()
        .find_map(|primitive| match primitive {
            bead::render::Primitive::Text { rect, text, .. } if text == label => Some(*rect),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing text {label:?}"))
}

fn settle_animation(harness: &mut Harness) {
    let started = Instant::now();
    while harness.session.preview_deadline().is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "clock width animation did not settle"
        );
        thread::sleep(Duration::from_millis(20));
        harness
            .update(BTreeMap::new())
            .expect("animation frame updates");
    }
}

fn clock_widget_width(harness: &Harness) -> f32 {
    let matches = harness
        .session
        .view_nodes()
        .iter()
        .filter(|node| matches!(&node.metadata,Some(Value::List(records))
            if records.iter().any(|record| record["widget_instance"]==true && record["iid"]=="clock-in-pill")))
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "one clock widget root must expose its measured draw rectangle"
    );
    bead::ui::rect(&matches[0].draw).w
}

fn node_rect_and_clip(
    harness: &Harness,
    suffix: &str,
) -> (bead::render::Rect, Option<bead::render::Rect>) {
    let nodes = harness.session.view_nodes();
    let node = nodes
        .iter()
        .find(|node| node.id.ends_with(suffix))
        .unwrap_or_else(|| {
            let ids = nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>();
            panic!("requested centered title node {suffix} exists; nodes={ids:?}")
        });
    (bead::ui::rect(&node.draw), node.clip)
}

fn title_text_rect_and_clip(harness: &Harness) -> (bead::render::Rect, Option<bead::render::Rect>) {
    let scene = harness.scene();
    let index = scene
        .primitives
        .iter()
        .position(|primitive| {
            matches!(primitive,
            bead::render::Primitive::Text { text, .. } if text.starts_with("A "))
        })
        .expect("centered title text primitive exists");
    let bead::render::Primitive::Text { rect, .. } = &scene.primitives[index] else {
        unreachable!("the selected title primitive is text")
    };
    (*rect, scene.clips.get(&index).copied())
}

#[test]
fn nested_pill_includes_child_growth_and_content_changes_do_not_move_neighbors() {
    let content = document("content");
    let first = harness(content.clone(), "Notes", 4);
    let second = harness(content, "A much longer active window title", 100_000);
    let first_reservations = reservations(&first);
    let second_reservations = reservations(&second);
    assert_eq!(
        first_reservations, second_reservations,
        "text and count changes inside max reservations must not alter slot geometry"
    );
    let child_extra = ["updates-in-pill", "clock-in-pill"]
        .iter()
        .map(|iid| {
            let actual = if *iid == "updates-in-pill" { 40. } else { 60. };
            (first_reservations[*iid] - actual).max(0.)
        })
        .sum::<f64>();
    assert_eq!(
        first_reservations["status-pill"],
        200. + child_extra,
        "the parent pill reserves its measured width plus every child's max-width delta"
    );
    assert!(
        preferred_clock_seconds_span(&first)
            >= (first_reservations["clock-in-pill"] / 32.).ceil(),
        "clock seconds span covers at least the default clock reservation"
    );
    assert_eq!(
        text_rect(&first, "09:08").x,
        text_rect(&second, "09:08").x,
        "default content-mode neighbor stays in place as notification count and title grow"
    );
    assert_eq!(
        text_rect(&first, "package_2").x,
        text_rect(&second, "package_2").x,
        "default content-mode pill child stays in place as counts and title grow"
    );

    let tiles = document("tiles");
    let short = harness(tiles.clone(), "Notes", 4);
    let long = harness(tiles, "A much longer active window title", 100_000);
    assert_eq!(
        text_rect(&short, "notifications").x,
        text_rect(&long, "notifications").x,
        "neighbor icon position is stable as title and count values grow"
    );
    assert_eq!(
        text_rect(&short, "package_2").x,
        text_rect(&long, "package_2").x,
        "nested pill child position is stable inside its reserved parent"
    );
    assert!(long.scene().primitives.iter().enumerate().any(|(index, primitive)| {
        matches!(primitive, bead::render::Primitive::Text { text, .. } if text.ends_with('…'))
            && long.scene().clips.contains_key(&index)
    }), "narrow title/count text is ellipsized within a clipped slot");
}

#[test]
fn seconds_and_date_clock_widths_animate_to_natural_size_and_stop() {
    let seconds_doc = document("content");
    let mut seconds = harness(seconds_doc, "Notes", 4);
    let reserved = reservations(&seconds)["clock-in-pill"];
    let seconds_start = text_rect(&seconds, "09:08:07").w;
    assert!(
        seconds_start < reserved as f32,
        "seconds display should use natural width within its max reservation"
    );
    seconds
        .update(BTreeMap::from([(
            "bar.clock.text".into(),
            Value::Text("1:2".into()),
        )]))
        .expect("shorter clock value updates");
    thread::sleep(Duration::from_millis(36));
    seconds
        .update(BTreeMap::new())
        .expect("intermediate animation frame");
    let seconds_mid = text_rect(&seconds, "1:2:07").w;
    settle_animation(&mut seconds);
    let seconds_end = text_rect(&seconds, "1:2:07").w;
    assert!(
        seconds_mid < seconds_start && seconds_mid > seconds_end,
        "seconds width moves through an intermediate size before settling"
    );
    assert!(
        seconds.session.preview_deadline().is_none(),
        "settled animation has no further frame deadline"
    );

    let mut date_doc = document("content");
    let pill = date_doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["widget"] == "pill")
        .unwrap();
    pill["children"][1]["style"] = json!("time+date");
    let mut date = harness(date_doc, "Notes", 4);
    let date_start = clock_widget_width(&date);
    date.update(BTreeMap::from([
        ("bar.clock.text".into(), Value::Text("9:08".into())),
        (
            "bead.desktop.clock.date".into(),
            Value::Text("26-1-1".into()),
        ),
    ]))
    .expect("shorter date and time update");
    thread::sleep(Duration::from_millis(36));
    date.update(BTreeMap::new())
        .expect("intermediate date animation frame");
    let date_mid = clock_widget_width(&date);
    settle_animation(&mut date);
    let date_end = clock_widget_width(&date);
    assert!(
        date_mid < date_start && date_mid > date_end,
        "date/time row moves through an intermediate natural width before settling"
    );
    assert!(
        date.session.preview_deadline().is_none(),
        "settled date animation has no further frame deadline"
    );
}

#[test]
fn centered_title_clamps_min_to_max_and_clips_during_max_shrink_animation() {
    let mut doc = document("content");
    {
        let title = doc["hosts"]["top_bar"]["instances"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|item| item["iid"] == "title-main")
            .unwrap();
        title["settings"]["min_width"] = json!(300);
        title["settings"]["max_width"] = json!(400);
    }
    let mut bar = harness(doc.clone(), "A much longer active window title", 4);
    settle_animation(&mut bar);
    assert!(node_rect_and_clip(&bar, ".title-content").0.w > 200.);

    doc["hosts"]["top_bar"]["instances"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["iid"] == "title-main")
        .unwrap()["settings"]["max_width"] = json!(240);
    bar.update(BTreeMap::from([(
        "widgets.layout".into(),
        Value::list(vec![doc.clone()]),
    )]))
    .expect("valid independent min/max title settings remain renderable");
    thread::sleep(Duration::from_millis(36));
    bar.update(BTreeMap::new())
        .expect("max shrink animation frame");
    assert!(
        bar.session.preview_deadline().is_some(),
        "the natural-width transition is active"
    );

    let content = node_rect_and_clip(&bar, ".title-content").0;
    assert!(
        content.w <= 240.0,
        "the title allocation respects configured max_width"
    );
    let (text, clip) = title_text_rect_and_clip(&bar);
    let clip = clip.expect("title child inherits content-row clipping");
    assert!(
        text.x >= clip.x - 0.01
            && text.y >= clip.y - 0.01
            && text.x + text.w <= clip.x + clip.w + 0.01
            && text.y + text.h <= clip.y + clip.h + 0.01,
        "animated text paint and hit bounds stay inside the current clipped content row"
    );
    let Value::List(rows) = bar.session.stores.get("widgets.layout").unwrap() else {
        panic!("canonical layout remains a document list")
    };
    let stored_title = rows[0]["hosts"]["top_bar"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["iid"] == "title-main")
        .unwrap();
    assert_eq!(stored_title["settings"]["min_width"], 300);
    assert_eq!(stored_title["settings"]["max_width"], 240);
}

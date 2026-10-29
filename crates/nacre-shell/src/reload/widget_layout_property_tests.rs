// SPDX-License-Identifier: GPL-3.0-only
//! B0198: drag robustness. Random drag sequences (out of and into the pill, between
//! regions, to the desktop and back, off to the removal zone) keep the layout valid and
//! render exactly like a clean layout with the same order; broken files repair on load.
use crate::widgets::widget_drag::Controller;
use bead::core::Value;
use bead_test::{Config, Harness};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path};

const WIDTH: f64 = 1920.;
const HEIGHT: f64 = 1200.;

fn catalogue() -> Json {
    // The real packages' default style names (the bar validates them); spans as shipped.
    let styles = [
        ("launcher", "nacre"),
        ("workspaces", "pearls"),
        ("title", "window-title"),
        ("pill", "pill"),
        ("updates", "icon+value"),
        ("sound", "icon"),
        ("wifi", "icon"),
        ("bluetooth", "icon"),
        ("battery", "icon+value"),
        ("notifications", "icon+value"),
        ("clock", "time"),
        ("power", "icon"),
    ];
    Json::Array(
        styles
            .iter()
            .map(|(id, style)| {
                let mut forms = json!({"bar":{"default_style":style,"styles":{*style:{"min_span":1,"max_span":60,"preferred_span":1}}}});
                let desktop = match *id {
                    "clock" => Some("digital"),
                    "battery" => Some("ring"),
                    _ => None,
                };
                if let Some(name) = desktop {
                    forms["desktop"] = json!({"default_style":name,"styles":{name:{"min_span":1,"max_span":1200,"preferred_span":240,"preferred_height":120}}});
                }
                json!({"id":id,"defaults":{},"settings":{},"forms":forms})
            })
            .collect(),
    )
}

/// The shipped default (shell/widgets/default.luau) at 1920 px.
fn default_layout() -> Json {
    let item = |iid: &str, widget: &str, host: &str, region: &str, start: u32| json!({"iid":iid,"widget":widget,"host":host,"slot":{"region":region,"start":start,"span":1},"settings":{}});
    let mut pill = item("status-pill", "pill", "top_bar", "right", 0);
    pill["children"] = json!([
        item("updates", "updates", "status-pill", "left", 0),
        item("sound", "sound", "status-pill", "centre", 0),
        item("wifi", "wifi", "status-pill", "centre", 1),
        item("bluetooth", "bluetooth", "status-pill", "centre", 2),
        item("battery", "battery", "status-pill", "centre", 3),
        item("notifications", "notifications", "status-pill", "centre", 4),
        item("clock", "clock", "status-pill", "right", 0),
        item("power", "power", "status-pill", "right", 1),
    ]);
    let mut launcher = item("launcher", "launcher", "top_bar", "left", 0);
    launcher["settings"] = json!({"icon":"nacre","custom_path":"","action":"launcher","route":""});
    json!({"version":1,"preset":"Nacre default","hosts":{"top_bar":{"tiles":60,"mode":"content",
        "regions":{"left":{"start":0,"span":60},"centre":{"start":0,"span":60},"right":{"start":0,"span":60}},
        "instances":[launcher,item("workspaces","workspaces","top_bar","left",1),
            item("title","title","top_bar","centre",0),pill]}}})
}

/// Every bar item's (iid, host, region, start, span) in document order.
fn slots(doc: &Json) -> Vec<String> {
    fn walk(items: &[Json], out: &mut Vec<String>) {
        for item in items {
            out.push(format!(
                "{} {} {} {} {}",
                item["iid"],
                item["host"],
                item["slot"]["region"],
                item["slot"]["start"],
                item["slot"]["span"]
            ));
            if let Some(children) = item["children"].as_array() {
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(
        doc["hosts"]["top_bar"]["instances"].as_array().unwrap(),
        &mut out,
    );
    out
}

fn rank(region: &str) -> u8 {
    match region {
        "left" => 0,
        "centre" => 1,
        _ => 2,
    }
}

/// Synthetic measured geometry in visual order: 64 px items, pills wrap their children.
fn geometry(doc: &Json) -> Vec<Json> {
    let mut rects = Vec::new();
    let items = doc["hosts"]["top_bar"]["instances"].as_array().unwrap();
    let width = |item: &Json| {
        item["children"]
            .as_array()
            .map_or(64., |children| 24. + 64. * children.len() as f64)
    };
    let place = |item: &Json, x: f64, rects: &mut Vec<Json>| {
        rects.push(json!({"iid":item["iid"],"x":x,"y":0,"w":width(item),"h":34}));
        for (index, child) in item["children"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            rects.push(json!({"iid":child["iid"],"x":x+12.+64.*index as f64,"y":0,"w":64,"h":34}));
        }
    };
    let region = |name: &str| -> Vec<&Json> {
        items
            .iter()
            .filter(|item| item["slot"]["region"] == name)
            .collect()
    };
    let mut x = 16.;
    for item in region("left") {
        place(item, x, &mut rects);
        x += width(item) + 8.;
    }
    let centre = region("centre");
    let total: f64 = centre.iter().map(|item| width(item) + 8.).sum();
    let mut x = 900_f64.min(WIDTH / 2. - total / 2.);
    for item in centre {
        place(item, x, &mut rects);
        x += width(item) + 8.;
    }
    let right = region("right");
    let total: f64 = right.iter().map(|item| width(item) + 8.).sum();
    let mut x = WIDTH - 16. - total;
    for item in right {
        place(item, x, &mut rects);
        x += width(item) + 8.;
    }
    rects
}

fn context(doc: &Json) -> Json {
    json!({"catalogue":catalogue(),"geometry":geometry(doc),"screen_width":WIDTH,"screen_height":HEIGHT,
        "canvas_width":WIDTH,"canvas_height":HEIGHT,"snap_distance":14,
        "remove_zone":{"width":192,"height":64,"bottom":64}})
}

fn locate<'a>(doc: &'a Json, iid: &str) -> Option<&'a Json> {
    fn find<'a>(items: &'a [Json], iid: &str) -> Option<&'a Json> {
        items.iter().find_map(|item| {
            (item["iid"] == iid).then_some(item).or_else(|| {
                item["children"]
                    .as_array()
                    .and_then(|children| find(children, iid))
            })
        })
    }
    doc["hosts"].as_object()?.values().find_map(|host| {
        host["instances"]
            .as_array()
            .and_then(|items| find(items, iid))
    })
}

fn rect(doc: &Json, iid: &str) -> Option<(f64, f64)> {
    geometry(doc)
        .into_iter()
        .find(|rect| rect["iid"] == iid)
        .map(|rect| (rect["x"].as_f64().unwrap(), rect["w"].as_f64().unwrap()))
}

fn drop_event(doc: &Json, iid: &str, destination: &str, x: f64, y: f64, phase: &str) -> Value {
    let item = locate(doc, iid).unwrap();
    let canvas = item["host"] == "canvas";
    let (width, height) = if canvas { (240., 120.) } else { (64., 34.) };
    Value::list(vec![
        json!({"phase":phase,"source":[{"iid":iid,"group":item["group"].as_str().unwrap_or(iid),
        "widget":item["widget"],"host":if canvas {"canvas"} else {"top_bar"},"form":if canvas {"desktop"} else {"bar"},
        "settings":{}}],"destination":destination,"x":x,"y":y,"global_x":x,"global_y":y,
        "destination_x":x,"destination_y":y,"output_origin_x":0,"output_origin_y":0,
        "screen_width":WIDTH,"screen_height":HEIGHT,"width":width,"height":height,"grab_x":width/2.,"grab_y":height/2.,
        "source_origin_valid":true,"target_origin_x":0,"target_origin_y":0,"context":context(doc)}),
    ])
}

struct Model {
    controller: Controller,
    doc: Json,
    epoch: f64,
}

impl Model {
    fn new(doc: Json) -> Self {
        let mut controller = Controller::new().unwrap();
        controller
            .sync(&BTreeMap::from([
                ("widgets.layout".into(), Value::list(vec![doc.clone()])),
                ("widgets.screen_width".into(), Value::Number(WIDTH)),
                ("widgets.screen_height".into(), Value::Number(HEIGHT)),
            ]))
            .unwrap();
        Self {
            controller,
            doc,
            epoch: 0.,
        }
    }

    /// One real gesture: move then drop, then retire. Returns whether it committed.
    fn drag(&mut self, iid: &str, destination: &str, x: f64, y: f64) -> bool {
        let before = self.doc.clone();
        let moved = self
            .controller
            .drag(drop_event(&before, iid, destination, x, y, "move"))
            .unwrap();
        if let Some(Value::Number(epoch)) = moved.get("widgets.drag.epoch") {
            self.epoch = *epoch;
        }
        if let Some(Value::List(preview)) = moved.get("widgets.drag.layout")
            && let Some(preview) = preview.first()
        {
            assert_valid(preview, true);
        }
        let dropped = self
            .controller
            .drag(drop_event(&before, iid, destination, x, y, "drop"))
            .unwrap();
        self.controller
            .drag(Value::list(vec![
                json!({"phase":"retire","epoch":self.epoch}),
            ]))
            .unwrap();
        match dropped.get("widgets.write") {
            Some(Value::List(rows)) if rows.len() == 1 => {
                self.doc = rows[0].clone();
                assert_valid(&self.doc, false);
                true
            }
            _ => {
                assert_eq!(
                    dropped.get("widgets.drag.phase"),
                    Some(&Value::Text("return".into()))
                );
                false
            }
        }
    }

    fn undo(&mut self) -> Json {
        let values = self
            .controller
            .edit(Value::list(vec![json!({"op":"undo"})]))
            .unwrap();
        let Some(Value::List(draft)) = values.get("widgets.draft") else {
            panic!("undo restores a draft")
        };
        draft[0].clone()
    }
}

/// Valid means: content packing, full regions, unique iids and, in every list, regions in
/// order with contiguous 0-based starts and form spans (all 1 in this catalogue).
fn assert_valid(doc: &Json, preview: bool) {
    let bar = &doc["hosts"]["top_bar"];
    assert_eq!(bar["mode"], "content", "{doc}");
    for region in ["left", "centre", "right"] {
        assert_eq!(bar["regions"][region]["start"], 0);
        assert_eq!(bar["regions"][region]["span"], bar["tiles"]);
    }
    let mut seen = std::collections::HashSet::new();
    fn walk(
        items: &[Json],
        seen: &mut std::collections::HashSet<String>,
        doc: &Json,
        preview: bool,
    ) {
        let mut cursor: BTreeMap<String, u64> = BTreeMap::new();
        let mut last_rank = 0;
        for item in items {
            assert!(
                seen.insert(item["iid"].as_str().unwrap().to_owned()),
                "duplicate iid in {doc}"
            );
            if !preview {
                assert!(
                    item.get("drag_placeholder").is_none(),
                    "placeholder committed: {doc}"
                );
            }
            let region = item["slot"]["region"].as_str().unwrap();
            assert!(rank(region) >= last_rank, "regions out of order in {doc}");
            last_rank = rank(region);
            let next = cursor.entry(region.to_owned()).or_default();
            assert_eq!(
                item["slot"]["start"].as_f64(),
                Some(*next as f64),
                "gap/overlap in {doc}"
            );
            assert_eq!(
                item["slot"]["span"].as_f64(),
                Some(1.),
                "inflated span in {doc}"
            );
            *next += 1;
            if let Some(children) = item["children"].as_array() {
                assert_eq!(item["widget"], "pill");
                walk(children, seen, doc, preview);
            }
        }
    }
    walk(
        bar["instances"].as_array().unwrap(),
        &mut seen,
        doc,
        preview,
    );
    for item in doc["hosts"]["canvas"]["instances"]
        .as_array()
        .into_iter()
        .flatten()
    {
        assert!(seen.insert(item["iid"].as_str().unwrap().to_owned()));
        let slot = &item["slot"];
        let (x, y, w, h) = (
            slot["x"].as_f64().unwrap(),
            slot["y"].as_f64().unwrap(),
            slot["w"].as_f64().unwrap(),
            slot["h"].as_f64().unwrap(),
        );
        assert!(
            x >= 0. && y >= 0. && x + w <= WIDTH && y + h <= HEIGHT,
            "{doc}"
        );
    }
}

/// A freshly authored layout with the same order: default preset, no per-group state.
fn clean(doc: &Json) -> Json {
    let mut clean = json!({"version":1,"preset":"Nacre default","hosts":doc["hosts"].clone()});
    fn strip(items: &mut [Json]) {
        for item in items {
            if let Some(object) = item.as_object_mut() {
                object.remove("group");
            }
            if let Some(children) = item["children"].as_array_mut() {
                strip(children);
            }
        }
    }
    for host in clean["hosts"].as_object_mut().unwrap().values_mut() {
        if let Some(items) = host["instances"].as_array_mut() {
            strip(items);
        }
    }
    clean
}

fn render_bar(doc: &Json) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.insert("widgets.layout".into(), Value::list(vec![doc.clone()]));
    stores.insert("bead.focused_title".into(), Value::Text(String::new()));
    stores.insert("bead.windows".into(), Value::list(Vec::new()));
    stores.insert("bead.workspaces".into(), Value::list(Vec::new()));
    let mut harness = Harness::load(
        &root,
        Path::new("bar/main.luau"),
        &root.join("palette.json"),
        stores,
        Config {
            width: WIDTH as u32,
            height: 50,
            scale: 1.,
        },
    )
    .unwrap();
    harness.render().unwrap().pixels
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
        &items[(self.next() % items.len() as u64) as usize]
    }
    fn x(&mut self) -> f64 {
        (self.next() % (WIDTH as u64 - 40)) as f64 + 20.
    }
}

fn bar_iids(doc: &Json) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(items: &[Json], out: &mut Vec<String>) {
        for item in items {
            if item["widget"] != "pill" {
                out.push(item["iid"].as_str().unwrap().to_owned());
            }
            if let Some(children) = item["children"].as_array() {
                walk(children, out);
            }
        }
    }
    walk(
        doc["hosts"]["top_bar"]["instances"].as_array().unwrap(),
        &mut out,
    );
    out
}

#[test]
fn random_drag_sequences_keep_a_valid_layout_that_renders_like_a_clean_one() {
    for seed in 1..=12_u64 {
        let mut rng = Lcg(seed);
        let mut model = Model::new(default_layout());
        for _ in 0..8 {
            let iids = bar_iids(&model.doc);
            let iid = rng.pick(&iids).to_owned();
            match rng.next() % 4 {
                // Into the pill, between its children.
                0 => {
                    let (x, w) = rect(&model.doc, "status-pill").unwrap();
                    let at = x + (rng.next() % w as u64) as f64;
                    model.drag(&iid, "bar", at, 17.);
                }
                // Anywhere on the bar: other regions, out of the pill.
                1 | 2 => {
                    let at = rng.x();
                    model.drag(&iid, "bar", at, 17.);
                }
                // To the desktop and back (desktop-capable widgets), else a bar move.
                _ => {
                    let item = locate(&model.doc, &iid).unwrap().clone();
                    if matches!(item["widget"].as_str(), Some("clock" | "battery")) {
                        model.drag(&iid, "desktop", 700., 500.);
                        let back = rng.x();
                        if locate(&model.doc, &iid).is_some_and(|item| item["host"] == "canvas") {
                            model.drag(&iid, "bar", back, 17.);
                        }
                    } else {
                        let at = rng.x();
                        model.drag(&iid, "bar", at, 17.);
                    }
                }
            }
        }
        assert_valid(&model.doc, false);
        assert_eq!(
            render_bar(&model.doc),
            render_bar(&clean(&model.doc)),
            "seed {seed}: the dragged layout must render like a clean layout with the same order"
        );
    }
}

#[test]
fn clock_out_of_the_pill_to_the_desktop_and_back_restores_the_default_exactly() {
    let default = default_layout();
    let mut model = Model::new(default.clone());
    // Out of the pill onto the bar, back in, onto the desktop, and back to its own spot.
    assert!(model.drag("clock", "bar", 700., 17.));
    assert_eq!(locate(&model.doc, "clock").unwrap()["host"], "top_bar");
    let (px, pw) = rect(&model.doc, "status-pill").unwrap();
    assert!(model.drag("clock", "bar", px + pw - 40., 17.));
    assert_eq!(locate(&model.doc, "clock").unwrap()["host"], "status-pill");
    assert!(model.drag("clock", "desktop", 700., 500.));
    assert_eq!(locate(&model.doc, "clock").unwrap()["host"], "canvas");
    // Back to its spot: released between notifications and power.
    let (nx, nw) = rect(&model.doc, "notifications").unwrap();
    let (wx, ww) = rect(&model.doc, "power").unwrap();
    assert!(model.drag("clock", "bar", (nx + nw / 2. + wx + ww / 2.) / 2., 17.));
    assert_eq!(
        slots(&model.doc),
        slots(&default),
        "the bar returns to the default order, starts and spans"
    );
    assert_eq!(render_bar(&model.doc), render_bar(&default));
    // Undo restores the exact previous document.
    let before = model.doc.clone();
    assert!(model.drag("power", "bar", 100., 17.));
    assert_eq!(model.undo(), before);
}

#[test]
fn invalid_drops_snap_back_and_the_removal_zone_takes_a_widget_off() {
    let default = default_layout();
    let mut model = Model::new(default.clone());
    // Bar-only widgets have no desktop form: the drop is rejected and nothing is written.
    assert!(!model.drag("power", "desktop", 700., 500.));
    assert_eq!(model.doc, default);
    // The pill cannot be dropped into itself.
    let (px, pw) = rect(&default, "status-pill").unwrap();
    model.drag("status-pill", "bar", px + pw / 2., 17.);
    assert_valid(&model.doc, false);
    // Dropping on the removal zone (bottom centre) removes the widget.
    assert!(model.drag("wifi", "desktop", WIDTH / 2., HEIGHT - 64. - 32.));
    assert!(locate(&model.doc, "wifi").is_none());
    assert_valid(&model.doc, false);
}

#[test]
fn a_layout_broken_by_an_old_drag_is_repaired_on_load() {
    // The owner's file after the bad drag (B0198): tiles mode, clock span 4 after
    // notifications, power at right/0, inflated pill span and regions.
    let broken: Json =
        serde_json::from_str(include_str!("widget_layout_broken_fixture.json")).unwrap();
    let mut repaired = broken.clone();
    let pill = &mut repaired["hosts"]["top_bar"]["instances"][3]["children"];
    // Same order, clean slots: clock last in the centre, power alone on the right.
    let order = [
        ("updates", "left", 0),
        ("sound", "centre", 0),
        ("wifi", "centre", 1),
        ("bluetooth", "centre", 2),
        ("battery", "centre", 3),
        ("notifications", "centre", 4),
        ("clock", "centre", 5),
        ("power", "right", 0),
    ];
    let children: Vec<Json> = order
        .iter()
        .map(|(iid, region, start)| {
            let mut item = pill
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["iid"] == *iid)
                .unwrap()
                .clone();
            item["slot"] = json!({"region":region,"start":start,"span":1});
            item
        })
        .collect();
    *pill = Json::Array(children);
    let clean_doc = {
        let mut doc = clean(&repaired);
        doc["hosts"]["top_bar"]["mode"] = json!("content");
        let tiles = doc["hosts"]["top_bar"]["tiles"].clone();
        for region in ["left", "centre", "right"] {
            doc["hosts"]["top_bar"]["regions"][region] = json!({"start":0,"span":tiles});
        }
        for (index, item) in doc["hosts"]["top_bar"]["instances"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            let start = [0, 1, 0, 0][index];
            item["slot"]["start"] = json!(start);
            item["slot"]["span"] = json!(1);
        }
        doc
    };
    assert_eq!(render_bar(&broken), render_bar(&clean_doc));
    // The first edit after load writes the repaired document.
    let mut model = Model::new(broken);
    assert!(model.drag("power", "bar", 1900., 17.));
    assert_valid(&model.doc, false);
}

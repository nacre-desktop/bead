// SPDX-License-Identifier: GPL-3.0-only
//! B0114 app model coverage without constructing the gallery UI.
use super::*;
use bead::core::Value;
use bead::core::source::Source;
use bead_test::{Config, Harness};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path};

fn model_harness() -> (Harness, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("bead-widget-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("widgets/app");
    std::fs::create_dir_all(&app).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    std::fs::copy(source.join("palette.json"), root.join("palette.json")).unwrap();
    std::fs::write(
        app.join("behavior.luau"),
        include_str!("../../../../shell/widgets/app/behavior.luau"),
    )
    .unwrap();
    std::fs::write(
        app.join("main.luau"),
        "local ui=require(\"bead.ui\")\nreturn ui.Box{id=\"root\",width=1,height=1}\n",
    )
    .unwrap();
    let harness = Harness::load(
        &root,
        Path::new("widgets/app/main.luau"),
        &root.join("palette.json"),
        BTreeMap::new(),
        Config {
            width: 800,
            height: 600,
            scale: 1.,
        },
    )
    .unwrap();
    (harness, root)
}

fn catalogue() -> Json {
    json!([{
        "id":"clock",
        "title":"Clock",
        "category":"bar",
        "defaults":{"format":"24"},
        "settings":{"format":{"type":"choice","options":["24","12"]}},
        "forms":{
            "bar":{"default_style":"time","styles":{"time":{"min_span":1,"max_span":4,"preferred_span":1}}},
            "desktop":{"default_style":"card","styles":{"card":{"min_span":160,"max_span":500,"preferred_span":240}}}
        }
    }])
}

fn document() -> Json {
    json!({
        "version":1,"preset":"Nacre default","future":{"opaque":[7,9]},
        "hosts":{"top_bar":{"tiles":60,"regions":{
            "left":{"start":0,"span":60},"centre":{"start":0,"span":60},"right":{"start":0,"span":60}
        },"instances":[{"iid":"clock","widget":"clock","host":"top_bar",
            "slot":{"region":"left","start":0,"span":1},"settings":{"format":"24","future":"keep"}}]}}
    })
}

fn act(harness: &mut Harness, mut request: Json, doc: &Json) {
    request["document"] = doc.clone();
    request["initial"] = doc.clone();
    request["catalogue"] = catalogue();
    let values = harness
        .session
        .action_values("widgets.edit", Value::list(vec![request]))
        .unwrap();
    harness.update(values).unwrap();
}

#[test]
fn form_toggle_shared_settings_undo_and_unknown_json_round_trip() {
    let (mut harness, root) = model_harness();
    let original = document();
    act(
        &mut harness,
        json!({"op":"toggle-form","widget":"clock","form":"desktop","enabled":true}),
        &original,
    );
    let after_toggle = harness.session.stores.get("widgets.draft").unwrap();
    let Value::List(after_toggle) = after_toggle else {
        panic!("draft is a document list")
    };
    let mut doc = after_toggle[0].clone();
    assert_eq!(doc["future"], json!({"opaque":[7,9]}));
    assert_eq!(doc["widgets"]["clock"]["forms"]["desktop"]["enabled"], true);
    assert_eq!(
        doc["hosts"]["canvas"]["instances"][0]["iid"],
        "clock:desktop"
    );
    assert_eq!(
        doc["hosts"]["canvas"]["instances"][0]["settings"]["future"],
        "keep"
    );

    act(
        &mut harness,
        json!({"op":"setting","widget":"clock","form":"bar","key":"format","value":"12"}),
        &doc,
    );
    let Value::List(draft) = harness.session.stores.get("widgets.draft").unwrap() else {
        panic!("draft is a document list")
    };
    doc = draft[0].clone();
    assert_eq!(
        doc["hosts"]["top_bar"]["instances"][0]["settings"]["format"],
        "12"
    );
    assert_eq!(
        doc["hosts"]["canvas"]["instances"][0]["settings"]["format"],
        "12"
    );

    act(&mut harness, json!({"op":"undo"}), &doc);
    let Value::List(draft) = harness.session.stores.get("widgets.draft").unwrap() else {
        panic!("draft is a document list")
    };
    assert_eq!(
        draft[0]["hosts"]["top_bar"]["instances"][0]["settings"]["format"],
        "24"
    );
    assert_eq!(
        draft[0]["hosts"]["canvas"]["instances"][0]["settings"]["format"],
        "24"
    );
    act(
        &mut harness,
        json!({"op":"default-trigger","value":"hover"}),
        &doc,
    );
    let Value::List(draft) = harness.session.stores.get("widgets.draft").unwrap() else {
        panic!("draft missing")
    };
    assert_eq!(draft[0]["dropdown_trigger"], "hover");
    act(&mut harness, json!({"op":"undo"}), &doc);
    let Value::List(draft) = harness.session.stores.get("widgets.draft").unwrap() else {
        panic!("draft missing")
    };
    assert!(draft[0]["dropdown_trigger"].is_null());
    drop(harness);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn display_preview_uses_primary_output_logical_dimensions() {
    let source = bead_test::source::FakeSource::new(json!({
        "health":{"available":true},
        "outputs":[
            {"name":"panel","primary":true,"width":3840,"height":2160,"scale":2.0},
            {"name":"side","width":1920,"height":1080,"scale":1.0}
        ]
    }))
    .unwrap();
    let values = widget_screen_values(&source.snapshot());
    assert_eq!(values["widgets.screen_width"], Value::Number(1920.));
    assert_eq!(values["widgets.screen_height"], Value::Number(1080.));
}

#[test]
fn late_widget_write_ack_preserves_newer_canonical_edits_and_reports_errors() {
    let layout = |name: &str| Value::list(vec![json!({"version":1,"name":name})]);
    let old = layout("old");
    let requested = layout("requested");
    let newer = layout("another writer");
    let canonical = BTreeMap::from([
        ("widgets.layout".into(), newer.clone()),
        ("widgets.rev".into(), Value::Number(9.)),
    ]);
    for error in [None, Some("revision conflict".to_owned())] {
        let changes = widget_write_resolution(
            Some(requested.clone()),
            Some((old.clone(), Value::list(Vec::new()), 7)),
            Some(8),
            error.clone(),
            canonical.clone(),
        );
        assert_eq!(changes["widgets.layout"], newer);
        assert_eq!(changes["widgets.rev"], Value::Number(9.));
        assert_eq!(
            changes["widgets.error"],
            Value::Text(error.unwrap_or_default())
        );
        assert_eq!(changes["widgets.busy"], Value::Bool(false));
    }
    let changes = widget_write_resolution(
        Some(requested),
        Some((old.clone(), Value::list(Vec::new()), 7)),
        None,
        Some("store unavailable".into()),
        BTreeMap::new(),
    );
    assert_eq!(changes["widgets.layout"], old);
    assert_eq!(changes["widgets.rev"], Value::Number(7.));
    assert_eq!(
        changes["widgets.error"],
        Value::Text("store unavailable".into())
    );
}

#[test]
fn real_widgets_gallery_starts_with_content_and_a_style_click_updates_the_preview() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut harness = Harness::load(
        &root,
        Path::new("widgets/app/main.luau"),
        &root.join("palette.json"),
        crate::desktop::values(&Default::default(), ""),
        Config {
            width: 1200,
            height: 800,
            scale: 1.5,
        },
    )
    .unwrap();
    let has_text = |h: &Harness, value: &str| {
        h.scene().primitives.iter().any(|primitive|
        matches!(primitive, bead::render::Primitive::Text {text,..} if text == value))
    };
    assert!(
        has_text(&harness, "Choose a widget"),
        "first scene: {:?}; nodes: {:?}",
        harness
            .scene()
            .primitives
            .iter()
            .filter_map(|p| match p {
                bead::render::Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        harness
            .session
            .view_nodes()
            .iter()
            .map(|n| (&n.label, bead::ui::rect(&n.draw)))
            .collect::<Vec<_>>()
    );
    assert!(has_text(&harness, "Clock"));
    assert!(
        harness
            .session
            .stores
            .get("bead.focused")
            .is_none_or(|value| value == &Value::Text(String::new()))
    );
    harness
        .update(BTreeMap::from([(
            "widgets.selected".into(),
            Value::Text("clock".into()),
        )]))
        .unwrap();
    let button = harness
        .session
        .view_nodes()
        .iter()
        .find(|node| node.label == "Seconds")
        .expect("clock style tile must be present");
    let rect = bead::ui::rect(&button.draw);
    harness
        .script([bead_test::Input::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        }])
        .unwrap();
    let Value::List(draft) = harness.session.stores.get("widgets.draft").unwrap() else {
        panic!("draft missing")
    };
    assert_eq!(
        draft[0]["hosts"]["top_bar"]["instances"][3]["children"][6]["style"],
        "seconds"
    );
}

#[test]
fn clock_detail_stage_and_style_previews_keep_all_content_inside_their_clips() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.insert("widgets.selected".into(), Value::Text("clock".into()));
    stores.insert("widgets.preview".into(), Value::list(Vec::new()));
    stores.insert("bar.clock.text".into(), Value::Text("09:08".into()));
    stores.insert("bead.desktop.clock".into(), Value::Text("09:08".into()));
    stores.insert(
        "bead.desktop.clock.seconds".into(),
        Value::Text("09:08:07".into()),
    );
    stores.insert(
        "bead.desktop.clock.date".into(),
        Value::Text("2026-10-25".into()),
    );
    let harness = Harness::load(
        &root,
        Path::new("widgets/app/main.luau"),
        &root.join("palette.json"),
        stores,
        Config {
            width: 1200,
            height: 800,
            scale: 1.5,
        },
    )
    .unwrap();
    let scene = harness.scene();
    let text = |expected: &str| {
        scene.primitives.iter().any(
            |primitive| matches!(primitive,bead::render::Primitive::Text{text,..} if text==expected),
        )
    };
    assert!(text("Your screen"), "clock detail stage caption was omitted");
    assert!(
        text("calendar_month"),
        "the mini bar did not render its clock/calendar glyph"
    );

    for expected in ["09:08:07", "2026-10-25"] {
        let matches: Vec<_> = scene
            .primitives
            .iter()
            .enumerate()
            .filter_map(|(index, primitive)| match primitive {
                bead::render::Primitive::Text {
                    rect,
                    text,
                    ..
                } if text == expected => Some((index, *rect)),
                _ => None,
            })
            .collect();
        assert_eq!(matches.len(), 1, "style preview omitted {expected}");
        let (index, rect) = matches[0];
        let clip = scene
            .clips
            .get(&index)
            .unwrap_or_else(|| panic!("{expected} preview is missing its clipping frame"));
        assert!(
            rect.x >= clip.x - 0.01
                && rect.y >= clip.y - 0.01
                && rect.x + rect.w <= clip.x + clip.w + 0.01
                && rect.y + rect.h <= clip.y + clip.h + 0.01,
            "{expected} preview content {rect:?} exceeds its clip {clip:?}"
        );
    }
}

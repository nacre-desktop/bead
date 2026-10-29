// SPDX-License-Identifier: GPL-3.0-only
//! B0191: markup (Luau view evaluation) per UI update on the real shell tree.
//!
//! Bench: `BEAD_PROFILE_INPUT=1 cargo test --release -p nacre-shell markup_bench -- --ignored --nocapture`,
//! then read the `INPUT_PROFILE stage=markup*` lines after each `MARKUP_BENCH` marker.
//! Each step changes stores a view reads while building (bound-only changes skip
//! markup): a workspace/title change, a keystroke with a new result page, a
//! dashboard refresh and arriving notifications.

use bead::core::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

type Step = Box<dyn Fn(usize) -> BTreeMap<String, Value>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

fn harness(entry: &str, mut models: BTreeMap<String, Value>) -> bead_test::Harness {
    models.insert("panel.open".into(), Value::Bool(true));
    let root = root();
    let icons = BTreeMap::from([("utilities-terminal".to_owned(), root.join("tests/icon.svg"))]);
    bead_test::Harness::load_with_icons(
        &root,
        Path::new(entry),
        &root.join("palette.json"),
        models,
        bead_test::Config {
            width: 1920,
            height: 1200,
            scale: 1.5,
        },
        icons,
    )
    .expect("shell entry loads")
}

fn apps(query: &str, count: usize) -> Value {
    Value::list(
        (0..count)
            .map(|i| {
                serde_json::json!({"id":format!("{query}-{i}.desktop"),"name":format!("{query} app {i}"),
                "icon":"utilities-terminal","selected":i==0,"description":"Terminal emulator",
                "favorite":false,"favoriteId":""})
            })
            .collect(),
    )
}

fn workspaces(active: usize) -> Value {
    Value::list(
        (1..=6)
            .map(|i| serde_json::json!({"id":i.to_string(),"name":i.to_string(),"index":i,"active":i==active,"urgent":false,"mru":false}))
            .collect(),
    )
}

/// (name, entry, initial models, per-step store changes)
fn views() -> Vec<(&'static str, &'static str, BTreeMap<String, Value>, Step)> {
    crate::i18n::init();
    let notifications = crate::notifications::values(&serde_json::Value::Null);
    let mut bar = crate::desktop::values(&Default::default(), "");
    bar.extend(notifications.clone());
    bar.insert("bead.workspaces".into(), workspaces(1));
    bar.insert("bead.windows".into(), Value::list(vec![]));
    bar.insert("bead.focused_title".into(), Value::Text("Editor".into()));
    vec![
        (
            "bar",
            "bar/main.luau",
            bar,
            Box::new(|i| {
                BTreeMap::from([
                    ("bead.workspaces".into(), workspaces(1 + i % 6)),
                    (
                        "bead.focused_title".into(),
                        Value::Text(format!("Window title {}", "x".repeat(i % 7))),
                    ),
                ])
            }),
        ),
        (
            "launcher",
            "ui/main.luau",
            BTreeMap::new(),
            Box::new(|i| {
                let query = &"terminal"[..1 + i % 8];
                let count = 12 - i % 4;
                BTreeMap::from([
                    ("launcher.search.text".into(), Value::Text(query.into())),
                    ("launcher.result_query".into(), Value::Text(query.into())),
                    ("launcher.mode".into(), Value::Text("search".into())),
                    ("launcher.apps".into(), apps(query, count)),
                    ("launcher.result_count".into(), Value::Number(count as f64)),
                    ("launcher.total_count".into(), Value::Number(count as f64)),
                ])
            }),
        ),
        (
            "dashboard",
            "dashboard/main.luau",
            BTreeMap::new(),
            Box::new(|i| {
                BTreeMap::from([
                    (
                        "dashboard.message".into(),
                        Value::Text(format!("Refreshed {i}")),
                    ),
                    ("dashboard.stale".into(), Value::Bool(i % 2 == 0)),
                    (
                        "dashboard.metric".into(),
                        Value::Text(format!("{}%", 20 + i % 50)),
                    ),
                ])
            }),
        ),
        (
            "notifications",
            "notifications/main.luau",
            notifications,
            Box::new(|i| {
                crate::notifications::values(&serde_json::json!({
                    "popup_count": 1 + i % 4,
                    "rows": (0..1 + i % 4).map(|n| serde_json::json!({"id":format!("n{}", i * 10 + n),"app":"Mail",
                        "summary":format!("Message {}", i * 10 + n),"body":"Hello there","icon":"",
                        "time":"now","urgency":"normal","actions":[],"popup":true})).collect::<Vec<_>>(),
                }))
            }),
        ),
    ]
}

/// Content of a scene that does not depend on motion timing: kinds, texts and images.
fn content(harness: &bead_test::Harness) -> Vec<String> {
    harness
        .scene()
        .primitives
        .iter()
        .map(|primitive| {
            let mut value = serde_json::to_value(primitive).expect("primitive");
            let keep = |key: &str| matches!(key, "type" | "text" | "path" | "family" | "size");
            if let Some(object) = value.as_object_mut() {
                object.retain(|key, _| keep(key));
            }
            value.to_string()
        })
        .collect()
}

/// Sequential updates on one (persistent-VM) generation draw what a fresh load
/// of the same stores draws.
#[test]
fn persistent_vm_updates_match_fresh_loads_on_shell_views() {
    for (name, entry, models, step) in views() {
        let mut persistent = harness(entry, models.clone());
        let mut stores = models;
        for i in 0..6 {
            let mut changes = step(i);
            // The title's width animates, so its elision depends on motion timing.
            changes.remove("bead.focused_title");
            stores.extend(changes.clone());
            persistent.update(changes).expect("update");
            let fresh = harness(entry, stores.clone());
            let (left, right) = (content(&persistent), content(&fresh));
            let differ: Vec<_> = left
                .iter()
                .zip(&right)
                .filter(|(a, b)| a != b)
                .take(4)
                .collect();
            assert!(
                left.len() == right.len() && differ.is_empty(),
                "{name} step {i}: {} vs {} primitives, first differences {differ:?}",
                left.len(),
                right.len()
            );
        }
    }
}

#[test]
#[ignore = "manual B0191 markup benchmark; release build on a build host"]
fn markup_bench() {
    for (name, entry, models, step) in views() {
        let mut harness = harness(entry, models);
        for i in 0..60 {
            println!("MARKUP_BENCH view={name} step={i}");
            harness.update(step(i)).expect("update");
        }
        println!(
            "MARKUP_BENCH_RESOURCES view={name} {}",
            harness.session.resource_counts()
        );
    }
    println!("MARKUP_BENCH done");
}

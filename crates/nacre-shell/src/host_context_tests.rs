// SPDX-License-Identifier: GPL-3.0-only
//! The bar's widget context (`bar.clock.twelve` and friends) is offered to whichever panel is
//! open; views that do not declare it must ignore it instead of failing with "unknown state".

use bead::core::Value;
use bead::wayland::{SceneSource, UiEvent};
use bead_test::{Config, Harness};
use std::{collections::BTreeMap, path::Path};

fn runtime(entry: &str, models: BTreeMap<String, Value>) -> crate::reload::Runtime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let harness = Harness::load(
        &root,
        Path::new(entry),
        &root.join("palette.json"),
        models,
        Config {
            width: 1920,
            height: 1200,
            scale: 1.0,
        },
    )
    .expect("shell entry loads");
    crate::reload::Runtime::new(harness.session)
}

#[test]
fn panels_ignore_undeclared_bar_widget_context() {
    let context = BTreeMap::from([
        ("bar.clock.twelve".to_string(), Value::Bool(true)),
        ("bar.height".to_string(), Value::Number(50.)),
        ("bead.desktop.clock".to_string(), Value::Text("12:34".into())),
        ("bead.workspaces".to_string(), Value::list(Vec::<serde_json::Value>::new())),
        ("bead.focused_title".to_string(), Value::Text("Notes".into())),
    ]);
    for (entry, models) in [
        ("popouts/main.luau", BTreeMap::new()),
        (
            "control/main.luau",
            BTreeMap::from([("cc.page".to_string(), Value::Text("home".into()))]),
        ),
    ] {
        let mut runtime = runtime(entry, models);
        runtime
            .stage(UiEvent::Model(context.clone()))
            .unwrap_or_else(|e| panic!("{entry}: {e}"));
        assert!(
            runtime.session.stores.get("bar.clock.twelve").is_none(),
            "{entry} must not gain bar state"
        );
    }
}

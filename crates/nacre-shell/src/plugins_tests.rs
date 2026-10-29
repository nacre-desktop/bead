// SPDX-License-Identifier: GPL-3.0-only
//! Spec 0203 in the real shell tree: the bundled updates plugin, the example plugin,
//! enable/disable, a faulting plugin and the Plugins page.
use super::*;
use bead::core::Value;
use bead_test::{Config, Harness};
use std::path::PathBuf;

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != "tests" {
                copy_tree(&entry.path(), &target);
            }
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn app(root: &Path, selected: &str) -> Harness {
    crate::i18n::init();
    let mut values = crate::desktop::values(&Default::default(), "");
    values.insert("widgets.selected".into(), Value::Text(selected.into()));
    Harness::load(
        root,
        Path::new("widgets/app/main.luau"),
        &root.join("palette.json"),
        values,
        Config {
            width: 1200,
            height: 800,
            scale: 1.,
        },
    )
    .unwrap()
}
fn texts(harness: &Harness) -> Vec<String> {
    harness
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn bundled_updates_plugin_loads_and_the_plugins_page_lists_its_access() {
    let root = repository().join("shell");
    let page = texts(&app(&root, "@plugins"));
    for expected in [
        "Plugins",
        "Updates",
        "Luau sandboxing is not a security boundary: only enable plugins you trust.",
    ] {
        assert!(
            page.iter().any(|t| t == expected),
            "{expected} missing: {page:?}"
        );
    }
    assert!(
        page.iter()
            .any(|t| t.starts_with("Access: Reads bar.updates.*")),
        "{page:?}"
    );
    let detail = texts(&app(&root, "updates"));
    assert!(detail.iter().any(|t| t == "Updates"), "{detail:?}");
}

#[test]
fn example_plugin_enables_renders_unloads_and_a_faulting_plugin_is_switched_off() {
    let temp = std::env::temp_dir().join(format!("nacre-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let root = temp.join("shell");
    copy_tree(&repository().join("shell"), &root);
    let user = temp.join("data/plugins");
    copy_tree(
        &repository().join("examples/plugins/hello"),
        &user.join("hello"),
    );
    // Same widget, but it reads a store it never declared.
    copy_tree(
        &repository().join("examples/plugins/hello"),
        &user.join("nosy"),
    );
    let manifest = std::fs::read_to_string(user.join("nosy/plugin.toml")).unwrap();
    std::fs::write(
        user.join("nosy/plugin.toml"),
        manifest.replace("\"hello\"", "\"nosy\""),
    )
    .unwrap();
    let widget = std::fs::read_to_string(user.join("nosy/widget.luau")).unwrap();
    std::fs::write(
        user.join("nosy/widget.luau"),
        widget
            .replace("\"hello\"", "\"nosy\"")
            .replace("text=greeting,", "text=ui.bind(\"session.message\"),"),
    )
    .unwrap();
    let config = plugins::Config {
        system: vec![root.canonicalize().unwrap().join("plugins")],
        user: Some(user),
        state: Some(temp.join("config/plugins.json")),
    };
    plugins::configure(&root, config.clone());

    let page = texts(&app(&root, "@plugins"));
    assert!(page.iter().any(|t| t == "Hello"), "{page:?}");
    assert!(
        texts(&app(&root, "hello")).iter().all(|t| t != "Hey"),
        "disabled by default"
    );

    assert_eq!(
        act(&root, "enable|hello")["plugins.message"],
        Value::Text("hello enabled".into())
    );
    act(&root, "set|hello|greeting|s:Hey");
    let detail = texts(&app(&root, "hello"));
    assert!(
        detail.iter().any(|t| t == "Hey"),
        "plugin setting reaches the widget: {detail:?}"
    );

    act(&root, "disable|hello");
    let gallery = texts(&app(&root, "hello"));
    assert!(
        gallery.iter().all(|t| t != "Hey"),
        "disabling removes the widget: {gallery:?}"
    );

    act(&root, "enable|nosy");
    let shown = texts(&app(&root, "nosy"));
    assert!(
        shown.iter().any(|t| t == "Plugins"),
        "the app still renders: {shown:?}"
    );
    let state = plugins::State::load(config.state.as_deref());
    assert!(
        state.faults["nosy"].contains("lacks capability topic:session.message"),
        "{state:?}"
    );
    let page = texts(&app(&root, "@plugins"));
    assert!(
        page.iter()
            .any(|t| t.starts_with("Disabled after an error: ")),
        "{page:?}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}

/// B0208: plugins install from the launcher (`>plugin <folder>`), arrive disabled, and list
/// their declared access before the user enables them; ordinary typing never scans.
#[test]
fn launcher_plugin_mode_installs_a_local_folder_and_lists_its_access() {
    let temp = std::env::temp_dir().join(format!("nacre-launcher-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let root = temp.join("shell");
    std::fs::create_dir_all(&root).unwrap();
    let source = temp.join("src/hello");
    copy_tree(&repository().join("examples/plugins/hello"), &source);
    let config = plugins::Config {
        system: Vec::new(),
        user: Some(temp.join("data/plugins")),
        state: Some(temp.join("config/plugins.json")),
    };
    plugins::configure(&root, config.clone());

    assert_eq!(
        launcher_values(&root, "firefox")["launcher.plugin.active"],
        Value::Bool(false)
    );
    let query = format!(">plugin {}", source.display());
    let mode = launcher_values(&root, &query);
    assert_eq!(mode["launcher.plugin.active"], Value::Bool(true));
    assert_eq!(
        mode["launcher.plugin.source"],
        Value::Text(source.display().to_string())
    );
    let started = act(&root, &format!("install|{}", source.display()));
    assert_eq!(
        started["plugins.message"],
        Value::Text("Installing…".into())
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let row = loop {
        let values = launcher_values(&root, &query);
        if let Value::List(rows) = &values["launcher.plugin.rows"]
            && let Some(row) = rows.iter().find(|row| row["id"] == "hello")
        {
            break row.clone();
        }
        assert!(std::time::Instant::now() < deadline, "install finished");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(row["enabled"], false, "user plugins arrive disabled");
    assert!(row["access"].as_str().is_some(), "{row}");
    act(&root, "enable|hello");
    let Value::List(rows) = &launcher_values(&root, &query)["launcher.plugin.rows"] else {
        panic!("rows")
    };
    assert_eq!(rows[0]["enabled"], true);
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn launcher_plugin_mode_renders_install_and_the_declared_access() {
    crate::i18n::init();
    let root = repository().join("shell");
    let values = BTreeMap::from([
        (
            "launcher.search.text".into(),
            Value::Text(">plugin /tmp/x".into()),
        ),
        ("launcher.plugin.active".into(), Value::Bool(true)),
        (
            "launcher.plugin.source".into(),
            Value::Text("/tmp/x".into()),
        ),
        (
            "launcher.plugin.rows".into(),
            Value::list(vec![
                serde_json::json!({"id":"nosy","name":"Nosy","enabled":false,
                "access":"Network access","error":"","update":false}),
            ]),
        ),
    ]);
    let harness = Harness::load(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        values,
        Config {
            width: 1280,
            height: 800,
            scale: 1.,
        },
    )
    .unwrap();
    let shown = texts(&harness);
    for expected in [
        "Install from /tmp/x",
        "Install",
        "Nosy",
        "Access: Network access",
        "Enable",
    ] {
        assert!(shown.iter().any(|t| t == expected), "{expected}: {shown:?}");
    }
}

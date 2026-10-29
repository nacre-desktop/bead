// SPDX-License-Identifier: GPL-3.0-only
//! B0199: the real launcher bundle with an inline form open (every kind lays out, has its
//! fields and buttons, and a choice is a store write), and the bar's recording button.
use bead::core::Value;
use bead_test::{Config, Harness, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn open(kind: &str) -> Result<Harness> {
    crate::i18n::init();
    let root = root();
    let mut stores: BTreeMap<String, Value> =
        crate::launcher_form::open(kind, "Restart the Nacre shell?")
            .into_iter()
            .collect();
    stores.remove("bead.focused");
    stores.insert("launcher.groups".into(), crate::launcher::fixture_groups());
    Harness::load_with_icons(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        stores,
        Config {
            width: 1920,
            height: 1200,
            scale: 1.5,
        },
        BTreeMap::new(),
    )
}
/// Every text the launcher draws.
fn shown(h: &Harness) -> Vec<String> {
    h.scene()
        .primitives
        .iter()
        .filter_map(|draw| match draw {
            bead::render::Primitive::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}
fn count(h: &Harness, action: &str, arg: &str) -> usize {
    h.session
        .view_nodes()
        .iter()
        .filter(|n| {
            n.action.as_deref() == Some(action) && n.action_arg == Some(Value::Text(arg.into()))
        })
        .count()
}

#[test]
fn every_form_kind_lays_out_with_its_fields_and_both_buttons() -> Result<()> {
    for (kind, title, inputs, choices) in [
        ("reminder", "Set a reminder", 2, 0),
        ("webapp", "Create a web app", 2, 0),
        ("tui", "Create a TUI app", 3, 4),
        ("theme-import", "Import theme", 1, 0),
        ("extension-import", "Add an extension", 1, 0),
        ("recording-area", "Restart the Nacre shell?", 0, 3),
        ("recording-screen", "Restart the Nacre shell?", 0, 3),
        ("confirm-restart", "Restart the Nacre shell?", 0, 0),
        ("confirm-mise", "Restart the Nacre shell?", 0, 0),
    ] {
        let h = open(kind)?;
        assert!(
            shown(&h).iter().any(|t| t == title),
            "{kind}: {:?}",
            shown(&h)
        );
        let ids: Vec<String> = h
            .session
            .view_nodes()
            .iter()
            .map(|n| n.id.clone())
            .collect();
        let fields = ids
            .iter()
            .filter(|id| {
                id.rsplit('.')
                    .next()
                    .is_some_and(|l| l.len() == 2 && l.starts_with('f'))
            })
            .count();
        assert_eq!(fields, inputs, "{kind}: {ids:?}");
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|n| n.action.as_deref() == Some("launcher.form.choose"))
                .count(),
            choices,
            "{kind}"
        );
        assert_eq!(count(&h, "launcher.form", "cancel"), 1, "{kind}");
        assert_eq!(count(&h, "launcher.form", "submit"), 1, "{kind}");
        // The form covers the launcher: no results and no search field behind it.
        assert!(
            !ids.iter()
                .any(|id| id.contains("search") || id.contains("results")),
            "{kind}: {ids:?}"
        );
        // The first field is the one `launcher_form::open` focuses.
        if inputs > 0 {
            assert!(
                ids.iter().any(|id| id == crate::launcher_form::FIRST_FIELD),
                "{kind}: {ids:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn every_form_string_exists_in_english() {
    crate::i18n::init();
    let source = include_str!("../../../shell/ui/components/form.luau");
    let mut ids = 0;
    for part in source.split('"').skip(1).step_by(2) {
        if part.starts_with(&format!("{}.launcher.form-", "bead")) {
            ids += 1;
            assert_ne!(
                crate::i18n::tr(part, &[]),
                part,
                "{part} has no English text"
            );
        }
    }
    assert!(ids >= 30, "{ids}");
    for id in [
        "bead.launcher.form-error-reminder",
        "bead.launcher.form-error-webapp",
        "bead.launcher.form-error-tui",
        "bead.launcher.form-error-path",
        "bead.launcher.form-error-url",
        "bead.launcher.form-error-audio",
        "bead.launcher.form-error-failed",
        "bead.bar.stop-recording",
    ] {
        assert_ne!(crate::i18n::tr(id, &[]), id, "{id}");
    }
}

#[test]
fn choices_write_their_slot_and_the_sound_choice_lists_the_three_modes() -> Result<()> {
    let mut h = open("recording-screen")?;
    for mode in ["none", "system", "mixed"] {
        assert_eq!(
            count(&h, "launcher.form.choose", &format!("1:{mode}")),
            1,
            "{mode}"
        );
    }
    let changes = h
        .session
        .action_values("launcher.form.choose", Value::Text("1:mixed".into()))?;
    h.update(changes)?;
    assert_eq!(
        h.session.stores.get("launcher.form.v1"),
        Some(&Value::Text("mixed".into()))
    );
    // A malformed argument changes nothing.
    let changes = h
        .session
        .action_values("launcher.form.choose", Value::Text("9:x".into()))?;
    assert!(!changes.contains_key("launcher.form.v9"));
    let mut tui = open("tui")?;
    assert_eq!(count(&tui, "launcher.form.choose", "4:wezterm"), 1);
    let changes = tui
        .session
        .action_values("launcher.form.choose", Value::Text("4:wezterm".into()))?;
    tui.update(changes)?;
    assert_eq!(
        tui.session.stores.get("launcher.form.v4"),
        Some(&Value::Text("wezterm".into()))
    );
    Ok(())
}

#[test]
fn an_error_shows_in_the_form_and_closing_restores_the_launcher() -> Result<()> {
    let mut h = open("reminder")?;
    assert!(!shown(&h).iter().any(|t| t.contains("Enter a duration")));
    h.update(BTreeMap::from([(
        "launcher.form.error".into(),
        Value::Text("Enter a duration from 0.1 to 1440 minutes and a message".into()),
    )]))?;
    assert!(shown(&h).iter().any(|t| t.contains("Enter a duration")));
    h.update(
        crate::launcher_form::close()
            .into_iter()
            .filter(|(k, _)| k != "bead.focused")
            .collect(),
    )?;
    assert!(
        h.session
            .view_nodes()
            .iter()
            .any(|n| n.id.contains("launcher.search"))
    );
    assert!(count(&h, "launcher.form", "submit") == 0);
    Ok(())
}

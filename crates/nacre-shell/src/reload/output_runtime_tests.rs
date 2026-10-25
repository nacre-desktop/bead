// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead::backends::{Id, Output, Snapshot, Window, Workspace};
use bead::core::Value;

fn snapshot() -> Snapshot {
    Snapshot {
        connected: true,
        outputs: ["left", "right"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| Output {
                id: Id(format!("output-{i}")),
                name: name.into(),
                width: 1920,
                height: 1080,
                scale: 1.,
                refresh_mhz: 60_000,
                x: i as i32 * 1920,
                y: 0,
                focused: i == 0,
            })
            .collect(),
        workspaces: (0..2)
            .map(|i| Workspace {
                id: Id(format!("workspace-{i}")),
                name: format!("{i}"),
                index: i,
                output: Some(Id(format!("output-{i}"))),
                active: true,
                focused: i == 0,
                urgent: false,
                fixed_slot: false,
                visiting: false,
            })
            .collect(),
        windows: (0..2)
            .map(|i| Window {
                id: Id(format!("window-{i}")),
                title: format!("Title {i}"),
                app_id: "fixture".into(),
                workspace: Some(Id(format!("workspace-{i}"))),
                output: None,
                focused: i == 0,
                floating: false,
                fullscreen: None,
                urgent: false,
            })
            .collect(),
        capabilities: {
            let mut capabilities = bead::backends::Capabilities::empty();
            capabilities
                .features
                .insert("perOutputWorkspaces".into(), true);
            capabilities
        },
        ..Snapshot::default()
    }
}

#[test]
fn backend_projection_is_output_scoped_and_unknown_outputs_fail_closed() {
    let snapshot = snapshot();
    let left = backend_values(&snapshot, Some("left"), &Default::default()).unwrap();
    let right = backend_values(&snapshot, Some("right"), &Default::default()).unwrap();
    assert_eq!(left["bead.focused_title"], Value::Text("Title 0".into()));
    assert_eq!(right["bead.focused_title"], Value::Text(String::new()));
    for (values, expected) in [(&left, "workspace-0"), (&right, "workspace-1")] {
        let Value::List(rows) = &values["bead.workspaces"] else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], expected);
        let Value::List(windows) = &values["bead.windows"] else {
            panic!()
        };
        assert_eq!(windows.len(), 1, "resolve window output through workspace");
    }
    let missing = backend_values(&snapshot, Some("removed"), &Default::default()).unwrap();
    assert_eq!(missing["bead.workspaces"], Value::list(vec![]));
    assert_eq!(missing["bead.windows"], Value::list(vec![]));
    let global = backend_values(&snapshot, None, &Default::default()).unwrap();
    let Value::List(rows) = &global["bead.workspaces"] else {
        panic!()
    };
    assert_eq!(rows.len(), 2, "legacy projection retains all workspaces");
}

#[test]
fn moving_a_window_between_outputs_cannot_leave_the_old_title() {
    let mut snapshot = snapshot();
    snapshot.windows[0].output = Some(Id("output-1".into()));
    let left = backend_values(&snapshot, Some("left"), &Default::default()).unwrap();
    let right = backend_values(&snapshot, Some("right"), &Default::default()).unwrap();
    assert_eq!(left["bead.focused_title"], Value::Text(String::new()));
    assert_eq!(right["bead.focused_title"], Value::Text("Title 0".into()));
    snapshot.outputs.retain(|o| o.name != "right");
    let removed = backend_values(&snapshot, Some("right"), &Default::default()).unwrap();
    assert_eq!(removed["bead.focused_title"], Value::Text(String::new()));
}

#[test]
fn title_survives_transient_focus_clear_only_while_its_window_stays_active() {
    let mut snapshot = snapshot();
    let previous = snapshot.windows[0].id.clone();
    snapshot.windows[0].focused = false;
    let (title, remembered) = focused_title(&snapshot, Some("left"), Some(&previous));
    assert_eq!(title, "Title 0");
    assert_eq!(remembered, Some(previous.clone()));

    snapshot.windows.remove(0);
    snapshot.windows[0].focused = false;
    let (title, remembered) = focused_title(&snapshot, Some("left"), Some(&previous));
    assert!(title.is_empty(), "a workspace with no window shows Desktop");
    assert_eq!(remembered, None);

    snapshot.windows[0].focused = true;
    let (title, remembered) = focused_title(&snapshot, Some("right"), Some(&previous));
    assert_eq!(title, "Title 1", "a real focus change wins over retention");
    assert_eq!(remembered, Some(snapshot.windows[0].id.clone()));

    snapshot.windows[0].focused = false;
    let (title, remembered) = focused_title(&snapshot, Some("left"), remembered.as_ref());
    assert!(
        title.is_empty(),
        "changing outputs cannot retain another output's title"
    );
    assert_eq!(remembered, None);
    let (title, remembered) = focused_title(&snapshot, Some("removed"), Some(&previous));
    assert!(title.is_empty(), "unknown outputs fail closed");
    assert_eq!(remembered, None);

    let unscoped = snapshot.windows[0].id.clone();
    snapshot.windows[0].workspace = None;
    snapshot.windows[0].output = Some(Id("output-1".into()));
    snapshot.workspaces.clear();
    let (title, remembered) = focused_title(&snapshot, Some("right"), Some(&unscoped));
    assert!(
        title.is_empty(),
        "no active workspace cannot justify retaining a title"
    );
    assert_eq!(remembered, None);

    snapshot.connected = false;
    let (title, remembered) = focused_title(&snapshot, Some("right"), Some(&previous));
    assert!(
        title.is_empty(),
        "a disconnected backend shows no stale title"
    );
    assert_eq!(remembered, None);
}

#[test]
fn output_state_and_reload_failure_are_independent() -> Result<(), Box<dyn Error>> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("bead-output-reload-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&root)?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let source = r#"local ui=require("bead.ui")
return ui.Box{id="root",width="100%",height=30,fill="surface",
 ui.Text{id="label",width=150,height=20,text=ui.bind("local.title"),color="onSurface"}}
"#;
    std::fs::write(root.join("main.luau"), source)?;
    std::fs::write(
        root.join("behavior.luau"),
        "return {stores={},actions={},computed={}}",
    )?;
    let palette_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let palette = bead::render::Palette::read(&palette_path)?;
    let make = |name: &str| -> Result<Runtime, Box<dyn Error>> {
        let mut session = bead::ui::Session::new(
            root.clone(),
            "main.luau".into(),
            palette_path.clone(),
            palette.clone(),
            bead::ui::recovery::safe_bar(&palette),
        );
        session.stores = session.stores.reconcile(&BTreeMap::from([(
            "local.title".into(),
            Value::Text(name.into()),
        )]))?;
        Ok(Runtime::for_output(session, Some(name.into())))
    };
    let mut left = make("left")?;
    let mut right = make("right")?;
    left.viewport(1000, 800);
    right.viewport(1800, 1200);
    for runtime in [&mut left, &mut right] {
        assert!(runtime.stage(UiEvent::Reload)?.is_some());
        assert_eq!(
            runtime.pending.as_ref().unwrap().width,
            if runtime.output.as_deref() == Some("left") {
                1000.
            } else {
                1800.
            }
        );
        runtime.validate()?;
        assert!(runtime.commit());
    }
    left.stage(UiEvent::Model(BTreeMap::from([(
        "local.title".into(),
        Value::Text("changed".into()),
    )])))?;
    left.commit();
    let (wakeup, events) = std::sync::mpsc::channel();
    left.services(Box::new(move |event| {
        let _ = wakeup.send(event);
    }));
    left.stage(UiEvent::OutputName("late-name".into()))?;
    left.commit();
    assert!(matches!(
        events.recv_timeout(std::time::Duration::from_secs(1))?,
        UiEvent::Reload
    ));
    assert_eq!(left.output.as_deref(), Some("late-name"));
    assert_eq!(
        left.session.stores.get("bead.output.name"),
        Some(&Value::Text("late-name".into()))
    );
    assert_eq!(
        left.session.stores.get("local.title"),
        Some(&Value::Text("changed".into()))
    );
    std::fs::write(root.join("main.luau"), "return invalid syntax !")?;
    assert!(left.stage(UiEvent::Reload).is_err());
    assert_eq!(
        right.session.stores.get("local.title"),
        Some(&Value::Text("right".into()))
    );
    assert_eq!(
        left.session.stores.get("local.title"),
        Some(&Value::Text("changed".into()))
    );
    std::fs::write(root.join("main.luau"), source)?;
    assert!(left.stage(UiEvent::Reload)?.is_some());
    left.validate()?;
    assert!(left.commit());
    assert_eq!(
        left.session.stores.get("local.title"),
        Some(&Value::Text("changed".into()))
    );
    assert_eq!(
        right.session.stores.get("bead.output.name"),
        Some(&Value::Text("right".into()))
    );
    Ok(())
}

// SPDX-License-Identifier: MIT
//! A folder bar hosting a lip receives the host's cue state (`bead.edge.*`) without a UI error.
use bead::host::{FolderServices, FolderView};
use bead::wayland::{SceneSource, UiEvent};
use std::path::PathBuf;

#[test]
fn bar_view_accepts_the_bar_hosted_lip_cue_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/bar-with-panels");
    let services = FolderServices {
        desktop: ["clock".to_string(), "audio".to_string()].into(),
        commands: Default::default(),
    };
    let mut view = FolderView::load_with_services(
        root,
        "view/main.luau".into(),
        "palette.json".into(),
        "Bar",
        (800, 40),
        services,
    )
    .unwrap()
    .with_panel_actions(Some(bead::core::Edge::Top));
    view.stage(UiEvent::Reload).unwrap().unwrap();
    view.commit();
    let cue = [
        (
            "bead.edge.dwell".to_string(),
            bead::core::Value::Number(0.5),
        ),
        ("bead.edge.fade".to_string(), bead::core::Value::Number(1.)),
    ];
    view.stage(UiEvent::Model(cue.into())).unwrap();
}

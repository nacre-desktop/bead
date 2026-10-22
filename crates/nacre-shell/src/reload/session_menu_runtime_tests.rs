// SPDX-License-Identifier: GPL-3.0-only
//! B0023: the real session panel template against a fake daemon.
use super::*;
use crate::session_menu::tests::{lab, ok};
use std::time::Duration;

fn panel(client: &crate::nacre_source::Client) -> Runtime {
    let palette =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        palette.parent().unwrap().into(),
        "session/main.luau".into(),
        palette,
        colors,
        cold,
    );
    let mut runtime = Runtime::new(session)
        .with_session_menu(client.clone())
        .unwrap();
    runtime.state_file = None;
    runtime.session.restore_on_start(BTreeMap::new());
    runtime.services(Box::new(|_| {}));
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("session template must load");
    runtime.commit();
    runtime
}
fn press(runtime: &mut Runtime, id: &str) {
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id == format!("session-popout.{id}"))
        .unwrap_or_else(|| {
            panic!(
                "{id} missing in {:?}",
                runtime
                    .session
                    .view_nodes()
                    .iter()
                    .map(|n| n.id.clone())
                    .collect::<Vec<_>>()
            )
        });
    let rect = bead::ui::rect(&node.draw);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
}
fn refresh(runtime: &mut Runtime) {
    runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
    runtime.commit();
}

#[tokio::test(flavor = "current_thread")]
async fn panel_is_inert_without_the_capability() {
    let (dir, menu, mut calls) = lab(false, vec![]).await;
    let mut runtime = panel(&menu.client());
    refresh(&mut runtime);
    assert_eq!(
        runtime.session.stores.get("session.enabled"),
        Some(&bead::core::Value::Bool(false))
    );
    // Without the capability the entries carry no action, so nothing is clickable.
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .all(|n| !n.id.starts_with("session-popout.act-"))
    );
    refresh(&mut runtime);
    assert!(calls.try_recv().is_err());
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .all(|n| n.id != "session-popout.confirm")
    );
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test(flavor = "current_thread")]
async fn panel_lock_and_restart_confirmation_end_to_end() {
    let (dir, menu, mut calls) = lab(
        true,
        vec![
            ok(serde_json::json!({"changed":false})),
            ok(serde_json::json!({"changed":true})),
        ],
    )
    .await;
    let mut runtime = panel(&menu.client());
    refresh(&mut runtime);
    press(&mut runtime, "act-lock");
    assert_eq!(
        calls.recv().await.unwrap(),
        serde_json::json!({"action":"lock"})
    );
    for _ in 0..500 {
        refresh(&mut runtime);
        if runtime.session.stores.get("session.message")
            != Some(&bead::core::Value::Text(String::new()))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_ne!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
    press(&mut runtime, "act-reboot");
    refresh(&mut runtime);
    assert!(calls.try_recv().is_err(), "restart waits for confirmation");
    press(&mut runtime, "cancel");
    refresh(&mut runtime);
    press(&mut runtime, "act-reboot");
    refresh(&mut runtime);
    press(&mut runtime, "confirm");
    assert_eq!(
        calls.recv().await.unwrap(),
        serde_json::json!({"action":"reboot","confirmed":true})
    );
    for _ in 0..500 {
        refresh(&mut runtime);
        if runtime.session.stores.get("panel.open") == Some(&bead::core::Value::Bool(false)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&bead::core::Value::Bool(false))
    );
    std::fs::remove_dir_all(dir).ok();
}

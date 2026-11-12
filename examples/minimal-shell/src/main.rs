// SPDX-License-Identifier: MIT
//! A self-contained host: application state and identity never enter Bead.
use bead::{
    core::Value,
    host::FolderView as View,
    render::{Rect, Scene},
    wayland::{Options, SceneSource, UiEvent, WindowSpec},
};
use std::{collections::BTreeMap, error::Error, path::PathBuf, sync::Arc};

const APP_ID: &str = "org.bead.Example.MinimalShell";
const TITLE: &str = "Bead minimal shell";
const SIZE: (u32, u32) = (440, 300);

fn fallback() -> Scene {
    Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: SIZE.0 as f32,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., SIZE.0 as f32, SIZE.1 as f32),
        primitives: vec![],
    }
}

fn accept(view: &mut View, event: UiEvent) -> Result<Scene, Box<dyn Error>> {
    view.check_render(event)
}

fn check(mut view: View) -> Result<(), Box<dyn Error>> {
    let scene = accept(&mut view, UiEvent::Reload)?;
    if !scene.primitives.iter().any(
        |p| matches!(p, bead::render::Primitive::Text {text, ..} if text == "Hello from a plugin"),
    ) {
        return Err("bundled hello plugin was not rendered".into());
    }
    accept(
        &mut view,
        UiEvent::Action {
            name: "increment".into(),
            argument: Value::Bool(true),
        },
    )?;
    if view.session.stores.get("count") != Some(&Value::Number(1.)) {
        return Err("button action did not update durable state".into());
    }
    let focused = view
        .session
        .focus_id("minimal.name")?
        .ok_or("missing text input")?;
    view.session.commit(focused);
    accept(
        &mut view,
        UiEvent::Key {
            text: "Reader".into(),
            backspace: false,
        },
    )?;
    accept(&mut view, UiEvent::Reload)?;
    if view.session.stores.get("name") != Some(&Value::Text("Reader".into())) {
        return Err("input state did not survive reload".into());
    }
    println!(
        "MINIMAL_SHELL_CHECK_OK app_id={APP_ID} action=1 input=Reader reload=preserved plugin=hello"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut checking = false;
    let mut seconds = None;
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => checking = true,
            "--root" => root = args.next().ok_or("missing UI root")?.into(),
            "--seconds" => {
                let duration = args.next().ok_or("missing duration")?.parse::<f64>()?;
                if !duration.is_finite() || duration <= 0. {
                    return Err("duration must be finite and positive".into());
                }
                seconds = Some(duration);
            }
            _ => {
                return Err(format!(
                    "unknown argument {arg}; use --check, --root DIR or --seconds N"
                )
                .into());
            }
        }
    }
    if checking {
        return check(View::load(
            root.clone(),
            "view/main.luau".into(),
            root.join("palette.json"),
            TITLE,
            SIZE,
        )?);
    }
    let window = WindowSpec {
        app_id: APP_ID.into(),
        title: TITLE.into(),
        command_targets: vec![],
        size: SIZE,
        min_size: (320, 240),
        size_for_output: None,
        on_resize: None,
        fallback: fallback(),
        factory: Arc::new(move |_| {
            View::load(
                root.clone(),
                "view/main.luau".into(),
                root.join("palette.json"),
                TITLE,
                SIZE,
            )
            .map(|v| Box::new(v) as Box<dyn SceneSource>)
            .map_err(|e| e.to_string())
        }),
        model: Arc::new(|_| BTreeMap::new()),
    };
    bead::wayland::run(
        Options {
            namespace: "bead-minimal-shell".into(),
            window: Some(window),
            app: Some(vec![]),
            interactive: true,
            keyboard_focus: true,
            seconds,
            ..Default::default()
        },
        fallback(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_bundle_renders_plugin_input_action_and_preserves_state_on_reload() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui");
        check(
            View::load(
                root.clone(),
                "view/main.luau".into(),
                root.join("palette.json"),
                TITLE,
                SIZE,
            )
            .unwrap(),
        )
        .unwrap();
    }
}

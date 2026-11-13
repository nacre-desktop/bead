// SPDX-License-Identifier: MIT
//! A self-contained host: application state and identity never enter Bead.
use bead::{
    core::Value,
    host::FolderView as View,
    render::Scene,
    wayland::{Options, SceneSource, UiEvent, WindowSpec},
};
use std::{error::Error, path::PathBuf, sync::Arc};

const APP_ID: &str = "org.bead.Example.MinimalShell";
const TITLE: &str = "Bead minimal shell";
const SIZE: (u32, u32) = (440, 300);

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
    let mut window = WindowSpec::new(
        APP_ID,
        TITLE,
        SIZE,
        Scene::empty(SIZE.0 as f32, SIZE.1 as f32),
        Arc::new(move |_| {
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
    );
    window.min_size = (320, 240);
    let mut options = Options::default();
    options.namespace = "bead-minimal-shell".into();
    options.window = Some(window);
    options.app = Some(vec![]);
    options.interactive = true;
    options.seconds = seconds;
    bead::wayland::run(options, Scene::empty(SIZE.0 as f32, SIZE.1 as f32))
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

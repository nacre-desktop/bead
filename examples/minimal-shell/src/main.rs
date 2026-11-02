// SPDX-License-Identifier: MIT
//! A self-contained host: application state and identity never enter Bead.
use bead::{
    core::Value,
    render::{Damage, Palette, PixelFormat, Rect, RendererKind, Scene, ShmTarget},
    ui::Session,
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

struct View {
    session: Session,
    pending: Option<Scene>,
}
impl View {
    fn load(root: PathBuf) -> Result<Self, Box<dyn Error>> {
        // Bundled plugins only: the example does not discover user config or write state.
        bead::ui::plugins::configure(
            &root,
            bead::ui::plugins::Config {
                system: vec![root.join("plugins")],
                user: None,
                state: None,
            },
        );
        let palette = root.join("palette.json");
        let mut session = Session::new(
            root,
            "view/main.luau".into(),
            palette.clone(),
            Palette::read(&palette)?,
            fallback(),
        );
        session.set_accessibility_label(TITLE);
        session.set_viewport(SIZE.0, SIZE.1);
        Ok(Self {
            session,
            pending: None,
        })
    }
}

impl SceneSource for View {
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn viewport(&mut self, width: u32, height: u32) -> bool {
        self.session.set_viewport(width, height)
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        let scene = match event {
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Release => self.session.release()?,
            UiEvent::Hover { x, y } => self.session.hover(x, y)?,
            UiEvent::Drag { x, y } => self.session.drag(x, y)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "tab" => self.session.focus_next(shift)?,
                "enter" | "space" => self.session.activate_focused()?,
                "left" => self
                    .session
                    .edit_key(bead::ui::input::EditKey::Left, shift)?,
                "right" => self
                    .session
                    .edit_key(bead::ui::input::EditKey::Right, shift)?,
                _ => None,
            },
            UiEvent::Action { name, argument } => {
                let patch = self.session.action_values(&name, argument)?;
                self.session.update(patch)?
            }
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::WatchError(error) => return Err(error.into()),
            _ => None,
        };
        self.pending = scene.clone();
        Ok(scene)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn state_summary(&self) -> String {
        self.session.state_summary()
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn commit(&mut self) -> bool {
        if let Some(scene) = self.pending.take() {
            self.session.commit(scene);
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        self.session.reject(error)
    }
}

fn accept(view: &mut View, event: UiEvent) -> Result<Scene, Box<dyn Error>> {
    let scene = view.stage(event)?.ok_or("expected a scene update")?;
    view.validate()?;
    let mut renderer = bead::render::renderer(RendererKind::Cpu)?;
    renderer.prepare(&scene, 1.)?;
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    renderer.render(
        &scene,
        ShmTarget {
            bytes: &mut pixels,
            width: SIZE.0,
            height: SIZE.1,
            scale: 1.,
            format: PixelFormat::Rgba,
        },
        Damage::full(SIZE.0, SIZE.1),
    )?;
    view.commit();
    Ok(scene)
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
        return check(View::load(root)?);
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
            View::load(root.clone())
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
        check(View::load(root).unwrap()).unwrap();
    }
}

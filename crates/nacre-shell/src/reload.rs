// SPDX-License-Identifier: GPL-3.0-only
use bead_render::Scene;
use bead_wayland::{SceneSource, UiEvent};
use std::error::Error;
pub struct Runtime {
    pub session: bead_ui::Session,
    pending: Option<Scene>,
}
impl Runtime {
    pub fn new(session: bead_ui::Session) -> Self {
        Self {
            session,
            pending: None,
        }
    }
}
impl SceneSource for Runtime {
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn viewport(&mut self, width: u32, height: u32) -> bool {
        self.session.set_viewport(width, height)
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        self.pending = None;
        let reload = matches!(event, UiEvent::Reload);
        let mut scene = match event {
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::Hover { x, y } => self.session.hover(x, y)?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "clipboard-copy" | "clipboard-cut" => {
                    if let Some(text) = self.session.selected_text() {
                        use std::io::Write;
                        let mut child = std::process::Command::new("wl-copy")
                            .args(["--type", "text/plain;charset=utf-8"])
                            .stdin(std::process::Stdio::piped())
                            .spawn()?;
                        child
                            .stdin
                            .take()
                            .ok_or("clipboard pipe")?
                            .write_all(text.as_bytes())?;
                        if !child.wait()?.success() {
                            return Err("clipboard copy failed".into());
                        }
                        if command == "clipboard-cut" {
                            self.session.key("", false)?
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                "clipboard-paste" => {
                    let output = std::process::Command::new("wl-paste")
                        .arg("--no-newline")
                        .output()?;
                    if !output.status.success() {
                        return Err("clipboard unavailable".into());
                    }
                    if output.stdout.len() > 65536 {
                        return Err("clipboard text exceeds64KiB".into());
                    }
                    self.session
                        .key(&String::from_utf8(output.stdout)?, false)?
                }
                "tab" => self.session.focus_next(shift)?,
                "enter" => self.session.activate_focused()?,
                "up" | "down" => self.session.focus_next(command == "up")?,
                "left" | "right" if self.session.surrounding_text().is_none() => {
                    self.session.focus_next(command == "left")?
                }
                key => {
                    let key = match key {
                        "left" => bead_ui::input::EditKey::Left,
                        "right" => bead_ui::input::EditKey::Right,
                        "home" => bead_ui::input::EditKey::Home,
                        "end" => bead_ui::input::EditKey::End,
                        "delete" => bead_ui::input::EditKey::Delete,
                        "select-all" => bead_ui::input::EditKey::SelectAll,
                        _ => return Err("unsupported input command".into()),
                    };
                    self.session.edit_key(key, shift)?
                }
            },
            UiEvent::WatchError(error) => return Err(error.into()),
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
        };
        if let Some(scene) = &mut scene {
            for draw in &mut scene.primitives {
                if let bead_render::Primitive::Image { path, .. } = draw
                    && let Some(icon) = path.to_str().and_then(|s| s.strip_prefix("@icon:"))
                {
                    *path = if std::env::var_os("BEAD_MATCHED_ICONS").is_some() {
                        std::env::var_os("BEAD_ICON_PATH")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| "shell/sample.svg".into())
                    } else {
                        freedesktop_icons::lookup(icon)
                            .with_size(48)
                            .find()
                            .unwrap_or_else(|| "shell/sample.svg".into())
                    };
                    let data = if !reload {
                        if let Some(data) = self.session.asset_snapshot(path) {
                            data
                        } else {
                            std::sync::Arc::from(std::fs::read(&*path)?)
                        }
                    } else {
                        std::sync::Arc::from(std::fs::read(&*path)?)
                    };
                    scene.asset_data.insert(path.clone(), data);
                }
            }
            if reload {
                scene.font_files = crate::font_sources::select(scene)?;
            }
        }
        if let Some(scene) = &mut scene {
            self.session.decorate_inputs(scene)?;
        }
        self.session.check_sources()?;
        self.pending = scene.clone();
        Ok(scene)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn resource_counts(&self) -> String {
        self.session.resource_counts()
    }
    fn state_summary(&self) -> String {
        self.session.state_summary()
    }
    fn commit(&mut self) {
        if let Some(scene) = self.pending.take() {
            self.session.commit(scene);
        }
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        let mut scene = self.session.reject(error);
        if scene.font_files.is_empty()
            && let Ok(files) = crate::font_sources::select(&scene)
        {
            scene.font_files = files;
        }
        scene
    }
}

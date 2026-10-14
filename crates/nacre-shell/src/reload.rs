// SPDX-License-Identifier: GPL-3.0-only
use bead_backends::Backend;
use bead_render::Scene;
use bead_wayland::{SceneSource, UiEvent};
use std::error::Error;
pub struct Runtime {
    pub session: bead_ui::Session,
    pending: Option<Scene>,
    backend: Option<std::sync::Arc<crate::backends::Native>>,
    subscription: Option<bead_backends::Subscription>,
    desktop: Option<std::sync::Arc<bead_services::Desktop>>,
    desktop_subscription: Option<bead_services::Subscription>,
    menu_item: String,
    bar: bool,
}
impl Runtime {
    pub fn new(session: bead_ui::Session) -> Self {
        Self {
            session,
            pending: None,
            backend: None,
            subscription: None,
            desktop: None,
            desktop_subscription: None,
            menu_item: String::new(),
            bar: false,
        }
    }
    pub fn with_desktop(mut self) -> Self {
        let desktop = bead_services::Desktop::start();
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::desktop::values(&desktop.snapshot(), ""))
            .expect("desktop store types");
        self.desktop = Some(desktop);
        self.bar = true;
        self
    }
    fn desktop_action(
        &mut self,
        name: &str,
        arg: bead_core::Value,
        delta: Option<f64>,
        context_menu: bool,
    ) -> Result<(), Box<dyn Error>> {
        use bead_services::Action;
        let desktop = self.desktop.as_ref().ok_or("desktop unavailable")?;
        let snapshot = desktop.snapshot();
        let action = match name {
            "services.audio" => {
                if let Some(delta) = delta {
                    Action::Volume(
                        (snapshot.audio.volume.ok_or("audio unavailable")? + delta.signum() * 0.05)
                            .clamp(0., 1.),
                    )
                } else {
                    Action::Mute(!snapshot.audio.muted.ok_or("audio unavailable")?)
                }
            }
            "services.tray" => {
                let bead_core::Value::Text(item) = arg else {
                    return Err("tray ID required".into());
                };
                let tray = snapshot
                    .tray
                    .iter()
                    .find(|t| t.id == item)
                    .ok_or("stale tray ID")?;
                if tray.item_is_menu || context_menu {
                    self.menu_item = if self.menu_item == item {
                        String::new()
                    } else {
                        item.clone()
                    };
                    Action::TrayPrepare { item }
                } else {
                    Action::TrayActivate { item, x: 0, y: 0 }
                }
            }
            "services.menu" => {
                let bead_core::Value::Text(value) = arg else {
                    return Err("menu ID required".into());
                };
                let (item, entry) = value.rsplit_once('|').ok_or("menu ID required")?;
                self.menu_item.clear();
                Action::TrayMenu {
                    item: item.into(),
                    entry: entry.parse()?,
                }
            }
            _ => return Err("unknown desktop action".into()),
        };
        desktop
            .action(action)
            .map_err(|e| -> Box<dyn Error> { e.into() })
    }
    pub fn with_backend(mut self, backend: std::sync::Arc<crate::backends::Native>) -> Self {
        let values = backend_values(&backend.snapshot());
        self.session.stores = self
            .session
            .stores
            .reconcile(&values)
            .expect("valid native defaults");
        self.backend = Some(backend);
        self
    }
    fn backend_action(&self, name: &str, arg: bead_core::Value) -> Result<(), Box<dyn Error>> {
        let backend = self.backend.as_ref().ok_or("backend unavailable")?;
        let bead_core::Value::Text(id) = arg else {
            return Err("backend action requires a stable ID".into());
        };
        let id = bead_backends::Id(id);
        let action = match name {
            "backend.workspace.switch" => bead_backends::Action::SwitchWorkspace { workspace: id },
            "backend.window.focus" => bead_backends::Action::FocusWindow { window: id },
            "backend.window.close" => bead_backends::Action::CloseWindow { window: id },
            "backend.windowTrash" => bead_backends::Action::WindowTrash { window: id },
            "backend.scratchpad" => bead_backends::Action::Scratchpad { window: Some(id) },
            "backend.windowCapture" => bead_backends::Action::WindowCapture { window: id },
            _ => {
                return Err(bead_backends::Error::Unsupported {
                    operation: name.into(),
                    backend: backend.snapshot().backend.clone(),
                }
                .into());
            }
        };
        let started = std::time::Instant::now();
        backend.action(action)?;
        println!(
            "BACKEND_ACTION name={name} request_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(())
    }
}
impl SceneSource for Runtime {
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        let wakeup = std::sync::Arc::new(wakeup);
        if let Some(desktop) = &self.desktop {
            let wakeup = wakeup.clone();
            self.desktop_subscription =
                Some(desktop.subscribe(move |snapshot| wakeup(UiEvent::Desktop(snapshot))));
        }
        if let Some(backend) = &self.backend {
            self.subscription = Some(backend.subscribe(Box::new(move |event| {
                if let bead_backends::Event::Snapshot(snapshot) = event {
                    wakeup(UiEvent::Backend(snapshot));
                }
            })));
        }
    }
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
    fn stage(&mut self, mut event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        self.pending = None;
        let host_action = match &event {
            UiEvent::Click { x, y }
            | UiEvent::ContextMenu { x, y }
            | UiEvent::Scroll { x, y, .. } => self.session.action_at(*x, *y),
            UiEvent::Command { command, .. } if command == "enter" => self.session.focused_action(),
            _ => None,
        };
        if let Some((_, name, arg)) = host_action.as_ref()
            && name.starts_with("services.")
        {
            let delta = if let UiEvent::Scroll { delta, .. } = &event {
                Some(*delta)
            } else {
                None
            };
            if delta.is_none() || name == "services.audio" {
                self.desktop_action(
                    name,
                    arg.clone(),
                    delta,
                    matches!(event, UiEvent::ContextMenu { .. }),
                )?;
            }
            event = UiEvent::Desktop(self.desktop.as_ref().unwrap().snapshot());
        }
        if let Some((id, name, arg)) = host_action
            && name.starts_with("backend.")
            && !matches!(event, UiEvent::Scroll { .. } | UiEvent::ContextMenu { .. })
        {
            self.backend_action(&name, arg)?;
            if !self.bar {
                let scene = self.session.focus_id(&id)?;
                self.pending = scene.clone();
                return Ok(scene);
            }
            event = UiEvent::Relayout;
        }
        let reload = matches!(event, UiEvent::Reload);
        let mut scene = match event {
            UiEvent::Desktop(snapshot) => self
                .session
                .update(crate::desktop::values(&snapshot, &self.menu_item))?,
            UiEvent::Backend(snapshot) => {
                println!(
                    "BACKEND_UPDATE backend={} connected={} workspaces={} windows={}",
                    snapshot.backend,
                    snapshot.connected,
                    snapshot.workspaces.len(),
                    snapshot.windows.len()
                );
                self.session.update(backend_values(&snapshot))?
            }
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
            UiEvent::ContextMenu { .. } => None,
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
        };
        if let Some(scene) = &mut scene {
            if let Some(desktop) = &self.desktop {
                crate::desktop::icons(scene, &desktop.snapshot())?;
            }
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
            if self.bar {
                scene.offset = (0., 0.);
                scene.slide = None;
                scene.bounds = bead_render::Rect::new(0., 0., scene.width, 50.);
            }
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

fn backend_values(
    snapshot: &bead_backends::Snapshot,
) -> std::collections::BTreeMap<String, bead_core::Value> {
    use bead_core::Value;
    let mut values = std::collections::BTreeMap::from([
        ("bead.connected".into(), Value::Bool(snapshot.connected)),
        (
            "bead.focused_title".into(),
            Value::Text(
                snapshot
                    .focused_window()
                    .map_or("", |w| w.title.as_str())
                    .into(),
            ),
        ),
        (
            "bead.outputs".into(),
            Value::List(
                snapshot
                    .outputs
                    .iter()
                    .map(|o| serde_json::to_value(o).unwrap())
                    .collect(),
            ),
        ),
        (
            "bead.workspaces".into(),
            Value::List(
                snapshot
                    .workspaces
                    .iter()
                    .map(|w| serde_json::to_value(w).unwrap())
                    .collect(),
            ),
        ),
        (
            "bead.windows".into(),
            Value::List(
                snapshot
                    .windows
                    .iter()
                    .map(|w| serde_json::to_value(w).unwrap())
                    .collect(),
            ),
        ),
    ]);
    for (name, supported) in &snapshot.capabilities.features {
        values.insert(format!("bead.cap.{name}"), Value::Bool(*supported));
    }
    values
}

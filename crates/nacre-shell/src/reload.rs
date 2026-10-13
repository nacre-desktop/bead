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
                    *path = freedesktop_icons::lookup(icon)
                        .with_size(48)
                        .find()
                        .unwrap_or_else(|| std::path::PathBuf::from("shell/sample.svg"));
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
        self.session.check_sources()?;
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

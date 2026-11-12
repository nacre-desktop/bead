// SPDX-License-Identifier: MIT
//! Small reusable host pieces for applications authored as folders of Luau files.
use crate::{
    render::{Damage, Palette, PixelFormat, Rect, RendererKind, Scene, ShmTarget},
    ui::Session,
    wayland::{SceneSource, UiEvent},
};
use std::{error::Error, path::PathBuf};

/// An empty, surface-sized fallback for a folder host.
pub fn empty_scene(size: (u32, u32)) -> Scene {
    Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: size.0 as f32,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., size.0 as f32, size.1 as f32),
        primitives: vec![],
    }
}

/// A Luau-backed scene source rooted in an application's folder.
pub struct FolderView {
    pub session: Session,
    pending: Option<Scene>,
    size: (u32, u32),
}
impl FolderView {
    pub fn load(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
    ) -> Result<Self, Box<dyn Error>> {
        crate::ui::plugins::configure(
            &root,
            crate::ui::plugins::Config {
                system: vec![root.join("plugins")],
                user: None,
                state: None,
            },
        );
        let palette = if palette.is_absolute() {
            palette
        } else {
            root.join(palette)
        };
        let colors = Palette::read(&palette)?;
        let fallback = empty_scene(size);
        let mut session = Session::new(root, view, palette, colors, fallback);
        session.set_accessibility_label(title);
        session.set_viewport(size.0, size.1);
        Ok(Self {
            session,
            pending: None,
            size,
        })
    }
    fn surface_scene(&self, mut scene: Scene) -> Scene {
        // Folder views occupy their surface; Luau layout reserves a panel shadow
        // margin by default, which would clip thin bars and offset their input.
        scene.offset = (0., 0.);
        scene.bounds = Rect::new(0., 0., self.size.0 as f32, self.size.1 as f32);
        scene
    }
    /// Stage, validate, and render one update on the CPU without a display connection.
    pub fn check_render(&mut self, event: UiEvent) -> Result<Scene, Box<dyn Error>> {
        let scene = self.stage(event)?.ok_or("expected a scene update")?;
        self.validate()?;
        let mut renderer = crate::render::renderer(RendererKind::Cpu)?;
        renderer.prepare(&scene, 1.)?;
        let mut pixels = vec![0; (self.size.0 * self.size.1 * 4) as usize];
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width: self.size.0,
                height: self.size.1,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(self.size.0, self.size.1),
        )?;
        self.commit();
        Ok(scene)
    }
}
impl SceneSource for FolderView {
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn viewport(&mut self, w: u32, h: u32) -> bool {
        self.size = (w, h);
        self.session.set_viewport(w, h)
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
                    .edit_key(crate::ui::input::EditKey::Left, shift)?,
                "right" => self
                    .session
                    .edit_key(crate::ui::input::EditKey::Right, shift)?,
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
            UiEvent::WatchError(e) => return Err(e.into()),
            _ => None,
        };
        let scene = scene.map(|scene| self.surface_scene(scene));
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
            self.session.commit(scene)
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        let scene = self.session.reject(error);
        self.surface_scene(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_bar_removes_panel_margin_and_uses_surface_viewport() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-bar");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Bar",
            (440, 36),
        )
        .unwrap();
        // Reproduce the old primary path: output-sized layout, then a 36px buffer.
        view.viewport(1280, 720);
        let raw = view.session.reload().unwrap();
        assert_eq!(raw.offset, (32., 32.));
        for primitive in &raw.primitives {
            if let crate::render::Primitive::Text { rect, .. } = primitive {
                assert!(
                    rect.h > 36.,
                    "old path lays out labels against the full output height"
                );
            }
        }
        assert!(view.viewport(1280, 36));
        let scene = view.check_render(UiEvent::Reload).unwrap();
        assert_eq!(scene.offset, (0., 0.));
        assert_eq!(scene.bounds, Rect::new(0., 0., 1280., 36.));
        let mut renderer = crate::render::renderer(RendererKind::Cpu).unwrap();
        for (scene, visible) in [(&raw, false), (&scene, true)] {
            let mut bytes = vec![0; 1280 * 36 * 4];
            renderer.prepare(scene, 1.).unwrap();
            renderer
                .render(
                    scene,
                    ShmTarget {
                        bytes: &mut bytes,
                        width: 1280,
                        height: 36,
                        scale: 1.,
                        format: PixelFormat::Rgba,
                    },
                    Damage::full(1280, 36),
                )
                .unwrap();
            assert_eq!(
                bytes[64 * 4 + 3] != 0,
                visible,
                "surface fill along the top edge, outside rounded corners"
            );
        }
        for label in ["Hello bar", "12:34"] {
            assert!(scene.primitives.iter().any(|p| matches!(p,
                crate::render::Primitive::Text { text, rect, .. }
                if text == label && rect.y < 36. && rect.x >= 0. && rect.x + rect.w <= 1280.
            )));
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shell-wide hide: unmap every bar and panel layer surface, keep all view state,
//! and remap on show. Unmapping drops the surface's exclusive zone, input region and
//! keyboard focus together, so windows regain the space. Wallpaper surfaces are
//! separate and untouched.
use super::*;
impl App {
    pub(super) fn set_hidden(&mut self, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        if hidden {
            // The dismiss catcher would otherwise keep swallowing clicks.
            drop(self.dismiss.take());
            for s in &mut self.surfaces {
                s.layer.wl_surface().attach(None, 0, 0);
                s.layer.commit();
                s.configured = false;
                s.redraw = false;
                s.callbacks.invalidate();
                s.decorations = None;
                s.previous = None;
                s.visual_damage = Default::default();
            }
            return;
        }
        let keyboard_dismiss = self.panel_target
            && self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|spec| spec.keyboard_focus);
        if keyboard_dismiss
            && let Some(output) = self.panel_output.clone()
            && let Err(error) = self.create_dismiss(&output)
        {
            self.fail(error);
        }
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            if self.surfaces[i].configured {
                // Created while hidden: never mapped, so no new configure will come.
                self.draw(i, &qh);
            } else {
                // Remap handshake: commit without a buffer, then draw on configure.
                self.surfaces[i].layer.commit();
            }
        }
    }
}

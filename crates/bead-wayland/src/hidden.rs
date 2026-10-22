// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shell-wide hide: destroy every bar and panel layer surface, keep all view state,
//! and recreate them on show. Destroying drops the exclusive zone, input region and
//! keyboard focus together, so windows regain the space. Layer-shell resets a merely
//! unmapped surface to its initial state, so recreating through `add_output` (the
//! start-up and hot-plug path) is the one place that owns anchors, size and margins.
//! Wallpaper surfaces are separate and untouched.
use super::*;
impl App {
    pub(super) fn set_hidden(&mut self, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        self.hidden = hidden;
        // The dismiss catcher would otherwise keep swallowing clicks.
        drop(self.dismiss.take());
        self.surfaces.clear();
        if hidden {
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
        let outputs: Vec<_> = self.output.outputs().collect();
        for output in outputs {
            self.add_output(&qh, output);
        }
    }
}

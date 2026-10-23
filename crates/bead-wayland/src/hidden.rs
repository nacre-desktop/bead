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
        if hidden {
            self.release_overlay();
        }
        self.rebuild_surfaces();
    }
    /// A persistent "no bar" state, separate from the hide toggle: bar surfaces are
    /// not created while off, panel surfaces still are.
    pub(super) fn set_bar_off(&mut self, off: bool) {
        if self.bar_off != off {
            self.bar_off = off;
            self.rebuild_surfaces();
        }
    }
    /// Destroy every layer surface and recreate those the current state allows.
    fn rebuild_surfaces(&mut self) {
        // The dismiss catcher would otherwise keep swallowing clicks.
        drop(self.dismiss.take());
        self.surfaces.clear();
        // Edge surfaces are recreated after the bars: later surfaces stack above earlier
        // ones in a layer, and a lip must sit above the bar's frame tiles.
        self.edge_surfaces.clear();
        if self.hidden {
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
        self.sync_edges();
    }
}
impl App {
    /// A [`ShellControl`] answer changed: bar presence, a now-forbidden open panel,
    /// or the decoration variant.
    pub(super) fn apply_control(&mut self) {
        let Some(control) = self.options.control.clone() else {
            return;
        };
        self.set_bar_off(!control.bar_enabled());
        if self.panel_target
            && self
                .panel_name
                .as_deref()
                .is_some_and(|name| !control.panel_allowed(name))
        {
            self.close_panel();
        }
        // The bar's thickness follows the frame settings live.
        let thickness = self.bar_thickness();
        for surface in self.surfaces.iter().filter(|s| !s.companion) {
            if self.options.bar {
                surface.layer.set_size(0, thickness);
                surface.layer.set_exclusive_zone(thickness as i32);
                surface.layer.commit();
            }
        }
        self.sync_edges();
        let variant = control.decoration_variant();
        if variant != self.decoration_variant {
            self.decoration_variant = variant;
            if let Err(error) = self.sync_all_decorations() {
                eprintln!("DECORATION_ERROR {error}");
            }
        }
    }
}

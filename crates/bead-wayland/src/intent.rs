// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hover intent for edge-anchored chrome: a dwell timer (a pointer must rest on a lip
//! before its panel opens), a gate that keeps a just-closed lip from reopening under the
//! pointer, and the lip geometry. Pure state machines: no Wayland objects and no clock;
//! the shell loop feeds them time and acts on the answers, so every rule is unit tested.
use bead_core::Edge;
use bead_render::Rect;
use std::collections::BTreeSet;

/// One edge-anchored hover/click target. Which panel it opens is configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct Lip {
    /// Stable id (also the gate key).
    pub id: String,
    pub edge: Edge,
    /// Extent along the edge, centred on it.
    pub length: f32,
    /// Extent across the edge: the frame width, or the bar height for a bar lip.
    pub thickness: f32,
    /// Panel view the lip opens.
    pub opens: String,
}
impl Lip {
    /// The lip's rectangle in output logical coordinates (`NacreFrameLips`: the top lip is
    /// `(w - 208) / 2, 0, 208, header`; the side lips `0 | w - frame, (h - 144) / 2, frame, 144`).
    pub fn rect(&self, (w, h): (f32, f32)) -> Rect {
        let (length, thickness) = (self.length.max(0.), self.thickness.max(0.));
        match self.edge {
            Edge::Top => Rect::new((w - length) / 2., 0., length, thickness),
            Edge::Bottom => Rect::new((w - length) / 2., h - thickness, length, thickness),
            Edge::Left => Rect::new(0., (h - length) / 2., thickness, length),
            Edge::Right => Rect::new(w - thickness, (h - length) / 2., thickness, length),
        }
    }
    pub fn contains(&self, output: (f32, f32), x: f32, y: f32) -> bool {
        let r = self.rect(output);
        r.w > 0. && r.h > 0. && x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h
    }
}

/// Port of `NacreHoverDwell`. With motion the fill advances per frame (a stalled frame
/// never consumes the remaining fill) and, once full, holds two more frames before it
/// fires, so the finished trace is seen. Without motion a plain timer fires after the
/// full delay. A zero delay fires at once.
#[derive(Clone, Debug)]
pub struct Dwell {
    duration_ms: u32,
    progress: f32,
    waiting: bool,
    animated: bool,
    full_frames: u8,
}
impl Default for Dwell {
    fn default() -> Self {
        Self {
            duration_ms: 350,
            progress: 0.,
            waiting: false,
            animated: false,
            full_frames: 2,
        }
    }
}
/// What the caller must do after starting a dwell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Fire now (zero delay).
    Fire,
    /// Call [`Dwell::advance`] every frame.
    Frames,
    /// Call [`Dwell::elapse`] after this many milliseconds.
    Timer(u32),
}
impl Dwell {
    pub fn waiting(&self) -> bool {
        self.waiting
    }
    pub fn progress(&self) -> f32 {
        self.progress
    }
    pub fn cancel(&mut self) {
        self.waiting = false;
        self.progress = 0.;
        self.full_frames = 2;
    }
    pub fn start(&mut self, duration_ms: u32, motion: bool) -> Start {
        self.cancel();
        self.duration_ms = duration_ms;
        self.waiting = true;
        self.animated = motion;
        if duration_ms == 0 {
            self.progress = 1.;
            self.waiting = false;
            Start::Fire
        } else if motion {
            Start::Frames
        } else {
            self.progress = 1.;
            Start::Timer(duration_ms)
        }
    }
    /// One animation frame of `seconds`; true when the dwell fires.
    pub fn advance(&mut self, seconds: f32) -> bool {
        if !self.waiting || !self.animated {
            return false;
        }
        if self.progress < 1. {
            let step = seconds.clamp(0., 0.035);
            self.progress = (self.progress + step * 1000. / self.duration_ms.max(1) as f32).min(1.);
            false
        } else {
            self.full_frames = self.full_frames.saturating_sub(1);
            if self.full_frames == 0 {
                self.waiting = false;
                true
            } else {
                false
            }
        }
    }
    /// The plain timer ran out; true when the dwell fires.
    pub fn elapse(&mut self) -> bool {
        std::mem::replace(&mut self.waiting, false)
    }
    /// Motion was switched off mid-dwell: finish on a timer for the unfilled part.
    /// `None` when no animated dwell is running.
    pub fn motion_off(&mut self) -> Option<u32> {
        if !(self.waiting && self.animated) {
            return None;
        }
        self.animated = false;
        let remaining = (self.duration_ms as f32 * (1. - self.progress)).round() as u32;
        self.progress = 1.;
        Some(remaining.max(1))
    }
}

/// `NacreHoverIntent.blocked/rearm` for lips: after a panel is dismissed while the pointer
/// is still on a lip, that lip cannot open again until the pointer leaves it.
#[derive(Clone, Debug, Default)]
pub struct Gate {
    blocked: BTreeSet<String>,
}
impl Gate {
    pub fn can_open(&self, key: &str) -> bool {
        !self.blocked.contains(key)
    }
    pub fn dismiss(&mut self, key: &str) {
        self.blocked.insert(key.to_owned());
    }
    pub fn rearm(&mut self, key: &str) {
        self.blocked.remove(key);
    }
    pub fn forget_prefix(&mut self, prefix: &str) {
        self.blocked.retain(|k| !k.starts_with(prefix));
    }
}

/// Whether a hover may start a dwell now (`NacreHoverIntent.canOpen` plus the
/// `clickEdgeMenus` switch): no button held, hover opening enabled and not blocked.
pub fn may_dwell(buttons_held: bool, click_to_open: bool, allowed: bool, gate_open: bool) -> bool {
    !buttons_held && !click_to_open && allowed && gate_open
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lip(edge: Edge, length: f32, thickness: f32) -> Lip {
        Lip {
            id: "x".into(),
            edge,
            length,
            thickness,
            opens: "p".into(),
        }
    }
    #[test]
    fn lip_rectangles_match_the_qml_formulas() {
        let out = (1920., 1200.);
        assert_eq!(
            lip(Edge::Top, 208., 50.).rect(out),
            Rect::new(856., 0., 208., 50.)
        );
        assert_eq!(
            lip(Edge::Left, 144., 10.).rect(out),
            Rect::new(0., 528., 10., 144.)
        );
        assert_eq!(
            lip(Edge::Right, 144., 10.).rect(out),
            Rect::new(1910., 528., 10., 144.)
        );
        assert_eq!(
            lip(Edge::Bottom, 208., 10.).rect(out),
            Rect::new(856., 1190., 208., 10.)
        );
    }
    #[test]
    fn lip_containment_is_half_open_and_empty_lips_hit_nothing() {
        let out = (1920., 1200.);
        let left = lip(Edge::Left, 144., 10.);
        assert!(left.contains(out, 0., 528.));
        assert!(left.contains(out, 9.9, 671.9));
        assert!(!left.contains(out, 10., 600.));
        assert!(!left.contains(out, 5., 672.));
        assert!(!left.contains(out, 5., 527.9));
        assert!(!lip(Edge::Left, 144., 0.).contains(out, 0., 600.));
        // A frame wider than the output stays an ordinary rectangle (no panic, no wrap).
        assert!(lip(Edge::Top, 5000., 50.).contains(out, 1., 1.));
    }
    #[test]
    fn animated_dwell_fills_then_holds_two_frames_before_firing() {
        let mut dwell = Dwell::default();
        assert_eq!(dwell.start(350, true), Start::Frames);
        assert!(dwell.waiting());
        let mut frames = 0;
        let fired = loop {
            frames += 1;
            if dwell.advance(1. / 60.) {
                break frames;
            }
            assert!(frames < 100, "never fired");
        };
        // 350 ms at 16.7 ms a frame fills in 21 frames, then two held frames.
        assert_eq!(fired, 23);
        assert!(!dwell.waiting());
        assert_eq!(dwell.progress(), 1.);
    }
    #[test]
    fn a_stalled_frame_cannot_consume_the_fill() {
        let mut dwell = Dwell::default();
        dwell.start(350, true);
        assert!(!dwell.advance(5.));
        // Capped at 35 ms: a tenth of 350 ms.
        assert!((dwell.progress() - 0.1).abs() < 1e-6);
        assert!(!dwell.advance(-1.));
        assert!((dwell.progress() - 0.1).abs() < 1e-6);
    }
    #[test]
    fn cancel_resets_and_restart_begins_from_empty() {
        let mut dwell = Dwell::default();
        dwell.start(350, true);
        dwell.advance(0.03);
        dwell.advance(0.03);
        assert!(dwell.progress() > 0.);
        dwell.cancel();
        assert!(!dwell.waiting() && dwell.progress() == 0.);
        assert!(!dwell.advance(0.016), "a cancelled dwell never fires");
        assert!(!dwell.elapse());
        dwell.start(350, true);
        assert_eq!(dwell.progress(), 0.);
    }
    #[test]
    fn zero_delay_fires_at_once_and_reduced_motion_uses_a_timer() {
        let mut dwell = Dwell::default();
        assert_eq!(dwell.start(0, true), Start::Fire);
        assert!(!dwell.waiting());
        assert_eq!(dwell.start(200, false), Start::Timer(200));
        assert_eq!(dwell.progress(), 1.);
        assert!(!dwell.advance(1.), "frames do nothing without motion");
        assert!(dwell.elapse());
        assert!(!dwell.elapse(), "fires once");
    }
    #[test]
    fn switching_motion_off_mid_dwell_finishes_on_a_timer_for_the_rest() {
        let mut dwell = Dwell::default();
        assert_eq!(dwell.motion_off(), None);
        dwell.start(400, true);
        dwell.advance(0.035);
        dwell.advance(0.035);
        let left = dwell.motion_off().unwrap();
        // 70 ms of 400 ms filled.
        assert!((left as i64 - 330).abs() <= 1, "{left}");
        assert_eq!(dwell.progress(), 1.);
        assert!(dwell.elapse());
    }
    #[test]
    fn the_gate_blocks_until_the_pointer_leaves() {
        let mut gate = Gate::default();
        assert!(gate.can_open("DP-1/left"));
        gate.dismiss("DP-1/left");
        assert!(!gate.can_open("DP-1/left"));
        assert!(gate.can_open("DP-2/left"), "per output");
        gate.rearm("DP-1/left");
        assert!(gate.can_open("DP-1/left"));
        gate.dismiss("DP-1/left");
        gate.dismiss("DP-1/right");
        gate.forget_prefix("DP-1/");
        assert!(gate.can_open("DP-1/left") && gate.can_open("DP-1/right"));
    }
    #[test]
    fn hover_needs_every_condition() {
        assert!(may_dwell(false, false, true, true));
        assert!(!may_dwell(true, false, true, true), "a held button");
        assert!(!may_dwell(false, true, true, true), "clickEdgeMenus");
        assert!(!may_dwell(false, false, false, true), "modal or fullscreen");
        assert!(!may_dwell(false, false, true, false), "blocked");
    }
}

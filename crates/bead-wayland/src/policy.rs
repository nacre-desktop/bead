// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-surface CPU-first / warm GPU / idle CPU lifecycle.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreparedFrameKey {
    pub generation: u64,
    pub ui_serial: u64,
    pub size: (u32, u32),
    pub scale120: u32,
}

pub(crate) fn prepared_frame_ready(
    configured: bool,
    placed: bool,
    snapshot: Option<PreparedFrameKey>,
    current: PreparedFrameKey,
) -> bool {
    configured && placed && snapshot == Some(current)
}

/// Run native preparation only for an accepted drag press on the docked panel.
pub(crate) fn prepare_native_for_drag_press<T>(
    accepted_press: bool,
    docked_source: bool,
    prepare: impl FnOnce() -> T,
) -> Option<T> {
    (accepted_press && docked_source).then(prepare)
}
/// An initial native frame is hidden until the latest complete geometry was
/// placed. Replies for superseded rectangles cannot uncover an older pose.
#[derive(Default)]
pub(crate) struct Handoff {
    requested: Option<bead_core::pearl::Rect>,
    placed: bool,
}
impl Handoff {
    pub fn request(&mut self, rect: bead_core::pearl::Rect) -> bool {
        if self.requested == Some(rect) {
            return false;
        }
        self.requested = Some(rect);
        self.placed = false;
        true
    }
    pub fn accepts(&self, rect: bead_core::pearl::Rect) -> bool {
        self.requested == Some(rect)
    }
    pub fn placed(&mut self, rect: bead_core::pearl::Rect) {
        if self.accepts(rect) {
            self.placed = true;
        }
    }
    pub fn can_draw(&self) -> bool {
        self.placed
    }
    pub fn finish_frame(&mut self, committed: bool) -> bool {
        if !committed || !self.placed {
            return false;
        }
        self.placed = false;
        true
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::{PreparedFrameKey, prepared_frame_ready};

    #[test]
    fn panel_open_close_without_drag_creates_zero_toplevels() {
        use super::prepare_native_for_drag_press;
        use std::cell::Cell;

        let creations = Cell::new(0);
        let prepare = || creations.set(creations.get() + 1);

        for _ in 0..20 {
            assert_eq!(prepare_native_for_drag_press(false, true, prepare), None);
        }
        assert_eq!(
            creations.get(),
            0,
            "opening and closing without dragging cannot create a toplevel"
        );
        assert_eq!(
            prepare_native_for_drag_press(true, false, prepare),
            None,
            "a pearl press cannot reserve a second native target"
        );
        assert_eq!(creations.get(), 0);
        assert_eq!(prepare_native_for_drag_press(true, true, prepare), Some(()));
        assert_eq!(creations.get(), 1, "a valid docked drag may prewarm once");
    }

    #[test]
    fn prewarm_requires_current_placement_configuration_and_snapshot() {
        let current = PreparedFrameKey {
            generation: 3,
            ui_serial: 41,
            size: (1280, 720),
            scale120: 180,
        };
        assert!(!prepared_frame_ready(false, true, Some(current), current));
        assert!(!prepared_frame_ready(true, false, Some(current), current));
        assert!(!prepared_frame_ready(true, true, None, current));
        for stale in [
            PreparedFrameKey {
                generation: 2,
                ..current
            },
            PreparedFrameKey {
                ui_serial: 40,
                ..current
            },
            PreparedFrameKey {
                size: (640, 360),
                ..current
            },
            PreparedFrameKey {
                scale120: 120,
                ..current
            },
        ] {
            assert!(!prepared_frame_ready(true, true, Some(stale), current));
        }
        assert!(prepared_frame_ready(true, true, Some(current), current));
    }

    use super::*;
    use bead_core::pearl::Rect;
    #[test]
    fn content_waits_for_complete_final_geometry_and_stale_replies_do_not_uncover() {
        let mut gate = Handoff::default();
        let drop = Rect::new(410., 250., 1100., 590.);
        let previous = drop.translated(-100., -100.);
        assert!(
            !gate.can_draw(),
            "configure/transparent map cannot retire source"
        );
        assert!(
            !gate.finish_frame(true),
            "a transparent map keeps the cover"
        );
        assert!(gate.request(previous));
        assert!(gate.request(drop));
        gate.placed(previous);
        assert!(!gate.can_draw(), "stale persisted placement cannot uncover");
        assert!(
            !gate.request(drop),
            "configure repeats do not requeue placement"
        );
        assert!(!gate.can_draw(), "failed placement leaves source visible");
        gate.placed(drop);
        assert!(
            gate.can_draw(),
            "only the final content commit may retire source"
        );
        assert!(
            !gate.finish_frame(false),
            "busy buffers and failed draws keep the cover"
        );
        assert!(gate.can_draw());
        assert!(
            gate.finish_frame(true),
            "content commits before the source unmaps"
        );
        assert!(
            !gate.finish_frame(true),
            "late/duplicate frames cannot retire twice"
        );
    }
}
#[derive(Default)]
pub(crate) struct Policy {
    first: bool,
    warmed: bool,
    gpu: bool,
    last_active: f64,
}
impl Policy {
    pub fn needs_warmup(&self) -> bool {
        self.first && !self.warmed
    }
    pub fn frame(&mut self, now: f64, animated: bool) -> bool {
        if !self.first {
            self.first = true;
            return false;
        }
        if animated {
            self.touch(now);
        }
        if !self.gpu && (!self.warmed || animated) {
            self.warmed = true;
            self.gpu = true;
            self.touch(now);
            return true;
        }
        false
    }
    pub fn touch(&mut self, now: f64) {
        if self.gpu {
            self.last_active = now;
        }
    }
    pub fn remaining(&self, now: f64) -> f64 {
        (2. - (now - self.last_active)).max(0.001)
    }
    pub fn expire(&mut self, now: f64) -> bool {
        if self.gpu && now - self.last_active >= 2. {
            self.gpu = false;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_frame_idle_release_and_animated_reopen() {
        let mut p = Policy::default();
        assert!(!p.frame(0., false));
        assert!(p.needs_warmup());
        assert!(p.frame(0.01, false));
        assert!(!p.expire(1.));
        assert!(p.expire(2.02));
        assert!(!p.frame(3., false));
        assert!(p.frame(4., true));
        assert!(!p.frame(5., true));
        assert!(!p.expire(6.));
        assert!(p.expire(7.));
    }

    #[test]
    fn active_gpu_touch_defers_expiry_until_idle() {
        let mut p = Policy::default();
        assert!(!p.frame(0., false));
        assert!(p.frame(0.01, false));
        p.touch(1.99);
        assert!((p.remaining(1.99) - 2.).abs() < f64::EPSILON);
        assert!(!p.expire(3.98));
        assert!(p.expire(4.0));
    }
}

/// Each surface owns one live frame request. Parked callbacks become stale when
/// content reopens or its renderer changes; late replies must not grant a frame.
#[derive(Default)]
pub(crate) struct Callbacks {
    pub pending: bool,
    epoch: u64,
}
impl Callbacks {
    pub fn request(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = true;
        self.epoch
    }
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = false;
    }
    pub fn accept(&mut self, epoch: u64) -> bool {
        if self.pending && epoch == self.epoch {
            self.pending = false;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod callback_tests {
    use super::*;
    #[test]
    fn reopening_discards_parked_callback_and_never_grants_a_duplicate_frame() {
        let mut c = Callbacks::default();
        let parked = c.request();
        c.invalidate();
        let current = c.request();
        assert!(!c.accept(parked));
        assert!(c.pending);
        assert!(c.accept(current));
        assert!(!c.accept(current));
    }
}

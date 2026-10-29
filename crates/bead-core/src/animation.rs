// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stable-ID animation state lives outside views. Rendering consumes only the pose.
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};
static REDUCED: AtomicBool = AtomicBool::new(false);
/// Process-wide reduced-motion switch: every later target is reached at once, so
/// no curve runs and nothing keeps redrawing. Hosts set it from their own settings.
pub fn set_reduced_motion(reduced: bool) {
    REDUCED.store(reduced, Ordering::Relaxed);
}
pub fn reduced_motion() -> bool {
    REDUCED.load(Ordering::Relaxed)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Curve {
    EaseOut,
    EaseInOut,
    /// Accelerating exit curve that commits most travel to its final frames.
    EmphasizedAccelerate,
    Spring,
}
#[derive(Clone, Copy)]
struct Motion {
    from: f32,
    to: f32,
    started: f64,
    duration: f64,
    curve: Curve,
    /// Waits at `from` until the first frame showing it is committed; that frame starts the clock.
    held: bool,
    /// When it was requested, and its committed frames (count, last, longest gap) for `Slide`.
    requested: f64,
    frames: u32,
    last_frame: f64,
    max_gap: f64,
}
/// One finished motion as the user saw it: request to first frame, then frames until the final pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slide {
    pub opening: bool,
    pub first_frame_ms: f64,
    pub frames: u32,
    pub duration_ms: f64,
    pub max_gap_ms: f64,
}
impl std::fmt::Display for Slide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dir={} first_frame_ms={:.1} frames={} duration_ms={:.1} max_gap_ms={:.1}",
            if self.opening { "open" } else { "close" },
            self.first_frame_ms,
            self.frames,
            self.duration_ms,
            self.max_gap_ms
        )
    }
}
#[derive(Default)]
pub struct Animations {
    states: BTreeMap<String, Motion>,
}
impl Animations {
    pub fn target(&mut self, id: &str, target: f32, now: f64, duration: f64, curve: Curve) {
        self.target_as(reduced_motion(), false, id, target, now, duration, curve);
    }
    /// Drives a closed (0) .. open (1) motion such as a panel reveal. A motion never seen before starts
    /// closed, and every motion this starts holds its first pose until `committed` reports the first
    /// committed frame that shows it: preparation, configure and slow first frames never spend it, so
    /// the whole curve is seen from its start (B0170).
    pub fn open_to(&mut self, id: &str, open: bool, now: f64, duration: f64, curve: Curve) {
        if open && !self.states.contains_key(id) {
            self.states.insert(
                id.into(),
                Motion {
                    from: 0.,
                    to: 0.,
                    started: now,
                    duration: 0.001,
                    curve,
                    held: false,
                    requested: now,
                    frames: 0,
                    last_frame: now,
                    max_gap: 0.,
                },
            );
        }
        let target = if open { 1. } else { 0. };
        self.target_as(reduced_motion(), true, id, target, now, duration, curve);
    }
    /// A frame showing `id`, posed at clock `drawn`, was committed at `now`. It is the first frame
    /// after every held request in this set, so all of their clocks start here (a motion replaced
    /// before it was drawn, such as one dropdown switching to another, then finishes normally).
    /// The commit with `id`'s final pose returns its `Slide` (once).
    pub fn committed(&mut self, id: &str, drawn: f64, now: f64) -> Option<Slide> {
        let finished = self
            .states
            .get(id)
            .is_some_and(|motion| !motion.held && drawn >= motion.started + motion.duration);
        for motion in self.states.values_mut().filter(|motion| motion.held) {
            motion.held = false;
            motion.started = now;
        }
        let motion = self.states.get_mut(id)?;
        if motion.from == motion.to || motion.frames == u32::MAX {
            return None;
        }
        if motion.frames > 0 {
            motion.max_gap = motion.max_gap.max(now - motion.last_frame);
        }
        motion.frames += 1;
        motion.last_frame = now;
        if !finished {
            return None;
        }
        let slide = Slide {
            opening: motion.to > motion.from,
            first_frame_ms: (motion.started - motion.requested) * 1000.,
            frames: motion.frames,
            duration_ms: (now - motion.started) * 1000.,
            max_gap_ms: motion.max_gap * 1000.,
        };
        motion.frames = u32::MAX;
        Some(slide)
    }
    #[allow(clippy::too_many_arguments)]
    fn target_as(
        &mut self,
        reduced: bool,
        hold: bool,
        id: &str,
        target: f32,
        now: f64,
        duration: f64,
        curve: Curve,
    ) {
        let current = if reduced {
            target
        } else {
            self.value(id, now).map_or(target, |v| v.0)
        };
        if self.states.get(id).is_some_and(|m| m.to == target) {
            return;
        }
        let duration = duration.max(0.001);
        self.states.insert(
            id.into(),
            Motion {
                from: current,
                to: target,
                // Already finished when reduced, so the pose is final and idle.
                started: if reduced { now - duration } else { now },
                duration,
                curve,
                held: hold && !reduced && current != target,
                requested: now,
                frames: 0,
                last_frame: now,
                max_gap: 0.,
            },
        );
    }
    pub fn value(&self, id: &str, now: f64) -> Option<(f32, bool)> {
        let motion = self.states.get(id)?;
        let t = if motion.held {
            0.
        } else {
            ((now - motion.started) / motion.duration).clamp(0., 1.) as f32
        };
        let progress = match motion.curve {
            Curve::EaseOut => 1. - (1. - t).powi(3),
            Curve::EaseInOut => {
                if t < 0.5 {
                    4. * t.powi(3)
                } else {
                    1. - (-2. * t + 2.).powi(3) / 2.
                }
            }
            Curve::EmphasizedAccelerate => t.powi(4),
            Curve::Spring => {
                let x = 8. * t;
                (1. - (1. + x) * (-x).exp()) / (1. - 9. * (-8_f32).exp())
            }
        };
        Some((motion.from + (motion.to - motion.from) * progress, t < 1.))
    }
    pub fn active(&self, now: f64) -> bool {
        self.states
            .values()
            .any(|m| (m.held || now < m.started + m.duration) && m.from != m.to)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_held_motion_starts_at_its_first_committed_frame() {
        let mut a = Animations::default();
        a.open_to("panel", true, 1., 0.4, Curve::EaseOut);
        // 150 ms of preparation and configure: still the first pose, still wanting frames.
        assert_eq!(a.value("panel", 1.15), Some((0., true)));
        assert!(a.active(5.));
        // The first content frame (drawn at progress 0) was committed at 1.15: the whole curve plays from there.
        assert_eq!(a.committed("panel", 1.14, 1.15), None);
        assert_eq!(a.value("panel", 1.15), Some((0., true)));
        assert!(
            a.value("panel", 1.35)
                .is_some_and(|(p, active)| p > 0. && p < 1. && active)
        );
        // Retargeting to the same end keeps the clock.
        a.open_to("panel", true, 1.4, 0.4, Curve::EaseOut);
        assert_eq!(a.value("panel", 1.56), Some((1., false)));
        // Frames every 8 ms with one late frame; the final pose reports the slide once.
        let mut time = 1.15;
        let slide = loop {
            time += if (1.3..1.31).contains(&time) {
                0.025
            } else {
                0.008
            };
            if let Some(slide) = a.committed("panel", time - 0.001, time) {
                break slide;
            }
            assert!(time < 1.56);
        };
        assert!(time > 1.549);
        assert!(slide.opening && slide.frames > 40);
        assert!((slide.first_frame_ms - 150.).abs() < 1e-6);
        assert!((slide.duration_ms - 400.).abs() < 10.);
        assert!((slide.max_gap_ms - 25.).abs() < 1e-6);
        assert_eq!(a.committed("panel", 2., 2.), None);
    }
    #[test]
    fn a_reversal_holds_its_current_pose_until_shown_and_plain_targets_never_hold() {
        let mut a = Animations::default();
        a.open_to("panel", true, 0., 0.4, Curve::EaseOut);
        a.committed("panel", 0., 0.);
        let mid = a.value("panel", 0.2).unwrap().0;
        a.open_to("panel", false, 0.2, 0.4, Curve::EaseOut);
        assert_eq!(a.value("panel", 0.3).unwrap().0, mid);
        a.committed("panel", 0.3, 0.3);
        assert!(a.value("panel", 0.5).unwrap().0 < mid);
        assert_eq!(a.value("panel", 0.71), Some((0., false)));
        let slide = a.committed("panel", 0.71, 0.71).unwrap();
        assert!(!slide.opening && slide.frames == 2);
        assert!((slide.first_frame_ms - 100.).abs() < 1e-6);
        // An open that is closed before it was ever shown has nothing left to animate.
        a.open_to("quick", true, 1., 0.4, Curve::EaseOut);
        a.open_to("quick", false, 1.1, 0.4, Curve::EaseOut);
        assert_eq!(a.value("quick", 1.1), Some((0., true)));
        assert!(!a.active(1.1));
        assert_eq!(a.committed("quick", 1.1, 1.1), None);
        // A motion replaced before it was ever drawn starts with the next committed frame.
        a.open_to("network", true, 2., 0.2, Curve::EaseOut);
        a.committed("network", 2., 2.);
        a.open_to("network", false, 2.5, 0.2, Curve::EaseOut);
        a.open_to("sound", true, 2.5, 0.2, Curve::EaseOut);
        assert!(a.active(10.));
        a.committed("sound", 2.5, 2.52);
        assert!(!a.active(2.73));
        assert_eq!(a.value("network", 2.73), Some((0., false)));
        // `target` keeps the request clock.
        a.target("plain", 0., 0., 0.4, Curve::EaseOut);
        a.target("plain", 1., 0., 0.4, Curve::EaseOut);
        assert_eq!(a.value("plain", 0.4), Some((1., false)));
    }
    #[test]
    fn cubic_in_out_has_symmetric_endpoints_midpoint_and_quarters() {
        let mut a = Animations::default();
        a.target("preview", 0., 0., 1., Curve::EaseInOut);
        a.target("preview", 1., 0., 1., Curve::EaseInOut);
        for (time, expected) in [
            (0., 0.),
            (0.25, 0.0625),
            (0.5, 0.5),
            (0.75, 0.9375),
            (1., 1.),
        ] {
            assert_eq!(a.value("preview", time).unwrap(), (expected, time < 1.));
        }
    }
    #[test]
    fn reduced_motion_reaches_every_target_at_once_and_keeps_nothing_active() {
        let mut a = Animations::default();
        a.target_as(false, false, "panel", 0., 0., 1., Curve::EaseOut);
        a.target_as(false, false, "panel", 1., 0., 1., Curve::EaseOut);
        assert_eq!(a.value("panel", 0.5).map(|v| v.1), Some(true));
        a.target_as(true, false, "panel", 0., 0.5, 1., Curve::Spring);
        assert_eq!(a.value("panel", 0.5), Some((0., false)));
        assert!(!a.active(0.5));
        a.target_as(true, false, "other", 1., 0.5, 1., Curve::EaseOut);
        assert_eq!(a.value("other", 0.5), Some((1., false)));
        assert!(!a.active(0.5));
    }
    #[test]
    fn reversing_and_view_reload_do_not_reset_progress() {
        let mut a = Animations::default();
        a.target("panel", 0., 0., 0.2, Curve::EaseOut);
        a.target("panel", 1., 0., 0.2, Curve::EaseOut);
        let before = a.value("panel", 0.1).unwrap().0;
        a.target("panel", 1., 0.1, 0.2, Curve::EaseOut);
        assert_eq!(a.value("panel", 0.1).unwrap().0, before);
        a.target("panel", 0., 0.1, 0.2, Curve::Spring);
        assert_eq!(a.value("panel", 0.1).unwrap().0, before);
        assert_eq!(a.value("panel", 0.3).unwrap().0, 0.);
    }
}

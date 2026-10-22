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
    Spring,
}
#[derive(Clone, Copy)]
struct Motion {
    from: f32,
    to: f32,
    started: f64,
    duration: f64,
    curve: Curve,
}
#[derive(Default)]
pub struct Animations {
    states: BTreeMap<String, Motion>,
}
impl Animations {
    pub fn target(&mut self, id: &str, target: f32, now: f64, duration: f64, curve: Curve) {
        self.target_as(reduced_motion(), id, target, now, duration, curve);
    }
    fn target_as(
        &mut self,
        reduced: bool,
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
            },
        );
    }
    pub fn value(&self, id: &str, now: f64) -> Option<(f32, bool)> {
        let motion = self.states.get(id)?;
        let t = ((now - motion.started) / motion.duration).clamp(0., 1.) as f32;
        let progress = match motion.curve {
            Curve::EaseOut => 1. - (1. - t).powi(3),
            Curve::EaseInOut => {
                if t < 0.5 {
                    4. * t.powi(3)
                } else {
                    1. - (-2. * t + 2.).powi(3) / 2.
                }
            }
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
            .any(|m| now < m.started + m.duration && m.from != m.to)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        a.target_as(false, "panel", 0., 0., 1., Curve::EaseOut);
        a.target_as(false, "panel", 1., 0., 1., Curve::EaseOut);
        assert_eq!(a.value("panel", 0.5).map(|v| v.1), Some(true));
        a.target_as(true, "panel", 0., 0.5, 1., Curve::Spring);
        assert_eq!(a.value("panel", 0.5), Some((0., false)));
        assert!(!a.active(0.5));
        a.target_as(true, "other", 1., 0.5, 1., Curve::EaseOut);
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

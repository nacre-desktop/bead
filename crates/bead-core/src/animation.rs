// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stable-ID animation state lives outside views. Rendering consumes only the pose.
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug)]
pub enum Curve {
    EaseOut,
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
        let current = self.value(id, now).map_or(target, |v| v.0);
        if self.states.get(id).is_some_and(|m| m.to == target) {
            return;
        }
        self.states.insert(
            id.into(),
            Motion {
                from: current,
                to: target,
                started: now,
                duration: duration.max(0.001),
                curve,
            },
        );
    }
    pub fn value(&self, id: &str, now: f64) -> Option<(f32, bool)> {
        let motion = self.states.get(id)?;
        let t = ((now - motion.started) / motion.duration).clamp(0., 1.) as f32;
        let progress = match motion.curve {
            Curve::EaseOut => 1. - (1. - t).powi(3),
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

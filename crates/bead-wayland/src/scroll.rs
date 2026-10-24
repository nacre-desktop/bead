// SPDX-License-Identifier: MIT OR Apache-2.0
//! Convert Wayland axis units into logical content distances for every surface.
use smithay_client_toolkit::seat::pointer::AxisScroll;

pub(super) fn distance(axis: AxisScroll) -> f64 {
    if axis.value120 != 0 {
        f64::from(axis.value120) * 5.
    } else if axis.discrete != 0 {
        f64::from(axis.discrete) * 600.
    } else {
        axis.absolute * 3.
    }
}

fn movement(axis: AxisScroll, content: bool) -> (f64, bool) {
    if content {
        (distance(axis), axis.value120 != 0 || axis.discrete != 0)
    } else {
        // Volume, workspace and tray gestures must stay one event per physical input.
        let raw = if axis.absolute != 0. {
            axis.absolute
        } else if axis.value120 != 0 {
            f64::from(axis.value120) / 8.
        } else {
            f64::from(axis.discrete) * 15.
        };
        (raw, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_gestures_are_never_amplified_or_glided() {
        let axis = AxisScroll {
            absolute: 15.,
            value120: 120,
            ..Default::default()
        };
        assert_eq!(movement(axis, false), (15., false));
        assert_eq!(movement(axis, true), (600., true));
        assert_eq!(
            movement(
                AxisScroll {
                    absolute: 2.5,
                    ..Default::default()
                },
                false
            ),
            (2.5, false)
        );
    }
    #[test]
    fn wheel_units_take_precedence_over_pixel_fallback() {
        assert_eq!(
            distance(AxisScroll {
                absolute: 15.,
                value120: 120,
                ..Default::default()
            }),
            600.
        );
        assert_eq!(
            distance(AxisScroll {
                value120: -30,
                ..Default::default()
            }),
            -150.
        );
        assert_eq!(
            distance(AxisScroll {
                discrete: 1,
                ..Default::default()
            }),
            600.
        );
        assert_eq!(
            distance(AxisScroll {
                absolute: -2.5,
                ..Default::default()
            }),
            -7.5
        );
        assert_eq!(distance(AxisScroll::default()), 0.);
    }
}

use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Target {
    Window(u64),
    Overlay(u64),
    Panel(u64),
    Primary(Option<(u64, String)>),
}
#[derive(Default)]
pub(super) struct State {
    serial: u64,
    motions: std::collections::BTreeMap<(Target, bool), Motion>,
}
struct Motion {
    serial: u64,
    started: Instant,
    distance: f64,
    delivered: f64,
    point: (f32, f32),
}
impl App {
    pub(super) fn scroll_target(&self, panel: bool) -> Target {
        if panel {
            Target::Panel(self.panel_epoch)
        } else {
            let output = self
                .active_input_output
                .as_ref()
                .and_then(|id| self.output_sessions.get(id).map(|s| (s.epoch, id.clone())));
            Target::Primary(output)
        }
    }
    fn deliver_scroll(&mut self, target: &Target, horizontal: bool, point: (f32, f32), delta: f64) {
        let event = if horizontal {
            UiEvent::HorizontalScroll {
                x: point.0,
                y: point.1,
                delta,
            }
        } else {
            UiEvent::Scroll {
                x: point.0,
                y: point.1,
                delta,
            }
        };
        match target {
            Target::Window(epoch) => self.ui(UiEvent::Window(*epoch, Box::new(event))),
            Target::Overlay(epoch) => self.ui(UiEvent::Overlay(*epoch, Box::new(event))),
            Target::Panel(epoch) => self.ui(UiEvent::Panel(*epoch, Box::new(event))),
            Target::Primary(Some((epoch, output))) => {
                self.ui(UiEvent::Output(*epoch, output.clone(), Box::new(event)))
            }
            Target::Primary(None) => self.ui(event),
        }
    }
    pub(super) fn axis_scroll(
        &mut self,
        target: Target,
        horizontal: bool,
        point: (f32, f32),
        axis: AxisScroll,
    ) {
        let settings = match &target {
            Target::Window(_) => self
                .window
                .as_ref()
                .map(|w| (w.wheel_step(), w.scroll_regions())),
            Target::Overlay(_) => self
                .overlay
                .as_ref()
                .map(|o| (o.wheel_step(), o.scroll_regions())),
            Target::Panel(_) => self
                .panel_runtime
                .as_ref()
                .map(|s| (s.wheel_step(), s.scroll_regions())),
            Target::Primary(Some((_, id))) => self
                .output_sessions
                .get(id)
                .and_then(|s| s.source.as_ref())
                .map(|s| (s.wheel_step(), s.scroll_regions())),
            Target::Primary(None) => self
                .runtime
                .as_ref()
                .map(|s| (s.wheel_step(), s.scroll_regions())),
        };
        let content = settings.as_ref().is_some_and(|(_, regions)| {
            regions.iter().any(|r| {
                point.0 >= r.x && point.1 >= r.y && point.0 < r.x + r.w && point.1 < r.y + r.h
            })
        });
        let step = settings
            .map(|(step, _)| step)
            .filter(|step| step.is_finite() && *step > 0.)
            .unwrap_or(600.);
        let (delta, wheel) = movement(axis, content);
        let delta = delta * if wheel { step / 600. } else { 1. };
        if delta == 0. {
            return;
        }
        let key = (target.clone(), horizontal);
        let previous = self.scrolls.motions.remove(&key);
        if !wheel || bead_core::animation::reduced_motion() {
            self.deliver_scroll(&target, horizontal, point, delta);
            return;
        }
        let remaining = previous
            .filter(|p| p.point == point)
            .map_or(0., |p| p.distance - p.delivered);
        self.scrolls.serial = self.scrolls.serial.wrapping_add(1);
        let serial = self.scrolls.serial;
        self.scrolls.motions.insert(
            key.clone(),
            Motion {
                serial,
                started: Instant::now(),
                distance: remaining + delta,
                delivered: 0.,
                point,
            },
        );
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(16)),
            move |_, _, app| {
                let Some(motion) = app
                    .scrolls
                    .motions
                    .get_mut(&key)
                    .filter(|m| m.serial == serial)
                else {
                    return TimeoutAction::Drop;
                };
                let progress = (motion.started.elapsed().as_secs_f64() / 0.210).min(1.);
                let next = motion.distance * (1. - (1. - progress).powi(3));
                let delta = next - motion.delivered;
                motion.delivered = next;
                let point = motion.point;
                app.deliver_scroll(&key.0, key.1, point, delta);
                if progress >= 1. {
                    app.scrolls.motions.remove(&key);
                    TimeoutAction::Drop
                } else {
                    TimeoutAction::ToDuration(Duration::from_millis(16))
                }
            },
        ) {
            self.fail(error);
        }
    }
}

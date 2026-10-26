// SPDX-License-Identifier: MIT OR Apache-2.0
//! Convert Wayland axis units into logical content distances for every surface.
use smithay_client_toolkit::seat::pointer::AxisScroll;
use std::sync::atomic::{AtomicU64, Ordering};
use wayland_client::protocol::wl_pointer::AxisSource;

/// Output refresh period reported by the latest presentation feedback (0 = unknown).
static REFRESH_NS: AtomicU64 = AtomicU64::new(0);
pub(super) fn note_refresh(refresh_ns: u32) {
    if refresh_ns > 0 {
        REFRESH_NS.store(u64::from(refresh_ns), Ordering::Relaxed);
    }
}
/// Glide, fling and native frames follow presentation feedback. With unknown refresh,
/// retain a 120 Hz opportunity; an arbitrary 60 Hz cap would skip alternate frames
/// on fast outputs (including compositors that report no refresh prediction).
pub(super) fn tick() -> Duration {
    let ns = REFRESH_NS.load(Ordering::Relaxed);
    Duration::from_nanos(if (4_000_000..=34_000_000).contains(&ns) {
        ns
    } else {
        8_333_333
    })
}
/// Chromium's Linux-default physics uses 96 logical pixels per inch and
/// physical deceleration 2.7559e-5 inch/ms².
const FLING_DECELERATION: f64 = 96. * 2.7559e-5 * 1_000_000.;
const FLING_DURATION_DECELERATION: f64 = 2300.;
const FLING_MIN_VELOCITY: f64 = 100.;
const WHEEL_DURATION: f64 = 0.125;
const WHEEL_STEP: f64 = 100.;
const LEGACY_WHEEL_NOTCH: f64 = 15.;

pub(super) fn distance(axis: AxisScroll) -> f64 {
    if axis.value120 != 0 {
        f64::from(axis.value120) * WHEEL_STEP / 120.
    } else if axis.discrete != 0 {
        f64::from(axis.discrete) * WHEEL_STEP
    } else {
        axis.absolute
    }
}

fn movement(axis: AxisScroll, content: bool, source: Option<AxisSource>) -> (f64, bool) {
    if content {
        if source == Some(AxisSource::Finger) {
            return (axis.absolute, false);
        }
        let wheel = matches!(source, Some(AxisSource::Wheel | AxisSource::WheelTilt))
            || axis.value120 != 0
            || axis.discrete != 0;
        let delta = if wheel && axis.value120 == 0 && axis.discrete == 0 {
            axis.absolute * WHEEL_STEP / LEGACY_WHEEL_NOTCH
        } else {
            distance(axis)
        };
        (delta, wheel)
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
        assert_eq!(movement(axis, false, None), (15., false));
        assert_eq!(movement(axis, true, None), (100., true));
        assert_eq!(
            movement(
                AxisScroll {
                    absolute: 2.5,
                    ..Default::default()
                },
                false,
                None
            ),
            (2.5, false)
        );
    }
    #[test]
    fn finger_pixels_are_one_to_one_and_legacy_wheel_uses_notch_units() {
        let axis = AxisScroll {
            absolute: -15.,
            ..Default::default()
        };
        assert_eq!(
            movement(axis, true, Some(AxisSource::Finger)),
            (-15., false)
        );
        assert_eq!(movement(axis, true, Some(AxisSource::Wheel)), (-100., true));
        assert_eq!(
            movement(
                AxisScroll {
                    absolute: 15.,
                    ..Default::default()
                },
                true,
                Some(AxisSource::Wheel)
            ),
            (100., true)
        );
        assert_eq!(
            movement(axis, true, Some(AxisSource::Continuous)),
            (-15., false)
        );
    }

    #[test]
    fn wheel_reversal_discards_pending_forward_travel() {
        let previous = Motion {
            serial: 0,
            started: Instant::now(),
            distance: 1200.,
            delivered: 100.,
            point: (4., 5.),
        };
        assert_eq!(remaining_distance(Some(&previous), (4., 5.), -1.), 0.);
        assert_eq!(remaining_distance(Some(&previous), (4., 5.), 1.), 1100.);
        assert_eq!(remaining_distance(Some(&previous), (8., 5.), 1.), 0.);
    }

    #[test]
    fn fling_decelerates_to_rest_in_the_direction_of_release() {
        let (first, done) = fling_position(-1500., 0.1, 1080.);
        assert!(first < 0. && !done);
        let (end, done) = fling_position(-1500., 5., 1080.);
        assert!(done);
        assert_eq!(fling_position(1500., 0.1, 1080.).0, -first);
        assert!((end + 1500_f64.powi(2) / (2. * FLING_DECELERATION)).abs() < 1e-6);
        // Chromium's default PhysicsBasedFlingCurve reference sequence. The
        // 1080-DIP vertical viewport caps the endpoint at three viewports.
        let first = fling_position(5000., 0.02, 1080.).0;
        let second = fling_position(5000., 0.25, 1080.).0;
        let end = fling_position(5000., 10., 1080.);
        assert!((first - 98.).abs() < 1.);
        assert!((second - first - 923.).abs() < 1.);
        assert!((end.0 - second - 2218.).abs() < 1.);
        assert_eq!(end, (3240., true));
        // Its wider horizontal viewport reaches the unbounded physics endpoint.
        let first = fling_position(5000., 0.02, 1920.).0;
        let second = fling_position(5000., 0.25, 1920.).0;
        let end = fling_position(5000., 10., 1920.).0;
        assert!((first - 99.).abs() < 1.);
        assert!((second - first - 1054.).abs() < 1.);
        assert!((end - second - 3571.).abs() < 1.);
    }

    #[test]
    fn physics_fling_matches_release_speed_and_finishes_within_two_seconds() {
        assert_eq!(fling_position(0., 0., 1080.), (0., true));
        assert_eq!(fling_position(5000., -0.02, 1080.), (0., false));
        assert_eq!(fling_position(5000., 0., 1080.), (0., false));
        assert!((fling_position(5000., 0.00001, 1080.).0 / 0.00001 - 5000.).abs() < 1.);
        assert_eq!(fling_position(100_000., 2., 1080.), (3240., true));
        assert_eq!(fling_position(-100_000., 2., 1080.), (-3240., true));
    }

    #[test]
    fn wheel_units_take_precedence_over_pixel_fallback() {
        assert_eq!(
            distance(AxisScroll {
                absolute: 15.,
                value120: 120,
                ..Default::default()
            }),
            100.
        );
        assert_eq!(
            distance(AxisScroll {
                value120: 30,
                ..Default::default()
            }),
            25.
        );
        assert_eq!(
            distance(AxisScroll {
                value120: -30,
                ..Default::default()
            }),
            -25.
        );
        assert_eq!(
            distance(AxisScroll {
                discrete: 1,
                ..Default::default()
            }),
            100.
        );
        assert_eq!(
            distance(AxisScroll {
                discrete: -1,
                ..Default::default()
            }),
            -100.
        );
        assert_eq!(
            distance(AxisScroll {
                absolute: -2.5,
                ..Default::default()
            }),
            -2.5
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
    pixels: std::collections::BTreeMap<(Target, bool), Pixels>,
    flings: std::collections::BTreeMap<(Target, bool), Fling>,
}
impl State {
    pub(super) fn is_animating(&self) -> bool {
        !self.motions.is_empty() || !self.flings.is_empty()
    }
}
struct Fling {
    serial: u64,
    started: Instant,
    velocity: f64,
    viewport: f64,
    delivered: f64,
    point: (f32, f32),
}
/// Pixel-input history of one scroll axis; a release with speed turns into a decelerating fling.
struct Pixels {
    last: Instant,
    velocity: f64,
    samples: u32,
    point: (f32, f32),
}
struct Motion {
    serial: u64,
    started: Instant,
    distance: f64,
    delivered: f64,
    point: (f32, f32),
}
/// Retarget from the current pose; reversing discards all undelivered travel.
fn remaining_distance(previous: Option<&Motion>, point: (f32, f32), delta: f64) -> f64 {
    previous
        .filter(|p| p.point == point && p.distance.signum() == delta.signum())
        .map_or(0., |p| p.distance - p.delivered)
}
impl App {
    fn fling_viewport(&self, target: &Target, horizontal: bool) -> f64 {
        let size = match target {
            Target::Window(epoch) => self
                .window
                .as_ref()
                .filter(|w| w.epoch() == *epoch)
                .map(|w| w.viewport_size()),
            Target::Overlay(epoch) => self
                .overlay
                .as_ref()
                .filter(|o| o.epoch() == *epoch)
                .map(|o| o.viewport_size()),
            Target::Panel(epoch) => self
                .surfaces
                .iter()
                .find(|s| s.companion && *epoch == self.panel_epoch)
                .map(|s| (s.width, s.height)),
            Target::Primary(output) => self
                .surfaces
                .iter()
                .find(|s| {
                    !s.companion
                        && output
                            .as_ref()
                            .is_none_or(|(_, name)| &s.output_name == name)
                })
                .map(|s| (s.width, s.height)),
        };
        size.map_or(0., |(width, height)| {
            f64::from(if horizontal { width } else { height })
        })
    }
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
        bead_core::vprintln!(
            "AXIS_DELIVER monotonic_ns={} horizontal={horizontal} x={} y={} delta={delta}",
            bead_core::profile::monotonic_ns(),
            point.0,
            point.1
        );
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
        source: Option<AxisSource>,
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
            .unwrap_or(WHEEL_STEP);
        let (delta, wheel) = movement(axis, content, source);
        let delta = delta * if wheel { step / WHEEL_STEP } else { 1. };
        let key = (target.clone(), horizontal);
        if delta == 0. {
            if axis.stop {
                self.release_pixels(&key);
            }
            return;
        }
        let previous = self.scrolls.motions.remove(&key);
        self.scrolls.flings.remove(&key);
        if !wheel || bead_core::animation::reduced_motion() {
            self.deliver_scroll(&target, horizontal, point, delta);
            if content
                && source == Some(AxisSource::Finger)
                && !bead_core::animation::reduced_motion()
            {
                self.track_pixels(key.clone(), point, delta);
                if axis.stop {
                    self.release_pixels(&key);
                }
            } else {
                self.scrolls.pixels.remove(&key);
            }
            return;
        }
        self.scrolls.pixels.remove(&key);
        let remaining = remaining_distance(previous.as_ref(), point, delta);
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
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(tick()), move |_, _, app| {
                    let Some(motion) = app
                        .scrolls
                        .motions
                        .get_mut(&key)
                        .filter(|m| m.serial == serial)
                    else {
                        return TimeoutAction::Drop;
                    };
                    let progress =
                        (motion.started.elapsed().as_secs_f64() / WHEEL_DURATION).min(1.);
                    let next = motion.distance * (1. - (1. - progress).powi(3));
                    let delta = next - motion.delivered;
                    motion.delivered = next;
                    let point = motion.point;
                    app.deliver_scroll(&key.0, key.1, point, delta);
                    if progress >= 1. {
                        app.scrolls.motions.remove(&key);
                        TimeoutAction::Drop
                    } else {
                        TimeoutAction::ToDuration(tick())
                    }
                })
        {
            self.fail(error);
        }
    }
    /// Estimate release speed only; finger positions are delivered without interpolation.
    fn track_pixels(&mut self, key: (Target, bool), point: (f32, f32), delta: f64) {
        let now = Instant::now();
        // A disappearing focus target may never receive axis_stop.
        self.scrolls
            .pixels
            .retain(|_, pixels| now.duration_since(pixels.last) <= Duration::from_millis(120));
        let pixels = self.scrolls.pixels.remove(&key).filter(|p| {
            p.point == point
                && now.duration_since(p.last) <= Duration::from_millis(120)
                && p.velocity * delta >= 0.
        });
        let elapsed_ms = pixels
            .as_ref()
            .map_or(16., |p| now.duration_since(p.last).as_secs_f64() * 1000.)
            .clamp(8., 50.);
        let sample = delta * 1000. / elapsed_ms;
        let (velocity, samples) = match &pixels {
            Some(p) if p.samples > 0 => (p.velocity * 0.55 + sample * 0.45, p.samples + 1),
            _ => (sample, 1),
        };
        self.scrolls.pixels.insert(
            key.clone(),
            Pixels {
                last: now,
                velocity,
                samples,
                point,
            },
        );
    }
    fn release_pixels(&mut self, key: &(Target, bool)) {
        let Some(pixels) = self.scrolls.pixels.remove(key) else {
            return;
        };
        if pixels.last.elapsed() > Duration::from_millis(120)
            || pixels.samples < 2
            || pixels.velocity.abs() <= FLING_MIN_VELOCITY
        {
            return;
        }
        let velocity = pixels.velocity;
        let viewport = self.fling_viewport(&key.0, key.1);
        if !velocity.is_finite() || viewport <= 0. {
            return;
        }
        self.scrolls.serial = self.scrolls.serial.wrapping_add(1);
        let serial = self.scrolls.serial;
        let key = key.clone();
        self.scrolls.flings.insert(
            key.clone(),
            Fling {
                serial,
                started: Instant::now(),
                velocity,
                viewport,
                delivered: 0.,
                point: pixels.point,
            },
        );
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(tick()), move |_, _, app| {
                    let Some(fling) = app
                        .scrolls
                        .flings
                        .get_mut(&key)
                        .filter(|f| f.serial == serial)
                    else {
                        return TimeoutAction::Drop;
                    };
                    let (next, done) = fling_position(
                        fling.velocity,
                        fling.started.elapsed().as_secs_f64(),
                        fling.viewport,
                    );
                    let delta = next - fling.delivered;
                    fling.delivered = next;
                    let point = fling.point;
                    app.deliver_scroll(&key.0, key.1, point, delta);
                    if done {
                        app.scrolls.flings.remove(&key);
                        TimeoutAction::Drop
                    } else {
                        TimeoutAction::ToDuration(tick())
                    }
                })
        {
            self.fail(error);
        }
    }
}

/// Unboosted Chromium Linux physics, evaluated independently per scroll axis.
fn fling_position(velocity: f64, t: f64, viewport: f64) -> (f64, bool) {
    let speed = velocity.abs();
    if !speed.is_finite() || speed == 0. || !viewport.is_finite() || viewport <= 0. {
        return (0., true);
    }
    if t <= 0. {
        return (0., false);
    }
    let distance = (speed * speed / (2. * FLING_DECELERATION)).min(3. * viewport);
    let duration = (speed / FLING_DURATION_DECELERATION).min(2.);
    if t >= duration {
        return (distance.copysign(velocity), true);
    }
    let slope = speed * duration / distance;
    let (p1x, p1y) = if slope * 0.2 < 1. {
        (0.2, 0.2 * slope)
    } else {
        (1. / slope, 1.)
    };
    let cubic = |u: f64, first: f64, second: f64| {
        3. * (1. - u).powi(2) * u * first + 3. * (1. - u) * u * u * second + u * u * u
    };
    let x = (t / duration).clamp(0., 1.);
    let (mut low, mut high) = (0., 1.);
    // Bounded monotonic inversion remains stable at the steep initial tangent
    // of fast, viewport-limited flings.
    for _ in 0..40 {
        let mid = (low + high) * 0.5;
        if cubic(mid, p1x, 0.55) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    (
        distance.copysign(velocity) * cubic((low + high) * 0.5, p1y, 1.),
        false,
    )
}

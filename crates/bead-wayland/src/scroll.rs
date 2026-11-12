// SPDX-License-Identifier: MIT
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
/// Shared launcher and Widgets wheel token, retained at 100 logical pixels per notch.
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

    #[test]
    fn finger_pixels_take_precedence_over_stale_wheel_units() {
        let axis = AxisScroll {
            absolute: -23.5,
            value120: 120,
            discrete: 1,
            ..Default::default()
        };
        assert_eq!(
            movement(axis, true, Some(AxisSource::Finger)),
            (-23.5, false)
        );
    }

    #[test]
    fn compositor_callbacks_drive_wheel_at_sixty_and_one_twenty_hz() {
        for (hz, expected) in [(60_u64, 8), (120, 15)] {
            let start = Instant::now();
            let target = Target::Window(7);
            let mut state = State::default();
            state.motions.insert(
                (target.clone(), false),
                Motion {
                    started: start,
                    distance: WHEEL_STEP,
                    delivered: 0.,
                    point: (4., 5.),
                },
            );
            let mut frames = 0;
            let mut travel = 0.;
            while state.animating(&target) {
                frames += 1;
                let now = start + Duration::from_nanos((frames * 1_000_000_000_u64).div_ceil(hz));
                let (delta, point) = state.step(&target, false, now).unwrap();
                assert_eq!(point, (4., 5.));
                assert!(delta > 0.);
                travel += delta;
            }
            assert_eq!(frames, expected);
            assert!((travel - WHEEL_STEP).abs() < 1e-9);
            assert!(
                state
                    .step(&target, false, start + Duration::from_secs(1))
                    .is_none()
            );
        }
    }

    #[test]
    fn callback_motion_is_isolated_by_axis_and_surface_epoch() {
        let start = Instant::now();
        let old = Target::Panel(7);
        let new = Target::Panel(8);
        let mut state = State::default();
        state.flings.insert(
            (old.clone(), true),
            Fling {
                started: start,
                velocity: 1500.,
                viewport: 1080.,
                delivered: 0.,
                point: (3., 4.),
            },
        );
        let now = start + Duration::from_millis(10);
        assert!(state.step(&new, true, now).is_none());
        assert!(state.step(&old, false, now).is_none());
        let (first, _) = state.step(&old, true, now).unwrap();
        assert!(first > 0.);
        state
            .step(&old, true, start + Duration::from_secs(3))
            .unwrap();
        assert!(!state.animating(&old));
    }

    #[test]
    fn scroll_and_render_share_one_callback_credit_until_motion_finishes() {
        let mut callbacks = policy::Callbacks::default();
        let scroll_credit = callbacks.request_if_idle().unwrap();
        assert!(callbacks.request_if_idle().is_none()); // rendering already has a wakeup
        assert!(callbacks.accept(scroll_credit));
        let render_credit = callbacks.request_if_idle().unwrap();
        assert!(callbacks.request_if_idle().is_none()); // scroll reuses the rendered frame
        callbacks.invalidate(); // surface retired or replaced
        let reopened = callbacks.request_if_idle().unwrap();
        assert!(!callbacks.accept(render_credit));
        assert!(callbacks.pending);
        assert!(callbacks.accept(reopened));
        assert!(!callbacks.pending); // no request after the last motion frame
    }

    #[test]
    fn canvas_motion_is_epoch_isolated_and_cancelled_on_retirement() {
        let started = Instant::now();
        let canvas = Target::Canvas(8);
        let mut state = State::default();
        state.motions.insert(
            (canvas.clone(), false),
            Motion {
                started,
                distance: 100.,
                delivered: 0.,
                point: (40.5, 70.25),
            },
        );
        assert!(
            state
                .step(
                    &Target::Canvas(9),
                    false,
                    started + Duration::from_millis(10)
                )
                .is_none()
        );
        let (_, point) = state
            .step(&canvas, false, started + Duration::from_millis(10))
            .unwrap();
        assert_eq!(point, (40.5, 70.25));
        state.interrupt(&canvas);
        assert!(!state.animating(&canvas));
        assert!(
            state
                .step(&canvas, false, started + Duration::from_secs(1))
                .is_none()
        );
    }

    #[test]
    fn idle_deadline_waits_for_one_shot_wakeup_before_requesting_frames() {
        let now = Instant::now();
        let hide = now + Duration::from_secs(1);
        let interval = Duration::from_millis(34);
        assert!(!callback_due(hide, now, interval));
        assert!(!callback_due(
            hide,
            now + Duration::from_millis(900),
            interval
        ));
        assert!(callback_due(hide, hide, interval));
        assert!(callback_due(
            now + Duration::from_nanos(8_333_333),
            now,
            interval
        ));
        assert!(callback_due(
            now + Duration::from_nanos(16_666_667),
            now,
            interval
        ));
    }

    #[test]
    fn slightly_early_callback_uses_this_refresh_instead_of_skipping_it() {
        let start = Instant::now();
        for hz in [60, 120] {
            let interval = Duration::from_nanos(1_000_000_000 / hz);
            let deadline = start + interval;
            let callback = deadline - Duration::from_micros(100);
            assert!(callback < deadline);
            assert!(callback_due(deadline, callback, interval / 2));
            assert!(!callback_due(
                start + Duration::from_secs(1),
                callback,
                interval / 2
            ));
        }
    }

    #[test]
    fn retarget_uses_callback_pose_and_preserves_total_wheel_travel() {
        let start = Instant::now();
        let target = Target::Overlay(2);
        let mut state = State::default();
        state.motions.insert(
            (target.clone(), false),
            Motion {
                started: start,
                distance: 100.,
                delivered: 0.,
                point: (1., 2.),
            },
        );
        let now = start + Duration::from_millis(50);
        let (first, _) = state.step(&target, false, now).unwrap();
        assert!((first - 78.4).abs() < 1e-9);
        let motion = state.motions.values_mut().next().unwrap();
        let pending = remaining_distance(Some(motion), (1., 2.), 100.);
        *motion = Motion {
            started: now,
            distance: pending + 100.,
            delivered: 0.,
            point: (1., 2.),
        };
        let (rest, _) = state
            .step(&target, false, now + Duration::from_millis(125))
            .unwrap();
        assert!((first + rest - 200.).abs() < 1e-9);
        assert!(!state.is_animating());
    }
}

use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Target {
    Canvas(u64),
    Window(u64),
    Overlay(u64),
    Panel(u64),
    Primary(Option<(u64, String)>),
}
#[derive(Default)]
pub(super) struct State {
    pub(super) in_frame: bool,
    pub(super) ui_depth: u32,
    wakeups: std::collections::BTreeMap<Target, (Instant, calloop::RegistrationToken)>,
    motions: std::collections::BTreeMap<(Target, bool), Motion>,
    pixels: std::collections::BTreeMap<(Target, bool), Pixels>,
    flings: std::collections::BTreeMap<(Target, bool), Fling>,
}
impl State {
    pub(super) fn interrupt(&mut self, target: &Target) {
        self.motions.retain(|(t, _), _| t != target);
        self.flings.retain(|(t, _), _| t != target);
        self.pixels.retain(|(t, _), _| t != target);
    }
    fn animating(&self, target: &Target) -> bool {
        self.motions.keys().any(|(t, _)| t == target)
            || self.flings.keys().any(|(t, _)| t == target)
    }
    /// Mutate existing entries in place; no timer registration or per-frame key clones.
    fn step(
        &mut self,
        target: &Target,
        horizontal: bool,
        now: Instant,
    ) -> Option<(f64, (f32, f32))> {
        let mut result = None;
        self.motions.retain(|(t, h), motion| {
            if t != target || *h != horizontal {
                return true;
            }
            let progress =
                (now.duration_since(motion.started).as_secs_f64() / WHEEL_DURATION).min(1.);
            let next = motion.distance * (1. - (1. - progress).powi(3));
            result = Some((next - motion.delivered, motion.point));
            motion.delivered = next;
            progress < 1.
        });
        self.flings.retain(|(t, h), fling| {
            if t != target || *h != horizontal {
                return true;
            }
            let (next, done) = fling_position(
                fling.velocity,
                now.duration_since(fling.started).as_secs_f64(),
                fling.viewport,
            );
            result = Some((next - fling.delivered, fling.point));
            fling.delivered = next;
            !done
        });
        result
    }
    pub(super) fn is_animating(&self) -> bool {
        !self.motions.is_empty() || !self.flings.is_empty()
    }
}
struct Fling {
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
fn callback_due(deadline: Instant, now: Instant, interval: Duration) -> bool {
    deadline.saturating_duration_since(now) <= interval
}
impl App {
    pub(super) fn cancel_scroll(&mut self, target: &Target) {
        if let Some((_, token)) = self.scrolls.wakeups.remove(target) {
            self.loop_handle.remove(token);
        }
        self.scrolls.interrupt(target);
    }
    /// Resolve only live surface identities: closed/replaced slots never drive old motion.
    pub(super) fn scroll_surface_target(&self, surface: &wl_surface::WlSurface) -> Option<Target> {
        if let Some(window) = self
            .window
            .as_ref()
            .filter(|w| w.scroll_surface() == surface)
        {
            return Some(Target::Window(window.epoch()));
        }
        if let Some(overlay) = self
            .overlay
            .as_ref()
            .filter(|o| o.scroll_surface() == surface)
        {
            return Some(Target::Overlay(overlay.epoch()));
        }
        if let Some(canvas) = self.canvases.values().find(|c| {
            c.scroll_active() && c.layer.as_ref().is_some_and(|l| l.wl_surface() == surface)
        }) {
            return Some(Target::Canvas(canvas.epoch));
        }
        let layer = self.surfaces.iter().find(|s| s.wl_surface() == surface)?;
        Some(if layer.companion {
            Target::Panel(self.panel_epoch)
        } else if self.options.primary_factory.is_some() {
            Target::Primary(
                self.output_sessions
                    .get(&layer.output_name)
                    .map(|session| (session.epoch, layer.output_name.clone())),
            )
        } else {
            Target::Primary(None)
        })
    }
    /// Bootstrap from input, then share the rendering callback credit on every frame.
    pub(super) fn request_scroll_frame(&mut self, target: &Target) {
        if !self.scrolls.animating(target) {
            return;
        }
        self.request_source_callback(target);
    }
    fn request_source_callback(&mut self, target: &Target) {
        let generation = match target {
            Target::Canvas(epoch) => self
                .canvases
                .values()
                .find(|c| c.epoch == *epoch)
                .map_or(0, |c| c.scene.resource_revision),
            Target::Panel(_) => self
                .companion
                .as_ref()
                .map_or(0, |(scene, _, _)| scene.resource_revision),
            Target::Primary(Some((_, id))) => self
                .output_sessions
                .get(id)
                .map_or(0, |s| s.scene.resource_revision),
            _ => self.scene.resource_revision,
        };
        let request = match target {
            Target::Canvas(epoch) => self
                .canvases
                .values_mut()
                .find(|c| c.epoch == *epoch && c.scroll_active())
                .and_then(|c| {
                    let surface = c.layer.as_ref()?.wl_surface().clone();
                    c.callbacks.request_if_idle().map(|credit| {
                        c.frame_pending = true;
                        (surface, credit, *epoch)
                    })
                }),
            Target::Window(epoch) => self
                .window
                .as_mut()
                .filter(|w| w.epoch() == *epoch)
                .and_then(|w| {
                    let surface = w.scroll_surface().clone();
                    w.callbacks
                        .request_if_idle()
                        .map(|credit| (surface, credit, *epoch))
                }),
            Target::Overlay(epoch) => self
                .overlay
                .as_mut()
                .filter(|o| o.epoch() == *epoch)
                .and_then(|o| {
                    let surface = o.scroll_surface().clone();
                    o.callbacks
                        .request_if_idle()
                        .map(|credit| (surface, credit, *epoch))
                }),
            Target::Panel(epoch) => self
                .surfaces
                .iter_mut()
                .find(|s| s.companion && *epoch == self.panel_epoch)
                .and_then(|s| {
                    s.callbacks
                        .request_if_idle()
                        .map(|credit| (s.wl_surface().clone(), credit, *epoch))
                }),
            Target::Primary(output) => {
                let valid = output.as_ref().is_none_or(|(epoch, name)| {
                    self.output_sessions
                        .get(name)
                        .is_some_and(|s| s.epoch == *epoch)
                });
                self.surfaces
                    .iter_mut()
                    .find(|s| {
                        valid
                            && !s.companion
                            && output
                                .as_ref()
                                .is_none_or(|(_, name)| &s.output_name == name)
                    })
                    .and_then(|s| {
                        s.callbacks
                            .request_if_idle()
                            .map(|credit| (s.wl_surface().clone(), credit, 0))
                    })
            }
        };
        if let Some((surface, epoch, source_epoch)) = request {
            surface.frame(
                &self.qh,
                FrameData {
                    surface: surface.clone(),
                    epoch,
                    generation,
                    source_epoch,
                },
            );
            surface.commit();
        }
    }
    pub(super) fn scroll_frame(&mut self, target: &Target, now: Instant) {
        self.scrolls.in_frame = true;
        for horizontal in [false, true] {
            if let Some((delta, point)) = self.scrolls.step(target, horizontal, now) {
                self.deliver_scroll(target, horizontal, point, delta);
            }
        }
        // Scrolling already advances view-local animation and decoration state.
        // Only request a second source update if its deadline is still pending.
        if self
            .source_deadline(target)
            .is_some_and(|deadline| callback_due(deadline, now, tick() / 2))
        {
            self.deliver_source_event(target, UiEvent::Model(Default::default()));
        }
        self.scrolls.in_frame = false;
    }
    fn source_deadline(&self, target: &Target) -> Option<Instant> {
        match target {
            Target::Canvas(epoch) => self
                .canvases
                .values()
                .find(|c| c.epoch == *epoch && c.scroll_active())
                .and_then(|c| c.source.wakeup_at()),
            Target::Window(epoch) => self
                .window
                .as_ref()
                .filter(|w| w.epoch() == *epoch)
                .and_then(|w| w.wakeup_at()),
            Target::Overlay(epoch) => self
                .overlay
                .as_ref()
                .filter(|o| o.epoch() == *epoch)
                .and_then(|o| o.wakeup_at()),
            Target::Panel(epoch) if *epoch == self.panel_epoch => {
                self.panel_runtime.as_ref().and_then(|s| s.wakeup_at())
            }
            Target::Primary(Some((epoch, id))) => self
                .output_sessions
                .get(id)
                .filter(|s| s.epoch == *epoch)
                .and_then(|s| s.source.as_ref())
                .and_then(|s| s.wakeup_at()),
            Target::Primary(None) => self.runtime.as_ref().and_then(|s| s.wakeup_at()),
            _ => None,
        }
    }
    /// Distant deadlines sleep once; animation deadlines consume compositor frame credit.
    pub(super) fn schedule_source_frames(&mut self) {
        if let Some(w) = &self.window {
            self.schedule_source_frame(Target::Window(w.epoch()));
        }
        if let Some(o) = &self.overlay {
            self.schedule_source_frame(Target::Overlay(o.epoch()));
        }
        for i in 0..self.canvases.len() {
            if let Some(epoch) = self
                .canvases
                .values()
                .nth(i)
                .filter(|c| c.scroll_active())
                .map(|c| c.epoch)
            {
                self.schedule_source_frame(Target::Canvas(epoch));
            }
        }
        for i in 0..self.surfaces.len() {
            let surface = self.surfaces[i].wl_surface().clone();
            if let Some(target) = self.scroll_surface_target(&surface) {
                self.schedule_source_frame(target);
            }
        }
    }
    fn schedule_source_frame(&mut self, target: Target) {
        let deadline = self.source_deadline(&target);
        if self
            .scrolls
            .wakeups
            .get(&target)
            .is_some_and(|(old, _)| deadline.is_some_and(|next| *old <= next))
        {
            return;
        }
        if let Some((_, token)) = self.scrolls.wakeups.remove(&target) {
            self.loop_handle.remove(token);
        }
        let Some(deadline) = deadline else {
            return;
        };
        let now = Instant::now();
        // A near-term source deadline is animation work even before presentation
        // feedback reveals its output's period (up to the supported ~30 Hz).
        if callback_due(deadline, now, Duration::from_millis(34)) {
            self.request_source_callback(&target);
        } else {
            let key = target.clone();
            let mut expected = deadline;
            let result = self.loop_handle.insert_source(
                Timer::from_duration(deadline.duration_since(now)),
                move |_, _, app| {
                    if let Target::Panel(epoch) = &key {
                        app.with_epoch(*epoch, |app| app.wake_scroll_source(&key, &mut expected))
                            .unwrap_or(TimeoutAction::Drop)
                    } else {
                        app.wake_scroll_source(&key, &mut expected)
                    }
                },
            );
            match result {
                Ok(token) => {
                    self.scrolls.wakeups.insert(target, (deadline, token));
                }
                Err(error) => self.fail(error),
            }
        }
    }
    fn wake_scroll_source(&mut self, key: &Target, expected: &mut Instant) -> TimeoutAction {
        if !self
            .scrolls
            .wakeups
            .get(key)
            .is_some_and(|(at, _)| *at == *expected)
        {
            return TimeoutAction::Drop;
        }
        if let Some(next) = self.source_deadline(key) {
            let now = Instant::now();
            if !callback_due(next, now, Duration::from_millis(34)) {
                // Moving input postpones idle hiding. Reuse this timer
                // instead of registering another source on every frame.
                self.scrolls.wakeups.get_mut(key).unwrap().0 = next;
                *expected = next;
                return TimeoutAction::ToDuration(next.duration_since(now));
            }
            self.scrolls.wakeups.remove(key);
            self.request_source_callback(key);
        } else {
            self.scrolls.wakeups.remove(key);
        }
        TimeoutAction::Drop
    }
    fn fling_viewport(&self, target: &Target, horizontal: bool) -> f64 {
        let size = match target {
            Target::Canvas(epoch) => self
                .canvases
                .values()
                .find(|c| c.epoch == *epoch && c.scroll_active())
                .map(|c| c.viewport_size),
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
            let output = self.options.primary_factory.as_ref().and_then(|_| {
                self.active_input_output
                    .as_ref()
                    .and_then(|id| self.output_sessions.get(id).map(|s| (s.epoch, id.clone())))
            });
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
        self.deliver_source_event(target, event);
    }
    fn deliver_source_event(&mut self, target: &Target, event: UiEvent) {
        match target {
            Target::Canvas(epoch) => self.ui(UiEvent::Canvas(*epoch, Box::new(event))),
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
        let regions = match &target {
            Target::Canvas(epoch) => self
                .canvases
                .values()
                .find(|c| c.epoch == *epoch && c.scroll_active())
                .map(|c| c.source.scroll_regions()),
            Target::Window(_) => self.window.as_ref().map(|w| w.scroll_regions()),
            Target::Overlay(_) => self.overlay.as_ref().map(|o| o.scroll_regions()),
            Target::Panel(_) => self.panel_runtime.as_ref().map(|s| s.scroll_regions()),
            Target::Primary(Some((_, id))) => self
                .output_sessions
                .get(id)
                .and_then(|s| s.source.as_ref())
                .map(|s| s.scroll_regions()),
            Target::Primary(None) => self.runtime.as_ref().map(|s| s.scroll_regions()),
        };
        let content = regions.as_ref().is_some_and(|regions| {
            regions.iter().any(|r| {
                point.0 >= r.x && point.1 >= r.y && point.0 < r.x + r.w && point.1 < r.y + r.h
            })
        });
        let (delta, wheel) = movement(axis, content, source);
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
        self.scrolls.motions.insert(
            key,
            Motion {
                started: Instant::now(),
                distance: remaining + delta,
                delivered: 0.,
                point,
            },
        );
        self.request_scroll_frame(&target);
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
        let key = key.clone();
        self.scrolls.flings.insert(
            key.clone(),
            Fling {
                started: Instant::now(),
                velocity,
                viewport,
                delivered: 0.,
                point: pixels.point,
            },
        );
        self.request_scroll_frame(&key.0);
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

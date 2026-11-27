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
    fn scroll_steps_follow_detents_finger_travel_and_the_shared_rate_limit() {
        let spec = |min_interval_ms| bead_render::ScrollSteps {
            min_interval_ms,
            threshold: 40.,
        };
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let wheel = |delta| (delta, true);
        let mut s = Steps::default();
        assert_eq!(s.event(false, wheel(-100.), false, t0, spec(0)), Some(-1.));
        assert_eq!(s.event(true, wheel(100.), false, t0, spec(0)), Some(1.));
        // 15 + 15 + 15 px at threshold 40: one step on the third, 5 px kept.
        let mut s = Steps::default();
        let finger: Vec<_> = (0..3)
            .map(|_| s.event(false, (15., false), false, t0, spec(0)))
            .collect();
        assert_eq!(finger, [None, None, Some(1.)]);
        assert_eq!(s.acc[0], 5.);
        // An axis stop resets the remainder.
        assert_eq!(s.event(false, (0., false), true, t0, spec(0)), None);
        assert_eq!(s.acc[0], 0.);
        assert_eq!(s.event(false, (-30., false), false, t0, spec(0)), None);
        assert_eq!(s.event(false, (-30., false), false, t0, spec(0)), Some(-1.));
        // Steps at 0, 30 and 70 ms with a 60 ms limit: 30 is dropped; axes share the limit.
        let mut s = Steps::default();
        assert_eq!(s.event(false, wheel(1.), false, at(0), spec(60)), Some(1.));
        assert_eq!(s.event(true, wheel(1.), false, at(30), spec(60)), None);
        assert_eq!(s.event(false, wheel(1.), false, at(70), spec(60)), Some(1.));
    }
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
    fn burst_lines_report_gaps_and_skip_single_steps() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let (y, x) = ((Target::Panel(1), false), (Target::Panel(1), true));
        let mut state = State::default();
        // A surface whose renderer does not composite shifts: every step is today's worker step.
        for ms in [0, 8, 16, 33] {
            assert_eq!(state.content_step(&y, None, (4., 5.), 9., at(ms)), Some(9.));
        }
        state.content_step(&x, None, (4., 5.), 9., at(5));
        let tick = Duration::from_nanos(8_333_333);
        let panel = |key: &(Target, bool)| {
            let (target, horizontal) = key;
            state.bursts[key].line(&surface_label(target), *horizontal, tick)
        };
        assert_eq!(
            panel(&y).as_deref(),
            Some(
                "SCROLL_FRAMES surface=panel axis=y frames=4 duration_ms=33.000 \
                 interval_mean=11.000 p95=17.000 max=17.000 tick_ms=8.333 worker_frames=4 \
                 plane=- main_steps=0 syncs=0 stalls=0"
            )
        );
        assert_eq!(panel(&x), None);
        state.end_idle();
        assert!(state.bursts.is_empty());
    }

    #[test]
    fn p95_is_nearest_rank() {
        let v: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(p95(&v), 19.);
        assert_eq!(p95(&[1., 2., 4.]), 4.);
        assert_eq!(p95(&[]), 0.);
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

#[cfg(test)]
mod plane_tests {
    use super::*;

    const KEY: (Target, bool) = (Target::Panel(1), false);
    const AT: (f32, f32) = (50., 100.);
    /// A 200 px viewport on 1000 px of content; moving: overscan 1, as layout bakes it.
    fn layers(offset: f32, moving: bool) -> Vec<bead_render::SceneLayer> {
        let clip = Rect::new(0., 0., 100., 200.);
        let (y, bottom) = if moving {
            ((-200_f32).max(-offset), 400_f32.min(1000. - offset))
        } else {
            (0., 200.)
        };
        let plane = bead_render::ScrollPlane {
            offset,
            limit: 800.,
            clip,
            interest: Rect::new(0., y, 100., bottom - y),
            ready: (y + offset, bottom + offset),
        };
        vec![bead_render::SceneLayer {
            id: "list".into(),
            parent: None,
            own: 0..1,
            subtree_end: 9,
            bounds: clip,
            scroll: Some(plane),
        }]
    }
    fn step(
        state: &mut State,
        scene: &[bead_render::SceneLayer],
        delta: f64,
        now: Instant,
    ) -> Option<f64> {
        state.content_step(&KEY, Some(scene), AT, delta, now)
    }
    fn line(state: &State) -> String {
        state.bursts[&KEY]
            .line("panel", false, Duration::from_millis(8))
            .unwrap()
    }

    #[test]
    fn a_step_inside_ready_sends_no_ui_event() {
        let (mut state, t0) = (State::default(), Instant::now());
        let scene = layers(300., true);
        assert_eq!(step(&mut state, &scene, 40., t0), None);
        assert!(state.syncs(None, false, t0).is_empty());
        // The draw moves the content up by what main moved, and damages the viewport.
        let (shifts, clips) = state.plane_shifts(&KEY.0, &scene, 1.5);
        assert_eq!(shifts, [(1..9, scene[0].scroll.unwrap().clip, (0., -40.))]);
        assert_eq!(clips, [scene[0].scroll.unwrap().clip]);
    }

    #[test]
    fn a_step_outside_ready_falls_to_the_worker_and_counts_a_stall() {
        let (mut state, t0) = (State::default(), Instant::now());
        let scene = layers(300., true);
        assert_eq!(step(&mut state, &scene, 40., t0), None);
        // 590 is past 500, the last offset with a ready viewport: the worker gets it + main's 40.
        let t1 = t0 + Duration::from_millis(8);
        assert_eq!(step(&mut state, &scene, 250., t1), Some(290.));
        // A plane at rest has `ready` = its viewport: today's worker step, not a stall.
        let mut rest = State::default();
        assert_eq!(step(&mut rest, &layers(300., false), 40., t0), Some(40.));
        assert_eq!(
            line(&state),
            "SCROLL_FRAMES surface=panel axis=y frames=2 duration_ms=8.000 interval_mean=8.000 \
             p95=8.000 max=8.000 tick_ms=8.000 worker_frames=1 plane=list main_steps=1 syncs=0 \
             stalls=1"
        );
        let b = &rest.bursts[&KEY];
        assert_eq!((b.worker, b.main, b.stalls), (1, 0, 0));
    }

    #[test]
    fn syncs_fire_at_50_ms_near_the_ready_edge_and_before_input() {
        let (mut state, t0) = (State::default(), Instant::now());
        let scene = layers(300., true);
        let scroll = |delta| {
            (
                Target::Panel(1),
                UiEvent::Scroll {
                    x: AT.0,
                    y: AT.1,
                    delta,
                },
            )
        };
        let syncs = |state: &mut State, flush, at| -> Vec<String> {
            let syncs = state.syncs(Some(&KEY.0), flush, at);
            syncs
                .iter()
                .map(|(t, e)| format!("{}:{e:?}", surface_label(t)))
                .collect()
        };
        let expect = |delta| vec![format!("panel:{:?}", scroll(delta).1)];
        step(&mut state, &scene, 40., t0);
        assert!(syncs(&mut state, false, t0 + Duration::from_millis(49)).is_empty());
        assert_eq!(
            syncs(&mut state, false, t0 + Duration::from_millis(50)),
            expect(40.)
        );
        // Near the edge (440: 60 px from the end of ready, ½ overscan is 100) with nothing in
        // flight it syncs at once.
        let mut edge = State::default();
        step(&mut edge, &scene, 140., t0);
        assert_eq!(syncs(&mut edge, false, t0), expect(140.));
        // Input flushes every plane first: view_event delivers these before its event.
        step(&mut state, &scene, 25., t0 + Duration::from_millis(60));
        let flushed = state.syncs(None, true, t0 + Duration::from_millis(61));
        assert_eq!(
            format!("{:?}", flushed[0].1),
            format!("{:?}", scroll(25.).1)
        );
        assert!(
            state
                .syncs(None, true, t0 + Duration::from_millis(62))
                .is_empty()
        );
        assert_eq!(state.bursts[&KEY].syncs, 2);
    }

    #[test]
    fn a_worker_write_is_adopted_and_stops_the_motion() {
        let (mut state, t0) = (State::default(), Instant::now());
        state.motions.insert(
            KEY,
            Motion {
                started: t0,
                distance: 300.,
                delivered: 40.,
                point: AT,
            },
        );
        step(&mut state, &layers(300., true), 40., t0);
        // The worker wrote 600 (a key, Luau or a reload): main takes it and its motion ends.
        state.adopt(&KEY.0, &layers(600., true));
        assert_eq!(state.planes.0[&(KEY.0, "list".to_string())].live, 600.);
        assert!(!state.animating(&KEY.0));
    }

    #[test]
    fn a_scene_that_merged_two_of_three_syncs_is_no_worker_write() {
        let (mut state, t0) = (State::default(), Instant::now());
        state.motions.insert(
            KEY,
            Motion {
                started: t0,
                distance: 300.,
                delivered: 0.,
                point: AT,
            },
        );
        for delta in [10., 20., 30.] {
            step(&mut state, &layers(300., true), delta, t0);
            assert_eq!(state.syncs(None, true, t0).len(), 1);
        }
        // The worker merged the first two syncs; the third is still in flight.
        let (shifts, _) = state.plane_shifts(&KEY.0, &layers(330., true), 1.);
        assert_eq!(shifts[0].2, (0., -30.));
        assert!(state.animating(&KEY.0));
        let (shifts, _) = state.plane_shifts(&KEY.0, &layers(360., true), 1.);
        assert!(shifts.is_empty() && state.planes.0[&(KEY.0, "list".to_string())].sent.is_empty());
    }

    #[test]
    fn the_settle_sends_the_final_sync_once_and_drops_the_plane_once_baked() {
        let (mut state, t0) = (State::default(), Instant::now());
        step(&mut state, &layers(300., true), 40., t0);
        // The motion ended (no wheel, fling or finger left): one final sync, then none.
        assert!(state.ended(&KEY.0));
        assert_eq!(state.syncs(Some(&KEY.0), true, t0).len(), 1);
        assert!(state.syncs(Some(&KEY.0), true, t0).is_empty());
        state.drop_idle(&KEY.0);
        assert_eq!(state.planes.0.len(), 1, "kept while the sync is in flight");
        // The draw of the scene that baked it shows it unshifted, then the plane drops.
        state.plane_shifts(&KEY.0, &layers(340., true), 1.);
        assert!(state.planes.0.is_empty());
    }

    fn kinds(events: &[(Target, UiEvent)]) -> Vec<String> {
        events.iter().map(|(_, e)| format!("{e:?}")).collect()
    }

    #[test]
    fn the_start_signal_is_sent_once_per_burst_per_plane_before_the_first_step() {
        let (mut state, t0) = (State::default(), Instant::now());
        let scene = layers(300., true);
        step(&mut state, &scene, 40., t0);
        let starts = state.take_starts();
        assert_eq!(
            kinds(&starts),
            [r#"Model({"bead.scrolling": Text("list")})"#]
        );
        step(&mut state, &scene, 40., t0);
        assert!(state.take_starts().is_empty(), "not per step");
        // After the settle the next burst names the plane again.
        state.settle(&KEY.0, t0);
        step(&mut state, &scene, 40., t0);
        assert_eq!(state.take_starts().len(), 1);
    }

    #[test]
    fn the_settle_sends_one_scroll_settled_after_the_final_sync() {
        let (mut state, t0) = (State::default(), Instant::now());
        step(&mut state, &layers(300., true), 40., t0);
        let out = state.settle(&KEY.0, t0);
        let sync = format!(
            "{:?}",
            UiEvent::Scroll {
                x: AT.0,
                y: AT.1,
                delta: 40.
            }
        );
        assert_eq!(
            kinds(&out),
            [sync, r#"ScrollSettled { id: "list" }"#.into()]
        );
        assert!(state.settle(&KEY.0, t0).is_empty(), "once");
    }

    #[test]
    fn a_close_flush_settles_only_its_own_target() {
        let (mut state, t0) = (State::default(), Instant::now());
        let other = (Target::Panel(2), false);
        step(&mut state, &layers(300., true), 40., t0);
        state.content_step(&other, Some(&layers(300., true)), AT, 40., t0);
        let out = state.settle(&KEY.0, t0);
        assert!(out.iter().all(|(t, _)| *t == KEY.0) && out.len() == 2);
        assert_eq!(
            state.settle(&other.0, t0).len(),
            2,
            "the other panel still moves"
        );
    }

    /// The plane's viewport as the source lists it; `settle`: it has `on_settle`.
    fn viewport(settle: bool) -> Vec<bead_render::ScrollViewport> {
        vec![bead_render::ScrollViewport {
            id: "list".into(),
            rect: Rect::new(0., 0., 100., 200.),
            horizontal: false,
            offset: 300.,
            settle,
        }]
    }

    #[test]
    fn a_burst_over_on_settle_sends_one_scroll_settled_on_the_worker_and_the_compositing_path() {
        let (mut state, t0) = (State::default(), Instant::now());
        // Worker path (the renderer does not composite): today's steps, then one settle.
        for delta in [40., 40., -15.] {
            state.note_settles(&KEY, viewport(true), AT);
            assert_eq!(state.content_step(&KEY, None, AT, delta, t0), Some(delta));
        }
        let settled = [r#"ScrollSettled { id: "list" }"#];
        assert_eq!(kinds(&state.settle(&KEY.0, t0)), settled);
        assert!(state.settle(&KEY.0, t0).is_empty(), "once per burst");
        // Compositing path: the named plane settles once, after the final sync.
        state.note_settles(&KEY, viewport(true), AT);
        step(&mut state, &layers(300., true), 40., t0);
        let out = kinds(&state.settle(&KEY.0, t0));
        assert_eq!(out.len(), 2);
        assert_eq!(out[1], settled[0]);
    }

    #[test]
    fn a_renderer_that_cannot_composite_gets_todays_events() {
        let (mut state, t0) = (State::default(), Instant::now());
        // U5a2: no settle over a viewport without `on_settle`, on the other axis, or elsewhere.
        let (mut x, mut away) = (viewport(true), viewport(true));
        (x[0].horizontal, away[0].rect.x) = (true, 500.);
        for viewports in [viewport(false), x, away] {
            state.note_settles(&KEY, viewports, AT);
        }
        for delta in [40., 40., -15.] {
            assert_eq!(state.content_step(&KEY, None, AT, delta, t0), Some(delta));
        }
        assert!(state.planes.0.is_empty() && state.syncs(None, true, t0).is_empty());
        assert!(state.take_starts().is_empty() && state.settle(&KEY.0, t0).is_empty());
        assert!(line(&state).ends_with("worker_frames=3 plane=- main_steps=0 syncs=0 stalls=0"));
    }

    #[test]
    fn a_renderer_that_stops_compositing_mid_burst_sends_the_pending_motion_at_once() {
        let (mut state, t0) = (State::default(), Instant::now());
        let scene = layers(300., true);
        assert_eq!(step(&mut state, &scene, 40., t0), None);
        // The GL renderer fell back: the next step carries main's 40 px with its own 10 px.
        assert_eq!(state.content_step(&KEY, None, AT, 10., t0), Some(50.));
        assert_eq!(state.plane_shifts(&KEY.0, &scene, 1.), (vec![], vec![]));
        assert_eq!(state.content_step(&KEY, None, AT, 10., t0), Some(10.));
    }

    #[test]
    fn one_surface_shifts_never_reach_another() {
        let (mut state, t0) = (State::default(), Instant::now());
        let (a, b) = (
            Target::Primary(Some((1, "A".into()))),
            Target::Primary(Some((2, "B".into()))),
        );
        let scene = layers(300., true);
        assert_eq!(
            state.content_step(&(a.clone(), false), Some(&scene[..]), AT, 40., t0),
            None
        );
        assert_eq!(state.plane_shifts(&b, &scene, 1.), (vec![], vec![]));
        assert_eq!(state.plane_shifts(&a, &scene, 1.).0.len(), 1);
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
    steps: std::collections::BTreeMap<(Target, String), Steps>,
    bursts: std::collections::BTreeMap<(Target, bool), Burst>,
    planes: Planes,
    /// B0419: planes the worker was told are moving (`bead.scrolling`), until their settle.
    named: std::collections::BTreeSet<(Target, String)>,
    /// U5a2: viewports with `on_settle` that a burst ran over, until its settle.
    settles: std::collections::BTreeSet<(Target, String)>,
    /// Start signals to deliver before the step that caused them.
    starts: Vec<(Target, UiEvent)>,
}
/// One motion burst of a (target, axis): its moved steps, for the SCROLL_FRAMES journal line.
struct Burst {
    last: Instant,
    gaps: Vec<f64>,
    worker: u32,
    /// B0419: steps main moved a plane for, its syncs, stalls (worker steps of a moving plane).
    main: u32,
    syncs: u32,
    stalls: u32,
    plane: Option<String>,
}
/// Nearest-rank 95th percentile of a sorted slice (0 when empty).
fn p95(sorted: &[f64]) -> f64 {
    (sorted.len() * 95)
        .div_ceil(100)
        .checked_sub(1)
        .map_or(0., |i| sorted[i])
}
impl Burst {
    fn line(&self, surface: &str, horizontal: bool, tick: Duration) -> Option<String> {
        let mut gaps = self.gaps.clone();
        gaps.sort_by(f64::total_cmp);
        let duration: f64 = gaps.iter().sum();
        Some(format!(
            "SCROLL_FRAMES surface={surface} axis={} frames={} duration_ms={duration:.3} \
             interval_mean={:.3} p95={:.3} max={:.3} tick_ms={:.3} worker_frames={} plane={} \
             main_steps={} syncs={} stalls={}",
            if horizontal { 'x' } else { 'y' },
            gaps.len() + 1,
            duration / gaps.len() as f64,
            p95(&gaps),
            gaps.last()?,
            tick.as_secs_f64() * 1000.,
            self.worker,
            self.plane.as_deref().unwrap_or("-"),
            self.main,
            self.syncs,
            self.stalls,
        ))
    }
}
fn surface_label(target: &Target) -> String {
    match target {
        Target::Canvas(e) => format!("canvas:{e}"),
        Target::Window(e) => format!("window:{e}"),
        Target::Overlay(e) => format!("overlay:{e}"),
        Target::Panel(_) => "panel".into(),
        Target::Primary(None) => "primary".into(),
        Target::Primary(Some((_, name))) => format!("primary:{name}"),
    }
}
/// Per-node state of a `scroll_steps` node (B0329): finger travel per axis, and the last step.
#[derive(Default)]
struct Steps {
    acc: [f64; 2],
    last: Option<Instant>,
}
impl Steps {
    /// One axis event: the +1 (down/right) or -1 step to deliver, if any. A wheel detent is one
    /// step; finger travel steps each `threshold` px and keeps the remainder. Both axes share
    /// the rate limit, and a step inside the window is dropped, not queued.
    fn event(
        &mut self,
        horizontal: bool,
        (delta, wheel): (f64, bool),
        stop: bool,
        now: Instant,
        spec: bead_render::ScrollSteps,
    ) -> Option<f64> {
        let acc = &mut self.acc[usize::from(horizontal)];
        let threshold = f64::from(spec.threshold);
        let step = if delta == 0. {
            None
        } else if wheel {
            Some(delta.signum())
        } else {
            *acc += delta;
            (acc.abs() >= threshold).then(|| {
                let sign = acc.signum();
                *acc -= sign * threshold;
                sign
            })
        };
        if stop {
            *acc = 0.;
        }
        let window = Duration::from_millis(spec.min_interval_ms.into());
        let step = step.filter(|_| self.last.is_none_or(|t| now.duration_since(t) >= window))?;
        self.last = Some(now);
        Some(step)
    }
}
impl State {
    pub(super) fn interrupt(&mut self, target: &Target) {
        self.stop(target);
        self.planes.0.retain(|(t, _), _| t != target);
        self.end_idle();
    }
    fn stop(&mut self, target: &Target) {
        self.motions.retain(|(t, _), _| t != target);
        self.flings.retain(|(t, _), _| t != target);
        self.pixels.retain(|(t, _), _| t != target);
    }
    fn ended(&self, target: &Target) -> bool {
        !self
            .motions
            .keys()
            .chain(self.flings.keys())
            .chain(self.pixels.keys())
            .any(|(t, _)| t == target)
    }
    /// B0419 (x is U5a): `layers` of a surface that composites shifts; Some(Δ) for the worker.
    fn content_step(
        &mut self,
        key: &(Target, bool),
        layers: Option<&[bead_render::SceneLayer]>,
        point: (f32, f32),
        delta: f64,
        now: Instant,
    ) -> Option<f64> {
        let hit = layers.filter(|_| !key.1).and_then(|layers| {
            self.adopt(&key.0, layers);
            self.planes.step(&key.0, layers, point, delta, now)
        });
        if let Some((id, ..)) = &hit
            && self.named.insert((key.0.clone(), id.clone()))
        {
            let model = [(
                "bead.scrolling".to_string(),
                bead_core::Value::Text(id.clone()),
            )];
            self.starts
                .push((key.0.clone(), UiEvent::Model(model.into())));
        }
        // A renderer that stopped compositing (a GL fallback) hands over the unsent motion now,
        // so the content does not snap back to the baked offset until the next flush.
        let delta = match layers {
            None if !key.1 => delta + self.planes.release(&key.0),
            _ => delta,
        };
        if delta != 0. {
            let burst = self.bursts.entry(key.clone()).or_insert(Burst {
                last: now,
                gaps: Vec::new(),
                worker: 0,
                main: 0,
                syncs: 0,
                stalls: 0,
                plane: None,
            });
            if burst.worker + burst.main > 0 {
                burst
                    .gaps
                    .push(now.duration_since(burst.last).as_secs_f64() * 1000.);
            }
            burst.last = now;
            let (worker, stall) = hit.as_ref().map_or((true, false), |h| (h.1.is_some(), h.2));
            (burst.worker, burst.main) = (
                burst.worker + u32::from(worker),
                burst.main + u32::from(!worker),
            );
            burst.stalls += u32::from(stall);
            if let Some((id, ..)) = &hit {
                burst.plane = Some(id.clone());
            }
        }
        hit.map_or(Some(delta), |(_, worker, _)| worker)
    }
    /// B0419 U5a2: a content burst over an `on_settle` viewport ends with its ScrollSettled.
    fn note_settles(
        &mut self,
        key: &(Target, bool),
        viewports: Vec<bead_render::ScrollViewport>,
        (x, y): (f32, f32),
    ) {
        for v in viewports {
            let r = v.rect;
            let inside = x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;
            if v.settle && v.horizontal == key.1 && inside {
                self.settles.insert((key.0.clone(), v.id));
            }
        }
    }
    /// B0419: the plane start signals to deliver before the step's own event.
    fn take_starts(&mut self) -> Vec<(Target, UiEvent)> {
        std::mem::take(&mut self.starts)
    }
    /// B0419: the final syncs of `target`, then one settle per plane the worker was told moves
    /// and (U5a2) per noted `on_settle` viewport.
    fn settle(&mut self, target: &Target, now: Instant) -> Vec<(Target, UiEvent)> {
        let mut out = self.syncs(Some(target), true, now);
        let settles = std::mem::take(&mut self.settles).into_iter();
        let (mine, rest): (std::collections::BTreeSet<_>, _) =
            settles.partition(|(t, _)| t == target);
        self.settles = rest;
        self.named.extend(mine);
        let named = std::mem::take(&mut self.named);
        for (t, id) in named {
            if t == *target {
                out.push((t, UiEvent::ScrollSettled { id }));
            } else {
                self.named.insert((t, id));
            }
        }
        out
    }
    /// B0419: a worker write to a plane main was moving ends the motion.
    fn adopt(&mut self, target: &Target, layers: &[bead_render::SceneLayer]) {
        if self.planes.adopt(target, layers) {
            self.stop(target);
        }
    }
    /// B0419: the plane syncs due, counted in their bursts.
    pub(super) fn syncs(
        &mut self,
        target: Option<&Target>,
        flush: bool,
        now: Instant,
    ) -> Vec<(Target, UiEvent)> {
        let syncs = self.planes.syncs(target, flush, now);
        for (t, _) in &syncs {
            if let Some(burst) = self.bursts.get_mut(&(t.clone(), false)) {
                burst.syncs += 1;
            }
        }
        syncs
    }
    /// B0419: after the motion, a plane drops once its syncs are baked and drawn.
    fn drop_idle(&mut self, target: &Target) {
        if self.ended(target) {
            self.planes.0.retain(|(t, _), e| {
                t != target || !e.sent.is_empty() || e.unsent != 0. || e.shifted
            });
        }
    }
    /// B0419: adopt the drawn scene, then its shifts and the clips to damage.
    pub(super) fn plane_shifts(
        &mut self,
        target: &Target,
        layers: &[bead_render::SceneLayer],
        scale: f32,
    ) -> (Vec<Shift>, Vec<Rect>) {
        if !self.planes.0.keys().any(|(t, _)| t == target) {
            return Default::default();
        }
        self.adopt(target, layers);
        let out = self.planes.shifts(target, layers, scale);
        self.drop_idle(target);
        out
    }
    /// Log and drop every burst whose motion, fling and finger state is gone.
    fn end_idle(&mut self) {
        let (motions, flings, pixels) = (&self.motions, &self.flings, &self.pixels);
        self.bursts.retain(|key, burst| {
            if motions.contains_key(key) || flings.contains_key(key) || pixels.contains_key(key) {
                return true;
            }
            if let Some(line) = burst.line(&surface_label(&key.0), key.1, tick()) {
                bead_core::profile::eline(format_args!("{line}"));
            }
            false
        });
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
/// B0419 (spec 0286 U2b): a moving plane syncs its offset to the worker at least this often.
const PLANE_SYNC: Duration = Duration::from_millis(50);
/// (content range, clip, (0, baked - shown)), as `Renderer::scroll_shifts` takes it.
type Shift = (std::ops::Range<usize>, Rect, (f32, f32));
/// The scroll planes main moves between worker syncs, per (target, plane id).
#[derive(Default)]
struct Planes(std::collections::BTreeMap<(Target, String), Live>);
struct Live {
    /// Shown (within the baked `ready`); `baked` is the last adopted scene's offset.
    live: f32,
    baked: f32,
    unsent: f64,
    /// Running totals of the syncs since `baked`: a scene bakes some prefix of them.
    sent: Vec<f64>,
    last_sync: Instant,
    /// So a sync hits the plane the steps did.
    point: (f32, f32),
    /// Near a `ready` edge with nothing in flight.
    due: bool,
    /// The next draw damages the clip the last one shifted.
    shifted: bool,
}
impl Live {
    /// Hand `delta` plus the unsent motion to the worker.
    fn send(&mut self, delta: f64, now: Instant) -> f64 {
        let delta = std::mem::take(&mut self.unsent) + delta;
        self.sent
            .push(self.sent.last().copied().unwrap_or(0.) + delta);
        (self.last_sync, self.due) = (now, false);
        delta
    }
}
/// The offsets whose viewport lies inside the baked `ready` span.
fn span(layer: &bead_render::SceneLayer, p: &bead_render::ScrollPlane) -> (f32, f32) {
    let lead = p.clip.y - layer.bounds.y;
    (p.ready.0 - lead, p.ready.1 - lead - p.clip.h)
}
impl Planes {
    /// The innermost plane under `point` that can take Δ (the `Session::scroll` rule) moves on
    /// main while its viewport stays in `ready`, else the worker gets it (id, worker Δ, stall).
    fn step(
        &mut self,
        target: &Target,
        layers: &[bead_render::SceneLayer],
        point: (f32, f32),
        delta: f64,
        now: Instant,
    ) -> Option<(String, Option<f64>, bool)> {
        let inside = |r: Rect| {
            point.0 >= r.x && point.1 >= r.y && point.0 < r.x + r.w && point.1 < r.y + r.h
        };
        let (layer, p, live) = layers.iter().rev().find_map(|l| {
            let p = l.scroll.filter(|p| inside(p.clip))?;
            let live = self
                .0
                .get(&(target.clone(), l.id.clone()))
                .map_or(p.offset, |e| e.live);
            ((live + delta as f32).min(p.limit).max(0.) != live).then_some((l, p, live))
        })?;
        let e = self
            .0
            .entry((target.clone(), layer.id.clone()))
            .or_insert(Live {
                live,
                baked: p.offset,
                unsent: 0.,
                sent: Vec::new(),
                last_sync: now,
                point,
                due: false,
                shifted: false,
            });
        let next = (live + delta as f32).min(p.limit).max(0.);
        let ((lo, hi), moving) = (span(layer, &p), p.interest != p.clip);
        e.point = point;
        if moving && next >= lo && next <= hi {
            e.unsent += f64::from(next - e.live);
            e.live = next;
            // ½ overscan viewport from a ready edge that is not the content's end.
            let margin = (p.interest.h - p.clip.h) / 4.;
            e.due = e.sent.is_empty()
                && ((lo > 0. && next - lo < margin) || (hi < p.limit && hi - next < margin));
            return Some((layer.id.clone(), None, false));
        }
        e.live = next;
        Some((layer.id.clone(), Some(e.send(delta, now)), moving))
    }
    /// An offset other than baked + a prefix of the syncs in flight is a worker write (keys,
    /// Luau, clamp, reload): `live` takes it. True when that stopped a plane main was moving.
    fn adopt(&mut self, target: &Target, layers: &[bead_render::SceneLayer]) -> bool {
        let mut stopped = false;
        self.0.retain(|(t, id), e| {
            let Some(p) = layers
                .iter()
                .find(|l| t == target && l.id == *id)
                .and_then(|l| l.scroll)
            else {
                return t != target;
            };
            let total = |i: usize| i.checked_sub(1).map_or(0., |i| e.sent[i]);
            let baked = |c: f64| (f64::from(e.baked) + c).min(f64::from(p.limit)).max(0.) as f32;
            // The longest prefix: a clamped or no-op sync bakes into the same offset.
            let found = (0..=e.sent.len())
                .rev()
                .find(|&i| (baked(total(i)) - p.offset).abs() <= 0.5)
                .map(|i| (i, total(i)));
            if let Some((i, c)) = found {
                e.sent.drain(..i);
                e.sent.iter_mut().for_each(|t| *t -= c);
                e.baked = p.offset;
                if e.sent.is_empty() && e.unsent == 0. {
                    e.live = p.offset;
                }
            } else {
                stopped |= p.interest != p.clip || e.unsent != 0.;
                (e.live, e.baked, e.unsent) = (p.offset, p.offset, 0.);
                e.sent.clear();
            }
            true
        });
        stopped
    }
    /// Syncs at 50 ms, near a `ready` edge, or on `flush` (input, motion end); `None`: all.
    fn syncs(
        &mut self,
        target: Option<&Target>,
        flush: bool,
        now: Instant,
    ) -> Vec<(Target, UiEvent)> {
        let mut out = Vec::new();
        for ((t, _), e) in &mut self.0 {
            if target.is_none_or(|target| target == t)
                && e.unsent != 0.
                && (flush || e.due || now.duration_since(e.last_sync) >= PLANE_SYNC)
            {
                let (x, y) = e.point;
                out.push((
                    t.clone(),
                    UiEvent::Scroll {
                        x,
                        y,
                        delta: e.send(0., now),
                    },
                ));
            }
        }
        out
    }
    /// Drop `target`'s planes, returning their unsent motion.
    fn release(&mut self, target: &Target) -> f64 {
        let mut unsent = 0.;
        self.0.retain(|(t, _), e| {
            unsent += if t == target { e.unsent } else { 0. };
            t != target
        });
        unsent
    }
    /// Shifts snapped to device px, and the clips to damage: the frame's own damage misses moves.
    fn shifts(
        &mut self,
        target: &Target,
        layers: &[bead_render::SceneLayer],
        scale: f32,
    ) -> (Vec<Shift>, Vec<Rect>) {
        let (mut shifts, mut clips) = (Vec::new(), Vec::new());
        for l in layers {
            let Some(p) = l.scroll else { continue };
            let Some(e) = self.0.get_mut(&(target.clone(), l.id.clone())) else {
                continue;
            };
            let (lo, hi) = span(l, &p);
            let shift = ((p.offset - e.live.min(hi).max(lo)) * scale).round() / scale;
            if e.shifted || p.interest != p.clip {
                clips.push(p.clip);
            }
            e.shifted = shift != 0.;
            if e.shifted {
                shifts.push((l.own.end..l.subtree_end, p.clip, (0., shift)));
            }
        }
        (shifts, clips)
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
        self.settle_scroll(target);
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
                        .request_if_presentable(s.shows_buffer())
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
                            .request_if_presentable(s.shows_buffer())
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
                self.content_scroll(target, horizontal, point, delta, now);
            }
        }
        self.end_motion(target, now);
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
    /// B0419: main moves a plane where the renderer composites shifts, else the worker gets Δ.
    fn content_scroll(
        &mut self,
        target: &Target,
        horizontal: bool,
        point: (f32, f32),
        delta: f64,
        now: Instant,
    ) {
        let surface = match target {
            Target::Panel(_) => self.companion.as_mut().map(|(s, r, _)| (&*s, r)),
            Target::Primary(Some((epoch, id))) => self
                .output_sessions
                .get(id)
                .filter(|s| s.epoch == *epoch)
                .map(|s| &s.scene)
                .zip(Some(&mut self.renderer)),
            Target::Primary(None) => Some((&self.scene, &mut self.renderer)),
            _ => None,
        };
        // The probe: an empty range only a compositing renderer accepts.
        let probe = [(0..0, Rect::new(0., 0., 0., 0.), (0., 0.))];
        let layers =
            surface.and_then(|(scene, r)| r.scroll_shifts(&probe).then_some(&scene.layers[..]));
        let key = (target.clone(), horizontal);
        let step = self.scrolls.content_step(&key, layers, point, delta, now);
        for (target, start) in self.scrolls.take_starts() {
            self.deliver_source_event(&target, start);
        }
        match step {
            Some(delta) => self.deliver_scroll(target, horizontal, point, delta),
            None => self.redraw_target(target),
        }
        for (target, sync) in self.scrolls.syncs(Some(target), false, now) {
            self.deliver_source_event(&target, sync);
        }
    }
    fn redraw_target(&mut self, target: &Target) {
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            let s = &self.surfaces[i];
            if match target {
                Target::Panel(_) => s.companion,
                Target::Primary(o) => {
                    !s.companion && o.as_ref().is_none_or(|(_, name)| *name == s.output_name)
                }
                _ => false,
            } {
                self.surfaces[i].redraw = true;
                self.draw(i, &qh);
            }
        }
    }
    /// B0419: the final syncs of `target`'s planes, then their settles.
    pub(super) fn settle_scroll(&mut self, target: &Target) {
        for (target, event) in self.scrolls.settle(target, Instant::now()) {
            self.deliver_source_event(&target, event);
        }
    }
    /// The final plane syncs and settles of an ended motion (B0419), then the burst lines.
    fn end_motion(&mut self, target: &Target, now: Instant) {
        if self.scrolls.ended(target) {
            for (target, event) in self.scrolls.settle(target, now) {
                self.deliver_source_event(&target, event);
            }
            self.scrolls.drop_idle(target);
        }
        self.scrolls.end_idle();
    }
    /// B0419: input hit-tests the offset main shows, so every unsynced plane syncs first.
    pub(super) fn flush_planes(&mut self) {
        for (target, sync) in self.scrolls.syncs(None, true, Instant::now()) {
            self.deliver_source_event(&target, sync);
        }
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
        let scene: Option<&dyn SceneSource> = match &target {
            Target::Canvas(epoch) => self
                .canvases
                .values()
                .find(|c| c.epoch == *epoch && c.scroll_active())
                .map(|c| &*c.source),
            Target::Window(_) => self.window.as_ref().map(|w| w.source()),
            Target::Overlay(_) => self.overlay.as_ref().map(|o| o.source()),
            Target::Panel(_) => self.panel_runtime.as_deref(),
            Target::Primary(Some((_, id))) => self
                .output_sessions
                .get(id)
                .and_then(|s| s.source.as_deref()),
            Target::Primary(None) => self.runtime.as_deref(),
        };
        let regions = scene.map(|s| s.scroll_regions());
        let steps = scene.map(|s| s.scroll_step_regions());
        let viewports = scene.map_or_else(Vec::new, |s| s.scroll_viewports());
        let inside = |r: &Rect| {
            point.0 >= r.x && point.1 >= r.y && point.0 < r.x + r.w && point.1 < r.y + r.h
        };
        if let Some((id, _, spec)) = steps
            .iter()
            .flatten()
            .rev()
            .find(|(_, region, _)| inside(region))
        {
            // A stepped node takes neither smoothing nor flings: one event, at most one step.
            let key = (target.clone(), horizontal);
            self.scrolls.motions.remove(&key);
            self.scrolls.flings.remove(&key);
            self.scrolls.pixels.remove(&key);
            let state = self.scrolls.steps.entry((target.clone(), id.clone()));
            let step = state.or_default().event(
                horizontal,
                movement(axis, true, source),
                axis.stop,
                Instant::now(),
                *spec,
            );
            if let Some(step) = step {
                self.deliver_scroll(&target, horizontal, point, step);
            }
            self.scrolls.end_idle();
            return;
        }
        let content = regions.is_some_and(|regions| regions.iter().any(inside));
        let (delta, wheel) = movement(axis, content, source);
        let key = (target.clone(), horizontal);
        if delta == 0. {
            if axis.stop {
                self.release_pixels(&key);
                self.end_motion(&target, Instant::now());
            }
            return;
        }
        if content {
            self.scrolls.note_settles(&key, viewports, point);
        }
        let previous = self.scrolls.motions.remove(&key);
        self.scrolls.flings.remove(&key);
        if !wheel || bead_core::animation::reduced_motion() {
            if content {
                self.content_scroll(&target, horizontal, point, delta, Instant::now());
            } else {
                self.deliver_scroll(&target, horizontal, point, delta);
            }
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
            self.end_motion(&target, Instant::now());
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

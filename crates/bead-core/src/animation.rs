// SPDX-License-Identifier: MIT
//! Stable-ID animation state lives outside views. Rendering consumes only the pose.
use crate::profile::{NO_SPLITS, Splits};
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
/// Named easing tokens; one list for every place a curve is named (Luau `ui.animate`,
/// `Panel.transition`, host motion specs and the compositor mapping).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Curve {
    Linear,
    /// `out-cubic`, the default opening curve (alias `ease`).
    EaseOut,
    /// `in-out-cubic` (alias `ease-in-out`).
    EaseInOut,
    /// `in-quart`: accelerating exit curve that commits most travel to its final frames.
    EmphasizedAccelerate,
    Spring,
    InCubic,
    OutQuint,
    /// Soft overshoot (settles from about 1.1); use on scale, never on panel geometry.
    OutBack,
}
impl Curve {
    pub const NAMES: [(&'static str, Curve); 8] = [
        ("linear", Curve::Linear),
        ("out-cubic", Curve::EaseOut),
        ("in-cubic", Curve::InCubic),
        ("out-quint", Curve::OutQuint),
        ("in-out-cubic", Curve::EaseInOut),
        ("out-back", Curve::OutBack),
        ("in-quart", Curve::EmphasizedAccelerate),
        ("spring", Curve::Spring),
    ];
    /// The canonical names above plus the older `Panel.transition` spellings.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ease" | "ease-out" => Some(Curve::EaseOut),
            "ease-in" => Some(Curve::InCubic),
            "ease-in-out" => Some(Curve::EaseInOut),
            _ => Self::NAMES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| *c),
        }
    }
    pub fn name(self) -> &'static str {
        Self::NAMES
            .iter()
            .find(|(_, c)| *c == self)
            .expect("named")
            .0
    }
    /// Eased progress for `t` in 0..=1 (clamped). Every curve starts at 0 and ends at 1.
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            Curve::Linear => t,
            Curve::EaseOut => 1. - (1. - t).powi(3),
            Curve::EaseInOut if t < 0.5 => 4. * t.powi(3),
            Curve::EaseInOut => 1. - (-2. * t + 2.).powi(3) / 2.,
            Curve::EmphasizedAccelerate => t.powi(4),
            Curve::Spring => {
                let x = 8. * t;
                (1. - (1. + x) * (-x).exp()) / (1. - 9. * (-8_f32).exp())
            }
            Curve::InCubic => t.powi(3),
            Curve::OutQuint => 1. - (1. - t).powi(5),
            Curve::OutBack => {
                // c1 = 1.70158, c3 = c1 + 1 (the compositor protocol uses the same constants).
                let s = t - 1.;
                1. + 2.70158 * s.powi(3) + 1.70158 * s.powi(2)
            }
        }
    }
}
/// Named durations in seconds (the review's 120 Hz motion table); `ui.motion.duration` in Luau.
pub const DURATIONS: [(&str, f64); 17] = [
    ("hover", 0.09),
    ("press", 0.06),
    ("release", 0.12),
    ("trace", 0.12),
    ("fade", 0.16),
    ("pulse", 1.6),
    ("peek", 0.2),
    ("open", 0.22),
    ("close", 0.16),
    ("slide", 0.24),
    ("drop", 0.26),
    ("switch", 0.18),
    ("badge", 0.14),
    ("osd_in", 0.16),
    ("osd_out", 0.2),
    ("toast", 0.22),
    ("dock", 0.22),
];
/// The one panel motion profile (`ui.motion.panel` in Luau): open takes `travel / speed` clamped
/// to `open_min..=open_max`, close takes `close`, a hover-opened panel closes `leave_ms` after the pointer leaves.
pub struct PanelProfile {
    pub speed: f64,
    pub open_min: f64,
    pub open_max: f64,
    pub close: f64,
    pub leave_ms: u64,
}
pub const PANEL: PanelProfile = PanelProfile {
    speed: 1600.,
    open_min: 0.18,
    open_max: 0.32,
    close: 0.18,
    leave_ms: 150,
};
/// Default open duration in seconds of a panel that travels `travel` logical pixels.
pub fn panel_open_duration(travel: f32) -> f64 {
    (f64::from(travel.max(0.)) / PANEL.speed).clamp(PANEL.open_min, PANEL.open_max)
}
pub fn duration(name: &str) -> Option<f64> {
    DURATIONS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}
/// Nanoseconds since the process' animation epoch: a clock serialisable into view markers.
pub fn clock_ns() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    EPOCH
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as u64
}
thread_local! {
    static BUILD_CLOCK: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}
/// Stamp the current view build: every marker started and sampled in it shares this instant,
/// so a motion's first frame shows exactly its `from`.
pub fn begin_build_clock() -> u64 {
    let now = clock_ns();
    BUILD_CLOCK.with(|clock| clock.set(Some(now)));
    now
}
/// The current build's clock, or the live clock outside a build.
pub fn build_clock() -> u64 {
    BUILD_CLOCK
        .with(|clock| clock.get())
        .unwrap_or_else(clock_ns)
}
/// A motion this short (the clamp of a zero duration) is final from the moment it is targeted:
/// holding it only keeps a panel on its CPU renderer and draws its first frame transparent (B0380).
const INSTANT: f64 = 0.001;
/// A render's largest phases (name, seconds), largest first.
type Phases = [(&'static str, f64); 4];
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
    /// The worst gap's commit index and its split (wait, queue, render), seconds (B0295), then
    /// the part of the wait after the compositor stamped its callback (B0408).
    worst: (u32, [f64; 4]),
    /// The three costliest frame renders (commit index, seconds, largest phases), costliest first
    /// (B0305, B0380).
    top: [(u32, f64, Phases); 3],
    /// The costliest render's largest phases (name, seconds), largest first (B0310).
    top_split: [(&'static str, f64); 4],
    /// Rasters the main-thread presenter built while this motion ran (B0388).
    builds: usize,
}
/// One finished motion as the user saw it: request to first frame, then frames until the final pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slide {
    pub opening: bool,
    pub first_frame_ms: f64,
    pub frames: u32,
    pub duration_ms: f64,
    pub max_gap_ms: f64,
    /// Commit index that ended the worst gap, and where the gap went: previous commit to the frame
    /// callback that drew it (0 if none), callback to draw start, draw start to commit.
    pub gap_at: u32,
    pub gap_wait_ms: f64,
    pub gap_queue_ms: f64,
    pub gap_render_ms: f64,
    /// The three costliest frame renders (commit index, ms), costliest first; unused slots are 0.
    pub render_top: [(u32, f64); 3],
    /// The largest phases (name, ms) of the costliest render, largest first; unused slots are empty.
    pub top_split: [(&'static str, f64); 4],
    /// The same four phases for each `render_top` entry, in its order (B0380).
    pub top_splits: [[(&'static str, f64); 4]; 3],
    /// Rasters the main-thread presenter built over the slide (B0388).
    pub main_builds: usize,
    /// B0408: of `gap_wait_ms`, the time from the compositor's timestamp on that frame callback to
    /// its dispatch here (0 without a callback or a usable stamp): the rest is the compositor's.
    pub gap_dispatch_ms: f64,
}
impl std::fmt::Display for Slide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dir={} first_frame_ms={:.1} frames={} duration_ms={:.1} max_gap_ms={:.1} \
             gap_at={} gap_wait_ms={:.1} gap_queue_ms={:.1} gap_render_ms={:.1} render_top=",
            if self.opening { "open" } else { "close" },
            self.first_frame_ms,
            self.frames,
            self.duration_ms,
            self.max_gap_ms,
            self.gap_at,
            self.gap_wait_ms,
            self.gap_queue_ms,
            self.gap_render_ms
        )?;
        for (n, (at, ms)) in self
            .render_top
            .iter()
            .filter(|(_, ms)| *ms > 0.)
            .enumerate()
        {
            write!(f, "{}{at}:{ms:.1}", if n > 0 { "," } else { "" })?;
        }
        f.write_str(" top_split=")?;
        for (n, (stage, ms)) in self
            .top_split
            .iter()
            .filter(|(stage, _)| !stage.is_empty())
            .enumerate()
        {
            write!(f, "{}{stage}:{ms:.1}", if n > 0 { "," } else { "" })?;
        }
        f.write_str(" top_splits=")?;
        let frames = self.render_top.iter().zip(&self.top_splits);
        for (n, ((at, _), phases)) in frames.filter(|(_, p)| !p[0].0.is_empty()).enumerate() {
            write!(f, "{}{at}", if n > 0 { ";" } else { "" })?;
            let phases = phases.iter().filter(|(stage, _)| !stage.is_empty());
            for (i, (stage, ms)) in phases.enumerate() {
                write!(f, "{}{stage}:{ms:.1}", if i > 0 { "/" } else { ":" })?;
            }
        }
        // B0390: a held motion's clock starts at its first commit, and only a zero-duration
        // (slide-less) one also ends there, so it alone reports exactly 0.
        if self.duration_ms == 0. {
            f.write_str(" slide=none")?;
        }
        write!(
            f,
            " main_builds={} gap_dispatch_ms={:.1}",
            self.main_builds, self.gap_dispatch_ms
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
                    worst: (0, [0.; 4]),
                    top: [(0, 0., [("", 0.); 4]); 3],
                    top_split: [("", 0.); 4],
                    builds: 0,
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
        self.committed_timed(id, drawn, now, None, now, &NO_SPLITS)
    }
    /// Charge `builds` main-thread rasters of this frame to `id`'s motion (B0388).
    pub fn add_builds(&mut self, id: &str, builds: usize) {
        if let Some(motion) = self.states.get_mut(id) {
            motion.builds += builds;
        }
    }
    /// `committed` with the frame's timing: the frame callback that triggered the draw (if any:
    /// its dispatch, on the same clock as `now`, and how long before that the compositor stamped
    /// it, B0408) and the draw start. They split the worst gap for `Slide`.
    pub fn committed_timed(
        &mut self,
        id: &str,
        drawn: f64,
        now: f64,
        callback: Option<(f64, f64)>,
        draw_start: f64,
        splits: &Splits,
    ) -> Option<Slide> {
        let finished = self.states.get(id).is_some_and(|motion| {
            motion.duration <= INSTANT
                || (!motion.held && drawn >= motion.started + motion.duration)
        });
        for motion in self.states.values_mut().filter(|motion| motion.held) {
            motion.held = false;
            motion.started = now;
        }
        let motion = self.states.get_mut(id)?;
        if motion.from == motion.to || motion.frames == u32::MAX {
            return None;
        }
        if motion.frames > 0 {
            let gap = now - motion.last_frame;
            if gap > motion.max_gap {
                let start = draw_start.clamp(motion.last_frame, now);
                let dispatched = callback.map(|(c, _)| c.clamp(motion.last_frame, start));
                let wait = dispatched.map_or(0., |c| c - motion.last_frame);
                let queue = start - dispatched.unwrap_or(motion.last_frame);
                let dispatch = callback.map_or(0., |(_, sent)| sent.clamp(0., wait));
                motion.max_gap = gap;
                motion.worst = (motion.frames, [wait, queue, now - start, dispatch]);
            }
        }
        let render = now - draw_start.clamp(motion.last_frame, now);
        if render > motion.top[2].1 {
            let mut phases = *splits;
            phases.sort_by(|a, b| b.1.total_cmp(&a.1));
            let phases = std::array::from_fn(|i| phases[i]);
            if render > motion.top[0].1 {
                motion.top_split = phases;
            }
            motion.top[2] = (motion.frames, render, phases);
            motion.top.sort_by(|a, b| b.1.total_cmp(&a.1));
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
            gap_at: motion.worst.0,
            gap_wait_ms: motion.worst.1[0] * 1000.,
            gap_queue_ms: motion.worst.1[1] * 1000.,
            gap_render_ms: motion.worst.1[2] * 1000.,
            render_top: motion.top.map(|(at, s, _)| (at, s * 1000.)),
            top_split: motion.top_split.map(|(stage, s)| (stage, s * 1000.)),
            top_splits: motion
                .top
                .map(|(_, _, p)| p.map(|(stage, s)| (stage, s * 1000.))),
            main_builds: motion.builds,
            gap_dispatch_ms: motion.worst.1[3] * 1000.,
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
        let duration = duration.max(INSTANT);
        if let Some(m) = self.states.get_mut(id).filter(|m| m.to == target) {
            // A pose not yet shown takes the scene's timing over a host fallback seeded before it
            // (B0307); a running motion keeps its own.
            if m.held {
                (m.duration, m.curve) = (duration, curve);
            }
            return;
        }
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
                worst: (0, [0.; 4]),
                top: [(0, 0., [("", 0.); 4]); 3],
                top_split: [("", 0.); 4],
                builds: 0,
            },
        );
    }
    pub fn value(&self, id: &str, now: f64) -> Option<(f32, bool)> {
        let motion = self.states.get(id)?;
        let t = if motion.duration <= INSTANT {
            1.
        } else if motion.held {
            0.
        } else {
            ((now - motion.started) / motion.duration).clamp(0., 1.) as f32
        };
        let progress = motion.curve.ease(t);
        Some((motion.from + (motion.to - motion.from) * progress, t < 1.))
    }
    pub fn active(&self, now: f64) -> bool {
        self.states.values().any(|m| {
            m.duration > INSTANT && (m.held || now < m.started + m.duration) && m.from != m.to
        })
    }
    /// Drop every motion whose id `keep` rejects.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.states.retain(|id, _| keep(id));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_panel_open_duration_is_travel_over_speed_within_its_bounds() {
        assert_eq!(panel_open_duration(0.), PANEL.open_min);
        assert_eq!(panel_open_duration(100.), PANEL.open_min, "short travel");
        assert!((panel_open_duration(400.) - 0.25).abs() < 1e-9, "in range");
        assert_eq!(panel_open_duration(4000.), PANEL.open_max, "long travel");
    }
    #[test]
    fn a_held_motion_adopts_a_new_duration_and_curve_but_a_running_one_keeps_its_own() {
        let mut a = Animations::default();
        a.open_to("p", true, 1., 0.4, Curve::EaseInOut);
        a.open_to("p", true, 1.05, 0.24, Curve::EaseOut);
        a.committed("p", 1.1, 1.1);
        // Held until committed, then the adopted .24 s EaseOut plays from 1.1.
        assert_eq!(a.value("p", 1.1 + 0.24), Some((1., false)));
        let mid = a.value("p", 1.1 + 0.12).unwrap().0;
        assert!((mid - Curve::EaseOut.ease(0.5)).abs() < 1e-6);
        a.open_to("p", true, 1.2, 0.5, Curve::Spring);
        assert_eq!(
            a.value("p", 1.1 + 0.24),
            Some((1., false)),
            "running keeps .24 s"
        );
    }
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
    fn a_zero_duration_motion_is_final_and_idle_from_the_moment_it_is_targeted() {
        let mut a = Animations::default();
        a.open_to("p", true, 1., 0., Curve::EaseOut);
        assert_eq!(a.value("p", 1.), Some((1., false)));
        assert!(!a.active(1.));
        a.add_builds("p", 3);
        a.add_builds("unknown", 9);
        let slide = a
            .committed_timed("p", 1., 1.1, None, 1.05, &NO_SPLITS)
            .unwrap();
        assert!(slide.opening && slide.frames == 1 && slide.main_builds == 3);
        assert!((slide.first_frame_ms - 100.).abs() < 1e-6);
        // PANEL_SLIDE marks the slide-less open with an appended key (B0390).
        let text = slide.to_string();
        assert!(
            text.starts_with("dir=open first_frame_ms=100.0 frames=1 duration_ms=0.0 ")
                && text.ends_with(" top_splits= slide=none main_builds=3 gap_dispatch_ms=0.0"),
            "{text}"
        );
        // A host fallback seeded held with a real duration, then the scene's zero duration.
        a.open_to("q", true, 2., 0.4, Curve::EaseOut);
        assert_eq!(a.value("q", 2.), Some((0., true)));
        assert!(a.active(2.));
        a.open_to("q", true, 2., 0., Curve::EaseOut);
        assert_eq!(a.value("q", 2.), Some((1., false)));
        assert!(!a.active(2.));
        assert!(a.committed("q", 2., 2.1).is_some());
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
    fn a_slide_splits_its_worst_gap_into_wait_queue_and_render() {
        let mut a = Animations::default();
        a.open_to("p", true, 0., 1., Curve::EaseOut);
        a.committed_timed("p", 0., 0., None, 0., &NO_SPLITS);
        // Gap 0.1: wait 0.04, queue 0.02, render 0.04.
        let mut frame1 = NO_SPLITS;
        frame1[0] = ("swap", 0.01);
        a.committed_timed("p", 0.1, 0.1, Some((0.04, 0.)), 0.06, &frame1);
        // Smaller gap 0.05, no callback: queue 0.01, render 0.04.
        a.committed_timed("p", 0.15, 0.15, None, 0.11, &NO_SPLITS);
        let mut splits = NO_SPLITS;
        splits[..3].copy_from_slice(&[("prepare", 0.01), ("gl_draw", 0.03), ("swap", 0.002)]);
        let slide = a
            .committed_timed("p", 1., 1.15, Some((0.2, 0.03)), 1.1, &splits)
            .unwrap();
        assert_eq!((slide.gap_at, slide.frames), (3, 4));
        assert!((slide.max_gap_ms - 1000.).abs() < 1e-6);
        let sum = slide.gap_wait_ms + slide.gap_queue_ms + slide.gap_render_ms;
        assert!((sum - slide.max_gap_ms).abs() < 1e-6);
        assert!((slide.gap_wait_ms - 50.).abs() < 1e-6);
        let text = slide.to_string();
        assert!(text.starts_with("dir=open first_frame_ms=0.0 frames=4 duration_ms="));
        assert!(text.contains(" max_gap_ms=1000.0 gap_at=3 gap_wait_ms=50.0"));
        // B0408: 30 ms of the 50 ms wait came after the compositor stamped the callback.
        assert!((slide.gap_dispatch_ms - 30.).abs() < 1e-6);
        // The three costliest renders by commit index, costliest first (the last render is 50 ms).
        assert!(text.contains(" render_top=3:50.0,"), "{text}");
        // The phases of that costliest render, largest first.
        assert!(
            text.contains(" top_split=gl_draw:30.0,prepare:10.0,swap:2.0 top_splits="),
            "{text}"
        );
        // The same phases for each costliest render that has any, by commit index (B0380).
        assert!(
            text.ends_with(
                " top_splits=3:gl_draw:30.0/prepare:10.0/swap:2.0;1:swap:10.0 main_builds=0 \
                 gap_dispatch_ms=30.0"
            ),
            "{text}"
        );
        assert_eq!(text.matches(':').count(), 12, "{text}");
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
    fn every_curve_has_exact_endpoints_and_the_expected_shape() {
        for (name, curve) in Curve::NAMES {
            assert_eq!(Curve::parse(name), Some(curve));
            assert_eq!(curve.name(), name);
            assert_eq!(curve.ease(0.), 0., "{name}");
            assert!((curve.ease(1.) - 1.).abs() < 1e-6, "{name}");
            assert_eq!(curve.ease(-1.), 0.);
            assert!((curve.ease(2.) - 1.).abs() < 1e-6);
            let mut last = 0.;
            let mut peak = 0_f32;
            for i in 1..=200 {
                let v = curve.ease(i as f32 / 200.);
                peak = peak.max(v);
                if curve != Curve::OutBack {
                    assert!(v >= last - 1e-6, "{name} not monotonic at {i}");
                    assert!((0. ..=1. + 1e-6).contains(&v), "{name} leaves 0..1 at {i}");
                }
                last = v;
            }
            if curve == Curve::OutBack {
                assert!(peak > 1.05 && peak < 1.11, "out-back overshoot {peak}");
            }
        }
        assert_eq!(Curve::parse("ease"), Some(Curve::EaseOut));
        assert_eq!(Curve::parse("ease-in-out"), Some(Curve::EaseInOut));
        assert_eq!(Curve::parse("ease-out"), Some(Curve::EaseOut));
        assert_eq!(Curve::parse("ease-in"), Some(Curve::InCubic));
        assert_eq!(duration("pulse"), Some(1.6));
        assert_eq!(Curve::parse("bounce"), None);
        assert!((Curve::EaseOut.ease(0.25) - 0.578125).abs() < 1e-6);
        assert!((Curve::OutQuint.ease(0.5) - 0.96875).abs() < 1e-6);
        assert!((Curve::InCubic.ease(0.5) - 0.125).abs() < 1e-6);
        // Spring settles: within 1 % of the target over the last fifth and never over 1.
        for i in 160..=200 {
            assert!((Curve::Spring.ease(i as f32 / 200.) - 1.).abs() < 0.01);
        }
        assert_eq!(duration("open"), Some(0.22));
        assert_eq!(duration("hover"), Some(0.09));
        assert_eq!(duration("bogus"), None);
        assert!(clock_ns() <= clock_ns());
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

// SPDX-License-Identifier: MIT
//! B0313: GPU crossfade of a wallpaper change at display rate.
//!
//! The wallpaper surface stays on the SHM path. For the length of one change it borrows the
//! shared EGL transport: both images are uploaded first, a held first pose is committed (that
//! commit starts the clock, B0170), every frame callback draws the next blend, and the final
//! SHM buffer is attached before the EGL surface and transport are released again. While the host
//! asks for warm mode (B0317) a fade ends at rest instead: the run (transport, EGL surface, canvas
//! and the shown texture) stays, and the next change starts from it.
// Without a GL renderer there is no fade: its state and helpers stay unused.
#![cfg_attr(not(feature = "native-gl"), allow(dead_code, unused_imports))]
use super::*;
use crate::runtime::{WallpaperBufferTarget, attach_wallpaper_buffer};
use bead_core::animation::Curve;

/// How a new wallpaper replaces the shown one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WallpaperFadeKind {
    Fade,
    /// A soft-edged disc growing from `origin` (0..1 of the output, from the top left).
    Grow {
        origin: (f32, f32),
    },
}
/// Host choice of kind, duration and curve for wallpaper changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallpaperFade {
    pub kind: WallpaperFadeKind,
    pub duration_ms: u32,
    pub curve: Curve,
}
impl Default for WallpaperFade {
    fn default() -> Self {
        Self {
            kind: WallpaperFadeKind::Fade,
            duration_ms: 700,
            curve: Curve::EaseInOut,
        }
    }
}

type PaletteMap = std::collections::BTreeMap<String, [u8; 4]>;
/// The palette follows the tween at about one step per 60 Hz frame, within these bounds (B0327).
pub(crate) const PALETTE_STEPS_MIN: usize = 10;
pub(crate) const PALETTE_STEPS_MAX: usize = 60;
/// Steps for a tween of `duration` seconds: 500 ms gives 30.
pub(crate) fn palette_steps(duration: f64) -> usize {
    ((duration * 60.).round() as usize).clamp(PALETTE_STEPS_MIN, PALETTE_STEPS_MAX)
}

fn to_linear(c: u8) -> f32 {
    (f32::from(c) / 255.).powf(2.2)
}
fn from_linear(c: f32) -> u8 {
    (c.clamp(0., 1.).powf(1. / 2.2) * 255.).round() as u8
}
/// Mix in linear light (gamma 2.2); alpha mixes directly. A role missing from `from` is taken as is.
pub(crate) fn mix_palette(from: &PaletteMap, to: &PaletteMap, t: f32) -> PaletteMap {
    if t >= 1. {
        return to.clone();
    }
    to.iter()
        .map(|(role, new)| {
            let old = from.get(role).unwrap_or(new);
            let mix = |a: u8, b: u8| from_linear(to_linear(a) + (to_linear(b) - to_linear(a)) * t);
            let alpha = f32::from(old[3]) + (f32::from(new[3]) - f32::from(old[3])) * t;
            (
                role.clone(),
                [
                    mix(old[0], new[0]),
                    mix(old[1], new[1]),
                    mix(old[2], new[2]),
                    alpha.round() as u8,
                ],
            )
        })
        .collect()
}
/// (seconds after the clock starts, mix) per step: each step shows the colour the wallpaper
/// blend has reached at that time, and the last one is exactly the new palette.
pub(crate) fn palette_schedule(duration: f64, curve: Curve) -> Vec<(f64, f32)> {
    let n = palette_steps(duration);
    (1..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            (duration * f64::from(t), curve.ease(t))
        })
        .collect()
}
#[derive(Debug, PartialEq)]
pub(crate) enum TweenRoute {
    Fence,
    /// Another output of the change that is already tweening.
    Joins,
    Starts,
    /// B0341: palette-only frames only: nothing changed, adopt with no tween.
    Unchanged,
}
/// B0333: the panel takes the tween's final step always, the intermediate ones only while open
/// and without `palette_tween = false`; a closed retained panel is re-staged once, at the end.
pub(crate) fn panel_takes_step(last: bool, open: bool, at_rest: bool) -> bool {
    last || (open && !at_rest)
}
fn skipped_suffix(n: u32) -> String {
    if n == 0 {
        String::new()
    } else {
        format!(" panel_steps_skipped={n}")
    }
}
pub(crate) fn tween_route(
    possible: bool,
    foreign_pending: bool,
    tween_to_key: bool,
    differs: bool,
) -> TweenRoute {
    match (possible && !foreign_pending, tween_to_key, differs) {
        (false, ..) => TweenRoute::Fence,
        (true, true, _) => TweenRoute::Joins,
        (true, false, true) => TweenRoute::Starts,
        (true, false, false) => TweenRoute::Fence,
    }
}
/// B0341: `tween_route` for a palette-only frame; the one Fence caused by nothing changing
/// (`same`) is `Unchanged`, every other Fence stays one.
pub(crate) fn palette_only_route(
    possible: bool,
    foreign_pending: bool,
    tween_to_key: bool,
    differs: bool,
    same: bool,
) -> TweenRoute {
    match tween_route(possible, foreign_pending, tween_to_key, differs) {
        TweenRoute::Fence if possible && !foreign_pending && same => TweenRoute::Unchanged,
        route => route,
    }
}
/// B0341: a palette-only frame is adopted when the output shows its pixels (a resting run or no
/// run) or a running fade is heading to them (its first frame already committed). Any run with an
/// upload queued is rejected: the queued frame would later overwrite the adopted revision. `run` is (resting, upload queued, commit pending).
pub(crate) fn adopts_palette_only(
    revision: u64,
    same_size: bool,
    transition: bool,
    run: Option<(bool, bool, bool)>,
) -> bool {
    revision != 0
        && same_size
        && !transition
        && run.is_none_or(|(resting, upload, commit)| !upload && (resting || !commit))
}
pub(crate) struct PaletteTween {
    from: PaletteMap,
    pub(crate) to: PaletteMap,
    steps: Vec<(f64, f32)>,
    next: usize,
    generation: u64,
    /// Clock times (seconds): the tween's start and the last fade frame that served it.
    started: f64,
    last_frame: f64,
    /// The fades hold their first pose: the tween starts with their clock (B0316).
    waiting: bool,
}
/// Fade frames arrive every refresh; a gap this long means they stopped (hidden surface).
const FRAMES_STOPPED_AFTER: f64 = 0.05;
impl PaletteTween {
    pub(crate) fn new(
        from: PaletteMap,
        to: PaletteMap,
        spec: WallpaperFade,
        started: f64,
        generation: u64,
    ) -> Self {
        Self {
            from,
            to,
            steps: palette_schedule(f64::from(spec.duration_ms) / 1000., spec.curve),
            next: 0,
            generation,
            started,
            last_frame: f64::NEG_INFINITY,
            waiting: false,
        }
    }
    /// The palette to show at clock `now` and the wait until the following step (`None`: it was the
    /// last). A step that is overdue is skipped for the newest due one: it shows the colour the
    /// wallpaper has reached. `None` when no step is due.
    fn advance(&mut self, now: f64) -> Option<(PaletteMap, Option<f64>)> {
        if self.waiting {
            return None;
        }
        let elapsed = now - self.started;
        let due = self.next
            + self.steps[self.next..]
                .iter()
                .rposition(|(at, _)| *at <= elapsed)?;
        self.next = due + 1;
        let wait = self
            .steps
            .get(self.next)
            .map(|(at, _)| (at - elapsed).max(0.));
        Some((mix_palette(&self.from, &self.to, self.steps[due].1), wait))
    }
    /// The fades' clock started (or no fade is left to wait for): the first step is due a step's
    /// time from now.
    fn start_clock(&mut self, now: f64) {
        (self.started, self.waiting) = (now, false);
    }
    /// Fade frames are flowing: they apply the steps, the timer only backs them up.
    fn frames_flow(&self, now: f64) -> bool {
        now - self.last_frame < FRAMES_STOPPED_AFTER
    }
    /// Seconds until the next step is due, at least a refresh while frames serve it.
    fn retry_after(&self, now: f64) -> f64 {
        (self.steps[self.next].0 + self.started - now).max(0.01)
    }
}

/// B0315: what the main thread did between fade frames. For the largest gap between two commits
/// it keeps the thread CPU time spent outside the fade frames (bar and panel repaints, palette
/// steps, any other callback): near the gap means our main thread, near 0 means waiting
/// (compositor, GPU or a blocking call). Two clock reads per frame, no allocation.
#[derive(Default)]
pub(crate) struct GapProbe {
    last: Option<(f64, CpuMark)>,
    /// (gap seconds, other, peer-frame and palette-step main-thread ms) of the largest gap so far.
    worst: (f64, f64, f64, f64),
    /// B0347: the three costliest frames' main-thread CPU (frame index, ms), costliest first.
    top_cpu: [(u32, f64); 3],
    frames: u32,
}
/// B0319: main-thread CPU (ns) at one instant: the thread total and the running sums spent inside
/// fade frames (all outputs) and inside palette steps (host callback included).
#[derive(Clone, Copy, Default)]
pub(crate) struct CpuMark {
    pub(crate) cpu: u64,
    pub(crate) frames: u64,
    pub(crate) steps: u64,
}
impl GapProbe {
    /// A frame committed at `now` (seconds), drawn from the mark `start` to the mark `end`; the
    /// frame sum of `end` includes this frame, so the peers' share of a gap excludes it.
    pub(crate) fn committed(&mut self, now: f64, start: CpuMark, end: CpuMark) {
        if let Some((wall, last)) = self.last {
            let gap = now - wall;
            if gap > self.worst.0 {
                let ms = |now: u64, then: u64| now.saturating_sub(then) as f64 / 1e6;
                self.worst = (
                    gap,
                    ms(start.cpu, last.cpu),
                    ms(start.frames, last.frames),
                    ms(start.steps, last.steps),
                );
            }
        }
        let cpu = end.cpu.saturating_sub(start.cpu) as f64 / 1e6;
        if cpu > self.top_cpu[2].1 {
            self.top_cpu[2] = (self.frames, cpu);
            self.top_cpu.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        self.frames += 1;
        self.last = Some((now, end));
    }
    /// `render_cpu_top` text: the frames' CPU beside `render_top`'s wall time.
    pub(crate) fn cpu_top(&self) -> String {
        let top: Vec<String> = self
            .top_cpu
            .iter()
            .filter(|(_, ms)| *ms > 0.)
            .map(|(at, ms)| format!("{at}:{ms:.1}"))
            .collect();
        top.join(",")
    }
    pub(crate) fn other_ms(&self) -> f64 {
        self.worst.1
    }
    /// Partner outputs' fade-frame CPU inside the largest gap.
    pub(crate) fn peer_ms(&self) -> f64 {
        self.worst.2
    }
    /// Palette-step CPU inside the largest gap.
    pub(crate) fn step_ms(&self) -> f64 {
        self.worst.3
    }
}

/// B0316: main-thread wall milliseconds of each phase of a fade start (the first output of a
/// change pays them all; the transport phase is 0 when another output already made it).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct SetupTimes {
    /// The shown image read from the attached buffer (or taken from the provider).
    pub(crate) read: f64,
    /// EGL display and context, made once per change and shared by its outputs.
    pub(crate) egl: f64,
    pub(crate) surface: f64,
    /// Renderer and canvas, then the two texture uploads.
    pub(crate) canvas: f64,
    pub(crate) upload: f64,
    /// The held first pose: draw and present.
    pub(crate) first: f64,
}
/// B0317: how the fade started. `warm`: from a resident run; `start_ms`: main-thread wall of the
/// start (or latest retarget), including the hand-over to the upload thread; `upload_wait_ms`:
/// from the frame being accepted to its texture being ready (0: uploaded on the main thread).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct StartInfo {
    pub(crate) warm: bool,
    pub(crate) start_ms: f64,
    pub(crate) upload_wait_ms: f64,
}
fn ms_since(began: std::time::Instant) -> f64 {
    began.elapsed().as_secs_f64() * 1e3
}

/// One journal line per finished fade, in the PANEL_SLIDE style: what the user saw.
#[allow(clippy::too_many_arguments)]
pub(crate) fn transition_marker(
    output: &str,
    kind: WallpaperFadeKind,
    slide: &bead_core::animation::Slide,
    palette_steps: u32,
    gap_other_ms: f64,
    render_cpu_top: &str,
    setup: &SetupTimes,
    retarget_ms: f64,
    clock_s: f64,
    (gap_peer_ms, gap_step_ms, step_max_ms): (f64, f64, f64),
    start: &StartInfo,
) -> String {
    let render_top: Vec<String> = slide
        .render_top
        .iter()
        .filter(|(_, ms)| *ms > 0.)
        .map(|(at, ms)| format!("{at}:{ms:.1}"))
        .collect();
    format!(
        "WALLPAPER_TRANSITION output={output} kind={} frames={} duration_ms={:.1} \
         first_frame_ms={:.1} max_gap_ms={:.1} gap_at={} render_top={} render_cpu_top={render_cpu_top} palette_steps={palette_steps} \
         gap_other_ms={gap_other_ms:.1} setup=read:{:.1},egl:{:.1},surface:{:.1},canvas:{:.1},\
         upload:{:.1},first:{:.1} retarget_ms={retarget_ms:.1} \
         clock_s={clock_s:.3} gap_peer_ms={gap_peer_ms:.1} gap_step_ms={gap_step_ms:.1} \
         step_max_ms={step_max_ms:.1} warm={} start_ms={:.1} upload_wait_ms={:.1}",
        match kind {
            WallpaperFadeKind::Fade => "fade",
            WallpaperFadeKind::Grow { .. } => "grow",
        },
        slide.frames,
        slide.duration_ms,
        slide.first_frame_ms,
        slide.max_gap_ms,
        slide.gap_at,
        render_top.join(","),
        setup.read,
        setup.egl,
        setup.surface,
        setup.canvas,
        setup.upload,
        setup.first,
        u8::from(start.warm),
        start.start_ms,
        start.upload_wait_ms,
    )
}

/// One journal line per change of warm mode (and when a release that waited for a fade is done),
/// so a resident run that never goes is visible. `bytes` estimates the textures held.
pub(crate) fn warm_marker(on: bool, outputs: usize, textures: usize, bytes: u64) -> String {
    format!(
        "WALLPAPER_WARM state={} outputs={outputs} textures={textures} bytes={bytes}",
        if on { "on" } else { "off" }
    )
}

/// Reduced motion, a failed GL setup or no spec is a cut.
pub(crate) fn plan_fade(
    spec: Option<WallpaperFade>,
    reduced_motion: bool,
    gl_failed: bool,
) -> Option<WallpaperFade> {
    spec.filter(|_| !reduced_motion && !gl_failed)
}
/// The shared transport goes once nothing fades, nothing else draws with GL and the host is CPU.
pub(crate) fn transport_releasable(fading: bool, other_gl: bool, cpu_host: bool) -> bool {
    !fading && !other_gl && cpu_host
}

/// B0317: what a fade that drew its last frame does with its GL run.
#[derive(Debug, PartialEq)]
pub(crate) enum FadeEnd {
    /// Warm: the run stays at rest, showing the final frame, without the SHM attach.
    Rest,
    /// Not warm (or GL failed): attach the final SHM buffer and release the run.
    Release,
}
pub(crate) fn fade_end(warm: bool, gl_failed: bool, upload_pending: bool) -> FadeEnd {
    if !gl_failed && (upload_pending || warm) {
        FadeEnd::Rest
    } else {
        FadeEnd::Release
    }
}
/// A run at rest is released when warm mode ends or GL failed, unless a frame still waits for its
/// upload (that fade runs and ends through `fade_end`); a running fade releases when it ends.
pub(crate) fn release_idle(
    warm: bool,
    gl_failed: bool,
    resting: bool,
    upload_pending: bool,
) -> bool {
    (!warm || gl_failed) && resting && !upload_pending
}
/// B0317: GL surfaces on the shared transport: panels, bars and companions, or a wallpaper fade
/// run (also one at rest). Every owner that would shut the transport down asks this.
pub(crate) fn gl_surfaces_present(other_surfaces: bool, wallpaper_runs: bool) -> bool {
    other_surfaces || wallpaper_runs
}
/// Warm mode may use the upload thread.
pub(crate) fn worker_wanted(warm: bool, gl_failed: bool, worker_failed: bool) -> bool {
    warm && !gl_failed && !worker_failed
}
/// An upload answer belongs to the run's newest waiting frame (last wins: older ones are deleted).
pub(crate) fn upload_current(waiting: Option<u64>, token: u64) -> bool {
    waiting == Some(token)
}
/// B0325: an output is primed (its resting run built from the shown image) while warm mode may
/// use the upload thread, it has no run yet and shows an image the SHM buffer still holds.
pub(crate) fn prime_wanted(
    warm: bool,
    gl_failed: bool,
    worker_failed: bool,
    has_run: bool,
    shown: bool,
) -> bool {
    worker_wanted(warm, gl_failed, worker_failed) && !has_run && shown
}
/// B0325: a primed run that never presented holds no CPU pixels: the SHM buffer it replaced is
/// still attached, so releasing it attaches nothing.
pub(crate) fn final_attach_needed(target: &[u8]) -> bool {
    !target.is_empty()
}
/// B0325: one line per primed output; `ms` is the main-thread time, `upload_wait_ms` the wait for
/// the upload thread.
pub(crate) fn prime_marker(output: &str, ms: f64, upload_wait_ms: f64) -> String {
    format!("WALLPAPER_WARM_PRIME output={output} ms={ms:.1} upload_wait_ms={upload_wait_ms:.1}")
}
/// B0325: the shown image of an output without a run is on the upload thread; its resting run is
/// built from the answer.
pub(crate) struct Prime {
    name: String,
    token: u64,
    queued: std::time::Instant,
    /// Main-thread ms spent so far.
    ms: f64,
}
/// Warm mode's state: the host's switch, and the upload thread while it is wanted.
#[derive(Default)]
pub(crate) struct Warm {
    pub(crate) on: bool,
    /// The upload thread failed: uploads stay on the main thread until warm mode changes.
    failed: bool,
    /// Warm mode ended and a fade still held its run: report when the last one is released.
    releasing: bool,
    next_token: u64,
    /// B0325: the one prime in flight (outputs are primed one at a time).
    prime: Option<Prime>,
    #[cfg(feature = "native-gl")]
    upload: Option<UploadThread>,
}
impl Warm {
    /// A frame for `output` supersedes its prime: the answer then finds no prime and its texture
    /// is deleted, and the frame takes the cold start.
    fn cancel_prime(&mut self, output: &str) -> bool {
        let hit = self
            .prime
            .as_ref()
            .is_some_and(|prime| prime.name == output);
        if hit {
            self.prime = None;
        }
        hit
    }
    /// The prime an upload answer belongs to; `None` once it was cancelled or released.
    fn take_prime(&mut self, token: u64) -> Option<Prime> {
        if self
            .prime
            .as_ref()
            .is_some_and(|prime| prime.token == token)
        {
            self.prime.take()
        } else {
            None
        }
    }
}
#[cfg(feature = "native-gl")]
struct UploadThread {
    uploader: bead_render::gl::Uploader,
    source: calloop::RegistrationToken,
}
/// A frame whose pixels the upload thread is turning into a texture; its fade starts on the answer.
#[cfg(feature = "native-gl")]
pub(super) struct PendingUpload {
    token: u64,
    spec: WallpaperFade,
    tween: Option<PaletteMap>,
    /// Everything but the pixels, which travel with the job.
    frame: WallpaperFrame,
    arrived: std::time::Instant,
    /// Main-thread ms spent handing the frame over.
    handover_ms: f64,
}
/// What a fade start carries besides the frame.
#[cfg(feature = "native-gl")]
struct Launch {
    began: std::time::Instant,
    /// The adopted texture of the frame's pixels, when the upload thread made one.
    texture: Option<u32>,
    tween: Option<PaletteMap>,
    /// Main-thread ms already spent on this start, and the wait for the upload thread.
    carried_ms: f64,
    upload_wait_ms: f64,
}

/// What a new run's fader starts from: both images, or (B0325) the shown image's texture.
#[cfg(feature = "native-gl")]
enum Seed<'a> {
    Fade { from: &'a [u8], to: &'a [u8] },
    Rest(u32),
}

/// B0316: the first outputs of a change show their first pose (the old image) at most this long
/// while another output of it is still being set up: one output's setup is 55-65 ms on the
/// owner's laptop, so two setups plus slack fit and a slow or missing output holds nothing for
/// longer than a few frames of a 700 ms fade.
pub(crate) const HOLD_CAP: f64 = 0.15;
/// Whether a fade at its held first pose keeps waiting for the others to start with it.
pub(crate) fn hold_first_pose(partner_coming: bool, waited: f64) -> bool {
    partner_coming && waited < HOLD_CAP
}
/// Another output with a shown image has a wallpaper request in flight: it fades with this change
/// (a hotplugged output has no image to fade from, and the output itself never counts).
pub(crate) fn partner_coming<'a>(
    own: &str,
    jobs: &std::collections::BTreeSet<String>,
    surfaces: impl IntoIterator<Item = (&'a str, bool)>,
) -> bool {
    surfaces
        .into_iter()
        .any(|(name, shown)| shown && name != own && jobs.contains(name))
}
/// Fades that started on one clock end together: a fade that drew its last frame waits for the
/// others of its clock, so its final-buffer attach never lands in their last frames.
pub(crate) fn group_done(
    runs: impl IntoIterator<Item = (Option<f64>, bool)>,
    clock: Option<f64>,
) -> bool {
    runs.into_iter()
        .filter(|(at, _)| *at == clock)
        .all(|(_, done)| done)
}

#[cfg(feature = "native-gl")]
pub(super) struct FadeRun {
    // Drop order: the textures go before the EGL window, which goes before the wl_surface.
    fader: bead_render::gl::WallpaperFader,
    gl: bead_render::gl::Surface,
    size: (u32, u32),
    spec: WallpaperFade,
    target: Vec<u8>,
    commit: Option<(u64, std::sync::Arc<dyn std::any::Any + Send + Sync>)>,
    palette: bead_render::Palette,
    generation: u64,
    /// Palette tween steps applied while this fade ran, for the marker.
    palette_steps: u32,
    gaps: GapProbe,
    /// B0316 diagnostics: the start's phases and the costliest retarget of this run.
    setup: SetupTimes,
    retarget_ms: f64,
    /// B0319: the costliest palette step (wall ms, host callback included) while this fade ran.
    step_max_ms: f64,
    /// B0316: when the clock of this fade started (`None` while the first pose is held), when the
    /// hold began, and whether the last frame was drawn (the finish waits for the group).
    clock: Option<f64>,
    held_since: f64,
    done: bool,
    /// B0317: the fade ended warm: only the shown texture is left, nothing is drawn until the
    /// next change starts from here.
    resting: bool,
    /// B0317: the newest frame waiting for the upload thread; the run keeps its image meanwhile.
    upload: Option<PendingUpload>,
    /// B0317 marker fields of the current fade.
    start: StartInfo,
}

#[cfg(feature = "native-gl")]
impl App {
    pub(super) fn fade_wanted(&self) -> bool {
        plan_fade(
            self.options.wallpaper_fade,
            bead_core::animation::reduced_motion(),
            self.wallpaper_gl_failed,
        )
        .is_some()
    }

    fn palette_tween_route(
        &self,
        key: &PaletteMap,
        output: &str,
        fade: bool,
        palette_only: bool,
    ) -> TweenRoute {
        let tween_to_key = self.palette_tween.as_ref().is_some_and(|t| t.to == *key);
        let differs = self
            .wallpaper_palette
            .as_ref()
            .is_some_and(|current| current != key);
        // Frames of an older change would be pushed back onto the fence by this one.
        let foreign_pending = self
            .wallpaper_pending_frames
            .iter()
            .any(|frame| !crate::runtime::frame_matches_palette(frame, key));
        palette_only_route(
            fade && self.wallpaper_palette_outputs.is_none()
                && self.wallpaper_surfaces.iter().any(|s| s.name == output),
            foreign_pending,
            tween_to_key,
            differs,
            palette_only && self.wallpaper_palette.as_ref() == Some(key),
        )
    }

    /// Whether a staged frame waits for its confirmation to tween instead of building the fence.
    pub(super) fn staged_waits_for_tween(&self, key: &PaletteMap, output: &str) -> bool {
        self.palette_tween_route(key, output, self.fade_wanted(), false) != TweenRoute::Fence
    }

    /// Whether this change swaps the palette with a fade instead of the palette fence.
    pub(super) fn install_with_palette_tween(&mut self, key: &PaletteMap, output: &str) -> bool {
        let route = self.palette_tween_route(key, output, self.fade_wanted(), false);
        if route == TweenRoute::Starts {
            self.pending_palette_target = Some(key.clone());
        }
        route != TweenRoute::Fence
    }

    /// B0327: a frame the provider calls palette-only keeps the shown pixels and eases the palette
    /// on the timer (no fade run exists to drive it). The revision is `uploaded` and `committed`
    /// as a cut of identical pixels would. B0341: a fade running toward these pixels (first frame
    /// committed, no upload queued) is adopted too, untouched but for its palette; an unchanged
    /// palette adopts with no tween. False: not palette-only, or a cut (reduced motion, no fade,
    /// an older change waiting, the palette fence): the caller takes today's path.
    pub(super) fn install_palette_only(
        &mut self,
        frame: &WallpaperFrame,
        key: &PaletteMap,
    ) -> bool {
        let Some(provider) = self.options.wallpaper.clone() else {
            return false;
        };
        let output = frame.output.as_str();
        let Some(index) = self
            .wallpaper_surfaces
            .iter()
            .position(|s| s.name == output)
        else {
            return false;
        };
        let surface = &self.wallpaper_surfaces[index];
        let shown = adopts_palette_only(
            surface.revision,
            (surface.width, surface.height) == (frame.width, frame.height),
            surface.transition.is_some(),
            surface
                .fade
                .as_ref()
                .map(|run| (run.resting, run.upload.is_some(), run.commit.is_some())),
        );
        if !shown || !provider.palette_only(output, frame.revision) {
            return false;
        }
        let spec = plan_fade(
            provider.fade(output).or(self.options.wallpaper_fade),
            bead_core::animation::reduced_motion(),
            self.wallpaper_gl_failed,
        );
        let route = self.palette_tween_route(key, output, spec.is_some(), true);
        let Some(spec) = spec.filter(|_| route != TweenRoute::Fence) else {
            return false;
        };
        let surface = &mut self.wallpaper_surfaces[index];
        surface.palette = Some(frame.palette.clone());
        surface.revision = frame.revision;
        if let Some(run) = surface.fade.as_mut() {
            run.palette = frame.palette.clone();
        }
        surface.staged = surface
            .staged
            .take()
            .filter(|staged| staged.revision != frame.revision);
        provider.uploaded(output, frame.revision, &frame.commit_token);
        self.wallpaper_committed(output, frame.revision, &frame.commit_token, false);
        if route == TweenRoute::Starts {
            self.start_palette_tween(key.clone(), spec);
        }
        true
    }

    /// After install: a fade starts the tween itself; a cut applies the new palette at once.
    pub(super) fn settle_palette_tween_start(&mut self, output: &str, revision: u64) {
        let Some(target) = self.pending_palette_target.take() else {
            return;
        };
        let installed = self
            .wallpaper_surfaces
            .iter()
            .any(|s| s.name == output && s.revision == revision);
        if installed {
            self.apply_palette_step(target, true);
        }
    }

    fn start_palette_tween(&mut self, to: PaletteMap, spec: WallpaperFade) {
        let from = self.wallpaper_palette.clone().unwrap_or_else(|| to.clone());
        self.palette_tween_generation += 1;
        let generation = self.palette_tween_generation;
        let now = self.started.elapsed().as_secs_f64();
        let mut tween = PaletteTween::new(from, to, spec, now, generation);
        tween.waiting = self
            .wallpaper_surfaces
            .iter()
            .filter_map(|s| s.fade.as_ref())
            .any(|run| run.clock.is_none());
        let first = tween.steps[0].0;
        self.palette_tween = Some(tween);
        self.schedule_palette_step(first, generation);
    }

    fn schedule_palette_step(&self, after: f64, generation: u64) {
        let _ = self.loop_handle.clone().insert_source(
            Timer::from_duration(Duration::from_secs_f64(after)),
            move |_, _, app| match app.palette_tween_timer(generation) {
                Some(wait) => TimeoutAction::ToDuration(Duration::from_secs_f64(wait)),
                None => TimeoutAction::Drop,
            },
        );
    }

    /// The timer drives the steps when no fade frame does (a cut on another output, a hidden
    /// surface); while frames flow it only waits. The wait until it looks again, if any.
    fn palette_tween_timer(&mut self, generation: u64) -> Option<f64> {
        let now = self.started.elapsed().as_secs_f64();
        let held = self
            .wallpaper_surfaces
            .iter()
            .filter_map(|s| s.fade.as_ref())
            .any(|run| run.clock.is_none());
        let tween = self
            .palette_tween
            .as_mut()
            .filter(|t| t.generation == generation)?;
        if tween.waiting && !held {
            // The held fade ended without starting a clock (removed output, GL failure).
            tween.start_clock(now);
        }
        let Some((palette, wait)) = (!tween.frames_flow(now))
            .then(|| tween.advance(now))
            .flatten()
        else {
            return Some(tween.retry_after(now));
        };
        self.apply_tween_step(palette, wait.is_none());
        wait
    }

    /// B0315: a due step runs right after a fade frame was submitted, so its repaint uses the
    /// slack of the frame budget instead of competing with the next frame. The last frame of the
    /// last fade completes the tween.
    pub(super) fn palette_step_after_frame(&mut self) {
        let now = self.started.elapsed().as_secs_f64();
        let fading = self
            .wallpaper_surfaces
            .iter()
            .any(|s| s.fade.as_ref().is_some_and(|run| !run.resting));
        let Some(tween) = self.palette_tween.as_mut() else {
            return;
        };
        tween.last_frame = now;
        if let Some((palette, wait)) = tween.advance(if fading { now } else { f64::INFINITY }) {
            self.apply_tween_step(palette, wait.is_none());
        }
    }

    fn apply_tween_step(&mut self, palette: PaletteMap, last: bool) {
        if last {
            self.palette_tween = None;
        }
        self.palette_tween_until = self.started.elapsed().as_secs_f64() + 1.;
        for run in self
            .wallpaper_surfaces
            .iter_mut()
            .filter_map(|s| s.fade.as_mut())
            .filter(|run| !run.resting)
        {
            run.palette_steps += 1;
        }
        let began = std::time::Instant::now();
        let cpu = bead_core::profile::thread_cpu_ns();
        self.apply_palette_step(palette, last);
        self.palette_step_cpu_ns += bead_core::profile::thread_cpu_ns().saturating_sub(cpu);
        let ms = ms_since(began);
        for run in self
            .wallpaper_surfaces
            .iter_mut()
            .filter_map(|s| s.fade.as_mut())
            .filter(|run| !run.resting)
        {
            run.step_max_ms = run.step_max_ms.max(ms);
        }
    }

    fn cpu_mark(&self) -> CpuMark {
        CpuMark {
            cpu: bead_core::profile::thread_cpu_ns(),
            frames: self.fade_frame_cpu_ns,
            steps: self.palette_step_cpu_ns,
        }
    }

    /// One palette step through the same recipients as a fenced change, without the fence.
    fn apply_palette_step(&mut self, palette: PaletteMap, last: bool) {
        let _span = bead_core::profile::Span::new("wallpaper_palette_step");
        let at_rest = self.companion.as_ref().is_some_and(|c| c.0.palette_at_rest);
        let notify = panel_takes_step(last, self.panel_target, at_rest);
        self.panel_steps_skipped += u32::from(!notify && self.panel_runtime.is_some());
        self.apply_wallpaper_palette(palette.clone(), notify);
        if self.options.primary_factory.is_some() {
            let (participants, active) = self.palette_participants();
            for (id, epoch) in participants {
                if active.as_ref() == Some(&id) {
                    self.ui(UiEvent::WallpaperPalette(palette.clone()));
                } else {
                    self.ui(UiEvent::Output(
                        epoch,
                        id,
                        Box::new(UiEvent::WallpaperPalette(palette.clone())),
                    ));
                }
            }
        } else if let Some(Ok(Some(scene))) = self.runtime.as_mut().map(|runtime| {
            runtime.wallpaper_palette(bead_render::Palette(std::sync::Arc::new(
                palette.into_iter().collect(),
            )))
        }) && self
            .runtime
            .as_mut()
            .is_some_and(|runtime| runtime.commit())
        {
            self.scene = scene;
            let qh = self.qh.clone();
            for index in 0..self.surfaces.len() {
                self.draw(index, &qh);
            }
        }
    }

    /// Reads the shown image from the attached SHM buffer (still mapped until the idle release).
    fn shown_wallpaper_rgba(&mut self, index: usize) -> Option<Vec<u8>> {
        let surface = &mut self.wallpaper_surfaces[index];
        let slot = surface.buffers.get(surface.attached?)?.slot();
        let len = surface.width as usize * surface.height as usize * 4;
        let bytes = surface.pool.raw_data_mut(&slot).get(..len)?;
        let mut rgba = bytes.to_vec();
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p.swap(0, 2));
        Some(rgba)
    }

    /// Start (or retarget) the crossfade to `frame`. The frame comes back when this is a cut.
    pub(super) fn start_wallpaper_fade(
        &mut self,
        index: usize,
        mut frame: WallpaperFrame,
    ) -> Result<(), WallpaperFrame> {
        let name = frame.output.clone();
        // A prime still in flight for this output loses: this frame starts cold and the
        // prime's texture is deleted on arrival.
        if self.warm.cancel_prime(&name) {
            self.schedule_prime();
        }
        let provider_spec = self.options.wallpaper.as_ref().and_then(|p| p.fade(&name));
        let Some(spec) = plan_fade(
            provider_spec.or(self.options.wallpaper_fade),
            bead_core::animation::reduced_motion(),
            self.wallpaper_gl_failed,
        ) else {
            // A running fade ends at its old target; the cut then replaces it.
            self.release_wallpaper_run(index);
            return Err(frame);
        };
        let began = std::time::Instant::now();
        if self.wallpaper_surfaces[index].fade.is_some() {
            // A run (fading or at rest) takes the new image, uploaded by the worker when warm.
            if self.upload_on_worker() {
                match self.queue_upload(index, frame, spec) {
                    None => return Ok(()),
                    Some(back) => frame = back,
                }
            }
            let launch = Launch {
                began,
                texture: None,
                tween: self.pending_palette_target.take(),
                carried_ms: 0.,
                upload_wait_ms: 0.,
            };
            return self.launch_on_run(index, frame, spec, launch);
        }
        let Some(old) = self
            .shown_wallpaper_rgba(index)
            .or_else(|| frame.previous_rgba.take())
            .filter(|old| old.len() == frame.rgba.len())
        else {
            return Err(frame);
        };
        let read_ms = ms_since(began);
        let seed = Seed::Fade {
            from: &old,
            to: &frame.rgba,
        };
        match self.create_fade_run(index, (frame.width, frame.height), spec, seed) {
            Ok(mut run) => {
                // The fade owns the final pixels; the frame is not read for them again.
                run.target = std::mem::take(&mut frame.rgba);
                run.commit = Some((frame.revision, frame.commit_token.clone()));
                run.palette = frame.palette.clone();
                self.wallpaper_surfaces[index].fade = Some(run);
            }
            Err(error) => {
                self.wallpaper_gl_failed = true;
                eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
                self.release_wallpaper_gl();
                return Err(frame);
            }
        }
        let uploader = std::time::Instant::now();
        self.start_upload_thread(&name);
        if let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() {
            run.setup.egl += ms_since(uploader);
        }
        let launch = Launch {
            began,
            texture: None,
            tween: self.pending_palette_target.take(),
            carried_ms: 0.,
            upload_wait_ms: 0.,
        };
        self.fade_launched(index, frame, spec, false, read_ms, launch);
        Ok(())
    }

    /// The resident run takes `frame`: the texture the upload thread made from its pixels, or the
    /// pixels themselves. A GL error drops the run and gives the frame back as a cut.
    fn launch_on_run(
        &mut self,
        index: usize,
        mut frame: WallpaperFrame,
        spec: WallpaperFade,
        launch: Launch,
    ) -> Result<(), WallpaperFrame> {
        let name = frame.output.clone();
        let size = (frame.width, frame.height);
        let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() else {
            return Err(frame);
        };
        let upload = std::time::Instant::now();
        let adopted = std::cell::Cell::new(false);
        let result = (|| -> Result<(), Box<dyn Error>> {
            if run.size != size {
                return Err("frame size differs from the run".into());
            }
            let transport = self.gl.as_ref().ok_or("GL transport missing")?;
            transport.activate(&run.gl, size.0, size.1)?;
            if let (Some(texture), Some(thread)) = (launch.texture, &self.warm.upload) {
                // From here the fader owns the texture and deletes it.
                thread.uploader.adopt(texture);
                adopted.set(true);
            }
            run.fader.retarget(match launch.texture {
                Some(texture) => bead_render::gl::Pixels::Texture(texture),
                None => bead_render::gl::Pixels::Rgba(&frame.rgba),
            })
        })();
        if result.is_err() {
            if let (Some(texture), false, Some(thread)) =
                (launch.texture, adopted.get(), &self.warm.upload)
            {
                thread.uploader.discard(texture);
            }
            if let Some(run) = self.wallpaper_surfaces[index].fade.take() {
                self.drop_fade_run(run);
            }
            self.release_wallpaper_gl();
            return Err(frame);
        }
        let run = self.wallpaper_surfaces[index].fade.as_mut().unwrap();
        // A frame still waiting for its upload is superseded: its texture is deleted on arrival.
        if let (Some(old), Some(provider)) = (run.upload.take(), &self.options.wallpaper) {
            provider.discarded(&name, old.frame.revision);
        }
        let retargeted = !run.resting;
        run.start.upload_wait_ms = launch.upload_wait_ms;
        if run.resting {
            run.start.warm = true;
            // The run lives on across fades: this one's marker starts from zero.
            run.palette_steps = 0;
            run.retarget_ms = 0.;
            run.step_max_ms = 0.;
            run.setup = SetupTimes {
                upload: ms_since(upload),
                ..SetupTimes::default()
            };
            run.resting = false;
        }
        run.target = std::mem::take(&mut frame.rgba);
        run.generation += 1;
        run.gaps = GapProbe::default();
        (run.clock, run.held_since, run.done) = (None, self.started.elapsed().as_secs_f64(), false);
        run.spec = spec;
        run.palette = frame.palette.clone();
        let replaced = run
            .commit
            .replace((frame.revision, frame.commit_token.clone()));
        if let (Some((revision, _)), Some(provider)) = (replaced, &self.options.wallpaper) {
            provider.discarded(&name, revision);
        }
        self.fade_launched(index, frame, spec, retargeted, 0., launch);
        Ok(())
    }

    /// The run holds the new image: take the frame's state, draw the held first pose and start
    /// the clock's wait, the watchdog and the palette tween.
    fn fade_launched(
        &mut self,
        index: usize,
        frame: WallpaperFrame,
        spec: WallpaperFade,
        retargeted: bool,
        read_ms: f64,
        launch: Launch,
    ) {
        let name = frame.output.clone();
        let surface = &mut self.wallpaper_surfaces[index];
        surface.staged = None;
        surface.transition = None;
        surface.pixels = None;
        surface.palette = Some(frame.palette);
        surface.release_wait_ms = 0;
        surface.revision = frame.revision;
        let generation = surface.fade.as_ref().map_or(0, |run| run.generation);
        let now = self.started.elapsed().as_secs_f64();
        let cpu_start = self.cpu_mark();
        // Replacing the motion restarts the curve at its held first pose.
        self.fade_animations.retain(|id| id != name);
        self.fade_animations.open_to(
            &name,
            true,
            now,
            f64::from(spec.duration_ms) / 1000.,
            spec.curve,
        );
        let drawn_at = std::time::Instant::now();
        if let Err(error) = self.fade_draw(index, 0., true) {
            self.wallpaper_gl_failed = true;
            eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
            self.finish_wallpaper_fade(index);
            return;
        }
        let first_ms = ms_since(drawn_at);
        if let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() {
            run.start.start_ms = launch.carried_ms + ms_since(launch.began);
        }
        self.fade_committed(index, now, cpu_start);
        if let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() {
            if retargeted {
                run.retarget_ms = run.retarget_ms.max(run.start.start_ms);
            } else {
                run.setup.read = read_ms;
                run.setup.first = first_ms;
            }
        }
        if retargeted {
            // The old clock's group may be complete without this fade.
            self.finish_completed_fades();
        }
        self.schedule_fade_watchdog(&name, generation, spec.duration_ms);
        // Outputs of one change tween once together: another one may have started it already.
        if let Some(target) = launch.tween
            && self.palette_tween.as_ref().is_none_or(|t| t.to != target)
        {
            self.start_palette_tween(target, spec);
        }
    }

    /// Whether this fade's pixels are uploaded by the worker thread (warm mode, thread alive).
    fn upload_on_worker(&self) -> bool {
        worker_wanted(self.warm.on, self.wallpaper_gl_failed, self.warm.failed)
            && self.warm.upload.is_some()
    }

    /// Hand the frame's pixels to the upload thread; the fade starts when it answers
    /// (`upload_done`). A frame still waiting for its upload is superseded (last wins). The frame
    /// comes back when the thread is gone.
    fn queue_upload(
        &mut self,
        index: usize,
        mut frame: WallpaperFrame,
        spec: WallpaperFade,
    ) -> Option<WallpaperFrame> {
        let arrived = std::time::Instant::now();
        let Some(thread) = &self.warm.upload else {
            return Some(frame);
        };
        self.warm.next_token += 1;
        let token = self.warm.next_token;
        let size = (frame.width, frame.height);
        if let Err(rgba) = thread
            .uploader
            .upload(token, size, std::mem::take(&mut frame.rgba))
        {
            frame.rgba = rgba;
            self.warm_upload_failed(&frame.output, "upload thread gone");
            return Some(frame);
        }
        let pending = PendingUpload {
            token,
            spec,
            tween: self.pending_palette_target.take(),
            frame,
            arrived,
            handover_ms: ms_since(arrived),
        };
        let run = self.wallpaper_surfaces[index].fade.as_mut()?;
        if let (Some(old), Some(provider)) = (run.upload.replace(pending), &self.options.wallpaper)
        {
            provider.discarded(&old.frame.output, old.frame.revision);
        }
        None
    }

    /// The upload thread answered: start the fade from the texture, if its frame is still the
    /// run's newest. A failure starts it from the pixels on this thread.
    pub(super) fn upload_done(&mut self, done: bead_render::gl::Uploaded) {
        if let Some(prime) = self.warm.take_prime(done.token) {
            return self.prime_done(prime, done);
        }
        let waiting = self.wallpaper_surfaces.iter().position(|s| {
            s.fade.as_ref().is_some_and(|run| {
                upload_current(run.upload.as_ref().map(|pending| pending.token), done.token)
            })
        });
        let Some(index) = waiting else {
            // Superseded, or its output went away: nobody wants the texture.
            if let (Ok(texture), Some(thread)) = (done.texture, &self.warm.upload) {
                thread.uploader.discard(texture);
            }
            self.stop_upload_thread_if_idle();
            return;
        };
        let Some(PendingUpload {
            spec,
            tween,
            mut frame,
            arrived,
            handover_ms,
            ..
        }) = self.wallpaper_surfaces[index]
            .fade
            .as_mut()
            .and_then(|run| run.upload.take())
        else {
            return;
        };
        frame.rgba = done.rgba;
        let texture = match done.texture {
            // `launch_on_run` adopts it once the run can take it.
            Ok(texture) => Some(texture),
            Err(reason) => {
                self.warm_upload_failed(&frame.output, reason);
                None
            }
        };
        let launch = Launch {
            began: std::time::Instant::now(),
            texture,
            tween,
            carried_ms: handover_ms,
            upload_wait_ms: if texture.is_some() {
                arrived.elapsed().as_secs_f64() * 1e3
            } else {
                0.
            },
        };
        if let Err(frame) = self.launch_on_run(index, frame, spec, launch) {
            // The run is gone (GL error): the new image replaces the old one as a cut.
            self.install_wallpaper(frame);
        }
        self.stop_upload_thread_if_idle();
    }

    /// The first warm fade made its run: start the upload thread on a context shared with the
    /// transport's. It lives until warm mode ends (or the transport goes).
    fn start_upload_thread(&mut self, name: &str) {
        if !worker_wanted(self.warm.on, self.wallpaper_gl_failed, self.warm.failed)
            || self.warm.upload.is_some()
        {
            return;
        }
        let Some(transport) = &self.gl else {
            return;
        };
        let (sender, receiver) = calloop::channel::channel::<bead_render::gl::Uploaded>();
        let uploader = match transport.uploader(move |done| {
            let _ = sender.send(done);
        }) {
            Ok(uploader) => uploader,
            Err(error) => return self.warm_upload_failed(name, error),
        };
        let source = self.loop_handle.insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(done) = event {
                app.upload_done(done);
            }
        });
        match source {
            Ok(source) => self.warm.upload = Some(UploadThread { uploader, source }),
            Err(error) => self.warm_upload_failed(name, error),
        }
    }

    /// The upload thread cannot be used: uploads stay on the main thread until warm mode ends.
    fn warm_upload_failed(&mut self, name: &str, reason: impl std::fmt::Display) {
        self.warm.failed = true;
        eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={reason}");
        self.stop_upload_thread_if_idle();
    }

    /// Stop and join the thread (it deletes the textures nobody adopted) once it is not wanted and
    /// no frame waits for its answer.
    fn stop_upload_thread_if_idle(&mut self) {
        let waiting = self
            .wallpaper_surfaces
            .iter()
            .any(|s| s.fade.as_ref().is_some_and(|run| run.upload.is_some()));
        if !waiting && !worker_wanted(self.warm.on, self.wallpaper_gl_failed, self.warm.failed) {
            self.stop_upload_thread();
        }
    }

    fn stop_upload_thread(&mut self) {
        // The thread deletes the texture of a prime in flight; its answer is never read.
        self.warm.prime = None;
        if let Some(thread) = self.warm.upload.take() {
            self.loop_handle.remove(thread.source);
            drop(thread.uploader);
        }
    }

    /// The shared transport, made on first use.
    fn ensure_transport(&mut self) -> Result<(), Box<dyn Error>> {
        use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
        use std::ptr::NonNull;
        if self.gl.is_none() {
            let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                NonNull::new(self.conn.backend().display_ptr().cast())
                    .ok_or("native display missing")?,
            ));
            // SAFETY: the retained connection outlives the EGL context and surfaces.
            self.gl = Some(unsafe { bead_render::gl::Transport::new(display) }?);
        }
        Ok(())
    }

    /// A run with its transport, EGL surface and canvas; the caller sets what comes from a frame
    /// (target, commit, palette).
    fn create_fade_run(
        &mut self,
        index: usize,
        size: (u32, u32),
        spec: WallpaperFade,
        seed: Seed,
    ) -> Result<FadeRun, Box<dyn Error>> {
        use raw_window_handle::{RawWindowHandle, WaylandWindowHandle};
        use std::ptr::NonNull;
        let (width, height) = size;
        let began = std::time::Instant::now();
        self.ensure_transport()?;
        let egl_ms = ms_since(began);
        let surface = &self.wallpaper_surfaces[index];
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.layer.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        let transport = self.gl.as_mut().unwrap();
        let began = std::time::Instant::now();
        // SAFETY: FadeRun drops the EGL surface before the wallpaper layer surface.
        let gl = unsafe { transport.create_surface(window, width, height) }?;
        let surface_ms = ms_since(began);
        let fader = match seed {
            Seed::Fade { from, to } => {
                bead_render::gl::WallpaperFader::new(&transport.display, size, from, to)?
            }
            Seed::Rest(texture) => bead_render::gl::WallpaperFader::at_rest(
                &transport.display,
                size,
                bead_render::gl::Pixels::Texture(texture),
            )?,
        };
        let fader_ms = fader.setup_ms;
        let logical = self
            .output
            .info(&surface.output)
            .and_then(|info| info.logical_size);
        if let Some(viewport) = &surface.viewport {
            if let Some((w, h)) = logical {
                viewport.set_destination(w.max(1), h.max(1));
            }
            surface.layer.wl_surface().set_buffer_scale(1);
        } else {
            surface
                .layer
                .wl_surface()
                .set_buffer_scale(surface.scale_factor.max(1));
        }
        Ok(FadeRun {
            fader,
            gl,
            size,
            spec,
            target: Vec::new(),
            commit: None,
            palette: bead_render::Palette(Default::default()),
            generation: 1,
            palette_steps: 0,
            gaps: GapProbe::default(),
            setup: SetupTimes {
                egl: egl_ms,
                surface: surface_ms,
                canvas: fader_ms.0,
                upload: fader_ms.1,
                ..SetupTimes::default()
            },
            retarget_ms: 0.,
            step_max_ms: 0.,
            clock: None,
            held_since: self.started.elapsed().as_secs_f64(),
            done: false,
            resting: false,
            upload: None,
            start: StartInfo::default(),
        })
    }

    /// B0325: warm mode began (or a prime ended): build the resting run of the next output that
    /// has none, on an idle turn so no turn pays for every output.
    fn schedule_prime(&self) {
        self.loop_handle.insert_idle(|app| app.prime_next());
    }

    /// Hand the shown image of one output to the upload thread; `prime_done` builds its run.
    fn prime_next(&mut self) {
        if self.warm.prime.is_some() {
            return;
        }
        let began = std::time::Instant::now();
        let Some(index) = self.wallpaper_surfaces.iter().position(|s| {
            prime_wanted(
                self.warm.on,
                self.wallpaper_gl_failed,
                self.warm.failed,
                s.fade.is_some(),
                s.attached.is_some() && s.transition.is_none(),
            )
        }) else {
            return;
        };
        let name = self.wallpaper_surfaces[index].name.clone();
        let size = (
            self.wallpaper_surfaces[index].width,
            self.wallpaper_surfaces[index].height,
        );
        // Not retried on failure: a prime is chained only from a successful one.
        let Some(rgba) = self.shown_wallpaper_rgba(index) else {
            return;
        };
        if let Err(error) = self.ensure_transport() {
            self.wallpaper_gl_failed = true;
            eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
            self.release_wallpaper_gl();
            return;
        }
        // The upload thread shares the transport's context, which a prime must make first.
        if let Some(Err(error)) = self.gl.as_mut().map(|gl| gl.ensure_context()) {
            self.warm_upload_failed(&name, error);
            return;
        }
        self.start_upload_thread(&name);
        let token = self.warm.next_token + 1;
        let Some(thread) = &self.warm.upload else {
            return;
        };
        if thread.uploader.upload(token, size, rgba).is_err() {
            self.warm_upload_failed(&name, "upload thread gone");
            return;
        }
        self.warm.next_token = token;
        self.warm.prime = Some(Prime {
            name,
            token,
            queued: std::time::Instant::now(),
            ms: ms_since(began),
        });
    }

    /// The upload thread made the shown image's texture: the resting run takes it, nothing is
    /// drawn or presented and the provider hears nothing. A texture nobody wants is deleted.
    fn prime_done(&mut self, prime: Prime, done: bead_render::gl::Uploaded) {
        let began = std::time::Instant::now();
        let upload_wait_ms = prime.queued.elapsed().as_secs_f64() * 1e3;
        let texture = match done.texture {
            Ok(texture) => texture,
            Err(reason) => {
                self.warm_upload_failed(&prime.name, reason);
                self.release_wallpaper_gl();
                return;
            }
        };
        let index = self.wallpaper_surfaces.iter().position(|s| {
            s.name == prime.name
                && s.fade.is_none()
                && s.width as usize * s.height as usize * 4 == done.rgba.len()
        });
        let wanted = worker_wanted(self.warm.on, self.wallpaper_gl_failed, self.warm.failed);
        let Some(index) = index.filter(|_| wanted) else {
            if let Some(thread) = &self.warm.upload {
                thread.uploader.discard(texture);
            }
            self.release_wallpaper_gl();
            return;
        };
        let surface = &self.wallpaper_surfaces[index];
        let size = (surface.width, surface.height);
        let spec = self.options.wallpaper_fade.unwrap_or_default();
        match self.create_fade_run(index, size, spec, Seed::Rest(texture)) {
            Ok(mut run) => {
                if let Some(thread) = &self.warm.upload {
                    thread.uploader.adopt(texture);
                }
                (run.resting, run.done) = (true, true);
                self.wallpaper_surfaces[index].fade = Some(run);
                let ms = prime.ms + ms_since(began);
                println!("{}", prime_marker(&prime.name, ms, upload_wait_ms));
                self.schedule_prime();
            }
            Err(error) => {
                if let Some(thread) = &self.warm.upload {
                    thread.uploader.discard(texture);
                }
                self.wallpaper_gl_failed = true;
                eprintln!(
                    "WALLPAPER_FADE_FALLBACK output={} reason={error}",
                    prime.name
                );
                self.release_wallpaper_gl();
            }
        }
    }

    /// Draw one blend, ask for the next frame callback and present.
    fn fade_draw(&mut self, index: usize, blend: f32, first: bool) -> Result<(), Box<dyn Error>> {
        let surface = &mut self.wallpaper_surfaces[index];
        let run = surface.fade.as_mut().ok_or("no fade")?;
        let transport = self.gl.as_ref().ok_or("GL transport missing")?;
        transport.activate(&run.gl, run.size.0, run.size.1)?;
        let mode = match run.spec.kind {
            WallpaperFadeKind::Fade => bead_render::gl::WallpaperBlend::Fade,
            WallpaperFadeKind::Grow { origin } => bead_render::gl::WallpaperBlend::Grow { origin },
        };
        run.fader.draw(&surface.name, blend, mode);
        let wl_surface = surface.layer.wl_surface();
        if first
            && let Some((provider, (revision, token))) =
                self.options.wallpaper.as_ref().zip(run.commit.as_ref())
        {
            provider.uploaded(&surface.name, *revision, token);
            if let Some(presentation) = &self.presentation {
                presentation.feedback(
                    wl_surface,
                    &self.qh,
                    PresentationData {
                        content: false,
                        surface: wl_surface.clone(),
                        generation: *revision,
                        source_epoch: 0,
                        visibility: 0,
                        update: *revision,
                    },
                );
            }
        }
        wl_surface.frame(
            &self.qh,
            FrameData {
                surface: wl_surface.clone(),
                epoch: 0,
                generation: run.generation,
                source_epoch: 0,
            },
        );
        transport.present(&run.gl, &[Damage::full(run.size.0, run.size.1)])
    }

    /// A fade frame reached the compositor: the first one starts the clock and reports the commit.
    /// While another output of the change is still being set up (B0316) the first pose stays held
    /// and the clock does not start: the commit that ends the hold starts every held fade, and
    /// the palette tween, at one instant.
    fn fade_committed(&mut self, index: usize, drawn: f64, cpu_start: CpuMark) {
        let now = self.started.elapsed().as_secs_f64();
        let name = self.wallpaper_surfaces[index].name.clone();
        let run = self.wallpaper_surfaces[index].fade.as_ref();
        let holding = run.is_some_and(|run| {
            run.clock.is_none()
                && hold_first_pose(self.fade_partner_coming(&name), now - run.held_since)
        });
        let slide = if holding {
            None
        } else {
            for run in self
                .wallpaper_surfaces
                .iter_mut()
                .filter_map(|s| s.fade.as_mut())
                .filter(|run| run.clock.is_none())
            {
                run.clock = Some(now);
            }
            if let Some(tween) = self.palette_tween.as_mut().filter(|t| t.waiting) {
                tween.start_clock(now);
            }
            self.fade_animations.committed_timed(
                &name,
                drawn,
                now,
                None,
                drawn,
                &bead_core::profile::NO_SPLITS,
            )
        };
        let mut end = self.cpu_mark();
        self.fade_frame_cpu_ns += end.cpu.saturating_sub(cpu_start.cpu);
        end.frames = self.fade_frame_cpu_ns;
        let surface = &mut self.wallpaper_surfaces[index];
        if let Some(run) = surface.fade.as_mut().filter(|_| !holding) {
            run.gaps.committed(now, cpu_start, end);
        }
        if let (Some(slide), Some(run)) = (slide, &surface.fade) {
            println!(
                "{}",
                transition_marker(
                    &name,
                    run.spec.kind,
                    &slide,
                    run.palette_steps,
                    run.gaps.other_ms(),
                    &run.gaps.cpu_top(),
                    &run.setup,
                    run.retarget_ms,
                    run.clock.unwrap_or_default(),
                    (run.gaps.peer_ms(), run.gaps.step_ms(), run.step_max_ms),
                    &run.start,
                )
            );
        }
        if let Some((revision, token)) = surface.fade.as_mut().and_then(|run| run.commit.take()) {
            self.wallpaper_committed(&name, revision, &token, true);
        }
    }

    fn fade_partner_coming(&self, own: &str) -> bool {
        partner_coming(
            own,
            &self.wallpaper_jobs,
            self.wallpaper_surfaces
                .iter()
                .map(|s| (s.name.as_str(), s.attached.is_some() || s.fade.is_some())),
        ) || self.wallpaper_surfaces.iter().any(|s| {
            // An output whose frame waits for its upload fades with this change too.
            s.name != own && s.fade.as_ref().is_some_and(|run| run.upload.is_some())
        })
    }

    /// Finish every fade that drew its last frame once all fades of its clock have.
    pub(super) fn finish_completed_fades(&mut self) {
        let runs: Vec<_> = self
            .wallpaper_surfaces
            .iter()
            .map(|s| s.fade.as_ref().map(|run| (run.clock, run.done)))
            .collect();
        for (index, own) in runs.iter().enumerate() {
            if let Some((clock, true)) = own
                && group_done(runs.iter().flatten().copied(), *clock)
            {
                self.finish_wallpaper_fade(index);
            }
        }
    }

    /// The compositor asked for the next fade frame.
    pub(super) fn wallpaper_fade_frame(
        &mut self,
        surface: &wl_surface::WlSurface,
        generation: u64,
    ) -> bool {
        let Some(index) = self
            .wallpaper_surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface && s.fade.is_some())
        else {
            return false;
        };
        if self.wallpaper_surfaces[index]
            .fade
            .as_ref()
            .map(|run| run.generation)
            != Some(generation)
            || self.wallpaper_surfaces[index]
                .fade
                .as_ref()
                .is_some_and(|run| run.done)
        {
            return true;
        }
        let name = self.wallpaper_surfaces[index].name.clone();
        let drawn = self.started.elapsed().as_secs_f64();
        let cpu_start = self.cpu_mark();
        let (blend, running) = self
            .fade_animations
            .value(&name, drawn)
            .unwrap_or((1., false));
        if self.fade_draw(index, blend, false).is_err() {
            self.wallpaper_gl_failed = true;
            self.finish_wallpaper_fade(index);
            return true;
        }
        self.fade_committed(index, drawn, cpu_start);
        if !running {
            if let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() {
                run.done = true;
            }
            self.finish_completed_fades();
        }
        self.palette_step_after_frame();
        true
    }

    /// Frame callbacks stop for a hidden surface; the fade still has to end.
    fn schedule_fade_watchdog(&self, name: &str, generation: u64, duration_ms: u32) {
        let name = name.to_owned();
        let _ = self.loop_handle.clone().insert_source(
            Timer::from_duration(Duration::from_millis(u64::from(duration_ms) + 500)),
            move |_, _, app| {
                if let Some(index) = app.wallpaper_surfaces.iter().position(|s| {
                    s.name == name
                        && s.fade
                            .as_ref()
                            .is_some_and(|run| run.generation == generation)
                }) {
                    app.finish_wallpaper_fade(index);
                }
                TimeoutAction::Drop
            },
        );
    }

    /// A fade drew its last frame (or its watchdog fired): warm, it rests; otherwise it is released.
    pub(super) fn finish_wallpaper_fade(&mut self, index: usize) {
        match self.wallpaper_surfaces[index].fade.as_ref() {
            None => {}
            Some(run) if run.resting => {}
            Some(run) => {
                match fade_end(self.warm.on, self.wallpaper_gl_failed, run.upload.is_some()) {
                    FadeEnd::Rest => self.rest_wallpaper_fade(index),
                    FadeEnd::Release => self.release_wallpaper_run(index),
                }
            }
        }
    }

    /// B0317: the fade ended warm. The EGL surface keeps showing the final frame (no SHM attach):
    /// `from` is deleted, the transport, surface, canvas and the shown texture stay.
    /// `committed` fired with the fade's first frame, as for a cold one; only a fade that never
    /// reached it reports here, so the provider hears each revision exactly once.
    fn rest_wallpaper_fade(&mut self, index: usize) {
        let began = std::time::Instant::now();
        let name = self.wallpaper_surfaces[index].name.clone();
        let Some(run) = self.wallpaper_surfaces[index].fade.as_mut() else {
            return;
        };
        let activated = (|| -> Result<(), Box<dyn Error>> {
            let transport = self.gl.as_ref().ok_or("GL transport missing")?;
            transport.activate(&run.gl, run.size.0, run.size.1)
        })();
        if let Err(error) = activated {
            self.wallpaper_gl_failed = true;
            eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
            self.release_wallpaper_run(index);
            return;
        }
        run.fader.settle();
        (run.resting, run.done) = (true, true);
        let commit = run.commit.take();
        self.fade_animations.retain(|id| id != name);
        if let Some((revision, token)) = commit {
            if let Some(provider) = &self.options.wallpaper {
                provider.uploaded(&name, revision, &token);
            }
            self.wallpaper_committed(&name, revision, &token, false);
        }
        self.schedule_wallpaper_release_poll(name.clone());
        // Warm mode began during this fade: the thread starts now, before the next change.
        self.start_upload_thread(&name);
        // The replaced pixels (and the provider's frame buffers) go back to the system now, as
        // for a cold finish: warm mode holds textures, not heap fragments.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: plain libc call.
        unsafe {
            libc::malloc_trim(0);
        }
        println!(
            "WALLPAPER_FADE_FINISH output={name} ms={:.1}{}",
            ms_since(began),
            skipped_suffix(std::mem::take(&mut self.panel_steps_skipped))
        );
    }

    /// Free a run's GL objects in the order EGL needs: textures and canvas with the context
    /// current, the context unbound, then the window surface.
    pub(super) fn drop_fade_run(&self, run: FadeRun) {
        let FadeRun {
            fader,
            gl,
            size,
            upload,
            ..
        } = run;
        // A frame that never got its fade is dropped: the provider hears it was discarded.
        if let (Some(pending), Some(provider)) = (upload, &self.options.wallpaper) {
            provider.discarded(&pending.frame.output, pending.frame.revision);
        }
        if let Some(transport) = &self.gl {
            let _ = transport.activate(&gl, size.0, size.1);
        }
        drop(fader);
        if let Some(transport) = &self.gl {
            let _ = transport.unbind();
        }
        drop(gl);
    }

    /// Attach the final SHM buffer, then give the run's GL objects (and, last, the transport) back.
    pub(super) fn release_wallpaper_run(&mut self, index: usize) {
        if self.release_run_objects(index) {
            self.release_wallpaper_gl();
        }
    }

    fn release_run_objects(&mut self, index: usize) -> bool {
        let Some(mut run) = self.wallpaper_surfaces[index].fade.take() else {
            return false;
        };
        let began = std::time::Instant::now();
        let target = std::mem::take(&mut run.target);
        let mut commit = run.commit.take();
        let palette = run.palette.clone();
        self.drop_fade_run(run);
        let output_handle = self.wallpaper_surfaces[index].output.clone();
        let logical = self
            .output
            .info(&output_handle)
            .and_then(|info| info.logical_size);
        let provider = self.options.wallpaper.clone();
        let name = self.wallpaper_surfaces[index].name.clone();
        let surface = &mut self.wallpaper_surfaces[index];
        self.fade_animations.retain(|id| id != name);
        let attached = if final_attach_needed(&target) {
            Some(attach_wallpaper_buffer(
                WallpaperBufferTarget {
                    pool: &mut surface.pool,
                    layer: &surface.layer,
                    buffers: &mut surface.buffers,
                    viewport: surface.viewport.as_ref(),
                    scale_factor: surface.scale_factor,
                    width: surface.width,
                    height: surface.height,
                    logical,
                    reserved_buffer: None,
                    prepared_buffer: None,
                    feedback: None,
                },
                &target,
                || {
                    if let (Some(provider), Some((revision, token))) = (&provider, &commit) {
                        provider.uploaded(&name, *revision, token);
                    }
                },
            ))
        } else {
            None
        };
        match attached {
            // A primed run that never presented: the SHM buffer is still attached.
            None => {}
            Some(Ok(selected)) => surface.attached = Some(selected),
            Some(Err(_)) => {
                // Buffer backpressure: the transition retry slot attaches it on the next tick.
                surface.transition = Some(WallpaperTransition {
                    commit: None,
                    palette,
                    pixels: target,
                    target: None,
                    remaining: 0,
                    uploaded: true,
                });
                self.schedule_wallpaper_tick(name.clone());
            }
        }
        if let Some((revision, token)) = commit.take() {
            self.wallpaper_committed(&name, revision, &token, false);
        }
        self.schedule_wallpaper_release_poll(name.clone());
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: plain libc call; returns the freed fade copies to the system.
        unsafe {
            libc::malloc_trim(0);
        }
        println!(
            "WALLPAPER_FADE_FINISH output={name} ms={:.1}{}",
            ms_since(began),
            skipped_suffix(std::mem::take(&mut self.panel_steps_skipped))
        );
        true
    }

    /// The shared transport is about to be dropped by another owner (GL failure, switch to the CPU
    /// renderer): end every wallpaper run first (final attach as a cut, GL objects, upload thread).
    pub(super) fn release_all_wallpaper_runs(&mut self) {
        for index in 0..self.wallpaper_surfaces.len() {
            self.release_run_objects(index);
        }
        self.stop_upload_thread();
    }

    /// The host's warm switch changed (or was read for the first time).
    pub(super) fn set_wallpaper_warm(&mut self, on: bool) {
        if self.warm.on == on {
            return;
        }
        self.warm.on = on;
        self.warm.failed = false;
        self.warm.releasing = false;
        if on {
            self.report_warm(true);
            self.schedule_prime();
            return;
        }
        self.warm.releasing = true;
        self.release_wallpaper_gl();
        // A fade that still runs keeps its run; the line with nothing left follows its end.
        if self.warm.releasing {
            self.report_warm(false);
        }
    }

    fn report_warm(&self, on: bool) {
        let runs = self
            .wallpaper_surfaces
            .iter()
            .filter_map(|s| s.fade.as_ref());
        let (mut outputs, mut textures, mut bytes) = (0, 0, 0u64);
        for run in runs {
            outputs += 1;
            textures += run.fader.textures();
            bytes +=
                run.fader.textures() as u64 * u64::from(run.size.0) * u64::from(run.size.1) * 4;
        }
        println!("{}", warm_marker(on, outputs, textures, bytes));
    }

    /// Give the shared EGL transport back once no fade or other GL surface needs it. Runs at rest
    /// go first when warm mode is over; a running fade releases itself when it ends.
    pub(super) fn release_wallpaper_gl(&mut self) {
        self.release_gl_objects();
        if self.warm.releasing && !self.wallpaper_surfaces.iter().any(|s| s.fade.is_some()) {
            self.warm.releasing = false;
            println!("{}", warm_marker(false, 0, 0, 0));
            let live = bead_render::gl::live_textures();
            if live != 0 {
                eprintln!("WALLPAPER_WARM_LEAK live_textures={live}");
            }
        }
    }

    fn release_gl_objects(&mut self) {
        for index in 0..self.wallpaper_surfaces.len() {
            let idle = self.wallpaper_surfaces[index]
                .fade
                .as_ref()
                .is_some_and(|run| {
                    release_idle(
                        self.warm.on,
                        self.wallpaper_gl_failed,
                        run.resting,
                        run.upload.is_some(),
                    )
                });
            if idle {
                self.release_run_objects(index);
            }
        }
        self.stop_upload_thread_if_idle();
        let fading = self.wallpaper_surfaces.iter().any(|s| s.fade.is_some());
        if !transport_releasable(
            fading,
            self.has_gl_surfaces(),
            self.options.renderer == RendererKind::Cpu,
        ) {
            return;
        }
        // The upload thread's context shares the transport's: it ends before the display does.
        self.stop_upload_thread();
        if let Some(transport) = self.gl.take() {
            // SAFETY: no fade, bar or companion owns an EGL surface or renderer any more.
            if let Err(error) = unsafe { transport.shutdown_exclusive() } {
                eprintln!("WALLPAPER_FADE_RELEASE_ERROR {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead_core::animation::Animations;
    fn blend(animations: &Animations, now: f64) -> f32 {
        animations.value("DP-1", now).unwrap().0
    }
    #[test]
    fn the_blend_holds_the_first_pose_then_follows_the_curve() {
        let spec = WallpaperFade::default();
        let seconds = f64::from(spec.duration_ms) / 1000.;
        let mut a = Animations::default();
        a.open_to("DP-1", true, 10., seconds, spec.curve);
        assert_eq!(
            blend(&a, 10.5),
            0.,
            "held until the first frame is committed"
        );
        a.committed("DP-1", 10.5, 10.5);
        assert_eq!(blend(&a, 10.5), 0.);
        let half = blend(&a, 10.5 + seconds / 2.);
        assert!((half - 0.5).abs() < 1e-6, "in-out-cubic is symmetric");
        assert_eq!(a.value("DP-1", 10.5 + seconds), Some((1., false)));
        let mut last = 0.;
        for step in 0..=84 {
            let value = blend(&a, 10.5 + f64::from(step) / 120.);
            assert!(value >= last);
            last = value;
        }
    }
    #[test]
    fn a_new_wallpaper_restarts_the_curve_from_the_held_pose() {
        let mut a = Animations::default();
        a.open_to("DP-1", true, 0., 0.7, Curve::EaseInOut);
        a.committed("DP-1", 0.1, 0.1);
        assert!(blend(&a, 0.4) > 0.);
        // The renderer baked the shown blend into the `from` texture: the new motion starts at 0.
        a.retain(|id| id != "DP-1");
        a.open_to("DP-1", true, 0.4, 0.7, Curve::EaseInOut);
        assert_eq!(blend(&a, 0.45), 0., "no jump on retarget");
        a.committed("DP-1", 0.45, 0.45);
        assert_eq!(a.value("DP-1", 0.45 + 0.7), Some((1., false)));
    }
    #[test]
    fn a_fade_ends_at_rest_only_while_warm_and_gl_works() {
        assert_eq!(fade_end(true, false, false), FadeEnd::Rest);
        assert_eq!(fade_end(false, false, false), FadeEnd::Release);
        assert_eq!(
            fade_end(true, true, false),
            FadeEnd::Release,
            "GL failure releases"
        );
        assert_eq!(
            fade_end(false, false, true),
            FadeEnd::Rest,
            "a frame waiting for its upload still gets its fade"
        );
        assert_eq!(
            fade_end(true, true, true),
            FadeEnd::Release,
            "no fade starts after a GL failure"
        );
    }
    #[test]
    fn warm_off_releases_runs_at_rest_at_once_and_running_ones_when_they_end() {
        // (warm, gl_failed, resting, upload waiting)
        assert!(
            !release_idle(true, false, true, false),
            "warm keeps the run"
        );
        assert!(
            release_idle(false, false, true, false),
            "off and idle: right away"
        );
        assert!(
            !release_idle(false, false, false, false),
            "off but fading: at its end"
        );
        assert!(
            !release_idle(false, false, true, true),
            "off, upload waiting: after its fade"
        );
        assert!(
            release_idle(true, true, true, false),
            "GL failure releases a run at rest"
        );
        // The running fade then ends through `fade_end`, with warm off: released.
        assert_eq!(fade_end(false, false, false), FadeEnd::Release);
    }
    #[test]
    fn a_wallpaper_run_keeps_the_shared_transport_alive_for_every_owner() {
        assert!(!gl_surfaces_present(false, false));
        assert!(gl_surfaces_present(false, true), "a resting run counts");
        assert!(gl_surfaces_present(true, false));
    }
    #[test]
    fn warm_mode_reports_its_state_in_one_line() {
        assert_eq!(
            warm_marker(true, 2, 2, 40_608_000),
            "WALLPAPER_WARM state=on outputs=2 textures=2 bytes=40608000"
        );
        assert_eq!(
            warm_marker(false, 0, 0, 0),
            "WALLPAPER_WARM state=off outputs=0 textures=0 bytes=0"
        );
    }
    fn primed(name: &str, token: u64) -> Warm {
        Warm {
            prime: Some(Prime {
                name: name.into(),
                token,
                queued: std::time::Instant::now(),
                ms: 0.,
            }),
            ..Warm::default()
        }
    }
    #[test]
    fn only_a_surface_without_a_run_but_with_a_shown_image_is_primed_while_warm() {
        // (warm, gl_failed, worker_failed, has_run, shown)
        assert!(prime_wanted(true, false, false, false, true));
        assert!(!prime_wanted(false, false, false, false, true), "warm off");
        assert!(!prime_wanted(true, true, false, false, true), "GL failed");
        assert!(!prime_wanted(true, false, true, false, true), "no thread");
        assert!(
            !prime_wanted(true, false, false, true, true),
            "has a run (also a cold fade that is still running: it rests resident at its end)"
        );
        assert!(!prime_wanted(true, false, false, false, false), "no image");
    }
    #[test]
    fn a_frame_during_a_prime_cancels_it_so_nothing_is_created_twice_or_lost() {
        let mut warm = primed("DP-1", 7);
        assert!(
            !warm.cancel_prime("eDP-1"),
            "another output's frame keeps it"
        );
        assert!(warm.prime.is_some());
        assert!(warm.cancel_prime("DP-1"), "the frame starts cold");
        assert!(
            warm.take_prime(7).is_none(),
            "the answer finds no prime: its texture is deleted"
        );
    }
    #[test]
    fn a_prime_answer_after_release_is_not_used() {
        let mut warm = primed("DP-1", 7);
        assert!(warm.take_prime(6).is_none(), "another token");
        warm.prime = None; // warm off, output removed or GL failure (`stop_upload_thread`)
        assert!(warm.take_prime(7).is_none());
        let mut warm = primed("DP-1", 7);
        assert_eq!(
            warm.take_prime(7).map(|prime| prime.name),
            Some("DP-1".into())
        );
        assert!(warm.take_prime(7).is_none(), "an answer is used once");
    }
    #[test]
    fn a_primed_run_that_never_presented_releases_without_an_attach() {
        assert!(!final_attach_needed(&[]));
        assert!(final_attach_needed(&[0, 0, 0, 255]));
    }
    #[test]
    fn a_primed_output_reports_its_cost_in_one_line() {
        assert_eq!(
            prime_marker("eDP-1", 12.34, 5.67),
            "WALLPAPER_WARM_PRIME output=eDP-1 ms=12.3 upload_wait_ms=5.7"
        );
    }
    #[test]
    fn the_upload_thread_serves_warm_mode_only_and_the_newest_frame_wins() {
        // (warm, gl_failed, worker_failed)
        assert!(worker_wanted(true, false, false));
        assert!(
            !worker_wanted(false, false, false),
            "not warm: today's main-thread upload"
        );
        assert!(!worker_wanted(true, true, false));
        assert!(
            !worker_wanted(true, false, true),
            "a failed thread falls back"
        );
        assert!(upload_current(Some(7), 7));
        assert!(
            !upload_current(Some(8), 7),
            "superseded: the texture is deleted"
        );
        assert!(!upload_current(None, 7), "the run is gone or fell back");
    }
    fn palette(colour: [u8; 4]) -> PaletteMap {
        [("accent".to_owned(), colour)].into()
    }
    #[test]
    fn the_palette_mixes_in_linear_light_and_ends_exactly_on_the_target() {
        let (a, b) = (palette([0, 0, 0, 255]), palette([255, 255, 255, 255]));
        assert_eq!(mix_palette(&a, &b, 0.), a);
        assert_eq!(mix_palette(&a, &b, 1.), b);
        let mid = mix_palette(&a, &b, 0.5)["accent"][0];
        assert!(
            (180..=190).contains(&mid),
            "linear mid grey is about 0.73 in sRGB: {mid}"
        );
        let new_role = mix_palette(&a, &[("other".to_owned(), [9, 9, 9, 9])].into(), 0.3);
        assert_eq!(
            new_role["other"],
            [9, 9, 9, 9],
            "a role without a source starts at its target"
        );
    }
    #[test]
    fn the_palette_schedule_is_bounded_and_follows_the_curve() {
        let steps = palette_schedule(0.7, Curve::EaseInOut);
        let n = steps.len();
        assert_eq!(n, 42, "700 ms at one step per 60 Hz frame");
        assert!((steps[n - 1].0 - 0.7).abs() < 1e-9);
        assert_eq!(steps[n - 1].1, 1.);
        assert!(
            steps
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1)
        );
        assert!(
            (steps[n / 2 - 1].1 - 0.5).abs() < 1e-6,
            "half time, half way"
        );
    }
    #[test]
    fn the_step_count_is_one_per_frame_within_bounds() {
        assert_eq!(palette_steps(0.5), 30);
        assert_eq!(palette_steps(0.05), PALETTE_STEPS_MIN);
        assert_eq!(palette_steps(0.), PALETTE_STEPS_MIN);
        assert_eq!(palette_steps(5.), PALETTE_STEPS_MAX);
        let s = palette_schedule(0.5, Curve::Linear);
        assert_eq!(
            (s.len(), s[29]),
            (30, (0.5, 1.)),
            "the last step is the end"
        );
    }
    #[test]
    fn a_finished_fade_reports_its_frames_in_the_marker() {
        let mut a = Animations::default();
        a.open_to("DP-1", true, 0., 0.7, Curve::EaseInOut);
        let mut time = 0.05;
        let slide = loop {
            if let Some(slide) = a.committed("DP-1", time, time + 0.002) {
                break slide;
            }
            time += if time > 0.3 && time < 0.35 {
                0.025
            } else {
                1. / 120.
            };
        };
        let line = transition_marker(
            "DP-1",
            WallpaperFadeKind::Grow { origin: (0.5, 0.5) },
            &slide,
            10,
            3.5,
            "4:1.2,9:0.8",
            &SetupTimes {
                read: 3.1,
                egl: 0.,
                surface: 2.,
                canvas: 20.4,
                upload: 9.8,
                first: 12.,
            },
            4.2,
            12.3456,
            (3.04, 0.25, 1.26),
            &StartInfo {
                warm: true,
                start_ms: 1.84,
                upload_wait_ms: 17.26,
            },
        );
        assert!(line.starts_with("WALLPAPER_TRANSITION output=DP-1 kind=grow frames="));
        assert!(
            line.contains("first_frame_ms=52.0") && line.contains("max_gap_ms=2"),
            "{line}"
        );
        assert!(
            line.ends_with(
                " palette_steps=10 gap_other_ms=3.5 setup=read:3.1,egl:0.0,surface:2.0,\
                 canvas:20.4,upload:9.8,first:12.0 retarget_ms=4.2 clock_s=12.346 \
                 gap_peer_ms=3.0 gap_step_ms=0.2 step_max_ms=1.3 warm=1 start_ms=1.8 \
                 upload_wait_ms=17.3"
            ),
            "{line}"
        );
        assert!(line.contains(" render_top="));
        assert!(
            line.contains(" render_cpu_top=4:1.2,9:0.8 palette_steps=10"),
            "{line}"
        );
    }
    #[test]
    fn the_gap_probe_charges_the_largest_gap_with_the_other_cpu_time_before_its_frame() {
        let ms = 1_000_000;
        let mark = |cpu, frames, steps| CpuMark {
            cpu: cpu * ms,
            frames: frames * ms,
            steps: steps * ms,
        };
        let mut p = GapProbe::default();
        p.committed(0., mark(0, 0, 0), mark(2, 2, 0));
        // 8 ms gap, 0.5 ms of other work, the frame itself 2 ms.
        p.committed(
            0.008,
            CpuMark {
                cpu: 2 * ms + ms / 2,
                ..mark(2, 2, 0)
            },
            mark(4, 4, 0),
        );
        assert!((p.other_ms() - 0.5).abs() < 1e-9);
        // A 16 ms gap with 9 ms of CPU: a partner's 3 ms frame and a 4 ms palette step; the
        // frames sum includes this output's own frame (2 ms) once it committed.
        p.committed(0.024, mark(13, 7, 4), mark(15, 9, 4));
        assert!((p.other_ms() - 9.).abs() < 1e-9);
        assert!((p.peer_ms() - 3.).abs() < 1e-9 && (p.step_ms() - 4.).abs() < 1e-9);
        // A later shorter gap does not replace it.
        p.committed(0.032, mark(30, 20, 9), mark(31, 21, 9));
        assert!((p.other_ms() - 9.).abs() < 1e-9 && (p.peer_ms() - 3.).abs() < 1e-9);
    }
    fn tween(from: u8, to: u8, started: f64) -> PaletteTween {
        PaletteTween::new(
            palette([from, 0, 0, 255]),
            palette([to, 0, 0, 255]),
            WallpaperFade::default(),
            started,
            1,
        )
    }
    /// Every step a tween delivers to a panel, as `apply_palette_step` decides it.
    fn delivered(t: &mut PaletteTween, open: bool, at_rest: bool) -> Vec<PaletteMap> {
        let (mut out, mut now) = (vec![], 0.);
        loop {
            now += 0.001;
            let Some((step, wait)) = t.advance(now) else {
                continue;
            };
            let last = wait.is_none();
            if panel_takes_step(last, open, at_rest) {
                out.push(step);
            }
            if last {
                return out;
            }
        }
    }
    #[test]
    fn an_at_rest_panel_gets_only_the_final_step_and_a_closed_one_too() {
        let n = tween(0, 200, 0.).steps.len();
        assert!(n > 1);
        let mut normal = tween(0, 200, 0.);
        assert_eq!(delivered(&mut normal, true, false).len(), n, "today");
        let mut rest = tween(0, 200, 0.);
        assert_eq!(
            delivered(&mut rest, true, true),
            vec![palette([200, 0, 0, 255])]
        );
        let mut closed = tween(0, 200, 0.);
        assert_eq!(
            delivered(&mut closed, false, false),
            vec![palette([200, 0, 0, 255])],
            "a closed retained panel is re-staged once, with no prop"
        );
        assert!(panel_takes_step(true, false, true), "a cut is a last step");
        // A retarget replaces the tween: only the new one's final palette is ever delivered.
        let mut second = PaletteTween::new(
            palette([100, 0, 0, 255]),
            palette([50, 0, 0, 255]),
            WallpaperFade::default(),
            1.,
            2,
        );
        assert_eq!(
            delivered(&mut second, true, true),
            vec![palette([50, 0, 0, 255])]
        );
    }
    #[test]
    fn the_skipped_step_count_only_shows_when_nonzero() {
        assert_eq!(skipped_suffix(0), "");
        assert_eq!(skipped_suffix(29), " panel_steps_skipped=29");
    }
    #[test]
    fn a_step_due_during_a_fade_is_applied_after_a_frame_not_by_the_timer() {
        let mut t = tween(0, 200, 10.);
        assert!(
            t.advance(10.01).is_none(),
            "the first step is due at 16.7 ms"
        );
        t.last_frame = 10.065;
        assert!(t.frames_flow(10.07), "frames flow: the timer defers");
        assert!(t.retry_after(10.07) >= 0.01, "and looks again soon");
        let (colour, wait) = t.advance(10.073).expect("the frame applies the due step");
        assert!(colour["accent"][0] > 0 && colour["accent"][0] < 100);
        assert!(
            (wait.unwrap() - 0.01033).abs() < 1e-4,
            "wait for the next step"
        );
        assert!(t.advance(10.074).is_none(), "applied once");
        assert!(!t.frames_flow(10.2), "frames stopped: the timer takes over");
    }
    #[test]
    fn without_a_fade_the_timer_runs_every_step_and_ends_exactly_on_the_target() {
        let mut t = tween(0, 200, 0.);
        let times: Vec<f64> = t.steps.iter().map(|(at, _)| *at).collect();
        let mut applied = Vec::new();
        for (k, at) in times.iter().enumerate() {
            let (colour, wait) = t.advance(*at).expect("a step is due at its time");
            assert_eq!(wait.is_none(), k == times.len() - 1);
            applied.push(colour["accent"][0]);
        }
        assert_eq!(applied.len(), times.len());
        assert!(applied.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(applied[times.len() - 1], 200, "exactly the new palette");
    }
    #[test]
    fn a_late_step_skips_the_overdue_ones_and_the_last_frame_completes_the_tween() {
        let mut t = tween(0, 200, 0.);
        let (_, wait) = t.advance(0.505).unwrap();
        assert_eq!(
            t.next, 30,
            "steps 1 to 29 were overdue, the one at 500 ms ran"
        );
        assert!((wait.unwrap() - 0.01167).abs() < 1e-4);
        let (colour, wait) = t.advance(f64::INFINITY).unwrap();
        assert_eq!(colour, palette([200, 0, 0, 255]));
        assert!(wait.is_none());
    }
    #[test]
    fn a_retarget_mid_tween_continues_from_the_colour_on_screen() {
        let mut first = tween(0, 200, 0.);
        let (shown, _) = first.advance(0.35).unwrap();
        let mid = shown["accent"][0];
        assert!(mid > 0 && mid < 200);
        // start_palette_tween builds the next tween from the palette that is applied.
        let mut next = PaletteTween::new(
            shown,
            palette([50, 0, 0, 255]),
            WallpaperFade::default(),
            0.35,
            2,
        );
        let (step, _) = next.advance(0.35 + 0.071).unwrap();
        let moved = i32::from(step["accent"][0]) - i32::from(mid);
        let towards = 50 - i32::from(mid);
        assert!(
            (towards..=0).contains(&moved),
            "the first step moves from the shown colour towards the target"
        );
        let (last, _) = next.advance(f64::INFINITY).unwrap();
        assert_eq!(last, palette([50, 0, 0, 255]));
    }
    #[test]
    fn a_palette_only_tween_runs_on_the_timer_joins_retargets_and_cuts_on_reduced_motion() {
        use TweenRoute::*;
        let spec = WallpaperFade {
            duration_ms: 500,
            ..WallpaperFade::default()
        };
        // No fade frame exists: no hold, the timer applies the 30 steps from the start.
        let mut t = PaletteTween::new(
            palette([0, 0, 0, 255]),
            palette([200, 0, 0, 255]),
            spec,
            5.,
            1,
        );
        assert!(!t.waiting && !t.frames_flow(5.));
        assert_eq!(t.steps.len(), 30);
        let (shown, _) = t.advance(5.25).expect("the timer applies a due step");
        // The other output's frame of the same change joins; a different target retargets from
        // the mix on screen; reduced motion or no fade is a cut.
        assert_eq!(tween_route(true, false, true, true), Joins);
        assert_eq!(tween_route(true, false, false, true), Starts);
        let mut next = PaletteTween::new(shown.clone(), palette([50, 0, 0, 255]), spec, 5.25, 2);
        assert_eq!(next.from, shown, "no snap back to the old palette");
        assert!(next.advance(5.25 + 0.2).is_some());
        assert_eq!(plan_fade(Some(spec), true, false), None);
        assert_eq!(tween_route(false, false, false, true), Fence);
    }
    #[test]
    fn a_staged_or_live_change_tweens_only_when_a_fade_runs_and_nothing_older_waits() {
        use TweenRoute::*;
        assert_eq!(tween_route(true, false, false, true), Starts);
        assert_eq!(
            tween_route(true, false, true, true),
            Joins,
            "another output of the same change"
        );
        assert_eq!(tween_route(true, false, false, false), Fence, "no change");
        assert_eq!(
            tween_route(false, false, true, true),
            Fence,
            "reduced motion, GL failed, no fade or a palette group waits"
        );
        assert_eq!(
            tween_route(true, true, true, true),
            Fence,
            "an older change still waits"
        );
    }
    #[test]
    fn a_palette_only_frame_is_adopted_for_resting_or_committed_runs_and_unchanged_palettes() {
        let run = |r, u, c| Some((r, u, c));
        assert!(adopts_palette_only(1, true, false, None));
        assert!(adopts_palette_only(1, true, false, run(true, false, false)));
        assert!(!adopts_palette_only(1, true, false, run(true, true, false)));
        assert!(adopts_palette_only(
            1,
            true,
            false,
            run(false, false, false)
        ));
        assert!(!adopts_palette_only(
            1,
            true,
            false,
            run(false, true, false)
        ));
        assert!(!adopts_palette_only(
            1,
            true,
            false,
            run(false, false, true)
        ));
        assert!(!adopts_palette_only(1, false, false, None));
        assert!(!adopts_palette_only(1, true, true, None));
        assert!(!adopts_palette_only(0, true, false, None));
        use TweenRoute::*;
        assert_eq!(
            palette_only_route(true, false, false, false, true),
            Unchanged
        );
        assert_eq!(palette_only_route(true, false, false, true, false), Starts);
        assert_eq!(palette_only_route(true, false, true, true, false), Joins);
        assert_eq!(
            palette_only_route(true, true, false, false, true),
            Fence,
            "foreign pending"
        );
        assert_eq!(
            palette_only_route(false, false, false, false, true),
            Fence,
            "fence/no fade"
        );
        assert_eq!(
            palette_only_route(true, false, false, false, false),
            Fence,
            "no key yet"
        );
    }
    #[test]
    fn the_first_output_holds_its_first_pose_while_a_partner_is_coming_up_to_the_cap() {
        assert!(hold_first_pose(true, 0.));
        assert!(hold_first_pose(true, HOLD_CAP - 0.001));
        assert!(
            !hold_first_pose(true, HOLD_CAP),
            "a slow partner holds nothing longer"
        );
        assert!(!hold_first_pose(false, 0.), "one output: today's behaviour");
    }
    #[test]
    fn only_another_output_with_a_shown_image_and_a_request_in_flight_is_a_partner() {
        let jobs: std::collections::BTreeSet<String> =
            ["eDP-1", "DP-1", "HDMI-1"].map(String::from).into();
        let surfaces = [("eDP-1", true), ("DP-1", true), ("HDMI-1", false)];
        assert!(partner_coming("eDP-1", &jobs, surfaces), "DP-1 fades too");
        let only_new = [("eDP-1", true), ("HDMI-1", false)];
        assert!(
            !partner_coming("eDP-1", &jobs, only_new),
            "a hotplugged output has no image to fade from"
        );
        let own = std::collections::BTreeSet::from(["eDP-1".to_owned()]);
        assert!(
            !partner_coming("eDP-1", &own, surfaces),
            "its own request never counts"
        );
        assert!(
            !partner_coming("eDP-1", &Default::default(), surfaces),
            "nothing in flight"
        );
    }
    #[test]
    fn held_fades_start_on_one_commit_and_end_together() {
        let mut a = Animations::default();
        let (seconds, curve) = (0.7, Curve::EaseInOut);
        a.open_to("eDP-1", true, 10., seconds, curve);
        // eDP-1 held its first pose (no commit); DP-1 is set up 60 ms later and commits.
        a.open_to("DP-1", true, 10.06, seconds, curve);
        assert_eq!(a.value("eDP-1", 10.05).unwrap().0, 0.);
        a.committed("DP-1", 10.07, 10.07);
        for step in 0..=84 {
            let at = 10.07 + f64::from(step) / 120.;
            assert_eq!(
                a.value("eDP-1", at),
                a.value("DP-1", at),
                "one curve at {at}"
            );
        }
        assert_eq!(a.value("eDP-1", 10.07 + seconds), Some((1., false)));
        assert_eq!(a.value("DP-1", 10.07 + seconds), Some((1., false)));
    }
    #[test]
    fn a_cap_expiry_starts_the_held_fade_alone_and_the_late_one_starts_later() {
        let mut a = Animations::default();
        a.open_to("eDP-1", true, 10., 0.7, Curve::EaseInOut);
        // The cap ended the hold: eDP-1 commits and starts alone.
        a.committed("eDP-1", 10.15, 10.15);
        a.open_to("DP-1", true, 10.3, 0.7, Curve::EaseInOut);
        a.committed("DP-1", 10.31, 10.31);
        assert!(a.value("eDP-1", 10.5).unwrap().0 > a.value("DP-1", 10.5).unwrap().0);
        assert_eq!(a.value("eDP-1", 10.85), Some((1., false)));
        assert!(
            a.value("DP-1", 10.85).unwrap().1,
            "the late output is still fading"
        );
    }
    #[test]
    fn a_retarget_while_held_stays_held_with_the_baked_pose_and_the_last_target_wins() {
        let mut a = Animations::default();
        a.open_to("eDP-1", true, 10., 0.7, Curve::EaseInOut);
        // A newer wallpaper arrives during the hold: the motion is replaced, still held at 0.
        a.retain(|id| id != "eDP-1");
        a.open_to("eDP-1", true, 10.05, 0.7, Curve::EaseInOut);
        assert_eq!(a.value("eDP-1", 10.1).unwrap().0, 0.);
        a.committed("eDP-1", 10.12, 10.12);
        assert_eq!(a.value("eDP-1", 10.12 + 0.7), Some((1., false)));
    }
    #[test]
    fn fades_finish_only_when_every_fade_of_their_clock_drew_its_last_frame() {
        let one = Some(10.07);
        assert!(
            group_done([(one, true)], one),
            "a lone fade finishes at once"
        );
        assert!(!group_done([(one, true), (one, false)], one));
        assert!(group_done([(one, true), (one, true)], one));
        assert!(
            group_done([(one, true), (Some(11.), false), (None, false)], one),
            "another change's fade does not delay it"
        );
    }
    #[test]
    fn the_palette_tween_starts_with_the_shared_clock_and_ends_exactly_on_the_target() {
        let mut t = tween(0, 200, 10.);
        t.waiting = true;
        assert!(
            t.advance(10.5).is_none(),
            "no step while the first pose is held"
        );
        t.start_clock(10.15);
        assert!(
            t.advance(10.15 + 0.01).is_none(),
            "the first step is due 16.7 ms after the clock"
        );
        assert!(t.advance(10.15 + 0.02).is_some());
        let (last, wait) = t.advance(f64::INFINITY).unwrap();
        assert_eq!(last, palette([200, 0, 0, 255]));
        assert!(wait.is_none());
    }
    #[test]
    fn reduced_motion_and_failed_gl_are_a_cut() {
        let spec = Some(WallpaperFade::default());
        assert_eq!(plan_fade(spec, false, false), spec);
        assert_eq!(plan_fade(spec, true, false), None);
        assert_eq!(plan_fade(spec, false, true), None);
        assert_eq!(plan_fade(None, false, false), None);
    }
    #[test]
    fn the_transport_is_released_only_when_nothing_uses_it() {
        assert!(transport_releasable(false, false, true));
        assert!(!transport_releasable(true, false, true), "still fading");
        assert!(
            !transport_releasable(false, true, true),
            "a bar or companion uses GL"
        );
        assert!(
            !transport_releasable(false, false, false),
            "a GL host keeps its context"
        );
    }
}

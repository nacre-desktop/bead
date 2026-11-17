// SPDX-License-Identifier: MIT
//! B0313: GPU crossfade of a wallpaper change at display rate.
//!
//! The wallpaper surface stays on the SHM path. For the length of one change it borrows the
//! shared EGL transport: both images are uploaded first, a held first pose is committed (that
//! commit starts the clock, B0170), every frame callback draws the next blend, and the final
//! SHM buffer is attached before the EGL surface and transport are released again.
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
/// The palette follows the fade in this many steps: a step repaints the bar and open panels on
/// the CPU, so the rate is bounded (about 14 steps per second at the default duration).
pub(crate) const PALETTE_STEPS: usize = 10;

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
    (1..=PALETTE_STEPS)
        .map(|k| {
            let t = k as f32 / PALETTE_STEPS as f32;
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
pub(crate) struct PaletteTween {
    from: PaletteMap,
    pub(crate) to: PaletteMap,
    steps: Vec<(f64, f32)>,
    next: usize,
    generation: u64,
    /// Clock times (seconds): the tween's start and the last fade frame that served it.
    started: f64,
    last_frame: f64,
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
        }
    }
    /// The palette to show at clock `now` and the wait until the following step (`None`: it was the
    /// last). A step that is overdue is skipped for the newest due one: it shows the colour the
    /// wallpaper has reached. `None` when no step is due.
    fn advance(&mut self, now: f64) -> Option<(PaletteMap, Option<f64>)> {
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
    last: Option<(f64, u64)>,
    /// (gap seconds, other main-thread ms) of the largest gap so far.
    worst: (f64, f64),
}
impl GapProbe {
    /// A frame committed at `now` (seconds), drawn from thread CPU `cpu_start` to `cpu_end` (ns).
    pub(crate) fn committed(&mut self, now: f64, cpu_start: u64, cpu_end: u64) {
        if let Some((wall, cpu)) = self.last {
            let gap = now - wall;
            if gap > self.worst.0 {
                self.worst = (gap, cpu_start.saturating_sub(cpu) as f64 / 1e6);
            }
        }
        self.last = Some((now, cpu_end));
    }
    pub(crate) fn other_ms(&self) -> f64 {
        self.worst.1
    }
}

/// One journal line per finished fade, in the PANEL_SLIDE style: what the user saw.
pub(crate) fn transition_marker(
    output: &str,
    kind: WallpaperFadeKind,
    slide: &bead_core::animation::Slide,
    palette_steps: u32,
    gap_other_ms: f64,
) -> String {
    let render_top: Vec<String> = slide
        .render_top
        .iter()
        .filter(|(_, ms)| *ms > 0.)
        .map(|(at, ms)| format!("{at}:{ms:.1}"))
        .collect();
    format!(
        "WALLPAPER_TRANSITION output={output} kind={} frames={} duration_ms={:.1} \
         first_frame_ms={:.1} max_gap_ms={:.1} gap_at={} render_top={} palette_steps={palette_steps} \
         gap_other_ms={gap_other_ms:.1}",
        match kind {
            WallpaperFadeKind::Fade => "fade",
            WallpaperFadeKind::Grow { .. } => "grow",
        },
        slide.frames,
        slide.duration_ms,
        slide.first_frame_ms,
        slide.max_gap_ms,
        slide.gap_at,
        render_top.join(",")
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

    fn palette_tween_route(&self, key: &PaletteMap, output: &str) -> TweenRoute {
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
        tween_route(
            self.fade_wanted()
                && self.wallpaper_palette_outputs.is_none()
                && self.wallpaper_surfaces.iter().any(|s| s.name == output),
            foreign_pending,
            tween_to_key,
            differs,
        )
    }

    /// Whether a staged frame waits for its confirmation to tween instead of building the fence.
    pub(super) fn staged_waits_for_tween(&self, key: &PaletteMap, output: &str) -> bool {
        self.palette_tween_route(key, output) != TweenRoute::Fence
    }

    /// Whether this change swaps the palette with a fade instead of the palette fence.
    pub(super) fn install_with_palette_tween(&mut self, key: &PaletteMap, output: &str) -> bool {
        let route = self.palette_tween_route(key, output);
        if route == TweenRoute::Starts {
            self.pending_palette_target = Some(key.clone());
        }
        route != TweenRoute::Fence
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
            self.apply_palette_step(target);
        }
    }

    fn start_palette_tween(&mut self, to: PaletteMap, spec: WallpaperFade) {
        let from = self.wallpaper_palette.clone().unwrap_or_else(|| to.clone());
        self.palette_tween_generation += 1;
        let generation = self.palette_tween_generation;
        let now = self.started.elapsed().as_secs_f64();
        let tween = PaletteTween::new(from, to, spec, now, generation);
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
        let tween = self
            .palette_tween
            .as_mut()
            .filter(|t| t.generation == generation)?;
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
        let fading = self.wallpaper_surfaces.iter().any(|s| s.fade.is_some());
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
        {
            run.palette_steps += 1;
        }
        self.apply_palette_step(palette);
    }

    /// One palette step through the same recipients as a fenced change, without the fence.
    fn apply_palette_step(&mut self, palette: PaletteMap) {
        let _span = bead_core::profile::Span::new("wallpaper_palette_step");
        self.apply_wallpaper_palette(palette.clone(), true);
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
        let provider_spec = self.options.wallpaper.as_ref().and_then(|p| p.fade(&name));
        let Some(spec) = plan_fade(
            provider_spec.or(self.options.wallpaper_fade),
            bead_core::animation::reduced_motion(),
            self.wallpaper_gl_failed,
        ) else {
            // A running fade ends at its old target; the cut then replaces it.
            self.finish_wallpaper_fade(index);
            return Err(frame);
        };
        let size = (frame.width, frame.height);
        if self.wallpaper_surfaces[index].fade.is_some() {
            let surface = &mut self.wallpaper_surfaces[index];
            let run = surface.fade.as_mut().unwrap();
            let result = (|| -> Result<(), Box<dyn Error>> {
                let transport = self.gl.as_ref().ok_or("GL transport missing")?;
                transport.activate(&run.gl, size.0, size.1)?;
                run.fader.retarget(&frame.rgba)
            })();
            if result.is_err() {
                self.wallpaper_surfaces[index].fade = None;
                self.release_wallpaper_gl();
                return Err(frame);
            }
            let run = self.wallpaper_surfaces[index].fade.as_mut().unwrap();
            run.target = std::mem::take(&mut frame.rgba);
            run.generation += 1;
            run.gaps = GapProbe::default();
            run.spec = spec;
            run.palette = frame.palette.clone();
            let replaced = run
                .commit
                .replace((frame.revision, frame.commit_token.clone()));
            if let (Some((revision, _)), Some(provider)) = (replaced, &self.options.wallpaper) {
                provider.discarded(&name, revision);
            }
        } else {
            let Some(old) = self
                .shown_wallpaper_rgba(index)
                .or_else(|| frame.previous_rgba.take())
                .filter(|old| old.len() == frame.rgba.len())
            else {
                return Err(frame);
            };
            match self.create_fade_run(index, &frame, &old, spec) {
                Ok(run) => self.wallpaper_surfaces[index].fade = Some(run),
                Err(error) => {
                    self.wallpaper_gl_failed = true;
                    eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
                    self.release_wallpaper_gl();
                    return Err(frame);
                }
            }
        }
        let surface = &mut self.wallpaper_surfaces[index];
        surface.staged = None;
        surface.transition = None;
        surface.pixels = None;
        surface.palette = Some(frame.palette);
        surface.release_wait_ms = 0;
        surface.revision = frame.revision;
        let generation = surface.fade.as_ref().map_or(0, |run| run.generation);
        let now = self.started.elapsed().as_secs_f64();
        let cpu_start = bead_core::profile::thread_cpu_ns();
        // Replacing the motion restarts the curve at its held first pose.
        self.fade_animations.retain(|id| id != name);
        self.fade_animations.open_to(
            &name,
            true,
            now,
            f64::from(spec.duration_ms) / 1000.,
            spec.curve,
        );
        if let Err(error) = self.fade_draw(index, 0., true) {
            self.wallpaper_gl_failed = true;
            eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
            self.finish_wallpaper_fade(index);
            return Ok(());
        }
        self.fade_committed(index, now, cpu_start);
        self.schedule_fade_watchdog(&name, generation, spec.duration_ms);
        if let Some(target) = self.pending_palette_target.take() {
            self.start_palette_tween(target, spec);
        }
        Ok(())
    }

    fn create_fade_run(
        &mut self,
        index: usize,
        frame: &WallpaperFrame,
        old: &[u8],
        spec: WallpaperFade,
    ) -> Result<FadeRun, Box<dyn Error>> {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;
        let (width, height) = (frame.width, frame.height);
        let surface = &self.wallpaper_surfaces[index];
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr().cast())
                .ok_or("native display missing")?,
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.layer.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        if self.gl.is_none() {
            // SAFETY: the retained connection outlives the EGL context and surfaces.
            self.gl = Some(unsafe { bead_render::gl::Transport::new(display) }?);
        }
        let transport = self.gl.as_mut().unwrap();
        // SAFETY: FadeRun drops the EGL surface before the wallpaper layer surface.
        let gl = unsafe { transport.create_surface(window, width, height) }?;
        let fader = bead_render::gl::WallpaperFader::new(
            &transport.display,
            (width, height),
            old,
            &frame.rgba,
        )?;
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
            size: (width, height),
            spec,
            target: frame.rgba.clone(),
            commit: Some((frame.revision, frame.commit_token.clone())),
            palette: frame.palette.clone(),
            generation: 1,
            palette_steps: 0,
            gaps: GapProbe::default(),
        })
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
        run.fader.draw(blend, mode);
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
    fn fade_committed(&mut self, index: usize, drawn: f64, cpu_start: u64) {
        let now = self.started.elapsed().as_secs_f64();
        let name = self.wallpaper_surfaces[index].name.clone();
        let slide = self.fade_animations.committed_timed(
            &name,
            drawn,
            now,
            None,
            drawn,
            &bead_core::profile::NO_SPLITS,
        );
        let surface = &mut self.wallpaper_surfaces[index];
        if let Some(run) = surface.fade.as_mut() {
            run.gaps
                .committed(now, cpu_start, bead_core::profile::thread_cpu_ns());
        }
        if let (Some(slide), Some(run)) = (slide, &surface.fade) {
            println!(
                "{}",
                transition_marker(
                    &name,
                    run.spec.kind,
                    &slide,
                    run.palette_steps,
                    run.gaps.other_ms()
                )
            );
        }
        if let Some((revision, token)) = surface.fade.as_mut().and_then(|run| run.commit.take()) {
            self.wallpaper_committed(&name, revision, &token, true);
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
        {
            return true;
        }
        let name = self.wallpaper_surfaces[index].name.clone();
        let drawn = self.started.elapsed().as_secs_f64();
        let cpu_start = bead_core::profile::thread_cpu_ns();
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
            self.finish_wallpaper_fade(index);
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

    /// Attach the final SHM buffer, then give the EGL surface and transport back.
    pub(super) fn finish_wallpaper_fade(&mut self, index: usize) {
        let Some(run) = self.wallpaper_surfaces[index].fade.take() else {
            return;
        };
        let FadeRun {
            fader,
            gl,
            target,
            mut commit,
            palette,
            ..
        } = run;
        drop(fader);
        if let Some(transport) = &self.gl {
            let _ = transport.unbind();
        }
        drop(gl);
        let output_handle = self.wallpaper_surfaces[index].output.clone();
        let logical = self
            .output
            .info(&output_handle)
            .and_then(|info| info.logical_size);
        let provider = self.options.wallpaper.clone();
        let name = self.wallpaper_surfaces[index].name.clone();
        let surface = &mut self.wallpaper_surfaces[index];
        self.fade_animations.retain(|id| id != name);
        let attached = attach_wallpaper_buffer(
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
        );
        match attached {
            Ok(selected) => surface.attached = Some(selected),
            Err(_) => {
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
        self.schedule_wallpaper_release_poll(name);
        self.release_wallpaper_gl();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: plain libc call; returns the freed fade copies to the system.
        unsafe {
            libc::malloc_trim(0);
        }
    }

    /// Give the shared EGL transport back once no fade or other GL surface needs it.
    pub(super) fn release_wallpaper_gl(&mut self) {
        let fading = self.wallpaper_surfaces.iter().any(|s| s.fade.is_some());
        if !transport_releasable(
            fading,
            self.has_gl_surfaces(),
            self.options.renderer == RendererKind::Cpu,
        ) {
            return;
        }
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
        assert_eq!(steps.len(), PALETTE_STEPS);
        assert!((steps[PALETTE_STEPS - 1].0 - 0.7).abs() < 1e-9);
        assert_eq!(steps[PALETTE_STEPS - 1].1, 1.);
        assert!(
            steps
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1)
        );
        assert!(
            (steps[PALETTE_STEPS / 2 - 1].1 - 0.5).abs() < 1e-6,
            "half time, half way"
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
        );
        assert!(line.starts_with("WALLPAPER_TRANSITION output=DP-1 kind=grow frames="));
        assert!(
            line.contains("first_frame_ms=52.0") && line.contains("max_gap_ms=2"),
            "{line}"
        );
        assert!(
            line.ends_with(" palette_steps=10 gap_other_ms=3.5"),
            "{line}"
        );
        assert!(line.contains(" render_top="));
    }
    #[test]
    fn the_gap_probe_charges_the_largest_gap_with_the_other_cpu_time_before_its_frame() {
        let ms = 1_000_000;
        let mut p = GapProbe::default();
        p.committed(0., 0, 2 * ms);
        // 8 ms gap, 0.5 ms of other work, the frame itself 2 ms.
        p.committed(0.008, 2 * ms + ms / 2, 4 * ms);
        assert!((p.other_ms() - 0.5).abs() < 1e-9);
        // A 16 ms gap with 9 ms of other work wins; a later shorter gap does not.
        p.committed(0.024, 13 * ms, 15 * ms);
        p.committed(0.032, 30 * ms, 31 * ms);
        assert!((p.other_ms() - 9.).abs() < 1e-9);
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
    #[test]
    fn a_step_due_during_a_fade_is_applied_after_a_frame_not_by_the_timer() {
        let mut t = tween(0, 200, 10.);
        assert!(t.advance(10.06).is_none(), "the first step is due at 70 ms");
        t.last_frame = 10.065;
        assert!(t.frames_flow(10.07), "frames flow: the timer defers");
        assert!(t.retry_after(10.07) >= 0.01, "and looks again soon");
        let (colour, wait) = t.advance(10.073).expect("the frame applies the due step");
        assert!(colour["accent"][0] > 0 && colour["accent"][0] < 100);
        assert!(
            (wait.unwrap() - 0.067).abs() < 1e-6,
            "wait for the second step"
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
            assert_eq!(wait.is_none(), k == PALETTE_STEPS - 1);
            applied.push(colour["accent"][0]);
        }
        assert_eq!(applied.len(), PALETTE_STEPS);
        assert!(applied.windows(2).all(|w| w[0] < w[1] || w[1] == 200));
        assert_eq!(applied[PALETTE_STEPS - 1], 200, "exactly the new palette");
    }
    #[test]
    fn a_late_step_skips_the_overdue_ones_and_the_last_frame_completes_the_tween() {
        let mut t = tween(0, 200, 0.);
        let (_, wait) = t.advance(0.5).unwrap();
        assert_eq!(
            t.next, 7,
            "steps 1 to 6 were overdue, the one at 490 ms ran"
        );
        assert!((wait.unwrap() - 0.06).abs() < 1e-6);
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

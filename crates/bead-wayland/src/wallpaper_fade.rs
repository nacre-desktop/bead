// SPDX-License-Identifier: MIT
//! B0313: GPU crossfade of a wallpaper change at display rate.
//!
//! The wallpaper surface stays on the SHM path. For the length of one change it borrows the
//! shared EGL transport: both images are uploaded first, a held first pose is committed (that
//! commit starts the clock, B0170), every frame callback draws the next blend, and the final
//! SHM buffer is attached before the EGL surface and transport are released again.
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
        self.fade_committed(index, now);
        self.schedule_fade_watchdog(&name, generation, spec.duration_ms);
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
    fn fade_committed(&mut self, index: usize, drawn: f64) {
        let now = self.started.elapsed().as_secs_f64();
        let name = self.wallpaper_surfaces[index].name.clone();
        self.fade_animations.committed_timed(
            &name,
            drawn,
            now,
            None,
            drawn,
            &bead_core::profile::NO_SPLITS,
        );
        let surface = &mut self.wallpaper_surfaces[index];
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
        let (blend, running) = self
            .fade_animations
            .value(&name, drawn)
            .unwrap_or((1., false));
        if self.fade_draw(index, blend, false).is_err() {
            self.wallpaper_gl_failed = true;
            self.finish_wallpaper_fade(index);
            return true;
        }
        self.fade_committed(index, drawn);
        if !running {
            self.finish_wallpaper_fade(index);
        }
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

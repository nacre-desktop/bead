// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-surface rendering, GPU staging and idle release.
use super::*;

/// Hidden content never requests a keyboard, including retained warm panels.
fn panel_keyboard_mode(visible: bool, click_focus: bool, exclusive: bool) -> KeyboardInteractivity {
    if !visible {
        KeyboardInteractivity::None
    } else if click_focus || !exclusive {
        KeyboardInteractivity::OnDemand
    } else {
        KeyboardInteractivity::Exclusive
    }
}

#[cfg(test)]
mod keyboard_policy_tests {
    use super::*;

    #[test]
    fn warm_updates_never_reacquire_keyboard_and_pinning_changes_mode() {
        for pinned in [false, true] {
            for typing in [false, true] {
                assert_eq!(
                    panel_keyboard_mode(false, pinned, typing),
                    KeyboardInteractivity::None
                );
            }
        }
        assert_eq!(
            panel_keyboard_mode(true, false, true),
            KeyboardInteractivity::Exclusive
        );
        assert_eq!(
            panel_keyboard_mode(true, true, true),
            KeyboardInteractivity::OnDemand
        );
    }
}

fn content_body(scene: &Scene) -> Option<bead_render::Rect> {
    scene
        .layers
        .iter()
        .find(|layer| layer.parent.is_none())
        .map(|layer| layer.bounds)
        .or_else(|| {
            scene
                .primitives
                .iter()
                .find_map(|primitive| match primitive {
                    bead_render::Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                        Some(*rect)
                    }
                    _ => None,
                })
        })
}

/// Native window geometry describes the complete body, even during its reveal.
pub(super) fn content_window_rect(scene: &Scene) -> Option<[i32; 4]> {
    let body = content_body(scene).filter(|body| body.w > 0. && body.h > 0.)?;
    let body = if scene.motion.is_some() {
        body.translated(-scene.bounds.x, -scene.bounds.y)
    } else {
        body.translated(scene.offset.0, scene.offset.1)
    };
    let (x, y) = (body.x.floor(), body.y.floor());
    Some([
        x as i32,
        y as i32,
        ((body.x + body.w).ceil() - x).max(0.) as i32,
        ((body.y + body.h).ceil() - y).max(0.) as i32,
    ])
}

/// Shadows reserve buffer space, but must not intercept adjacent controls.
/// Retained layout bounds survive a shell replacing its background primitive.
pub(super) fn content_input_rect(scene: &Scene) -> Option<[i32; 4]> {
    let body = content_body(scene)?;
    let painted = body.translated(scene.offset.0, scene.offset.1);
    // An entering panel may slide across adjacent surfaces. Keep hit testing
    // inside its final content footprint throughout the animation.
    let clip = if scene.motion.is_some() {
        body.translated(-scene.bounds.x, -scene.bounds.y)
    } else {
        painted
    };
    let x = painted.x.max(clip.x).floor();
    let y = painted.y.max(clip.y).floor();
    let right = (painted.x + painted.w).min(clip.x + clip.w).ceil();
    let bottom = (painted.y + painted.h).min(clip.y + clip.h).ceil();
    Some([
        x as i32,
        y as i32,
        (right - x).max(0.) as i32,
        (bottom - y).max(0.) as i32,
    ])
}

/// Adopt only pixels the following complete repairs will leave untouched.
fn copy_outside_repairs(
    source: &[u8],
    target: &mut [u8],
    width: u32,
    height: u32,
    repairs: &[Damage],
    zero_target: bool,
) {
    let mut ordered = repairs.iter().collect::<Vec<_>>();
    ordered.sort_unstable_by_key(|r| r.x);
    for y in 0..height {
        let row = y as usize * width as usize * 4;
        let mut x = 0;
        for r in &ordered {
            if y < r.y || y >= r.y.saturating_add(r.h) || r.w == 0 {
                continue;
            }
            let left = r.x.min(width);
            if left > x {
                let range = row + x as usize * 4..row + left as usize * 4;
                copy_adopted_span(&source[range.clone()], &mut target[range], zero_target);
            }
            x = x.max(r.x.saturating_add(r.w).min(width));
        }
        if x < width {
            let range = row + x as usize * 4..row + width as usize * 4;
            copy_adopted_span(&source[range.clone()], &mut target[range], zero_target);
        }
    }
}

fn copy_adopted_span(source: &[u8], target: &mut [u8], zero_target: bool) {
    if !zero_target {
        target.copy_from_slice(source);
        return;
    }
    static ZERO: [u8; 4096] = [0; 4096];
    for (source, target) in source.chunks(ZERO.len()).zip(target.chunks_mut(ZERO.len())) {
        // Fresh slots in a new SHM pool are zero-filled. Avoid faulting their
        // transparent margins into residency just to copy zeroes.
        // SAFETY: both read ranges contain source.len() bytes (at most 4096).
        if unsafe { libc::memcmp(source.as_ptr().cast(), ZERO.as_ptr().cast(), source.len()) } != 0
        {
            target.copy_from_slice(source);
        }
    }
}

/// Best-effort huge-page advice on the mapped pages touched by an existing frame.
/// Adjacent slots must share advice on their boundary page, so the kernel can
/// merge their VMAs and use huge pages across slot boundaries.
fn advise_frame_pages(bytes: &mut [u8], include_edges: bool) {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: sysconf has no pointer or process-state preconditions.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page <= 0 {
            return;
        }
        let page = page as usize;
        let start = bytes.as_mut_ptr() as usize;
        let end = start + bytes.len();
        let (begin, end) = if include_edges {
            (start / page * page, end.next_multiple_of(page))
        } else {
            (start.next_multiple_of(page), end / page * page)
        };
        if end > begin {
            // SAFETY: SlotPool maps whole OS pages. Rounding a live slot to
            // its containing pages stays in that mapping; advice does not
            // read or write other slots sharing the first/last page.
            // Failure merely retains ordinary small-page allocation behavior.
            unsafe {
                libc::madvise(begin as *mut _, end - begin, libc::MADV_HUGEPAGE);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (bytes, include_edges);
}

/// Quiet time before an idle SHM pool is compacted. A surface that holds keyboard
/// focus (an open launcher) waits longer: the retire-and-repaint costs the next
/// key ~2.4 ms of UI CPU (B0040), and hiding the panel destroys its pool anyway.
/// Set `SHM_COMPACT_FOCUSED_IDLE` to `SHM_COMPACT_IDLE` for the previous behaviour.
const SHM_COMPACT_IDLE: Duration = Duration::from_millis(250);
const SHM_COMPACT_FOCUSED_IDLE: Duration = Duration::from_secs(5);
fn shm_compact_idle(surface: &Surface) -> Duration {
    if surface
        .keyboard_visible
        .is_some_and(|mode| mode != KeyboardInteractivity::None)
    {
        SHM_COMPACT_FOCUSED_IDLE
    } else {
        SHM_COMPACT_IDLE
    }
}
impl App {
    fn trim_idle_shm(&mut self, index: usize) {
        let surface = &mut self.surfaces[index];
        if surface.shm_trim_pending
            || surface.buffers.is_empty()
            || (surface.heap_trimmed_ui_serial == surface.ui_serial
                && surface
                    .buffers
                    .iter()
                    .filter(|buffer| buffer.painted)
                    .count()
                    <= 1)
        {
            return;
        }
        surface.shm_trim_pending = true;
        let identity = surface.wl_surface().clone();
        let qh = self.qh.clone();
        let expires = Instant::now() + SHM_COMPACT_FOCUSED_IDLE + Duration::from_secs(3);
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(shm_compact_idle(surface)),
            move |_, _, app| {
                // The surface may belong to a pearl that is not swapped in right now.
                app.on_surface(&identity, |app| {
                    let Some(index) = app
                        .surfaces
                        .iter()
                        .position(|s| s.wl_surface() == &identity)
                    else {
                        return TimeoutAction::Drop;
                    };
                    let surface = &mut app.surfaces[index];
                    if surface.callbacks.pending
                        || surface
                            .last_callback
                            .is_none_or(|last| last.elapsed() < shm_compact_idle(surface))
                    {
                        if Instant::now() < expires {
                            let quiet = shm_compact_idle(surface);
                            let wait = surface
                                .last_callback
                                .map_or(quiet, |last| quiet.saturating_sub(last.elapsed()));
                            return TimeoutAction::ToDuration(wait.max(Duration::from_millis(100)));
                        }
                        surface.shm_trim_pending = false;
                        return TimeoutAction::Drop;
                    }
                    surface.shm_trim_pending = false;
                    surface.heap_trimmed_ui_serial = surface.ui_serial;
                    if surface
                        .buffers
                        .iter()
                        .filter(|buffer| buffer.painted)
                        .count()
                        <= 1
                    {
                        #[cfg(all(target_os = "linux", target_env = "gnu"))]
                        unsafe {
                            libc::malloc_trim(0);
                        }
                        return TimeoutAction::Drop;
                    }
                    // An idle burst can have touched several reusable slots. Retire
                    // that pool and repaint one current frame, instead of retaining
                    // spare frame pages indefinitely. Active old buffers remain
                    // compositor-owned until release, as during a geometry change.
                    let pool = match SlotPool::new(4096, &app.shm) {
                        Ok(pool) => pool,
                        Err(error) => {
                            app.fail(error);
                            return TimeoutAction::Drop;
                        }
                    };
                    surface.buffers.clear();
                    surface.pool = pool;
                    surface.previous = None;
                    surface.visual_damage = Default::default();
                    surface.redraw = true;
                    bead_core::vprintln!("SHM_POOL_COMPACT output={}", surface.output_name);
                    app.draw(index, &qh);
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    unsafe {
                        libc::malloc_trim(0);
                    }
                    TimeoutAction::Drop
                })
                .unwrap_or(TimeoutAction::Drop)
            },
        ) {
            self.surfaces[index].shm_trim_pending = false;
            self.fail(error);
        }
    }
    pub(super) fn update_companion_policy(
        &mut self,
        index: usize,
        qh: &QueueHandle<Self>,
    ) -> Result<(), Box<dyn Error>> {
        if !self.options.lazy_gl
            || !self.surfaces[index].companion
            || !self.surfaces[index].configured
            || self.surfaces[index].callbacks.pending
        {
            return Ok(());
        }
        let desired = self.options.companion.as_ref().map(|(_, kind)| *kind);
        if desired != Some(RendererKind::FemtovgGl) {
            return Ok(());
        }
        let now = self.started.elapsed().as_secs_f64();
        // Moving a cached surface does not need a cold GPU upgrade. Keep an
        // existing GL renderer warm during prewarm, but let an idle CPU panel
        // hand off and return using its already rendered pixels.
        let animated = self.options.animate
            || (self.panel_dock_transition.is_none() && self.animations.active(now));
        if self.companion_policy.frame(now, animated) {
            if let Some((_, renderer, kind)) = &mut self.companion {
                *renderer = bead_render::renderer(RendererKind::FemtovgGl)?;
                *kind = RendererKind::FemtovgGl;
            }
            self.surfaces[index].visual_damage = Default::default();
            println!("RENDER_POLICY companion=true transition=cpu-to-gl");
            // A retained panel keeps its prepared renderer along with its
            // worker/layer. Replacing that single panel releases all three.
            if !self.release_timer
                && !self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_some_and(|spec| spec.retain_when_closed)
            {
                let qh = qh.clone();
                self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(2)), move |_, _, app| {
                    if !app.release_timer||app.companion.is_none(){return TimeoutAction::Drop;}
                    let now = app.started.elapsed().as_secs_f64();
                    if app.panel_target
                        && app
                            .surfaces
                            .iter()
                            .any(|surface| surface.companion && surface.pointer_visual.is_some())
                    {
                        app.companion_policy.touch(now);
                        return TimeoutAction::ToDuration(Duration::from_secs_f64(
                            app.companion_policy.remaining(now),
                        ));
                    }
                    if app.surfaces.iter().any(|surface| surface.prepared_native.is_some()) {
                        app.companion_policy.frame(now, true);
                    }
                    if app.companion_policy.expire(now) {
                        if let Some((_, renderer, kind)) = &mut app.companion {
                            match bead_render::renderer(RendererKind::Cpu) {
                                Ok(cpu) => { *renderer = cpu; *kind = RendererKind::Cpu; }
                                Err(error) => { app.fail(error); return TimeoutAction::Drop; }
                            }
                        }
                        #[cfg(any(feature="native-gl",feature="wgpu"))]
                        {
                            for surface in app.surfaces.iter_mut().filter(|s| s.companion) {
                                #[cfg(feature="native-gl")]
                                {
                                    drop(surface.gl.take());
                                    surface.gl_prepared = false;
                                    surface.gl_prepare_pending = false;
                                }
                                #[cfg(feature="wgpu")]
                            drop(surface.gpu.take());
                                surface.visual_damage = Default::default();
                                surface.callbacks.invalidate();
                            }
                            #[cfg(feature="native-gl")]
                            if !app.has_gl_surfaces() && app.options.renderer == RendererKind::Cpu
                                && let Some(transport) = app.gl.take() {
                                    // SAFETY: the CPU bar/companion own no GL resources;
                                    // this process owns one EGL display and all EGL surfaces are gone.
                                    if let Err(error) = unsafe { transport.shutdown_exclusive() } {
                                        app.fail(error); return TimeoutAction::Drop;
                                    }
                            }
                            #[cfg(feature="wgpu")]
                            if !app.has_gpu_surfaces() { app.gpu = None; }
                        }
                        println!("RENDER_POLICY companion=true transition=gl-to-cpu shared_context_released={}", app.options.renderer == RendererKind::Cpu);
                        app.release_timer = false;
                        let indices: Vec<_> = app.surfaces.iter().enumerate().filter(|(_, s)| s.companion).map(|(i, _)| i).collect();
                        for i in indices { app.draw(i, &qh); }
                        TimeoutAction::Drop
                    } else {
                        TimeoutAction::ToDuration(Duration::from_secs_f64(app.companion_policy.remaining(now)))
                    }
                })?;
                self.release_timer = true;
            }
        } else if self.surfaces[index].frames == 0 {
            self.surfaces[index].redraw = true;
            println!("RENDER_POLICY companion=true first_frame=cpu");
        }
        Ok(())
    }

    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    pub(super) fn ensure_gpu(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;
        let surface = &mut self.surfaces[index];
        let (w, h, _, _) = buffer_dimensions(surface.width, surface.height, surface.scale120)?;
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr().cast())
                .ok_or("native display missing")?,
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        #[cfg(feature = "native-gl")]
        if self.options.renderer == RendererKind::FemtovgGl {
            if self.gl.is_none() {
                // SAFETY: the retained connection outlives the EGL context/surfaces.
                self.gl = Some(unsafe { bead_render::gl::Transport::new(display) }?);
            }
            if surface.gl.is_none() {
                bead_core::vprintln!(
                    "BUFFER companion={} logical={}x{} physical={}x{} scale={} transport=GL",
                    surface.companion,
                    surface.width,
                    surface.height,
                    w,
                    h,
                    surface.scale120 as f32 / 120.
                );
                // SAFETY: Surface::drop drops EGL before its wl_surface.
                surface.gl = Some(unsafe {
                    self.gl
                        .as_mut()
                        .unwrap()
                        .create_surface(window, w as u32, h as u32)
                }?);
            }
            return Ok(());
        }
        #[cfg(feature = "wgpu")]
        {
            if self.gpu.is_none() {
                self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
            }
            if surface.gpu.is_none() {
                // SAFETY: Surface::drop drops swapchains before their protocol objects.
                surface.gpu = Some(unsafe {
                    self.gpu.as_mut().unwrap().create_surface(
                        display,
                        window,
                        w as u32,
                        h as u32,
                        self.options.renderer,
                    )
                }?);
            }
        }
        Ok(())
    }
    pub(super) fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        if self.hidden {
            return;
        }
        if self.wallpaper_palette_outputs.is_some() {
            self.surfaces[index].redraw = true;
            return;
        }
        if self.surfaces[index].companion {
            // A worker failure publishes an error scene. A loading worker must never
            // expose the fallback buffer, even when hydration takes longer than a frame.
            if !self.panel_ready {
                self.surfaces[index].redraw = true;
                return;
            }
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
            let previous_frames = self.surfaces[index].frames;
            let original = if !self.surfaces[index].callbacks.pending {
                self.pointer_visual_position(index);
                self.pointer_visual_scene(index)
                    .map(|scene| std::mem::replace(&mut self.companion.as_mut().unwrap().0, scene))
            } else {
                None
            };
            self.draw_current(index, qh);
            if let Some(scene) = original {
                self.companion.as_mut().unwrap().0 = scene;
            }
            let pending = (self.surfaces[index].frames > previous_frames)
                .then(|| self.pointer_visual_retire(index));
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
            #[cfg(feature = "native-gl")]
            if self.surfaces[index].gl_prepare_pending {
                self.surfaces[index].gl_prepare_pending = false;
                let unprepared: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, surface)| {
                        surface.companion && surface.configured && !surface.gl_prepared
                    })
                    .map(|(index, _)| index)
                    .collect();
                if let Some(index) = unprepared.first().copied() {
                    self.draw(index, qh);
                } else {
                    if self
                        .surfaces
                        .iter()
                        .filter(|surface| surface.companion)
                        .all(|surface| surface.frames == 0)
                    {
                        // GL uploads happened without a present; begin reveal only
                        // after the prepared frame can actually be shown.
                        self.panel_content_ready();
                    }
                    let prepared: Vec<_> = self
                        .surfaces
                        .iter()
                        .enumerate()
                        .filter(|(_, surface)| surface.companion && surface.configured)
                        .map(|(index, _)| index)
                        .collect();
                    for index in prepared {
                        self.draw(index, qh);
                    }
                }
            }
            if self.surfaces.get(index).is_some_and(|surface| {
                surface.frames > previous_frames
                    && surface
                        .pointer_visual
                        .as_ref()
                        .is_some_and(pointer_motion::Visual::has_painted_feedback)
            }) {
                // A label repaint can submit the latest child pose too. Flush
                // its matching local value just as the pose-only path does.
                self.pointer_visual_flush(index);
            }
            if self.surfaces[index].companion
                && self.surfaces[index].frames > previous_frames
                && self.pearl.is_none()
            {
                // Admission prewarm follows the accepted source frame and restored pointer scene.
                self.prewarm_resident_panel();
            }
            for event in pending.into_iter().flatten() {
                self.view_event(true, event);
            }
            return;
        }
        if self.options.primary_factory.is_some() && !self.surfaces[index].companion {
            let id = self.surfaces[index].output_name.clone();
            if self.active_output.as_deref() != Some(id.as_str()) {
                if let Some(mut session) = self.output_sessions.remove(&id) {
                    let previous_output = self.active_output.replace(id.clone());
                    let previous_epoch = self.active_output_epoch.replace(session.epoch);
                    std::mem::swap(&mut self.runtime, &mut session.source);
                    std::mem::swap(&mut self.scene, &mut session.scene);
                    std::mem::swap(&mut self.animations, &mut session.animations);
                    let previous_serial = std::mem::replace(&mut self.ui_serial, session.ui_serial);
                    let previous_primary_serial =
                        std::mem::replace(&mut self.primary_ui_serial, session.ui_serial);
                    self.draw(index, qh);
                    session.ui_serial = std::mem::replace(&mut self.ui_serial, previous_serial);
                    self.primary_ui_serial = previous_primary_serial;
                    std::mem::swap(&mut self.animations, &mut session.animations);
                    std::mem::swap(&mut self.scene, &mut session.scene);
                    std::mem::swap(&mut self.runtime, &mut session.source);
                    self.active_output = previous_output;
                    self.active_output_epoch = previous_epoch;
                    self.output_sessions.insert(id, session);
                }
                return;
            }
        }
        self.draw_current(index, qh);
    }
    fn draw_current(&mut self, index: usize, qh: &QueueHandle<Self>) {
        if self.surfaces[index].companion
            && self
                .panel_cover
                .as_ref()
                .is_some_and(|(serial, _)| self.panel_ui_serial < *serial)
        {
            return;
        }
        if self.surfaces[index]
            .handoff
            .as_ref()
            .is_some_and(|h| !h.gate.can_draw())
        {
            return;
        }
        if self.surfaces[index].companion
            && self.panel_dock_transition.is_some()
            && let Some((scene, _, _)) = self.companion.as_ref()
        {
            let output = self.surfaces[index].output.clone();
            self.position_panel_surface(&self.surfaces[index].layer, &output, scene);
        }
        let decoration_result = if self.surfaces[index].companion {
            self.sync_all_decorations()
        } else {
            self.sync_decorations(index)
        };
        if let Err(error) = decoration_result {
            self.fail(error);
            return;
        }
        let frame_started = Instant::now();
        if let Err(error) = self.update_companion_policy(index, qh) {
            self.fail(error);
            return;
        }
        let previous_frames = self.surfaces[index].frames;
        let mut busy = false;
        // (motion id, clock of this frame) once this frame is the first to show a motion.
        let mut motion_clock: Option<(String, f64)> = None;
        let result = (|| -> Result<(), Box<dyn Error>> {
            let s = &mut self.surfaces[index];
            let wl_surface = s.wl_surface().clone();
            if s.configured
                && !s.callbacks.pending
                && let Some(handoff) = s
                    .handoff
                    .as_ref()
                    .filter(|handoff| handoff.has_prepared_frame())
            {
                let generation = self
                    .companion
                    .as_ref()
                    .map_or(self.scene.resource_revision, |(scene, _, _)| {
                        scene.resource_revision
                    });
                let source_epoch = if s.companion { self.panel_epoch } else { 0 };
                let (w, h, _, _) = buffer_dimensions(s.width, s.height, s.scale120)?;
                wl_surface.damage_buffer(0, 0, w, h);
                wl_surface.frame(
                    qh,
                    FrameData {
                        surface: wl_surface.clone(),
                        epoch: s.callbacks.request(),
                        generation,
                        source_epoch,
                        prepared_frame: None,
                    },
                );
                s.callbacks.pending = true;
                s.redraw = false;
                s.presented_generation = None;
                if let Some(presentation) = &self.presentation {
                    presentation.feedback(
                        &wl_surface,
                        qh,
                        PresentationData {
                            source_epoch,
                            update: s.ui_serial,
                            surface: wl_surface.clone(),
                            generation,
                        },
                    );
                }
                bead_core::profile::mark("panel_commit");
                if handoff.first_frame_attached {
                    wl_surface.commit();
                } else if let Some(first_frame) = handoff.first_frame.as_ref() {
                    first_frame.attach_to(&wl_surface)?;
                    wl_surface.commit();
                }
                #[cfg(feature = "native-gl")]
                if handoff.first_gl_frame {
                    let transport = self.gl.as_ref().ok_or("missing prepared GL transport")?;
                    let target = s.gl.as_ref().ok_or("missing prepared GL surface")?;
                    transport.activate(target, w as u32, h as u32)?;
                    transport.present(target, &[Damage::full(w as u32, h as u32)])?;
                }
                s.frames += 1;
                return Ok(());
            }
            let (scene, renderer, _kind, panel) = if s.companion {
                let (scene, renderer, kind) =
                    self.companion.as_mut().ok_or("missing companion scene")?;
                (scene, renderer, *kind, true)
            } else {
                (
                    &mut self.scene,
                    &mut self.renderer,
                    self.options.renderer,
                    self.options.panel,
                )
            };
            if !s.configured {
                return Ok(());
            }
            if s.callbacks.pending {
                s.redraw = true;
                return Ok(());
            }
            s.redraw = false;
            let mut opaque_motion = self.options.animate;
            let mut reveal_progress = None;
            if let Some(motion) = &scene.motion {
                let now = self.started.elapsed().as_secs_f64();
                let open = if self.options.animate {
                    ((now - 3.).max(0.) % 2.) >= 1. || now < 3.
                } else if s.companion && self.options.panel_spec.is_some() {
                    self.panel_target
                } else {
                    motion.open
                };
                // B0067: a panel opens from closed. A fresh animation set would take the first target as where the
                // panel already is and show it fully open at once (the cold open never animated).
                if open && self.animations.value(&motion.id, now).is_none() {
                    self.animations.target(
                        &motion.id,
                        0.,
                        now,
                        0.001,
                        motion.curve.unwrap_or(bead_core::animation::Curve::EaseOut),
                    );
                }
                self.animations.target(
                    &motion.id,
                    if open { 1. } else { 0. },
                    now,
                    if self.options.animate {
                        1.
                    } else {
                        motion.duration
                    },
                    motion.curve.unwrap_or(if motion.spring {
                        bead_core::animation::Curve::Spring
                    } else {
                        bead_core::animation::Curve::EaseOut
                    }),
                );
                let (progress, active) = self.animations.value(&motion.id, now).unwrap();
                motion_clock = Some((motion.id.clone(), now));
                // A callback can arrive after the motion expires. Keep it armed
                // until we have drawn the terminal pose, rather than making the
                // next input finish the motion and invalidate the whole surface.
                s.redraw |= active;
                scene.opacity = motion.opacity(progress);
                // A revealing panel stays put and is clipped instead (Scene::reveal_window).
                let (dx, dy) = if motion.reveal {
                    reveal_progress = Some(progress);
                    (0., 0.)
                } else {
                    motion.edge.closed_offset(
                        motion.slide_distance.unwrap_or(scene.bounds.w),
                        motion.slide_distance.unwrap_or(scene.bounds.h),
                        progress,
                    )
                };
                opaque_motion = active && (dx != 0. || dy != 0. || reveal_progress.is_some());
                scene.offset.0 = 32. + dx;
                scene.offset.1 = 32. + dy;
            }
            if let Some(progress) = reveal_progress {
                scene.reveal(progress);
            }

            let scale = s.scale120 as f32 / 120.;
            let (w, h, stride, size) = buffer_dimensions(s.width, s.height, s.scale120)?;
            s.background_visible = scene.opacity > 0.001;
            let background_requested = self.options.background_blur && s.background_visible;
            let logical_size = (s.width, s.height);
            background::update(
                &self.background,
                &self.compositor,
                s,
                qh,
                background_requested,
                logical_size,
            );
            if !panel {
                scene.bounds.w = s.width as f32;
                scene.bounds.h = s.height as f32;
                scene.width = s.width as f32;
            }
            if panel {
                if self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_none_or(|spec| spec.position.is_none())
                    && scene.motion.as_ref().is_none_or(|m| {
                        m.edge == bead_core::Edge::Top || m.edge == bead_core::Edge::Bottom
                    })
                {
                    // Custom positioners already align the body through the padded
                    // surface origin, even when the compositor clips that surface.
                    scene.offset.0 = ((s.width as f32 - scene.width) / 2.).max(0.);
                }
                if let Some(slide) = scene.slide {
                    let progress = if self.options.animate {
                        let elapsed = self.started.elapsed().as_secs_f32();
                        if self.options.start_open && (elapsed < 3. || !s.ready) {
                            1.
                        } else {
                            let t = ((elapsed - if self.options.start_open { 3. } else { 0. })
                                % slide.period_seconds)
                                / (slide.period_seconds / 2.);
                            let progress = if t <= 1. { t } else { 2. - t };
                            if self.options.start_open {
                                1. - progress
                            } else {
                                progress
                            }
                        }
                    } else {
                        1.
                    };
                    let eased = progress * progress * (3. - 2. * progress);
                    scene.opacity = eased;
                    let y = slide.closed_y + (slide.open_y - slide.closed_y) * eased;
                    scene.offset.1 = (y * scale).round() / scale;
                }
            }
            if self.options.debug_progress
                && let Some(bead_render::Primitive::Rounded { rect, .. }) =
                    scene.primitives.last_mut()
            {
                let phase = self.started.elapsed().as_secs_f32() % 2.;
                rect.w = 1. + 299. * if phase < 1. { phase } else { 2. - phase };
            }
            if panel
                && self.options.interactive
                && (!s.companion || self.options.panel_spec.is_some())
            {
                let spec = self.options.panel_spec.as_ref();
                let passive = s.companion && spec.is_some_and(|spec| spec.passive);
                let text_focus = s.companion
                    && self
                        .panel_runtime
                        .as_ref()
                        .is_some_and(|source| source.surrounding_text().is_some());
                let keyboard_focus =
                    text_focus || !s.companion || spec.is_some_and(|spec| spec.keyboard_focus);
                let visible = scene.opacity > 0.001
                    && if s.companion && self.options.panel_spec.is_some() {
                        self.panel_target
                    } else {
                        scene.motion.as_ref().is_none_or(|m| m.open)
                    };
                let input_rect = if visible && s.companion && !passive && !self.options.test_passive
                {
                    content_input_rect(scene)
                } else {
                    None
                };
                if s.input_visible != Some(visible) || s.input_rect != input_rect {
                    if visible && !self.options.test_passive && !passive {
                        if let Some([x, y, w, h]) = input_rect {
                            let region = Region::new(&self.compositor)?;
                            if w > 0 && h > 0 {
                                region.add(x, y, w, h);
                            }
                            wl_surface.set_input_region(Some(region.wl_region()));
                        } else {
                            wl_surface.set_input_region(None);
                        }
                    } else {
                        let empty = Region::new(&self.compositor)?;
                        wl_surface.set_input_region(Some(empty.wl_region()));
                    }
                    s.input_visible = Some(visible);
                    s.input_rect = input_rect;
                    bead_core::vprintln!("PANEL_INPUT visible={visible}");
                }
                let focus_visible = visible
                    && !self.options.test_passive
                    && !passive
                    && if s.companion {
                        keyboard_focus
                    } else {
                        self.options.keyboard_focus || self.options.test_keyboard
                    };
                let interactivity = panel_keyboard_mode(
                    focus_visible,
                    s.companion
                        && (self.pearl.is_some()
                            || self
                                .panel_runtime
                                .as_ref()
                                .is_some_and(|source| !source.outside_dismiss())),
                    self.options.test_keyboard || s.companion,
                );
                if s.keyboard_visible != Some(interactivity) {
                    s.layer.set_keyboard_interactivity(interactivity);
                    s.keyboard_visible = Some(interactivity);
                }
            }
            s.offset = scene.offset;
            s.input_content_offset = scene
                .motion
                .as_ref()
                .filter(|motion| motion.reveal)
                .and_then(|motion| {
                    motion.slide_distance.map(|distance| {
                        motion
                            .edge
                            .closed_offset(distance, distance, motion.progress)
                    })
                })
                .unwrap_or((0., 0.));
            renderer.presentation_motion(opaque_motion);
            {
                let _prepare = bead_core::profile::Span::new("present_prepare");
                renderer.prepare(scene, scale)?;
            }
            let mut regions = {
                let _damage = bead_core::profile::Span::new("visual_damage");
                if self.options.damage {
                    if let Some(frame) = renderer.prepared_frame() {
                        s.visual_damage.update(
                            &frame,
                            scene.offset,
                            scene.opacity,
                            w as u32,
                            h as u32,
                        )?
                    } else {
                        vec![Damage::full(w as u32, h as u32)]
                    }
                } else {
                    vec![Damage::full(w as u32, h as u32)]
                }
            };
            // An accepted generation can be pixel-identical (including edits to
            // filtered rows). Request actual compositor feedback for that new
            // transaction too; a callback-only commit has no presentation event.
            if regions.is_empty() && s.presented_generation != Some(s.ui_serial) {
                regions.push(Damage {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                });
            }
            if self.options.trace_state || self.options.debug_progress {
                let pixels: u64 = regions
                    .iter()
                    .map(|r| u64::from(r.w) * u64::from(r.h))
                    .sum();
                bead_core::vprintln!(
                    "DAMAGE rectangles={} pixels={} share={:.6}",
                    regions.len(),
                    pixels,
                    pixels as f64 / (f64::from(w) * f64::from(h))
                );
            }
            if let Some(viewport) = &s.viewport {
                wl_surface.set_buffer_scale(1);
                viewport.set_destination(s.width as i32, s.height as i32);
            } else {
                wl_surface.set_buffer_scale((s.scale120 / 120) as i32);
            }
            #[cfg(feature = "native-gl")]
            if _kind == RendererKind::FemtovgGl {
                use raw_window_handle::{
                    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
                };
                use std::ptr::NonNull;
                let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(self.conn.backend().display_ptr().cast())
                        .ok_or("native display missing")?,
                ));
                let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(wl_surface.id().as_ptr().cast())
                        .ok_or("native surface missing")?,
                ));
                if self.gl.is_none() {
                    // SAFETY: the connection outlives Transport and surfaces.
                    self.gl = Some(
                        unsafe { bead_render::gl::Transport::new(display) }
                            .map_err(|e| GpuUnavailable(e.to_string()))?,
                    );
                }
                let transport = self.gl.as_mut().unwrap();
                if s.gl.is_none() {
                    // SAFETY: EGL surface is dropped before the retained wl_surface.
                    s.gl = Some(
                        unsafe { transport.create_surface(window, w as u32, h as u32) }
                            .map_err(|e| GpuUnavailable(e.to_string()))?,
                    );
                    bead_core::vprintln!(
                        "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GL",
                        s.output.id().protocol_id(),
                        s.width,
                        s.height,
                        w,
                        h
                    );
                }
                let surface = s.gl.as_ref().unwrap();
                // Paint the target backbuffer without swapping it. It shares
                // the existing context, renderer and raster cache; only release
                // presents this frame together with its final surface offset.
                if s.native.is_none()
                    && let Some(prepared) = s.prepared_native.as_mut()
                    && prepared.configured
                    && prepared.scale120 == s.scale120
                {
                    let key = policy::PreparedFrameKey {
                        generation: prepared.size_generation,
                        ui_serial: s.ui_serial,
                        size: (s.width, s.height),
                        scale120: s.scale120,
                    };
                    if !prepared.gl_ready
                        || prepared.snapshot_key() != Some(key)
                        || !regions.is_empty()
                    {
                        prepared.gl_ready = false;
                        if prepared.gl.is_none() {
                            use raw_window_handle::{RawWindowHandle, WaylandWindowHandle};
                            let window =
                                prepared.window.as_ref().ok_or("missing prewarm window")?;
                            let handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                                std::ptr::NonNull::new(window.wl_surface().id().as_ptr().cast())
                                    .ok_or("missing prewarm surface")?,
                            ));
                            // SAFETY: PreparedNative drops EGL before its window.
                            prepared.gl = Some(unsafe {
                                transport.create_surface(handle, w as u32, h as u32)
                            }?);
                        }
                        transport.activate(prepared.gl.as_ref().unwrap(), w as u32, h as u32)?;
                        renderer.draw(
                            scene,
                            RenderTarget::Gl(bead_render::gl::GlTarget {
                                display: &transport.display,
                                width: w as u32,
                                height: h as u32,
                            }),
                            Damage::full(w as u32, h as u32),
                        )?;
                        prepared.snapshot = None;
                        prepared.server_attached_key = None;
                        prepared.server_warm_key = None;
                        prepared.server_pending_frame = None;
                        prepared.server_warm_pixel = None;
                        prepared.server_ready_logged_generation = None;
                        prepared.snapshot_size_generation = key.generation;
                        prepared.snapshot_ui_serial = key.ui_serial;
                        prepared.snapshot_size = key.size;
                        prepared.snapshot_scale120 = key.scale120;
                        prepared.gl_ready = true;
                    }
                }
                let retained_gl_prepare = s.companion
                    && !s.gl_prepared
                    && self
                        .options
                        .panel_spec
                        .as_ref()
                        .is_some_and(|spec| spec.retain_when_closed);
                let activate_started = Instant::now();
                transport.activate(surface, w as u32, h as u32)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.activate_ms
                        .push(activate_started.elapsed().as_secs_f64() * 1000.);
                }
                if regions.is_empty() && !retained_gl_prepare {
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            prepared_frame: None,
                        },
                    );
                    s.callbacks.pending = true;
                    if let Some(visual) = &mut s.pointer_visual {
                        visual.stage_commit()?;
                    }
                    if let Some(presentation) = &self.presentation
                        && let Some(visual) = &s.pointer_visual
                    {
                        visual.feedback(presentation, &wl_surface, qh);
                    }
                    wl_surface.commit();
                    s.frames += 1;
                    return Ok(());
                }
                let start = Instant::now();
                let compose_profile = bead_core::profile::Span::new("compose");
                if retained_gl_prepare {
                    let opacity = scene.opacity;
                    // This private backbuffer is never presented. A partial alpha
                    // forces the retained opacity group to be built and uploaded.
                    scene.opacity = 0.5;
                    let prepared = renderer.draw(
                        scene,
                        RenderTarget::Gl(bead_render::gl::GlTarget {
                            display: &transport.display,
                            width: w as u32,
                            height: h as u32,
                        }),
                        Damage::full(w as u32, h as u32),
                    );
                    scene.opacity = opacity;
                    prepared?;
                    drop(compose_profile);
                    s.visual_damage = Default::default();
                    s.gl_prepared = true;
                    s.gl_prepare_pending = true;
                    if bead_core::profile::frame_trace() {
                        println!(
                            "PANEL_GPU_PREPARED monotonic_ns={} prepare_ms={:.3}",
                            monotonic_ns(),
                            frame_started.elapsed().as_secs_f64() * 1000.
                        );
                    }
                    return Ok(());
                }
                renderer.draw(
                    scene,
                    RenderTarget::Gl(bead_render::gl::GlTarget {
                        display: &transport.display,
                        width: w as u32,
                        height: h as u32,
                    }),
                    Damage::full(w as u32, h as u32),
                )?;
                drop(compose_profile);
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                }
                for region in &regions {
                    wl_surface.damage_buffer(
                        region.x as i32,
                        region.y as i32,
                        region.w as i32,
                        region.h as i32,
                    );
                }
                wl_surface.frame(
                    qh,
                    FrameData {
                        surface: wl_surface.clone(),
                        epoch: s.callbacks.request(),
                        generation: scene.resource_revision,
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        prepared_frame: None,
                    },
                );
                s.callbacks.pending = true;
                if let Some(visual) = &mut s.pointer_visual {
                    visual.stage_commit()?;
                }
                if let Some(p) = &self.presentation {
                    if let Some(visual) = &s.pointer_visual {
                        visual.feedback(p, &wl_surface, qh);
                    }
                    p.feedback(
                        &wl_surface,
                        qh,
                        PresentationData {
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            update: s.ui_serial,
                            surface: wl_surface.clone(),
                            generation: scene.resource_revision,
                        },
                    );
                }
                let swap_started = Instant::now();
                if s.companion {
                    bead_core::profile::mark("panel_commit");
                }
                transport.present(surface, &regions)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.swap_ms.push(swap_started.elapsed().as_secs_f64() * 1000.);
                }
                s.frames += 1;
                return Ok(());
            }
            #[cfg(feature = "wgpu")]
            if renderer.wants_gpu() {
                if self.gpu.is_none() {
                    self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
                }
                let transport = self.gpu.as_mut().unwrap();
                if s.gpu.is_none() {
                    use raw_window_handle::{
                        RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle,
                        WaylandWindowHandle,
                    };
                    use std::ptr::NonNull;
                    let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                        NonNull::new(self.conn.backend().display_ptr().cast())
                            .ok_or("native display missing")?,
                    ));
                    let surface = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                        NonNull::new(wl_surface.id().as_ptr().cast())
                            .ok_or("native surface missing")?,
                    ));
                    // SAFETY: Surface::drop destroys GPU surface before protocol surface;
                    // app clears surfaces while the connection is still alive.
                    let created = unsafe {
                        transport.create_surface(display, surface, w as u32, h as u32, _kind)
                    };
                    match created {
                        Ok(gpu) => s.gpu = Some(gpu),
                        Err(e) => {
                            if !self.options.auto_fallback {
                                return Err(e);
                            }
                            eprintln!("FALLBACK GPU unavailable: {e}");
                            *renderer = bead_render::renderer(RendererKind::Cpu)?;
                            self.options.renderer = RendererKind::Cpu;
                            #[cfg(feature = "wgpu")]
                            {
                                self.gpu = None;
                            }
                        }
                    }
                    if s.gpu.is_some() {
                        bead_core::vprintln!(
                            "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GPU",
                            s.output.id().protocol_id(),
                            s.width,
                            s.height,
                            w,
                            h
                        );
                    }
                }
                if let Some(gpu) = s.gpu.as_mut() {
                    let transport = self.gpu.as_mut().unwrap();
                    transport.resize(gpu, w as u32, h as u32);
                    let dev = &transport.context.devices[gpu.dev_id];
                    use bead_render::gpu::{GpuTarget, wgpu};
                    let acquired = Instant::now();
                    let frame = match gpu.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(f)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                        other => return Err(format!("GPU acquire failed: {other:?}").into()),
                    };
                    let acquire_ms = acquired.elapsed().as_secs_f64() * 1000.;
                    let start = Instant::now();
                    let compose_profile = bead_core::profile::Span::new("compose");
                    renderer.draw(
                        scene,
                        RenderTarget::Gpu(GpuTarget {
                            device: &dev.device,
                            queue: &dev.queue,
                            texture: &frame.texture,
                            target_view: gpu.target.as_ref().map(|(_, view, _)| view),
                            width: w as u32,
                            height: h as u32,
                        }),
                        Damage::full(w as u32, h as u32),
                    )?;
                    drop(compose_profile);
                    if let Some((_, view, blitter)) = &gpu.target {
                        let mut encoder = dev.device.create_command_encoder(&Default::default());
                        blitter.copy(
                            &dev.device,
                            &mut encoder,
                            view,
                            &frame.texture.create_view(&Default::default()),
                        );
                        dev.queue.submit([encoder.finish()]);
                    }
                    if self.started.elapsed().as_secs_f64() > 2. {
                        s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                        s.acquire_ms.push(acquire_ms);
                    }
                    wl_surface.damage_buffer(0, 0, w, h);
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            prepared_frame: None,
                        },
                    );
                    s.callbacks.pending = true;
                    if let Some(visual) = &mut s.pointer_visual {
                        visual.stage_commit()?;
                    }
                    if let Some(p) = &self.presentation {
                        if let Some(visual) = &s.pointer_visual {
                            visual.feedback(p, &wl_surface, qh);
                        }
                        p.feedback(
                            &wl_surface,
                            qh,
                            PresentationData {
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                                update: s.ui_serial,
                                surface: wl_surface.clone(),
                                generation: scene.resource_revision,
                            },
                        );
                    }
                    dev.queue.present(frame);
                    let _ = dev.device.poll(wgpu::PollType::Poll);
                    s.frames += 1;
                    return Ok(());
                }
                // An explicitly permitted automatic fallback uses the same retained Scene.
                renderer.prepare(scene, scale)?;
            }
            let repair_profile = bead_core::profile::Span::new("repair_setup");
            let format = if cfg!(target_endian = "little")
                && !self.options.force_argb
                && self.shm.formats().contains(&wl_shm::Format::Abgr8888)
            {
                PixelFormat::Rgba
            } else {
                PixelFormat::Bgra
            };
            if !cfg!(target_endian = "little") {
                return Err("CPU renderer currently requires little endian".into());
            }
            if s.geometry != (w, h) || s.format != format || s.buffers.is_empty() {
                s.buffers.clear();
                s.geometry = (w, h);
                s.format = format;
                s.previous = None;
                // Pools only grow, and an old geometry's active slots cannot be
                // reused yet. Start a bounded pool for this geometry instead of
                // retaining old-scale pages or growing around those slots.
                // SCTK defers active wl_buffer destruction until release; the
                // compositor retains its backing even after our mapping drops.
                // SlotPool rounds each slot to a 64-byte boundary.
                s.pool = SlotPool::new(size.next_multiple_of(64) * 3, &self.shm)?;
                for _ in 0..3 {
                    let (buffer, canvas) = s.pool.create_buffer(
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )?;
                    if scene.motion.is_some() {
                        advise_frame_pages(canvas, false);
                    }
                    s.buffers.push(Reusable {
                        buffer,
                        pending: vec![Damage::full(w as u32, h as u32)],
                        painted: false,
                    });
                    s.allocations += 1;
                }
                bead_core::vprintln!(
                    "BUFFER output={} logical={}x{} physical={}x{} scale={scale} format={format:?} allocation_count={} pool_bytes={} buffer_bytes={size}",
                    s.output.id().protocol_id(),
                    s.width,
                    s.height,
                    w,
                    h,
                    s.allocations,
                    s.pool.len()
                );
            }
            let current = scene.bounds();
            for b in &mut s.buffers {
                for region in &regions {
                    track_repair(&mut b.pending, *region);
                }
            }
            // Animation history needs the fresher-pixel policy. Ordinary surfaces
            // keep the cheap first-free path (and the existing unpainted adoption).
            let plan = if scene.motion.is_some() {
                repair_plan(
                    s.buffers.iter().map(|b| {
                        (
                            !b.buffer.slot().has_active_buffers(),
                            b.painted,
                            b.pending.as_slice(),
                        )
                    }),
                    w as u64 * h as u64,
                )
            } else {
                s.buffers
                    .iter()
                    .position(|b| !b.buffer.slot().has_active_buffers())
                    .map(|slot| {
                        let adopt = (!s.buffers[slot].painted)
                            .then(|| s.buffers.iter().position(|b| b.painted))
                            .flatten();
                        (slot, adopt)
                    })
            };
            let Some((slot, adopt)) = plan else {
                // The compositor still holds every buffer (a slow release right after a
                // resize): try again shortly rather than ending the whole shell.
                s.redraw = true;
                busy = true;
                return Ok(());
            };
            // An older painted slot can still owe an entire animation repaint.
            // Copy fresher pixels and their matching repairs together, just as
            // for an unpainted spare left by pool compaction.
            if let Some(from) = adopt {
                let pending = s.buffers[from].pending.clone();
                let zero_target = !s.buffers[slot].painted;
                // A held source is read-only; canvas() is only used for the free target.
                let (source, len) = {
                    let held = s.buffers[from].buffer.slot();
                    let data = s.pool.raw_data_mut(&held);
                    (data.as_ptr(), data.len())
                };
                if let Some(target) = s.buffers[slot].buffer.canvas(&mut s.pool) {
                    if zero_target && scene.motion.is_some() {
                        // Join boundary-page advice only when the spare is used,
                        // keeping untouched spare pages out of settled residency.
                        advise_frame_pages(target, true);
                    }
                    // SAFETY: distinct slots in the same live, unmoved pool mapping.
                    // Repairs fully replace their pixels, so only their complement
                    // needs the adopted contents. The source is never written.
                    copy_outside_repairs(
                        unsafe { std::slice::from_raw_parts(source, len) },
                        target,
                        w as u32,
                        h as u32,
                        &pending,
                        zero_target,
                    );
                    s.buffers[slot].pending = pending;
                }
            }
            let b = &mut s.buffers[slot];
            b.painted = true;
            let bounding = b
                .pending
                .iter()
                .copied()
                .fold(Damage::full(0, 0), Damage::union);
            let repairs = if _kind == RendererKind::Cpu {
                b.pending.as_slice()
            } else {
                std::slice::from_ref(&bounding)
            };
            let start = Instant::now();
            drop(repair_profile);
            let compose_profile = bead_core::profile::Span::new("compose");
            let canvas = b
                .buffer
                .canvas(&mut s.pool)
                .ok_or("compositor owns buffer")?;
            let warm_target = s.prepared_native.as_ref().filter(|prepared| {
                prepared.configured
                    && prepared.viewport.is_some()
                    && prepared.source_size == (s.width, s.height)
                    && prepared.scale120 == s.scale120
            });
            let preferred_pixel = warm_target.and_then(|prepared| prepared.server_warm_pixel);
            for damage in repairs {
                renderer.draw(
                    scene,
                    RenderTarget::Shm(ShmTarget {
                        bytes: &mut *canvas,
                        width: w as u32,
                        height: h as u32,
                        scale,
                        format,
                    }),
                    *damage,
                )?;
            }
            let transparent = warm_target.and_then(|_| {
                transparent_corner(
                    canvas,
                    w as u32,
                    h as u32,
                    stride as usize,
                    preferred_pixel,
                )
            });
            if std::env::var_os("BEAD_PROFILE_DAMAGE").is_some() {
                println!(
                    "INPUT_REPAIR rects={} pixels={} bounding_pixels={}",
                    repairs.len(),
                    repairs
                        .iter()
                        .map(|d| u64::from(d.w) * u64::from(d.h))
                        .sum::<u64>(),
                    u64::from(bounding.w) * u64::from(bounding.h)
                );
            }
            drop(compose_profile);
            if self.started.elapsed().as_secs_f64() > 2. {
                s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let _commit_profile = bead_core::profile::Span::new("commit");
            for region in &regions {
                wl_surface.damage_buffer(
                    region.x as i32,
                    region.y as i32,
                    region.w as i32,
                    region.h as i32,
                );
            }
            wl_surface.frame(
                qh,
                FrameData {
                    surface: wl_surface.clone(),
                    epoch: s.callbacks.request(),
                    generation: scene.resource_revision,
                    source_epoch: if s.companion { self.panel_epoch } else { 0 },
                    prepared_frame: None,
                },
            );
            s.callbacks.pending = true;
            if let Some(visual) = &mut s.pointer_visual {
                visual.stage_commit()?;
            }
            if let Some(p) = &self.presentation {
                if let Some(visual) = &s.pointer_visual {
                    visual.feedback(p, &wl_surface, qh);
                }
                p.feedback(
                    &wl_surface,
                    qh,
                    PresentationData {
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        update: s.ui_serial,
                        surface: wl_surface.clone(),
                        generation: scene.resource_revision,
                    },
                );
            }
            b.buffer.attach_to(&wl_surface)?;
            if s.companion {
                bead_core::profile::mark("panel_commit");
            }
            wl_surface.commit();
            if s.native.is_none() && s.prepared_native.is_some() {
                let slot = b.buffer.slot();
                let snapshot = s
                    .pool
                    .create_buffer_in(
                        &slot,
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )
                    .ok();
                if let Some(prepared) = s.prepared_native.as_mut() {
                    prepared.snapshot = snapshot;
                    #[cfg(feature = "native-gl")]
                    {
                        prepared.gl_ready = false;
                    }
                    prepared.snapshot_size_generation = prepared.size_generation;
                    prepared.snapshot_ui_serial = s.ui_serial;
                    prepared.snapshot_size = (s.width, s.height);
                    prepared.snapshot_scale120 = s.scale120;
                    prepared.server_attached_key = None;
                    prepared.server_warm_key = None;
                    prepared.server_pending_frame = None;
                    prepared.server_warm_pixel = None;
                    if let Some(pixel) = transparent
                        && prepared.configured
                        && prepared.source_size == (s.width, s.height)
                        && prepared.scale120 == s.scale120
                        && let (Some(viewport), Some(buffer), Some(window)) = (
                            prepared.viewport.as_ref(),
                            prepared.snapshot.as_ref(),
                            prepared.window.as_ref(),
                        )
                    {
                        let target = window.wl_surface();
                        target.set_buffer_scale(1);
                        viewport.set_source(
                            f64::from(pixel.0),
                            f64::from(pixel.1),
                            1.0,
                            1.0,
                        );
                        viewport.set_destination(s.width as i32, s.height as i32);
                        buffer.attach_to(target)?;
                        target.damage_buffer(0, 0, w, h);
                        prepared.server_frame_epoch =
                            prepared.server_frame_epoch.wrapping_add(1).max(1);
                        let frame_epoch = prepared.server_frame_epoch;
                        let key = policy::PreparedFrameKey {
                            generation: prepared.size_generation,
                            ui_serial: s.ui_serial,
                            size: (s.width, s.height),
                            scale120: s.scale120,
                        };
                        target.frame(
                            qh,
                            FrameData {
                                surface: target.clone(),
                                epoch: frame_epoch,
                                generation: scene.resource_revision,
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                                prepared_frame: Some(PreparedFrameData {
                                    epoch: frame_epoch,
                                    prepared_epoch: prepared.epoch,
                                    size_generation: key.generation,
                                    ui_serial: key.ui_serial,
                                }),
                            },
                        );
                        target.commit();
                        prepared.server_attached_key = Some(key);
                        prepared.server_pending_frame =
                            Some((frame_epoch, prepared.epoch, key));
                        prepared.server_warm_pixel = Some(pixel);
                    }
                }
            }
            b.pending.clear();
            s.previous = Some(current);
            s.frames += 1;
            Ok(())
        })();
        if result.is_ok() && self.surfaces[index].frames > previous_frames {
            self.log_prepared_native_ready(index);
        }
        if result.is_ok()
            && self.surfaces[index].companion
            && self.surfaces[index].frames > previous_frames
        {
            if self
                .panel_dock_transition
                .as_ref()
                .is_some_and(|t| !t.started)
            {
                self.start_dock_transition_after_commit();
            } else if self
                .dock_transition_progress(self.started.elapsed().as_secs_f64())
                .is_some_and(|progress| progress >= 1.)
            {
                self.panel_dock_transition = None;
            }
        }
        self.retire_native_cover(
            index,
            result.is_ok() && self.surfaces[index].frames > previous_frames,
        );
        if result.is_ok()
            && self.surfaces[index].companion
            && self.surfaces[index].frames > previous_frames
        {
            // Reverse handoff: native resources retire only after the attached
            // destination has committed accepted content, never on dock request.
            self.panel_cover = None;
        }
        if let Some((id, drawn)) = motion_clock
            && self.surfaces[index].frames > previous_frames
            && frame_started.elapsed() > Duration::from_millis(4)
        {
            // Baking textures for a motion's first frame can take several frames' time.
            self.animations
                .rebase_fresh(&id, drawn, self.started.elapsed().as_secs_f64());
        }
        if busy {
            let qh = qh.clone();
            let retry = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(8)),
                move |_, _, app| {
                    if index < app.surfaces.len() {
                        app.draw(index, &qh);
                    }
                    TimeoutAction::Drop
                },
            );
            if let Err(error) = retry {
                self.fail(error);
            }
        }
        if self.surfaces[index].frames > previous_frames
            && self.started.elapsed().as_secs_f64() > 2.
        {
            if bead_core::profile::frame_trace() {
                println!(
                    "UI_FRAME monotonic_ns={} render_ms={:.3} companion={}",
                    monotonic_ns(),
                    frame_started.elapsed().as_secs_f64() * 1000.,
                    self.surfaces[index].companion
                );
            }
            if self.options.debug_luau_stall_ms.is_some() {
                bead_core::vprintln!(
                    "DEBUG_FRAME monotonic_ns={} companion={}",
                    monotonic_ns(),
                    self.surfaces[index].companion
                );
            }
            if self.options.trace_state && self.surfaces[index].companion {
                bead_core::vprintln!(
                    "PANEL_FRAME processing_ms={:.3}",
                    frame_started.elapsed().as_secs_f64() * 1000.
                );
            }
            self.surfaces[index]
                .frame_ms
                .push(frame_started.elapsed().as_secs_f64() * 1000.);
        }
        let companion = self.surfaces[index].companion;
        let arm_transient = if companion {
            self.companion
                .as_mut()
                .is_some_and(|(_, r, _)| r.take_transient_timer())
        } else {
            self.renderer.take_transient_timer()
        };
        if arm_transient {
            let epoch = self.panel_epoch;
            if let Err(error) = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(500)),
                move |_, _, app| {
                    // A pearl's renderer is only in `app.companion` while its slot is swapped in.
                    let remaining = if companion {
                        app.with_epoch(epoch, |app| {
                            app.companion
                                .as_mut()
                                .and_then(|(_, r, _)| r.trim_transient_idle())
                        })
                        .flatten()
                    } else {
                        app.renderer.trim_transient_idle()
                    };
                    remaining.map_or(TimeoutAction::Drop, TimeoutAction::ToDuration)
                },
            ) {
                self.fail(error);
            }
        }
        if result.is_ok() {
            self.trim_idle_shm(index);
            if self.surfaces[index].frames > previous_frames {
                let revision = if companion {
                    self.companion
                        .as_ref()
                        .map(|(scene, _, _)| scene.resource_revision)
                } else {
                    Some(self.scene.resource_revision)
                };
                let generation = revision
                    .map(|revision| (if companion { self.panel_epoch } else { 0 }, revision));
                if self.surfaces[index].heap_trim_generation != generation {
                    self.surfaces[index].heap_trim_generation = generation;
                    // Acceptance precedes presenting the replacement. Wait until
                    // old frame references and reveal scratch have been released;
                    // this heap-only trim does not retire or repaint SHM buffers.
                    if let Err(error) = self.loop_handle.insert_source(
                        Timer::from_duration(Duration::from_millis(500)),
                        |_, _, _| {
                            #[cfg(all(target_os = "linux", target_env = "gnu"))]
                            unsafe {
                                libc::malloc_trim(0);
                            }
                            TimeoutAction::Drop
                        },
                    ) {
                        self.fail(error);
                    }
                }
            }
        }
        if let Err(e) = result {
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            if self.options.auto_fallback
                && self.surfaces[index].companion
                && e.downcast_ref::<GpuUnavailable>().is_some()
            {
                eprintln!("FALLBACK companion GPU unavailable: {e}");
                let renderer = match bead_render::renderer(RendererKind::Cpu) {
                    Ok(renderer) => renderer,
                    Err(error) => {
                        self.fail(error);
                        return;
                    }
                };
                if let Some((_, active, kind)) = &mut self.companion {
                    *active = renderer;
                    *kind = RendererKind::Cpu;
                } else {
                    self.fail("companion renderer unavailable during GPU fallback");
                    return;
                }
                if let Some((_, desired)) = &mut self.options.companion {
                    // Keep this slot on CPU after fallback instead of retrying GL
                    // on every frame. A replacement panel may choose its own policy.
                    *desired = RendererKind::Cpu;
                }
                for surface in self.surfaces.iter_mut().filter(|surface| surface.companion) {
                    #[cfg(feature = "native-gl")]
                    {
                        drop(surface.gl.take());
                        surface.gl_prepared = false;
                        surface.gl_prepare_pending = false;
                    }
                    #[cfg(feature = "wgpu")]
                    drop(surface.gpu.take());
                    surface.callbacks.invalidate();
                    surface.visual_damage = Default::default();
                }
                self.release_timer = false;
                self.companion_policy = Default::default();
                #[cfg(feature = "native-gl")]
                if self.options.renderer == RendererKind::Cpu
                    && !self.has_gl_surfaces()
                    && let Some(transport) = self.gl.take()
                    && let Err(error) = {
                        // SAFETY: all companion GL surfaces were just dropped, no primary
                        // GL surface remains, and the CPU renderer owns no EGL resources.
                        unsafe { transport.shutdown_exclusive() }
                    }
                {
                    self.fail(error);
                    return;
                }
                #[cfg(feature = "wgpu")]
                if !self.has_gpu_surfaces() {
                    self.gpu = None;
                }
                println!("RENDER_POLICY companion=true transition=gpu-to-cpu fallback=true");
                if self
                    .surfaces
                    .iter()
                    .filter(|surface| surface.companion)
                    .all(|surface| surface.frames == 0)
                {
                    self.animations = Default::default();
                }
                let indices: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, surface)| surface.companion)
                    .map(|(index, _)| index)
                    .collect();
                for index in indices {
                    // draw() already selected the panel animation map for this call.
                    self.draw_current(index, qh);
                }
                return;
            }
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            if self.options.auto_fallback
                && !self.surfaces[index].companion
                && e.downcast_ref::<GpuUnavailable>().is_some()
            {
                eprintln!("FALLBACK GPU unavailable: {e}");
                if let Ok(renderer) = bead_render::renderer(RendererKind::Cpu) {
                    self.renderer = renderer;
                }
                for surface in &mut self.surfaces {
                    #[cfg(feature = "native-gl")]
                    {
                        drop(surface.gl.take());
                        surface.gl_prepared = false;
                        surface.gl_prepare_pending = false;
                    }
                    #[cfg(feature = "wgpu")]
                    drop(surface.gpu.take());
                    surface.callbacks.pending = false;
                }
                #[cfg(feature = "native-gl")]
                {
                    self.gl = None;
                }
                #[cfg(feature = "wgpu")]
                {
                    self.gpu = None;
                }
                self.options.renderer = RendererKind::Cpu;
                for i in 0..self.surfaces.len() {
                    self.draw(i, qh);
                }
                return;
            }
            self.fail(e);
        }
    }
}

/// Keep a bounded repair history for each compositor-owned SHM buffer.
fn track_repair(pending: &mut Vec<Damage>, mut region: Damage) {
    if region.w == 0 || region.h == 0 {
        return;
    }
    let mut index = 0;
    while index < pending.len() {
        let old = pending[index];
        if region.x < old.x + old.w
            && old.x < region.x + region.w
            && region.y < old.y + old.h
            && old.y < region.y + region.h
        {
            region = region.union(pending.swap_remove(index));
            index = 0;
        } else {
            index += 1;
        }
    }
    if pending.len() < 8 {
        pending.push(region);
    } else {
        let area = |d: Damage| u64::from(d.w) * u64::from(d.h);
        let index = pending
            .iter()
            .enumerate()
            .min_by_key(|(_, old)| {
                area(old.union(region))
                    .saturating_sub(area(**old))
                    .saturating_sub(area(region))
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        pending[index] = pending[index].union(region);
    }
}

/// Prefer current pixels over an older animation frame or an unpainted slot.
/// Rebase stale pixels only when the saved repair exceeds a quarter of a frame;
/// smaller edits do not justify a full-buffer copy. Held sources are read-only.
#[inline(never)]
fn repair_plan<'a>(
    buffers: impl Iterator<Item = (bool, bool, &'a [Damage])> + Clone,
    frame_pixels: u64,
) -> Option<(usize, Option<usize>)> {
    let pixels = |pending: &[Damage]| {
        pending
            .iter()
            .map(|d| u64::from(d.w) * u64::from(d.h))
            .sum::<u64>()
    };
    let (slot, (_, painted, pending)) = buffers
        .clone()
        .enumerate()
        .filter(|(_, (available, _, _))| *available)
        .min_by_key(|(_, (_, painted, pending))| (!*painted, pixels(pending)))?;
    let adopt = buffers
        .enumerate()
        .filter(|(_, (_, painted, _))| *painted)
        .min_by_key(|(_, (_, _, pending))| pixels(pending))
        .filter(|(from, (_, _, source))| {
            *from != slot && (!painted || pixels(pending) > pixels(source) + frame_pixels / 4)
        })
        .map(|(from, _)| from);
    Some((slot, adopt))
}

fn transparent_corner(
    bytes: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    preferred: Option<(u32, u32)>,
) -> Option<(u32, u32)> {
    let alpha_zero = |(x, y): (u32, u32)| {
        x < width
            && y < height
            && bytes
                .get(y as usize * stride + x as usize * 4 + 3)
                .is_some_and(|alpha| *alpha == 0)
    };
    let right = width.checked_sub(1)?;
    let bottom = height.checked_sub(1)?;
    let clear_corner = |(x, y): (u32, u32)| {
        if !(x == 0 || x == right) || !(y == 0 || y == bottom) {
            return false;
        }
        let nx = if x == 0 { right.min(1) } else { x - 1 };
        let ny = if y == 0 { bottom.min(1) } else { y - 1 };
        // Linear filtering can sample inward neighbors of the 1x1 crop.
        [(x, y), (nx, y), (x, ny), (nx, ny)]
            .into_iter()
            .all(alpha_zero)
    };
    if let Some(pixel) = preferred.filter(|pixel| clear_corner(*pixel)) {
        return Some(pixel);
    }
    // Warming is optional: checking corners keeps opaque-scene work bounded.
    [(0, 0), (right, 0), (0, bottom), (right, bottom)]
        .into_iter()
        .find(|pixel| clear_corner(*pixel))
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    use bead_render::{Primitive, Rect, Renderer, Scene, tree::TreeCpuRenderer};
    use std::collections::BTreeMap;

    #[test]
    fn transparent_corner_requires_verified_zero_alpha_and_honors_row_stride() {
        let mut pixels = vec![0xff; 48];
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            pixels[y * 16 + x * 4 + 3] = 0;
        }
        assert_eq!(transparent_corner(&pixels, 3, 3, 16, None), Some((0, 0)));
        assert_eq!(transparent_corner(&pixels, 3, 3, 16, Some((9, 9))), Some((0, 0)));
        pixels[23] = 1; // A filtered inward neighbor must also be transparent.
        assert_eq!(transparent_corner(&pixels, 3, 3, 16, None), None);
        let mut opaque_corners = vec![0xff; 36];
        opaque_corners[19] = 0; // An interior hole does not trigger a full scan.
        assert_eq!(transparent_corner(&opaque_corners, 3, 3, 12, None), None);
        assert_eq!(transparent_corner(&[], 0, 0, 0, None), None);
    }

    #[test]
    fn settled_pixels_win_over_animation_debt_but_held_slots_are_never_written() {
        let animation = [Damage::full(1746, 981)];
        let edit = [Damage::full(200, 40)];
        let empty = [];
        let slots = [
            (true, true, animation.as_slice()),
            (true, true, edit.as_slice()),
            (true, false, empty.as_slice()),
        ];
        let area = 1746 * 981;
        assert_eq!(repair_plan(slots.into_iter(), area), Some((1, None)));
        let mut held = slots;
        held[1].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), Some((0, Some(1))));
        held[0].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), Some((2, Some(1))));
        held[2].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), None);
        let narrow = [Damage::full(100, 40)];
        assert_eq!(
            repair_plan(
                [
                    (true, true, edit.as_slice()),
                    (false, true, narrow.as_slice())
                ]
                .into_iter(),
                area,
            ),
            Some((0, None)),
            "small repair differences must not cause a full-buffer copy"
        );
    }

    #[test]
    fn repair_history_is_bounded_and_preserves_every_dirty_rectangle() {
        let changes: Vec<_> = (0..30)
            .map(|i| Damage {
                x: i * 7,
                y: (i % 3) * 9,
                w: 2,
                h: 2,
            })
            .collect();
        let mut pending = Vec::new();
        for change in &changes {
            track_repair(&mut pending, *change);
            assert!(pending.len() <= 8);
        }
        for change in changes {
            assert!(pending.iter().any(|d| d.x <= change.x
                && d.y <= change.y
                && d.x + d.w >= change.x + change.w
                && d.y + d.h >= change.y + change.h));
        }
        let mut separate = Vec::new();
        track_repair(
            &mut separate,
            Damage {
                x: 2,
                y: 2,
                w: 4,
                h: 4,
            },
        );
        track_repair(
            &mut separate,
            Damage {
                x: 80,
                y: 50,
                w: 4,
                h: 4,
            },
        );
        assert_eq!(
            separate.len(),
            2,
            "distant edits must not repaint their intervening area"
        );
    }

    #[test]
    fn three_buffer_disjoint_repair_matches_full_frames_with_age_and_alpha() {
        for scale in [1., 1.5] {
            let (width, height) = ((96. * scale) as u32, (72. * scale) as u32);
            for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
                let mut buffers: Vec<_> = (0..3)
                    .map(|_| {
                        (
                            vec![0; (width * height * 4) as usize],
                            vec![Damage::full(width, height)],
                            false,
                        )
                    })
                    .collect();
                let mut renderer = TreeCpuRenderer::default();
                for frame in 0..12 {
                    let fill = |rect, color| Primitive::Rounded {
                        rect,
                        radius: 3.,
                        fill: color,
                        border: None,
                    };
                    let scene = Scene {
                        opacity: 1.,
                        motion: None,
                        resource_revision: 1,
                        asset_data: BTreeMap::new(),
                        clips: BTreeMap::new(),
                        layers: vec![],
                        prepared: BTreeMap::new(),
                        font_files: vec![],
                        width: 96.,
                        slide: None,
                        offset: (0., 0.),
                        bounds: Rect::new(0., 0., 96., 72.),
                        primitives: vec![
                            fill(Rect::new(0., 0., 96., 72.), [17, 31, 71, 210]),
                            fill(Rect::new(4., 4., 16., 10.), [frame * 13, 93, 25, 170]),
                            fill(Rect::new(74., 54., 16., 10.), [29, frame * 11, 217, 255]),
                        ],
                    };
                    for (_, pending, _) in &mut buffers {
                        track_repair(
                            pending,
                            Rect::new(2., 2., 20., 14.).pixels(scale, width, height),
                        );
                        track_repair(
                            pending,
                            Rect::new(72., 52., 20., 14.).pixels(scale, width, height),
                        );
                    }
                    renderer.prepare(&scene, scale).unwrap();
                    let frame_index = usize::from(frame);
                    let (slot, adopt) = repair_plan(
                        buffers
                            .iter()
                            .enumerate()
                            .map(|(index, (_, pending, painted))| {
                                let available = if frame_index % 4 == 0 {
                                    index == frame_index % 3
                                } else {
                                    index != (frame_index + 1) % 3
                                };
                                (available, *painted, pending.as_slice())
                            }),
                        u64::from(width) * u64::from(height),
                    )
                    .unwrap();
                    let adopted = adopt.map(|from| (from, buffers[from].clone()));
                    if let Some((_, (bytes, pending, _))) = &adopted {
                        let zero_target = !buffers[slot].2;
                        copy_outside_repairs(
                            bytes,
                            &mut buffers[slot].0,
                            width,
                            height,
                            pending,
                            zero_target,
                        );
                        buffers[slot].1 = pending.clone();
                    }
                    let (bytes, pending, painted) = &mut buffers[slot];
                    for damage in pending.iter().copied() {
                        renderer
                            .draw(
                                &scene,
                                RenderTarget::Shm(ShmTarget {
                                    bytes: &mut *bytes,
                                    width,
                                    height,
                                    scale,
                                    format,
                                }),
                                damage,
                            )
                            .unwrap();
                    }
                    pending.clear();
                    *painted = true;
                    let mut expected = vec![0; bytes.len()];
                    let mut reference = TreeCpuRenderer::default();
                    reference.prepare(&scene, scale).unwrap();
                    reference
                        .draw(
                            &scene,
                            RenderTarget::Shm(ShmTarget {
                                bytes: &mut expected,
                                width,
                                height,
                                scale,
                                format,
                            }),
                            Damage::full(width, height),
                        )
                        .unwrap();
                    assert!(
                        *bytes == expected,
                        "stale buffer at frame{frame},scale{scale},format{format:?}"
                    );
                    if let Some((from, before)) = adopted {
                        assert_eq!(buffers[from], before, "adoption wrote to its source");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    fn scene() -> Scene {
        let body = Rect::new(0., 0., 200., 100.);
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![bead_render::SceneLayer {
                id: "content".into(),
                parent: None,
                own: 0..2,
                subtree_end: 2,
                bounds: body,
            }],
            prepared: Default::default(),
            font_files: vec![],
            width: 200.,
            slide: None,
            primitives: vec![
                bead_render::Primitive::Rounded {
                    rect: Rect::new(0., 0., 0., 0.),
                    radius: 0.,
                    fill: [0; 4],
                    border: None,
                },
                bead_render::Primitive::Rounded {
                    rect: body,
                    radius: 8.,
                    fill: [20; 4],
                    border: None,
                },
            ],
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., 264., 164.),
        }
    }
    #[test]
    fn painted_content_excludes_shadow_and_survives_background_replacement() {
        let mut scene = scene();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.primitives.clear();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (-10.5, 5.25);
        assert_eq!(content_input_rect(&scene), Some([-11, 5, 201, 101]));
    }
    #[test]
    fn unretained_fallback_skips_empty_shadow_placeholder() {
        let mut scene = scene();
        scene.layers.clear();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.primitives.clear();
        assert_eq!(content_input_rect(&scene), None);
    }
    #[test]
    fn entering_panel_never_captures_above_its_final_content_footprint() {
        let mut scene = scene();
        scene.motion = Some(bead_render::Motion {
            id: "content".into(),
            open: true,
            duration: 0.5,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            fade: true,
            reveal: false,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });
        scene.offset = (32., -18.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 50]));
        assert_eq!(content_window_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (32., -100.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 0]));
        assert_eq!(content_window_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (32., 32.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-surface rendering, GPU staging and idle release.
use super::*;

/// Shadows reserve buffer space, but must not intercept adjacent controls.
/// Retained layout bounds survive a shell replacing its background primitive.
fn content_input_rect(scene: &Scene) -> Option<[i32; 4]> {
    let body = scene
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
        })?;
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

impl App {
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
        let animated = self.options.animate || self.animations.active(now);
        if self.companion_policy.frame(now, animated) {
            if let Some((_, renderer, kind)) = &mut self.companion {
                *renderer = bead_render::renderer(RendererKind::FemtovgGl)?;
                *kind = RendererKind::FemtovgGl;
            }
            self.surfaces[index].visual_damage = Default::default();
            println!("RENDER_POLICY companion=true transition=cpu-to-gl");
            if !self.release_timer {
                let qh = qh.clone();
                self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(2)), move |_, _, app| {
                    if !app.release_timer||app.companion.is_none(){return TimeoutAction::Drop;}
                    let now = app.started.elapsed().as_secs_f64();
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
                            drop(surface.gl.take());
                                #[cfg(feature="wgpu")]
                            drop(surface.gpu.take());
                                surface.visual_damage = Default::default();
                                surface.callbacks.invalidate();
                            }
                            #[cfg(feature="native-gl")]
                            if app.surfaces.iter().all(|s| s.gl.is_none()) && app.options.renderer == RendererKind::Cpu
                                && let Some(transport) = app.gl.take() {
                                    // SAFETY: the CPU bar/companion own no GL resources;
                                    // this process owns one EGL display and all EGL surfaces are gone.
                                    if let Err(error) = unsafe { transport.shutdown_exclusive() } {
                                        app.fail(error); return TimeoutAction::Drop;
                                    }
                            }
                            #[cfg(feature="wgpu")]
                            if app.surfaces.iter().all(|s| s.gpu.is_none()) { app.gpu = None; }
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
            NonNull::new(surface.layer.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        #[cfg(feature = "native-gl")]
        if self.options.renderer == RendererKind::FemtovgGl {
            if self.gl.is_none() {
                // SAFETY: the retained connection outlives the EGL context/surfaces.
                self.gl = Some(unsafe { bead_render::gl::Transport::new(display) }?);
            }
            if surface.gl.is_none() {
                println!(
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
        if self.surfaces[index].companion {
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
            self.draw_current(index, qh);
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
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
        let result = (|| -> Result<(), Box<dyn Error>> {
            let s = &mut self.surfaces[index];
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
            if let Some(motion) = &scene.motion {
                let now = self.started.elapsed().as_secs_f64();
                let open = if self.options.animate {
                    ((now - 3.).max(0.) % 2.) >= 1. || now < 3.
                } else if s.companion && self.options.panel_spec.is_some() {
                    self.panel_target
                } else {
                    motion.open
                };
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
                scene.opacity = progress;
                let (dx, dy) = motion.edge.closed_offset(
                    motion.slide_distance.unwrap_or(scene.bounds.w),
                    motion.slide_distance.unwrap_or(scene.bounds.h),
                    progress,
                );
                opaque_motion = active && (dx != 0. || dy != 0.);
                scene.offset.0 = 32. + dx;
                scene.offset.1 = 32. + dy;
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
                if scene.motion.as_ref().is_none_or(|m| {
                    m.edge == bead_core::Edge::Top || m.edge == bead_core::Edge::Bottom
                }) {
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
                && scene.motion.is_some()
            {
                let spec = self.options.panel_spec.as_ref();
                let passive = s.companion && spec.is_some_and(|spec| spec.passive);
                let keyboard_focus = !s.companion || spec.is_some_and(|spec| spec.keyboard_focus);
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
                            s.layer
                                .wl_surface()
                                .set_input_region(Some(region.wl_region()));
                        } else {
                            s.layer.wl_surface().set_input_region(None);
                        }
                    } else {
                        let empty = Region::new(&self.compositor)?;
                        s.layer
                            .wl_surface()
                            .set_input_region(Some(empty.wl_region()));
                    }
                    s.input_visible = Some(visible);
                    s.input_rect = input_rect;
                    println!("PANEL_INPUT visible={visible}");
                }
                let focus_visible = visible
                    && !self.options.test_passive
                    && !passive
                    && if s.companion {
                        keyboard_focus
                    } else {
                        self.options.keyboard_focus || self.options.test_keyboard
                    };
                if s.keyboard_visible != Some(focus_visible) {
                    let interactivity = if !focus_visible {
                        KeyboardInteractivity::None
                    } else if self.options.test_keyboard || s.companion {
                        KeyboardInteractivity::Exclusive
                    } else {
                        KeyboardInteractivity::OnDemand
                    };
                    s.layer.set_keyboard_interactivity(interactivity);
                    s.keyboard_visible = Some(focus_visible);
                }
            }
            s.offset = scene.offset;
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
                println!(
                    "DAMAGE rectangles={} pixels={} share={:.6}",
                    regions.len(),
                    pixels,
                    pixels as f64 / (f64::from(w) * f64::from(h))
                );
            }
            if let Some(viewport) = &s.viewport {
                s.layer.wl_surface().set_buffer_scale(1);
                viewport.set_destination(s.width as i32, s.height as i32);
            } else {
                s.layer
                    .wl_surface()
                    .set_buffer_scale((s.scale120 / 120) as i32);
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
                    NonNull::new(s.layer.wl_surface().id().as_ptr().cast())
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
                    println!(
                        "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GL",
                        s.output.id().protocol_id(),
                        s.width,
                        s.height,
                        w,
                        h
                    );
                }
                let surface = s.gl.as_ref().unwrap();
                let activate_started = Instant::now();
                transport.activate(surface, w as u32, h as u32)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.activate_ms
                        .push(activate_started.elapsed().as_secs_f64() * 1000.);
                }
                if regions.is_empty() {
                    s.layer.wl_surface().frame(
                        qh,
                        FrameData {
                            surface: s.layer.wl_surface().clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        },
                    );
                    s.callbacks.pending = true;
                    s.layer.commit();
                    s.frames += 1;
                    return Ok(());
                }
                let start = Instant::now();
                let compose_profile = bead_core::profile::Span::new("compose");
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
                    s.layer.wl_surface().damage_buffer(
                        region.x as i32,
                        region.y as i32,
                        region.w as i32,
                        region.h as i32,
                    );
                }
                s.layer.wl_surface().frame(
                    qh,
                    FrameData {
                        surface: s.layer.wl_surface().clone(),
                        epoch: s.callbacks.request(),
                        generation: scene.resource_revision,
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                    },
                );
                s.callbacks.pending = true;
                if let Some(p) = &self.presentation {
                    p.feedback(
                        s.layer.wl_surface(),
                        qh,
                        PresentationData {
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            update: s.ui_serial,
                            surface: s.layer.wl_surface().clone(),
                            generation: scene.resource_revision,
                        },
                    );
                }
                let swap_started = Instant::now();
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
                        NonNull::new(s.layer.wl_surface().id().as_ptr().cast())
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
                        println!(
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
                    s.layer.wl_surface().damage_buffer(0, 0, w, h);
                    s.layer.wl_surface().frame(
                        qh,
                        FrameData {
                            surface: s.layer.wl_surface().clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        },
                    );
                    s.callbacks.pending = true;
                    if let Some(p) = &self.presentation {
                        p.feedback(
                            s.layer.wl_surface(),
                            qh,
                            PresentationData {
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                                update: s.ui_serial,
                                surface: s.layer.wl_surface().clone(),
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
            if s.geometry != (w, h) || s.format != format {
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
                    let (buffer, _) = s.pool.create_buffer(
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )?;
                    s.buffers.push(Reusable {
                        buffer,
                        pending: vec![Damage::full(w as u32, h as u32)],
                    });
                    s.allocations += 1;
                }
                println!(
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
            let b = s
                .buffers
                .iter_mut()
                .find(|b| !b.buffer.slot().has_active_buffers())
                .ok_or("three buffers still in use at callback")?;
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
            let compose_profile = bead_core::profile::Span::new("compose");
            let canvas = b
                .buffer
                .canvas(&mut s.pool)
                .ok_or("compositor owns buffer")?;
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
            for region in &regions {
                s.layer.wl_surface().damage_buffer(
                    region.x as i32,
                    region.y as i32,
                    region.w as i32,
                    region.h as i32,
                );
            }
            s.layer.wl_surface().frame(
                qh,
                FrameData {
                    surface: s.layer.wl_surface().clone(),
                    epoch: s.callbacks.request(),
                    generation: scene.resource_revision,
                    source_epoch: if s.companion { self.panel_epoch } else { 0 },
                },
            );
            s.callbacks.pending = true;
            if let Some(p) = &self.presentation {
                p.feedback(
                    s.layer.wl_surface(),
                    qh,
                    PresentationData {
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        update: s.ui_serial,
                        surface: s.layer.wl_surface().clone(),
                        generation: scene.resource_revision,
                    },
                );
            }
            b.buffer.attach_to(s.layer.wl_surface())?;
            s.layer.commit();
            b.pending.clear();
            s.previous = Some(current);
            s.frames += 1;
            Ok(())
        })();
        if self.surfaces[index].frames > previous_frames
            && self.started.elapsed().as_secs_f64() > 2.
        {
            if self.options.debug_luau_stall_ms.is_some() {
                println!(
                    "DEBUG_FRAME monotonic_ns={} companion={}",
                    monotonic_ns(),
                    self.surfaces[index].companion
                );
            }
            if self.options.trace_state && self.surfaces[index].companion {
                println!(
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
                    let remaining = if companion {
                        if app.panel_epoch != epoch {
                            return TimeoutAction::Drop;
                        }
                        app.companion
                            .as_mut()
                            .and_then(|(_, r, _)| r.trim_transient_idle())
                    } else {
                        app.renderer.trim_transient_idle()
                    };
                    remaining.map_or(TimeoutAction::Drop, TimeoutAction::ToDuration)
                },
            ) {
                self.fail(error);
            }
        }
        if let Err(e) = result {
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
                    drop(surface.gl.take());
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

#[cfg(test)]
mod repair_tests {
    use super::*;
    use bead_render::{Primitive, Rect, Renderer, Scene, tree::TreeCpuRenderer};
    use std::collections::BTreeMap;

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
                    for (_, pending) in &mut buffers {
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
                    let (bytes, pending) = &mut buffers[usize::from(frame) % 3];
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
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
        });
        scene.offset = (32., -18.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 50]));
        scene.offset = (32., -100.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 0]));
        scene.offset = (32., 32.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
    }
}

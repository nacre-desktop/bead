// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor, presentation, seat, keyboard and IME protocol dispatch.
// Handlers hand events for a detached pearl's surfaces to the same handler with that
// pearl swapped in, so some parameters are used only by that recursive call.
#![allow(clippy::only_used_in_recursion)]
use super::*;
delegate_noop!(App: ignore ZwpRelativePointerManagerV1);
impl Dispatch<ZwpRelativePointerV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion { dx, dy, .. } = event {
            state.gesture_relative_motion(dx, dy);
        }
    }
}
impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if let Some(id) = self.slot_of_surface(surface) {
            self.in_slot(id, |app| app.scale_factor_changed(conn, qh, surface, scale));
            return;
        }
        if self.window_surface() == Some(surface)
            && self.window.as_ref().is_some_and(|w| !w.has_fractional())
        {
            self.window_scale(scale.max(1) as u32 * 120);
            return;
        }
        if self.overlay_surface() == Some(surface)
            && self.overlay.as_ref().is_some_and(|o| !o.has_fractional())
        {
            self.overlay_scale(scale.max(1) as u32 * 120);
            return;
        }
        if self.canvas_surface(surface) && !self.canvas_has_fractional(surface) {
            self.canvas_scale(surface, scale.max(1) as u32 * 120);
            return;
        }
        if self
            .edge_surfaces
            .iter()
            .any(|s| s.layer.wl_surface() == surface)
        {
            // Integer scale only matters without wp_fractional_scale.
            return;
        }
        if let Some(i) = self.surfaces.iter().position(|s| s.wl_surface() == surface)
            && self.surfaces[i].fractional.is_none()
        {
            self.surfaces[i].scale120 = scale.max(1) as u32 * 120;
            if self.options.primary_factory.is_some() && !self.surfaces[i].companion {
                let id = self.surfaces[i].output_name.clone();
                if let Some(session) = self.output_sessions.get(&id) {
                    let deferred = session
                        .source
                        .as_ref()
                        .is_some_and(|s| s.deferred_updates());
                    self.ui(UiEvent::Output(
                        session.epoch,
                        id,
                        Box::new(UiEvent::Relayout),
                    ));
                    if deferred {
                        return;
                    }
                }
            }
            self.draw(i, qh);
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        time: u32,
    ) {
        if let Some(id) = self.slot_of_surface(surface) {
            self.in_slot(id, |app| app.frame(conn, qh, surface, time));
            return;
        }
        if let Some(i) = self.surfaces.iter().position(|s| s.wl_surface() == surface) {
            let now_seconds = self.started.elapsed().as_secs_f64();
            let animation_active = if self.surfaces[i].companion {
                self.panel_animations.active(now_seconds)
            } else if self.options.primary_factory.is_some() {
                self.output_sessions
                    .get(&self.surfaces[i].output_name)
                    .is_some_and(|session| session.animations.active(now_seconds))
            } else {
                self.animations.active(now_seconds)
            };
            let s = &mut self.surfaces[i];
            s.callbacks.pending = false;
            if bead_core::profile::frame_trace() {
                println!(
                    "UI_CALLBACK monotonic_ns={} companion={}",
                    bead_core::profile::monotonic_ns(),
                    s.companion
                );
            }
            let now = Instant::now();
            if self.started.elapsed().as_secs_f64() > 2.
                && let Some(last) = s.last_callback
            {
                s.callbacks_ms
                    .push(now.duration_since(last).as_secs_f64() * 1000.);
            }
            s.last_callback = Some(now);
            if !s.ready {
                s.ready = true;
                println!(
                    "READY output={} elapsed_ms={:.3}",
                    s.output.id().protocol_id(),
                    self.started.elapsed().as_secs_f64() * 1000.
                );
            }
            if self
                .options
                .seconds
                .is_some_and(|secs| self.started.elapsed().as_secs_f64() >= secs)
            {
                self.signal.stop();
            } else if self.options.animate
                || self.options.debug_continuous
                || s.redraw
                || (s.companion && self.options.lazy_gl && self.companion_policy.needs_warmup())
                || animation_active
            {
                self.draw(i, qh);
            }
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl Dispatch<WpFractionalScaleV1, wl_surface::WlSurface> for App {
    fn event(
        state: &mut Self,
        proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &wl_surface::WlSurface,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let Some(id) = state.slot_of_surface(surface) {
            state.in_slot(id, |app| Self::event(app, proxy, event, surface, conn, qh));
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.window_surface() == Some(surface)
        {
            state.window_scale(scale);
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.overlay_surface() == Some(surface)
        {
            state.overlay_scale(scale);
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.canvas_surface(surface)
        {
            state.canvas_scale(surface, scale);
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.edge_scale(surface, scale)
        {
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some(i) = state
                .surfaces
                .iter()
                .position(|s| s.wl_surface() == surface)
        {
            state.surfaces[i].scale120 = scale;
            if state.options.primary_factory.is_some() && !state.surfaces[i].companion {
                let id = state.surfaces[i].output_name.clone();
                if let Some(session) = state.output_sessions.get(&id) {
                    let deferred = session
                        .source
                        .as_ref()
                        .is_some_and(|s| s.deferred_updates());
                    state.ui(UiEvent::Output(
                        session.epoch,
                        id,
                        Box::new(UiEvent::Relayout),
                    ));
                    if deferred {
                        return;
                    }
                }
            }
            state.draw(i, qh);
        }
    }
}
impl Dispatch<WpPresentation, ()> for App {
    fn event(
        _: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            println!("PRESENTATION clock_id={clk_id}");
        }
    }
}
fn layout_input_point(visual_point: (f32, f32), content_offset: (f32, f32)) -> (f32, f32) {
    (
        visual_point.0 - content_offset.0,
        visual_point.1 - content_offset.1,
    )
}

impl App {
    fn record_first_frame(&mut self, surface: &wl_surface::WlSurface, revision: u64, epoch: u64) {
        let Some(index) = self.surfaces.iter().position(|s| s.wl_surface() == surface) else {
            return;
        };
        let companion = self.surfaces[index].companion;
        if (companion && epoch != self.panel_epoch)
            || self.surfaces[index].recovered_generation == Some((epoch, revision))
        {
            return;
        }
        if !companion && self.options.primary_factory.is_some() {
            let output = self.surfaces[index].output_name.clone();
            let source = if self.active_output.as_deref() == Some(output.as_str()) {
                self.runtime.as_mut()
            } else {
                self.output_sessions
                    .get_mut(&output)
                    .and_then(|session| session.source.as_mut())
            };
            if let Some(source) = source {
                source.presented(revision);
                self.surfaces[index].recovered_generation = Some((epoch, revision));
            }
            return;
        }
        let source = if companion {
            &mut self.panel_runtime
        } else {
            &mut self.runtime
        };
        if let Some(source) = source {
            source.presented(revision);
            self.surfaces[index].recovered_generation = Some((epoch, revision));
        }
    }
}

impl Dispatch<WpPresentationFeedback, PresentationData> for App {
    fn event(
        state: &mut Self,
        proxy: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &PresentationData,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let Some(id) = state.slot_of_surface(&data.surface) {
            state.in_slot(id, |app| Self::event(app, proxy, event, data, conn, qh));
            return;
        }
        if let Some(s) = state
            .surfaces
            .iter_mut()
            .find(|s| s.wl_surface() == &data.surface)
        {
            match event {
                wp_presentation_feedback::Event::Presented {
                    tv_sec_hi,
                    tv_sec_lo,
                    tv_nsec,
                    refresh,
                    flags,
                    ..
                } => {
                    let ns = ((u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo)) * 1_000_000_000
                        + u64::from(tv_nsec);
                    crate::scroll::note_refresh(refresh);
                    if state.started.elapsed().as_secs_f64() > 2.
                        && let Some(last) = s.last_presented
                        && ns > last
                    {
                        s.presented_ms.push((ns - last) as f64 / 1_000_000.);
                    }
                    if state.options.trace_state && s.companion {
                        bead_core::vprintln!("PANEL_PRESENTED monotonic_ns={ns}");
                    }
                    if state.options.debug_luau_stall_ms.is_some() {
                        bead_core::vprintln!(
                            "DEBUG_PRESENTED monotonic_ns={ns} companion={}",
                            s.companion
                        );
                    }
                    if s.presented_generation != Some(data.update) {
                        bead_core::vprintln!(
                            "UI_PRESENTED generation={} monotonic_ns={} refresh_ns={} companion={} update={} cpu_ns={}",
                            data.generation,
                            ns,
                            refresh,
                            s.companion,
                            data.update,
                            if bead_core::profile::enabled() {
                                bead_core::profile::process_cpu_ns()
                            } else {
                                0
                            }
                        );
                        s.presented_generation = Some(data.update);
                    }
                    s.last_presented = Some(ns);
                    s.refresh_ns = refresh;
                    s.presentation_kind = flags.into();
                }
                wp_presentation_feedback::Event::Discarded => s.discarded += 1,
                _ => {}
            }
        }
        if matches!(event, wp_presentation_feedback::Event::Presented { .. }) {
            state.record_first_frame(&data.surface, data.generation, data.source_epoch);
        }
    }
}
delegate_noop!(App: ignore WpFractionalScaleManagerV1);
delegate_noop!(App: ignore WpViewporter);
delegate_noop!(App: ignore WpViewport);
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(qh, output);
        self.restore_pearls();
        self.sync_edges();
        self.app_open_pending();
    }
    fn update_output(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.add_output(qh, output.clone());
        self.restore_pearls();
        self.sync_edges();
        self.app_open_pending();
        self.canvas_output_updated(&output);
        if self.options.primary_factory.is_some() {
            let id = self.output_id(&output);
            let viewport = self.output.info(&output).and_then(|info| info.logical_size);
            if let Some(session) = self.output_sessions.get_mut(&id)
                && let Some(source) = &mut session.source
                && let Some((width, height)) = viewport
                && width > 0
                && height > 0
                && source.viewport(width as u32, height as u32)
            {
                let epoch = session.epoch;
                self.ui(UiEvent::Output(epoch, id, Box::new(UiEvent::Relayout)));
            }
        }
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let output_id = self
            .surfaces
            .iter()
            .find(|surface| surface.output == output)
            .map(|surface| surface.output_name.clone())
            .unwrap_or_else(|| self.output_id(&output));
        if let Some(mut session) = self.output_sessions.remove(&output_id) {
            session.epoch = session.epoch.wrapping_add(1);
            session.source = None;
            if self.active_output.as_deref() == Some(output_id.as_str()) {
                self.active_output = None;
                self.active_output_epoch = None;
            }
            println!(
                "UI_SESSION_REMOVED output={output_id} epoch={}",
                session.epoch
            );
        }
        let finish_palette = if let Some(waiting) = &mut self.wallpaper_palette_outputs {
            waiting.expected.remove(&output_id);
            waiting.expected.is_subset(&waiting.settled)
        } else {
            false
        };
        if finish_palette && let Some(waiting) = self.wallpaper_palette_outputs.take() {
            self.wallpaper_palette_settled(&waiting.palette, waiting.accepted);
        }
        if self.panel_output.as_ref() == Some(&output) {
            self.release_panel();
        }
        self.drop_slots_on(&output);
        if self.overlay_on(&output) {
            self.release_overlay();
        }
        self.canvas_output_removed(&output);
        self.surfaces.retain(|s| s.output != output);
        self.edge_output_removed(&output);
        let removed: Vec<String> = self
            .wallpaper_surfaces
            .iter()
            .filter(|s| s.output == output)
            .map(|s| s.name.clone())
            .collect();
        self.wallpaper_surfaces.retain(|s| s.output != output);
        if let Some(provider) = &self.options.wallpaper {
            for name in &removed {
                provider.output_removed(name);
            }
        }
        for name in &removed {
            self.wallpaper_jobs.remove(name);
            self.wallpaper_dirty.remove(name);
        }
        let discarded: Vec<_> = self
            .wallpaper_pending_frames
            .iter()
            .filter(|frame| removed.contains(&frame.output))
            .map(|frame| (frame.output.clone(), frame.revision))
            .collect();
        self.wallpaper_pending_frames
            .retain(|frame| !removed.contains(&frame.output));
        if let Some(provider) = &self.options.wallpaper {
            for (name, revision) in discarded {
                provider.discarded(&name, revision);
            }
        }
        for name in &removed {
            println!("WALLPAPER_SURFACE_RELEASED output={name}");
        }
        println!("OUTPUT_REMOVED remaining={}", self.surfaces.len());
    }
}
impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, conn: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface) {
        // The dormant layer role is retained while the visible xdg pearl is open.
        if self
            .surfaces
            .iter()
            .any(|s| s.layer == *layer && s.native.is_some())
        {
            return;
        }
        if let Some(id) = self.slot_of_layer(layer) {
            self.in_slot(id, |app| app.closed(conn, qh, layer));
            return;
        }
        if self.edge_closed(layer) {
            return;
        }
        if self.overlay_layer_is(layer) {
            self.release_overlay();
            return;
        }
        if self.canvas_layer_is(layer) {
            self.canvas_release_layer(layer);
            return;
        }
        let removed: Vec<String> = self
            .wallpaper_surfaces
            .iter()
            .filter(|surface| surface.layer == *layer)
            .map(|surface| surface.name.clone())
            .collect();
        self.wallpaper_surfaces
            .retain(|surface| surface.layer != *layer);
        if let Some(provider) = &self.options.wallpaper {
            for name in &removed {
                provider.output_removed(name);
            }
        }
        for name in &removed {
            self.wallpaper_jobs.remove(name);
            self.wallpaper_dirty.remove(name);
        }
        let discarded: Vec<_> = self
            .wallpaper_pending_frames
            .iter()
            .filter(|frame| removed.contains(&frame.output))
            .map(|frame| (frame.output.clone(), frame.revision))
            .collect();
        self.wallpaper_pending_frames
            .retain(|frame| !removed.contains(&frame.output));
        if let Some(provider) = &self.options.wallpaper {
            for (name, revision) in discarded {
                provider.discarded(&name, revision);
            }
        }
        self.surfaces.retain(|s| s.layer != *layer);
    }
    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        config: LayerSurfaceConfigure,
        serial: u32,
    ) {
        if let Some(id) = self.slot_of_layer(layer) {
            self.in_slot(id, |app| app.configure(conn, qh, layer, config, serial));
            return;
        }
        if self.edge_configure(layer, config.new_size) {
            return;
        }
        if self.overlay_layer_is(layer) {
            self.overlay_configure(config.new_size);
            return;
        }
        if self.canvas_layer_is(layer) {
            self.canvas_configure(layer, config.new_size);
            return;
        }
        if let Some(surface) = self
            .wallpaper_surfaces
            .iter_mut()
            .find(|surface| surface.layer == *layer)
        {
            if let Some(viewport) = &surface.viewport {
                let destination = (
                    config.new_size.0.max(1) as i32,
                    config.new_size.1.max(1) as i32,
                );
                if surface.viewport_destination != Some(destination) {
                    viewport.set_destination(destination.0, destination.1);
                    surface.viewport_destination = Some(destination);
                }
            }
            return;
        }
        if let Some(dismiss) = &mut self.dismiss
            && dismiss.layer == *layer
        {
            if self.options.trace_state {
                println!(
                    "DISMISS_CONFIG width={} height={}",
                    config.new_size.0, config.new_size.1
                );
            }
            dismiss.viewport.set_destination(
                config.new_size.0.max(1) as i32,
                config.new_size.1.max(1) as i32,
            );
            let region = match Region::new(&self.compositor) {
                Ok(r) => r,
                Err(e) => {
                    self.fail(e);
                    return;
                }
            };
            if dismiss.enabled && !self.options.test_passive {
                let width = config.new_size.0.max(1) as i32;
                let height = config.new_size.1.max(1) as i32;
                let bar_extent = if self.options.bar {
                    self.surfaces
                        .iter()
                        .find(|surface| {
                            !surface.companion
                                && self.panel_output.as_ref() == Some(&surface.output)
                        })
                        .map(|surface| match self.options.edge {
                            bead_core::Edge::Top | bead_core::Edge::Bottom => surface.height as i32,
                            bead_core::Edge::Left | bead_core::Edge::Right => surface.width as i32,
                        })
                        .unwrap_or(0)
                        .clamp(
                            0,
                            if matches!(
                                self.options.edge,
                                bead_core::Edge::Top | bead_core::Edge::Bottom
                            ) {
                                height
                            } else {
                                width
                            },
                        )
                } else {
                    0
                };
                let (x, y, w, h) = match self.options.edge {
                    bead_core::Edge::Top => (0, bar_extent, width, height - bar_extent),
                    bead_core::Edge::Bottom => (0, 0, width, height - bar_extent),
                    bead_core::Edge::Left => (bar_extent, 0, width - bar_extent, height),
                    bead_core::Edge::Right => (0, 0, width - bar_extent, height),
                };
                if w > 0 && h > 0 {
                    region.add(x, y, w, h);
                }
            }
            layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
            layer.wl_surface().damage_buffer(0, 0, 1, 1);
            // The transparent pixel remains attached across configure events.
            // Reattaching its active SCTK buffer before release is an error;
            // only viewport and input-region state change on later configures.
            if !dismiss.attached {
                if let Err(e) = dismiss.buffer.attach_to(layer.wl_surface()) {
                    self.fail(e);
                    return;
                }
                dismiss.attached = true;
            }
            layer.commit();
            return;
        }
        if let Some(i) = self.surfaces.iter().position(|s| s.layer == *layer) {
            // Native pearl content is configured by its fresh xdg toplevel role.
            // This retained layer role is blank and exists for a later dock.
            if self.surfaces[i].native.is_some() {
                return;
            }
            let s = &mut self.surfaces[i];
            if (s.width, s.height) != config.new_size {
                // A compositor may park the old-size callback until a buffer
                // satisfying this configure arrives. Do not wait on that epoch.
                s.callbacks.invalidate();
                s.redraw = true;
            }
            s.width = config.new_size.0;
            s.height = config.new_size.1;
            s.configured = true;
            if let Some(bridge) = &mut self.accessibility {
                let bounds = accesskit::Rect {
                    x0: 0.,
                    y0: 0.,
                    x1: f64::from(s.width),
                    y1: f64::from(s.height),
                };
                bridge.adapter.set_root_window_bounds(bounds, bounds);
            }

            if self.surfaces[i].companion && self.panel_runtime.is_some() {
                let surface = &self.surfaces[i];
                let viewport = self
                    .output
                    .info(&surface.output)
                    .and_then(|i| i.logical_size)
                    .filter(|(w, h)| *w > 0 && *h > 0)
                    .map(|(w, h)| (w as u32, h as u32))
                    .unwrap_or((surface.width, surface.height));
                if let Some(source) = &mut self.panel_runtime {
                    source.viewport(viewport.0, viewport.1);
                }
                self.panel_ui(UiEvent::Relayout);
                return;
            }
            if self.options.primary_factory.is_some() && !self.surfaces[i].companion {
                let id = self.surfaces[i].output_name.clone();
                let viewport = self
                    .output
                    .info(&self.surfaces[i].output)
                    .and_then(|info| info.logical_size)
                    .filter(|(w, h)| *w > 0 && *h > 0);
                if let Some(session) = self.output_sessions.get_mut(&id) {
                    let initial_load = !session.initialized;
                    if let (Some(source), Some((w, h))) = (&mut session.source, viewport) {
                        source.viewport(w as u32, h as u32);
                    }
                    let event = if session.initialized {
                        UiEvent::Relayout
                    } else {
                        UiEvent::Reload
                    };
                    let epoch = session.epoch;
                    let apply_palette = initial_load && self.wallpaper_palette_outputs.is_none();
                    self.ui(UiEvent::Output(epoch, id, Box::new(event)));
                    if apply_palette && let Some(palette) = self.wallpaper_palette.clone() {
                        self.ui(UiEvent::Output(
                            epoch,
                            self.surfaces[i].output_name.clone(),
                            Box::new(UiEvent::WallpaperPalette(palette)),
                        ));
                    }
                }
                return;
            }
            // One shared view fits the smallest output; anonymous layout boxes never own state.
            let viewport = self
                .surfaces
                .iter()
                .filter_map(|surface| {
                    self.output
                        .info(&surface.output)
                        .and_then(|info| info.logical_size)
                })
                .fold(None, |old: Option<(i32, i32)>, size| {
                    Some(old.map_or(size, |old| (old.0.min(size.0), old.1.min(size.1))))
                });
            let viewport_changed =
                if let (Some(runtime), Some((w, h))) = (&mut self.runtime, viewport) {
                    runtime.viewport(w.max(1) as u32, h.max(1) as u32)
                } else {
                    false
                };
            if self.runtime.is_some() && !self.ui_initialized {
                self.ui_initialized = true;
                self.ui(UiEvent::Reload);
            } else if viewport_changed {
                self.ui(UiEvent::Relayout);
            } else {
                self.draw(i, qh);
            }
        }
    }
}
delegate_registry!(App);
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
smithay_client_toolkit::delegate_dispatch2!(App);
impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }
    fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.options.interactive {
            self.clipboard.seat(&seat, qh);
        }
    }
    fn new_capability(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if !self.options.interactive {
            return;
        }
        self.clipboard.seat(&seat, qh);
        match capability {
            Capability::Keyboard if self.keyboard.is_none() => {
                self.clipboard.seat(&seat, qh);
                if self.text_input.is_none()
                    && let Some(manager) = &self.text_input_manager
                {
                    self.text_input = Some(manager.get_text_input(&seat, qh, ()));
                    println!("IME_PROTOCOL text_input_v3=true");
                }

                // Older compositors send repeat_info, not repeated key events.
                // SCTK owns the timer, keymap repeatability and release/focus cancellation.
                let repeat_conn = conn.clone();
                let repeat_qh = qh.clone();
                match self.seat.get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(move |app, keyboard, event| {
                        app.repeat_key(
                            &repeat_conn,
                            &repeat_qh,
                            keyboard,
                            app.last_input_serial,
                            event,
                        );
                    }),
                ) {
                    Ok(k) => self.keyboard = Some(k),
                    Err(e) => self.fail(e),
                }
            }
            Capability::Pointer if self.pointer.is_none() => match self.seat.get_pointer(qh, &seat)
            {
                Ok(p) => {
                    if let Some(manager) = &self.relative_manager {
                        self.relative_pointer = Some(manager.get_relative_pointer(&p, qh, ()));
                    }
                    if let Some(manager) = &self.cursor_manager {
                        self.cursor_device = Some(manager.get_pointer(&p, qh, ()));
                    }
                    self.pointer = Some(p)
                }
                Err(e) => self.fail(e),
            },
            _ => {}
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard => {
                if let Some(k) = self.keyboard.take() {
                    k.release();
                    self.keyboard_surface = None;
                }
            }
            Capability::Pointer => {
                if let Some(relative) = self.relative_pointer.take() {
                    relative.destroy();
                }
                self.gesture_release();
                if let Some(p) = self.pointer.take() {
                    p.release();
                }
            }
            _ => {}
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            // B0048: a held Super+drag belongs to the gesture, wherever the panel is now.
            if self.gesture_owns(&event.surface) {
                if let PointerEventKind::Release { button: 0x110, .. } = event.kind {
                    self.gesture_release();
                }
                continue;
            }
            if matches!(event.kind, PointerEventKind::Press { button: 0x110, .. })
                && self.gesture_press(&event.surface, event.position)
            {
                self.pointer_pressed = false;
                continue;
            }
            if let Some(id) = self.slot_of_surface(&event.surface) {
                self.in_slot(id, |app| app.pointer_event(event));
            } else {
                self.pointer_event(event);
            }
        }
    }
}
impl App {
    fn pointer_event(&mut self, event: &PointerEvent) {
        bead_core::profile::position(
            "pointer_received",
            event.position.0 as f32,
            event.position.1 as f32,
        );
        bead_core::profile::mark("pointer_received");
        if self.window_surface() == Some(&event.surface) {
            self.window_pointer(event);
            return;
        }
        if self.overlay_surface() == Some(&event.surface) {
            self.overlay_pointer(event);
            return;
        }
        if self.canvas_surface(&event.surface) {
            self.canvas_pointer(event);
            return;
        }
        if self
            .dismiss
            .as_ref()
            .is_some_and(|d| d.layer.wl_surface() == &event.surface)
        {
            if matches!(event.kind, PointerEventKind::Enter { .. }) {
                self.view_event(true, UiEvent::PointerRegion(false));
            }
            if matches!(event.kind, PointerEventKind::Press { button: 0x110, .. }) {
                if self.options.trace_state {
                    println!("DISMISS_INPUT press=true");
                }
                if self
                    .panel_runtime
                    .as_ref()
                    .is_none_or(|r| r.outside_dismiss())
                {
                    self.close_panel();
                }
            }
            return;
        }
        if self.edge_pointer(event) {
            return;
        }
        let Some(bar) = self
            .surfaces
            .iter()
            .position(|s| s.wl_surface() == &event.surface)
        else {
            return;
        };
        if !self.surfaces[bar].companion {
            self.edge_bar_pointer(bar, event);
        }
        let surface = &self.surfaces[bar];
        let panel = surface.companion;
        let surface_size = (surface.width, surface.height);
        let offset = surface.offset;
        let input_content_offset = surface.input_content_offset;
        let configured = surface.configured;
        let surface_output_name = surface.output_name.clone();
        let hovered_output = self.output.info(&surface.output).and_then(|i| i.name);
        self.active_input_output = Some(surface_output_name.clone());
        if panel {
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.edge_panel_hover(true);
                    self.view_event(true, UiEvent::PointerRegion(true));
                    self.view_event(false, UiEvent::PointerRegion(true))
                }
                PointerEventKind::Leave { .. } => {
                    self.edge_panel_hover(false);
                    self.view_event(true, UiEvent::PointerRegion(false));
                    self.view_event(false, UiEvent::PointerRegion(false))
                }
                _ => {}
            }
        }
        if !panel {
            let source = if self.options.primary_factory.is_some() {
                self.output_sessions
                    .get(&surface_output_name)
                    .and_then(|s| s.source.as_ref())
            } else {
                self.runtime.as_ref()
            };
            let hovered = if matches!(event.kind, PointerEventKind::Leave { .. }) {
                None
            } else {
                source.and_then(|r| {
                    // Match UI hit testing: a child action wins over its clickable parent.
                    r.input_regions().into_iter().rev().find(|r| {
                        r.prepare_panel
                            && event.position.0 >= f64::from(r.rect.x)
                            && event.position.1 >= f64::from(r.rect.y)
                            && event.position.0 < f64::from(r.rect.x + r.rect.w)
                            && event.position.1 < f64::from(r.rect.y + r.rect.h)
                            && r.clip.is_none_or(|c| {
                                event.position.0 >= f64::from(c.x)
                                    && event.position.1 >= f64::from(c.y)
                                    && event.position.0 < f64::from(c.x + c.w)
                                    && event.position.1 < f64::from(c.y + c.h)
                            })
                    })
                })
            };
            let target = hovered
                .as_ref()
                .map(|r| r.prepare_view.clone().unwrap_or_default());
            let entered = target.is_some() && target != self.panel_hovered;
            self.panel_hovered = target;
            if entered && let Some(region) = hovered {
                if let Some(name) = region.prepare_view.as_deref() {
                    self.prepare_named_panel(
                        Some(name),
                        hovered_output.as_deref(),
                        region.prepare_center,
                    );
                } else {
                    self.prepare_panel(hovered_output.as_deref());
                }
            }
        }
        // Some compositors route all pointer events to an exclusive keyboard
        // layer, even outside its bounds. Treat that path like the backdrop.
        if panel
            && matches!(event.kind, PointerEventKind::Press { button: 0x110, .. })
            && self
                .panel_runtime
                .as_ref()
                .is_none_or(|r| r.outside_dismiss())
            && (event.position.0 < 0.
                || event.position.1 < 0.
                || event.position.0 >= surface_size.0 as f64
                || event.position.1 >= surface_size.1 as f64)
        {
            self.pointer_pressed = false;
            self.close_panel();
            return;
        }
        let visual_x = event.position.0 as f32 - offset.0;
        let visual_y = event.position.1 as f32 - offset.1;
        if !configured {
            return;
        }
        if panel
            && matches!(event.kind, PointerEventKind::Press { button: 0x110, .. })
            && self
                .panel_runtime
                .as_ref()
                .is_some_and(|r| r.dismiss_at(visual_x, visual_y))
        {
            self.close_panel();
            self.view_event(
                true,
                UiEvent::Click {
                    x: visual_x,
                    y: visual_y,
                },
            );
            return;
        }
        // Rendered reveal content moves while Session hit regions stay at their
        // layout positions. Map pointer events back to that layout; keep the
        // fixed contour's dismiss handling above in visual surface coordinates.
        let (x, y) = layout_input_point((visual_x, visual_y), input_content_offset);
        if let PointerEventKind::Enter { serial, .. } = event.kind {
            self.cursor_enter_serial = serial;
        }
        if let Some(device) = &self.cursor_device
            && self.cursor_enter_serial != 0
        {
            let output_name = self.active_input_output.as_deref();
            let source = if panel {
                self.panel_runtime.as_ref()
            } else if self.options.primary_factory.is_some() {
                output_name
                    .and_then(|name| self.output_sessions.get(name))
                    .and_then(|s| s.source.as_ref())
            } else {
                self.runtime.as_ref()
            };
            let shape = match source.map(|r| r.pointer_cursor(x, y)) {
                Some("text") => wp_cursor_shape_device_v1::Shape::Text,
                Some("pointer") => wp_cursor_shape_device_v1::Shape::Pointer,
                _ => wp_cursor_shape_device_v1::Shape::Default,
            };
            device.set_shape(self.cursor_enter_serial, shape);
        }
        match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                if self.pointer_pressed {
                    self.view_event(panel, UiEvent::Drag { x, y });
                } else {
                    self.view_event(panel, UiEvent::Hover { x, y });
                }
            }
            PointerEventKind::Leave { .. } => {
                self.pointer_pressed = false;
                self.view_event(panel, UiEvent::Hover { x: -1., y: -1. });
            }
            PointerEventKind::Press { button: 0x110, .. } => {
                self.pointer_pressed = true;
                self.view_event(panel, UiEvent::Click { x, y });
            }
            PointerEventKind::Release { button: 0x110, .. } => {
                self.pointer_pressed = false;
                self.view_event(panel, UiEvent::Release);
            }
            PointerEventKind::Press { button: 0x111, .. } => {
                self.view_event(panel, UiEvent::ContextMenu { x, y })
            }
            PointerEventKind::Press { button: 0x112, .. } => {
                self.view_event(panel, UiEvent::SecondaryClick { x, y })
            }
            PointerEventKind::Axis {
                vertical,
                horizontal,
                ..
            } => {
                let target = self.scroll_target(panel);
                self.axis_scroll(target.clone(), false, (x, y), vertical);
                self.axis_scroll(target, true, (x, y), horizontal);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod input_coordinate_tests {
    use super::*;

    #[test]
    fn pointer_events_follow_revealed_sliding_content() {
        let surface_offset = (32., 32.);
        let content_offset = bead_core::Edge::Bottom.closed_offset(590., 590., 0.25);
        let layout_point = (420., 180.);
        let compositor_point = (
            layout_point.0 + surface_offset.0 + content_offset.0,
            layout_point.1 + surface_offset.1 + content_offset.1,
        );
        let visual_point = (
            compositor_point.0 - surface_offset.0,
            compositor_point.1 - surface_offset.1,
        );
        let point = layout_input_point(visual_point, content_offset);
        assert_eq!(point, layout_point);
    }
}
impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.options.trace_state {
            println!("INPUT_ENTER surface={}", surface.id().protocol_id());
        }
        self.keyboard_surface = Some(surface.clone());
        if let Some(bridge) = &mut self.accessibility {
            bridge.adapter.update_window_focus_state(true);
        }
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.keyboard_surface = None;
        if let Some(bridge) = &mut self.accessibility {
            bridge.adapter.update_window_focus_state(false);
        }
    }
    fn press_key(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.last_input_serial = serial;
        if let Some(id) = self
            .keyboard_surface
            .clone()
            .and_then(|surface| self.slot_of_surface(&surface))
        {
            self.in_slot(id, |app| app.press_key(conn, qh, keyboard, serial, event));
            return;
        }
        if self.pearl.is_some() && self.modifiers.ctrl && event.keysym == Keysym::w {
            self.close_panel();
            return;
        }
        if bead_core::profile::enabled() {
            println!(
                "INPUT_RECEIVED monotonic_ns={} cpu_ns={}",
                bead_core::profile::monotonic_ns(),
                bead_core::profile::process_cpu_ns()
            );
        }
        if self.options.trace_state {
            println!(
                "INPUT_KEY sym={:?} focused_surface={}",
                event.keysym,
                self.keyboard_surface.is_some()
            );
        }
        if self.keyboard_surface.is_some()
            && self.keyboard_surface.as_ref() == self.window_surface()
        {
            self.window_key(&event);
            return;
        }
        if self
            .keyboard_surface
            .as_ref()
            .is_some_and(|surface| self.surfaces.iter().any(|s| s.wl_surface() == surface))
        {
            self.active_input_output = self.keyboard_surface.as_ref().and_then(|surface| {
                self.surfaces
                    .iter()
                    .find(|s| s.wl_surface() == surface)
                    .map(|s| s.output_name.clone())
            });
            let panel = self.keyboard_surface.as_ref().is_some_and(|surface| {
                self.surfaces
                    .iter()
                    .any(|s| s.companion && s.wl_surface() == surface)
            });
            let command = window::key_command(event.keysym, self.modifiers.ctrl);
            if let Some(command) = command {
                if panel
                    && ((command == "escape" && self.pearl.is_none())
                        || (command == "enter"
                            && self
                                .panel_runtime
                                .as_ref()
                                .is_some_and(|r| r.focused_dismiss())))
                {
                    self.close_panel();
                }
                self.view_event(
                    panel,
                    UiEvent::Command {
                        command: command.into(),
                        shift: self.modifiers.shift || event.keysym == Keysym::ISO_Left_Tab,
                    },
                );
            } else if !self.modifiers.ctrl && !self.modifiers.alt {
                self.view_event(
                    panel,
                    UiEvent::Key {
                        text: event.utf8.unwrap_or_default(),
                        backspace: event.keysym == Keysym::BackSpace,
                    },
                );
            }
        }
    }
    fn repeat_key(
        &mut self,
        c: &Connection,
        qh: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        s: u32,
        event: KeyEvent,
    ) {
        self.press_key(c, qh, k, s, event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl Dispatch<ZwpTextInputV3, ()> for App {
    fn event(
        state: &mut Self,
        input: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                state.active_input_output = state
                    .surfaces
                    .iter()
                    .find(|s| s.wl_surface() == &surface)
                    .map(|s| s.output_name.clone());
                state.ime_surface = Some(surface);
                state.sync_ime();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.ime_surface = None;
                input.disable();
                input.commit();
                state.ime_last = None;
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                state.ime_pending.0 = text.unwrap_or_default()
            }
            zwp_text_input_v3::Event::CommitString { text } => state.ime_pending.1 = text,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.ime_pending.2 = before_length;
                state.ime_pending.3 = after_length;
            }
            zwp_text_input_v3::Event::Done { .. } => {
                let (preedit, commit, before, after) = std::mem::take(&mut state.ime_pending);
                state.view_event(
                    false,
                    UiEvent::Ime {
                        preedit,
                        commit,
                        before,
                        after,
                    },
                );
            }
            _ => {}
        }
    }
}
wayland_client::delegate_noop!(App:ZwpTextInputManagerV3);

wayland_client::delegate_noop!(App:WpCursorShapeManagerV1);
wayland_client::delegate_noop!(App:WpCursorShapeDeviceV1);

impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, FrameData> for App {
    fn event(
        state: &mut Self,
        proxy: &wayland_client::protocol::wl_callback::WlCallback,
        event: wayland_client::protocol::wl_callback::Event,
        data: &FrameData,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let Some(id) = state.slot_of_surface(&data.surface) {
            state.in_slot(id, |app| Self::event(app, proxy, event, data, conn, qh));
            return;
        }
        if matches!(
            &event,
            wayland_client::protocol::wl_callback::Event::Done { .. }
        ) && state.canvas_surface(&data.surface)
        {
            state.canvas_frame(&data.surface, data.epoch);
            return;
        }
        if let wayland_client::protocol::wl_callback::Event::Done { callback_data } = event
            && let Some(surface) = state
                .surfaces
                .iter_mut()
                .find(|s| s.wl_surface() == &data.surface)
            && surface.callbacks.accept(data.epoch)
        {
            if state.presentation.is_none() {
                // Callback acknowledgement is only the persistence fallback; never a
                // substitute for wp_presentation timestamps in performance proofs.
                state.record_first_frame(&data.surface, data.generation, data.source_epoch);
            }
            CompositorHandler::frame(state, conn, qh, &data.surface, callback_data);
        }
    }
}

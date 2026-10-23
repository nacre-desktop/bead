// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor, presentation, seat, keyboard and IME protocol dispatch.
use super::*;
impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if self.window_surface() == Some(surface)
            && self.window.as_ref().is_some_and(|w| !w.has_fractional())
        {
            self.window_scale(scale.max(1) as u32 * 120);
            return;
        }
        if let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
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
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        {
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
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && state.window_surface() == Some(surface)
        {
            state.window_scale(scale);
            return;
        }
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some(i) = state
                .surfaces
                .iter()
                .position(|s| s.layer.wl_surface() == surface)
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
impl App {
    fn record_first_frame(&mut self, surface: &wl_surface::WlSurface, revision: u64, epoch: u64) {
        let Some(index) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        else {
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
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &PresentationData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(s) = state
            .surfaces
            .iter_mut()
            .find(|s| s.layer.wl_surface() == &data.surface)
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
                    if state.started.elapsed().as_secs_f64() > 2.
                        && let Some(last) = s.last_presented
                        && ns > last
                    {
                        s.presented_ms.push((ns - last) as f64 / 1_000_000.);
                    }
                    if state.options.trace_state && s.companion {
                        println!("PANEL_PRESENTED monotonic_ns={ns}");
                    }
                    if state.options.debug_luau_stall_ms.is_some() {
                        println!(
                            "DEBUG_PRESENTED monotonic_ns={ns} companion={}",
                            s.companion
                        );
                    }
                    if s.presented_generation != Some(data.update) {
                        println!(
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
    }
    fn update_output(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.add_output(qh, output.clone());
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
        self.surfaces.retain(|s| s.output != output);
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
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
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
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        config: LayerSurfaceConfigure,
        _: u32,
    ) {
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
            if !self.options.test_passive {
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
        _: &Connection,
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

                match self.seat.get_keyboard(qh, &seat, None) {
                    Ok(k) => self.keyboard = Some(k),
                    Err(e) => self.fail(e),
                }
            }
            Capability::Pointer if self.pointer.is_none() => match self.seat.get_pointer(qh, &seat)
            {
                Ok(p) => {
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
            if self.window_surface() == Some(&event.surface) {
                self.window_pointer(event);
                continue;
            }
            if self
                .dismiss
                .as_ref()
                .is_some_and(|d| d.layer.wl_surface() == &event.surface)
            {
                if matches!(event.kind, PointerEventKind::Press { button: 0x110, .. }) {
                    if self.options.trace_state {
                        println!("DISMISS_INPUT press=true");
                    }
                    self.close_panel();
                }
                continue;
            }
            let Some(surface) = self
                .surfaces
                .iter()
                .find(|s| s.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            let panel = surface.companion;
            let surface_size = (surface.width, surface.height);
            let offset = surface.offset;
            let configured = surface.configured;
            let surface_output_name = surface.output_name.clone();
            let hovered_output = self.output.info(&surface.output).and_then(|i| i.name);
            self.active_input_output = Some(surface_output_name.clone());
            if panel {
                match event.kind {
                    PointerEventKind::Enter { .. } => {
                        self.view_event(false, UiEvent::PointerRegion(true))
                    }
                    PointerEventKind::Leave { .. } => {
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
                        r.input_regions().into_iter().find(|r| {
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
                && (event.position.0 < 0.
                    || event.position.1 < 0.
                    || event.position.0 >= surface_size.0 as f64
                    || event.position.1 >= surface_size.1 as f64)
            {
                self.pointer_pressed = false;
                self.close_panel();
                continue;
            }
            let x = event.position.0 as f32 - offset.0;
            let y = event.position.1 as f32 - offset.1;
            if !configured {
                continue;
            }
            if panel
                && matches!(event.kind, PointerEventKind::Press { button: 0x110, .. })
                && self
                    .panel_runtime
                    .as_ref()
                    .is_some_and(|r| r.dismiss_at(x, y))
            {
                self.close_panel();
                self.view_event(true, UiEvent::Click { x, y });
                continue;
            }
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
                PointerEventKind::Release { button: 0x110, .. } => self.pointer_pressed = false,
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
                    if vertical.absolute != 0. {
                        self.view_event(
                            panel,
                            UiEvent::Scroll {
                                x,
                                y,
                                delta: vertical.absolute,
                            },
                        );
                    }
                    if horizontal.absolute != 0. {
                        self.view_event(
                            panel,
                            UiEvent::HorizontalScroll {
                                x,
                                y,
                                delta: horizontal.absolute,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
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
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.last_input_serial = serial;
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
        if self.keyboard_surface.as_ref().is_some_and(|surface| {
            self.surfaces
                .iter()
                .any(|s| s.layer.wl_surface() == surface)
        }) {
            self.active_input_output = self.keyboard_surface.as_ref().and_then(|surface| {
                self.surfaces
                    .iter()
                    .find(|s| s.layer.wl_surface() == surface)
                    .map(|s| s.output_name.clone())
            });
            let panel = self.keyboard_surface.as_ref().is_some_and(|surface| {
                self.surfaces
                    .iter()
                    .any(|s| s.companion && s.layer.wl_surface() == surface)
            });
            let command = window::key_command(event.keysym, self.modifiers.ctrl);
            if let Some(command) = command {
                if panel
                    && (command == "escape"
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
                    .find(|s| s.layer.wl_surface() == &surface)
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
        _: &wayland_client::protocol::wl_callback::WlCallback,
        event: wayland_client::protocol::wl_callback::Event,
        data: &FrameData,
        conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_callback::Event::Done { callback_data } = event
            && let Some(surface) = state
                .surfaces
                .iter_mut()
                .find(|s| s.layer.wl_surface() == &data.surface)
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

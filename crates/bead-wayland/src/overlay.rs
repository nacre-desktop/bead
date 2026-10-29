// SPDX-License-Identifier: MIT OR Apache-2.0
//! B0053: the overlay slot, a second view slot beside the one resident panel.
//!
//! An overlay view is a content-sized layer surface on one output that coexists with
//! whatever the panel slot shows (launcher, dashboard, popout). The application
//! catalogs its views ([`Options::overlays`]) and offers them with
//! [`UiEvent::CompanionOffer`]; the framework knows no view names, placement or
//! suppression rule. Like the window slot, nothing is resident while no overlay view is
//! shown: closing animates out, then drops the source, renderer and every buffer.
use super::*;

pub(super) struct Overlay {
    name: String,
    epoch: u64,
    layer: LayerSurface,
    output: wl_output::WlOutput,
    source: Box<dyn SceneSource>,
    scene: Scene,
    renderer: Box<dyn Renderer>,
    animations: bead_core::animation::Animations,
    pool: Option<SlotPool>,
    buffers: Vec<Buffer>,
    damage: crate::buffer_damage::BufferDamage,
    ui_serial: u64,
    presented_update: Option<u64>,
    geometry: (i32, i32),
    /// Logical size last requested from the compositor and the one it configured.
    asked: (u32, u32),
    size: (u32, u32),
    configured: bool,
    scale120: u32,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    /// Target visibility; false while the close animation runs.
    open: bool,
    close_token: u64,
    passive: bool,
    position: Option<PanelPositioner>,
    input: Option<(bool, Option<[i32; 4]>)>,
    offset: (f32, f32),
    pressed: bool,
    cursor_serial: u32,
    last_draw: Option<Instant>,
    timer: bool,
}
impl Overlay {
    pub(super) fn output_mode_changed(&mut self, output: &wl_output::WlOutput, interval: Duration) {
        if self.output == *output {
            self.source.frame_interval(interval);
        }
    }
    pub(super) fn viewport_size(&self) -> (u32, u32) {
        self.size
    }
    pub(super) fn scroll_regions(&self) -> Vec<Rect> {
        self.source.scroll_regions()
    }
    pub(super) fn wheel_step(&self) -> f64 {
        self.source.wheel_step()
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(super) fn has_fractional(&self) -> bool {
        self.fractional.is_some()
    }
}
impl Drop for Overlay {
    fn drop(&mut self) {
        if let Some(f) = self.fractional.take() {
            f.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
    }
}

/// One journal line per finished slide, so a live session shows what the user saw (B0170).
pub(super) fn log_slide(id: &str, slide: Option<bead_core::animation::Slide>) {
    if let Some(slide) = slide {
        eprintln!("PANEL_SLIDE name={id} {slide}");
    }
}
/// Logical surface size for a content-sized scene (same rule as a panel's).
fn content_size(scene: &Scene) -> (u32, u32) {
    let height = scene.bounds.y + scene.bounds.h + scene.offset.1;
    (
        scene.bounds.w.ceil().max(1.) as u32,
        height.ceil().max(1.) as u32,
    )
}

impl App {
    pub(super) fn overlay_presentation(
        &mut self,
        data: &PresentationData,
        event: &wp_presentation_feedback::Event,
    ) -> bool {
        let Some(o) = self
            .overlay
            .as_mut()
            .filter(|o| o.epoch == data.source_epoch && o.layer.wl_surface() == &data.surface)
        else {
            return false;
        };
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                ..
            } => {
                crate::scroll::note_refresh(*refresh);
                let ns = ((u64::from(*tv_sec_hi) << 32) | u64::from(*tv_sec_lo)) * 1_000_000_000
                    + u64::from(*tv_nsec);
                if bead_core::profile::frame_trace() {
                    println!(
                        "WINDOW_PRESENTED monotonic_ns={ns} epoch={} update={} generation={} refresh_ns={} kind=overlay",
                        o.epoch, data.update, data.generation, refresh
                    );
                }
                if let Some(revision) = o.damage.presented(data.update) {
                    o.source.presented(revision);
                }
                if o.presented_update.is_none_or(|last| data.update > last) {
                    o.source.presented(data.generation);
                    o.presented_update = Some(data.update);
                }
            }
            wp_presentation_feedback::Event::Discarded => {
                bead_core::vprintln!(
                    "WINDOW_DISCARDED monotonic_ns={} epoch={} update={} generation={} kind=overlay",
                    super::monotonic_ns(),
                    o.epoch,
                    data.update,
                    data.generation
                );
            }
            _ => {}
        }
        true
    }
    pub(super) fn overlay_surface(&self) -> Option<&wl_surface::WlSurface> {
        self.overlay.as_ref().map(|o| o.layer.wl_surface())
    }
    pub(super) fn overlay_layer_is(&self, layer: &LayerSurface) -> bool {
        self.overlay.as_ref().is_some_and(|o| &o.layer == layer)
    }
    pub(super) fn overlay_on(&self, output: &wl_output::WlOutput) -> bool {
        self.overlay.as_ref().is_some_and(|o| &o.output == output)
    }
    pub(super) fn is_overlay_view(&self, view: &str) -> bool {
        self.options
            .overlays
            .as_ref()
            .is_some_and(|views| views.contains(view))
    }
    /// The named output when present, else the invoking one (B0194), else the first.
    fn overlay_output(&self, name: Option<&str>) -> Option<wl_output::WlOutput> {
        let by_name = |wanted: &str| {
            self.output
                .outputs()
                .find(|o| self.output.info(o).and_then(|i| i.name).as_deref() == Some(wanted))
        };
        let resolved = self.invocation_output(name);
        resolved
            .as_deref()
            .and_then(by_name)
            .or_else(|| self.output.outputs().next())
    }
    /// Service offer for an overlay view. A shown view keeps its output (new offers
    /// choose it only when the slot is idle); a different view replaces the shown one.
    pub(super) fn overlay_offer(&mut self, view: &str, output: Option<&str>, open: bool) {
        if !open {
            if self
                .overlay
                .as_ref()
                .is_some_and(|o| o.name == view && o.open)
            {
                self.overlay_close();
            }
            return;
        }
        if self.hidden {
            return;
        }
        if let Some(shown) = &mut self.overlay {
            if shown.name == view {
                // Re-offered while fading out: stay.
                if !shown.open {
                    shown.open = true;
                    shown.close_token = shown.close_token.wrapping_add(1);
                    self.overlay_ui(UiEvent::PanelVisibility(true));
                }
                return;
            }
            self.release_overlay();
        }
        if let Err(error) = self.open_overlay(view, output) {
            eprintln!("COMPANION_ERROR {error}");
        }
    }
    fn open_overlay(&mut self, view: &str, output: Option<&str>) -> Result<(), String> {
        let layer_kind = self
            .options
            .overlays
            .as_ref()
            .ok_or("overlay views are not configured")?
            .layer(view);
        let spec = self
            .options
            .overlays
            .as_ref()
            .ok_or("overlay views are not configured")?
            .resolve(Some(view))?
            .1
            .clone();
        let output = self
            .overlay_output(output)
            .ok_or("overlay output not available")?;
        let qh = self.qh.clone();
        let surface = self.compositor.create_surface(&qh);
        // Unmapped until the first scene: never intercept input before then.
        let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
        surface.set_input_region(Some(empty.wl_region()));
        let fractional = if self.viewporter.is_some() {
            self.fractional
                .as_ref()
                .map(|m| m.get_fractional_scale(&surface, &qh, surface.clone()))
        } else {
            None
        };
        let viewport = if fractional.is_some() {
            self.viewporter
                .as_ref()
                .map(|m| m.get_viewport(&surface, &qh, ()))
        } else {
            None
        };
        let layer = self.layers.create_layer_surface(
            &qh,
            surface,
            layer_kind,
            Some(&self.options.namespace),
            Some(&output),
        );
        let size = content_size(&spec.fallback);
        layer.set_size(size.0, size.1);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        self.place_overlay(&layer, &output, &spec.fallback, spec.position.as_ref());
        layer.commit();
        self.overlay_epoch = self.overlay_epoch.wrapping_add(1);
        let epoch = self.overlay_epoch;
        let factory = spec.factory.clone();
        let mut source: Box<dyn SceneSource> = Box::new(worker::Source::new(
            Box::new(move || factory(true)),
            spec.fallback.clone(),
        ));
        let sender = self
            .event_sender
            .clone()
            .ok_or("UI event channel unavailable")?;
        let notify = sender.clone();
        source.services(Box::new(move |event| {
            let _ = notify.send(UiEvent::Overlay(epoch, Box::new(event)));
        }));
        source
            .watch(Box::new(move |result| {
                let event = match result {
                    Ok(()) => UiEvent::Reload,
                    Err(e) => UiEvent::WatchError(e),
                };
                let _ = sender.send(UiEvent::Overlay(epoch, Box::new(event)));
            }))
            .map_err(|e| e.to_string())?;
        let viewport_size = self
            .output
            .info(&output)
            .and_then(|i| i.logical_size)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .map_or(size, |(w, h)| (w as u32, h as u32));
        source.viewport(viewport_size.0, viewport_size.1);
        // B0188: motion inside the view (the OSD level fill) ticks at the display's rate, not
        // the 16 ms default every other slot already overrides.
        source.frame_interval(self.output_frame_interval(&output));
        let renderer = bead_render::renderer(RendererKind::Cpu).map_err(|e| e.to_string())?;
        let scale120 = self
            .output
            .info(&output)
            .map_or(120, |i| i.scale_factor.max(1) as u32 * 120);
        self.overlay = Some(Overlay {
            name: view.to_owned(),
            epoch,
            layer,
            output,
            source,
            scene: spec.fallback.clone(),
            renderer,
            animations: Default::default(),
            pool: None,
            buffers: Vec::new(),
            damage: Default::default(),
            ui_serial: 0,
            presented_update: None,
            geometry: (0, 0),
            asked: size,
            size,
            configured: false,
            scale120,
            fractional,
            viewport,
            open: true,
            close_token: 0,
            passive: spec.passive,
            position: spec.position.clone(),
            input: None,
            offset: (0., 0.),
            pressed: false,
            cursor_serial: 0,
            last_draw: None,
            timer: false,
        });
        if let Some(palette) = self.wallpaper_palette.clone() {
            self.overlay_ui(UiEvent::WallpaperPalette(palette));
        }
        self.overlay_ui(UiEvent::Reload);
        self.overlay_ui(UiEvent::PanelVisibility(true));
        println!("OVERLAY_OPEN view={view} epoch={epoch}");
        Ok(())
    }
    fn place_overlay(
        &self,
        layer: &LayerSurface,
        output: &wl_output::WlOutput,
        scene: &Scene,
        position: Option<&PanelPositioner>,
    ) {
        let logical = self.output.info(output).and_then(|i| i.logical_size);
        if let Some(position) = position
            && let Some(size) = logical
        {
            let (x, y) = position(scene, None, size);
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_margin(y, 0, 0, x);
            return;
        }
        let (anchor, [top, right, bottom, left]) = panels::placement(
            scene.motion.as_ref().map_or(self.options.edge, |m| m.edge),
            None,
            content_size(scene),
            logical,
        );
        layer.set_anchor(anchor);
        layer.set_margin(top, right, bottom, left);
    }
    pub(super) fn overlay_close(&mut self) {
        let Some(overlay) = self.overlay.as_mut() else {
            return;
        };
        if !overlay.open {
            return;
        }
        overlay.open = false;
        overlay.close_token = overlay.close_token.wrapping_add(1);
        let (epoch, token) = (overlay.epoch, overlay.close_token);
        let fade = overlay.scene.motion.as_ref().map_or(0., |m| m.duration);
        self.overlay_ui(UiEvent::PanelVisibility(false));
        // Let the fade finish, then release everything. A re-offer cancels it.
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(
                u64::from((fade * 1000.) as u32) + 250,
            )),
            move |_, _, app| {
                if app
                    .overlay
                    .as_ref()
                    .is_some_and(|o| o.epoch == epoch && o.close_token == token && !o.open)
                {
                    app.release_overlay();
                }
                TimeoutAction::Drop
            },
        );
    }
    pub(super) fn release_overlay(&mut self) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        let output = overlay.output.clone();
        drop(overlay);
        // With no view of either slot up, the primary session may shed its warm caches,
        // as it does when the panel slot closes.
        if !self.panel_target && self.panel_runtime.is_none() {
            self.primary_idle(&output, true);
        }
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: allocator trim only; the source, renderer and buffers are gone.
        unsafe {
            libc::malloc_trim(0);
        }
        // Worker packets queued before the release still own rasters: trim after they drain.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(150)),
            |_, _, app| {
                if app.overlay.is_none() {
                    // SAFETY: allocator trim only.
                    unsafe {
                        libc::malloc_trim(0);
                    }
                }
                TimeoutAction::Drop
            },
        );
        println!("OVERLAY_CLOSED resident=0");
    }
    pub(super) fn overlay_scale(&mut self, scale120: u32) {
        if let Some(o) = self.overlay.as_mut()
            && o.scale120 != scale120
        {
            o.scale120 = scale120;
            o.damage.reset(
                o.geometry.0.max(0) as u32,
                o.geometry.1.max(0) as u32,
                o.buffers.len(),
            );
            self.overlay_ui(UiEvent::Relayout);
        }
    }
    pub(super) fn overlay_configure(&mut self, size: (u32, u32)) {
        let Some(o) = self.overlay.as_mut() else {
            return;
        };
        o.size = if size.0 == 0 || size.1 == 0 {
            o.asked
        } else {
            size
        };
        o.configured = true;
        if let Some(v) = &o.viewport {
            v.set_destination(o.size.0 as i32, o.size.1 as i32);
        }
        self.overlay_draw();
    }
    /// Service wakeups and input for the overlay; the product source stages them.
    pub(super) fn overlay_ui(&mut self, event: UiEvent) {
        if let UiEvent::ResourcesPrepared(result) = event {
            match result {
                Ok(()) => self.overlay_draw(),
                Err(error) => eprintln!("OVERLAY_DRAW_ERROR {error}"),
            }
            return;
        }
        let Some(o) = self.overlay.as_mut() else {
            return;
        };
        let scale = o.scale120 as f32 / 120.;
        o.source.scales(vec![scale]);
        let staged = o.source.stage(event).and_then(|scene| {
            if let Some(scene) = &scene {
                o.renderer.prepare(scene, scale)?;
            }
            o.source.validate()?;
            Ok(scene)
        });
        let scene = match staged {
            Ok(Some(scene)) => {
                if !o.source.commit() {
                    return;
                }
                scene
            }
            Ok(None) => return,
            Err(e) => {
                eprintln!("UI_ERROR {e}");
                o.source.error_scene(e.to_string())
            }
        };
        o.scene = scene;
        o.ui_serial = o.ui_serial.wrapping_add(1);
        if self.options.trace_state {
            bead_core::vprintln!(
                "WINDOW_STATE monotonic_ns={} epoch={} update={} generation={} kind=overlay state={}",
                super::monotonic_ns(),
                o.epoch,
                o.ui_serial,
                o.scene.resource_revision,
                o.source.state_summary()
            );
        }
        // A content change resizes the surface; the draw waits for its configure.
        let wanted = content_size(&o.scene);
        if wanted != o.asked {
            o.asked = wanted;
            o.layer.set_size(wanted.0, wanted.1);
            if let Some(o) = self.overlay.as_ref() {
                self.place_overlay(&o.layer, &o.output, &o.scene, o.position.as_ref());
                o.layer.commit();
            }
            return;
        }
        self.overlay_draw();
    }
    fn overlay_retry(&mut self, after: Duration) {
        let Some(o) = self.overlay.as_mut() else {
            return;
        };
        if o.timer {
            return;
        }
        o.timer = true;
        let epoch = o.epoch;
        let _ = self
            .loop_handle
            .insert_source(Timer::from_duration(after), move |_, _, app| {
                if let Some(o) = app.overlay.as_mut().filter(|o| o.epoch == epoch) {
                    o.timer = false;
                    app.overlay_draw();
                }
                TimeoutAction::Drop
            });
    }
    fn overlay_draw(&mut self) {
        let Some(o) = self.overlay.as_mut() else {
            return;
        };
        if !o.configured || self.hidden {
            return;
        }
        if let Some(last) = o.last_draw {
            let delay = super::scroll::tick().saturating_sub(last.elapsed());
            if !delay.is_zero() {
                self.overlay_retry(delay);
                return;
            }
        }
        match self.overlay_render() {
            Ok(animating) => {
                if animating {
                    self.overlay_retry(super::scroll::tick());
                }
            }
            Err(e) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    match super::rendering::prepare_pending_images(
                        e.as_ref(),
                        overlay.source.as_mut(),
                    ) {
                        Ok(true) => return,
                        Ok(false) => (),
                        Err(error) => {
                            eprintln!("OVERLAY_DRAW_ERROR {error}");
                            return;
                        }
                    }
                }
                eprintln!("OVERLAY_DRAW_ERROR {e}");
            }
        }
    }
    /// Draws one frame; true while a fade is still running.
    fn overlay_render(&mut self) -> Result<bool, Box<dyn Error>> {
        let frame_started = Instant::now();
        let rgba = cfg!(target_endian = "little")
            && !self.options.force_argb
            && self.shm.formats().contains(&wl_shm::Format::Abgr8888);
        let format = if rgba {
            PixelFormat::Rgba
        } else {
            PixelFormat::Bgra
        };
        let started = self.started;
        let o = self.overlay.as_mut().ok_or("overlay closed")?;
        let now = started.elapsed().as_secs_f64();
        let scale = o.scale120 as f32 / 120.;
        let (w, h, stride, size) = buffer_dimensions(o.size.0, o.size.1, o.scale120)?;
        let scene = &mut o.scene;
        let mut animating = false;
        if let Some(motion) = scene.motion.clone() {
            let prior_x = scene.offset.0;
            let (duration, curve) = motion.transition(o.open);
            o.animations
                .open_to(&motion.id, o.open, now, duration, curve);
            let (progress, active) = o
                .animations
                .value(&motion.id, now)
                .ok_or("motion value missing")?;
            scene.opacity = motion.opacity(progress);
            let (dx, dy) = scene.panel_offset(progress, o.open);
            animating = active;
            scene.offset = (32. + dx, 32. + dy);

            if matches!(motion.edge, bead_core::Edge::Top | bead_core::Edge::Bottom) {
                scene.offset.0 = if o.open {
                    ((o.size.0 as f32 - scene.width) / 2.).max(0.)
                } else {
                    prior_x
                };
            }
        }
        o.offset = scene.offset;
        // Input only where the card is, and only while it is shown and interactive.
        let visible = o.open && scene.opacity > 0.001 && !self.options.test_passive;
        let rect = if visible && !o.passive {
            rendering::content_input_rect(scene)
        } else {
            None
        };
        if o.input != Some((visible, rect)) {
            let region = Region::new(&self.compositor)?;
            if let Some([x, y, w, h]) = rect
                && w > 0
                && h > 0
            {
                region.add(x, y, w, h);
            }
            o.layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
            o.input = Some((visible, rect));
        }
        if o.geometry != (w, h) || o.buffers.is_empty() {
            o.buffers.clear();
            let mut pool = SlotPool::new(size.next_multiple_of(64) * 3, &self.shm)?;
            for _ in 0..3 {
                let (buffer, _) = pool.create_buffer(
                    w,
                    h,
                    stride,
                    if rgba {
                        wl_shm::Format::Abgr8888
                    } else {
                        wl_shm::Format::Argb8888
                    },
                )?;
                o.buffers.push(buffer);
            }
            o.pool = Some(pool);
            o.geometry = (w, h);
            o.damage.reset(w as u32, h as u32, o.buffers.len());
        }
        o.renderer.prepare(scene, scale)?;
        o.damage.update(
            o.renderer.prepared_frame().as_deref(),
            scene.offset,
            scene.opacity,
            w as u32,
            h as u32,
        )?;
        let motion_id = scene.motion.as_ref().map(|motion| motion.id.clone());
        if o.damage.surface.w == 0 || o.damage.surface.h == 0 {
            if let Some(revision) = o.damage.identical_generation(scene.resource_revision) {
                o.source.presented(revision);
            }
            // The pose is already on screen.
            if let Some(id) = &motion_id {
                log_slide(
                    id,
                    o.animations
                        .committed(id, now, started.elapsed().as_secs_f64()),
                );
            }
            return Ok(animating);
        }
        let pool = o.pool.as_mut().ok_or("overlay pool missing")?;
        let Some(slot) = o.buffers.iter().enumerate().position(|(index, b)| {
            !b.slot().has_active_buffers()
                && o.damage.pending[index].w > 0
                && o.damage.pending[index].h > 0
        }) else {
            // Every buffer is still held by the compositor: try again shortly.
            return Ok(true);
        };
        let surface_damage = o.damage.surface;
        let damage = o
            .damage
            .repair(pool, &o.buffers, slot, w as u32, h as u32)?;
        let canvas = o.buffers[slot]
            .canvas(pool)
            .ok_or("compositor owns buffer")?;
        o.renderer.draw(
            scene,
            RenderTarget::Shm(ShmTarget {
                bytes: &mut *canvas,
                width: w as u32,
                height: h as u32,
                scale,
                format,
            }),
            damage,
        )?;
        let surface = o.layer.wl_surface();
        if o.viewport.is_some() {
            surface.set_buffer_scale(1);
        } else {
            surface.set_buffer_scale((o.scale120 / 120).max(1) as i32);
        }
        surface.damage_buffer(
            surface_damage.x as i32,
            surface_damage.y as i32,
            surface_damage.w as i32,
            surface_damage.h as i32,
        );
        if let Some(presentation) = &self.presentation {
            presentation.feedback(
                surface,
                &self.qh,
                PresentationData {
                    visibility: 0,
                    source_epoch: o.epoch,
                    update: o.ui_serial,
                    surface: surface.clone(),
                    generation: scene.resource_revision,
                },
            );
        }
        o.buffers[slot].attach_to(surface)?;
        o.layer.commit();
        if let Some(id) = &motion_id {
            log_slide(
                id,
                o.animations
                    .committed(id, now, started.elapsed().as_secs_f64()),
            );
        }
        o.damage.committed(slot, o.ui_serial);
        o.last_draw = Some(frame_started);
        if bead_core::profile::frame_trace() {
            println!(
                "WINDOW_FRAME monotonic_ns={} epoch={} update={} generation={} kind=overlay render_ms={:.3} damage={},{},{},{}",
                super::monotonic_ns(),
                o.epoch,
                o.ui_serial,
                scene.resource_revision,
                frame_started.elapsed().as_secs_f64() * 1000.,
                surface_damage.x,
                surface_damage.y,
                surface_damage.w,
                surface_damage.h
            );
            println!(
                "WINDOW_COMMIT monotonic_ns={} epoch={} update={} generation={} kind=overlay",
                super::monotonic_ns(),
                o.epoch,
                o.ui_serial,
                scene.resource_revision
            );
        }
        Ok(animating)
    }
    /// `bead overlay`: residency and placement of the overlay slot (introspection).
    pub(super) fn overlay_state(&self) -> serde_json::Value {
        let Some(o) = &self.overlay else {
            return serde_json::json!({"resident": false});
        };
        serde_json::json!({
            "resident": true, "view": o.name, "open": o.open, "configured": o.configured,
            "output": self.output_id(&o.output), "size": [o.size.0, o.size.1],
        })
    }
    /// `bead overlay-regions` (private input proof only): the shown view's hit regions.
    pub(super) fn overlay_regions(&self) -> serde_json::Value {
        let Some(o) = &self.overlay else {
            return serde_json::json!({"regions": []});
        };
        serde_json::json!({
            "regions": o.source.input_regions().into_iter().map(|r| serde_json::json!({
                "id": r.id, "rect": [r.rect.x, r.rect.y, r.rect.w, r.rect.h], "cursor": r.cursor,
            })).collect::<Vec<_>>(),
            "offset": [o.offset.0, o.offset.1], "size": [o.size.0, o.size.1],
        })
    }
    pub(super) fn overlay_pointer(&mut self, event: &PointerEvent) {
        let Some(o) = self.overlay.as_mut() else {
            return;
        };
        let x = event.position.0 as f32 - o.offset.0;
        let y = event.position.1 as f32 - o.offset.1;
        if let PointerEventKind::Enter { serial, .. } = event.kind {
            o.cursor_serial = serial;
        }
        if let Some(device) = &self.cursor_device
            && o.cursor_serial != 0
        {
            let shape = match o.source.pointer_cursor(x, y) {
                "text" => wp_cursor_shape_device_v1::Shape::Text,
                "pointer" => wp_cursor_shape_device_v1::Shape::Pointer,
                _ => wp_cursor_shape_device_v1::Shape::Default,
            };
            device.set_shape(o.cursor_serial, shape);
        }
        let pressed = o.pressed;
        let ev = match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => Some(if pressed {
                UiEvent::Drag { x, y }
            } else {
                UiEvent::Hover { x, y }
            }),
            PointerEventKind::Leave { .. } => {
                o.pressed = false;
                Some(UiEvent::Hover { x: -1., y: -1. })
            }
            PointerEventKind::Press { button: 0x110, .. } => {
                o.pressed = true;
                Some(UiEvent::Click { x, y })
            }
            PointerEventKind::Release { button: 0x110, .. } => {
                o.pressed = false;
                Some(UiEvent::Release)
            }
            PointerEventKind::Press { button: 0x111, .. } => Some(UiEvent::ContextMenu { x, y }),
            PointerEventKind::Press { button: 0x112, .. } => Some(UiEvent::SecondaryClick { x, y }),
            PointerEventKind::Axis {
                vertical,
                horizontal,
                source,
                ..
            } => {
                let target = crate::scroll::Target::Overlay(o.epoch);
                self.axis_scroll(target.clone(), false, (x, y), vertical, source);
                self.axis_scroll(target, true, (x, y), horizontal, source);
                None
            }
            _ => None,
        };
        if let Some(ev) = ev {
            self.overlay_ui(ev);
        }
    }
}

/// Named overlay views: the same catalog shape as [`Panels`], without a default view.
#[derive(Clone)]
pub struct Overlays {
    views: std::collections::BTreeMap<String, PanelSpec>,
    fullscreen_covered: std::collections::BTreeSet<String>,
}
impl Overlays {
    pub fn new(views: std::collections::BTreeMap<String, PanelSpec>) -> Result<Self, String> {
        if views.is_empty() || views.len() > 8 {
            return Err("overlay catalog needs one to eight views".into());
        }
        if views.keys().any(|name| name.is_empty() || name.len() > 64) {
            return Err("invalid overlay view name".into());
        }
        Ok(Self {
            views,
            fullscreen_covered: Default::default(),
        })
    }
    /// Put selected views with desktop chrome (Top), below fullscreen clients.
    /// Other views retain Overlay, e.g. a level indicator explicitly shown over video.
    pub fn with_fullscreen_covered(mut self, views: &[String]) -> Result<Self, String> {
        for view in views {
            if !self.contains(view) {
                return Err(format!("unknown fullscreen-covered overlay {view}"));
            }
            self.fullscreen_covered.insert(view.clone());
        }
        Ok(self)
    }
    pub(super) fn layer(&self, view: &str) -> Layer {
        if self.fullscreen_covered.contains(view) {
            Layer::Top
        } else {
            Layer::Overlay
        }
    }
    pub fn contains(&self, view: &str) -> bool {
        self.views.contains_key(view)
    }
    fn resolve(&self, view: Option<&str>) -> Result<(&str, &PanelSpec), String> {
        view.and_then(|v| self.views.get_key_value(v))
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| "unknown overlay view".into())
    }
}

#[cfg(test)]
mod tests {
    use bead_core::animation::{Animations, Curve};

    /// One display refresh at 120 Hz, the owner's laptop.
    const VBLANK: f64 = 1. / 120.;

    /// Plays a show or hide on the overlay's draw cadence (one pose per refresh while the motion
    /// is active, no re-staging). Returns the commit times and the PANEL_SLIDE marker.
    fn play(
        open: bool,
        duration: f64,
        curve: Curve,
    ) -> (Vec<f64>, Option<bead_core::animation::Slide>) {
        let mut animations = Animations::default();
        if !open {
            animations.open_to("osd", true, 0., 0.15, Curve::EaseOut);
            let _ = animations.committed("osd", 0., 0.);
        }
        let start = if open { 0. } else { 1. };
        animations.open_to("osd", open, start, duration, curve);
        let (mut commits, mut marker, mut now) = (Vec::new(), None, start);
        loop {
            let (_, active) = animations.value("osd", now).expect("motion value");
            commits.push(now);
            marker = animations.committed("osd", now, now).or(marker);
            if !active || commits.len() > 200 {
                return (commits, marker);
            }
            now += VBLANK;
        }
    }

    #[test]
    fn osd_and_toast_motion_commit_every_refresh_at_120_hz() {
        let motion = bead_render::Motion {
            id: "osd".into(),
            open: true,
            duration: 0.150,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Bottom,
            slide_distance: Some(4.),
            fade: true,
            reveal: false,
            progress: 0.,
            parts: Vec::new(),
            art: None,
        };
        let (open_duration, open_curve) = motion.transition(true);
        let (close_duration, close_curve) = motion.transition(false);
        assert!(close_duration < open_duration, "exits are shorter (B0181)");
        assert_eq!(close_curve, Curve::EmphasizedAccelerate, "exits accelerate");
        let (shown, shown_marker) = play(true, open_duration, open_curve);
        let (hidden, hidden_marker) = play(false, close_duration, close_curve);
        for (name, commits, floor) in [("show", &shown, 17), ("hide", &hidden, 11)] {
            assert!(
                commits.len() >= floor,
                "{name} committed {} frames",
                commits.len()
            );
            let gap = commits
                .windows(2)
                .map(|w| w[1] - w[0])
                .fold(0_f64, f64::max);
            assert!(gap <= 0.009, "{name} maximum frame gap {gap:.6}s");
        }
        for marker in [shown_marker, hidden_marker] {
            assert!(
                marker.is_some(),
                "a finished motion logs its PANEL_SLIDE marker"
            );
        }
    }
}

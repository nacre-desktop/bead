// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lazy output canvases. Each mapped surface is only the visible content bounds.
use super::*;

fn canvas_event_belongs_to_host(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::CompanionVisibility(_)
            | UiEvent::CompanionSelectAt { .. }
            | UiEvent::CompanionSelect { .. }
            | UiEvent::CompanionAction { .. }
            | UiEvent::CompanionMotionAction { .. }
            | UiEvent::HostModel(_)
            | UiEvent::CanvasPrepare { .. }
            | UiEvent::CanvasOffer { .. }
            | UiEvent::CanvasModel { .. }
    )
}

fn canvas_wakeup_delay(
    visible: bool,
    timer_pending: bool,
    deadline: Option<Instant>,
    now: Instant,
) -> Option<Duration> {
    if !visible || timer_pending {
        return None;
    }
    deadline.map(|deadline| {
        deadline
            .saturating_duration_since(now)
            .max(Duration::from_millis(1))
    })
}

pub(super) struct Canvas {
    pub(super) epoch: u64,
    lifecycle: CanvasLifecycle,
    retain_when_closed: bool,
    initial_bounds: Option<Rect>,
    reserved_size: Option<(f32, f32)>,
    buffer_attached: bool,
    pub(super) layer: Option<LayerSurface>,
    pub(super) output: wl_output::WlOutput,
    pub(super) source: Box<dyn SceneSource>,
    pub(super) scene: Scene,
    pub(super) source_offset: (f32, f32),
    pub(super) renderer: Box<dyn Renderer>,
    pub(super) pool: Option<SlotPool>,
    pub(super) buffers: Vec<Buffer>,
    /// Damage accumulated independently for each reusable slot.
    pub(super) damage_history: Vec<Damage>,
    pub(super) geometry: Option<(i32, i32)>,
    /// Full logical output viewport passed to the source for placement.
    pub(super) viewport_size: (u32, u32),
    /// Global output-space bounds of the visible primitives and mapped surface bounds.
    pub(super) bounds: Option<Rect>,
    pub(super) mapped_bounds: Option<Rect>,
    pub(super) render_size: (u32, u32),
    pub(super) configured: bool,
    pub(super) scale120: u32,
    pub(super) fractional: Option<WpFractionalScaleV1>,
    pub(super) viewport: Option<WpViewport>,
    pub(super) interactive: bool,
    pub(super) input: Option<Vec<[i32; 4]>>,
    pub(super) pressed: bool,
    pub(super) callbacks: policy::Callbacks,
    pub(super) frame_pending: bool,
    tick_timer: bool,
    pub(super) dirty: bool,
    pub(super) refresh_ns: u32,
}

impl Drop for Canvas {
    fn drop(&mut self) {
        if let Some(scale) = self.fractional.take() {
            scale.destroy();
        }
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
    }
}

fn visible_bounds(scene: &Scene, output: Option<(u32, u32)>) -> Option<Rect> {
    if scene.opacity <= 0.001 {
        return None;
    }
    let mut bounds: Option<Rect> = None;
    for primitive in &scene.primitives {
        let rect = match primitive {
            bead_render::Primitive::Arc { rect, color, .. } if color[3] > 0 => Some(*rect),
            bead_render::Primitive::Polyline {
                rect, color, fill, ..
            } if color[3] > 0 || fill.is_some_and(|fill| fill[3] > 0) => Some(*rect),
            bead_render::Primitive::Rounded {
                rect, fill, border, ..
            } if fill[3] > 0 || border.is_some_and(|(color, width)| color[3] > 0 && width > 0.) => {
                Some(*rect)
            }
            bead_render::Primitive::Text {
                rect, color, text, ..
            } if color[3] > 0 && !text.is_empty() => Some(*rect),
            bead_render::Primitive::Image { rect, .. }
            | bead_render::Primitive::Shadow { rect, .. }
                if rect.w > 0. && rect.h > 0. =>
            {
                Some(*rect)
            }
            _ => None,
        };
        let Some(rect) = rect else {
            continue;
        };
        let painted = rect.translated(scene.offset.0, scene.offset.1);
        let clipped = output.map_or(painted, |(width, height)| {
            painted.intersect(Rect::new(0., 0., width as f32, height as f32))
        });
        if clipped.w <= 0. || clipped.h <= 0. {
            continue;
        }
        bounds = Some(bounds.map_or(clipped, |current| current.union(clipped)));
    }
    let bounds = bounds?;
    let left = bounds.x.floor();
    let top = bounds.y.floor();
    // A constant outward pixel keeps fractional translation from toggling the
    // layer size and forcing a configure round trip on each movement.
    let right = left + bounds.w.ceil() + 1.;
    let bottom = top + bounds.h.ceil() + 1.;
    let right = output.map_or(right, |(width, _)| right.min(width as f32));
    let bottom = output.map_or(bottom, |(_, height)| bottom.min(height as f32));
    Some(Rect::new(left, top, right - left, bottom - top))
}

fn reserve_canvas_size(source: Option<Rect>, prepared: Option<Rect>) -> Option<(f32, f32)> {
    match (source, prepared) {
        (Some(source), Some(prepared)) => {
            Some((source.w.max(prepared.w), source.h.max(prepared.h)))
        }
        (Some(source), None) => Some((source.w, source.h)),
        (None, Some(prepared)) => Some((prepared.w, prepared.h)),
        (None, None) => None,
    }
}

fn canvas_bounds_with_reserve(bounds: Rect, reserved_size: Option<(f32, f32)>) -> Rect {
    reserved_size.map_or(bounds, |(width, height)| {
        Rect::new(
            bounds.x,
            bounds.y,
            width.max(bounds.w),
            height.max(bounds.h),
        )
    })
}

fn crop_scene(mut scene: Scene, bounds: Rect) -> Scene {
    scene.bounds = Rect::new(
        bounds.x - scene.offset.0,
        bounds.y - scene.offset.1,
        bounds.w,
        bounds.h,
    );
    scene.offset.0 -= bounds.x;
    scene.offset.1 -= bounds.y;
    scene.prepared.clear();
    scene
}

fn canvas_surface_bounds(
    open: bool,
    scene_bounds: Rect,
    initial_bounds: Option<Rect>,
    reserved_size: Option<(f32, f32)>,
) -> Rect {
    if open {
        canvas_bounds_with_reserve(scene_bounds, reserved_size)
    } else {
        initial_bounds
            .map(|bounds| canvas_bounds_with_reserve(bounds, reserved_size))
            .unwrap_or(scene_bounds)
    }
}

struct CanvasLifecycle {
    open: bool,
    pending_open: bool,
    unmapped: bool,
    warmup_packets_pending: usize,
    retention_generation: u64,
}
impl CanvasLifecycle {
    fn new(open: bool, warmup_packets_pending: usize) -> Self {
        Self {
            open,
            pending_open: false,
            unmapped: false,
            warmup_packets_pending,
            retention_generation: 0,
        }
    }
    fn set_open(&mut self, open: bool) -> u64 {
        self.open = open;
        self.retention_generation = self.retention_generation.wrapping_add(1);
        self.retention_generation
    }
    fn offer_open(&mut self) {
        self.pending_open = !self.open;
        if self.pending_open {
            self.retention_generation = self.retention_generation.wrapping_add(1);
        }
    }
    fn close(&mut self) {
        self.set_open(false);
        self.pending_open = false;
    }
    fn unmap(&mut self) -> bool {
        if self.unmapped {
            return false;
        }
        self.unmapped = true;
        true
    }
    fn unmap_if_buffered(&mut self, buffer_attached: bool) -> bool {
        buffer_attached && self.unmap()
    }
    fn begin_remap(&mut self) -> bool {
        if !self.unmapped {
            return false;
        }
        self.unmapped = false;
        true
    }
    fn warmup_response(&mut self) -> bool {
        if self.warmup_packets_pending == 0 {
            return false;
        }
        self.warmup_packets_pending -= 1;
        true
    }
    fn accept_scene(&mut self, warmup_response: bool) -> bool {
        if self.pending_open && !warmup_response {
            self.set_open(true);
            self.pending_open = false;
        }
        self.open
    }
    fn expires(&self, generation: u64) -> bool {
        !self.open && self.retention_generation == generation
    }
}

impl App {
    pub(super) fn canvas_presentation(
        &mut self,
        data: &PresentationData,
        event: &wp_presentation_feedback::Event,
    ) -> bool {
        let Some(canvas) = self.canvases.values_mut().find(|canvas| {
            canvas.epoch == data.source_epoch
                && canvas
                    .layer
                    .as_ref()
                    .is_some_and(|layer| layer.wl_surface() == &data.surface)
        }) else {
            return false;
        };
        if let wp_presentation_feedback::Event::Presented { refresh, .. } = event
            && *refresh > 0
        {
            canvas.refresh_ns = *refresh;
            canvas
                .source
                .frame_interval(Duration::from_nanos(u64::from(*refresh)));
        }
        true
    }
    pub(super) fn canvas_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.canvases.values().any(|canvas| {
            canvas
                .layer
                .as_ref()
                .is_some_and(|layer| layer.wl_surface() == surface)
        })
    }
    pub(super) fn canvas_has_fractional(&self, surface: &wl_surface::WlSurface) -> bool {
        self.canvases
            .values()
            .find(|canvas| {
                canvas
                    .layer
                    .as_ref()
                    .is_some_and(|layer| layer.wl_surface() == surface)
            })
            .is_some_and(|canvas| canvas.fractional.is_some())
    }
    pub(super) fn canvas_layer_is(&self, layer: &LayerSurface) -> bool {
        self.canvases
            .values()
            .any(|canvas| canvas.layer.as_ref() == Some(layer))
    }
    pub(super) fn canvas_offer(
        &mut self,
        view: &str,
        output_name: Option<&str>,
        open: bool,
        values: std::collections::BTreeMap<String, bead_core::Value>,
    ) {
        if !open {
            let stale = self
                .canvases
                .keys()
                .filter(|(output, name)| {
                    name == view && output_name.is_none_or(|wanted| wanted == output)
                })
                .cloned()
                .collect::<Vec<_>>();
            for key in stale {
                if self
                    .canvases
                    .get(&key)
                    .is_some_and(|canvas| canvas.retain_when_closed)
                {
                    self.canvas_hide(&key);
                    self.canvas_schedule_release(&key);
                } else {
                    self.release_canvas(&key);
                }
            }
            return;
        }
        self.canvas_start(view, output_name, values, true, None);
    }
    pub(super) fn canvas_prepare(
        &mut self,
        view: &str,
        output_name: Option<&str>,
        values: std::collections::BTreeMap<String, bead_core::Value>,
        initial_bounds: Option<Rect>,
    ) {
        self.canvas_start(view, output_name, values, false, initial_bounds);
    }
    fn canvas_start(
        &mut self,
        view: &str,
        output_name: Option<&str>,
        values: std::collections::BTreeMap<String, bead_core::Value>,
        open: bool,
        initial_bounds: Option<Rect>,
    ) {
        if self.hidden {
            return;
        }
        let output = output_name
            .and_then(|wanted| {
                self.output.outputs().find(|output| {
                    self.output
                        .info(output)
                        .and_then(|info| info.name)
                        .as_deref()
                        == Some(wanted)
                })
            })
            .or_else(|| {
                self.active_output.as_deref().and_then(|wanted| {
                    self.output.outputs().find(|output| {
                        self.output
                            .info(output)
                            .and_then(|info| info.name)
                            .as_deref()
                            == Some(wanted)
                    })
                })
            })
            .or_else(|| self.output.outputs().next());
        let Some(output) = output else {
            return;
        };
        let name = self
            .output
            .info(&output)
            .and_then(|info| info.name)
            .unwrap_or_else(|| "default".into());
        let key = (name.clone(), view.to_owned());
        if self.canvases.contains_key(&key) {
            let pending_open = open
                && self
                    .canvases
                    .get(&key)
                    .is_some_and(|canvas| !canvas.lifecycle.open);
            if let Some(canvas) = self.canvases.get_mut(&key) {
                if !open {
                    canvas.initial_bounds = initial_bounds;
                    canvas.reserved_size = None;
                }
                if open {
                    canvas.lifecycle.offer_open();
                }
            }
            if open {
                self.canvas_ui(&key, UiEvent::Model(values));
                if pending_open {
                    self.canvas_schedule_release(&key);
                }
            } else {
                self.canvas_hide(&key);
                self.canvas_ui(&key, UiEvent::Model(values));
                self.canvas_schedule_release(&key);
            }
            return;
        }
        let Some(catalog) = self.options.canvases.as_ref() else {
            return;
        };
        let spec = match catalog.resolve(Some(view)) {
            Ok((_, spec)) => spec.clone(),
            Err(error) => {
                eprintln!("CANVAS_ERROR {error}");
                return;
            }
        };
        if !open && !spec.retain_when_closed {
            return;
        }
        let Some((width, height)) = self
            .output
            .info(&output)
            .and_then(|info| info.logical_size)
            .filter(|(width, height)| *width > 0 && *height > 0)
        else {
            return;
        };
        self.canvas_epoch = self.canvas_epoch.wrapping_add(1);
        let epoch = self.canvas_epoch;
        let factory = spec.factory.clone();
        let fallback = spec.fallback.clone();
        let mut source: Box<dyn SceneSource> = Box::new(worker::Source::new(
            Box::new(move || factory(true)),
            fallback.clone(),
        ));
        source.frame_interval(self.output_frame_interval(&output));
        let Some(sender) = self.event_sender.clone() else {
            return;
        };
        let wake_sender = sender.clone();
        source.services(Box::new(move |event| {
            let _ = wake_sender.send(UiEvent::Canvas(epoch, Box::new(event)));
        }));
        let watch_sender = sender;
        if let Err(error) = source.watch(Box::new(move |result| {
            let event = match result {
                Ok(()) => UiEvent::Reload,
                Err(error) => UiEvent::WatchError(error),
            };
            let _ = watch_sender.send(UiEvent::Canvas(epoch, Box::new(event)));
        })) {
            eprintln!("CANVAS_ERROR {error}");
            return;
        }
        if let Err(error) = source.initial_model(values) {
            eprintln!("CANVAS_ERROR {error}");
            return;
        }
        source.viewport(width as u32, height as u32);
        let renderer = match bead_render::renderer(spec.renderer) {
            Ok(renderer) => renderer,
            Err(error) => {
                eprintln!("CANVAS_ERROR {error}");
                return;
            }
        };
        let scale120 = self
            .surfaces
            .iter()
            .find(|surface| surface.output == output && !surface.companion && surface.configured)
            .map(|surface| surface.scale120)
            .or_else(|| {
                self.output
                    .info(&output)
                    .map(|info| info.scale_factor.max(1) as u32 * 120)
            })
            .unwrap_or(120);
        self.canvases.insert(
            key.clone(),
            Canvas {
                epoch,
                lifecycle: CanvasLifecycle::new(open, 0),
                retain_when_closed: spec.retain_when_closed,
                initial_bounds,
                reserved_size: None,
                buffer_attached: false,
                layer: None,
                output,
                source,
                source_offset: fallback.offset,
                scene: fallback,
                renderer,
                pool: None,
                buffers: Vec::new(),
                damage_history: Vec::new(),
                geometry: None,
                viewport_size: (width as u32, height as u32),
                bounds: None,
                mapped_bounds: None,
                render_size: (0, 0),
                configured: false,
                scale120,
                fractional: None,
                viewport: None,
                interactive: !spec.passive,
                input: None,
                pressed: false,
                callbacks: Default::default(),
                frame_pending: false,
                tick_timer: false,
                dirty: false,
                refresh_ns: 0,
            },
        );
        if let Some(palette) = self.wallpaper_palette.clone() {
            self.canvas_ui(&key, UiEvent::WallpaperPalette(palette));
        }
        self.canvas_ui(&key, UiEvent::OutputName(key.0.clone()));
        self.canvas_ui(&key, UiEvent::Reload);
        if !open {
            self.canvas_schedule_release(&key);
        }
    }
    fn canvas_hide(&mut self, key: &(String, String)) {
        if let Some(canvas) = self.canvases.get_mut(key) {
            canvas.lifecycle.close();
            canvas.callbacks.invalidate();
            canvas.frame_pending = false;
            canvas.dirty = false;
            canvas.pressed = false;
            let mut commit = false;
            if canvas
                .input
                .as_ref()
                .is_none_or(|regions| !regions.is_empty())
                && let Some(layer) = &canvas.layer
            {
                match Region::new(&self.compositor) {
                    Ok(empty) => {
                        layer.wl_surface().set_input_region(Some(empty.wl_region()));
                        canvas.input = Some(Vec::new());
                        commit = true;
                    }
                    Err(error) => eprintln!("CANVAS_ERROR {error}"),
                }
            }
            if canvas.lifecycle.unmap_if_buffered(canvas.buffer_attached) {
                if let Some(layer) = &canvas.layer {
                    layer.wl_surface().attach(None, 0, 0);
                    commit = true;
                }
                canvas.buffer_attached = false;
                canvas.configured = false;
                canvas.mapped_bounds = None;
            }
            if commit && let Some(layer) = &canvas.layer {
                layer.commit();
            }
        }
    }
    fn canvas_schedule_release(&mut self, key: &(String, String)) {
        let Some(canvas) = self.canvases.get(key) else {
            return;
        };
        let epoch = canvas.epoch;
        let generation = canvas.lifecycle.retention_generation;
        let key = key.clone();
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_secs(2)),
            move |_, _, app| {
                let retained = app.canvases.get(&key).is_some_and(|canvas| {
                    canvas.epoch == epoch && canvas.lifecycle.expires(generation)
                });
                if retained {
                    app.release_canvas(&key);
                }
                TimeoutAction::Drop
            },
        ) {
            eprintln!("CANVAS_ERROR {error}");
        }
    }
    pub(super) fn canvas_ui(&mut self, key: &(String, String), event: UiEvent) {
        if let UiEvent::ResourcesPrepared(result) = event {
            match result {
                Ok(()) => {
                    if let Some(canvas) = self.canvases.get_mut(key) {
                        canvas.dirty = true;
                    }
                    if let Err(error) = self.canvas_draw(key) {
                        eprintln!("CANVAS_ERROR {error}");
                    }
                }
                Err(error) => eprintln!("CANVAS_ERROR {error}"),
            }
            return;
        }
        let Some(canvas) = self.canvases.get_mut(key) else {
            return;
        };
        if !canvas.lifecycle.open
            && !canvas.lifecycle.pending_open
            && !matches!(&event, UiEvent::Worker(_))
        {
            canvas.lifecycle.warmup_packets_pending += 1;
        }
        let warmup_response = matches!(
            &event,
            UiEvent::Worker(packet)
                if (packet.is_accepted() || packet.is_settled() || packet.is_failed())
                    && canvas.lifecycle.warmup_packets_pending > 0
        );
        if warmup_response {
            canvas.lifecycle.warmup_response();
        }
        canvas.source.scales(vec![canvas.scale120 as f32 / 120.]);
        let scale = canvas.scale120 as f32 / 120.;
        let staged = canvas.source.stage(event).and_then(|scene| {
            if let Some(scene) = &scene {
                canvas.renderer.prepare(scene, scale)?;
            }
            canvas.source.validate()?;
            Ok(scene)
        });
        match staged {
            Ok(Some(scene)) => {
                if !canvas.source.commit() {
                    return;
                }
                canvas.lifecycle.accept_scene(warmup_response);
                let hidden_prepare = !canvas.lifecycle.open && canvas.initial_bounds.is_some();
                let mut bounds =
                    visible_bounds(&scene, (!hidden_prepare).then_some(canvas.viewport_size));
                if canvas.interactive {
                    for region in canvas
                        .source
                        .input_regions()
                        .into_iter()
                        .filter(|region| region.include_in_bounds)
                    {
                        let rect = region
                            .clip
                            .map_or(region.rect, |clip| region.rect.intersect(clip))
                            .translated(scene.offset.0, scene.offset.1)
                            .intersect(Rect::new(
                                0.,
                                0.,
                                canvas.viewport_size.0 as f32,
                                canvas.viewport_size.1 as f32,
                            ));
                        if rect.w > 0. && rect.h > 0. {
                            bounds = Some(bounds.map_or(rect, |bounds| bounds.union(rect)));
                        }
                    }
                }
                let bounds = bounds.map(|rect| {
                    let x = rect.x.floor();
                    let y = rect.y.floor();
                    Rect::new(
                        x,
                        y,
                        (rect.x + rect.w).ceil() - x,
                        (rect.y + rect.h).ceil() - y,
                    )
                });
                let Some(mut bounds) = bounds else {
                    if let Some(fractional) = canvas.fractional.take() {
                        fractional.destroy();
                    }
                    if let Some(viewport) = canvas.viewport.take() {
                        viewport.destroy();
                    }
                    canvas.layer.take();
                    canvas.pool = None;
                    canvas.buffers.clear();
                    canvas.damage_history.clear();
                    canvas.geometry = None;
                    canvas.mapped_bounds = None;
                    canvas.bounds = None;
                    canvas.render_size = (0, 0);
                    canvas.configured = false;
                    canvas.input = None;
                    canvas.callbacks.invalidate();
                    canvas.frame_pending = false;
                    canvas.dirty = false;
                    canvas.pressed = false;
                    return;
                };
                if hidden_prepare {
                    canvas.reserved_size = reserve_canvas_size(canvas.initial_bounds, Some(bounds));
                }
                bounds = canvas_bounds_with_reserve(bounds, canvas.reserved_size);
                let surface_bounds = canvas_surface_bounds(
                    canvas.lifecycle.open,
                    bounds,
                    canvas.initial_bounds,
                    canvas.reserved_size,
                );
                canvas.source_offset = scene.offset;
                canvas.scene = crop_scene(scene, bounds);
                canvas.bounds = Some(bounds);
                canvas.render_size = (bounds.w as u32, bounds.h as u32);
                canvas.dirty = true;
                if !canvas.lifecycle.open {
                    if let Err(error) = canvas.renderer.prepare(&canvas.scene, scale) {
                        eprintln!("CANVAS_ERROR {error}");
                        return;
                    }
                    if canvas.initial_bounds.is_some()
                        && let Err(error) = self.canvas_map_bounds(key, surface_bounds)
                    {
                        eprintln!("CANVAS_ERROR {error}");
                    }
                    return;
                }
                canvas.initial_bounds = None;
                if let Err(error) = self.canvas_map_bounds(key, surface_bounds) {
                    eprintln!("CANVAS_ERROR {error}");
                    return;
                }
                let _ = self.canvas_draw(key);
            }
            Ok(None) => (),
            Err(error) => eprintln!("CANVAS_ERROR {error}"),
        }
        self.canvas_schedule_wakeup(key);
    }

    fn canvas_schedule_wakeup(&mut self, key: &(String, String)) {
        let now = Instant::now();
        let Some((delay, epoch)) = self.canvases.get(key).and_then(|canvas| {
            let delay = canvas_wakeup_delay(
                canvas.lifecycle.open && canvas.bounds.is_some(),
                canvas.tick_timer,
                canvas.source.wakeup_at(),
                now,
            )?;
            Some((delay, canvas.epoch))
        }) else {
            return;
        };
        if let Some(canvas) = self.canvases.get_mut(key) {
            canvas.tick_timer = true;
        }
        let timer_key = key.clone();
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(delay), move |_, _, app| {
                    let open = app.canvases.get_mut(&timer_key).is_some_and(|canvas| {
                        if canvas.epoch != epoch {
                            return false;
                        }
                        canvas.tick_timer = false;
                        canvas.lifecycle.open && canvas.bounds.is_some()
                    });
                    if open {
                        app.canvas_ui(&timer_key, UiEvent::Model(Default::default()));
                    }
                    TimeoutAction::Drop
                })
        {
            if let Some(canvas) = self.canvases.get_mut(key) {
                canvas.tick_timer = false;
            }
            eprintln!("CANVAS_ERROR {error}");
        }
    }
    pub(super) fn canvas_event(&mut self, epoch: u64, mut event: UiEvent) {
        let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| canvas.epoch == epoch)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        if matches!(&event, UiEvent::CompanionSelectAt { .. })
            && let Some(previous) = self.panel_canvas_owner.replace(epoch)
            && previous != epoch
            && let Some(sender) = &self.event_sender
        {
            let _ = sender.send(UiEvent::Canvas(previous, Box::new(UiEvent::PanelDismissed)));
        }
        match &mut event {
            UiEvent::CompanionSelect { output, .. } | UiEvent::CompanionSelectAt { output, .. }
                if output.is_none() =>
            {
                *output = Some(key.0.clone())
            }
            _ => {}
        }
        if canvas_event_belongs_to_host(&event) {
            self.ui(event);
        } else {
            self.canvas_ui(&key, event);
        }
    }
    fn canvas_map_bounds(
        &mut self,
        key: &(String, String),
        bounds: Rect,
    ) -> Result<(), Box<dyn Error>> {
        let (output, bounds, previous, viewport_size, existing) = {
            let canvas = self.canvases.get(key).ok_or("canvas missing")?;
            (
                canvas.output.clone(),
                bounds,
                canvas.mapped_bounds,
                canvas.viewport_size,
                canvas.layer.is_some(),
            )
        };
        if existing && previous == Some(bounds) {
            return Ok(());
        }
        let size = (
            bounds.w.ceil().max(1.) as u32,
            bounds.h.ceil().max(1.) as u32,
        );
        let layer = if let Some(layer) = self
            .canvases
            .get(key)
            .and_then(|canvas| canvas.layer.clone())
        {
            layer
        } else {
            let qh = self.qh.clone();
            let surface = self.compositor.create_surface(&qh);
            let empty = Region::new(&self.compositor)?;
            surface.set_input_region(Some(empty.wl_region()));
            let fractional = self
                .viewporter
                .is_some()
                .then(|| {
                    self.fractional
                        .as_ref()
                        .map(|manager| manager.get_fractional_scale(&surface, &qh, surface.clone()))
                })
                .flatten();
            let viewport = fractional.as_ref().and_then(|_| {
                self.viewporter
                    .as_ref()
                    .map(|manager| manager.get_viewport(&surface, &qh, ()))
            });
            let layer = self.layers.create_layer_surface(
                &qh,
                surface,
                match self.options.canvas_layers.get(&key.1) {
                    Some(CanvasLayer::Overlay) => Layer::Overlay,
                    _ => Layer::Bottom,
                },
                Some(&self.options.namespace),
                Some(&output),
            );
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            if let Some(canvas) = self.canvases.get_mut(key) {
                canvas.fractional = fractional;
                canvas.viewport = viewport;
                canvas.input = Some(Vec::new());
            }
            layer
        };
        layer.set_size(size.0, size.1);
        layer.set_exclusive_zone(-1);
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(bounds.y.round() as i32, 0, 0, bounds.x.round() as i32);
        if let Some(canvas) = self.canvases.get_mut(key) {
            canvas.layer = Some(layer.clone());
            if canvas.lifecycle.begin_remap() {
                canvas.configured = false;
            }
            if let Some(viewport) = &canvas.viewport {
                viewport.set_destination(size.0 as i32, size.1 as i32);
            }
            let size_changed =
                previous.is_none_or(|previous| previous.w != bounds.w || previous.h != bounds.h);
            canvas.mapped_bounds = Some(bounds);
            if size_changed {
                canvas.pool = None;
                canvas.buffers.clear();
                canvas.geometry = None;
                canvas.damage_history = vec![
                    Damage {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0
                    };
                    2
                ];
                canvas.configured = false;
            }
            canvas.viewport_size = viewport_size;
        }
        layer.commit();
        Ok(())
    }
    fn canvas_draw(&mut self, key: &(String, String)) -> Result<(), Box<dyn Error>> {
        let rgba = cfg!(target_endian = "little")
            && !self.options.force_argb
            && self.shm.formats().contains(&wl_shm::Format::Abgr8888);
        let format = if rgba {
            PixelFormat::Rgba
        } else {
            PixelFormat::Bgra
        };
        let canvas = self.canvases.get_mut(key).ok_or("canvas closed")?;
        if !canvas.lifecycle.open || !canvas.configured || canvas.bounds.is_none() {
            return Ok(());
        }
        let layer = canvas.layer.as_ref().ok_or("canvas layer missing")?;
        let surface = layer.wl_surface().clone();
        let scale = canvas.scale120 as f32 / 120.;
        let (width, height, stride, size) =
            buffer_dimensions(canvas.render_size.0, canvas.render_size.1, canvas.scale120)?;
        let bounds = canvas.bounds.ok_or("canvas bounds missing")?;
        let regions = if canvas.interactive {
            canvas
                .source
                .input_regions()
                .into_iter()
                .filter_map(|region| {
                    let rect = region
                        .clip
                        .map_or(region.rect, |clip| region.rect.intersect(clip))
                        .translated(canvas.source_offset.0, canvas.source_offset.1);
                    let x = (rect.x - bounds.x).floor().max(0.) as i32;
                    let y = (rect.y - bounds.y).floor().max(0.) as i32;
                    let right = (rect.x + rect.w - bounds.x).ceil().min(bounds.w) as i32;
                    let bottom = (rect.y + rect.h - bounds.y).ceil().min(bounds.h) as i32;
                    (right > x && bottom > y).then_some([x, y, right - x, bottom - y])
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if canvas.input.as_ref() != Some(&regions) {
            let region = Region::new(&self.compositor)?;
            for [x, y, w, h] in &regions {
                region.add(*x, *y, *w, *h);
            }
            surface.set_input_region(Some(region.wl_region()));
            canvas.input = Some(regions);
        }
        let geometry = (width, height);
        if canvas.geometry != Some(geometry) || canvas.pool.is_none() || canvas.buffers.is_empty() {
            canvas.buffers.clear();
            let mut pool = SlotPool::new(size.next_multiple_of(64) * 2, &self.shm)?;
            for _ in 0..2 {
                let (buffer, _) = pool.create_buffer(
                    width,
                    height,
                    stride,
                    if rgba {
                        wl_shm::Format::Abgr8888
                    } else {
                        wl_shm::Format::Argb8888
                    },
                )?;
                canvas.buffers.push(buffer);
            }
            canvas.pool = Some(pool);
            canvas.geometry = Some(geometry);
            canvas.damage_history = vec![
                Damage {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0
                };
                2
            ];
        }
        let full = Damage::full(width as u32, height as u32);
        for history in &mut canvas.damage_history {
            *history = history.union(full);
        }
        let pool = canvas.pool.as_mut().ok_or("canvas pool missing")?;
        let Some(slot) = canvas
            .buffers
            .iter()
            .position(|buffer| !buffer.slot().has_active_buffers())
        else {
            canvas.dirty = true;
            if let Some(epoch) = canvas.callbacks.request_if_idle() {
                surface.frame(
                    &self.qh,
                    FrameData {
                        surface: surface.clone(),
                        epoch,
                        generation: canvas.scene.resource_revision,
                        source_epoch: canvas.epoch,
                    },
                );
                canvas.frame_pending = true;
            }
            canvas
                .layer
                .as_ref()
                .ok_or("canvas layer missing")?
                .commit();
            return Ok(());
        };
        let mut damage = std::mem::replace(
            &mut canvas.damage_history[slot],
            Damage {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
        );
        if damage.w == 0 || damage.h == 0 {
            damage = full;
        }
        canvas.renderer.prepare(&canvas.scene, scale)?;
        let bytes = canvas.buffers[slot]
            .canvas(pool)
            .ok_or("compositor owns canvas buffer")?;
        if let Err(error) = canvas.renderer.draw(
            &canvas.scene,
            RenderTarget::Shm(ShmTarget {
                bytes: &mut *bytes,
                width: width as u32,
                height: height as u32,
                scale,
                format,
            }),
            damage,
        ) {
            if super::rendering::prepare_pending_images(error.as_ref(), canvas.source.as_mut())? {
                canvas.dirty = true;
                return Ok(());
            }
            return Err(error);
        }
        surface.set_buffer_scale(if canvas.viewport.is_some() {
            1
        } else {
            (canvas.scale120 / 120).max(1) as i32
        });
        surface.damage_buffer(
            damage.x as i32,
            damage.y as i32,
            damage.w as i32,
            damage.h as i32,
        );
        if let Some(epoch) = canvas.callbacks.request_if_idle() {
            surface.frame(
                &self.qh,
                FrameData {
                    surface: surface.clone(),
                    epoch,
                    generation: canvas.scene.resource_revision,
                    source_epoch: canvas.epoch,
                },
            );
            canvas.frame_pending = true;
        }
        canvas.dirty = false;
        canvas.buffers[slot].attach_to(&surface)?;
        if let Some(presentation) = &self.presentation {
            presentation.feedback(
                &surface,
                &self.qh,
                PresentationData {
                    source_epoch: canvas.epoch,
                    visibility: 0,
                    update: canvas.scene.resource_revision,
                    surface: surface.clone(),
                    generation: canvas.scene.resource_revision,
                },
            );
        }
        canvas
            .layer
            .as_ref()
            .ok_or("canvas layer missing")?
            .commit();
        canvas.buffer_attached = true;
        Ok(())
    }
    pub(super) fn canvas_configure(&mut self, layer: &LayerSurface, size: (u32, u32)) {
        let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| canvas.layer.as_ref() == Some(layer))
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        if let Some(canvas) = self.canvases.get_mut(&key) {
            canvas.configured = true;
            if let Some(viewport) = &canvas.viewport {
                viewport.set_destination(canvas.render_size.0 as i32, canvas.render_size.1 as i32);
            }
            if size.0 == 0 || size.1 == 0 {
                canvas.configured = false;
            }
        }
        let _ = self.canvas_draw(&key);
    }
    pub(super) fn canvas_scale(&mut self, surface: &wl_surface::WlSurface, scale120: u32) {
        let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| {
                canvas
                    .layer
                    .as_ref()
                    .is_some_and(|layer| layer.wl_surface() == surface)
            })
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        if let Some(canvas) = self.canvases.get_mut(&key)
            && canvas.scale120 != scale120
        {
            canvas.scale120 = scale120;
            canvas.geometry = None;
            canvas.pool = None;
            canvas.buffers.clear();
        }
        self.canvas_ui(&key, UiEvent::Relayout);
    }
    pub(super) fn canvas_frame(&mut self, surface: &wl_surface::WlSurface, epoch: u64) {
        let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| {
                canvas
                    .layer
                    .as_ref()
                    .is_some_and(|layer| layer.wl_surface() == surface)
            })
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        let again = if let Some(canvas) = self.canvases.get_mut(&key) {
            if canvas.callbacks.accept(epoch) {
                canvas.frame_pending = false;
                canvas.dirty
            } else {
                false
            }
        } else {
            false
        };
        if again {
            let _ = self.canvas_draw(&key);
        }
    }
    pub(super) fn canvas_pointer(&mut self, event: &PointerEvent) {
        let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| {
                canvas
                    .layer
                    .as_ref()
                    .is_some_and(|layer| layer.wl_surface() == &event.surface)
            })
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        let Some(canvas) = self.canvases.get_mut(&key) else {
            return;
        };
        let bounds = canvas.bounds.unwrap_or(Rect::new(0., 0., 0., 0.));
        let (x, y) = (
            event.position.0 as f32 + bounds.x - canvas.source_offset.0,
            event.position.1 as f32 + bounds.y - canvas.source_offset.1,
        );
        let inside = canvas
            .source
            .input_regions()
            .iter()
            .any(|region| region.contains(x, y));
        let dispatched = match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                Some(if canvas.pressed {
                    UiEvent::Drag { x, y }
                } else {
                    UiEvent::Hover { x, y }
                })
            }
            PointerEventKind::Leave { .. } => {
                canvas.pressed = false;
                Some(UiEvent::Hover { x: -1., y: -1. })
            }
            PointerEventKind::Press { button: 0x110, .. } if inside => {
                canvas.pressed = true;
                Some(UiEvent::Click { x, y })
            }
            PointerEventKind::Release { button: 0x110, .. } => {
                canvas.pressed = false;
                Some(UiEvent::Release)
            }
            PointerEventKind::Press { button: 0x111, .. } if inside => {
                Some(UiEvent::ContextMenu { x, y })
            }
            PointerEventKind::Press { button: 0x112, .. } if inside => {
                Some(UiEvent::SecondaryClick { x, y })
            }
            _ => None,
        };
        if let Some(event) = dispatched {
            self.canvas_ui(&key, event);
        }
    }
    pub(super) fn canvas_release_layer(&mut self, layer: &LayerSurface) {
        if let Some(key) = self
            .canvases
            .iter()
            .find(|(_, canvas)| canvas.layer.as_ref() == Some(layer))
            .map(|(key, _)| key.clone())
        {
            if let Some(canvas) = self.canvases.get_mut(&key)
                && canvas.retain_when_closed
                && !canvas.lifecycle.open
            {
                canvas.layer = None;
                if let Some(fractional) = canvas.fractional.take() {
                    fractional.destroy();
                }
                if let Some(viewport) = canvas.viewport.take() {
                    viewport.destroy();
                }
                canvas.pool = None;
                canvas.buffers.clear();
                canvas.damage_history.clear();
                canvas.geometry = None;
                canvas.mapped_bounds = None;
                canvas.configured = false;
                canvas.buffer_attached = false;
                return;
            }
            self.canvases.remove(&key);
        }
    }
    pub(super) fn canvas_output_removed(&mut self, output: &wl_output::WlOutput) {
        self.canvases.retain(|_, canvas| &canvas.output != output);
    }
    pub(super) fn canvas_output_updated(&mut self, output: &wl_output::WlOutput) {
        let Some((width, height)) = self
            .output
            .info(output)
            .and_then(|info| info.logical_size)
            .filter(|(width, height)| *width > 0 && *height > 0)
        else {
            return;
        };
        let keys = self
            .canvases
            .iter()
            .filter(|(_, canvas)| &canvas.output == output)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in keys {
            let changed = if let Some(canvas) = self.canvases.get_mut(&key) {
                canvas.viewport_size = (width as u32, height as u32);
                canvas.source.viewport(width as u32, height as u32)
            } else {
                false
            };
            if changed {
                self.canvas_ui(&key, UiEvent::Relayout);
            }
        }
    }
    pub(super) fn release_canvas(&mut self, key: &(String, String)) {
        if self
            .canvases
            .get(key)
            .is_some_and(|canvas| Some(canvas.epoch) == self.panel_canvas_owner)
        {
            self.close_panel();
            self.panel_canvas_owner = None;
        }
        if let Some(canvas) = self.canvases.remove(key) {
            // Source teardown joins the worker; return its freed scratch/arena pages
            // after the scene and renderer owners have also been dropped.
            drop(canvas);
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            {
                // SAFETY: process-local allocator trim after canvas worker retirement.
                unsafe { libc::malloc_trim(0) };
                bead_core::profile::mark("canvas_retired_trimmed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_wakeup_only_schedules_visible_unarmed_sources() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(16);
        assert_eq!(
            canvas_wakeup_delay(true, false, Some(deadline), now),
            Some(Duration::from_millis(16))
        );
        assert_eq!(canvas_wakeup_delay(false, false, Some(deadline), now), None);
        assert_eq!(canvas_wakeup_delay(true, true, Some(deadline), now), None);
        assert_eq!(
            canvas_wakeup_delay(true, false, Some(now), now),
            Some(Duration::from_millis(1))
        );
    }

    #[test]
    fn host_canvas_envelopes_are_routed_out_of_the_canvas_runtime() {
        assert!(canvas_event_belongs_to_host(&UiEvent::CanvasPrepare {
            view: "widgets-drag".into(),
            output: Some("fixture-output".into()),
            values: std::collections::BTreeMap::new(),
            initial_bounds: Some(Rect::new(10., 20., 30., 40.)),
        }));
        assert!(canvas_event_belongs_to_host(&UiEvent::CanvasModel {
            view: "widgets-canvas".into(),
            output: Some("fixture-output".into()),
            values: std::collections::BTreeMap::new(),
        }));
        assert!(canvas_event_belongs_to_host(&UiEvent::CanvasOffer {
            view: "widgets-canvas".into(),
            output: Some("fixture-output".into()),
            open: true,
            values: std::collections::BTreeMap::new(),
        }));
        assert!(canvas_event_belongs_to_host(&UiEvent::CompanionVisibility(
            true
        )));
        assert!(canvas_event_belongs_to_host(&UiEvent::CompanionVisibility(
            false
        )));
        assert!(!canvas_event_belongs_to_host(&UiEvent::Model(
            std::collections::BTreeMap::new()
        )));
    }

    #[test]
    fn prepared_canvas_reserves_target_extent_at_source_origin_and_tracks_active_origin() {
        let source = Rect::new(12., 18., 69., 28.);
        let active = Rect::new(300., 400., 71., 29.);
        let reserved = Some((221., 97.));
        assert_eq!(
            canvas_surface_bounds(false, active, Some(source), reserved),
            Rect::new(source.x, source.y, 221., 97.)
        );
        assert_eq!(
            canvas_surface_bounds(true, active, Some(source), reserved),
            Rect::new(active.x, active.y, 221., 97.)
        );
        let mut lifecycle = CanvasLifecycle::new(false, 0);
        assert!(!lifecycle.unmap_if_buffered(false));
        assert!(!lifecycle.unmapped);
        assert!(lifecycle.unmap_if_buffered(true));
        assert!(lifecycle.unmapped);
    }

    #[test]
    fn prepared_canvas_reserves_unclipped_target_size_until_a_new_prepare() {
        let source = Rect::new(12., 18., 70., 29.);
        let prepared = Rect::new(300., 400., 221., 97.);
        let reserved = reserve_canvas_size(Some(source), Some(prepared));
        assert_eq!(reserved, Some((221., 97.)));

        for (x, y, natural_width) in [(20., 22., 71.), (40., 26., 111.), (80., 30., 221.)] {
            assert_eq!(
                canvas_bounds_with_reserve(Rect::new(x, y, natural_width, 29.), reserved,),
                Rect::new(x, y, 221., 97.),
                "active content moves naturally inside the fixed reservation"
            );
        }

        let larger = Rect::new(40., 30., 300., 120.);
        assert_eq!(
            canvas_bounds_with_reserve(larger, reserved),
            larger,
            "a reservation cannot crop content that legitimately grows beyond its hint"
        );

        let next_source = Rect::new(12., 18., 42., 24.);
        let next_prepared = Rect::new(80., 90., 55., 33.);
        let next_reserved = reserve_canvas_size(Some(next_source), Some(next_prepared));
        assert_eq!(next_reserved, Some((55., 33.)));
        assert_eq!(
            canvas_bounds_with_reserve(next_prepared, next_reserved),
            next_prepared,
            "a fresh hidden prepare replaces the prior gesture's larger reserve"
        );
    }

    #[test]
    fn prepared_canvas_reuses_on_offer_and_expires_after_the_last_close() {
        let mut lifecycle = CanvasLifecycle::new(false, 0);
        let first_prepare = lifecycle.retention_generation;
        assert!(lifecycle.expires(first_prepare));

        // Palette/reload and any model wakeups that arrive while still hidden
        // are drained before the active Model is allowed to map the layer.
        lifecycle.warmup_packets_pending += 3;
        lifecycle.offer_open();
        for _ in 0..3 {
            let stale_warmup = lifecycle.warmup_response();
            assert!(!lifecycle.accept_scene(stale_warmup));
        }
        assert!(!lifecycle.expires(first_prepare));
        assert!(lifecycle.accept_scene(false));
        let opened = lifecycle.retention_generation;
        assert!(!lifecycle.expires(opened));

        lifecycle.close();
        let first_close = lifecycle.retention_generation;
        assert!(lifecycle.unmap());
        lifecycle.close();
        assert!(
            !lifecycle.unmap(),
            "repeated close offers must not submit another null-buffer commit"
        );
        let closed = lifecycle.retention_generation;
        assert!(!lifecycle.expires(first_close));
        assert!(!lifecycle.expires(opened));
        assert!(lifecycle.expires(closed));

        lifecycle.offer_open();
        assert!(
            lifecycle.begin_remap(),
            "a retained layer must reconfigure on an empty commit before drawing again"
        );
        assert!(!lifecycle.begin_remap());
        assert!(lifecycle.accept_scene(false));
        let reopened = lifecycle.retention_generation;
        assert!(!lifecycle.expires(closed));
        assert!(!lifecycle.expires(reopened));
    }

    fn scene(offset: (f32, f32)) -> Scene {
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![bead_render::SceneLayer {
                id: "sample".into(),
                parent: None,
                own: 0..1,
                subtree_end: 1,
                bounds: Rect::new(96., 96., 88., 34.),
            }],
            prepared: Default::default(),
            font_files: Vec::new(),
            width: 1920.,
            slide: None,
            primitives: vec![bead_render::Primitive::Rounded {
                rect: Rect::new(96., 96., 88., 34.),
                radius: 0.,
                fill: [200, 0, 0, 255],
                border: None,
            }],
            offset,
            bounds: Rect::new(-32., -32., 1984., 1144.),
        }
    }
    #[test]
    fn cropped_canvas_keeps_pixels_and_anchor_without_an_output_sized_raster() {
        for offset in [(0., 0.), (32., 32.)] {
            let original = scene(offset);
            let bounds = visible_bounds(&original, Some((1920, 1080))).unwrap();
            assert_eq!(bounds, Rect::new(96. + offset.0, 96. + offset.1, 89., 35.));
            let cropped = crop_scene(original, bounds);
            let mut bytes = vec![0; 89 * 35 * 4];
            let mut renderer = bead_render::renderer(RendererKind::Cpu).unwrap();
            renderer.prepare(&cropped, 1.).unwrap();
            renderer
                .draw(
                    &cropped,
                    RenderTarget::Shm(ShmTarget {
                        bytes: &mut bytes,
                        width: 89,
                        height: 35,
                        scale: 1.,
                        format: PixelFormat::Rgba,
                    }),
                    Damage::full(89, 35),
                )
                .unwrap();
            for (index, pixel) in bytes.as_chunks::<4>().0.iter().enumerate() {
                let painted = index % 89 < 88 && index / 89 < 34;
                assert_eq!(*pixel, if painted { [200, 0, 0, 255] } else { [0; 4] });
            }
        }
    }
    #[test]
    fn fractional_canvas_translation_keeps_size_and_contains_painted_content() {
        for fraction in [0., 0.25, 0.5, 0.75] {
            let original = scene((fraction, fraction));
            let bounds = visible_bounds(&original, Some((1920, 1080))).unwrap();
            assert_eq!((bounds.w, bounds.h), (89., 35.));
            let painted = Rect::new(96. + fraction, 96. + fraction, 88., 34.);
            assert!(bounds.x <= painted.x && bounds.y <= painted.y);
            assert!(bounds.x + bounds.w >= painted.x + painted.w);
            assert!(bounds.y + bounds.h >= painted.y + painted.h);
        }
    }
    #[test]
    fn empty_and_transparent_canvases_need_no_surface_bounds() {
        let mut empty = scene((0., 0.));
        empty.primitives.clear();
        assert_eq!(visible_bounds(&empty, Some((1920, 1080))), None);
        let mut hidden = scene((0., 0.));
        hidden.opacity = 0.;
        assert_eq!(visible_bounds(&hidden, Some((1920, 1080))), None);
    }

    #[test]
    fn prepared_extent_is_unclipped_but_open_bounds_are_output_clipped() {
        let mut prepared = scene((0., 0.));
        prepared.primitives = vec![bead_render::Primitive::Rounded {
            rect: Rect::new(1800., 100., 220., 96.),
            radius: 0.,
            fill: [200, 0, 0, 255],
            border: None,
        }];
        let unbounded = visible_bounds(&prepared, None).unwrap();
        let open = visible_bounds(&prepared, Some((1920, 1080))).unwrap();
        assert_eq!((unbounded.w, unbounded.h), (221., 97.));
        assert_eq!(open.w, 120.);
        assert!(unbounded.w > open.w);
    }
}

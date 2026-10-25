// SPDX-License-Identifier: MIT OR Apache-2.0
//! Lazy output canvases. Each mapped surface is only the visible content bounds.
use super::*;

pub(super) struct Canvas {
    pub(super) epoch: u64,
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
    pub(super) dirty: bool,
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

fn visible_bounds(scene: &Scene, output: (u32, u32)) -> Option<Rect> {
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
        let clipped = painted.intersect(Rect::new(0., 0., output.0 as f32, output.1 as f32));
        if clipped.w <= 0. || clipped.h <= 0. {
            continue;
        }
        bounds = Some(bounds.map_or(clipped, |current| current.union(clipped)));
    }
    let bounds = bounds?;
    let left = bounds.x.floor();
    let top = bounds.y.floor();
    let right = (bounds.x + bounds.w).ceil();
    let bottom = (bounds.y + bounds.h).ceil();
    Some(Rect::new(left, top, right - left, bottom - top))
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

impl App {
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
                self.release_canvas(&key);
            }
            return;
        }
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
            .output
            .info(&output)
            .map_or(120, |info| info.scale_factor.max(1) as u32 * 120);
        self.canvases.insert(
            key.clone(),
            Canvas {
                epoch,
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
                dirty: false,
            },
        );
        if let Some(palette) = self.wallpaper_palette.clone() {
            self.canvas_ui(&key, UiEvent::WallpaperPalette(palette));
        }
        self.canvas_ui(&key, UiEvent::OutputName(key.0.clone()));
        self.canvas_ui(&key, UiEvent::PanelVisibility(true));
        self.canvas_ui(&key, UiEvent::Reload);
    }
    pub(super) fn canvas_ui(&mut self, key: &(String, String), event: UiEvent) {
        let Some(canvas) = self.canvases.get_mut(key) else {
            return;
        };
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
                let mut bounds = visible_bounds(&scene, canvas.viewport_size);
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
                let Some(bounds) = bounds else {
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
                canvas.source_offset = scene.offset;
                canvas.scene = crop_scene(scene, bounds);
                canvas.bounds = Some(bounds);
                canvas.render_size = (bounds.w as u32, bounds.h as u32);
                canvas.dirty = true;
                if let Err(error) = self.canvas_map_bounds(key) {
                    eprintln!("CANVAS_ERROR {error}");
                    return;
                }
                let _ = self.canvas_draw(key);
            }
            Ok(None) => (),
            Err(error) => eprintln!("CANVAS_ERROR {error}"),
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
        if matches!(
            &event,
            UiEvent::CompanionSelectAt { .. }
                | UiEvent::CompanionSelect { .. }
                | UiEvent::CompanionAction { .. }
                | UiEvent::HostModel(_)
                | UiEvent::CanvasOffer { .. }
        ) {
            self.ui(event);
        } else {
            self.canvas_ui(&key, event);
        }
    }
    fn canvas_map_bounds(&mut self, key: &(String, String)) -> Result<(), Box<dyn Error>> {
        let (output, bounds, previous, viewport_size, existing) = {
            let canvas = self.canvases.get(key).ok_or("canvas missing")?;
            (
                canvas.output.clone(),
                canvas.bounds.ok_or("empty canvas")?,
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
                Layer::Bottom,
                Some(&self.options.namespace),
                Some(&output),
            );
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            if let Some(canvas) = self.canvases.get_mut(key) {
                canvas.fractional = fractional;
                canvas.viewport = viewport;
            }
            layer
        };
        layer.set_size(size.0, size.1);
        layer.set_exclusive_zone(-1);
        layer.set_anchor(Anchor::TOP | Anchor::LEFT);
        layer.set_margin(bounds.y.round() as i32, 0, 0, bounds.x.round() as i32);
        if let Some(canvas) = self.canvases.get_mut(key) {
            canvas.layer = Some(layer.clone());
            if let Some(viewport) = &canvas.viewport {
                viewport.set_destination(size.0 as i32, size.1 as i32);
            }
            if previous.is_none_or(|previous| previous.w != bounds.w || previous.h != bounds.h) {
                canvas.mapped_bounds = Some(bounds);
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
        if !canvas.configured || canvas.bounds.is_none() {
            return Ok(());
        }
        if canvas.frame_pending {
            canvas.dirty = true;
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
            surface.frame(
                &self.qh,
                FrameData {
                    surface: surface.clone(),
                    epoch: canvas.callbacks.request(),
                    generation: canvas.scene.resource_revision,
                    source_epoch: canvas.epoch,
                },
            );
            canvas.frame_pending = true;
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
        canvas.renderer.draw(
            &canvas.scene,
            RenderTarget::Shm(ShmTarget {
                bytes: &mut *bytes,
                width: width as u32,
                height: height as u32,
                scale,
                format,
            }),
            damage,
        )?;
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
        surface.frame(
            &self.qh,
            FrameData {
                surface: surface.clone(),
                epoch: canvas.callbacks.request(),
                generation: canvas.scene.resource_revision,
                source_epoch: canvas.epoch,
            },
        );
        canvas.frame_pending = true;
        canvas.dirty = false;
        canvas.buffers[slot].attach_to(&surface)?;
        canvas
            .layer
            .as_ref()
            .ok_or("canvas layer missing")?
            .commit();
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
        self.canvases.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            let bounds = visible_bounds(&original, (1920, 1080)).unwrap();
            assert_eq!(bounds, Rect::new(96. + offset.0, 96. + offset.1, 88., 34.));
            let cropped = crop_scene(original, bounds);
            let mut bytes = vec![0; 88 * 34 * 4];
            let mut renderer = bead_render::renderer(RendererKind::Cpu).unwrap();
            renderer.prepare(&cropped, 1.).unwrap();
            renderer
                .draw(
                    &cropped,
                    RenderTarget::Shm(ShmTarget {
                        bytes: &mut bytes,
                        width: 88,
                        height: 34,
                        scale: 1.,
                        format: PixelFormat::Rgba,
                    }),
                    Damage::full(88, 34),
                )
                .unwrap();
            assert!(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == [200, 0, 0, 255])
            );
        }
    }
    #[test]
    fn empty_and_transparent_canvases_need_no_surface_bounds() {
        let mut empty = scene((0., 0.));
        empty.primitives.clear();
        assert_eq!(visible_bounds(&empty, (1920, 1080)), None);
        let mut hidden = scene((0., 0.));
        hidden.opacity = 0.;
        assert_eq!(visible_bounds(&hidden, (1920, 1080)), None);
    }
}

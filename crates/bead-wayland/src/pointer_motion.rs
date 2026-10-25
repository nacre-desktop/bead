// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pointer poses: one small synchronized child, no motion-time raster.
use super::*;

pub(super) struct Visual {
    spec: DragVisual,
    draw: bead_render::Primitive,
    surface: wl_surface::WlSurface,
    role: wl_subsurface::WlSubsurface,
    viewport: WpViewport,
    _buffer: Buffer,
    _pool: SlotPool,
    x: f32,
    dirty: bool,
    received: u64,
    pending: Option<(f32, f32)>,
    timer: bool,
    token: u64,
    released: bool,
    retiring: bool,
    scale120: u32,
    generation: u64,
    mapped: bool,
    pixels: (u32, u32),
}
impl Drop for Visual {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.role.destroy();
        self.surface.destroy();
    }
}
impl Visual {
    pub(super) fn stage_commit(&mut self) -> Result<(), Box<dyn Error>> {
        if self.retiring {
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            return Ok(());
        }
        if !self.mapped && !self.retiring {
            self._buffer.attach_to(&self.surface)?;
            self.surface
                .damage_buffer(0, 0, self.pixels.0 as i32, self.pixels.1 as i32);
            self.surface.commit();
            self.mapped = true;
        }
        Ok(())
    }
    pub(super) fn feedback(
        &self,
        presentation: &WpPresentation,
        parent: &wl_surface::WlSurface,
        qh: &QueueHandle<App>,
    ) {
        if !self.retiring {
            presentation.feedback(
                parent,
                qh,
                Feedback {
                    x: self.x,
                    received: self.received,
                    committed: monotonic_ns(),
                },
            );
        }
    }
}
struct Feedback {
    x: f32,
    received: u64,
    committed: u64,
}
impl Dispatch<WpPresentationFeedback, Feedback> for App {
    fn event(
        _: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &Feedback,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation_feedback::Event::Presented {
            tv_sec_hi,
            tv_sec_lo,
            tv_nsec,
            ..
        } = event
        {
            let presented = ((u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo)) * 1_000_000_000
                + u64::from(tv_nsec);
            println!(
                "POINTER_VISUAL_PRESENTED x={} received_ns={} commit_ns={} presented_ns={presented}",
                data.x, data.received, data.committed
            );
        }
    }
}
fn pose(spec: &DragVisual, x: f32) -> f32 {
    let fraction = ((x - spec.pointer.0) / (spec.pointer.1 - spec.pointer.0)).clamp(0., 1.);
    spec.travel.0 + fraction * (spec.travel.1 - spec.travel.0)
}
fn rect(primitive: &bead_render::Primitive) -> Rect {
    match primitive {
        bead_render::Primitive::Rounded { rect, .. }
        | bead_render::Primitive::Text { rect, .. }
        | bead_render::Primitive::Image { rect, .. }
        | bead_render::Primitive::Shadow { rect, .. }
        | bead_render::Primitive::Arc { rect, .. }
        | bead_render::Primitive::Polyline { rect, .. } => *rect,
    }
}
fn painted_layer(scene: &Scene, id: &str) -> Option<bead_render::Primitive> {
    let layer = scene.layers.iter().find(|layer| layer.id == id)?;
    if layer.own.len() != 1 || layer.subtree_end != layer.own.end {
        return None;
    }
    let draw = scene.primitives.get(layer.own.start)?;
    match draw {
        bead_render::Primitive::Rounded { fill, border, .. }
            if fill[3] > 0 || border.is_some_and(|(color, width)| color[3] > 0 && width > 0.) =>
        {
            Some(draw.clone())
        }
        _ => None,
    }
}
fn raster_scene(parent: &Scene, draw: &bead_render::Primitive) -> Scene {
    let bounds = rect(draw);
    let mut scene = parent.clone();
    let mut primitive = draw.clone();
    if let bead_render::Primitive::Rounded { rect, .. } = &mut primitive {
        rect.x = 0.;
        rect.y = 0.;
    }
    scene.primitives = vec![primitive];
    scene.layers.clear();
    scene.clips.clear();
    scene.prepared.clear();
    scene.font_files.clear();
    scene.asset_data.clear();
    scene.motion = None;
    scene.slide = None;
    scene.offset = (0., 0.);
    scene.width = bounds.w;
    scene.bounds = Rect::new(0., 0., bounds.w, bounds.h);
    scene
}
fn same_skin(a: &bead_render::Primitive, b: &bead_render::Primitive) -> bool {
    match (a, b) {
        (
            bead_render::Primitive::Rounded {
                rect: a,
                radius: ar,
                fill: af,
                border: ab,
            },
            bead_render::Primitive::Rounded {
                rect: b,
                radius: br,
                fill: bf,
                border: bb,
            },
        ) => a.y == b.y && a.w == b.w && a.h == b.h && ar == br && af == bf && ab == bb,
        _ => false,
    }
}
impl App {
    pub(super) fn pointer_visual_begin(&mut self, index: usize, x: f32, y: f32) {
        if !self.surfaces[index].companion
            || self
                .panel_animations
                .active(self.started.elapsed().as_secs_f64())
        {
            return;
        }
        if let Some(visual) = &mut self.surfaces[index].pointer_visual {
            if visual.spec.grab.hit_test(x, y, None, None) && !visual.retiring {
                visual.released = false;
                visual.token = monotonic_ns();
            } else {
                visual.released = true;
                visual.retiring = true;
            }
            return;
        }
        let Some(spec) = self.panel_runtime.as_ref().and_then(|source| {
            source
                .drag_visuals()
                .into_iter()
                .find(|v| v.grab.hit_test(x, y, None, None))
        }) else {
            return;
        };
        if let Err(error) = self.pointer_visual_create(index, spec, x) {
            eprintln!("POINTER_VISUAL_ERROR {error}");
        }
    }
    fn pointer_visual_create(
        &mut self,
        index: usize,
        spec: DragVisual,
        x: f32,
    ) -> Result<(), Box<dyn Error>> {
        let Some((original, _, _)) = &self.companion else {
            return Ok(());
        };
        let Some(draw) = painted_layer(original, &spec.id) else {
            return Ok(());
        };
        let rect = rect(&draw);
        if ![
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            spec.pointer.0,
            spec.pointer.1,
            spec.travel.0,
            spec.travel.1,
        ]
        .iter()
        .all(|n| n.is_finite())
            || rect.w <= 0.
            || rect.h <= 0.
            || spec.pointer.1 <= spec.pointer.0
            || spec.travel.1 < spec.travel.0
        {
            return Ok(());
        }
        let scale = self.surfaces[index].scale120 as f32 / 120.;
        let width = (rect.w.ceil() * scale).ceil() as u32;
        let height = (rect.h.ceil() * scale).ceil() as u32;
        let Some(bytes) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|area| area.checked_mul(4))
            .filter(|bytes| *bytes <= 64 * 1024)
        else {
            return Ok(());
        };
        let mut pool = SlotPool::new(bytes.max(4096), &self.shm)?;
        let (buffer, pixels) = pool.create_buffer(
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
        )?;
        let scene = raster_scene(original, &draw);
        let mut renderer = bead_render::CpuRenderer::default();
        renderer.render(
            &scene,
            ShmTarget {
                bytes: pixels,
                width,
                height,
                scale,
                format: PixelFormat::Bgra,
            },
            Damage {
                x: 0,
                y: 0,
                w: width,
                h: height,
            },
        )?;
        let subcompositor = self.subcompositor.as_ref().ok_or("missing subcompositor")?;
        let viewporter = self.viewporter.as_ref().ok_or("missing viewporter")?;
        let child = self.compositor.create_surface(&self.qh);
        let role =
            subcompositor.get_subsurface(&child, self.surfaces[index].wl_surface(), &self.qh, ());
        let viewport = viewporter.get_viewport(&child, &self.qh, ());
        viewport.set_destination(rect.w.ceil() as i32, rect.h.ceil() as i32);
        let empty = Region::new(&self.compositor)?;
        child.set_input_region(Some(empty.wl_region()));
        role.set_sync();
        let target = pose(&spec, x);
        self.surfaces[index].pointer_visual = Some(Visual {
            spec,
            draw,
            surface: child,
            role,
            viewport,
            _buffer: buffer,
            _pool: pool,
            x: target,
            dirty: true,
            received: monotonic_ns(),
            pending: None,
            timer: false,
            token: monotonic_ns(),
            released: false,
            retiring: false,
            scale120: self.surfaces[index].scale120,
            generation: original.resource_revision,
            mapped: false,
            pixels: (width, height),
        });
        // Removing the parent copy and mapping the child happen on the same parent commit.
        let qh = self.qh.clone();
        self.draw(index, &qh);
        Ok(())
    }
    /// Coalesce script work independently of pointer poses. Release is an ordering barrier.
    pub(super) fn pointer_visual_motion(&mut self, index: usize, x: f32, y: f32) -> bool {
        let Some(visual) = &mut self.surfaces[index].pointer_visual else {
            return false;
        };
        if visual.released || visual.retiring {
            return false;
        }
        visual.x = pose(&visual.spec, x);
        visual.received = monotonic_ns();
        visual.dirty = true;
        visual.pending = Some((x, y));
        let arm = !visual.timer;
        let token = visual.token;
        visual.timer = true;
        let parent = self.surfaces[index].wl_surface().clone();
        if arm {
            let epoch = self.panel_epoch;
            if let Err(error) = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(34)),
                move |_, _, app| {
                    app.with_epoch(epoch, |app| {
                        if let Some(index) =
                            app.surfaces.iter().position(|s| s.wl_surface() == &parent)
                            && app.surfaces[index]
                                .pointer_visual
                                .as_ref()
                                .is_some_and(|v| v.token == token)
                        {
                            app.pointer_visual_flush(index);
                        }
                    });
                    TimeoutAction::Drop
                },
            ) {
                self.fail(error);
            }
        }
        self.pointer_visual_commit(index);
        true
    }
    pub(super) fn pointer_visual_flush(&mut self, index: usize) {
        let pending = self.surfaces[index].pointer_visual.as_mut().and_then(|v| {
            v.timer = false;
            v.pending.take()
        });
        if let Some((x, y)) = pending {
            self.view_event(true, UiEvent::Drag { x, y });
        }
    }
    pub(super) fn pointer_visual_release(&mut self, index: usize) {
        self.pointer_visual_flush(index);
        if let Some(visual) = &mut self.surfaces[index].pointer_visual {
            visual.released = true;
            visual.token = monotonic_ns();
        }
    }
    pub(super) fn pointer_visual_position(&mut self, index: usize) {
        let offset = self.surfaces[index].offset;
        if let Some(visual) = &mut self.surfaces[index].pointer_visual {
            let rect = rect(&visual.draw);
            visual.role.set_position(
                (visual.x + offset.0).round() as i32,
                (rect.y + offset.1).round() as i32,
            );
            visual.dirty = false;
        }
    }
    pub(super) fn pointer_visual_commit(&mut self, index: usize) {
        let surface = &self.surfaces[index];
        if surface.callbacks.pending
            || surface.redraw
            || !surface
                .pointer_visual
                .as_ref()
                .is_some_and(|v| v.dirty && v.mapped)
        {
            return;
        }
        self.pointer_visual_position(index);
        let s = &mut self.surfaces[index];
        let parent = s.wl_surface().clone();
        let generation = self.companion.as_ref().map_or(0, |c| c.0.resource_revision);
        parent.frame(
            &self.qh,
            FrameData {
                surface: parent.clone(),
                epoch: s.callbacks.request(),
                generation,
                source_epoch: self.panel_epoch,
            },
        );
        if let Some(presentation) = &self.presentation {
            let visual = s.pointer_visual.as_ref().unwrap();
            presentation.feedback(
                &parent,
                &self.qh,
                Feedback {
                    x: visual.x,
                    received: visual.received,
                    committed: monotonic_ns(),
                },
            );
        }
        bead_core::profile::mark("pointer_visual_commit");
        parent.commit();
    }
    /// A scratch scene leaves the source transaction and worker rasters intact.
    pub(super) fn pointer_visual_scene(&mut self, index: usize) -> Option<Scene> {
        let id = self.surfaces[index]
            .pointer_visual
            .as_ref()?
            .spec
            .id
            .clone();
        let live = self.panel_runtime.as_ref().is_some_and(|source| {
            source.drag_visuals().iter().any(|v| {
                v.id == id
                    && v.grab
                        == self.surfaces[index]
                            .pointer_visual
                            .as_ref()
                            .unwrap()
                            .spec
                            .grab
            })
        });
        let s = &mut self.surfaces[index];
        let visual = s.pointer_visual.as_mut()?;
        let mut scene = self.companion.as_ref()?.0.clone();
        let layer = scene.layers.iter().find(|layer| layer.id == visual.spec.id);
        let final_pose = layer
            .and_then(|layer| scene.primitives.get(layer.own.start))
            .map(|p| rect(p).x);
        visual.retiring |= !self.panel_target
            || self.animations.active(self.started.elapsed().as_secs_f64())
            || visual.scale120 != s.scale120
            || visual.generation != scene.resource_revision
            || layer.is_none()
            || !live
            || !painted_layer(&scene, &visual.spec.id)
                .as_ref()
                .is_some_and(|draw| same_skin(draw, &visual.draw))
            || (visual.released && final_pose.is_some_and(|x| (x - visual.x).abs() <= 0.5));
        if visual.retiring {
            // Destruction itself is immediate in Wayland. Unmap synchronously first,
            // restore the parent's copy, then destroy only after that parent commits.
            return None;
        }
        let layer = layer.unwrap();
        let removed = layer.own.clone();
        let mut index = 0;
        scene.retain_primitives(|_| {
            let keep = !removed.contains(&index);
            index += 1;
            keep
        });
        scene.prepared.clear();
        Some(scene)
    }
    pub(super) fn pointer_visual_retire(&mut self, index: usize) -> Option<(f32, f32)> {
        if self.surfaces[index]
            .pointer_visual
            .as_ref()
            .is_some_and(|v| v.retiring)
        {
            return self.surfaces[index]
                .pointer_visual
                .take()
                .and_then(|v| v.pending);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absolute_pose_clamps_without_step_quantization_or_delta_drift() {
        let spec = DragVisual {
            id: "visual".into(),
            grab: Rect {
                x: 0.,
                y: 0.,
                w: 100.,
                h: 30.,
            },
            travel: (6., 86.),
            pointer: (10., 90.),
        };
        assert_eq!(pose(&spec, -200.), 6.);
        assert_eq!(pose(&spec, 200.), 86.);
        assert_eq!(pose(&spec, 42.25), 38.25);
        assert_eq!(pose(&spec, 10.), 6.);
    }

    #[test]
    fn child_raster_uses_painted_scene_pixels_in_local_coordinates() {
        let bounds = Rect::new(92., 135., 8., 16.);
        let scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![bead_render::SceneLayer {
                id: "thumb".into(),
                parent: None,
                own: 0..1,
                subtree_end: 1,
                bounds,
            }],
            prepared: Default::default(),
            font_files: vec![],
            width: 500.,
            slide: None,
            primitives: vec![bead_render::Primitive::Rounded {
                rect: bounds,
                radius: 3.,
                fill: [40, 180, 120, 255],
                border: None,
            }],
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., 500., 900.),
        };
        let draw = painted_layer(&scene, "thumb").expect("painted layer");
        let child = raster_scene(&scene, &draw);
        let mut pixels = vec![0; 12 * 24 * 4];
        let mut renderer = bead_render::CpuRenderer::default();
        renderer.prepare(&child, 1.5).unwrap();
        renderer
            .render(
                &child,
                ShmTarget {
                    bytes: &mut pixels,
                    width: 12,
                    height: 24,
                    scale: 1.5,
                    format: PixelFormat::Bgra,
                },
                Damage::full(12, 24),
            )
            .unwrap();
        let center = (12 * 12 + 6) * 4;
        assert_eq!(&pixels[center..center + 4], &[120, 180, 40, 255]);
        assert_eq!(pixels[3], 0, "rounded corner stays transparent");
        let mut invisible = scene;
        if let bead_render::Primitive::Rounded { fill, .. } = &mut invisible.primitives[0] {
            *fill = [0; 4];
        }
        assert!(
            painted_layer(&invisible, "thumb").is_none(),
            "input hit geometry cannot replace paint"
        );
    }
}

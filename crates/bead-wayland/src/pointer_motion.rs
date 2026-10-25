// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pointer poses: one small synchronized child, isolated from panel rendering.
use super::*;

pub(super) struct Visual {
    spec: DragVisual,
    draw: bead_render::Primitive,
    surface: wl_surface::WlSurface,
    role: wl_subsurface::WlSubsurface,
    viewport: WpViewport,
    buffers: Vec<Buffer>,
    pool: SlotPool,
    bounds: Rect,
    opacity: f32,
    release_target: u64,
    x: f32,
    dirty: bool,
    commit_queued: bool,
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
    pub(super) fn accepts_motion(&self) -> bool {
        !self.released && !self.retiring
    }
    pub(super) fn stage_commit(&mut self) -> Result<(), Box<dyn Error>> {
        if self.retiring {
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            return Ok(());
        }
        if self.spec.fill.is_some() && (self.dirty || !self.mapped) {
            let free = self
                .buffers
                .iter()
                .position(|buffer| buffer.canvas(&mut self.pool).is_some());
            let slot = if let Some(slot) = free {
                slot
            } else if self.buffers.len() < 2 {
                let (buffer, _) = self.pool.create_buffer(
                    self.pixels.0 as i32,
                    self.pixels.1 as i32,
                    self.pixels.0 as i32 * 4,
                    wl_shm::Format::Argb8888,
                )?;
                self.buffers.push(buffer);
                self.buffers.len() - 1
            } else {
                // Keep the previous coherent slider; the parent's next callback retries.
                return Ok(());
            };
            let bytes = self.buffers[slot]
                .canvas(&mut self.pool)
                .ok_or("slider slot became busy")?;
            paint_child(
                &mut ShmTarget {
                    bytes,
                    width: self.pixels.0,
                    height: self.pixels.1,
                    scale: self.scale120 as f32 / 120.,
                    format: PixelFormat::Bgra,
                },
                &self.spec,
                &self.draw,
                self.bounds,
                self.x,
                self.opacity,
            )?;
            self.buffers[slot].attach_to(&self.surface)?;
            self.surface
                .damage_buffer(0, 0, self.pixels.0 as i32, self.pixels.1 as i32);
            self.surface.commit();
            self.dirty = false;
            self.mapped = true;
        } else if !self.mapped {
            self.buffers[0].attach_to(&self.surface)?;
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
        if !self.retiring
            && !self.dirty
            && (bead_core::profile::enabled() || bead_core::profile::frame_trace())
        {
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
                "POINTER_VISUAL_PRESENTED x={} received_ns={} commit_ns={} presented_ns={presented} observed_ns={}",
                data.x,
                data.received,
                data.committed,
                monotonic_ns()
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
fn paint_child(
    target: &mut ShmTarget<'_>,
    spec: &DragVisual,
    draw: &bead_render::Primitive,
    bounds: Rect,
    x: f32,
    opacity: f32,
) -> Result<(), Box<dyn Error>> {
    target.bytes.fill(0);
    let alpha = |mut color: [u8; 4]| {
        color[3] = (color[3] as f32 * opacity.clamp(0., 1.)).round() as u8;
        color
    };
    if let Some(fill) = &spec.fill {
        let width = fill.minimum + x - spec.travel.0;
        let to = if width < fill.gradient_min {
            fill.from
        } else {
            fill.to
        };
        bead_render::pointer_paint::rounded_gradient(
            target,
            Rect::new(
                fill.rect.x - bounds.x,
                fill.rect.y - bounds.y,
                width,
                fill.rect.h,
            ),
            fill.radius,
            alpha(fill.from),
            alpha(to),
            None,
        )?;
    }
    let bead_render::Primitive::Rounded {
        rect,
        radius,
        fill,
        border,
    } = draw
    else {
        return Err("unsupported pointer paint".into());
    };
    let left = if spec.fill.is_some() {
        x - bounds.x
    } else {
        0.
    };
    let top = if spec.fill.is_some() {
        rect.y - bounds.y
    } else {
        0.
    };
    bead_render::pointer_paint::rounded_gradient(
        target,
        Rect::new(left, top, rect.w, rect.h),
        *radius,
        alpha(*fill),
        alpha(*fill),
        border.map(|(color, width)| (alpha(color), width)),
    )?;
    if bead_core::profile::enabled()
        && let Some(fill) = &spec.fill
    {
        println!(
            "SLIDER_PAINT id={} received_x={x} fill_end={} handle_center={}",
            spec.id,
            fill.rect.x + fill.minimum + x - spec.travel.0,
            x + rect.w / 2.
        );
    }
    Ok(())
}
fn fill_skin(a: &Option<crate::DragFill>, b: &Option<crate::DragFill>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.id == b.id
                && a.rect.x == b.rect.x
                && a.rect.y == b.rect.y
                && a.rect.h == b.rect.h
                && a.minimum == b.minimum
                && a.radius == b.radius
                && a.gradient_min == b.gradient_min
                && a.from == b.from
                && a.to == b.to
        }
        _ => false,
    }
}
fn overlay_safe(scene: &Scene, spec: &DragVisual, bounds: Rect) -> bool {
    let Some(thumb) = scene.layers.iter().find(|l| l.id == spec.id) else {
        return false;
    };
    let mut ranges = vec![thumb.own.clone()];
    if let Some(fill) = &spec.fill {
        let Some(layer) = scene.layers.iter().find(|l| l.id == fill.id) else {
            return false;
        };
        ranges.push(layer.own.start..layer.subtree_end);
    }
    if ranges.iter().flat_map(|r| r.clone()).any(|i| {
        scene.clip_at(i).is_some_and(|clip| {
            bounds.x < clip.x
                || bounds.y < clip.y
                || bounds.x + bounds.w > clip.x + clip.w
                || bounds.y + bounds.h > clip.y + clip.h
        })
    }) {
        return false;
    }
    let first = ranges.iter().map(|r| r.start).min().unwrap();
    !scene.primitives.iter().enumerate().skip(first).any(|(i,draw)| {
        if ranges.iter().any(|range|range.contains(&i)) { return false; }
        if matches!(draw,bead_render::Primitive::Rounded {fill,border,..} if fill[3]==0 && border.is_none()) { return false; }
        let r=rect(draw);
        r.x<bounds.x+bounds.w && r.x+r.w>bounds.x && r.y<bounds.y+bounds.h && r.y+r.h>bounds.y
    })
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
        let bounds = if let Some(fill) = &spec.fill {
            if ![
                fill.rect.x,
                fill.rect.y,
                fill.rect.w,
                fill.rect.h,
                fill.minimum,
                fill.radius,
                fill.gradient_min,
            ]
            .iter()
            .all(|v| v.is_finite())
                || fill.rect.w <= 0.
                || fill.gradient_min < 0.
                || fill.minimum <= 0.
                || fill.radius < 0.
                || fill.rect.h <= 0.
                || (fill.rect.x + fill.minimum - spec.travel.0 - rect.w / 2.).abs() > 0.001
            {
                return Ok(());
            }
            Rect::new(
                spec.travel.0,
                rect.y,
                spec.travel.1 - spec.travel.0 + rect.w,
                rect.h,
            )
            .union(Rect::new(
                fill.rect.x,
                fill.rect.y,
                fill.minimum + spec.travel.1 - spec.travel.0,
                fill.rect.h,
            ))
        } else {
            Rect::new(0., 0., rect.w, rect.h)
        };
        let swept = if spec.fill.is_some() {
            bounds
        } else {
            Rect::new(
                spec.travel.0,
                rect.y,
                spec.travel.1 - spec.travel.0 + rect.w,
                rect.h,
            )
        };
        if ![swept.x, swept.y, swept.w, swept.h]
            .iter()
            .all(|n| n.is_finite())
            || swept.w <= 0.
            || swept.h <= 0.
            || !overlay_safe(original, &spec, swept)
        {
            return Ok(());
        }
        let scale = self.surfaces[index].scale120 as f32 / 120.;
        let width = (bounds.w.ceil() * scale).ceil() as u32;
        let height = (bounds.h.ceil() * scale).ceil() as u32;
        let Some(bytes) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|area| area.checked_mul(4))
            .filter(|bytes| *bytes > 0 && *bytes <= 64 * 1024)
        else {
            return Ok(());
        };
        let mut pool = SlotPool::new((bytes * 2).max(4096), &self.shm)?;
        let (buffer, pixels) = pool.create_buffer(
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
        )?;
        let target = pose(&spec, x);
        paint_child(
            &mut ShmTarget {
                bytes: pixels,
                width,
                height,
                scale,
                format: PixelFormat::Bgra,
            },
            &spec,
            &draw,
            bounds,
            target,
            original.opacity,
        )?;
        let subcompositor = self.subcompositor.as_ref().ok_or("missing subcompositor")?;
        let viewporter = self.viewporter.as_ref().ok_or("missing viewporter")?;
        let child = self.compositor.create_surface(&self.qh);
        let role =
            subcompositor.get_subsurface(&child, self.surfaces[index].wl_surface(), &self.qh, ());
        let viewport = viewporter.get_viewport(&child, &self.qh, ());
        viewport.set_destination(bounds.w.ceil() as i32, bounds.h.ceil() as i32);
        let empty = Region::new(&self.compositor)?;
        child.set_input_region(Some(empty.wl_region()));
        role.set_sync();
        let target = pose(&spec, x);
        let release_target = spec.release_serial;
        self.surfaces[index].pointer_visual = Some(Visual {
            spec,
            draw,
            surface: child,
            role,
            viewport,
            buffers: vec![buffer],
            pool,
            bounds,
            opacity: original.opacity,
            release_target,
            x: target,
            dirty: true,
            commit_queued: false,
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
        let arm = !visual.timer && !visual.spec.local_feedback;
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
            v.pending.take().map(|(x, y)| {
                if v.spec.local_feedback {
                    UiEvent::LocalDrag { x, y }
                } else {
                    UiEvent::Drag { x, y }
                }
            })
        });
        if let Some(event) = pending {
            self.view_event(true, event);
        }
    }
    pub(super) fn pointer_visual_release(&mut self, index: usize) {
        // A release is an ordering barrier, not another replaceable motion.
        self.pointer_visual_commit_now(index);
        self.pointer_visual_flush(index);
        if let Some(visual) = &mut self.surfaces[index].pointer_visual {
            if !visual.released {
                visual.release_target = visual.release_target.wrapping_add(1);
            }
            visual.released = true;
            visual.token = monotonic_ns();
        }
    }
    pub(super) fn pointer_visual_position(&mut self, index: usize) {
        let offset = self.surfaces[index].offset;
        if let Some(visual) = &mut self.surfaces[index].pointer_visual {
            let rect = rect(&visual.draw);
            let (x, y) = if visual.spec.fill.is_some() {
                (visual.bounds.x, visual.bounds.y)
            } else {
                (visual.x, rect.y)
            };
            visual
                .role
                .set_position((x + offset.0).round() as i32, (y + offset.1).round() as i32);
            if visual.spec.fill.is_none() {
                visual.dirty = false;
            }
        }
    }
    /// Submit the newest pose after this ready-fd dispatch has drained, before sleep.
    pub(super) fn pointer_visual_commit(&mut self, index: usize) {
        let surface = &mut self.surfaces[index];
        if surface.callbacks.pending || surface.redraw {
            return;
        }
        let Some(visual) = &mut surface.pointer_visual else {
            return;
        };
        if !visual.dirty || !visual.mapped || visual.retiring || visual.commit_queued {
            return;
        }
        visual.commit_queued = true;
        let parent = surface.wl_surface().clone();
        let epoch = self.panel_epoch;
        self.loop_handle.insert_idle(move |app| {
            app.with_epoch(epoch, |app| {
                if let Some(index) = app.surfaces.iter().position(|s| s.wl_surface() == &parent) {
                    if let Some(visual) = &mut app.surfaces[index].pointer_visual {
                        visual.commit_queued = false;
                    }
                    app.pointer_visual_commit_now(index);
                }
            });
        });
    }
    fn pointer_visual_commit_now(&mut self, index: usize) {
        let surface = &self.surfaces[index];
        if surface.callbacks.pending
            || surface.redraw
            || !surface
                .pointer_visual
                .as_ref()
                .is_some_and(|v| v.dirty && v.mapped && !v.retiring)
        {
            return;
        }
        self.pointer_visual_position(index);
        if let Some(visual) = &mut self.surfaces[index].pointer_visual
            && let Err(error) = visual.stage_commit()
        {
            self.fail(error);
            return;
        }
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
            s.pointer_visual
                .as_ref()
                .unwrap()
                .feedback(presentation, &parent, &self.qh);
        }
        bead_core::profile::mark("pointer_visual_commit");
        parent.commit();
        if self.surfaces[index]
            .pointer_visual
            .as_ref()
            .is_some_and(|v| v.spec.local_feedback && !v.dirty)
        {
            self.pointer_visual_flush(index);
        }
    }
    /// A scratch scene leaves the source transaction and worker rasters intact.
    pub(super) fn pointer_visual_scene(&mut self, index: usize) -> Option<Scene> {
        let id = self.surfaces[index]
            .pointer_visual
            .as_ref()?
            .spec
            .id
            .clone();
        let live = self
            .panel_runtime
            .as_ref()
            .and_then(|source| source.drag_visuals().into_iter().find(|v| v.id == id));
        let s = &mut self.surfaces[index];
        let visual = s.pointer_visual.as_mut()?;
        let mut scene = self.companion.as_ref()?.0.clone();
        let layer = scene.layers.iter().find(|layer| layer.id == visual.spec.id);
        visual.retiring |= !self.panel_target
            || self.animations.active(self.started.elapsed().as_secs_f64())
            || visual.scale120 != s.scale120
            || visual.generation != scene.resource_revision
            || layer.is_none()
            || live.as_ref().is_none_or(|v| {
                v.grab != visual.spec.grab || !fill_skin(&v.fill, &visual.spec.fill)
            })
            || !overlay_safe(
                &scene,
                &visual.spec,
                if visual.spec.fill.is_some() {
                    visual.bounds
                } else {
                    let r = rect(&visual.draw);
                    Rect::new(
                        visual.spec.travel.0,
                        r.y,
                        visual.spec.travel.1 - visual.spec.travel.0 + r.w,
                        r.h,
                    )
                },
            )
            || !painted_layer(&scene, &visual.spec.id)
                .as_ref()
                .is_some_and(|draw| same_skin(draw, &visual.draw))
            || (visual.released
                && live
                    .as_ref()
                    .is_some_and(|v| v.release_serial == visual.release_target));
        if visual.retiring {
            // Preserve the final value across a moved stable control; never replay
            // old coordinates into a view that removed that control.
            if let Some((x, _)) = visual.pending {
                visual.pending = live.as_ref().map(|next| {
                    let fraction = ((x - visual.spec.pointer.0)
                        / (visual.spec.pointer.1 - visual.spec.pointer.0))
                        .clamp(0., 1.);
                    (
                        next.pointer.0 + fraction * (next.pointer.1 - next.pointer.0),
                        next.grab.y + next.grab.h / 2.,
                    )
                });
            }
            // Destruction itself is immediate in Wayland. Unmap synchronously first,
            // restore the parent's copy, then destroy only after that parent commits.
            return None;
        }
        let layer = layer.unwrap();
        let removed = layer.own.clone();
        let fill = visual
            .spec
            .fill
            .as_ref()
            .and_then(|fill| scene.layers.iter().find(|l| l.id == fill.id))
            .map(|l| l.own.start..l.subtree_end);
        let mut index = 0;
        scene.retain_primitives(|_| {
            let keep =
                !removed.contains(&index) && !fill.as_ref().is_some_and(|r| r.contains(&index));
            index += 1;
            keep
        });
        scene.prepared.clear();
        Some(scene)
    }
    pub(super) fn pointer_visual_retire(&mut self, index: usize) -> Vec<UiEvent> {
        if !self.surfaces[index]
            .pointer_visual
            .as_ref()
            .is_some_and(|v| v.retiring)
        {
            return Vec::new();
        }
        let visual = self.surfaces[index].pointer_visual.take().unwrap();
        let mut events = Vec::with_capacity(2);
        if let Some((x, y)) = visual.pending {
            events.push(if visual.spec.local_feedback {
                UiEvent::LocalDrag { x, y }
            } else {
                UiEvent::Drag { x, y }
            });
        }
        if !visual.released {
            self.pointer_pressed = false;
            events.push(UiEvent::Release);
        }
        events
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
            fill: None,
            local_feedback: false,
            release_serial: 0,
        };
        assert_eq!(pose(&spec, -200.), 6.);
        assert_eq!(pose(&spec, 200.), 86.);
        assert_eq!(pose(&spec, 42.25), 38.25);
        assert_eq!(pose(&spec, 10.), 6.);
    }

    #[test]
    fn every_progress_frame_has_one_value_and_no_fill_handle_gap() {
        let bounds = Rect::new(0., 0., 108., 16.);
        let draw = bead_render::Primitive::Rounded {
            rect: Rect::new(0., 0., 8., 16.),
            radius: 3.,
            fill: [40, 220, 120, 255],
            border: None,
        };
        let spec = DragVisual {
            id: "slider.thumb".into(),
            grab: bounds,
            travel: (0., 100.),
            pointer: (4., 104.),
            release_serial: 0,
            local_feedback: false,
            fill: Some(crate::DragFill {
                id: "slider.fill".into(),
                rect: Rect::new(0., 5., 4., 6.),
                minimum: 4.,
                radius: 3.,
                gradient_min: 16.,
                from: [220, 40, 40, 255],
                to: [40, 40, 220, 255],
            }),
        };
        for scale in [1., 1.5] {
            let width = (108. * scale) as u32;
            let height = (16. * scale) as u32;
            let mut pixels = vec![0; width as usize * height as usize * 4];
            for step in 0..=100 {
                let x = step as f32;
                paint_child(
                    &mut ShmTarget {
                        bytes: &mut pixels,
                        width,
                        height,
                        scale,
                        format: PixelFormat::Bgra,
                    },
                    &spec,
                    &draw,
                    bounds,
                    x,
                    1.,
                )
                .unwrap();
                let y = (8. * scale) as usize;
                let alpha = |px: usize| pixels[(y * width as usize + px) * 4 + 3];
                for px in (2. * scale).ceil() as usize..((x + 6.) * scale).floor() as usize {
                    assert!(
                        alpha(px) > 0,
                        "transparent gap at step={step}, scale={scale}, pixel={px}"
                    );
                }
                for px in ((x + 8.) * scale).ceil() as usize + 1..width as usize {
                    assert_eq!(alpha(px), 0, "stale paint beyond handle");
                }
            }
        }
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
        let spec = DragVisual {
            id: "thumb".into(),
            grab: bounds,
            travel: (92., 192.),
            pointer: (92., 192.),
            fill: None,
            local_feedback: false,
            release_serial: 0,
        };
        let mut pixels = vec![0; 12 * 24 * 4];
        paint_child(
            &mut ShmTarget {
                bytes: &mut pixels,
                width: 12,
                height: 24,
                scale: 1.5,
                format: PixelFormat::Bgra,
            },
            &spec,
            &draw,
            Rect::new(0., 0., 8., 16.),
            92.,
            1.,
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

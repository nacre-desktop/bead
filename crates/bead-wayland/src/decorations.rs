// SPDX-License-Identifier: MIT
//! Static, bounded vector tiles; no timer, GL context or full-output bitmap.
use super::*;
struct Child {
    surface: wl_surface::WlSurface,
    role: wl_subsurface::WlSubsurface,
    viewport: WpViewport,
    _buffer: Buffer,
}
impl Drop for Child {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.role.destroy();
        self.surface.destroy();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct PanelKey {
    origin: (i32, i32),
    bounds: (u32, u32),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    viewport: (u32, u32),
    scale120: u32,
    engaged: bool,
    panel: Option<PanelKey>,
    variant: u64,
}
/// B0117: decoration tiles already rasterised, per key (idle frame, joined frame of a panel), newest last. Opening
/// and closing a panel swaps tile sets; rasterising them took 10-20 ms on the main thread right at the animation
/// start. A set is at most 2 MiB (~0.5 MiB for a 1920x1200 frame at 1.5); B0205 keeps six (the idle frame and
/// five panels), since four thrashed when control centre and a status popout alternated.
type Raster = (Rect, u32, u32, std::sync::Arc<[u8]>);
/// One missing tile set to raster: its key, specs, pixel sizes and scale.
type Work = (Key, Vec<Decoration>, Vec<(u32, u32)>, f32);
/// A background job's answer: its generation and every set it rastered.
type Reply = (u64, Result<Vec<(Key, Vec<Raster>)>, String>);
/// The background state (B0387): the newest palette wins and one job runs at a time. Each
/// background sync is a generation; its misses replace the pending request, which starts only
/// when no job is in flight, and an answer older than the newest sync is dropped.
#[derive(Default)]
struct Jobs {
    generation: u64,
    flight: Option<u64>,
    pending: Vec<Work>,
}
impl Jobs {
    fn begin(&mut self) {
        self.generation += 1;
        self.pending.clear();
    }
    fn request(&mut self, work: Work) {
        if !self.pending.iter().any(|(key, ..)| *key == work.0) {
            self.pending.push(work);
        }
    }
    /// The pending request as a job, unless one is in flight.
    fn start(&mut self) -> Option<(u64, Vec<Work>)> {
        if self.flight.is_some() || self.pending.is_empty() {
            return None;
        }
        self.flight = Some(self.generation);
        Some((self.generation, std::mem::take(&mut self.pending)))
    }
    /// The job answered; whether its answer is still the newest sync's.
    fn finish(&mut self, generation: u64) -> bool {
        self.flight = None;
        generation == self.generation
    }
}
/// The sets, the background state and its channel (made at the first background miss).
#[derive(Default)]
pub(super) struct RasterCache {
    sets: Vec<(Key, Vec<Raster>)>,
    queue: Jobs,
    sender: Option<calloop::channel::Sender<Reply>>,
}
const RASTER_SETS: usize = 6;
impl RasterCache {
    /// Keep a set (a key already cached keeps its own), newest last, at most `RASTER_SETS`.
    fn insert(&mut self, key: Key, set: Vec<Raster>) {
        if self.sets.iter().any(|(k, _)| *k == key) {
            return;
        }
        self.sets.push((key, set));
        if self.sets.len() > RASTER_SETS {
            self.sets.remove(0);
        }
    }
}
/// `posed`: the progress the scene was last drawn at (1 in a scene not posed yet); `clock`: the
/// reveal's, held at its first pose until a frame showing that pose is committed.
fn reveal_drawn(posed: f32, clock: Option<(f32, bool)>) -> bool {
    posed > 0. && clock.is_some_and(|(progress, _)| progress > 0.)
}
/// Raster every tile into owned pixels; also runs on the background thread.
fn raster_tiles(
    specs: &[Decoration],
    sizes: &[(u32, u32)],
    scale: f32,
) -> Result<Vec<Raster>, String> {
    specs
        .iter()
        .zip(sizes)
        .map(|(spec, &(w, h))| {
            let mut bytes = vec![0; w as usize * h as usize * 4];
            bead_render::svg_into(
                &spec.svg,
                ShmTarget {
                    bytes: &mut bytes,
                    width: w,
                    height: h,
                    scale,
                    format: PixelFormat::Bgra,
                },
            )
            .map_err(|e| e.to_string())?;
            Ok((spec.rect, w, h, std::sync::Arc::from(bytes)))
        })
        .collect()
}
pub(super) struct Tiles {
    key: Key,
    pub bytes: usize,
    _children: Vec<Child>,
    _pool: SlotPool,
}
impl App {
    pub(super) fn sync_all_decorations(&mut self) -> Result<(), String> {
        self.sync_all_decorations_in(false)
    }
    /// `background`: a palette-driven sync rasters a missing set off the main thread (B0387); the
    /// current tiles stay until it arrives.
    pub(super) fn sync_all_decorations_in(&mut self, background: bool) -> Result<(), String> {
        let _busy = bead_core::profile::Busy::new("decorations");
        if background {
            self.decoration_cache.queue.begin();
        }
        let synced =
            (0..self.surfaces.len()).try_for_each(|i| self.sync_decorations_in(i, background));
        let started = if background {
            self.start_decoration_job()
        } else {
            Ok(())
        };
        synced.and(started)
    }
    pub(super) fn sync_decorations(&mut self, index: usize) -> Result<(), String> {
        self.sync_decorations_in(index, false)
    }
    fn decoration_raster_done(
        &mut self,
        generation: u64,
        sets: Result<Vec<(Key, Vec<Raster>)>, String>,
    ) {
        let current = self.decoration_cache.queue.finish(generation);
        match sets {
            // An older answer is dropped: a newer sync wants other sets.
            Ok(sets) if current => {
                for (key, set) in sets {
                    self.decoration_cache.insert(key, set);
                }
                // The sync installs the matching set and starts any pending job.
                if let Err(error) = self.sync_all_decorations_in(true) {
                    eprintln!("DECORATION_ERROR {error}");
                }
                return;
            }
            Err(error) => eprintln!("DECORATION_ERROR {error}"),
            Ok(_) => {}
        }
        if let Err(error) = self.start_decoration_job() {
            eprintln!("DECORATION_ERROR {error}");
        }
    }
    fn sync_decorations_in(&mut self, index: usize, background: bool) -> Result<(), String> {
        // A pearl is not part of the frame; the docked panel's tiles stay as they are.
        if self.pearl.is_some() {
            return Ok(());
        }
        let surface = &self.surfaces[index];
        if surface.companion || !surface.configured {
            return Ok(());
        }
        let Some(factory) = self.options.decorations.clone() else {
            return Ok(());
        };
        let Some((width, height)) = self
            .output
            .info(&surface.output)
            .and_then(|o| o.logical_size)
            .filter(|(w, h)| *w > 0 && *h > 0)
        else {
            return Ok(());
        };
        let on_output = self.panel_output.as_ref() == Some(&surface.output);
        // B0205: a panel shows nothing until its content (or a ready fallback) arrives, so the
        // frame keeps its current tiles instead of rasterising a join for the placeholder's shape
        // that is replaced a moment later (one extra set per open, ~30 ms on battery).
        if self.panel_target && on_output && !self.panel_ready && !self.panel_fallback_ready {
            return Ok(());
        }
        let joined = self.panel_target && on_output && self.panel_reveal_drawn();
        let engaged = joined
            && self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|s| s.engages_decorations);
        let panel = if joined {
            self.companion
                .as_ref()
                .zip(self.options.panel_spec.as_ref())
                .and_then(|((scene, _, _), spec)| {
                    spec.position.as_ref().map(|position| DecorationPanel {
                        scene,
                        origin: position(scene, self.panel_center, (width, height)),
                    })
                })
        } else {
            None
        };
        let shape = panel.map(|p| PanelKey {
            origin: p.origin,
            bounds: (p.scene.bounds.w.to_bits(), p.scene.bounds.h.to_bits()),
        });
        let key = Key {
            viewport: (width as u32, height as u32),
            scale120: surface.scale120,
            engaged,
            panel: shape,
            variant: self.decoration_variant,
        };
        if surface.decorations.as_ref().is_some_and(|d| d.key == key) {
            return Ok(());
        }
        let _sync_profile = bead_core::profile::Span::new("decorations_sync");
        let subcompositor = self
            .subcompositor
            .as_ref()
            .ok_or("Static decorations need wl_subcompositor")?;
        let viewporter = self
            .viewporter
            .as_ref()
            .ok_or("Static decorations need wp_viewporter")?;
        let cached = self
            .decoration_cache
            .sets
            .iter()
            .position(|(k, _)| *k == key)
            .map(|i| self.decoration_cache.sets.remove(i).1);
        let specs: Vec<Decoration> = match &cached {
            Some(rasters) => rasters
                .iter()
                .map(|(rect, _, _, _)| Decoration {
                    rect: *rect,
                    svg: std::sync::Arc::from(&[][..]),
                })
                .collect(),
            None => factory((width as u32, height as u32), engaged, panel)?,
        };
        if specs.len() > 16 {
            return Err("Too many static decorations".into());
        }
        let scale = key.scale120 as f32 / 120.;
        let mut sizes = Vec::new();
        let mut total = 0usize;
        for spec in &specs {
            if ![spec.rect.x, spec.rect.y, spec.rect.w, spec.rect.h]
                .iter()
                .all(|n| n.is_finite())
                || spec.rect.w <= 0.
                || spec.rect.h <= 0.
                || spec.svg.len() > 1_048_576
            {
                return Err("Invalid static decoration".into());
            }
            let w = (spec.rect.w * scale).ceil() as u32;
            let h = (spec.rect.h * scale).ceil() as u32;
            let bytes = (w as usize)
                .checked_mul(h as usize)
                .and_then(|n| n.checked_mul(4))
                .ok_or("Static decoration overflow")?;
            // SlotPool rounds each slot to64 bytes. Reserve that amount now;
            // a tiny final-slot shortfall otherwise doubles the entire pool.
            let reserved = bytes
                .checked_add(63)
                .map(|n| n & !63)
                .ok_or("Static decoration overflow")?;
            total = total
                .checked_add(reserved)
                .ok_or("Static decoration overflow")?;
            if total > 2 * 1024 * 1024 {
                return Err("Static decoration SHM budget exceeded".into());
            }
            sizes.push((w, h));
        }
        let rasters = match cached {
            Some(rasters) => rasters,
            None if background => {
                self.decoration_cache
                    .queue
                    .request((key, specs, sizes, scale));
                return Ok(());
            }
            None => raster_tiles(&specs, &sizes, scale)?,
        };
        let mut pool = SlotPool::new(total.max(4096), &self.shm).map_err(|e| e.to_string())?;
        let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
        let parent = surface.wl_surface();
        let mut children = Vec::new();
        for (spec, (w, h)) in specs.into_iter().zip(sizes) {
            let child = self.compositor.create_surface(&self.qh);
            let role = subcompositor.get_subsurface(&child, parent, &self.qh, ());
            let viewport = viewporter.get_viewport(&child, &self.qh, ());
            let (buffer, bytes) = pool
                .create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888)
                .map_err(|e| e.to_string())?;
            let (_, rw, rh, pixels) = &rasters[children.len()];
            if (*rw, *rh) != (w, h) {
                return Err("Static decoration size mismatch".into());
            }
            bytes.copy_from_slice(pixels);
            role.set_position(spec.rect.x.round() as i32, spec.rect.y.round() as i32);
            role.set_desync();
            // The half-stroke inside the bar must remain visible too. Tiles
            // never cover controls and have an empty input region.
            role.place_above(parent);
            viewport.set_destination(spec.rect.w.round() as i32, spec.rect.h.round() as i32);
            child.set_input_region(Some(empty.wl_region()));
            buffer.attach_to(&child).map_err(|e| e.to_string())?;
            child.damage_buffer(0, 0, w as i32, h as i32);
            child.commit();
            children.push(Child {
                surface: child,
                role,
                viewport,
                _buffer: buffer,
            });
        }
        self.decoration_cache.insert(key, rasters);
        self.surfaces[index].decorations = Some(Tiles {
            key,
            bytes: total,
            _children: children,
            _pool: pool,
        });
        self.surfaces[index].commit();
        Ok(())
    }
    /// B0421: whether the companion has drawn part of its body. A reveal's first pose shows
    /// nothing, so a join made then shows the rim opening and the panel edge in the strips
    /// before the slide. A compositor-drawn reveal (opt-in) never poses the scene: it joins at once.
    fn panel_reveal_drawn(&self) -> bool {
        let Some((scene, _, _)) = &self.companion else {
            return true;
        };
        let now = self.started.elapsed().as_secs_f64();
        crate::rendering::reveal_contour_capable(scene, self.layer_motion_manager.as_ref())
            || scene.motion.as_ref().is_some_and(|m| {
                // The companion's draw swaps the panel's clock into `animations`.
                let clock = self.animations.value(&m.id, now);
                reveal_drawn(
                    m.progress,
                    clock.or_else(|| self.panel_animations.value(&m.id, now)),
                )
            })
    }
    /// Start the pending request on one thread; its answer comes back through the calloop channel.
    fn start_decoration_job(&mut self) -> Result<(), String> {
        let Some((generation, work)) = self.decoration_cache.queue.start() else {
            return Ok(());
        };
        let sender = match self.decoration_cache.sender.clone() {
            Some(sender) => sender,
            None => {
                let (sender, receiver) = calloop::channel::channel::<Reply>();
                let source = self.loop_handle.insert_source(receiver, |event, _, app| {
                    if let calloop::channel::Event::Msg((generation, sets)) = event {
                        app.decoration_raster_done(generation, sets);
                    }
                });
                if let Err(error) = source {
                    self.decoration_cache.queue.finish(generation);
                    return Err(error.to_string());
                }
                self.decoration_cache.sender = Some(sender.clone());
                sender
            }
        };
        std::thread::spawn(move || {
            // A panic still answers, so the job never stays in flight.
            let sets = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work.into_iter()
                    .map(|(key, specs, sizes, scale)| {
                        Ok((key, raster_tiles(&specs, &sizes, scale)?))
                    })
                    .collect()
            }))
            .unwrap_or_else(|_| Err("Decoration raster panicked".into()));
            let _ = sender.send((generation, sets));
        });
        Ok(())
    }
}
delegate_noop!(App: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(App: ignore wl_subsurface::WlSubsurface);
#[cfg(test)]
mod tests {
    use super::*;
    fn key(variant: u64) -> Key {
        Key {
            viewport: (100, 50),
            scale120: 120,
            engaged: false,
            panel: None,
            variant,
        }
    }
    /// Two tiles in one palette colour.
    fn specs(fill: &str) -> (Vec<Decoration>, Vec<(u32, u32)>) {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4"><rect width="5" height="4" fill="{fill}"/></svg>"#
        );
        let tile = |x: f32, w: f32| Decoration {
            rect: Rect { x, y: 0., w, h: 4. },
            svg: std::sync::Arc::from(svg.as_bytes()),
        };
        (vec![tile(0., 8.), tile(8., 4.)], vec![(8, 4), (4, 4)])
    }
    #[test]
    fn raster_tiles_on_a_thread_equals_inline() {
        let (specs, sizes) = specs("#336699");
        let inline = raster_tiles(&specs, &sizes, 1.).unwrap();
        assert!(inline[0].3.iter().any(|b| *b != 0));
        let thread = std::thread::spawn(move || raster_tiles(&specs, &sizes, 1.).unwrap())
            .join()
            .unwrap();
        assert_eq!(inline, thread);
    }
    #[test]
    fn the_newest_palette_wins_with_one_job_at_a_time() {
        let fill = |variant| if variant == 1 { "#336699" } else { "#993366" };
        let work = |variant| {
            let (specs, sizes) = specs(fill(variant));
            (key(variant), specs, sizes, 1.)
        };
        let raster = |variant| {
            let (specs, sizes) = specs(fill(variant));
            raster_tiles(&specs, &sizes, 1.).unwrap()
        };
        let mut cache = RasterCache::default();
        cache.queue.begin();
        cache.queue.request(work(1));
        let (first, _) = cache.queue.start().unwrap();
        // Palette 2 arrives while job 1 runs: it waits, and asking twice keeps one request.
        cache.queue.begin();
        cache.queue.request(work(2));
        cache.queue.request(work(2));
        assert!(cache.queue.start().is_none(), "one job at a time");
        // Job 1 answers late: dropped, then exactly one job starts for palette 2.
        assert!(!cache.queue.finish(first));
        let (second, jobs) = cache.queue.start().unwrap();
        assert!(cache.queue.start().is_none());
        assert_eq!(jobs.len(), 1);
        let (key2, specs2, sizes2, scale) = jobs.into_iter().next().unwrap();
        let rasters = raster_tiles(&specs2, &sizes2, scale).unwrap();
        assert!(cache.queue.finish(second));
        cache.insert(key2, rasters);
        // The one cached set is palette 2's, pixel for pixel, not palette 1's.
        assert!(cache.sets.len() == 1 && cache.sets[0].0 == key(2));
        assert_eq!(cache.sets[0].1, raster(2));
        assert_ne!(cache.sets[0].1, raster(1));
    }
    /// B0421: the presenter's open sequence, from the request to rest, then a reopen mid-close.
    /// `joins` is the frame's join decision (`panel_reveal_drawn` while the panel is targeted).
    #[test]
    fn the_frame_joins_a_revealing_panel_once_its_reveal_has_drawn() {
        use bead_core::animation::{Animations, Curve};
        let body = Rect {
            x: 32.,
            y: 0.,
            w: 200.,
            h: 120.,
        };
        let mut scene = Scene::empty(264., 152.);
        scene.primitives.push(bead_render::Primitive::Rounded {
            rect: body,
            radius: 8.,
            fill: [40, 40, 40, 255],
            border: None,
            radial: None,
        });
        // A fresh scene says progress 1 until the presenter poses it.
        scene.motion = Some(bead_render::Motion {
            id: "panel".into(),
            open: true,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            auto_open: false,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            scale_from: None,
            fade: false,
            reveal: true,
            progress: 1.,
            parts: Vec::new(),
            art: None,
            body: Some(body),
        });
        let mut clock = Animations::default();
        let joins = |target: bool, scene: &Scene, clock: &Animations, now| {
            target
                && reveal_drawn(
                    scene.motion.as_ref().unwrap().progress,
                    clock.value("panel", now),
                )
        };
        let pose = |scene: &mut Scene, clock: &Animations, now, open| {
            let (progress, _) = clock.value("panel", now).unwrap();
            scene.panel_offset(progress, open);
            progress
        };
        // The open request: the reveal holds its closed first pose.
        clock.open_to("panel", true, 0., 0.2, Curve::EaseOut);
        assert!(!joins(true, &scene, &clock, 0.), "fresh scene, held pose");
        // The first frame shows nothing; its commit starts the clock.
        assert_eq!(pose(&mut scene, &clock, 0., true), 0.);
        clock.committed("panel", 0., 0.01);
        // A bar redraw before the next panel frame: the clock moved, nothing is drawn yet.
        assert!(
            !joins(true, &scene, &clock, 0.02),
            "before the first revealed frame"
        );
        // The first revealed frame draws part of the body: the frame joins it from then on.
        assert!(pose(&mut scene, &clock, 0.03, true) > 0.);
        assert!(
            joins(true, &scene, &clock, 0.04),
            "after the first revealed frame"
        );
        assert_eq!(pose(&mut scene, &clock, 1., true), 1.);
        assert!(joins(true, &scene, &clock, 1.), "at rest");
        // The close drops the join with the target, while the panel's own art still covers the
        // strips (it shows nothing below a pixel of body, where the frame is plain again).
        clock.open_to("panel", false, 2., 0.2, Curve::EmphasizedAccelerate);
        assert!(!joins(false, &scene, &clock, 2.), "closing");
        pose(&mut scene, &clock, 2., false);
        clock.committed("panel", 2., 2.);
        assert!(pose(&mut scene, &clock, 2.1, false) > 0.);
        // Reopened mid-close: the body is on screen, so the join comes back at once.
        clock.open_to("panel", true, 2.1, 0.2, Curve::EaseOut);
        assert!(joins(true, &scene, &clock, 2.1), "reopened mid-close");
    }
    #[test]
    fn the_cache_keeps_its_own_set_and_six_sets() {
        let mut cache = RasterCache::default();
        cache.insert(key(1), Vec::new());
        cache.insert(
            key(1),
            vec![(
                Rect {
                    x: 0.,
                    y: 0.,
                    w: 0.,
                    h: 0.,
                },
                0,
                0,
                std::sync::Arc::from(&[][..]),
            )],
        );
        assert!(cache.sets[0].1.is_empty());
        for variant in 10..20 {
            cache.insert(key(variant), Vec::new());
        }
        assert_eq!(cache.sets.len(), RASTER_SETS);
    }
}

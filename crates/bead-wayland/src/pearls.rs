// SPDX-License-Identifier: MIT OR Apache-2.0
//! B0048: detach any panel session into a floating surface (a pearl) and dock it back.
//!
//! The resident panel is one group of state (worker, scene, renderer, surfaces).
//! Detaching moves that group, unchanged, into a slot; a slot is processed by
//! swapping it with the resident group around the existing panel code, the way
//! output sessions already are. Policy (which views, which edge, insets, look) is
//! supplied by the application through [`PearlOptions`]; nothing here names a view.
use super::*;
use bead_core::pearl::{self, Drag, Insets, Monitor, Outcome, Point, Source, Tuning};

/// Reserved edge space of an output (bar, frame) by output name and logical size.
pub type FrameInsets = std::sync::Arc<dyn Fn(&str, (i32, i32)) -> Insets + Send + Sync>;

/// What the application decides about pearls.
#[derive(Clone)]
pub struct PearlOptions {
    pub tuning: Tuning,
    /// Reserved edge space of an output (bar, frame) by output name and logical size.
    pub frame: FrameInsets,
    /// The configured view that draws drag guides (see [`Guide`]); none: no guides.
    pub guide_view: Option<String>,
    /// Where the layout is kept between runs; none: pearls are not remembered.
    pub store: Option<std::path::PathBuf>,
    /// At most this many pearls at once.
    pub max: usize,
    /// Private-compositor proofs only: any left press starts the pull-out gesture,
    /// because a virtual pointer cannot hold Super.
    pub test_any_press: bool,
}

/// One frame of drag guidance for one output, in that output's logical pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    pub edge: bead_core::Edge,
    pub output: (i32, i32),
    /// Insets of the output along its edges.
    pub frame: [f64; 4],
    pub landing: Option<(f64, f64)>,
    pub film: Option<Film>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Film {
    pub origin: (f64, f64),
    pub tip: (f64, f64),
    pub armed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Pearl,
    Guide,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Info {
    pub id: u32,
    pub kind: Kind,
    /// The visible body in global logical coordinates (pearls).
    pub rect: pearl::Rect,
}

/// The resident-panel state, as one movable unit.
pub(crate) struct Group {
    runtime: Option<Box<dyn SceneSource>>,
    epoch: u64,
    name: Option<String>,
    center: Option<f32>,
    close_epoch: u64,
    reload_pending: bool,
    target: bool,
    output: Option<wl_output::WlOutput>,
    companion: Option<(Scene, Box<dyn Renderer>, RendererKind)>,
    animations: bead_core::animation::Animations,
    policy: policy::Policy,
    dismiss: Option<panel::Dismiss>,
    ui_serial: u64,
    spec: Option<PanelSpec>,
    option_companion: Option<(Scene, RendererKind)>,
    lazy_gl: bool,
    release_timer: bool,
    pub(crate) info: Option<Info>,
    surfaces: Vec<Surface>,
}
impl Group {
    fn empty(epoch: u64, name: Option<String>, spec: Option<PanelSpec>) -> Self {
        Self {
            runtime: None,
            epoch,
            name,
            center: None,
            close_epoch: 0,
            reload_pending: false,
            target: false,
            output: None,
            companion: None,
            animations: Default::default(),
            policy: Default::default(),
            dismiss: None,
            ui_serial: 0,
            spec,
            option_companion: None,
            lazy_gl: false,
            release_timer: false,
            info: None,
            surfaces: Vec::new(),
        }
    }
    pub(crate) fn id(&self) -> Option<u32> {
        self.info.map(|i| i.id)
    }
    pub(crate) fn owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.surfaces
            .iter()
            .any(|s| s.layer.wl_surface() == surface)
    }
    pub(crate) fn alive(&self) -> bool {
        self.runtime.is_some() && !self.surfaces.is_empty()
    }
}

/// A press that may become a drag, and the drag it became.
pub(crate) struct Gesture {
    surface: wl_surface::WlSurface,
    drag: Drag,
    /// The pearl the docked panel turned into, once it left the threshold.
    lifted: Option<u32>,
    last_move: Instant,
}

impl App {
    pub(super) fn next_epoch(&mut self) -> u64 {
        self.epoch_seq += 1;
        self.epoch_seq
    }
    fn swap_group(&mut self, g: &mut Group) {
        use std::mem::swap;
        swap(&mut self.panel_runtime, &mut g.runtime);
        swap(&mut self.panel_epoch, &mut g.epoch);
        swap(&mut self.panel_name, &mut g.name);
        swap(&mut self.panel_center, &mut g.center);
        swap(&mut self.panel_close_epoch, &mut g.close_epoch);
        swap(&mut self.panel_reload_pending, &mut g.reload_pending);
        swap(&mut self.panel_target, &mut g.target);
        swap(&mut self.panel_output, &mut g.output);
        swap(&mut self.companion, &mut g.companion);
        swap(&mut self.panel_animations, &mut g.animations);
        swap(&mut self.companion_policy, &mut g.policy);
        swap(&mut self.dismiss, &mut g.dismiss);
        swap(&mut self.panel_ui_serial, &mut g.ui_serial);
        swap(&mut self.options.panel_spec, &mut g.spec);
        swap(&mut self.options.companion, &mut g.option_companion);
        swap(&mut self.options.lazy_gl, &mut g.lazy_gl);
        swap(&mut self.release_timer, &mut g.release_timer);
        swap(&mut self.pearl, &mut g.info);
        let mine: Vec<Surface> = {
            let mut kept = Vec::new();
            let mut out = Vec::new();
            for s in self.surfaces.drain(..) {
                if s.companion {
                    out.push(s)
                } else {
                    kept.push(s)
                }
            }
            self.surfaces = kept;
            out
        };
        self.surfaces.append(&mut g.surfaces);
        g.surfaces = mine;
    }
    fn blank_group(&self) -> Group {
        let (name, spec) = match self
            .options
            .panels
            .as_ref()
            .and_then(|p| p.resolve(None).ok())
        {
            Some((name, spec)) => (Some(name.to_owned()), Some(spec.clone())),
            None => (self.panel_name.clone(), self.options.panel_spec.clone()),
        };
        let mut spec = spec;
        if let Some(spec) = &mut spec {
            spec.initial_open = false;
        }
        Group::empty(self.epoch_seq, name, spec)
    }
    /// Run `f` with a slot's panel in place of the resident one. A slot whose panel
    /// was released, or whose surfaces are gone, is dropped afterwards.
    pub(super) fn in_slot<R>(&mut self, id: u32, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        if self.pearl.is_some_and(|i| i.id == id) {
            return Some(f(self));
        }
        let at = self.slots.iter().position(|g| g.id() == Some(id))?;
        let mut g = self.slots.remove(at);
        self.swap_group(&mut g);
        let result = f(self);
        self.swap_group(&mut g);
        if g.alive() {
            self.slots.insert(at.min(self.slots.len()), g);
        } else {
            let was_pearl = g.info.is_some_and(|i| i.kind == Kind::Pearl);
            drop(g);
            if was_pearl {
                self.save_pearls();
            }
        }
        Some(result)
    }
    pub(super) fn slot_of_surface(&self, surface: &wl_surface::WlSurface) -> Option<u32> {
        if self.slots.is_empty()
            || self
                .surfaces
                .iter()
                .any(|s| s.layer.wl_surface() == surface)
        {
            return None;
        }
        self.slots
            .iter()
            .find(|g| g.owns(surface))
            .and_then(|g| g.id())
    }
    /// Run `f` against the panel a worker epoch belongs to.
    pub(super) fn with_epoch(&mut self, epoch: u64, f: impl FnOnce(&mut App)) {
        if self.panel_epoch == epoch && self.panel_runtime.is_some() {
            f(self);
        } else if let Some(id) = self
            .slots
            .iter()
            .find(|g| g.epoch == epoch)
            .and_then(|g| g.id())
        {
            self.in_slot(id, f);
        }
    }
    pub(super) fn pearls_on(&self) -> bool {
        self.options.pearls.is_some()
            && self
                .options
                .control
                .as_ref()
                .is_none_or(|c| c.pearls_enabled())
    }
    fn tuning(&self) -> Tuning {
        self.options
            .pearls
            .as_ref()
            .map_or_else(Tuning::default, |p| p.tuning)
    }
    fn output_named(&self, name: &str) -> Option<wl_output::WlOutput> {
        self.output
            .outputs()
            .find(|o| self.output.info(o).and_then(|i| i.name).as_deref() == Some(name))
    }
    fn monitors(&self) -> Vec<Monitor> {
        let Some(options) = &self.options.pearls else {
            return Vec::new();
        };
        self.output
            .outputs()
            .filter_map(|o| {
                let info = self.output.info(&o)?;
                let (x, y) = info.logical_position?;
                let (w, h) = info.logical_size?;
                let name = info.name?;
                let frame = (options.frame)(&name, (w, h));
                Some(Monitor {
                    name,
                    rect: pearl::Rect::new(x as f64, y as f64, w as f64, h as f64),
                    frame,
                })
            })
            .collect()
    }
    fn output_origin(&self, output: &wl_output::WlOutput) -> (f64, f64) {
        self.output
            .info(output)
            .and_then(|i| i.logical_position)
            .map_or((0., 0.), |(x, y)| (x as f64, y as f64))
    }

    /// Margins that put a pearl's visible body at its rectangle. A compositor shrinks a
    /// surface that reaches past its output (and the view would re-lay itself out), so
    /// the whole surface, shadow inset included, is kept on the output.
    pub(super) fn pearl_margins(
        &self,
        output: &wl_output::WlOutput,
        scene: &Scene,
        rect: pearl::Rect,
    ) -> (i32, i32) {
        let (ox, oy) = self.output_origin(output);
        let body = rendering::content_input_rect(scene).unwrap_or([0; 4]);
        let size = self
            .output
            .info(output)
            .and_then(|i| i.logical_size)
            .unwrap_or((i32::MAX / 2, i32::MAX / 2));
        let width = scene.bounds.w.ceil() as i32;
        let height = (scene.bounds.y + scene.bounds.h + scene.offset.1).ceil() as i32;
        (
            ((rect.x - ox).round() as i32 - body[0]).clamp(0, (size.0 - width).max(0)),
            ((rect.y - oy).round() as i32 - body[1]).clamp(0, (size.1 - height).max(0)),
        )
    }
    fn surface_origin_global(&self, surface: &Surface) -> (f64, f64) {
        let (ox, oy) = self.output_origin(&surface.output);
        let (x, y) = self
            .companion
            .as_ref()
            .map(|(scene, _, _)| {
                if let Some(info) = &self.pearl {
                    self.pearl_margins(&surface.output, scene, info.rect)
                } else {
                    self.options
                        .panel_spec
                        .as_ref()
                        .and_then(|s| s.position.as_ref())
                        .zip(
                            self.output
                                .info(&surface.output)
                                .and_then(|i| i.logical_size),
                        )
                        .map_or((0, 0), |(position, size)| {
                            position(scene, self.panel_center, size)
                        })
                }
            })
            .unwrap_or((0, 0));
        (ox + x as f64, oy + y as f64)
    }
    /// The visible body of the resident (docked) panel in global coordinates.
    fn resident_body(&self) -> Option<(pearl::Rect, String)> {
        let surface = self.surfaces.iter().find(|s| s.companion)?;
        let (scene, _, _) = self.companion.as_ref()?;
        let body = rendering::content_input_rect(scene)?;
        let (gx, gy) = self.surface_origin_global(surface);
        Some((
            pearl::Rect::new(
                gx + body[0] as f64,
                gy + body[1] as f64,
                body[2] as f64,
                body[3] as f64,
            ),
            surface.output_name.clone(),
        ))
    }
    fn resident_edge(&self) -> bead_core::Edge {
        self.companion
            .as_ref()
            .and_then(|(s, _, _)| s.motion.as_ref().map(|m| m.edge))
            .unwrap_or(self.options.edge)
    }

    // ---- detach, dock, close ------------------------------------------------

    /// Move the open resident panel out into a pearl whose body is at `rect`.
    pub(super) fn detach_resident(&mut self, rect: pearl::Rect) -> Result<u32, String> {
        if !self.pearls_on() {
            return Err("pearls are disabled".into());
        }
        let name = self.panel_name.clone().unwrap_or_default();
        if self.panel_runtime.is_none() || !self.panel_target || self.pearl.is_some() {
            return Err("no open docked panel".into());
        }
        if self
            .options
            .control
            .as_ref()
            .is_some_and(|c| !c.pearl_allowed(&name))
        {
            return Err(format!("panel {name} cannot be detached"));
        }
        let max = self.options.pearls.as_ref().map_or(0, |p| p.max);
        if self.slots.iter().filter(|g| g.is_pearl()).count() >= max {
            return Err("too many pearls".into());
        }
        let mut g = self.blank_group();
        self.swap_group(&mut g);
        // `g` now holds the panel; the resident group is blank and closed.
        self.pearl_seq += 1;
        let id = self.pearl_seq;
        g.info = Some(Info {
            id,
            kind: Kind::Pearl,
            rect,
        });
        drop(g.dismiss.take());
        self.slots.push(g);
        self.in_slot(id, |app| app.become_pearl());
        if let Err(error) = self.sync_all_decorations() {
            eprintln!("DECORATION_ERROR {error}");
        }
        self.save_pearls();
        println!("PEARL_DETACHED id={id} view={name}");
        Ok(id)
    }
    /// With the slot swapped in: reposition, hand focus to click, tell the view.
    fn become_pearl(&mut self) {
        self.panel_ui(UiEvent::PanelDetached(true));
        self.place_pearl_surfaces();
    }
    /// Re-anchor every surface of the swapped-in pearl and redraw it.
    fn place_pearl_surfaces(&mut self) {
        self.reposition_companions(true);
        let qh = self.qh.clone();
        let indices: Vec<_> = (0..self.surfaces.len())
            .filter(|i| self.surfaces[*i].companion)
            .collect();
        for i in indices {
            self.draw(i, &qh);
        }
    }
    /// Apply the swapped-in panel's placement; `reset` forgets cached input and
    /// keyboard state so the next frame recomputes it (a pearl focuses on click).
    fn reposition_companions(&mut self, reset: bool) {
        let layers: Vec<_> = self
            .surfaces
            .iter()
            .filter(|s| s.companion)
            .map(|s| (s.layer.clone(), s.output.clone()))
            .collect();
        if let Some((scene, _, _)) = &self.companion {
            for (layer, output) in &layers {
                self.position_panel_surface(layer, output, scene);
            }
        }
        for s in self.surfaces.iter_mut().filter(|s| s.companion) {
            if reset {
                s.keyboard_visible = None;
                s.input_visible = None;
                s.callbacks.invalidate();
            }
            s.layer.commit();
        }
    }
    pub(super) fn move_pearl(&mut self, id: u32, rect: pearl::Rect) {
        self.in_slot(id, |app| {
            if let Some(info) = &mut app.pearl {
                info.rect = rect;
            }
            app.reposition_companions(false);
        });
    }
    /// Dock a pearl to the edge of `output` (its own when none).
    pub(super) fn dock_pearl(&mut self, id: u32, output: Option<&str>) -> Result<(), String> {
        let at = self
            .slots
            .iter()
            .position(|g| g.id() == Some(id) && g.is_pearl())
            .ok_or("no such pearl")?;
        let target = output
            .and_then(|n| self.output_named(n))
            .or_else(|| self.slots[at].surfaces.first().map(|s| s.output.clone()))
            .ok_or("no output to dock to")?;
        let mut g = self.slots.remove(at);
        // One docked panel at a time: whatever is open yields at once.
        if self.panel_runtime.is_some() {
            self.release_panel();
        }
        self.swap_group(&mut g);
        drop(g);
        self.pearl = None;
        let spec = self
            .options
            .panel_spec
            .clone()
            .ok_or("pearl lost its view")?;
        self.panel_target = true;
        self.panel_center = None;
        drop(self.dismiss.take());
        let moved = self.panel_output.as_ref() != Some(&target);
        if moved || spec.keyboard_focus {
            self.surfaces.retain(|s| !s.companion);
            self.panel_output = Some(target.clone());
            if spec.keyboard_focus {
                self.create_dismiss(&target)?;
            }
            let qh = self.qh.clone();
            self.add_output(&qh, target.clone());
        }
        self.panel_ui(UiEvent::PanelDetached(false));
        self.panel_ui(UiEvent::PanelVisibility(true));
        self.reposition_companions(true);
        self.sync_all_decorations()?;
        self.save_pearls();
        println!("PEARL_DOCKED id={id}");
        Ok(())
    }
    pub(super) fn close_pearl(&mut self, id: u32) -> Result<(), String> {
        self.in_slot(id, |app| app.release_panel())
            .ok_or("no such pearl")?;
        Ok(())
    }
    pub(super) fn pearl_listing(&self) -> serde_json::Value {
        serde_json::json!(
            self.slots
                .iter()
                .filter(|g| g.is_pearl())
                .filter_map(|g| {
                    let i = g.info?;
                    Some(serde_json::json!({
                        "id": i.id,
                        "view": g.name,
                        "output": g.surfaces.first().map(|s| s.output_name.clone()),
                        "rect": [i.rect.x, i.rect.y, i.rect.w, i.rect.h],
                    }))
                })
                .collect::<Vec<_>>()
        )
    }

    // ---- persistence ---------------------------------------------------------

    pub(super) fn save_pearls(&mut self) {
        let Some(path) = self.options.pearls.as_ref().and_then(|p| p.store.clone()) else {
            return;
        };
        let mut layout = pearl::Layout::default();
        // Pearls whose output has not appeared yet are kept, not forgotten.
        layout.windows.extend(self.pending_pearls.iter().cloned());
        for g in self.slots.iter().filter(|g| g.is_pearl()) {
            if let (Some(i), Some(name)) = (g.info, &g.name) {
                layout.windows.push(pearl::Entry {
                    kind: name.clone(),
                    origin: g
                        .surfaces
                        .first()
                        .map(|s| s.output_name.clone())
                        .unwrap_or_default(),
                    initial: serde_json::Value::Null,
                    placement: Some(pearl::Placement::exact(i.rect)),
                });
            }
        }
        if let Err(error) = layout.save(&path) {
            eprintln!("PEARL_SAVE_ERROR {error}");
        }
    }
    pub(super) fn load_pearls(&mut self) {
        if let Some(path) = self.options.pearls.as_ref().and_then(|p| p.store.clone()) {
            self.pending_pearls = pearl::Layout::load(&path).windows;
        }
    }
    /// Open the remembered pearls of every output that is now ready.
    pub(super) fn restore_pearls(&mut self) {
        if self.pending_pearls.is_empty() || !self.pearls_on() || self.hidden {
            return;
        }
        let ready: Vec<String> = self.monitors().into_iter().map(|m| m.name).collect();
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_pearls)
            .into_iter()
            .partition(|e| ready.contains(&e.origin));
        self.pending_pearls = later;
        for entry in now {
            let rect = entry.placement.map(|p| p.rect());
            if let Err(error) = self.open_pearl(&entry.kind, Some(&entry.origin), rect) {
                eprintln!("PEARL_RESTORE_ERROR view={} {error}", entry.kind);
            }
        }
    }
    /// Open `view` straight into a pearl (no docked flash of the panel itself).
    pub(super) fn open_pearl(
        &mut self,
        view: &str,
        output: Option<&str>,
        rect: Option<pearl::Rect>,
    ) -> Result<u32, String> {
        if !self.pearls_on() {
            return Err("pearls are disabled".into());
        }
        let control = self.options.control.clone();
        if control
            .as_ref()
            .is_some_and(|c| !c.pearl_allowed(view) || !c.panel_allowed(view))
        {
            return Err(format!("panel {view} cannot be a pearl"));
        }
        let max = self.options.pearls.as_ref().map_or(0, |p| p.max);
        if self.slots.iter().filter(|g| g.is_pearl()).count() >= max {
            return Err("too many pearls".into());
        }
        let monitor = self
            .monitors()
            .into_iter()
            .find(|m| output.is_none_or(|n| n == m.name))
            .ok_or("panel output not available")?;
        let (name, spec) = {
            let panels = self
                .options
                .panels
                .as_ref()
                .ok_or("named panels are not configured")?;
            let (name, spec) = panels.resolve(Some(view))?;
            (name.to_owned(), spec.clone())
        };
        self.pearl_seq += 1;
        let id = self.pearl_seq;
        let rect = rect.unwrap_or_else(|| {
            // Not a position to guess: the middle of the output.
            let size = (spec.fallback.bounds.w as f64, spec.fallback.bounds.h as f64);
            let at = monitor.rect.centre();
            let p = pearl::place(at, size, &monitor.rect, &self.tuning());
            pearl::Rect::new(p.x, p.y, size.0, size.1)
        });
        let mut g = Group::empty(self.epoch_seq, Some(name), Some(spec));
        g.info = Some(Info {
            id,
            kind: Kind::Pearl,
            rect,
        });
        // Swap an empty group in, start it as an ordinary opening panel, swap back.
        self.swap_group(&mut g);
        let started = self.start_panel(Some(&monitor.name), true);
        if started.is_ok() {
            self.panel_ui(UiEvent::PanelDetached(true));
            self.place_pearl_surfaces();
        }
        self.swap_group(&mut g);
        started?;
        self.slots.push(g);
        self.save_pearls();
        Ok(id)
    }

    // ---- pointer gestures ----------------------------------------------------

    /// Run `f` with the group that owns `surface` in place (the resident one, or a slot).
    fn on_surface<R>(
        &mut self,
        surface: &wl_surface::WlSurface,
        f: impl FnOnce(&mut App) -> R,
    ) -> Option<R> {
        match self.slot_of_surface(surface) {
            Some(id) => self.in_slot(id, f),
            None => self
                .surfaces
                .iter()
                .any(|s| s.layer.wl_surface() == surface)
                .then(|| f(self)),
        }
    }
    /// With the surface's group in place: its body, source, edge, output and the pointer.
    #[allow(clippy::type_complexity)]
    fn gesture_subject(
        &self,
        surface: &wl_surface::WlSurface,
        local: (f64, f64),
    ) -> Option<(
        pearl::Rect,
        Source,
        bead_core::Edge,
        String,
        Point,
        Option<u32>,
    )> {
        if self.pearl.is_some_and(|i| i.kind == Kind::Guide) {
            return None;
        }
        let s = self
            .surfaces
            .iter()
            .find(|s| s.layer.wl_surface() == surface && s.companion)?;
        let (gx, gy) = self.surface_origin_global(s);
        let point = Point {
            x: gx + local.0,
            y: gy + local.1,
        };
        if let Some(info) = self.pearl {
            return Some((
                info.rect,
                Source::Pearl,
                self.resident_edge(),
                s.output_name.clone(),
                point,
                Some(info.id),
            ));
        }
        let name = self.panel_name.clone().unwrap_or_default();
        if self
            .options
            .control
            .as_ref()
            .is_some_and(|c| !c.pearl_allowed(&name))
        {
            return None;
        }
        let (body, screen) = self.resident_body()?;
        Some((
            body,
            Source::Docked,
            self.resident_edge(),
            screen,
            point,
            None,
        ))
    }
    /// A Super+press on a panel starts a gesture; returns true when the event is consumed.
    pub(super) fn gesture_press(
        &mut self,
        surface: &wl_surface::WlSurface,
        local: (f64, f64),
    ) -> bool {
        if !self.pearls_on()
            || self.gesture.is_some()
            || !(self.modifiers.logo
                || self
                    .options
                    .pearls
                    .as_ref()
                    .is_some_and(|p| p.test_any_press))
            || self
                .options
                .control
                .as_ref()
                .is_some_and(|c| !c.pearl_drag())
        {
            return false;
        }
        let Some(Some((body, source, edge, screen, point, lifted))) =
            self.on_surface(surface, |app| app.gesture_subject(surface, local))
        else {
            return false;
        };
        let monitors = self.monitors();
        let drag = Drag::begin(
            source,
            edge,
            &screen,
            body,
            point,
            &monitors,
            &self.tuning(),
        );
        self.gesture = Some(Gesture {
            surface: surface.clone(),
            drag,
            lifted,
            last_move: Instant::now() - Duration::from_secs(1),
        });
        true
    }
    pub(super) fn gesture_owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.gesture.as_ref().is_some_and(|g| &g.surface == surface)
    }
    /// Pointer motion while a gesture is held. `local` is surface-relative.
    pub(super) fn gesture_motion(&mut self, local: (f64, f64)) {
        let Some(mut gesture) = self.gesture.take() else {
            return;
        };
        // Right after a move the compositor may still deliver positions relative
        // to the old origin; wait for them to settle rather than overshoot.
        if gesture.last_move.elapsed() < Duration::from_millis(4) {
            self.gesture = Some(gesture);
            return;
        }
        let (ox, oy) = self.gesture_origin(&gesture);
        let point = Point {
            x: ox + local.0,
            y: oy + local.1,
        };
        let monitors = self.monitors();
        let tuning = self.tuning();
        gesture.drag.update(point, &monitors, &tuning);
        if gesture.drag.ready && gesture.lifted.is_none() && gesture.drag.source == Source::Docked {
            // Pulled past the threshold: the panel itself comes along as a pearl.
            let body = gesture.drag.body;
            match self.detach_resident(body) {
                Ok(id) => gesture.lifted = Some(id),
                Err(error) => {
                    eprintln!("PEARL_LIFT_ERROR {error}");
                    self.end_guides();
                    return;
                }
            }
        }
        if let Some(id) = gesture.lifted
            && gesture.drag.ready
        {
            let r = gesture.drag.current();
            let rect = pearl::Rect::new(r.x.round(), r.y.round(), r.w, r.h);
            self.move_pearl(id, rect);
            gesture.last_move = Instant::now();
        }
        if gesture.drag.ready {
            self.update_guides(&gesture.drag, &monitors);
        }
        self.gesture = Some(gesture);
    }
    fn gesture_origin(&mut self, gesture: &Gesture) -> (f64, f64) {
        let surface = gesture.surface.clone();
        self.on_surface(&surface.clone(), |app| {
            app.surfaces
                .iter()
                .find(|s| s.layer.wl_surface() == &surface)
                .map(|s| app.surface_origin_global(s))
        })
        .flatten()
        .unwrap_or((0., 0.))
    }
    pub(super) fn gesture_release(&mut self) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        self.end_guides();
        let outcome = gesture.drag.release();
        match (outcome, gesture.lifted) {
            (Outcome::Return, Some(id)) => {
                if let Err(error) = self.dock_pearl(id, Some(&gesture.drag.screen)) {
                    eprintln!("PEARL_DOCK_ERROR {error}");
                }
            }
            (
                Outcome::Detach {
                    dock: Some(screen), ..
                },
                Some(id),
            )
            | (Outcome::Dock { screen }, Some(id)) => {
                if let Err(error) = self.dock_pearl(id, Some(&screen)) {
                    eprintln!("PEARL_DOCK_ERROR {error}");
                }
            }
            (Outcome::Detach { rect, .. } | Outcome::Moved { rect }, Some(id)) => {
                self.move_pearl(id, rect);
                self.save_pearls();
            }
            // Never pulled: the press was only a press.
            (_, None) => {}
        }
    }

    // ---- guides --------------------------------------------------------------

    fn update_guides(&mut self, drag: &Drag, monitors: &[Monitor]) {
        let Some(view) = self
            .options
            .pearls
            .as_ref()
            .and_then(|p| p.guide_view.clone())
        else {
            return;
        };
        for m in monitors {
            let landing = pearl::landing(drag.edge, m);
            let film = drag.magnet.as_ref().filter(|g| g.screen == m.name);
            let guide = Guide {
                edge: drag.edge,
                output: (m.rect.w as i32, m.rect.h as i32),
                frame: [m.frame.top, m.frame.right, m.frame.bottom, m.frame.left],
                landing: Some((landing.origin.x - m.rect.x, landing.origin.y - m.rect.y)),
                film: film.map(|g| Film {
                    origin: (g.origin.x - m.rect.x, g.origin.y - m.rect.y),
                    tip: (g.tip.x - m.rect.x, g.tip.y - m.rect.y),
                    armed: g.armed,
                }),
            };
            let id = match self.guide_ids.get(&m.name) {
                Some(id) => *id,
                None => match self.start_guide(&view, &m.name) {
                    Ok(id) => {
                        self.guide_ids.insert(m.name.clone(), id);
                        id
                    }
                    Err(error) => {
                        eprintln!("PEARL_GUIDE_ERROR {error}");
                        continue;
                    }
                },
            };
            // Only changes cost the worker anything.
            if self
                .guide_last
                .get(&m.name)
                .is_some_and(|last| **last == guide)
            {
                continue;
            }
            let guide = std::sync::Arc::new(guide);
            self.guide_last.insert(m.name.clone(), guide.clone());
            self.in_slot(id, |app| app.panel_ui(UiEvent::PearlGuide(guide)));
        }
    }
    fn start_guide(&mut self, view: &str, output: &str) -> Result<u32, String> {
        let (name, spec) = {
            let panels = self.options.panels.as_ref().ok_or("no panels")?;
            let (name, spec) = panels.resolve(Some(view))?;
            (name.to_owned(), spec.clone())
        };
        self.pearl_seq += 1;
        let id = self.pearl_seq;
        let mut g = Group::empty(self.epoch_seq, Some(name), Some(spec));
        g.info = Some(Info {
            id,
            kind: Kind::Guide,
            rect: Default::default(),
        });
        self.swap_group(&mut g);
        let started = self.start_panel(Some(output), true);
        self.swap_group(&mut g);
        started?;
        self.slots.push(g);
        Ok(id)
    }
    pub(super) fn end_guides(&mut self) {
        self.guide_last.clear();
        for (_, id) in std::mem::take(&mut self.guide_ids) {
            self.in_slot(id, |app| app.release_panel());
        }
    }
}
impl Group {
    pub(crate) fn is_pearl(&self) -> bool {
        self.info.is_some_and(|i| i.kind == Kind::Pearl)
    }
}
impl App {
    pub(super) fn resident_body_rect(&self) -> Option<pearl::Rect> {
        self.resident_body().map(|(rect, _)| rect)
    }
    pub(super) fn last_pearl(&self) -> Option<u32> {
        self.slots
            .iter()
            .rev()
            .find(|g| g.is_pearl())
            .and_then(|g| g.id())
    }
}
impl App {
    pub(super) fn slot_of_layer(&self, layer: &LayerSurface) -> Option<u32> {
        if self.slots.is_empty() || self.surfaces.iter().any(|s| s.layer == *layer) {
            return None;
        }
        self.slots
            .iter()
            .find(|g| g.surfaces.iter().any(|s| s.layer == *layer))
            .and_then(|g| g.id())
    }
}
impl App {
    /// An output went away: its pearls and guides go with it, but the pearls are
    /// remembered and come back when an output of that name does.
    pub(super) fn drop_slots_on(&mut self, output: &wl_output::WlOutput) {
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|g| g.surfaces.iter().any(|s| s.output == *output));
        self.slots = kept;
        for g in gone.iter().filter(|g| g.is_pearl()) {
            if let (Some(i), Some(name), Some(s)) = (g.info, &g.name, g.surfaces.first()) {
                self.pending_pearls.push(pearl::Entry {
                    kind: name.clone(),
                    origin: s.output_name.clone(),
                    initial: serde_json::Value::Null,
                    placement: Some(pearl::Placement::exact(i.rect)),
                });
            }
        }
        self.guide_ids.clear();
        self.guide_last.clear();
        drop(gone);
    }
}

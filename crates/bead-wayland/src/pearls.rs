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

/// A mapped or moved native pearl. `first` is true after its initial xdg configure.
#[derive(Clone, Debug)]
pub struct NativePearlPlacement {
    pub id: u32,
    pub view: String,
    pub title: String,
    pub rect: pearl::Rect,
    pub first: bool,
}

/// A compositor-owned move of a native pearl, reported at move begin and end.
#[derive(Clone, Debug)]
pub struct NativePearlMotion {
    pub id: u32,
    pub moving: bool,
    pub rect: Option<pearl::Rect>,
    pub output: Option<String>,
}

pub type NativePearlWatch = std::sync::Arc<
    dyn Fn(Box<dyn Fn(NativePearlMotion) + Send + Sync>) -> Option<Box<dyn Send>> + Send + Sync,
>;
pub type NativePearlTitle = std::sync::Arc<dyn Fn(u32, &str) -> String + Send + Sync>;
pub type NativePearlComplete = Box<dyn FnOnce(bool) + Send>;
pub type NativePearlPrewarmComplete = Box<dyn FnOnce(Option<pearl::Rect>) + Send>;
pub type NativePearlPrewarm =
    std::sync::Arc<dyn Fn(NativePearlPlacement, NativePearlPrewarmComplete) + Send + Sync>;

/// Product supplied identity and asynchronous placement for native pearl windows.
#[derive(Clone)]
pub struct NativePearlOptions {
    pub title: NativePearlTitle,
    /// Complete only once the compositor has applied and settled the requested geometry.
    pub place: std::sync::Arc<dyn Fn(NativePearlPlacement, NativePearlComplete) + Send + Sync>,
    pub watch: Option<NativePearlWatch>,
    /// Query the hidden window's initial global body pose. Opt in only if the
    /// host supports commit-time `wl_surface.offset` placement from that pose.
    pub prewarm: Option<NativePearlPrewarm>,
}

/// Pins the old layer's last frame, including fractional viewport and renderer
/// resources, until a placed native content frame replaces it in one flush.
pub(crate) struct Handoff {
    pub(super) gate: policy::Handoff,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    pool: SlotPool,
    buffers: Vec<Reusable>,
    pub(super) first_frame: Option<Buffer>,
    /// The CPU alias is already attached with a verified transparent 1x1 crop.
    pub(super) first_frame_attached: bool,
    #[cfg(feature = "native-gl")]
    pub(super) first_gl_frame: bool,
    #[cfg(feature = "wgpu")]
    gpu: Option<bead_render::gpu::Surface>,
    #[cfg(feature = "native-gl")]
    gl: Option<bead_render::gl::Surface>,
    decorations: Option<decorations::Tiles>,
    background: Option<wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    background_size: Option<(i32, i32)>,
    size: (u32, u32),
    scale120: u32,
}
impl Drop for Handoff {
    fn drop(&mut self) {
        if let Some(effect) = self.background.take() {
            effect.destroy();
        }
        if let Some(f) = self.fractional.take() {
            f.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
    }
}
impl Handoff {
    pub(super) fn has_prepared_frame(&self) -> bool {
        let ready = self.first_frame.is_some();
        #[cfg(feature = "native-gl")]
        let ready = ready || self.first_gl_frame;
        ready
    }
}

/// A configured native target prepared while its source layer keeps rendering
/// and following the drag. CPU snapshots use a verified transparent viewport
/// crop until the release geometry and full viewport are committed.
pub(crate) struct PreparedNative {
    // Destroy surface-attached protocol objects before SCTK's owned wl_surface.
    pub(super) fractional: Option<WpFractionalScaleV1>,
    pub(super) viewport: Option<WpViewport>,
    pub(super) window: Option<smithay_client_toolkit::shell::xdg::window::Window>,
    pub(super) pool: Option<SlotPool>,
    pub(super) buffers: Vec<Reusable>,
    pub(super) snapshot: Option<Buffer>,
    #[cfg(feature = "native-gl")]
    pub(super) gl: Option<bead_render::gl::Surface>,
    #[cfg(feature = "native-gl")]
    pub(super) gl_ready: bool,
    /// Physical dimensions of the natural-content EGL backbuffer. This can be
    /// taller than the clipped source layer while the panel is docked.
    #[cfg(feature = "native-gl")]
    pub(super) gl_size: Option<(u32, u32)>,
    pub(super) anchor: pearl::Rect,
    pub(super) request: pearl::Rect,
    pub(super) id: u32,
    pub(super) epoch: u64,
    pub(super) size_generation: u64,
    pub(super) source_size: (u32, u32),
    pub(super) scale120: u32,
    pub(super) configured: bool,
    pub(super) placed: bool,
    pub(super) snapshot_size_generation: u64,
    pub(super) snapshot_ui_serial: u64,
    pub(super) snapshot_size: (u32, u32),
    pub(super) snapshot_scale120: u32,
    pub(super) ready_logged_generation: Option<u64>,
    pub(super) server_attached_key: Option<policy::PreparedFrameKey>,
    pub(super) server_warm_key: Option<policy::PreparedFrameKey>,
    pub(super) server_frame_epoch: u64,
    pub(super) server_pending_frame: Option<(u64, u64, policy::PreparedFrameKey)>,
    pub(super) server_warm_pixel: Option<(u32, u32)>,
    pub(super) server_ready_logged_generation: Option<u64>,
}
impl Drop for PreparedNative {
    fn drop(&mut self) {
        #[cfg(feature = "native-gl")]
        drop(self.gl.take());
        if let Some(f) = self.fractional.take() {
            f.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
    }
}

impl PreparedNative {
    pub(super) fn snapshot_key(&self) -> Option<policy::PreparedFrameKey> {
        let ready = self.snapshot.is_some();
        #[cfg(feature = "native-gl")]
        let ready = ready || self.gl_ready;
        ready.then_some(policy::PreparedFrameKey {
            generation: self.snapshot_size_generation,
            ui_serial: self.snapshot_ui_serial,
            size: self.snapshot_size,
            scale120: self.snapshot_scale120,
        })
    }
}

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
    /// Use xdg toplevels for detached pearls and report their placement to the product.
    pub native: Option<NativePearlOptions>,
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

fn placement_monitor<'a>(
    rect: pearl::Rect,
    origin: Option<&str>,
    monitors: &'a [Monitor],
) -> Option<&'a Monitor> {
    monitors
        .iter()
        .find(|monitor| monitor.rect.contains(rect.centre()))
        .or_else(|| {
            monitors
                .iter()
                .find(|monitor| Some(monitor.name.as_str()) == origin)
        })
        .or_else(|| monitors.first())
}

fn cpu_alias_already_attached(
    has_cpu_snapshot: bool,
    snapshot: Option<policy::PreparedFrameKey>,
    attached: Option<policy::PreparedFrameKey>,
    pixel: Option<(u32, u32)>,
) -> bool {
    has_cpu_snapshot && snapshot.is_some() && snapshot == attached && pixel.is_some()
}

fn cpu_warm_callback_current(
    pending: Option<(u64, u64, policy::PreparedFrameKey)>,
    attached: Option<policy::PreparedFrameKey>,
    snapshot: Option<policy::PreparedFrameKey>,
    epoch: u64,
    prepared_epoch: u64,
    key: policy::PreparedFrameKey,
) -> bool {
    pending == Some((epoch, prepared_epoch, key)) && attached == Some(key) && snapshot == Some(key)
}

/// The resident-panel state, as one movable unit.
pub(crate) struct Group {
    runtime: Option<Box<dyn SceneSource>>,
    epoch: u64,
    name: Option<String>,
    center: Option<f32>,
    anchor: Option<PanelAnchor>,
    canvas_owner: Option<u64>,
    close_epoch: u64,
    cover: Option<(u64, Vec<Surface>)>,
    dock_transition: Option<DockTransition>,
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
    reserved_pearl_id: Option<u32>,
    prewarm_attempted: bool,
    surfaces: Vec<Surface>,
}
impl Group {
    fn empty(epoch: u64, name: Option<String>, spec: Option<PanelSpec>) -> Self {
        Self {
            runtime: None,
            epoch,
            name,
            center: None,
            anchor: None,
            canvas_owner: None,
            close_epoch: 0,
            cover: None,
            dock_transition: None,
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
            reserved_pearl_id: None,
            prewarm_attempted: false,
            surfaces: Vec::new(),
        }
    }
    pub(crate) fn id(&self) -> Option<u32> {
        self.info.map(|i| i.id)
    }
    pub(crate) fn owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.surfaces.iter().any(|s| s.owns_surface(surface))
    }
    pub(crate) fn alive(&self) -> bool {
        self.runtime.is_some() && !self.surfaces.is_empty()
    }
}

/// One reverse handoff. The animation clock starts only after the first
/// accepted destination content commit, so a slow first raster cannot consume it.
#[derive(Clone)]
pub(crate) struct DockTransition {
    pub from: pearl::Rect,
    pub animation_id: String,
    pub started: bool,
}

pub(super) fn interpolate_dock_rect(
    from: pearl::Rect,
    to: pearl::Rect,
    progress: f32,
) -> pearl::Rect {
    let t = f64::from(progress.clamp(0., 1.));
    pearl::Rect::new(
        from.x + (to.x - from.x) * t,
        from.y + (to.y - from.y) * t,
        from.w + (to.w - from.w) * t,
        from.h + (to.h - from.h) * t,
    )
}

/// A press that may become a drag, and the drag it became.
pub(crate) struct Gesture {
    surface: wl_surface::WlSurface,
    drag: Drag,
    /// The pearl the docked panel turned into, once it left the threshold.
    lifted: Option<u32>,
}

impl App {
    /// Renderer retirement must include parked panels and both sides of a handoff.
    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    fn retained_surfaces(&self) -> impl Iterator<Item = &Surface> {
        self.surfaces
            .iter()
            .chain(self.panel_cover.iter().flat_map(|(_, surfaces)| surfaces))
            .chain(self.slots.iter().flat_map(|group| {
                group
                    .surfaces
                    .iter()
                    .chain(group.cover.iter().flat_map(|(_, surfaces)| surfaces))
            }))
    }
    #[cfg(feature = "native-gl")]
    pub(super) fn has_gl_surfaces(&self) -> bool {
        self.retained_surfaces().any(|s| {
            s.gl.is_some()
                || s.handoff.as_ref().is_some_and(|h| h.gl.is_some())
                || s.prepared_native.as_ref().is_some_and(|p| p.gl.is_some())
        })
    }
    #[cfg(feature = "wgpu")]
    pub(super) fn has_gpu_surfaces(&self) -> bool {
        self.retained_surfaces()
            .any(|s| s.gpu.is_some() || s.handoff.as_ref().is_some_and(|h| h.gpu.is_some()))
    }
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
        swap(&mut self.panel_anchor, &mut g.anchor);
        swap(&mut self.panel_canvas_owner, &mut g.canvas_owner);
        swap(&mut self.panel_close_epoch, &mut g.close_epoch);
        swap(&mut self.panel_cover, &mut g.cover);
        swap(&mut self.panel_dock_transition, &mut g.dock_transition);
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
        swap(&mut self.reserved_pearl_id, &mut g.reserved_pearl_id);
        swap(&mut self.prewarm_attempted, &mut g.prewarm_attempted);
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

    pub(super) fn dock_transition_progress(&self, now: f64) -> Option<f32> {
        let transition = self.panel_dock_transition.as_ref()?;
        if !transition.started {
            return Some(0.);
        }
        Some(
            self.animations
                .value(&transition.animation_id, now)
                .or_else(|| self.panel_animations.value(&transition.animation_id, now))
                .map_or(1., |(progress, _)| progress),
        )
    }

    /// Start the shell-selected easing after the destination's first content frame.
    pub(super) fn start_dock_transition_after_commit(&mut self) {
        let Some(transition) = self.panel_dock_transition.as_mut() else {
            return;
        };
        if transition.started {
            return;
        }
        let now = self.started.elapsed().as_secs_f64();
        if let Some(motion) = self
            .companion
            .as_ref()
            .and_then(|(scene, _, _)| scene.motion.as_ref())
        {
            let curve = motion
                .return_curve
                .or(motion.curve)
                .unwrap_or(if motion.spring {
                    bead_core::animation::Curve::Spring
                } else {
                    bead_core::animation::Curve::EaseOut
                });
            let duration = motion.return_duration.unwrap_or(motion.duration);
            self.animations
                .target(&transition.animation_id, 0., now, duration, curve);
            self.animations
                .target(&transition.animation_id, 1., now, duration, curve);
        }
        transition.started = true;
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
        let previous_output = g.output.clone();
        self.swap_group(&mut g);
        let result = f(self);
        self.swap_group(&mut g);
        if g.alive() {
            self.slots.insert(at.min(self.slots.len()), g);
        } else {
            let was_pearl = g.info.is_some_and(|i| i.kind == Kind::Pearl);
            let output = g.output.clone().or(previous_output);
            drop(g);
            if was_pearl {
                if self.panel_runtime.is_none()
                    && !self.slots.iter().any(|slot| slot.is_pearl())
                    && let Some(output) = output
                {
                    self.primary_idle(&output, true);
                }
                self.save_pearls();
                self.release_pearl_watch_if_idle();
            }
        }
        Some(result)
    }
    pub(super) fn slot_of_surface(&self, surface: &wl_surface::WlSurface) -> Option<u32> {
        if self.slots.is_empty() || self.surfaces.iter().any(|s| s.wl_surface() == surface) {
            return None;
        }
        self.slots
            .iter()
            .find(|g| g.owns(surface))
            .and_then(|g| g.id())
    }
    /// Run `f` against the panel a worker epoch belongs to.
    pub(super) fn with_epoch<R>(&mut self, epoch: u64, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        if self.panel_epoch == epoch && self.panel_runtime.is_some() {
            Some(f(self))
        } else {
            let id = self
                .slots
                .iter()
                .find(|g| g.epoch == epoch)
                .and_then(|g| g.id())?;
            self.in_slot(id, f)
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
    pub(super) fn output_origin(&self, output: &wl_output::WlOutput) -> (f64, f64) {
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
        self.detach_resident_with_role(rect, true)
    }
    fn detach_resident_with_role(
        &mut self,
        rect: pearl::Rect,
        native: bool,
    ) -> Result<u32, String> {
        if !self.pearls_on() {
            return Err("pearls are disabled".into());
        }
        if self
            .options
            .pearls
            .as_ref()
            .is_some_and(|p| p.native.is_some())
            && self.xdg.is_none()
        {
            return Err("native pearls require xdg-shell".into());
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
        let reserved_id = self.reserved_pearl_id.take();
        let id = reserved_id.unwrap_or_else(|| {
            self.pearl_seq += 1;
            self.pearl_seq
        });
        let mut g = self.blank_group();
        self.swap_group(&mut g);
        // `g` now holds the panel; keep the admission identity through the lift.
        g.reserved_pearl_id = Some(id);
        g.info = Some(Info {
            id,
            kind: Kind::Pearl,
            rect,
        });
        self.slots.push(g);
        self.in_slot(id, |app| app.become_pearl(native));
        if let Err(error) = self.sync_all_decorations() {
            eprintln!("DECORATION_ERROR {error}");
        }
        self.save_pearls();
        println!("PEARL_DETACHED id={id} view={name}");
        Ok(id)
    }
    /// With the slot swapped in: reposition, hand focus to click, tell the view.
    fn become_pearl(&mut self, native: bool) {
        if let Err(error) = self.set_dismiss_enabled(false) {
            eprintln!("PEARL_DISMISS_ERROR {error}");
        }
        self.panel_ui(UiEvent::PanelDetached(true));
        if self.native_pearls_enabled() {
            if native {
                // Direct/API detaches do not share the gesture-lift timing window.
                self.cancel_prepared_native();
                if let Err(error) = self.promote_native_pearl() {
                    eprintln!("PEARL_NATIVE_ERROR {error}");
                }
            } else if !self
                .surfaces
                .iter()
                .any(|surface| surface.prepared_native.is_some())
                && !self.prewarm_attempted
                && let Err(error) = self.prepare_native_pearl()
            {
                eprintln!("PEARL_NATIVE_ERROR {error}");
            }
        }
        self.place_pearl_surfaces();
    }
    fn native_pearls_enabled(&self) -> bool {
        self.pearl.is_some_and(|info| info.kind == Kind::Pearl)
            && self
                .options
                .pearls
                .as_ref()
                .is_some_and(|options| options.native.is_some())
    }

    /// Prepare a hidden native target after a valid drag press on this docked
    /// panel. The active layer remains the renderer and input surface.
    fn prewarm_drag_target(&mut self) {
        if self.pearl.is_some()
            || !self.pearls_on()
            || !self.panel_target
            || !self.panel_ready
            || self
                .panel_animations
                .active(self.started.elapsed().as_secs_f64())
            || self.panel_runtime.is_none()
            || self.prewarm_attempted
            || !self
                .surfaces
                .iter()
                .any(|surface| surface.companion && surface.configured && surface.frames > 0)
        {
            return;
        }
        let Some((name, native)) = self.panel_name.as_deref().zip(
            self.options
                .pearls
                .as_ref()
                .and_then(|options| options.native.as_ref()),
        ) else {
            return;
        };
        if native.prewarm.is_none()
            || self
                .options
                .control
                .as_ref()
                .is_some_and(|control| !control.pearl_allowed(name))
            || self.slots.iter().filter(|group| group.is_pearl()).count()
                >= self
                    .options
                    .pearls
                    .as_ref()
                    .map_or(0, |options| options.max)
        {
            return;
        }
        self.prewarm_attempted = true;
        if self.reserved_pearl_id.is_none() {
            self.pearl_seq = self.pearl_seq.wrapping_add(1).max(1);
            self.reserved_pearl_id = Some(self.pearl_seq);
        }
        if let Err(error) = self.prepare_native_pearl() {
            eprintln!("PEARL_PREWARM_ERROR {error}");
        }
    }

    /// Create the native target while the current layer remains the active
    /// renderer and input surface. The target stays transparent until release.
    fn prepare_native_pearl(&mut self) -> Result<(), String> {
        let (Some(id), Some(name), Some(native)) = (
            self.pearl.map(|info| info.id).or(self.reserved_pearl_id),
            self.panel_name.clone(),
            self.options
                .pearls
                .as_ref()
                .and_then(|options| options.native.clone()),
        ) else {
            return Ok(());
        };
        if native.prewarm.is_none() {
            return Ok(());
        }
        let Some(anchor) = self
            .pearl
            .map(|info| info.rect)
            .or_else(|| self.resident_body().map(|(rect, _)| rect))
        else {
            return Ok(());
        };
        let xdg = self.xdg.as_ref().ok_or("compositor has no xdg-shell")?;
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            if !self.surfaces[i].companion || self.surfaces[i].native.is_some() {
                continue;
            }
            if self.surfaces[i].prepared_native.is_some() {
                continue;
            }
            let surface = self.compositor.create_surface(&qh);
            if surface.version() < 5 {
                continue;
            }
            let fractional = if self.viewporter.is_some() {
                self.fractional
                    .as_ref()
                    .map(|manager| manager.get_fractional_scale(&surface, &qh, surface.clone()))
            } else {
                None
            };
            let cpu_target = self
                .companion
                .as_ref()
                .is_some_and(|(_, _, kind)| *kind == RendererKind::Cpu);
            let viewport = if fractional.is_some() || cpu_target {
                self.viewporter
                    .as_ref()
                    .map(|manager| manager.get_viewport(&surface, &qh, ()))
            } else {
                None
            };
            let window = xdg.create_window(
                surface,
                smithay_client_toolkit::shell::xdg::window::WindowDecorations::RequestClient,
                &qh,
            );
            window.set_title((native.title)(id, &name));
            window.set_app_id("bead.pearl");
            let body = self
                .companion
                .as_ref()
                .and_then(|(scene, _, _)| rendering::content_window_rect(scene));
            let (min_width, min_height) = body
                .map(|[_, _, w, h]| (w.max(1) as u32, h.max(1) as u32))
                .unwrap_or((
                    self.surfaces[i].width.max(1),
                    self.surfaces[i].height.max(1),
                ));
            window.set_min_size(Some((min_width, min_height)));
            window.set_max_size(Some((min_width, min_height)));
            if let Some([x, y, w, h]) = body {
                smithay_client_toolkit::shell::xdg::XdgSurface::set_window_geometry(
                    &window,
                    x.max(0) as u32,
                    y.max(0) as u32,
                    w.max(1) as u32,
                    h.max(1) as u32,
                );
            }
            window.commit();
            self.surfaces[i].prepared_native = Some(PreparedNative {
                fractional,
                viewport,
                window: Some(window),
                pool: Some(SlotPool::new(4096, &self.shm).map_err(|e| e.to_string())?),
                buffers: Vec::new(),
                snapshot: None,
                #[cfg(feature = "native-gl")]
                gl: None,
                #[cfg(feature = "native-gl")]
                gl_ready: false,
                #[cfg(feature = "native-gl")]
                gl_size: None,
                anchor,
                request: anchor,
                id,
                epoch: self.panel_epoch,
                size_generation: 1,
                source_size: (self.surfaces[i].width, self.surfaces[i].height),
                scale120: self.surfaces[i].scale120,
                configured: false,
                placed: false,
                snapshot_size_generation: 0,
                snapshot_ui_serial: 0,
                snapshot_size: (0, 0),
                snapshot_scale120: 0,
                ready_logged_generation: None,
                server_attached_key: None,
                server_warm_key: None,
                server_frame_epoch: 0,
                server_pending_frame: None,
                server_warm_pixel: None,
                server_ready_logged_generation: None,
            });
        }
        if self.pearl_watch.is_none()
            && let Some(watch) = native.watch
        {
            let sender = self
                .event_sender
                .clone()
                .ok_or("UI event channel unavailable")?;
            let notify: Box<dyn Fn(NativePearlMotion) + Send + Sync> = Box::new(move |motion| {
                let _ = sender.send(UiEvent::NativePearlMotion(motion));
            });
            self.pearl_watch = watch(notify);
        }
        Ok(())
    }
    fn promote_native_pearl(&mut self) -> Result<(), String> {
        let id = self.pearl.ok_or("no pearl to promote")?.id;
        let name = self.panel_name.clone().ok_or("pearl has no view")?;
        let native = self
            .options
            .pearls
            .as_ref()
            .and_then(|p| p.native.clone())
            .ok_or("native pearl options unavailable")?;
        if let Some([_, _, w, h]) = self
            .companion
            .as_ref()
            .and_then(|(scene, _, _)| rendering::content_window_rect(scene))
        {
            self.bound_native_pearl_body(w, h);
        }
        let xdg = self.xdg.as_ref().ok_or("compositor has no xdg-shell")?;
        let qh = self.qh.clone();
        let indices: Vec<_> = self
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| s.companion && s.native.is_none())
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            let surface = self.compositor.create_surface(&qh);
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
            let window = xdg.create_window(
                surface,
                smithay_client_toolkit::shell::xdg::window::WindowDecorations::RequestClient,
                &qh,
            );
            let title = (native.title)(id, &name);
            window.set_title(title);
            window.set_app_id("bead.pearl");
            window.set_min_size(Some((1, 1)));
            if let Some((scene, _, _)) = &self.companion
                && let Some([x, y, w, h]) = rendering::content_window_rect(scene)
            {
                smithay_client_toolkit::shell::xdg::XdgSurface::set_window_geometry(
                    &window,
                    x.max(0) as u32,
                    y.max(0) as u32,
                    w.max(1) as u32,
                    h.max(1) as u32,
                );
                window.set_min_size(Some((w.max(1) as u32, h.max(1) as u32)));
                window.set_max_size(Some((w.max(1) as u32, h.max(1) as u32)));
            }
            window.commit();
            let s = &mut self.surfaces[i];
            s.handoff = Some(Handoff {
                gate: Default::default(),
                fractional: s.fractional.take(),
                viewport: s.viewport.take(),
                pool: std::mem::replace(
                    &mut s.pool,
                    SlotPool::new(4096, &self.shm).map_err(|e| e.to_string())?,
                ),
                buffers: std::mem::take(&mut s.buffers),
                first_frame: None,
                first_frame_attached: false,
                #[cfg(feature = "native-gl")]
                first_gl_frame: false,
                #[cfg(feature = "wgpu")]
                gpu: s.gpu.take(),
                #[cfg(feature = "native-gl")]
                gl: s.gl.take(),
                decorations: s.decorations.take(),
                background: s.background.take(),
                background_size: s.background_size.take(),
                size: (s.width, s.height),
                scale120: s.scale120,
            });
            s.fractional = fractional;
            s.viewport = viewport;
            s.native = Some(window);
            s.native_configure_size = None;
            s.configured = false;
            s.redraw = true;
            s.callbacks.invalidate();
            // Idle-pool timers belong to the old wl_surface identity. They cannot
            // compact this replacement surface or clear its pending flag.
            s.shm_trim_pending = false;
            s.heap_trimmed_ui_serial = u64::MAX;
            s.previous = None;
            s.visual_damage = Default::default();
            s.presented_generation = None;
            // The old layer stays mapped. The destination first maps with a
            // transparent buffer so compositor placement cannot show a wrong pose.
        }
        if self.pearl_watch.is_none()
            && let Some(watch) = native.watch
        {
            let sender = self
                .event_sender
                .clone()
                .ok_or("UI event channel unavailable")?;
            let notify: Box<dyn Fn(NativePearlMotion) + Send + Sync> = Box::new(move |motion| {
                let _ = sender.send(UiEvent::NativePearlMotion(motion));
            });
            self.pearl_watch = watch(notify);
        }
        Ok(())
    }

    fn bound_native_pearl_body(&mut self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let Some(info) = self.pearl.filter(|info| info.kind == Kind::Pearl) else {
            return;
        };
        let monitors = self.monitors();
        let origin = self
            .surfaces
            .iter()
            .find(|s| s.companion)
            .map(|s| s.output_name.as_str());
        let Some(monitor) = placement_monitor(info.rect, origin, &monitors) else {
            return;
        };
        let rect = pearl::Rect::new(info.rect.x, info.rect.y, w as f64, h as f64);
        let rect = pearl::recover(
            Some(rect),
            (w as f64, h as f64),
            &monitor.rect,
            &self.tuning(),
        );
        self.pearl = Some(Info { rect, ..info });
        if let Some(output) = self.output_named(&monitor.name) {
            self.panel_output = Some(output.clone());
            for surface in self.surfaces.iter_mut().filter(|s| s.companion) {
                surface.output = output.clone();
                surface.output_name = monitor.name.clone();
            }
            if let Some(source) = &mut self.panel_runtime
                && source.viewport(monitor.rect.w as u32, monitor.rect.h as u32)
                && let Some(sender) = &self.event_sender
            {
                let _ = sender.send(UiEvent::Panel(
                    self.panel_epoch,
                    Box::new(UiEvent::Relayout),
                ));
            }
        }
    }

    /// A restored pearl can map before its worker replaces the placeholder scene.
    /// Keep native buffer size and window geometry in step with accepted content.
    pub(super) fn sync_native_pearl_scene(&mut self) -> bool {
        if !self.surfaces.iter().any(|surface| {
            surface.companion && (surface.native.is_some() || surface.prepared_native.is_some())
        }) {
            return false;
        }
        let Some((scene, _, _)) = &self.companion else {
            return false;
        };
        let width = scene.bounds.w.ceil().max(1.) as u32;
        let height =
            (scene.bounds.y + scene.bounds.h + scene.slide.map_or(scene.offset.1, |s| s.open_y))
                .ceil()
                .max(1.) as u32;
        let body = rendering::content_window_rect(scene);
        let previous = self.pearl.map(|info| info.rect);
        if let Some([_, _, w, h]) = body {
            self.bound_native_pearl_body(w, h);
        }
        let mut changed = previous != self.pearl.map(|info| info.rect);
        for surface in self.surfaces.iter_mut().filter(|surface| {
            surface.companion && (surface.native.is_some() || surface.prepared_native.is_some())
        }) {
            let size_changed = surface.width != width || surface.height != height;
            if size_changed {
                surface.callbacks.invalidate();
                surface.width = width;
                surface.height = height;
                surface.redraw = true;
                changed = true;
            }
            if let Some(viewport) = &surface.viewport {
                viewport.set_destination(width as i32, height as i32);
            }
            if let Some(window) = surface.native.as_ref()
                && let Some([x, y, w, h]) = body
            {
                smithay_client_toolkit::shell::xdg::XdgSurface::set_window_geometry(
                    window,
                    x.max(0) as u32,
                    y.max(0) as u32,
                    w.max(1) as u32,
                    h.max(1) as u32,
                );
                window.set_min_size(Some((w.max(1) as u32, h.max(1) as u32)));
                window.set_max_size(Some((w.max(1) as u32, h.max(1) as u32)));
            }
            if let Some(prepared) = surface.prepared_native.as_mut()
                && (size_changed
                    || prepared.source_size != (width, height)
                    || self.pearl.is_some_and(|info| {
                        prepared.anchor.w != info.rect.w || prepared.anchor.h != info.rect.h
                    }))
            {
                if let Some(window) = prepared.window.as_ref()
                    && let Some([x, y, w, h]) = body
                {
                    smithay_client_toolkit::shell::xdg::XdgSurface::set_window_geometry(
                        window,
                        x.max(0) as u32,
                        y.max(0) as u32,
                        w.max(1) as u32,
                        h.max(1) as u32,
                    );
                    window.set_min_size(Some((w.max(1) as u32, h.max(1) as u32)));
                    window.set_max_size(Some((w.max(1) as u32, h.max(1) as u32)));
                    window.commit();
                }
                prepared.size_generation = prepared.size_generation.wrapping_add(1);
                prepared.source_size = (width, height);
                prepared.configured = false;
                prepared.placed = false;
                prepared.snapshot = None;
                prepared.server_attached_key = None;
                prepared.server_warm_key = None;
                prepared.server_pending_frame = None;
                prepared.server_warm_pixel = None;
                prepared.server_ready_logged_generation = None;
                #[cfg(feature = "native-gl")]
                {
                    prepared.gl_ready = false;
                }
                prepared.snapshot_size_generation = 0;
                prepared.ready_logged_generation = None;
                if let Some(info) = self.pearl {
                    prepared.anchor = pearl::Rect::new(
                        prepared.anchor.x,
                        prepared.anchor.y,
                        info.rect.w,
                        info.rect.h,
                    );
                    prepared.request = prepared.anchor;
                }
            }
        }
        changed
    }

    /// Use an accepted idle panel frame to absorb a prepared native target's
    /// natural-size update before the next user input arrives.
    pub(super) fn prewarm_native_pearl_on_idle_frame(
        &mut self,
        index: usize,
        animation_active: bool,
    ) -> bool {
        if animation_active
            || self.scrolls.is_animating()
            || self.options.animate
            || self.options.debug_continuous
            || self.pearl.is_some()
            || !self.panel_target
            || !self.panel_ready
            || self.panel_runtime.is_none()
            || !self
                .surfaces
                .get(index)
                .is_some_and(|surface| surface.companion && surface.ready)
            || !self
                .surfaces
                .iter()
                .any(|surface| surface.companion && surface.prepared_native.is_some())
            || self.surfaces.iter().any(|surface| {
                surface.companion && (surface.callbacks.pending || surface.pointer_visual.is_some())
            })
        {
            return false;
        }
        self.sync_native_pearl_scene()
    }

    pub(super) fn place_native_pearl(&mut self, first: bool) {
        let (Some(info), Some(name), Some(native)) = (
            self.pearl,
            self.panel_name.clone(),
            self.options.pearls.as_ref().and_then(|p| p.native.clone()),
        ) else {
            return;
        };
        if info.kind != Kind::Pearl {
            return;
        }
        if !self
            .surfaces
            .iter()
            .any(|surface| surface.companion && surface.native.is_some() && surface.configured)
        {
            return;
        }
        let initial = self.surfaces.iter().any(|s| s.handoff.is_some());
        let mut requested = false;
        for h in self.surfaces.iter_mut().filter_map(|s| s.handoff.as_mut()) {
            requested |= h.gate.request(info.rect);
        }
        if initial && !requested {
            return;
        }
        if initial {
            // A cold worker can replace its provisional size while covered.
            // Commit that complete transparent geometry before asking the
            // compositor to place it, otherwise old min/max constraints clamp it.
            let pending: Vec<_> = self
                .surfaces
                .iter()
                .enumerate()
                .filter(|(_, surface)| surface.handoff.is_some())
                .map(|(index, _)| index)
                .collect();
            for index in pending {
                if let Err(error) = self.map_native_pearl_transparent(index) {
                    eprintln!("PEARL_NATIVE_ERROR {error}");
                    self.native_pearl_placed(self.panel_epoch, info.rect, false);
                    return;
                }
            }
        }
        let title = (native.title)(info.id, &name);
        let sender = self.event_sender.clone();
        let epoch = self.panel_epoch;
        (native.place)(
            NativePearlPlacement {
                id: info.id,
                view: name,
                title,
                rect: info.rect,
                first: first || initial,
            },
            Box::new(move |success| {
                if initial && let Some(sender) = sender {
                    let _ = sender.send(UiEvent::NativePearlPlaced {
                        id: info.id,
                        epoch,
                        rect: info.rect,
                        success,
                    });
                }
            }),
        );
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
        if let Some((scene, _, _)) = &self.companion {
            for s in self
                .surfaces
                .iter()
                .filter(|s| s.companion && s.native.is_none())
            {
                self.position_panel_surface(&s.layer, &s.output, scene);
            }
        }
        for s in self.surfaces.iter_mut().filter(|s| s.companion) {
            if reset {
                s.keyboard_visible = None;
                s.input_visible = None;
                s.callbacks.invalidate();
            }
            s.commit();
        }
    }
    pub(super) fn move_pearl(&mut self, id: u32, rect: pearl::Rect) {
        self.in_slot(id, |app| {
            if let Some(info) = &mut app.pearl {
                info.rect = rect;
            }
            app.reposition_companions(false);
            if !app
                .surfaces
                .iter()
                .any(|surface| surface.prepared_native.is_some())
            {
                app.place_native_pearl(false);
            }
        });
    }
    pub(super) fn prepared_native_scale_changed(&mut self, i: usize, scale120: u32) {
        let Some(prepared) = self
            .surfaces
            .get_mut(i)
            .and_then(|surface| surface.prepared_native.as_mut())
        else {
            return;
        };
        if prepared.scale120 == scale120 {
            return;
        }
        prepared.size_generation = prepared.size_generation.wrapping_add(1);
        prepared.scale120 = scale120;
        prepared.configured = false;
        prepared.placed = false;
        prepared.snapshot = None;
        prepared.server_attached_key = None;
        prepared.server_warm_key = None;
        prepared.server_pending_frame = None;
        prepared.server_warm_pixel = None;
        prepared.server_ready_logged_generation = None;
        #[cfg(feature = "native-gl")]
        {
            prepared.gl_ready = false;
        }
        prepared.snapshot_size_generation = 0;
        prepared.ready_logged_generation = None;
        self.prepared_window_configure(i);
    }

    pub(super) fn native_window_configure(
        &mut self,
        window: &smithay_client_toolkit::shell::xdg::window::Window,
        configure: smithay_client_toolkit::shell::xdg::window::WindowConfigure,
    ) {
        if let Some(i) = self.surfaces.iter().position(|surface| {
            surface
                .prepared_native
                .as_ref()
                .is_some_and(|prepared| prepared.window.as_ref() == Some(window))
        }) {
            self.prepared_window_configure(i);
            return;
        }
        let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.native.as_ref() == Some(window))
        else {
            return;
        };
        let first = !self.surfaces[i].configured;
        let configured_width = configure
            .new_size
            .0
            .map_or(self.surfaces[i].width.max(1), |v| v.get());
        let configured_height = configure
            .new_size
            .1
            .map_or(self.surfaces[i].height.max(1), |v| v.get());
        let resized =
            self.surfaces[i].native_configure_size != Some((configured_width, configured_height));
        let surface = &mut self.surfaces[i];
        surface.configured = true;
        surface.native_configure_size = Some((configured_width, configured_height));
        surface.redraw = true;
        let content_changed = self.sync_native_pearl_scene();
        if first || (content_changed && self.surfaces[i].handoff.is_some()) {
            self.place_native_pearl(true);
        } else if resized {
            self.save_pearls();
        }
        let qh = self.qh.clone();
        self.draw(i, &qh);
    }

    fn prepared_window_configure(&mut self, i: usize) {
        let s = &mut self.surfaces[i];
        let Some(prepared) = s.prepared_native.as_mut() else {
            return;
        };
        if prepared.configured
            && prepared.source_size == (s.width, s.height)
            && prepared.scale120 == s.scale120
        {
            return;
        }
        let snapshot_current = prepared.snapshot_key().is_some()
            && prepared.snapshot_size_generation == prepared.size_generation
            && prepared.snapshot_size == (s.width, s.height)
            && prepared.snapshot_scale120 == s.scale120
            && prepared.snapshot_ui_serial == s.ui_serial;
        let Some(window) = prepared.window.as_ref() else {
            return;
        };
        let surface = window.wl_surface().clone();
        let (buffer_width, buffer_height) = if prepared.viewport.is_some() {
            (1, 1)
        } else {
            (s.width as i32, s.height as i32)
        };
        let scale120 = if prepared.viewport.is_some() {
            120
        } else {
            prepared.scale120
        };
        let (w, h, stride, _) =
            match buffer_dimensions(buffer_width as u32, buffer_height as u32, scale120) {
                Ok(dimensions) => dimensions,
                Err(error) => {
                    eprintln!("PEARL_PREWARM_ERROR {error}");
                    return;
                }
            };
        let Some(pool) = prepared.pool.as_mut() else {
            eprintln!("PEARL_PREWARM_ERROR bootstrap pool is unavailable");
            return;
        };
        let Ok((buffer, canvas)) = pool.create_buffer(w, h, stride, wl_shm::Format::Argb8888)
        else {
            eprintln!("PEARL_PREWARM_ERROR transparent bootstrap allocation failed");
            return;
        };
        canvas.fill(0);
        let Ok(empty) = Region::new(&self.compositor) else {
            eprintln!("PEARL_PREWARM_ERROR input region allocation failed");
            return;
        };
        surface.set_input_region(Some(empty.wl_region()));
        if let Some(viewport) = &prepared.viewport {
            surface.set_buffer_scale(1);
            viewport.set_source(-1.0, -1.0, -1.0, -1.0);
            viewport.set_destination(s.width as i32, s.height as i32);
        } else {
            surface.set_buffer_scale((prepared.scale120 / 120) as i32);
        }
        if let Err(error) = buffer.attach_to(&surface) {
            eprintln!("PEARL_PREWARM_ERROR {error}");
            return;
        }
        surface.damage_buffer(0, 0, w, h);
        surface.commit();
        prepared.buffers.push(Reusable {
            buffer,
            pending: Vec::new(),
            painted: false,
        });
        prepared.source_size = (s.width, s.height);
        prepared.configured = true;
        prepared.placed = false;
        prepared.server_attached_key = None;
        prepared.server_warm_key = None;
        prepared.server_pending_frame = None;
        prepared.server_warm_pixel = None;
        prepared.server_ready_logged_generation = None;
        if !snapshot_current {
            prepared.snapshot = None;
            #[cfg(feature = "native-gl")]
            {
                prepared.gl_ready = false;
            }
        }
        if let Some(info) = self.pearl {
            prepared.anchor = pearl::Rect::new(
                prepared.anchor.x,
                prepared.anchor.y,
                info.rect.w,
                info.rect.h,
            );
            prepared.request = prepared.anchor;
        }
        self.place_prepared_native(i);
        #[cfg(feature = "native-gl")]
        if self.surfaces[i].gl.is_some() {
            self.surfaces[i].redraw = true;
            self.draw(i, &self.qh.clone());
        }
    }

    fn place_prepared_native(&mut self, i: usize) {
        let (Some(name), Some(native), Some(prepared)) = (
            self.panel_name.clone(),
            self.options
                .pearls
                .as_ref()
                .and_then(|options| options.native.clone()),
            self.surfaces
                .get(i)
                .and_then(|surface| surface.prepared_native.as_ref()),
        ) else {
            return;
        };
        if !prepared.configured {
            return;
        }
        let id = prepared.id;
        let rect = prepared.request;
        let epoch = prepared.epoch;
        let generation = prepared.size_generation;
        let sender = self.event_sender.clone();
        let Some(prewarm) = native.prewarm else {
            return;
        };
        prewarm(
            NativePearlPlacement {
                id,
                view: name.clone(),
                title: (native.title)(id, &name),
                rect,
                first: true,
            },
            Box::new(move |anchor| {
                if let Some(sender) = sender {
                    let _ = sender.send(UiEvent::NativePearlPrewarmed {
                        id,
                        epoch,
                        generation,
                        rect,
                        anchor,
                    });
                }
            }),
        );
    }

    pub(super) fn native_pearl_prewarmed(
        &mut self,
        id: u32,
        epoch: u64,
        generation: u64,
        rect: pearl::Rect,
        anchor: Option<pearl::Rect>,
    ) {
        if self.panel_epoch != epoch {
            return;
        }
        let index = self.surfaces.iter_mut().position(|surface| {
            let Some(prepared) = surface.prepared_native.as_mut() else {
                return false;
            };
            if prepared.id != id
                || prepared.epoch != epoch
                || prepared.size_generation != generation
                || prepared.request != rect
                || !prepared.configured
            {
                return false;
            }
            if let Some(anchor) = anchor {
                prepared.anchor = anchor;
                prepared.placed = true;
            }
            true
        });
        let Some(index) = index else {
            return;
        };
        if anchor.is_none() {
            for surface in &mut self.surfaces {
                if surface.prepared_native.as_ref().is_some_and(|prepared| {
                    prepared.epoch == epoch && prepared.size_generation == generation
                }) {
                    surface.prepared_native = None;
                }
            }
            self.release_pearl_watch_if_idle();
            eprintln!("PEARL_PREWARM_FAILED; release will use covered fallback");
        } else {
            self.log_prepared_native_ready(index);
        }
    }

    pub(super) fn log_prepared_native_ready(&mut self, i: usize) {
        let id = self.pearl.map_or(0, |info| info.id);
        let Some(surface) = self.surfaces.get_mut(i) else {
            return;
        };
        let Some(prepared) = surface.prepared_native.as_mut() else {
            return;
        };
        let snapshot = prepared.snapshot_key();
        let current = policy::PreparedFrameKey {
            generation: prepared.size_generation,
            ui_serial: surface.ui_serial,
            size: (surface.width, surface.height),
            scale120: surface.scale120,
        };
        if prepared.placed
            && policy::prepared_frame_ready(prepared.configured, prepared.placed, snapshot, current)
            && prepared.ready_logged_generation != Some(prepared.size_generation)
        {
            prepared.ready_logged_generation = Some(prepared.size_generation);
            bead_core::vprintln!(
                "PEARL_PREWARM_READY id={} epoch={} generation={} ui_serial={} monotonic_ns={}",
                id,
                prepared.epoch,
                prepared.size_generation,
                surface.ui_serial,
                bead_core::profile::monotonic_ns()
            );
        }
    }

    pub(super) fn prepared_native_frame(
        &mut self,
        surface: &wl_surface::WlSurface,
        frame: PreparedFrameData,
    ) {
        let panel_epoch = self.panel_epoch;
        let Some(prepared_surface) = self.surfaces.iter_mut().find(|surface_state| {
            surface_state
                .prepared_native
                .as_ref()
                .and_then(|prepared| prepared.window.as_ref())
                .is_some_and(|window| window.wl_surface() == surface)
        }) else {
            return;
        };
        let Some(prepared) = prepared_surface.prepared_native.as_mut() else {
            return;
        };
        let key = policy::PreparedFrameKey {
            generation: frame.size_generation,
            ui_serial: frame.ui_serial,
            size: (prepared_surface.width, prepared_surface.height),
            scale120: prepared_surface.scale120,
        };
        if !cpu_warm_callback_current(
            prepared.server_pending_frame,
            prepared.server_attached_key,
            prepared.snapshot_key(),
            frame.epoch,
            frame.prepared_epoch,
            key,
        ) || prepared.epoch != frame.prepared_epoch
            || panel_epoch != frame.prepared_epoch
        {
            return;
        }
        prepared.server_pending_frame = None;
        prepared.server_warm_key = Some(key);
        if prepared.server_ready_logged_generation != Some(frame.size_generation) {
            prepared.server_ready_logged_generation = Some(frame.size_generation);
            println!(
                "PEARL_SERVER_PREWARM_READY id={} epoch={} generation={} ui_serial={} monotonic_ns={}",
                prepared.id,
                prepared.epoch,
                frame.size_generation,
                frame.ui_serial,
                bead_core::profile::monotonic_ns()
            );
        }
    }

    pub(super) fn cancel_prepared_native(&mut self) {
        for surface in &mut self.surfaces {
            surface.prepared_native = None;
        }
        self.release_pearl_watch_if_idle();
    }

    pub(super) fn cancel_prepared_window(
        &mut self,
        window: &smithay_client_toolkit::shell::xdg::window::Window,
    ) -> bool {
        let id = self.surfaces.iter().find_map(|surface| {
            surface
                .prepared_native
                .as_ref()
                .filter(|prepared| prepared.window.as_ref() == Some(window))
                .map(|prepared| prepared.id)
        });
        let Some(id) = id else {
            return false;
        };
        for surface in &mut self.surfaces {
            if surface
                .prepared_native
                .as_ref()
                .is_some_and(|prepared| prepared.id == id)
            {
                surface.prepared_native = None;
            }
        }
        if self.pearl.is_none() && self.reserved_pearl_id == Some(id) {
            self.reserved_pearl_id = None;
            self.prewarm_attempted = true;
        }
        self.release_pearl_watch_if_idle();
        true
    }

    fn activate_prepared_native(&mut self, rect: pearl::Rect) -> bool {
        let Some(native) = self.options.pearls.as_ref().and_then(|p| p.native.as_ref()) else {
            return false;
        };
        if native.prewarm.is_none() {
            return false;
        }
        let expected_id = self.pearl.map(|info| info.id).or(self.reserved_pearl_id);
        let indices: Vec<_> = self
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, surface)| surface.companion)
            .map(|(index, _)| index)
            .collect();
        if expected_id.is_none()
            || indices.is_empty()
            || indices.iter().any(|&i| {
                let surface = &self.surfaces[i];
                let Some(prepared) = surface.prepared_native.as_ref() else {
                    return true;
                };
                if Some(prepared.id) != expected_id {
                    return true;
                }
                #[cfg(feature = "native-gl")]
                let gl_extent_matches = !prepared.gl_ready
                    || buffer_dimensions(surface.width, surface.height, surface.scale120)
                        .is_ok_and(|(width, height, _, _)| {
                            prepared.gl_size == Some((width as u32, height as u32))
                        });
                #[cfg(not(feature = "native-gl"))]
                let gl_extent_matches = true;
                prepared
                    .window
                    .as_ref()
                    .is_none_or(|window| window.wl_surface().version() < 5)
                    || !gl_extent_matches
                    || !policy::prepared_frame_ready(
                        prepared.configured,
                        prepared.placed,
                        prepared.snapshot_key(),
                        policy::PreparedFrameKey {
                            generation: prepared.size_generation,
                            ui_serial: surface.ui_serial,
                            size: (surface.width, surface.height),
                            scale120: surface.scale120,
                        },
                    )
                    || prepared.source_size != (surface.width, surface.height)
            })
        {
            return false;
        }
        if let Some(info) = &mut self.pearl {
            info.rect = rect;
        }
        for i in indices {
            let s = &mut self.surfaces[i];
            let mut prepared = s.prepared_native.take().unwrap();
            let surface = prepared.window.as_ref().unwrap().wl_surface().clone();
            let dx = (rect.x - prepared.anchor.x).round() as i32;
            let dy = (rect.y - prepared.anchor.y).round() as i32;
            surface.offset(dx, dy);
            if let Some([x, y, w, h]) = s.input_rect.filter(|_| s.input_visible == Some(true)) {
                if let Ok(region) = Region::new(&self.compositor) {
                    if w > 0 && h > 0 {
                        region.add(x, y, w, h);
                    }
                    surface.set_input_region(Some(region.wl_region()));
                }
            } else if let Ok(empty) = Region::new(&self.compositor) {
                surface.set_input_region(Some(empty.wl_region()));
            }
            let snapshot_key = prepared.snapshot_key();
            let first_frame_attached = cpu_alias_already_attached(
                prepared.snapshot.is_some(),
                snapshot_key,
                prepared.server_attached_key,
                prepared.server_warm_pixel,
            );
            let server_callback_ready =
                snapshot_key.is_some() && prepared.server_warm_key == snapshot_key;
            bead_core::vprintln!(
                "PEARL_SERVER_PREWARM_ACTIVATE id={} attached={} callback_ready={}",
                prepared.id,
                first_frame_attached,
                server_callback_ready
            );
            if let Some(viewport) = &prepared.viewport {
                viewport.set_source(-1.0, -1.0, -1.0, -1.0);
                viewport.set_destination(s.width as i32, s.height as i32);
            }
            let snapshot = prepared.snapshot.take();
            let bootstrap_pool = prepared.pool.take().unwrap();
            let bootstrap_buffers = std::mem::take(&mut prepared.buffers);
            #[cfg(feature = "native-gl")]
            let use_gl = prepared.gl_ready;
            #[cfg(not(feature = "native-gl"))]
            let use_gl = false;
            let (cover_pool, cover_buffers) = if use_gl {
                // A prepared EGL backbuffer needs no source SHM residency.
                // Preserve that pool only for the CPU alias handoff.
                s.geometry = (0, 0);
                (
                    std::mem::replace(&mut s.pool, bootstrap_pool),
                    std::mem::replace(&mut s.buffers, bootstrap_buffers),
                )
            } else {
                (bootstrap_pool, bootstrap_buffers)
            };
            let mut handoff = Handoff {
                gate: Default::default(),
                fractional: s.fractional.take(),
                viewport: s.viewport.take(),
                pool: cover_pool,
                buffers: cover_buffers,
                first_frame: snapshot,
                first_frame_attached,
                #[cfg(feature = "native-gl")]
                first_gl_frame: prepared.gl_ready,
                #[cfg(feature = "wgpu")]
                gpu: s.gpu.take(),
                #[cfg(feature = "native-gl")]
                gl: s.gl.take(),
                decorations: s.decorations.take(),
                background: s.background.take(),
                background_size: s.background_size.take(),
                size: (s.width, s.height),
                scale120: s.scale120,
            };
            let _ = handoff.gate.request(rect);
            handoff.gate.placed(rect);
            s.fractional = prepared.fractional.take();
            s.viewport = prepared.viewport.take();
            s.native = prepared.window.take();
            #[cfg(feature = "native-gl")]
            {
                s.gl = prepared.gl.take();
            }
            s.native_configure_size = Some((s.width, s.height));
            s.configured = true;
            s.callbacks.invalidate();
            s.callbacks.pending = false;
            // The native target received the source's current input region above.
            // Keep the cache so the first-frame fast path does not expose a full window.
            s.redraw = true;
            s.handoff = Some(handoff);
            drop(prepared);
        }
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            if self.surfaces[i].companion && self.surfaces[i].handoff.is_some() {
                self.draw(i, &qh);
            }
        }
        true
    }

    fn map_native_pearl_transparent(&mut self, i: usize) -> Result<(), Box<dyn Error>> {
        let s = &mut self.surfaces[i];
        let surface = s.wl_surface().clone();
        let (w, h, stride, _) = buffer_dimensions(s.width, s.height, s.scale120)?;
        let (buffer, canvas) = s
            .pool
            .create_buffer(w, h, stride, wl_shm::Format::Argb8888)?;
        canvas.fill(0);
        let empty = Region::new(&self.compositor)?;
        surface.set_input_region(Some(empty.wl_region()));
        s.layer
            .wl_surface()
            .set_input_region(Some(empty.wl_region()));
        s.layer.commit();
        if let Some(viewport) = &s.viewport {
            surface.set_buffer_scale(1);
            viewport.set_destination(s.width as i32, s.height as i32);
        } else {
            surface.set_buffer_scale((s.scale120 / 120) as i32);
        }
        buffer.attach_to(&surface)?;
        surface.damage_buffer(0, 0, w, h);
        surface.commit();
        s.buffers.push(Reusable {
            buffer,
            pending: vec![],
            painted: false,
        });
        // Its current transparent slot is compositor-owned until replaced. The
        // first content draw needs a fresh reusable pool, even at the same size.
        s.geometry = (0, 0);
        Ok(())
    }

    pub(super) fn native_pearl_placed(&mut self, epoch: u64, rect: pearl::Rect, success: bool) {
        if self.panel_epoch != epoch {
            return;
        }
        let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.handoff.as_ref().is_some_and(|h| h.gate.accepts(rect)))
        else {
            return;
        };
        if !success {
            // The cover never went away. Restore its resources and keep the
            // dropped layer usable when the adapter cannot place a native window.
            let s = &mut self.surfaces[i];
            let mut h = s.handoff.take().unwrap();
            #[cfg(feature = "wgpu")]
            {
                std::mem::swap(&mut s.gpu, &mut h.gpu);
                drop(h.gpu.take());
            }
            #[cfg(feature = "native-gl")]
            {
                std::mem::swap(&mut s.gl, &mut h.gl);
                drop(h.gl.take());
            }
            s.native = None;
            if let Some(f) = s.fractional.take() {
                f.destroy();
            }
            if let Some(v) = s.viewport.take() {
                v.destroy();
            }
            s.fractional = h.fractional.take();
            s.viewport = h.viewport.take();
            std::mem::swap(&mut s.pool, &mut h.pool);
            std::mem::swap(&mut s.buffers, &mut h.buffers);
            s.decorations = h.decorations.take();
            s.background = h.background.take();
            s.background_size = h.background_size;
            (s.width, s.height) = h.size;
            s.scale120 = h.scale120;
            s.native_configure_size = None;
            s.callbacks.invalidate();
            s.previous = None;
            s.visual_damage = Default::default();
            s.input_visible = None;
            s.keyboard_visible = None;
            s.redraw = true;
            eprintln!("PEARL_HANDOFF_FAILED source retained");
        } else {
            if self.pearl.is_some_and(|info| info.rect != rect) {
                self.place_native_pearl(true);
                return;
            }
            self.surfaces[i].handoff.as_mut().unwrap().gate.placed(rect);
            self.surfaces[i].input_visible = None;
            self.surfaces[i].redraw = true;
        }
        let qh = self.qh.clone();
        self.draw(i, &qh);
    }

    pub(super) fn retire_native_cover(&mut self, i: usize, committed: bool) {
        let s = &mut self.surfaces[i];
        let retire = s
            .handoff
            .as_mut()
            .is_some_and(|handoff| handoff.gate.finish_frame(committed));
        if retire {
            s.layer.wl_surface().attach(None, 0, 0);
            s.layer.commit();
            // Submit the covered swap before allocator/driver retirement work.
            let _ = self.conn.flush();
            let mut handoff = s.handoff.take().unwrap();
            let backend = if handoff.first_frame.is_some() {
                "shm"
            } else {
                "rendered"
            };
            #[cfg(feature = "native-gl")]
            let backend = if handoff.first_gl_frame {
                "gl"
            } else {
                backend
            };
            if let Some(buffer) = handoff.first_frame.take() {
                // The first native frame aliases the active source SlotPool. Keep
                // its distinct wl_buffer in the active vector until compositor release.
                s.buffers.push(Reusable {
                    buffer,
                    pending: Vec::new(),
                    painted: true,
                });
            }
            bead_core::vprintln!(
                "PEARL_HANDOFF_COMMITTED id={} rect={:?} monotonic_ns={} backend={}",
                self.pearl.map_or(0, |info| info.id),
                self.pearl.map(|info| info.rect),
                bead_core::profile::monotonic_ns(),
                backend
            );
            drop(handoff);
        }
    }
    pub(super) fn native_pearl_motion(&mut self, motion: NativePearlMotion) {
        if self.slot_of_id(motion.id).is_none() {
            return;
        }
        let outcome = self.in_slot(motion.id, |app| app.native_pearl_motion_in_slot(&motion));
        if let Some(Some(Outcome::Dock { screen })) = outcome {
            if let Err(error) = self.dock_pearl(motion.id, Some(&screen)) {
                eprintln!("PEARL_DOCK_ERROR id={} {error}", motion.id);
            }
        } else if !motion.moving {
            self.save_pearls();
            self.release_pearl_watch_if_idle();
        }
    }
    fn native_pearl_motion_in_slot(&mut self, motion: &NativePearlMotion) -> Option<Outcome> {
        let mut info = self.pearl?;
        if info.kind != Kind::Pearl {
            return None;
        }
        if let Some(rect) = motion.rect {
            info.rect = rect;
            self.pearl = Some(info);
        }
        let monitors = self.monitors();
        if motion.moving {
            if let Some(monitor) = motion
                .output
                .as_deref()
                .and_then(|name| monitors.iter().find(|m| m.name == name))
                .or_else(|| {
                    monitors
                        .iter()
                        .find(|m| m.rect.contains(info.rect.centre()))
                })
            {
                let center = info.rect.centre();
                let point = Point {
                    x: center
                        .x
                        .clamp(monitor.rect.x, monitor.rect.x + monitor.rect.w - 1. / 256.),
                    y: center
                        .y
                        .clamp(monitor.rect.y, monitor.rect.y + monitor.rect.h - 1. / 256.),
                };
                let mut drag = Drag::begin(
                    Source::Pearl,
                    self.resident_edge(),
                    &monitor.name,
                    info.rect,
                    point,
                    &monitors,
                    &self.tuning(),
                );
                drag.ready = true;
                drag.magnet = None;
                self.update_guides(&drag, &monitors);
            }
            return None;
        }
        self.end_guides();
        motion.rect?;
        let center = info.rect.centre();
        let monitor = motion
            .output
            .as_deref()
            .and_then(|name| monitors.iter().find(|m| m.name == name))
            .or_else(|| monitors.iter().find(|m| m.rect.contains(center)))
            .or_else(|| monitors.first());
        let Some(monitor) = monitor else {
            return Some(Outcome::Moved { rect: info.rect });
        };
        if let Some(output) = self.output_named(&monitor.name) {
            self.panel_output = Some(output.clone());
            for surface in self.surfaces.iter_mut().filter(|s| s.companion) {
                surface.output = output.clone();
                surface.output_name = monitor.name.clone();
            }
        }
        let point = Point {
            x: center
                .x
                .clamp(monitor.rect.x, monitor.rect.x + monitor.rect.w - 1. / 256.),
            y: center
                .y
                .clamp(monitor.rect.y, monitor.rect.y + monitor.rect.h - 1. / 256.),
        };
        let mut drag = Drag::begin(
            Source::Pearl,
            self.resident_edge(),
            &monitor.name,
            info.rect,
            point,
            &monitors,
            &self.tuning(),
        );
        drag.ready = true;
        Some(drag.release())
    }
    fn slot_of_id(&self, id: u32) -> Option<usize> {
        self.slots.iter().position(|group| group.id() == Some(id))
    }
    fn release_pearl_watch_if_idle(&mut self) {
        if !self
            .surfaces
            .iter()
            .any(|s| s.native.is_some() || s.prepared_native.is_some())
            && !self.slots.iter().any(|g| {
                g.surfaces
                    .iter()
                    .any(|s| s.native.is_some() || s.prepared_native.is_some())
            })
        {
            self.pearl_watch = None;
        }
    }
    /// Dock a pearl to the edge of `output` (its own when none).
    pub(super) fn dock_pearl(&mut self, id: u32, output: Option<&str>) -> Result<(), String> {
        let at = self
            .slots
            .iter()
            .position(|g| g.id() == Some(id) && g.is_pearl())
            .ok_or("no such pearl")?;
        let dropped = self.slots[at]
            .info
            .filter(|info| info.kind == Kind::Pearl)
            .map(|info| info.rect);
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
        self.cancel_prepared_native();
        self.reserved_pearl_id = None;
        self.prewarm_attempted = false;
        drop(g);
        self.pearl = None;
        let spec = self
            .options
            .panel_spec
            .clone()
            .ok_or("pearl lost its view")?;
        self.panel_target = true;
        self.panel_center = None;
        self.panel_dock_transition = dropped.map(|from| DockTransition {
            from,
            animation_id: format!("bead.pearl.dock.{id}"),
            started: false,
        });
        let moved = self.panel_output.as_ref() != Some(&target);
        // Native EGL/wgpu resources must retire before their xdg role. A fresh
        // layer surface also receives fresh idle timers and fractional state.
        let native = self
            .surfaces
            .iter()
            .any(|s| s.companion && s.native.is_some());
        if moved || native || (spec.keyboard_focus && self.dismiss.is_none()) {
            drop(self.dismiss.take());
            let (cover, primary) = self.surfaces.drain(..).partition(|s| s.companion);
            self.surfaces = primary;
            self.panel_cover = Some((self.panel_ui_serial.wrapping_add(1), cover));
            self.panel_output = Some(target.clone());
            if spec.keyboard_focus {
                self.create_dismiss(&target)?;
            }
            let qh = self.qh.clone();
            self.add_output(&qh, target.clone());
        }
        self.panel_ui(UiEvent::PanelDetached(false));
        self.panel_ui(UiEvent::PanelVisibility(true));
        self.set_dismiss_enabled(
            self.panel_ready
                && self
                    .panel_runtime
                    .as_ref()
                    .is_some_and(|source| source.outside_dismiss()),
        )?;
        self.reposition_companions(true);
        self.sync_all_decorations()?;
        self.save_pearls();
        self.release_pearl_watch_if_idle();
        println!("PEARL_DOCKED id={id}");
        Ok(())
    }
    pub(super) fn close_pearl(&mut self, id: u32) -> Result<(), String> {
        if self.gesture.as_ref().is_some_and(|g| g.lifted == Some(id)) {
            self.gesture = None;
            self.end_guides();
        }
        self.in_slot(id, |app| app.release_panel())
            .ok_or("no such pearl")?;
        // Opening a panel wakes the primary view; `release_panel` leaves that to the
        // last panel out, which for a pearl can only be known here.
        self.save_pearls();
        self.release_pearl_watch_if_idle();
        Ok(())
    }
    /// Close every open pearl and forget entries for absent outputs as well.
    pub(super) fn reset_pearls(&mut self) {
        if let Some(gesture) = self.gesture.take() {
            self.on_surface(&gesture.surface, |app| {
                app.cancel_prepared_native();
                app.reserved_pearl_id = None;
                app.prewarm_attempted = false;
            });
        }
        self.end_guides();
        self.pending_pearls.clear();
        let ids: Vec<_> = self
            .slots
            .iter()
            .filter(|g| g.is_pearl())
            .filter_map(Group::id)
            .collect();
        for id in ids {
            let _ = self.close_pearl(id);
        }
        self.save_pearls();
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

    pub(super) fn schedule_pearl_save(&mut self) {
        if self.pearl_save_pending
            || self
                .options
                .pearls
                .as_ref()
                .is_none_or(|p| p.store.is_none())
        {
            return;
        }
        self.pearl_save_pending = true;
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(300)),
            |_, _, app| {
                app.pearl_save_pending = false;
                app.save_pearls();
                TimeoutAction::Drop
            },
        ) {
            self.pearl_save_pending = false;
            eprintln!("PEARL_SAVE_ERROR {error}");
        }
    }

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
                    initial: g
                        .runtime
                        .as_ref()
                        .map_or(serde_json::Value::Null, |source| source.saved_state()),
                    placement: Some(pearl::Placement::exact(i.rect)),
                });
            }
        }
        // Configure callbacks can save while their slot is swapped in. Include
        // that live group as well, otherwise resizing forgets the current pearl.
        if let (Some(info), Some(name), Some(source)) = (
            self.pearl.filter(|info| info.kind == Kind::Pearl),
            self.panel_name.as_ref(),
            self.panel_runtime.as_ref(),
        ) {
            layout.windows.push(pearl::Entry {
                kind: name.clone(),
                origin: self
                    .surfaces
                    .iter()
                    .find(|s| s.companion)
                    .map(|s| s.output_name.clone())
                    .unwrap_or_default(),
                initial: source.saved_state(),
                placement: Some(pearl::Placement::exact(info.rect)),
            });
        }
        if self.pearl_saved.as_ref() == Some(&layout) {
            return;
        }
        // Deliver the covered frame transaction before synchronous persistence.
        // In particular, fsync must not keep a released native frame client-side.
        let _ = self.conn.flush();
        let _store_profile = bead_core::profile::Span::new("pearl_store");
        if let Err(error) = layout.save(&path) {
            eprintln!("PEARL_SAVE_ERROR {error}");
        } else {
            self.pearl_saved = Some(layout);
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
            match self.open_pearl(&entry.kind, Some(&entry.origin), rect) {
                Ok(id) => {
                    self.in_slot(id, |app| app.panel_ui(UiEvent::Restore(entry.initial)));
                }
                Err(error) => eprintln!("PEARL_RESTORE_ERROR view={} {error}", entry.kind),
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
        if self
            .options
            .pearls
            .as_ref()
            .is_some_and(|p| p.native.is_some())
            && self.xdg.is_none()
        {
            return Err("native pearls require xdg-shell".into());
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
        if self.panel_runtime.is_some() && self.pearl.is_none() {
            self.cancel_prepared_native();
            self.reserved_pearl_id = None;
            self.prewarm_attempted = false;
        }
        self.pearl_seq += 1;
        let id = self.pearl_seq;
        let fallback_size = rendering::content_window_rect(&spec.fallback)
            .map(|body| (body[2].max(1) as f64, body[3].max(1) as f64))
            .unwrap_or((spec.fallback.bounds.w as f64, spec.fallback.bounds.h as f64));
        let rect = pearl::recover(rect, fallback_size, &monitor.rect, &self.tuning());
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
            self.become_pearl(true);
        }
        self.swap_group(&mut g);
        started?;
        self.slots.push(g);
        self.save_pearls();
        Ok(id)
    }

    // ---- pointer gestures ----------------------------------------------------

    /// Run `f` with the group that owns `surface` in place (the resident one, or a slot).
    pub(super) fn on_surface<R>(
        &mut self,
        surface: &wl_surface::WlSurface,
        f: impl FnOnce(&mut App) -> R,
    ) -> Option<R> {
        match self.slot_of_surface(surface) {
            Some(id) => self.in_slot(id, f),
            None => self
                .surfaces
                .iter()
                .any(|s| s.wl_surface() == surface)
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
            .find(|s| s.wl_surface() == surface && s.companion)?;
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
            || self.relative_pointer.is_none()
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
            self.on_surface(surface, |app| {
                let subject = app.gesture_subject(surface, local)?;
                let docked_source = subject.1 == Source::Docked;
                let _ = policy::prepare_native_for_drag_press(true, docked_source, || {
                    app.prewarm_drag_target();
                });
                Some(subject)
            })
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
        });
        true
    }
    pub(super) fn gesture_owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.gesture.as_ref().is_some_and(|g| &g.surface == surface)
    }
    pub(super) fn gesture_relative_motion(&mut self, dx: f64, dy: f64) {
        let monitors = self.monitors();
        let Some(gesture) = self.gesture.as_ref() else {
            return;
        };
        // Accelerated deltas are logical surface coordinates. Buffer scale and
        // our own margin commits must never feed back into pointer travel.
        let point = gesture.drag.relative_point(dx, dy, &monitors);
        self.gesture_point(point);
    }
    fn gesture_point(&mut self, point: Point) {
        let Some(mut gesture) = self.gesture.take() else {
            return;
        };
        let monitors = self.monitors();
        let tuning = self.tuning();
        gesture.drag.update(point, &monitors, &tuning);
        if gesture.drag.ready && gesture.lifted.is_none() && gesture.drag.source == Source::Docked {
            // Pulled past the threshold: the panel itself comes along as a pearl.
            let body = gesture.drag.body;
            match self.detach_resident_with_role(body, false) {
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
        }
        if gesture.drag.ready {
            self.update_guides(&gesture.drag, &monitors);
        }
        self.gesture = Some(gesture);
    }
    pub(super) fn gesture_release(&mut self) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        // Hide guides in the same transaction, but retire their workers and
        // allocator pages only after the released panel has been submitted.
        let guide_ids: Vec<_> = self.guide_ids.values().copied().collect();
        for id in guide_ids {
            self.in_slot(id, |app| {
                for surface in app.surfaces.iter().filter(|surface| surface.companion) {
                    surface.layer.wl_surface().attach(None, 0, 0);
                    surface.layer.commit();
                }
            });
        }
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
                let activated = self
                    .in_slot(id, |app| app.activate_prepared_native(rect))
                    .unwrap_or(false);
                if !activated {
                    self.in_slot(id, |app| app.cancel_prepared_native());
                    self.move_pearl(id, rect);
                    self.in_slot(id, |app| {
                        if app.native_pearls_enabled()
                            && let Err(error) = app.promote_native_pearl()
                        {
                            eprintln!("PEARL_NATIVE_ERROR {error}");
                        }
                    });
                }
                self.save_pearls();
            }
            // Never pulled: retire the target created for this drag press.
            (_, None) => {
                self.on_surface(&gesture.surface, |app| {
                    app.cancel_prepared_native();
                    app.reserved_pearl_id = None;
                    app.prewarm_attempted = false;
                });
            }
        }
        self.end_guides();
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
                    initial: g
                        .runtime
                        .as_ref()
                        .map_or(serde_json::Value::Null, |source| source.saved_state()),
                    placement: Some(pearl::Placement::exact(i.rect)),
                });
            }
        }
        self.guide_ids.clear();
        self.guide_last.clear();
        drop(gone);
        self.release_pearl_watch_if_idle();
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    #[test]
    fn attached_cpu_alias_stays_on_the_same_buffer_while_callback_is_pending() {
        let key = policy::PreparedFrameKey {
            generation: 4,
            ui_serial: 8,
            size: (900, 600),
            scale120: 180,
        };
        assert!(cpu_alias_already_attached(
            true,
            Some(key),
            Some(key),
            Some((0, 0))
        ));
        assert!(!cpu_alias_already_attached(
            false,
            Some(key),
            Some(key),
            Some((0, 0))
        ));
        assert!(!cpu_alias_already_attached(
            true,
            Some(key),
            Some(key),
            None
        ));
        assert!(!cpu_alias_already_attached(
            true,
            Some(key),
            Some(policy::PreparedFrameKey {
                ui_serial: 7,
                ..key
            }),
            Some((0, 0))
        ));
        assert!(cpu_warm_callback_current(
            Some((3, 9, key)),
            Some(key),
            Some(key),
            3,
            9,
            key
        ));
        assert!(!cpu_warm_callback_current(
            Some((2, 9, key)),
            Some(key),
            Some(key),
            3,
            9,
            key
        ));
        assert!(!cpu_warm_callback_current(
            Some((3, 8, key)),
            Some(key),
            Some(key),
            3,
            9,
            key
        ));
        assert!(!cpu_warm_callback_current(
            Some((3, 9, key)),
            Some(key),
            Some(policy::PreparedFrameKey {
                generation: 5,
                ..key
            }),
            3,
            9,
            key
        ));
    }

    #[test]
    fn a_released_rect_selects_its_new_output_before_the_origin_fallback() {
        let monitors = [
            Monitor {
                name: "A".into(),
                rect: pearl::Rect::new(0., 0., 1920., 1200.),
                frame: Default::default(),
            },
            Monitor {
                name: "B".into(),
                rect: pearl::Rect::new(1920., 0., 1920., 1200.),
                frame: Default::default(),
            },
        ];
        let moved = pearl::Rect::new(2200., 300., 600., 400.);
        assert_eq!(
            placement_monitor(moved, Some("A"), &monitors).unwrap().name,
            "B"
        );
        let stale = pearl::Rect::new(100000., 100000., 100., 100.);
        assert_eq!(
            placement_monitor(stale, Some("B"), &monitors).unwrap().name,
            "B"
        );
        assert!(placement_monitor(stale, Some("B"), &[]).is_none());
    }

    #[test]
    fn dock_transition_interpolates_monotonically_and_reaches_the_policy_endpoint() {
        let from = pearl::Rect::new(1400., 500., 600., 400.);
        let to = pearl::Rect::new(0., 70., 600., 400.);
        let mut previous = from;
        for progress in [0.25, 0.5, 0.75, 1.0] {
            let current = interpolate_dock_rect(from, to, progress);
            assert!(current.x <= previous.x);
            assert!(current.y <= previous.y);
            previous = current;
        }
        assert_eq!(previous, to);
        assert_eq!(interpolate_dock_rect(from, to, 0.), from);
    }
}

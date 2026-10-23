// SPDX-License-Identifier: MIT OR Apache-2.0
//! Edge-anchored chrome (B0047): reserved screen edges and hover/click lips.
//!
//! The product describes both through [`ShellControl::edge_chrome`]; this module owns the
//! surfaces. A reserved edge is a thin input-less layer surface whose exclusive zone keeps
//! windows off a frame side. A lip is a small strip on an edge that opens a named panel
//! after the pointer rests on it (or on click); the product supplies its artwork as a
//! static SVG per state. Nothing here knows which panels exist or what a frame looks like.
use super::*;
use crate::intent::{Dwell, Gate, Lip, Start, may_dwell};
use std::sync::Arc;

/// What the product wants at the screen edges. Compared by value: a change rebuilds only
/// the surfaces whose geometry differs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeChrome {
    /// Exclusive zones in pixels as `[top, right, bottom, left]`; 0 creates no surface.
    pub reserved: [u32; 4],
    pub lips: Vec<Lip>,
    /// Hover rest time before a lip opens its panel.
    pub hover_delay_ms: u32,
    /// Open on click only (`clickEdgeMenus`).
    pub click_to_open: bool,
}
/// The artwork inputs for one lip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LipState {
    /// Dwell fill, 0..1.
    pub progress: f32,
    /// Its panel is open on this output.
    pub active: bool,
}
/// Static SVG for a lip of the given logical size and state; `None` draws nothing. The
/// SVG's own size must be that logical size.
pub type EdgeArt = Arc<dyn Fn(&Lip, (f32, f32), LipState) -> Option<Arc<[u8]>> + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Reserved(usize),
    Lip(String),
}
#[derive(Clone, Debug, PartialEq)]
struct Geometry {
    anchor: Anchor,
    size: (u32, u32),
    margin: [i32; 4],
    exclusive: i32,
    input: bool,
}
pub(super) struct EdgeSurface {
    output: wl_output::WlOutput,
    output_name: String,
    kind: Kind,
    geometry: Geometry,
    pub(super) layer: LayerSurface,
    viewport: WpViewport,
    fractional: Option<WpFractionalScaleV1>,
    pool: SlotPool,
    buffer: Option<Buffer>,
    size: (u32, u32),
    scale120: u32,
    configured: bool,
    drawn: Option<(u32, (u32, u32), u32)>,
}
impl Drop for EdgeSurface {
    fn drop(&mut self) {
        if let Some(f) = &self.fractional {
            f.destroy();
        }
        self.viewport.destroy();
    }
}
#[derive(Default)]
pub(super) struct EdgeState {
    chrome: EdgeChrome,
    dwell: Dwell,
    gate: Gate,
    /// `output/lip id` of the lip under the pointer.
    hovered: Option<String>,
    /// The lip whose dwell is running.
    dwelling: Option<String>,
    dwell_epoch: u64,
    /// The lip that opened the current panel by hover (a click-opened panel stays open).
    hover_opened: Option<String>,
    panel_hovered: bool,
    exit_epoch: u64,
}
fn key(output: &str, lip: &str) -> String {
    format!("{output}/{lip}")
}
const SIDES: [Edge; 4] = [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left];
/// Pointer leave to panel close for a hover-opened panel (QML's 130-140 ms exit timers).
const EXIT_MS: u64 = 130;
const FRAME: Duration = Duration::from_millis(16);

fn strip_geometry(edge: Edge, extent: u32) -> Geometry {
    let (anchor, size) = match edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, extent)),
        Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, extent)),
        Edge::Left => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (extent, 0)),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (extent, 0)),
    };
    Geometry {
        anchor,
        size,
        margin: [0; 4],
        exclusive: extent as i32,
        input: false,
    }
}
fn lip_geometry(lip: &Lip, output: (i32, i32)) -> Geometry {
    let rect = lip.rect((output.0 as f32, output.1 as f32));
    let (w, h) = (rect.w.round().max(1.) as u32, rect.h.round().max(1.) as u32);
    let (x, y) = (rect.x.round() as i32, rect.y.round() as i32);
    let (anchor, margin) = match lip.edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT, [0, 0, 0, x]),
        Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT, [0, 0, 0, x]),
        Edge::Left => (Anchor::LEFT | Anchor::TOP, [y, 0, 0, 0]),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP, [y, 0, 0, 0]),
    };
    Geometry {
        anchor,
        size: (w, h),
        margin,
        exclusive: -1,
        input: true,
    }
}
use bead_core::Edge;

impl App {
    fn edge_namespace(&self) -> String {
        format!("{}-edge", self.options.namespace)
    }
    /// A lip on the bar's own edge lives inside the bar surface (its hit test uses the
    /// bar's pointer events); every other lip, and every lip while the bar is off, is a
    /// strip surface.
    fn bar_hosts(&self, lip: &Lip) -> bool {
        self.options.bar && !self.bar_off && lip.edge == self.options.edge
    }
    pub(super) fn bar_thickness(&self) -> u32 {
        self.options
            .control
            .as_ref()
            .and_then(|c| c.bar_thickness())
            .unwrap_or(50)
    }
    /// Reconcile edge surfaces with what the product now asks for.
    pub(super) fn sync_edges(&mut self) {
        let Some(control) = self.options.control.clone() else {
            return;
        };
        self.edge.chrome = control.edge_chrome();
        if self.hidden {
            self.edge_surfaces.clear();
            return;
        }
        let chrome = self.edge.chrome.clone();
        let mut wanted: Vec<(wl_output::WlOutput, String, Kind, Geometry)> = Vec::new();
        for output in self.output.outputs() {
            let Some(info) = self.output.info(&output) else {
                continue;
            };
            let (Some(name), Some((w, h))) = (
                info.name.clone(),
                info.logical_size.filter(|(w, h)| *w > 0 && *h > 0),
            ) else {
                continue;
            };
            for (index, edge) in SIDES.into_iter().enumerate() {
                if chrome.reserved[index] > 0 {
                    wanted.push((
                        output.clone(),
                        name.clone(),
                        Kind::Reserved(index),
                        strip_geometry(edge, chrome.reserved[index]),
                    ));
                }
            }
            for lip in chrome.lips.iter().filter(|l| !self.bar_hosts(l)) {
                wanted.push((
                    output.clone(),
                    name.clone(),
                    Kind::Lip(lip.id.clone()),
                    lip_geometry(lip, (w, h)),
                ));
            }
        }
        self.edge_surfaces.retain(|s| {
            wanted
                .iter()
                .any(|(o, _, k, _)| *o == s.output && *k == s.kind)
        });
        for (output, name, kind, geometry) in wanted {
            let existing = self
                .edge_surfaces
                .iter()
                .position(|s| s.output == output && s.kind == kind);
            match existing {
                Some(i) if self.edge_surfaces[i].geometry == geometry => {}
                Some(i) => {
                    let s = &mut self.edge_surfaces[i];
                    s.layer.set_anchor(geometry.anchor);
                    s.layer.set_size(geometry.size.0, geometry.size.1);
                    let [t, r, b, l] = geometry.margin;
                    s.layer.set_margin(t, r, b, l);
                    s.layer.set_exclusive_zone(geometry.exclusive);
                    s.geometry = geometry;
                    s.drawn = None;
                    s.layer.commit();
                }
                None => match self.create_edge(&output, &name, kind, geometry) {
                    Ok(surface) => self.edge_surfaces.push(surface),
                    Err(error) => eprintln!("EDGE_SURFACE_ERROR {error}"),
                },
            }
        }
        self.redraw_lips();
    }
    fn create_edge(
        &mut self,
        output: &wl_output::WlOutput,
        name: &str,
        kind: Kind,
        geometry: Geometry,
    ) -> Result<EdgeSurface, String> {
        let manager = self
            .viewporter
            .as_ref()
            .ok_or("edge chrome needs wp_viewporter")?;
        let surface = self.compositor.create_surface(&self.qh);
        if !geometry.input {
            let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
            surface.set_input_region(Some(empty.wl_region()));
        }
        let viewport = manager.get_viewport(&surface, &self.qh, ());
        let fractional = self
            .fractional
            .as_ref()
            .map(|m| m.get_fractional_scale(&surface, &self.qh, surface.clone()));
        let layer = self.layers.create_layer_surface(
            &self.qh,
            surface,
            Layer::Top,
            Some(&self.edge_namespace()),
            Some(output),
        );
        layer.set_anchor(geometry.anchor);
        layer.set_size(geometry.size.0, geometry.size.1);
        let [t, r, b, l] = geometry.margin;
        layer.set_margin(t, r, b, l);
        layer.set_exclusive_zone(geometry.exclusive);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        let scale120 = self
            .output
            .info(output)
            .map_or(120, |i| i.scale_factor.max(1) as u32 * 120);
        Ok(EdgeSurface {
            output: output.clone(),
            output_name: name.to_owned(),
            kind,
            geometry,
            layer,
            viewport,
            fractional,
            pool: SlotPool::new(4096, &self.shm).map_err(|e| e.to_string())?,
            buffer: None,
            size: (0, 0),
            scale120,
            configured: false,
            drawn: None,
        })
    }
    pub(super) fn edge_configure(&mut self, layer: &LayerSurface, size: (u32, u32)) -> bool {
        let Some(i) = self.edge_surfaces.iter().position(|s| s.layer == *layer) else {
            return false;
        };
        let s = &mut self.edge_surfaces[i];
        s.size = size;
        s.configured = true;
        s.drawn = None;
        self.draw_edge(i);
        true
    }
    pub(super) fn edge_closed(&mut self, layer: &LayerSurface) -> bool {
        let before = self.edge_surfaces.len();
        self.edge_surfaces.retain(|s| s.layer != *layer);
        before != self.edge_surfaces.len()
    }
    pub(super) fn edge_scale(&mut self, surface: &wl_surface::WlSurface, scale120: u32) -> bool {
        let Some(i) = self
            .edge_surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        else {
            return false;
        };
        self.edge_surfaces[i].scale120 = scale120.max(1);
        self.edge_surfaces[i].drawn = None;
        self.draw_edge(i);
        true
    }
    pub(super) fn edge_output_removed(&mut self, output: &wl_output::WlOutput) {
        if let Some(name) = self
            .edge_surfaces
            .iter()
            .find(|s| s.output == *output)
            .map(|s| s.output_name.clone())
        {
            self.edge.gate.forget_prefix(&format!("{name}/"));
            if self
                .edge
                .hovered
                .as_deref()
                .is_some_and(|k| k.starts_with(&format!("{name}/")))
            {
                self.edge.hovered = None;
            }
        }
        self.edge_surfaces.retain(|s| s.output != *output);
    }
    fn lip_named(&self, id: &str) -> Option<Lip> {
        self.edge.chrome.lips.iter().find(|l| l.id == id).cloned()
    }
    /// The lip's panel is open on this output.
    fn lip_active(&self, output: &wl_output::WlOutput, lip: &Lip) -> bool {
        self.panel_target
            && self.panel_name.as_deref() == Some(lip.opens.as_str())
            && self.panel_output.as_ref() == Some(output)
    }
    fn lip_progress(&self, key: &str) -> f32 {
        if self.edge.dwelling.as_deref() == Some(key) {
            self.edge.dwell.progress()
        } else {
            0.
        }
    }
    fn draw_edge(&mut self, index: usize) {
        let s = &self.edge_surfaces[index];
        if !s.configured || s.size.0 == 0 || s.size.1 == 0 {
            return;
        }
        let (size, scale120) = (s.size, s.scale120);
        let svg = match &s.kind {
            Kind::Reserved(_) => None,
            Kind::Lip(id) => self.lip_named(id).and_then(|lip| {
                let state = LipState {
                    progress: self.lip_progress(&key(&s.output_name, id)),
                    active: self.lip_active(&s.output, &lip),
                };
                let art = self.options.edge_art.as_ref()?;
                art(&lip, (size.0 as f32, size.1 as f32), state)
            }),
        };
        // Skip identical frames: the art is a pure function of the state.
        let stamp = svg.as_ref().map_or(0, |s| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            s.hash(&mut h);
            h.finish() as u32
        });
        if self.edge_surfaces[index].drawn == Some((stamp, size, scale120)) {
            return;
        }
        let scale = scale120 as f32 / 120.;
        let (w, h) = if svg.is_some() {
            (
                (size.0 as f32 * scale).ceil() as u32,
                (size.1 as f32 * scale).ceil() as u32,
            )
        } else {
            (1, 1)
        };
        let s = &mut self.edge_surfaces[index];
        let created =
            s.pool
                .create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888);
        let (buffer, bytes) = match created {
            Ok(created) => created,
            Err(error) => {
                eprintln!("EDGE_SURFACE_ERROR {error}");
                return;
            }
        };
        match svg {
            Some(svg) => {
                if let Err(error) = bead_render::svg_into(
                    &svg,
                    ShmTarget {
                        bytes,
                        width: w,
                        height: h,
                        scale,
                        format: PixelFormat::Bgra,
                    },
                ) {
                    eprintln!("EDGE_ART_ERROR {error}");
                    return;
                }
            }
            None => bytes.fill(0),
        }
        s.viewport.set_destination(size.0 as i32, size.1 as i32);
        if buffer.attach_to(s.layer.wl_surface()).is_err() {
            return;
        }
        s.layer.wl_surface().damage_buffer(0, 0, w as i32, h as i32);
        s.layer.commit();
        s.buffer = Some(buffer);
        s.drawn = Some((stamp, size, scale120));
    }
    fn redraw_lips(&mut self) {
        for i in 0..self.edge_surfaces.len() {
            if matches!(self.edge_surfaces[i].kind, Kind::Lip(_)) {
                self.draw_edge(i);
            }
        }
    }
    fn lip_allowed(&self, lip: &Lip) -> bool {
        let Some(control) = &self.options.control else {
            return false;
        };
        let open = self
            .panel_target
            .then_some(self.panel_name.as_deref())
            .flatten();
        control.edge_hover_allowed(open) && control.panel_allowed(&lip.opens)
    }
    /// The pointer is on a lip (enter or motion).
    fn lip_approach(&mut self, output: &str, lip: &Lip, wl_output: Option<&wl_output::WlOutput>) {
        let k = key(output, &lip.id);
        self.edge.exit_epoch = self.edge.exit_epoch.wrapping_add(1);
        self.edge.hovered = Some(k.clone());
        let active = wl_output.is_some_and(|o| self.lip_active(o, lip));
        let chrome = &self.edge.chrome;
        if active
            || self.edge.dwell.waiting()
            || !may_dwell(
                self.pointer_pressed,
                chrome.click_to_open,
                self.lip_allowed(lip),
                self.edge.gate.can_open(&k),
            )
        {
            return;
        }
        let delay = chrome.hover_delay_ms;
        let motion = !bead_core::animation::reduced_motion();
        self.edge.dwelling = Some(k.clone());
        self.edge.dwell_epoch = self.edge.dwell_epoch.wrapping_add(1);
        let epoch = self.edge.dwell_epoch;
        match self.edge.dwell.start(delay, motion) {
            Start::Fire => self.lip_fire(),
            Start::Frames => {
                self.schedule_dwell_frames(epoch);
                self.redraw_lip(&k);
            }
            Start::Timer(ms) => {
                self.schedule_dwell_timer(epoch, ms);
                self.redraw_lip(&k);
            }
        }
    }
    fn schedule_dwell_frames(&mut self, epoch: u64) {
        let mut last = Instant::now();
        let _ = self
            .loop_handle
            .insert_source(Timer::from_duration(FRAME), move |_, _, app| {
                if app.edge.dwell_epoch != epoch {
                    return TimeoutAction::Drop;
                }
                let now = Instant::now();
                let seconds = now.duration_since(last).as_secs_f32();
                last = now;
                if app.edge.dwell.advance(seconds) {
                    app.lip_fire();
                    TimeoutAction::Drop
                } else {
                    if let Some(k) = app.edge.dwelling.clone() {
                        app.redraw_lip(&k);
                    }
                    TimeoutAction::ToDuration(FRAME)
                }
            });
    }
    fn schedule_dwell_timer(&mut self, epoch: u64, ms: u32) {
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(u64::from(ms))),
            move |_, _, app| {
                if app.edge.dwell_epoch == epoch && app.edge.dwell.elapse() {
                    app.lip_fire();
                }
                TimeoutAction::Drop
            },
        );
    }
    /// A dwell completed: open the panel if the pointer is still there and still allowed.
    fn lip_fire(&mut self) {
        let Some(k) = self.edge.dwelling.take() else {
            return;
        };
        self.edge.dwell.cancel();
        let Some((output, id)) = k.split_once('/') else {
            return;
        };
        let (output, id) = (output.to_owned(), id.to_owned());
        let still = self.edge.hovered.as_deref() == Some(k.as_str());
        let Some(lip) = self.lip_named(&id) else {
            return;
        };
        if !still
            || self.edge.chrome.click_to_open
            || !self.lip_allowed(&lip)
            || !self.edge.gate.can_open(&k)
        {
            self.redraw_lip(&k);
            return;
        }
        self.edge.hover_opened = Some(k.clone());
        if let Err(error) = self.request_panel_at(Some(&lip.opens), Some(&output), true, None) {
            eprintln!("EDGE_OPEN_ERROR {error}");
            self.edge.hover_opened = None;
        }
        self.redraw_lip(&k);
    }
    fn redraw_lip(&mut self, k: &str) {
        let Some((output, id)) = k.split_once('/') else {
            return;
        };
        if let Some(i) = self
            .edge_surfaces
            .iter()
            .position(|s| s.output_name == output && s.kind == Kind::Lip(id.to_owned()))
        {
            self.draw_edge(i);
        }
    }
    fn lip_leave(&mut self, output: &str, lip: &Lip) {
        let k = key(output, &lip.id);
        if self.edge.dwelling.as_deref() == Some(k.as_str()) {
            self.edge.dwelling = None;
            self.edge.dwell.cancel();
            self.edge.dwell_epoch = self.edge.dwell_epoch.wrapping_add(1);
        }
        if self.edge.hovered.as_deref() == Some(k.as_str()) {
            self.edge.hovered = None;
        }
        self.edge.gate.rearm(&k);
        self.redraw_lip(&k);
        self.settle_edge_exit();
    }
    fn lip_click(&mut self, output: &str, lip: &Lip, wl_output: &wl_output::WlOutput) {
        let k = key(output, &lip.id);
        if self.options.trace_state {
            println!(
                "EDGE_CLICK lip={k} active={} allowed={}",
                self.lip_active(wl_output, lip),
                self.lip_allowed(lip)
            );
        }
        self.edge.dwelling = None;
        self.edge.dwell.cancel();
        self.edge.dwell_epoch = self.edge.dwell_epoch.wrapping_add(1);
        self.edge.hover_opened = None;
        if !self.lip_allowed(lip) {
            return;
        }
        if self.lip_active(wl_output, lip) {
            self.close_panel();
        } else if let Err(error) = self.request_panel_at(Some(&lip.opens), Some(output), true, None)
        {
            eprintln!("EDGE_OPEN_ERROR {error}");
        }
        self.redraw_lip(&k);
    }
    /// A hover-opened panel closes shortly after the pointer leaves both it and its lip.
    pub(super) fn settle_edge_exit(&mut self) {
        let Some(opened) = self.edge.hover_opened.clone() else {
            return;
        };
        if !self.panel_target
            || self.edge.panel_hovered
            || self.edge.hovered.as_deref() == Some(opened.as_str())
        {
            return;
        }
        self.edge.exit_epoch = self.edge.exit_epoch.wrapping_add(1);
        let epoch = self.edge.exit_epoch;
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(EXIT_MS)),
            move |_, _, app| {
                if app.edge.exit_epoch == epoch
                    && app.edge.hover_opened.as_deref() == Some(opened.as_str())
                    && app.panel_target
                    && !app.edge.panel_hovered
                    && app.edge.hovered.as_deref() != Some(opened.as_str())
                {
                    app.edge.hover_opened = None;
                    app.close_panel();
                }
                TimeoutAction::Drop
            },
        );
    }
    /// The pointer entered or left a companion (panel) surface.
    pub(super) fn edge_panel_hover(&mut self, inside: bool) {
        self.edge.panel_hovered = inside;
        self.edge.exit_epoch = self.edge.exit_epoch.wrapping_add(1);
        if !inside {
            self.settle_edge_exit();
        }
    }
    /// A panel opened or closed: lips redraw their active state; a panel dismissed under a
    /// resting pointer must not reopen until the pointer leaves (`NacreHoverIntent.dismiss`).
    pub(super) fn edges_panel_changed(&mut self, closed: bool) {
        if self.options.control.is_none() {
            return;
        }
        if closed {
            self.edge.hover_opened = None;
            if let Some(k) = self.edge.hovered.clone() {
                self.edge.gate.dismiss(&k);
            }
            self.edge.dwelling = None;
            self.edge.dwell.cancel();
            self.edge.dwell_epoch = self.edge.dwell_epoch.wrapping_add(1);
        }
        self.redraw_lips();
        if !closed {
            // The panel's output is recorded a moment later in the open path.
            let _ = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(30)),
                |_, _, app| {
                    // Surfaces created later stack above earlier ones in a layer: the
                    // panel now covers the lips, so recreate them on top (QML: lips z 3).
                    if app.panel_target {
                        app.edge_surfaces
                            .retain(|s| !matches!(s.kind, Kind::Lip(_)));
                        app.sync_edges();
                    }
                    app.redraw_lips();
                    TimeoutAction::Drop
                },
            );
        }
    }
    /// Pointer events on a lip strip surface. Returns false for any other surface.
    pub(super) fn edge_pointer(&mut self, event: &PointerEvent) -> bool {
        let Some(i) = self
            .edge_surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == &event.surface)
        else {
            return false;
        };
        let s = &self.edge_surfaces[i];
        let Kind::Lip(id) = s.kind.clone() else {
            return true;
        };
        let (output, wl_output) = (s.output_name.clone(), s.output.clone());
        let Some(lip) = self.lip_named(&id) else {
            return true;
        };
        match event.kind {
            PointerEventKind::Enter { serial } => {
                self.cursor_enter_serial = serial;
                if let Some(device) = &self.cursor_device {
                    device.set_shape(serial, wp_cursor_shape_device_v1::Shape::Pointer);
                }
                self.lip_approach(&output, &lip, Some(&wl_output));
            }
            PointerEventKind::Motion { .. } => {
                self.lip_approach(&output, &lip, Some(&wl_output));
            }
            PointerEventKind::Leave { .. } => self.lip_leave(&output, &lip),
            PointerEventKind::Press { button: 0x110, .. } => {
                self.lip_click(&output, &lip, &wl_output)
            }
            _ => {}
        }
        true
    }
    /// Pointer events on the bar surface: a bar-hosted lip counts only where the bar has
    /// no control of its own under the pointer.
    pub(super) fn edge_bar_pointer(&mut self, bar: usize, event: &PointerEvent) {
        if self.edge.chrome.lips.is_empty() || self.options.control.is_none() {
            return;
        }
        let s = &self.surfaces[bar];
        let (output, wl_output) = (s.output_name.clone(), s.output.clone());
        let Some((w, h)) = self
            .output
            .info(&s.output)
            .and_then(|i| i.logical_size)
            .filter(|(w, h)| *w > 0 && *h > 0)
        else {
            return;
        };
        // Surface coordinates to output coordinates for the bar's edge.
        let origin = match self.options.edge {
            Edge::Top | Edge::Left => (0., 0.),
            Edge::Bottom => (0., h as f32 - s.height as f32),
            Edge::Right => (w as f32 - s.width as f32, 0.),
        };
        let (x, y) = (
            event.position.0 as f32 + origin.0,
            event.position.1 as f32 + origin.1,
        );
        let lips: Vec<Lip> = self
            .edge
            .chrome
            .lips
            .iter()
            .filter(|l| self.bar_hosts(l))
            .cloned()
            .collect();
        for lip in lips {
            let k = key(&output, &lip.id);
            let free = !matches!(event.kind, PointerEventKind::Leave { .. })
                && lip.contains((w as f32, h as f32), x, y)
                && self.bar_cursor_is_default(&output, event);
            if free {
                match event.kind {
                    PointerEventKind::Press { button: 0x110, .. } => {
                        self.lip_click(&output, &lip, &wl_output)
                    }
                    _ => self.lip_approach(&output, &lip, Some(&wl_output)),
                }
            } else if self.edge.hovered.as_deref() == Some(k.as_str()) {
                self.lip_leave(&output, &lip);
            }
        }
    }
    fn bar_cursor_is_default(&self, output: &str, event: &PointerEvent) -> bool {
        let source = if self.options.primary_factory.is_some() {
            self.output_sessions
                .get(output)
                .and_then(|s| s.source.as_ref())
        } else {
            self.runtime.as_ref()
        };
        source.is_none_or(|r| {
            r.pointer_cursor(
                event.position.0 as f32 - self.bar_offset(output).0,
                event.position.1 as f32 - self.bar_offset(output).1,
            ) == "default"
        })
    }
    fn bar_offset(&self, output: &str) -> (f32, f32) {
        self.surfaces
            .iter()
            .find(|s| !s.companion && s.output_name == output)
            .map_or((0., 0.), |s| s.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserved_strips_reserve_their_side_only() {
        let left = strip_geometry(Edge::Left, 10);
        assert_eq!(left.anchor, Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM);
        assert_eq!(
            (left.size, left.exclusive, left.input),
            ((10, 0), 10, false)
        );
        let bottom = strip_geometry(Edge::Bottom, 12);
        assert_eq!(bottom.anchor, Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        assert_eq!((bottom.size, bottom.exclusive), ((0, 12), 12));
    }
    #[test]
    fn lip_strips_sit_on_the_lip_rectangle_and_ignore_other_exclusive_zones() {
        let lip = |edge, length, thickness| Lip {
            id: "l".into(),
            edge,
            length,
            thickness,
            opens: "p".into(),
        };
        let left = lip_geometry(&lip(Edge::Left, 144., 10.), (1920, 1200));
        assert_eq!(left.anchor, Anchor::LEFT | Anchor::TOP);
        assert_eq!((left.size, left.margin), ((10, 144), [528, 0, 0, 0]));
        assert!(left.input && left.exclusive == -1);
        let right = lip_geometry(&lip(Edge::Right, 144., 10.), (1920, 1200));
        assert_eq!(right.anchor, Anchor::RIGHT | Anchor::TOP);
        assert_eq!((right.size, right.margin), ((10, 144), [528, 0, 0, 0]));
        let top = lip_geometry(&lip(Edge::Top, 208., 10.), (1920, 1200));
        assert_eq!(top.anchor, Anchor::TOP | Anchor::LEFT);
        assert_eq!((top.size, top.margin), ((208, 10), [0, 0, 0, 856]));
        // Odd output sizes round to whole pixels and never produce a zero-sized strip.
        let odd = lip_geometry(&lip(Edge::Left, 0., 0.), (1919, 1081));
        assert_eq!(odd.size, (1, 1));
    }
}

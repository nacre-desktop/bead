// SPDX-License-Identifier: MIT
//! Optional client side of Bead's compositor-driven plane animation protocol.

use std::sync::{Arc, Mutex};

use bead_render::{
    Rect,
    layer_motion::{LayerMotionPlan, PlaneState},
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, globals::GlobalList,
};

use crate::{
    layer_motion_protocol::bead_layer_motion_manager_v1::BeadLayerMotionManagerV1,
    layer_motion_protocol::bead_layer_motion_v1::{self, BeadLayerMotionV1},
    rendering::layer_motion_completion_uses_cpu_reveal,
    runtime::App,
};

delegate_noop!(App: ignore BeadLayerMotionManagerV1);

/// Events received for a compositor-driven slide.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayerMotionEvent {
    PlaneState { plane: u32, state: PlaneState },
    Done { serial: u32 },
    Cancelled { serial: u32 },
}

/// Registry-bound optional support for compositor-driven layer motion.
pub(crate) struct LayerMotionManager(BeadLayerMotionManagerV1);

struct DispatchData {
    events: Mutex<Vec<LayerMotionEvent>>,
}

impl LayerMotionManager {
    /// Bind the protocol when the compositor advertises it.
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<App>) -> Option<Self> {
        globals.bind(qh, 1..=2, ()).ok().map(Self)
    }

    pub(crate) fn supports_reveal(&self) -> bool {
        self.0.version() >= 2
    }

    /// Create a controller associated with a layer-role surface.
    pub(crate) fn create(
        &self,
        surface: &WlSurface,
        qh: &QueueHandle<App>,
    ) -> LayerMotionController {
        let events = Arc::new(DispatchData {
            events: Mutex::new(Vec::new()),
        });
        LayerMotionController {
            proxy: self.0.get_motion(surface, qh, events.clone()),
            events,
            planes: std::collections::BTreeMap::new(),
            next_serial: 1,
            active_serial: None,
        }
    }
}

impl Drop for LayerMotionManager {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

/// A group controller. Queue a plan, attach its atlas buffer, then commit the
/// associated wl_surface once so buffer and plane state become visible together.
pub(crate) struct LayerMotionController {
    proxy: BeadLayerMotionV1,
    events: Arc<DispatchData>,
    planes: std::collections::BTreeMap<u32, Rect>,
    next_serial: u32,
    active_serial: Option<u32>,
}

impl LayerMotionController {
    /// Queue one synchronized animation request for all planes in the plan.
    /// The caller must attach the plan's atlas buffer and commit the surface.
    pub(crate) fn queue(&mut self, plan: &LayerMotionPlan) -> Option<u32> {
        if plan.planes.len() > 64
            || plan.planes.iter().any(|plane| {
                self.planes
                    .get(&plane.id)
                    .is_some_and(|source| *source != plane.source)
            })
        {
            return None;
        }
        if plan.reveal.is_some() && self.proxy.version() < 2 {
            return None;
        }
        for plane in &plan.planes {
            match self.planes.entry(plane.id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(plane.source);
                    self.proxy.add_plane(
                        plane.id,
                        plane.source.x as f64,
                        plane.source.y as f64,
                        plane.source.w as f64,
                        plane.source.h as f64,
                        plane.from.destination.x as f64,
                        plane.from.destination.y as f64,
                        plane.from.destination.w as f64,
                        plane.from.destination.h as f64,
                        plane.from.clip.x as f64,
                        plane.from.clip.y as f64,
                        plane.from.clip.w as f64,
                        plane.from.clip.h as f64,
                        plane.from.opacity as f64,
                    );
                }
                std::collections::btree_map::Entry::Occupied(_) => {
                    self.proxy.set_plane_from(
                        plane.id,
                        plane.from.destination.x as f64,
                        plane.from.destination.y as f64,
                        plane.from.destination.w as f64,
                        plane.from.destination.h as f64,
                        plane.from.clip.x as f64,
                        plane.from.clip.y as f64,
                        plane.from.clip.w as f64,
                        plane.from.clip.h as f64,
                        plane.from.opacity as f64,
                    );
                }
            }
            self.proxy.set_plane_to(
                plane.id,
                plane.to.destination.x as f64,
                plane.to.destination.y as f64,
                plane.to.destination.w as f64,
                plane.to.destination.h as f64,
                plane.to.clip.x as f64,
                plane.to.clip.y as f64,
                plane.to.clip.w as f64,
                plane.to.clip.h as f64,
                plane.to.opacity as f64,
            );
            let (mode, threshold) = match plane.opacity_mode {
                bead_render::layer_motion::OpacityMode::Linear => {
                    (bead_layer_motion_v1::OpacityMode::Linear, 0)
                }
                bead_render::layer_motion::OpacityMode::OpaqueAbove(threshold) => (
                    bead_layer_motion_v1::OpacityMode::OpaqueAbove,
                    (threshold.clamp(0., 1.) * 1_000_000.).round() as u32,
                ),
            };
            self.proxy.set_plane_opacity_mode(plane.id, mode, threshold);
        }
        if self.proxy.version() >= 2 {
            if let Some(contour) = &plan.reveal {
                let mut static_rects = Vec::with_capacity(contour.static_rects.len() * 16);
                for rect in &contour.static_rects {
                    encode_rect(&mut static_rects, *rect);
                }
                let mut holes = Vec::with_capacity(contour.holes.len() * 16);
                for rect in &contour.holes {
                    encode_rect(&mut holes, *rect);
                }
                let [from_tl, from_tr, from_br, from_bl] = contour.radii_from;
                let [to_tl, to_tr, to_br, to_bl] = contour.radii_to;
                let corner_mode = match contour.corner_mode {
                    bead_render::layer_motion::RevealCornerMode::Circular => {
                        bead_layer_motion_v1::CornerMode::Circular
                    }
                    bead_render::layer_motion::RevealCornerMode::QuadraticCut => {
                        bead_layer_motion_v1::CornerMode::QuadraticCut
                    }
                };
                self.proxy.set_reveal_bounds(
                    contour.output_bounds.x as f64,
                    contour.output_bounds.y as f64,
                    contour.output_bounds.w as f64,
                    contour.output_bounds.h as f64,
                    contour.crop.x as f64,
                    contour.crop.y as f64,
                    contour.crop.w as f64,
                    contour.crop.h as f64,
                );
                self.proxy.set_reveal_geometry(
                    contour.animated_from.x as f64,
                    contour.animated_from.y as f64,
                    contour.animated_from.w as f64,
                    contour.animated_from.h as f64,
                    contour.animated_to.x as f64,
                    contour.animated_to.y as f64,
                    contour.animated_to.w as f64,
                    contour.animated_to.h as f64,
                    from_tl as f64,
                    from_tr as f64,
                    from_br as f64,
                    from_bl as f64,
                    to_tl as f64,
                    to_tr as f64,
                    to_br as f64,
                    to_bl as f64,
                    corner_mode,
                    contour.square_edges,
                    pack_rgba(contour.fill),
                    contour.opacity_plane,
                );
                self.proxy.set_reveal_mode(match contour.mode {
                    bead_render::layer_motion::RevealMode::Visible => {
                        bead_layer_motion_v1::RevealMode::Visible
                    }
                    bead_render::layer_motion::RevealMode::TrackingOnly => {
                        bead_layer_motion_v1::RevealMode::TrackingOnly
                    }
                });
                self.proxy.set_reveal_content_clip(
                    contour.content_clip_from.x as f64,
                    contour.content_clip_from.y as f64,
                    contour.content_clip_from.w as f64,
                    contour.content_clip_from.h as f64,
                    contour.content_clip_to.x as f64,
                    contour.content_clip_to.y as f64,
                    contour.content_clip_to.w as f64,
                    contour.content_clip_to.h as f64,
                );
                self.proxy.set_reveal_rects(static_rects, holes);
                if let Some(rim) = contour.rim {
                    self.proxy.set_reveal_rim(
                        pack_rgba(rim.colors[0]),
                        pack_rgba(rim.colors[1]),
                        pack_rgba(rim.colors[2]),
                        rim.stops[0] as f64,
                        rim.stops[1] as f64,
                        rim.stops[2] as f64,
                        rim.start.0 as f64,
                        rim.start.1 as f64,
                        rim.end.0 as f64,
                        rim.end.1 as f64,
                        rim.width as f64,
                        rim.opacity as f64,
                    );
                } else {
                    self.proxy
                        .set_reveal_rim(0, 0, 0, 0., 1., 1., 0., 0., 0., 0., 0., 0.);
                }
                if let Some(shadow) = contour.shadow {
                    self.proxy.set_reveal_shadow(
                        pack_rgba(shadow.color),
                        shadow.blur as f64,
                        shadow.spread as f64,
                        shadow.offset.0 as f64,
                        shadow.offset.1 as f64,
                    );
                } else {
                    self.proxy.set_reveal_shadow(0, 0., 0., 0., 0.);
                }
            } else {
                self.proxy.set_reveal_bounds(0., 0., 0., 0., 0., 0., 0., 0.);
                self.proxy.set_reveal_geometry(
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    bead_layer_motion_v1::CornerMode::Circular,
                    0,
                    0,
                    0,
                );
                self.proxy
                    .set_reveal_mode(bead_layer_motion_v1::RevealMode::TrackingOnly);
                self.proxy
                    .set_reveal_content_clip(0., 0., 0., 0., 0., 0., 0., 0.);
                self.proxy.set_reveal_rects(Vec::new(), Vec::new());
                self.proxy
                    .set_reveal_rim(0, 0, 0, 0., 1., 1., 0., 0., 0., 0., 0., 0.);
                self.proxy.set_reveal_shadow(0, 0., 0., 0., 0.);
            }
        }
        let curve = match plan.curve {
            bead_core::animation::Curve::EaseOut => bead_layer_motion_v1::Curve::EaseOutCubic,
            bead_core::animation::Curve::EaseInOut => bead_layer_motion_v1::Curve::EaseInOutCubic,
            bead_core::animation::Curve::EmphasizedAccelerate => {
                bead_layer_motion_v1::Curve::EmphasizedAccelerate
            }
            bead_core::animation::Curve::Spring => bead_layer_motion_v1::Curve::Spring,
        };
        self.proxy.set_curve(curve);
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1).max(1);
        self.proxy.animate(
            serial,
            (plan.duration.as_secs_f64() * 1000.)
                .round()
                .clamp(1., u32::MAX as f64) as u32,
            if plan.retarget { 1 } else { 0 },
        );
        self.active_serial = Some(serial);
        Some(serial)
    }

    /// Queue or clear the immutable terminal snapshot in the same commit as
    /// the plan. The caller owns allocation and keeps the SHM buffer alive.
    pub(crate) fn queue_terminal_buffer(
        &mut self,
        buffer: Option<&wayland_client::protocol::wl_buffer::WlBuffer>,
        logical_size: (u32, u32),
    ) {
        if self.proxy.version() >= 2 {
            let (width, height) = if buffer.is_some() {
                (logical_size.0 as f64, logical_size.1 as f64)
            } else {
                (0., 0.)
            };
            self.proxy.set_reveal_terminal_buffer(buffer, width, height);
        }
    }

    /// Reset plane presentation atomically with the next wl_surface commit.
    /// The caller must first attach ordinary replacement content; keep this
    /// controller alive so later animations continue its serial sequence.
    pub(crate) fn reset_presentation(&mut self) {
        drop(self.take_events());
        self.proxy.reset();
        self.planes.clear();
        self.active_serial = None;
    }

    pub(crate) fn retire_completed(&mut self, serial: u32) {
        if self.active_serial == Some(serial) {
            self.active_serial = None;
        }
    }

    pub(crate) fn has_active_serial(&self) -> bool {
        self.active_serial.is_some()
    }

    fn matches(&self, data: &Arc<DispatchData>) -> bool {
        Arc::ptr_eq(&self.events, data)
    }

    pub(crate) fn needs_recreation(&self, plan: &LayerMotionPlan) -> bool {
        self.planes.len() != plan.planes.len()
            || plan
                .planes
                .iter()
                .any(|plane| self.planes.get(&plane.id) != Some(&plane.source))
    }

    /// Drain compositor state and completion events received since the last call.
    pub(crate) fn take_events(&self) -> Vec<LayerMotionEvent> {
        std::mem::take(
            &mut *self
                .events
                .events
                .lock()
                .expect("motion event lock poisoned"),
        )
    }
}

fn encode_rect(bytes: &mut Vec<u8>, rect: Rect) {
    for value in [rect.x, rect.y, rect.w, rect.h] {
        let fixed = (f64::from(value) * 256.).round() as i32;
        bytes.extend_from_slice(&fixed.to_ne_bytes());
    }
}

fn pack_rgba(color: [u8; 4]) -> u32 {
    u32::from_be_bytes(color)
}

impl Drop for LayerMotionController {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x as f32, y as f32, w as f32, h as f32)
}

impl Dispatch<BeadLayerMotionV1, Arc<DispatchData>> for App {
    fn event(
        state: &mut Self,
        _: &BeadLayerMotionV1,
        event: bead_layer_motion_v1::Event,
        data: &Arc<DispatchData>,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let converted = match event {
            bead_layer_motion_v1::Event::PlaneState {
                plane,
                x,
                y,
                width,
                height,
                clip_x,
                clip_y,
                clip_width,
                clip_height,
                opacity,
            } => LayerMotionEvent::PlaneState {
                plane,
                state: PlaneState {
                    destination: rect(x, y, width, height),
                    clip: rect(clip_x, clip_y, clip_width, clip_height),
                    opacity: opacity as f32,
                },
            },
            bead_layer_motion_v1::Event::Done { serial } => LayerMotionEvent::Done { serial },
            bead_layer_motion_v1::Event::Cancelled { serial } => {
                LayerMotionEvent::Cancelled { serial }
            }
        };
        let terminal = match &converted {
            LayerMotionEvent::Done { serial } => Some((*serial, true)),
            LayerMotionEvent::Cancelled { serial } => Some((*serial, false)),
            LayerMotionEvent::PlaneState { .. } => None,
        };
        data.events
            .lock()
            .expect("motion event lock poisoned")
            .push(converted);
        let Some((serial, completed)) = terminal else {
            return;
        };
        let Some(index) = state.surfaces.iter().position(|surface| {
            surface
                .layer_motion
                .as_ref()
                .is_some_and(|motion| motion.matches(data))
        }) else {
            data.events
                .lock()
                .expect("motion event lock poisoned")
                .clear();
            return;
        };
        let current = state.surfaces[index]
            .layer_motion
            .as_ref()
            .is_some_and(|motion| motion.active_serial == Some(serial));
        if current {
            let surface_companion = state.surfaces[index].companion;
            let surface_output = state.surfaces[index].output_name.clone();
            let target_scene = if surface_companion {
                state.companion.as_ref().map(|(scene, _, _)| scene)
            } else if state.options.primary_factory.is_some()
                && state.active_output.as_deref() != Some(surface_output.as_str())
            {
                state
                    .output_sessions
                    .get(&surface_output)
                    .map(|session| &session.scene)
            } else {
                Some(&state.scene)
            };
            let target_motion = target_scene.and_then(|scene| scene.motion.as_ref());
            let target_open = if surface_companion && state.options.panel_spec.is_some() {
                state.panel_target
            } else {
                target_motion.is_some_and(|motion| motion.open)
            };
            let cpu_reveal_handoff = layer_motion_completion_uses_cpu_reveal(
                completed,
                state.surfaces[index].layer_motion_open,
                target_open,
                target_motion.is_some_and(|motion| motion.reveal),
                target_scene.is_some_and(|scene| {
                    crate::rendering::reveal_contour_capable(
                        scene,
                        state.layer_motion_manager.as_ref(),
                    )
                }),
            );
            if cpu_reveal_handoff {
                if let Some(motion) = state.surfaces[index].layer_motion.as_mut() {
                    motion.take_events();
                    motion.retire_completed(serial);
                }
                state.panel_frame_until = None;
                state.surfaces[index].redraw = true;
                state.draw(index, qh);
                return;
            }
            let settled_open = completed
                && state.surfaces[index].layer_motion_open == Some(true)
                && state.surfaces[index].layer_motion_ui_serial == state.surfaces[index].ui_serial
                && (!state.surfaces[index].companion || state.panel_target);
            let reveal_contour_available = target_scene.is_some_and(|scene| {
                target_motion.is_some_and(|motion| motion.reveal)
                    && crate::rendering::reveal_contour_capable(
                        scene,
                        state.layer_motion_manager.as_ref(),
                    )
            });
            if completed && reveal_contour_available && target_open {
                if let Some(motion) = state.surfaces[index].layer_motion.as_mut() {
                    motion.take_events();
                    motion.retire_completed(serial);
                }
                state.panel_frame_until = None;
                state.surfaces[index].layer_motion_active = true;
                if settled_open {
                    // Keep the contour controller and atlas attached as static
                    // compositor presentation. Later transitions can retarget it;
                    // content replacement will reset atomically with fresh pixels.
                    state.surfaces[index].redraw = false;
                    return;
                }
                state.surfaces[index].redraw = true;
                state.draw(index, qh);
                return;
            }
            state.surfaces[index].layer_motion_active = false;
            let surface = state.surfaces[index].layer.wl_surface().clone();
            if let Some(motion) = state.surfaces[index].layer_motion.as_mut() {
                motion.take_events();
            }
            drop(state.surfaces[index].layer_motion.take());
            state.surfaces[index].layer_motion_plan = None;
            state.surfaces[index].layer_motion_resting_offset = None;
            state.panel_frame_until = None;
            if state.surfaces[index].companion && !state.panel_target {
                state.surfaces[index].layer.unmap();
                state.surfaces[index].panel_unmapped = true;
                state.surfaces[index].layer_motion_open = None;
            } else {
                state.surfaces[index].layer_motion = state
                    .layer_motion_manager
                    .as_ref()
                    .map(|manager| manager.create(&surface, qh));
                if settled_open {
                    // The submitted atlas already is the exact open scene. Removing
                    // the identity presentation controller settles it without a
                    // redundant full CPU paint.
                    state.surfaces[index].redraw = false;
                } else {
                    state.draw(index, qh);
                }
            }
        } else {
            // A reset or a replaced serial makes its terminal event stale. Drop
            // its queued plane states too, rather than retaining them forever.
            data.events
                .lock()
                .expect("motion event lock poisoned")
                .clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer_motion_protocol::server::{
        bead_layer_motion_manager_v1::BeadLayerMotionManagerV1 as ServerMotionManager,
        bead_layer_motion_v1::{BeadLayerMotionV1 as ServerMotion, Request as ServerMotionRequest},
    };
    use bead_render::{
        Damage, Motion, PixelFormat, Primitive, Renderer, Result, RevealArt, Scene, ShmTarget,
        layer_motion::{LayerMotionPlan, RevealContour, RevealCornerMode, RevealRim, RevealShadow},
        tree::TreeCpuRenderer,
    };
    use std::{
        os::unix::net::UnixStream,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        thread,
        time::{Duration, Instant},
    };
    use wayland_server::{
        Client, DataInit, Dispatch as ServerDispatch, Display, DisplayHandle, GlobalDispatch, New,
        protocol::{wl_compositor, wl_surface},
    };

    #[test]
    fn reveal_v2_keeps_v1_wire_opcodes_and_wayland_argument_limits() {
        let xml = include_str!("../../../protocols/bead-layer-motion-v1.xml");
        let interface = |name: &str| {
            xml.split(&format!("interface name=\"{name}\""))
                .nth(1)
                .expect("protocol interface")
                .split("</interface>")
                .next()
                .expect("closed protocol interface")
        };
        let names = |interface: &str, element: &str| {
            interface
                .split(&format!("<{element} "))
                .skip(1)
                .map(|entry| {
                    entry
                        .split("name=\"")
                        .nth(1)
                        .and_then(|entry| entry.split('\"').next())
                        .expect("protocol element name")
                        .to_owned()
                })
                .collect::<Vec<_>>()
        };
        let manager = interface("bead_layer_motion_manager_v1");
        assert_eq!(names(manager, "request"), ["destroy", "get_motion"]);
        let controller = interface("bead_layer_motion_v1");
        let requests = names(controller, "request");
        assert_eq!(
            &requests[..9],
            [
                "destroy",
                "add_plane",
                "set_plane_to",
                "set_plane_from",
                "set_curve",
                "set_plane_opacity_mode",
                "animate",
                "cancel",
                "reset",
            ],
            "version 1 request opcodes must not move"
        );
        assert_eq!(
            names(controller, "event"),
            ["plane_state", "done", "cancelled"],
            "version 1 event opcodes must not move"
        );
        assert_eq!(
            requests.last().map(String::as_str),
            Some("set_reveal_terminal_buffer"),
            "terminal buffer request must be appended after existing requests"
        );
        let terminal = controller
            .split("<request name=\"set_reveal_terminal_buffer\"")
            .nth(1)
            .unwrap()
            .split("</request>")
            .next()
            .unwrap();
        assert!(terminal.contains(
            "name=\"buffer\" type=\"object\" interface=\"wl_buffer\" allow-null=\"true\""
        ));
        for request in controller.split("<request ").skip(1) {
            let request = request.split("</request>").next().unwrap();
            assert!(
                request.matches("<arg ").count() <= 20,
                "Wayland request argument limit exceeded: {}",
                request.split("name=\"").nth(1).unwrap_or_default()
            );
        }
        let geometry = controller
            .split("<request name=\"set_reveal_geometry\"")
            .nth(1)
            .unwrap()
            .split("</request>")
            .next()
            .unwrap();
        assert_eq!(geometry.matches("<arg ").count(), 20);
        let shadow = controller
            .split("<request name=\"set_reveal_shadow\"")
            .nth(1)
            .unwrap()
            .split("</request>")
            .next()
            .unwrap();
        assert_eq!(shadow.matches("<arg ").count(), 5);
    }

    #[derive(Default)]
    struct Counts {
        commits: AtomicUsize,
        plane_adds: AtomicUsize,
        animates: AtomicUsize,
        resets: AtomicUsize,
        reveal_bounds: AtomicUsize,
        reveal_geometry: AtomicUsize,
        reveal_clip: AtomicUsize,
        reveal_rects: AtomicUsize,
        reveal_rim: AtomicUsize,
        reveal_shadow: AtomicUsize,
        terminal_buffer: AtomicUsize,
        reveal_mode: AtomicUsize,
        tracking_mode: AtomicUsize,
        request_order: Mutex<Vec<&'static str>>,
    }

    #[derive(Clone)]
    struct FakeState(Arc<Counts>);

    #[derive(Debug)]
    struct FakeClient;

    impl wayland_server::backend::ClientData for FakeClient {}

    impl GlobalDispatch<wl_compositor::WlCompositor, ()> for FakeState {
        fn bind(
            _: &mut Self,
            _: &DisplayHandle,
            _: &Client,
            resource: New<wl_compositor::WlCompositor>,
            _: &(),
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    impl ServerDispatch<wl_compositor::WlCompositor, ()> for FakeState {
        fn request(
            _: &mut Self,
            _: &Client,
            _: &wl_compositor::WlCompositor,
            request: wl_compositor::Request,
            _: &(),
            _: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            if let wl_compositor::Request::CreateSurface { id } = request {
                data_init.init(id, ());
            }
        }
    }

    impl GlobalDispatch<ServerMotionManager, ()> for FakeState {
        fn bind(
            _: &mut Self,
            _: &DisplayHandle,
            _: &Client,
            resource: New<ServerMotionManager>,
            _: &(),
            data_init: &mut DataInit<'_, Self>,
        ) {
            data_init.init(resource, ());
        }
    }

    impl ServerDispatch<ServerMotionManager, ()> for FakeState {
        fn request(
            _: &mut Self,
            _: &Client,
            _: &ServerMotionManager,
            request: crate::layer_motion_protocol::server::bead_layer_motion_manager_v1::Request,
            _: &(),
            _: &DisplayHandle,
            data_init: &mut DataInit<'_, Self>,
        ) {
            if let crate::layer_motion_protocol::server::bead_layer_motion_manager_v1::Request::GetMotion { id, .. } = request {
                data_init.init(id, ());
            }
        }
    }

    impl ServerDispatch<wl_surface::WlSurface, ()> for FakeState {
        fn request(
            state: &mut Self,
            _: &Client,
            _: &wl_surface::WlSurface,
            request: wl_surface::Request,
            _: &(),
            _: &DisplayHandle,
            _: &mut DataInit<'_, Self>,
        ) {
            match request {
                wl_surface::Request::Attach { .. } => {
                    state.0.request_order.lock().unwrap().push("attach")
                }
                wl_surface::Request::Commit => {
                    state.0.request_order.lock().unwrap().push("commit");
                    state.0.commits.fetch_add(1, Ordering::Release);
                }
                _ => {}
            }
        }
    }

    impl ServerDispatch<ServerMotion, ()> for FakeState {
        fn request(
            state: &mut Self,
            _: &Client,
            _: &ServerMotion,
            request: ServerMotionRequest,
            _: &(),
            _: &DisplayHandle,
            _: &mut DataInit<'_, Self>,
        ) {
            match request {
                ServerMotionRequest::AddPlane { .. } => {
                    state.0.plane_adds.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::Animate { .. } => {
                    state.0.animates.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::Reset => {
                    state.0.resets.fetch_add(1, Ordering::Relaxed);
                    state.0.request_order.lock().unwrap().push("reset");
                }
                ServerMotionRequest::SetRevealBounds { .. } => {
                    state.0.reveal_bounds.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealGeometry { .. } => {
                    state.0.reveal_geometry.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealContentClip { .. } => {
                    state.0.reveal_clip.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealRects { .. } => {
                    state.0.reveal_rects.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealRim { .. } => {
                    state.0.reveal_rim.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealShadow { .. } => {
                    state.0.reveal_shadow.fetch_add(1, Ordering::Relaxed);
                }
                ServerMotionRequest::SetRevealMode { mode } => {
                    state.0.reveal_mode.fetch_add(1, Ordering::Relaxed);
                    if matches!(
                        mode,
                        wayland_server::WEnum::Value(
                            crate::layer_motion_protocol::server::bead_layer_motion_v1::RevealMode::TrackingOnly
                        )
                    ) {
                        state.0.tracking_mode.fetch_add(1, Ordering::Relaxed);
                    }
                }
                ServerMotionRequest::SetRevealTerminalBuffer {
                    buffer,
                    width,
                    height,
                } => {
                    assert!(buffer.is_none());
                    assert_eq!((width, height), (0., 0.));
                    state.0.terminal_buffer.fetch_add(1, Ordering::Relaxed);
                    state.0.request_order.lock().unwrap().push("terminal");
                }
                _ => {}
            }
        }
    }

    fn server(
        stream: UnixStream,
        counts: Arc<Counts>,
        motion_version: Option<u32>,
    ) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
        let mut display = Display::<FakeState>::new().unwrap();
        let mut handle = display.handle();
        let state = FakeState(counts);
        handle.create_global::<FakeState, wl_compositor::WlCompositor, _>(6, ());
        if let Some(version) = motion_version {
            handle.create_global::<FakeState, ServerMotionManager, _>(version, ());
        }
        handle
            .insert_client(stream, Arc::new(FakeClient))
            .expect("insert fake client");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                display.dispatch_clients(&mut state.clone()).unwrap();
                display.flush_clients().unwrap();
                thread::sleep(Duration::from_millis(1));
            }
        });
        (thread, stop)
    }

    fn sample_scene() -> Scene {
        Scene {
            opacity: 1.,
            motion: Some(Motion {
                id: "motion-test".into(),
                open: true,
                duration: 0.3,
                return_duration: None,
                return_curve: None,
                spring: false,
                curve: None,
                edge: bead_core::Edge::Bottom,
                slide_distance: Some(32.),
                fade: false,
                reveal: false,
                progress: 1.,
                parts: Vec::new(),
                art: None,
            }),
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: Vec::new(),
            width: 32.,
            slide: None,
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(4., 4., 24., 24.),
                radius: 3.,
                fill: [35, 80, 120, 255],
                border: None,
            }],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 32., 32.),
        }
    }

    struct CountPaint {
        inner: TreeCpuRenderer,
        paints: usize,
    }

    impl Renderer for CountPaint {
        fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
            self.inner.prepare(scene, scale)
        }
        fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, damage: Damage) -> Result<()> {
            self.paints += 1;
            self.inner.render(scene, target, damage)
        }
    }

    #[test]
    fn one_paint_and_one_group_request_reach_the_wayland_dispatch_queue() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts.clone(), Some(1));
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        let qh = queue.handle();
        let manager = LayerMotionManager::bind(&globals, &qh).expect("advertised manager");
        let compositor = smithay_client_toolkit::compositor::CompositorState::bind(&globals, &qh)
            .expect("advertised compositor");
        let surface = compositor.create_surface(&qh);
        let mut renderer = CountPaint {
            inner: TreeCpuRenderer::default(),
            paints: 0,
        };
        let mut plan = LayerMotionPlan::for_slide(
            &sample_scene(),
            false,
            32,
            32,
            1.,
            PixelFormat::Rgba,
            &mut renderer,
        )
        .expect("prepare slide plan")
        .expect("supported slide");
        plan.atlas.take_pixels();
        assert_eq!(renderer.paints, 1);
        let mut controller = manager.create(&surface, &qh);
        controller.queue_terminal_buffer(None, (0, 0));
        controller.queue(&plan).expect("one valid plane");
        surface.commit();
        connection.flush().expect("flush requests");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.animates.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(counts.plane_adds.load(Ordering::Relaxed), 1);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 1);
        assert_eq!(counts.commits.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_bounds.load(Ordering::Relaxed), 0);
        assert_eq!(counts.reveal_geometry.load(Ordering::Relaxed), 0);
        assert_eq!(counts.terminal_buffer.load(Ordering::Relaxed), 0);
        for plane in &mut plan.planes {
            std::mem::swap(&mut plane.from, &mut plane.to);
        }
        plan.retarget = true;
        controller.queue_terminal_buffer(None, (0, 0));
        controller.queue(&plan).expect("retarget same atlas");
        surface.commit();
        connection.flush().expect("flush retarget");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.animates.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(renderer.paints, 1, "retarget reuses the existing atlas");
        assert_eq!(counts.plane_adds.load(Ordering::Relaxed), 1);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 2);
        assert_eq!(counts.commits.load(Ordering::Relaxed), 2);
        assert_eq!(counts.terminal_buffer.load(Ordering::Relaxed), 0);
        drop(controller);
        drop(surface);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }

    #[test]
    fn reveal_descriptor_is_submitted_once_and_style_edits_reuse_the_controller() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts.clone(), Some(2));
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        let qh = queue.handle();
        let manager = LayerMotionManager::bind(&globals, &qh).expect("advertised manager v2");
        assert!(manager.supports_reveal());
        let compositor = smithay_client_toolkit::compositor::CompositorState::bind(&globals, &qh)
            .expect("advertised compositor");
        let surface = compositor.create_surface(&qh);
        let mut scene = sample_scene();
        let body = Rect::new(4., 4., 24., 24.);
        let primitive = scene.primitives[0].clone();
        let motion = scene.motion.as_mut().unwrap();
        motion.reveal = true;
        motion.parts = vec![(0, primitive)];
        motion.art = Some(RevealArt {
            body,
            index: 0,
            contour: Some(RevealContour {
                output_bounds: Rect::new(0., 0., 32., 32.),
                crop: Rect::new(0., 0., 32., 32.),
                static_rects: vec![Rect::new(0., 0., 32., 4.)],
                holes: Vec::new(),
                animated_from: Rect::new(4., 28., 24., 0.),
                animated_to: body,
                content_clip_from: Rect::new(-4096., -4096., 8224., 8224.),
                content_clip_to: Rect::new(0., 0., 32., 32.),
                radii_from: [0.; 4],
                radii_to: [3.; 4],
                corner_mode: RevealCornerMode::QuadraticCut,
                mode: bead_render::layer_motion::RevealMode::Visible,
                opacity_plane: 0,
                square_edges: 2,
                fill: [35, 80, 120, 255],
                rim: Some(RevealRim {
                    colors: [[100, 130, 180, 255]; 3],
                    stops: [0., 0.38, 1.],
                    start: (0., 0.),
                    end: (0., 32.),
                    width: 1.,
                    opacity: 0.25,
                }),
                // This compound shape has fixed rectangles and quadratic-cut
                // corners; the protocol permits shadows only with a plain
                // circular rounded contour.
                shadow: None,
            }),
            draw: Arc::new(|_, _| panic!("compositor contour skips per-frame art draw")),
        });
        let mut renderer = CountPaint {
            inner: TreeCpuRenderer::default(),
            paints: 0,
        };
        let mut plan =
            LayerMotionPlan::for_slide(&scene, true, 32, 32, 1., PixelFormat::Rgba, &mut renderer)
                .expect("prepare reveal plan")
                .expect("supported descriptor");
        assert_eq!(
            renderer.paints, 2,
            "visible reveal captures atlas and endpoint"
        );
        let mut controller = manager.create(&surface, &qh);
        controller.queue_terminal_buffer(None, (0, 0));
        assert!(controller.queue(&plan).is_some());
        surface.commit();
        connection.flush().expect("flush reveal request");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.animates.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(counts.reveal_bounds.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_geometry.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_clip.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_rects.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_rim.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_shadow.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_mode.load(Ordering::Relaxed), 1);
        assert_eq!(counts.tracking_mode.load(Ordering::Relaxed), 0);
        assert_eq!(counts.terminal_buffer.load(Ordering::Relaxed), 1);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 1);

        plan.reveal.as_mut().unwrap().rim.as_mut().unwrap().opacity = 0.16;
        assert!(!controller.needs_recreation(&plan));
        plan.retarget = true;
        assert!(controller.queue(&plan).is_some());
        surface.commit();
        connection.flush().expect("flush style update");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.animates.load(Ordering::Relaxed) < 2 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(renderer.paints, 2);
        assert_eq!(counts.plane_adds.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_rim.load(Ordering::Relaxed), 2);
        assert_eq!(counts.reveal_mode.load(Ordering::Relaxed), 2);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 2);
        assert_eq!(counts.terminal_buffer.load(Ordering::Relaxed), 1);
        assert_eq!(counts.tracking_mode.load(Ordering::Relaxed), 0);
        drop(controller);
        drop(surface);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }

    #[test]
    fn whole_scene_reveal_close_sends_tracking_only_contour_once() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts.clone(), Some(2));
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        let qh = queue.handle();
        let manager = LayerMotionManager::bind(&globals, &qh).expect("advertised manager v2");
        let compositor = smithay_client_toolkit::compositor::CompositorState::bind(&globals, &qh)
            .expect("advertised compositor");
        let surface = compositor.create_surface(&qh);
        let mut scene = sample_scene();
        scene.motion.as_mut().unwrap().reveal = true;
        let mut renderer = TreeCpuRenderer::default();
        let plan =
            LayerMotionPlan::for_slide(&scene, false, 32, 32, 1., PixelFormat::Rgba, &mut renderer)
                .unwrap()
                .unwrap();
        assert_eq!(
            plan.reveal.as_ref().unwrap().mode,
            bead_render::layer_motion::RevealMode::TrackingOnly
        );
        let mut controller = manager.create(&surface, &qh);
        assert!(controller.queue(&plan).is_some());
        surface.commit();
        connection.flush().expect("flush close tracker");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.animates.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(counts.tracking_mode.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_geometry.load(Ordering::Relaxed), 1);
        assert_eq!(counts.reveal_clip.load(Ordering::Relaxed), 1);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 1);
        drop(controller);
        drop(surface);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }

    #[test]
    fn reset_is_staged_after_replacement_attach_and_reuses_serial_sequence() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts.clone(), Some(1));
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        let qh = queue.handle();
        let manager = LayerMotionManager::bind(&globals, &qh).expect("advertised manager");
        let compositor = smithay_client_toolkit::compositor::CompositorState::bind(&globals, &qh)
            .expect("advertised compositor");
        let surface = compositor.create_surface(&qh);
        let mut renderer = CountPaint {
            inner: TreeCpuRenderer::default(),
            paints: 0,
        };
        let mut plan = LayerMotionPlan::for_slide(
            &sample_scene(),
            false,
            32,
            32,
            1.,
            PixelFormat::Rgba,
            &mut renderer,
        )
        .expect("prepare slide plan")
        .expect("supported slide");
        plan.atlas.take_pixels();
        let mut controller = manager.create(&surface, &qh);
        assert_eq!(controller.queue(&plan), Some(1));
        surface.commit();
        connection.flush().expect("flush initial request");
        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.commits.load(Ordering::Acquire) < 1 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(counts.commits.load(Ordering::Acquire), 1);
        assert_eq!(counts.animates.load(Ordering::Relaxed), 1);
        assert!(controller.has_active_serial());
        controller.retire_completed(1);
        assert!(!controller.has_active_serial());
        counts.request_order.lock().unwrap().clear();

        // No replacement slot is needed in this wire-level test; a null attach
        // stands in for the already-acquired replacement buffer. Production
        // queues reset only after a real buffer attach succeeds.
        surface.attach(None, 0, 0);
        controller
            .events
            .events
            .lock()
            .unwrap()
            .push(LayerMotionEvent::Done { serial: 1 });
        controller.reset_presentation();
        assert!(controller.planes.is_empty());
        assert_eq!(controller.active_serial, None);
        assert_eq!(controller.next_serial, 2);
        assert!(controller.take_events().is_empty());
        surface.commit();
        connection.flush().expect("flush reset commit");

        let deadline = Instant::now() + Duration::from_secs(2);
        while counts.commits.load(Ordering::Acquire) < 2 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(counts.commits.load(Ordering::Acquire), 2);
        assert_eq!(counts.resets.load(Ordering::Relaxed), 1);
        assert_eq!(
            *counts.request_order.lock().unwrap(),
            ["attach", "reset", "commit"]
        );
        assert_eq!(controller.queue(&plan), Some(2));
        drop(controller);
        drop(surface);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }

    #[test]
    fn absent_global_keeps_the_stock_compositor_path_available() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts, None);
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, _queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        assert!(LayerMotionManager::bind(&globals, &_queue.handle()).is_none());
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }
}

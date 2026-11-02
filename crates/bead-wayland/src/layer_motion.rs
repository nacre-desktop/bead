// SPDX-License-Identifier: MIT
//! Optional client side of Bead's compositor-driven plane animation protocol.

use std::sync::{Arc, Mutex};

use bead_render::{
    Rect,
    layer_motion::{LayerMotionPlan, PlaneState},
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList};

use crate::{
    layer_motion_protocol::bead_layer_motion_manager_v1::BeadLayerMotionManagerV1,
    layer_motion_protocol::bead_layer_motion_v1::{self, BeadLayerMotionV1},
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
        globals.bind(qh, 1..=1, ()).ok().map(Self)
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
        let terminal_serial = match &converted {
            LayerMotionEvent::Done { serial } | LayerMotionEvent::Cancelled { serial } => {
                Some(*serial)
            }
            LayerMotionEvent::PlaneState { .. } => None,
        };
        data.events
            .lock()
            .expect("motion event lock poisoned")
            .push(converted);
        let Some(serial) = terminal_serial else {
            return;
        };
        let Some(index) = state.surfaces.iter().position(|surface| {
            surface
                .layer_motion
                .as_ref()
                .is_some_and(|motion| motion.matches(data))
        }) else {
            return;
        };
        let current = state.surfaces[index]
            .layer_motion
            .as_ref()
            .is_some_and(|motion| motion.active_serial == Some(serial));
        if current {
            state.surfaces[index].layer_motion_active = false;
            let surface = state.surfaces[index].layer.wl_surface().clone();
            if let Some(motion) = state.surfaces[index].layer_motion.as_mut() {
                motion.take_events();
            }
            drop(state.surfaces[index].layer_motion.take());
            state.surfaces[index].layer_motion_plan = None;
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
                state.draw(index, qh);
            }
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
        Damage, Motion, PixelFormat, Primitive, Renderer, Result, Scene, ShmTarget,
        layer_motion::LayerMotionPlan, tree::TreeCpuRenderer,
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

    #[derive(Default)]
    struct Counts {
        commits: AtomicUsize,
        plane_adds: AtomicUsize,
        animates: AtomicUsize,
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
            if matches!(request, wl_surface::Request::Commit) {
                state.0.commits.fetch_add(1, Ordering::Relaxed);
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
                _ => {}
            }
        }
    }

    fn server(
        stream: UnixStream,
        counts: Arc<Counts>,
        advertise_motion: bool,
    ) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
        let mut display = Display::<FakeState>::new().unwrap();
        let mut handle = display.handle();
        let state = FakeState(counts);
        handle.create_global::<FakeState, wl_compositor::WlCompositor, _>(6, ());
        if advertise_motion {
            handle.create_global::<FakeState, ServerMotionManager, _>(1, ());
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
        let (server_thread, stop) = server(server_stream, counts.clone(), true);
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
        for plane in &mut plan.planes {
            std::mem::swap(&mut plane.from, &mut plane.to);
        }
        plan.retarget = true;
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
        drop(controller);
        drop(surface);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }

    #[test]
    fn absent_global_keeps_the_stock_compositor_path_available() {
        let counts = Arc::new(Counts::default());
        let (client_stream, server_stream) = UnixStream::pair().unwrap();
        let (server_thread, stop) = server(server_stream, counts, false);
        let connection = wayland_client::Connection::from_socket(client_stream).unwrap();
        let (globals, _queue) = wayland_client::globals::registry_queue_init::<App>(&connection)
            .expect("registry roundtrip");
        assert!(LayerMotionManager::bind(&globals, &_queue.handle()).is_none());
        stop.store(true, Ordering::Relaxed);
        server_thread.join().unwrap();
    }
}

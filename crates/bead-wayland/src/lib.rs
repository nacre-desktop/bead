// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor-neutral layer surfaces with fractional scaling and reusable SHM.
mod accessibility;
use bead_render::{
    Damage, PixelFormat, Rect, RenderTarget, Renderer, RendererKind, Scene, ShmTarget,
};
use calloop::{
    EventLoop, LoopSignal,
    signals::{Signal, Signals},
    timer::{TimeoutAction, Timer},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::calloop_wayland_source::WaylandSource,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use std::{
    error::Error,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::{self, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    presentation_time::client::{
        wp_presentation::{self, WpPresentation},
        wp_presentation_feedback::{self, WpPresentationFeedback},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
#[derive(Clone, Debug)]
pub enum UiEvent {
    Reload,
    Desktop(std::sync::Arc<bead_services::Snapshot>),
    Backend(std::sync::Arc<bead_backends::Snapshot>),
    Relayout,
    Hover {
        x: f32,
        y: f32,
    },
    Command {
        command: String,
        shift: bool,
    },
    Accessibility {
        target: u64,
        action: String,
        value: Option<String>,
    },
    Ime {
        preedit: String,
        commit: Option<String>,
        before: u32,
        after: u32,
    },
    WatchError(String),
    ContextMenu {
        x: f32,
        y: f32,
    },
    Click {
        x: f32,
        y: f32,
    },
    Scroll {
        x: f32,
        y: f32,
        delta: f64,
    },
    Key {
        text: String,
        backspace: bool,
    },
}
/// Candidate state is committed only after render resources have been prepared.
#[cfg(feature = "gpu")]
#[derive(Debug)]
struct GpuUnavailable(String);
#[cfg(feature = "gpu")]
impl std::fmt::Display for GpuUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
#[cfg(feature = "gpu")]
impl Error for GpuUnavailable {}
pub trait SceneSource {
    fn services(&mut self, _wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {}
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        None
    }
    fn pointer_cursor(&self, _x: f32, _y: f32) -> &str {
        "default"
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        None
    }
    fn viewport(&mut self, _width: u32, _height: u32) -> bool {
        false
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>>;
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>>;
    fn validate(&self) -> Result<(), Box<dyn Error>>;
    fn state_summary(&self) -> String;
    fn resource_counts(&self) -> String {
        String::new()
    }
    fn commit(&mut self);
    fn error_scene(&mut self, error: String) -> Scene;
}
#[derive(Clone)]
pub struct Options {
    pub namespace: String,
    pub animate: bool,
    pub damage: bool,
    pub force_argb: bool,
    pub seconds: Option<f64>,
    pub bar: bool,
    pub panel: bool,
    pub content_size: bool,
    pub renderer: RendererKind,
    pub output: Option<String>,
    pub auto_fallback: bool,
    pub started: Instant,
    pub start_open: bool,
    pub memory_usage: bool,
    pub surface_count: usize,
    /// Distinct passive companion scene, sharing this process and GPU transport.
    pub companion: Option<(Scene, RendererKind)>,
    pub interactive: bool,
    pub keyboard_focus: bool,
    pub test_keyboard: bool,
    /// Click-through proof surfaces that reserve no desktop space.
    pub test_passive: bool,
    pub trace_state: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            namespace: "bead-dev".into(),
            animate: false,
            damage: true,
            force_argb: false,
            seconds: None,
            bar: false,
            panel: false,
            content_size: false,
            renderer: RendererKind::Cpu,
            output: None,
            auto_fallback: false,
            started: Instant::now(),
            start_open: false,
            memory_usage: false,
            surface_count: 1,
            companion: None,
            interactive: false,
            keyboard_focus: true,
            test_keyboard: false,
            test_passive: false,
            trace_state: false,
        }
    }
}
#[derive(Clone)]
struct PresentationData {
    update: u64,
    surface: wl_surface::WlSurface,
    generation: u64,
}
struct Reusable {
    buffer: Buffer,
    pending: Damage,
}
struct Surface {
    companion: bool,
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    #[cfg(feature = "gpu")]
    gpu: Option<bead_render::gpu::Surface>,
    #[cfg(feature = "gpu")]
    gl: Option<bead_render::gl::Surface>,
    buffers: Vec<Reusable>,
    width: u32,
    height: u32,
    scale120: u32,
    configured: bool,
    ready: bool,
    frame_pending: bool,
    redraw: bool,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    geometry: (i32, i32),
    offset: (f32, f32),
    format: PixelFormat,
    previous: Option<Rect>,
    allocations: usize,
    frames: usize,
    render_ms: Vec<f64>,
    acquire_ms: Vec<f64>,
    swap_ms: Vec<f64>,
    activate_ms: Vec<f64>,
    callbacks_ms: Vec<f64>,
    last_callback: Option<Instant>,
    presented_ms: Vec<f64>,
    last_presented: Option<u64>,
    refresh_ns: u32,
    presentation_kind: u32,
    discarded: usize,
    presented_generation: Option<u64>,
}
impl Drop for Surface {
    fn drop(&mut self) {
        #[cfg(feature = "gpu")]
        drop(self.gpu.take());
        #[cfg(feature = "gpu")]
        drop(self.gl.take());
        if let Some(f) = &self.fractional {
            f.destroy();
        }
        if let Some(v) = &self.viewport {
            v.destroy();
        }
    }
}
struct App {
    runtime: Option<Box<dyn SceneSource>>,
    seat: SeatState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard_surface: Option<wl_surface::WlSurface>,
    modifiers: Modifiers,
    accessibility: Option<accessibility::Bridge>,
    cursor_manager: Option<WpCursorShapeManagerV1>,
    cursor_device: Option<WpCursorShapeDeviceV1>,
    cursor_enter_serial: u32,
    text_input_manager: Option<ZwpTextInputManagerV3>,
    text_input: Option<ZwpTextInputV3>,
    ime_surface: Option<wl_surface::WlSurface>,
    ime_pending: (String, Option<String>, u32, u32),
    ime_last: Option<(String, i32, i32)>,
    reload_pending: bool,
    ui_initialized: bool,
    ui_serial: u64,
    qh: QueueHandle<App>,
    #[cfg(feature = "gpu")]
    gpu: Option<bead_render::gpu::Transport>,
    #[cfg(feature = "gpu")]
    gl: Option<bead_render::gl::Transport>,
    #[cfg(feature = "gpu")]
    conn: Connection,
    registry: RegistryState,
    output: OutputState,
    compositor: CompositorState,
    layers: LayerShell,
    shm: Shm,
    fractional: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    presentation: Option<WpPresentation>,
    surfaces: Vec<Surface>,
    options: Options,
    signal: LoopSignal,
    failure: Option<String>,
    started: Instant,
    scene: Scene,
    renderer: Box<dyn Renderer>,
    companion: Option<(Scene, Box<dyn Renderer>, RendererKind)>,
    animations: bead_core::animation::Animations,
}
pub fn run(options: Options, scene: Scene) -> Result<(), Box<dyn Error>> {
    let renderer = bead_render::renderer(options.renderer)?;
    run_with_renderer(options, scene, renderer)
}
/// Shell widgets supply Scene commands; they never call the rasterizer.
pub fn run_with_renderer(
    options: Options,
    scene: Scene,
    renderer: Box<dyn Renderer>,
) -> Result<(), Box<dyn Error>> {
    run_runtime(options, scene, renderer, None)
}
pub fn run_runtime(
    options: Options,
    scene: Scene,
    renderer: Box<dyn Renderer>,
    runtime: Option<Box<dyn SceneSource>>,
) -> Result<(), Box<dyn Error>> {
    if options.namespace.is_empty()
        || options.namespace.len() > 128
        || options.namespace.contains('\0')
    {
        return Err("invalid layer namespace".into());
    }
    let started = options.started;
    let conn = Connection::connect_to_env()?;
    let (globals, events) = registry_queue_init(&conn)?;
    let qh = events.handle();
    let mut loop_: EventLoop<App> = EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), events).insert(loop_.handle())?;
    loop_.handle().insert_source(
        Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?,
        |_, _, app| app.signal.stop(),
    )?;
    let companion = options
        .companion
        .as_ref()
        .map(|(s, k)| bead_render::renderer(*k).map(|r| (s.clone(), r, *k)))
        .transpose()?;
    let mut app = App {
        runtime,
        seat: SeatState::new(&globals, &qh),
        keyboard: None,
        pointer: None,
        keyboard_surface: None,
        modifiers: Modifiers::default(),
        accessibility: None,
        cursor_manager: globals.bind(&qh, 1..=1, ()).ok(),
        cursor_device: None,
        cursor_enter_serial: 0,
        text_input_manager: globals.bind(&qh, 1..=1, ()).ok(),
        text_input: None,
        ime_surface: None,
        ime_pending: Default::default(),
        ime_last: None,
        reload_pending: false,
        ui_initialized: false,
        ui_serial: 0,
        qh: qh.clone(),
        #[cfg(feature = "gpu")]
        gpu: None,
        #[cfg(feature = "gpu")]
        gl: None,
        #[cfg(feature = "gpu")]
        conn: conn.clone(),
        registry: RegistryState::new(&globals),
        output: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh)?,
        layers: LayerShell::bind(&globals, &qh)?,
        shm: Shm::bind(&globals, &qh)?,
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        presentation: globals.bind(&qh, 1..=2, ()).ok(),
        surfaces: Vec::new(),
        options,
        signal: loop_.get_signal(),
        failure: None,
        started,
        scene,
        renderer,
        companion,
        animations: Default::default(),
    };
    println!(
        "START pid={} namespace={} damage={} direct_shm=true buffers_max=3",
        std::process::id(),
        app.options.namespace,
        app.options.damage
    );
    if let Some(seconds) = app.options.seconds {
        loop_.handle().insert_source(
            Timer::from_duration(Duration::from_secs_f64(seconds)),
            |_, _, app| {
                app.signal.stop();
                TimeoutAction::Drop
            },
        )?;
    }
    if app.runtime.is_some() {
        let (sender, receiver) = calloop::channel::channel::<UiEvent>();
        loop_.handle().insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(event) = event {
                app.ui(event);
            }
        })?;
        if let Some(runtime) = &mut app.runtime {
            let service_sender = sender.clone();
            runtime.services(Box::new(move |event| {
                let _ = service_sender.send(event);
            }));
        }
        app.accessibility = Some(accessibility::Bridge::new(sender));
    }
    if let Some(runtime) = &mut app.runtime {
        let (sender, channel) = calloop::channel::channel::<Result<(), String>>();
        runtime.watch(Box::new(move |event| {
            let _ = sender.send(event);
        }))?;
        let handle = loop_.handle();
        let timer_handle = handle.clone();
        handle.insert_source(channel, move |event, _, app| {
            if let calloop::channel::Event::Msg(result) = event {
                if let Err(error) = result {
                    app.ui(UiEvent::WatchError(error));
                } else if !app.reload_pending {
                    app.reload_pending = true;
                    if let Err(error) = timer_handle.insert_source(
                        Timer::from_duration(Duration::from_millis(50)),
                        |_, _, app| {
                            app.reload_pending = false;
                            app.ui(UiEvent::Reload);
                            TimeoutAction::Drop
                        },
                    ) {
                        app.fail(error);
                    }
                }
            }
        })?;
    }
    for output in app.output.outputs() {
        app.add_output(&qh, output);
    }
    let result = loop_.run(None, &mut app, |_| {});
    for s in &app.surfaces {
        println!(
            "ACQUIRE p50_ms={:.3} p99_ms={:.3}",
            percentile(&s.acquire_ms, 0.5),
            percentile(&s.acquire_ms, 0.99)
        );
        println!(
            "METRICS companion={} output={} frames={} allocations={} render_p50_ms={:.3} render_p99_ms={:.3} callback_p50_ms={:.3} callback_p99_ms={:.3} presented_p50_ms={:.3} presented_p99_ms={:.3} presented_samples={} refresh_ns={} kind={} discarded={} swap_p50_ms={:.3} swap_p99_ms={:.3} swap_max_ms={:.3} activate_p99_ms={:.3}",
            s.companion,
            s.output.id().protocol_id(),
            s.frames,
            s.allocations,
            percentile(&s.render_ms, 0.50),
            percentile(&s.render_ms, 0.99),
            percentile(&s.callbacks_ms, 0.50),
            percentile(&s.callbacks_ms, 0.99),
            percentile(&s.presented_ms, 0.50),
            percentile(&s.presented_ms, 0.99),
            s.presented_ms.len(),
            s.refresh_ns,
            s.presentation_kind,
            s.discarded,
            percentile(&s.swap_ms, 0.50),
            percentile(&s.swap_ms, 0.99),
            s.swap_ms
                .iter()
                .copied()
                .reduce(f64::max)
                .unwrap_or(f64::NAN),
            percentile(&s.activate_ms, 0.99)
        );
    }
    // Destroy GPU-owned renderer objects while their context/surfaces are alive.
    app.renderer = bead_render::renderer(RendererKind::Cpu)?;
    app.companion = None;
    app.surfaces.clear();
    let _ = conn.flush();
    println!("STOP surfaces=0");
    result?;
    if let Some(e) = app.failure {
        return Err(e.into());
    }
    Ok(())
}
fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: initialized writable timespec for CLOCK_MONOTONIC.
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    let mut v = samples.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).ceil() as usize]
}
impl App {
    fn ui(&mut self, event: UiEvent) {
        let Some(mut runtime) = self.runtime.take() else {
            return;
        };
        let reload = matches!(event, UiEvent::Reload);
        let started = Instant::now();
        if self.options.trace_state
            && let UiEvent::Backend(snapshot) = &event
        {
            println!(
                "UI_EVENT monotonic_ns={} title={}",
                monotonic_ns(),
                snapshot.focused_window().map_or("", |w| w.title.as_str())
            );
        }
        let staged = runtime.stage(event).and_then(|scene| {
            if let Some(scene) = &scene {
                let indices: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.configured && !s.companion)
                    .map(|(i, _)| i)
                    .collect();
                for i in indices {
                    #[cfg(feature = "gpu")]
                    if self.renderer.wants_gpu()
                        && let Err(error) = self.ensure_gpu(i)
                    {
                        if !self.options.auto_fallback {
                            return Err(error);
                        }
                        eprintln!("FALLBACK GPU unavailable during staging: {error}");
                        self.renderer = bead_render::renderer(RendererKind::Cpu)?;
                        for surface in &mut self.surfaces {
                            drop(surface.gl.take());
                            drop(surface.gpu.take());
                        }
                        self.gl = None;
                        self.gpu = None;
                        self.options.renderer = RendererKind::Cpu;
                    }
                    let surface = &self.surfaces[i];
                    self.renderer
                        .prepare(scene, surface.scale120 as f32 / 120.)?;
                    #[cfg(feature = "gpu")]
                    if let Some(gl) = &surface.gl {
                        let transport = self.gl.as_ref().ok_or("missing shared GL context")?;
                        let (w, h, _, _) =
                            buffer_dimensions(surface.width, surface.height, surface.scale120)?;
                        transport.activate(gl, w as u32, h as u32)?;
                        self.renderer
                            .stage_resources(bead_render::ResourceTarget::Gl {
                                display: &transport.display,
                            })?;
                    } else if let Some(gpu) = &surface.gpu {
                        let device = &self
                            .gpu
                            .as_ref()
                            .ok_or("missing shared GPU device")?
                            .context
                            .devices[gpu.dev_id];
                        self.renderer
                            .stage_resources(bead_render::ResourceTarget::Wgpu {
                                device: &device.device,
                                queue: &device.queue,
                            })?;
                    }
                }
            }
            runtime.validate()?;
            Ok(scene)
        });
        match staged {
            Ok(Some(scene)) => {
                runtime.commit();
                if let (Some(bridge), Some(tree)) =
                    (&mut self.accessibility, runtime.accessibility_tree())
                {
                    bridge.update(tree);
                }
                if self.options.trace_state {
                    println!("UI_STATE {}", runtime.state_summary());
                }
                self.ui_serial += 1;
                self.scene = scene;
                if self.options.content_size {
                    let width = self.scene.bounds.w.ceil().max(1.) as u32;
                    let height = (self.scene.bounds.y
                        + self.scene.bounds.h
                        + self.scene.slide.map_or(self.scene.offset.1, |s| s.open_y))
                    .ceil()
                    .max(1.) as u32;
                    for surface in self.surfaces.iter().filter(|s| !s.companion) {
                        if surface.width != width || surface.height != height {
                            surface.layer.set_size(width, height);
                            surface.layer.commit();
                        }
                    }
                }
                println!(
                    "UI_PLATFORM a11y_adapters={} ime_objects={} ui_channels={}",
                    usize::from(self.accessibility.is_some()),
                    usize::from(self.text_input.is_some()),
                    if self.accessibility.is_some() { 2 } else { 1 }
                );
                let (layers, images) = self.renderer.resource_counts();
                println!(
                    "UI_RESOURCES {} layers={} gpu_images={} surfaces={}",
                    runtime.resource_counts(),
                    layers,
                    images,
                    self.surfaces.len()
                );
                println!(
                    "UI_COMMIT generation={} reload={} prepare_ms={:.3} monotonic_ns={} update={}",
                    self.scene.resource_revision,
                    reload,
                    started.elapsed().as_secs_f64() * 1000.,
                    monotonic_ns(),
                    self.ui_serial
                );
            }
            Ok(None) => {
                self.runtime = Some(runtime);
                return;
            }
            Err(error) => {
                eprintln!("UI_ERROR {error}");
                self.scene = runtime.error_scene(error.to_string());
            }
        }
        self.runtime = Some(runtime);
        self.sync_ime();
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            self.draw(i, &qh);
        }
    }

    #[cfg(feature = "gpu")]
    fn ensure_gpu(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;
        let surface = &mut self.surfaces[index];
        let (w, h, _, _) = buffer_dimensions(surface.width, surface.height, surface.scale120)?;
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr().cast())
                .ok_or("native display missing")?,
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.layer.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        if self.options.renderer == RendererKind::FemtovgGl {
            if self.gl.is_none() {
                // SAFETY: the retained connection outlives the EGL context/surfaces.
                self.gl = Some(unsafe { bead_render::gl::Transport::new(display) }?);
            }
            if surface.gl.is_none() {
                println!(
                    "BUFFER companion={} logical={}x{} physical={}x{} scale={} transport=GL",
                    surface.companion,
                    surface.width,
                    surface.height,
                    w,
                    h,
                    surface.scale120 as f32 / 120.
                );
                // SAFETY: Surface::drop drops EGL before its wl_surface.
                surface.gl = Some(unsafe {
                    self.gl
                        .as_mut()
                        .unwrap()
                        .create_surface(window, w as u32, h as u32)
                }?);
            }
        } else {
            if self.gpu.is_none() {
                self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
            }
            if surface.gpu.is_none() {
                // SAFETY: Surface::drop drops swapchains before their protocol objects.
                surface.gpu = Some(unsafe {
                    self.gpu.as_mut().unwrap().create_surface(
                        display,
                        window,
                        w as u32,
                        h as u32,
                        self.options.renderer,
                    )
                }?);
            }
        }
        Ok(())
    }
    fn sync_ime(&mut self) {
        let Some(input) = &self.text_input else {
            return;
        };
        if self.ime_surface.is_none() {
            return;
        }
        let current = self.runtime.as_ref().and_then(|r| r.surrounding_text());
        if current == self.ime_last {
            return;
        }
        if let Some((text, cursor, anchor)) = &current {
            input.enable();
            input.set_content_type(
                zwp_text_input_v3::ContentHint::None,
                zwp_text_input_v3::ContentPurpose::Normal,
            );
            let mut start = (*cursor as usize).saturating_sub(2000);
            while !text.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (start + 4000).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let cursor = (*cursor as usize).clamp(start, end) - start;
            let mut anchor = (*anchor as usize).clamp(start, end);
            while !text.is_char_boundary(anchor) {
                anchor -= 1;
            }
            input.set_surrounding_text(
                text[start..end].to_owned(),
                cursor as i32,
                (anchor - start) as i32,
            );
            input.set_cursor_rectangle(32, 32, 1, 24);
            input.commit();
        } else {
            input.disable();
            input.commit();
        }
        self.ime_last = current;
    }
    fn fail(&mut self, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        self.signal.stop();
    }
    fn add_output(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.options.output.as_ref().is_some_and(|name| {
            self.output.info(&output).and_then(|i| i.name).as_ref() != Some(name)
        }) {
            return;
        }
        let count = self.surfaces.iter().filter(|s| s.output == output).count();
        let total = self.options.surface_count + usize::from(self.companion.is_some());
        let companion = count == self.options.surface_count;
        if count >= total {
            return;
        }
        let result = (|| -> Result<Surface, Box<dyn Error>> {
            let surface = self.compositor.create_surface(qh);
            let empty = Region::new(&self.compositor)?;
            if !self.options.interactive || companion {
                surface.set_input_region(Some(empty.wl_region()));
            }
            let fractional = if self.viewporter.is_some() {
                self.fractional
                    .as_ref()
                    .map(|m| m.get_fractional_scale(&surface, qh, surface.clone()))
            } else {
                None
            };
            let viewport = if fractional.is_some() {
                self.viewporter
                    .as_ref()
                    .map(|m| m.get_viewport(&surface, qh, ()))
            } else {
                None
            };
            let layer = self.layers.create_layer_surface(
                qh,
                surface,
                Layer::Top,
                Some(&self.options.namespace),
                Some(&output),
            );
            let content = companion || (self.options.panel && self.options.content_size);
            let scene = if companion {
                &self.companion.as_ref().unwrap().0
            } else {
                &self.scene
            };
            layer.set_anchor(if content {
                Anchor::TOP
            } else if self.options.panel {
                Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
            });
            if content {
                let height = scene.bounds.y
                    + scene.bounds.h
                    + scene.slide.map_or(scene.offset.1, |s| s.open_y);
                layer.set_size(scene.bounds.w.ceil() as u32, height.ceil().max(1.) as u32);
            } else {
                layer.set_size(
                    0,
                    if self.options.bar && !companion {
                        50
                    } else if self.options.panel || companion {
                        0
                    } else {
                        32
                    },
                );
            }
            if companion {
                layer.set_margin(70, 0, 0, 0);
            }
            layer.set_exclusive_zone(if self.options.test_passive {
                -1
            } else if self.options.bar && !companion {
                50
            } else if self.options.panel || companion {
                -1
            } else {
                0
            });
            layer.set_keyboard_interactivity(if self.options.test_keyboard && !companion {
                KeyboardInteractivity::Exclusive
            } else if self.options.interactive && self.options.keyboard_focus && !companion {
                KeyboardInteractivity::OnDemand
            } else {
                KeyboardInteractivity::None
            });
            layer.commit();
            let scale120 = self
                .output
                .info(&output)
                .map_or(120, |i| i.scale_factor.max(1) as u32 * 120);
            Ok(Surface {
                companion,
                output: output.clone(),
                layer,
                pool: SlotPool::new(4096, &self.shm)?,
                #[cfg(feature = "gpu")]
                gpu: None,
                #[cfg(feature = "gpu")]
                gl: None,
                buffers: Vec::new(),
                width: 0,
                height: 32,
                scale120,
                configured: false,
                ready: false,
                frame_pending: false,
                redraw: false,
                fractional,
                viewport,
                geometry: (0, 0),
                offset: (0., 0.),
                format: PixelFormat::Bgra,
                previous: None,
                allocations: 0,
                frames: 0,
                render_ms: Vec::new(),
                acquire_ms: Vec::new(),
                swap_ms: Vec::new(),
                activate_ms: Vec::new(),
                callbacks_ms: Vec::new(),
                last_callback: None,
                presented_ms: Vec::new(),
                last_presented: None,
                refresh_ns: 0,
                presentation_kind: 0,
                discarded: 0,
                presented_generation: None,
            })
        })();
        match result {
            Ok(s) => {
                self.surfaces.push(s);
                if self.surfaces.iter().filter(|s| s.output == output).count() < total {
                    self.add_output(qh, output);
                }
            }
            Err(e) => self.fail(e),
        }
    }
    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let result = (|| -> Result<(), Box<dyn Error>> {
            let s = &mut self.surfaces[index];
            let (scene, renderer, _kind, panel) = if s.companion {
                let (scene, renderer, kind) =
                    self.companion.as_mut().ok_or("missing companion scene")?;
                (scene, renderer, *kind, true)
            } else {
                (
                    &mut self.scene,
                    &mut self.renderer,
                    self.options.renderer,
                    self.options.panel,
                )
            };
            if !s.configured {
                return Ok(());
            }
            if s.frame_pending {
                s.redraw = true;
                return Ok(());
            }
            s.redraw = false;
            if let Some(motion) = &scene.motion {
                let now = self.started.elapsed().as_secs_f64();
                let open = if self.options.animate {
                    ((now - 3.).max(0.) % 2.) >= 1. || now < 3.
                } else {
                    motion.open
                };
                self.animations.target(
                    &motion.id,
                    if open { 1. } else { 0. },
                    now,
                    if self.options.animate {
                        1.
                    } else {
                        motion.duration
                    },
                    if motion.spring {
                        bead_core::animation::Curve::Spring
                    } else {
                        bead_core::animation::Curve::EaseOut
                    },
                );
                let progress = self.animations.value(&motion.id, now).unwrap().0;
                scene.opacity = progress;
                scene.offset.1 = 32. - (1. - progress) * scene.bounds.h;
            }

            let scale = s.scale120 as f32 / 120.;
            let (w, h, stride, size) = buffer_dimensions(s.width, s.height, s.scale120)?;
            if !panel {
                scene.bounds.w = s.width as f32;
                scene.bounds.h = s.height as f32;
                scene.width = s.width as f32;
            }
            if panel {
                scene.offset.0 = ((s.width as f32 - scene.width) / 2.).max(0.);
                if let Some(slide) = scene.slide {
                    let progress = if self.options.animate {
                        let elapsed = self.started.elapsed().as_secs_f32();
                        if self.options.start_open && (elapsed < 3. || !s.ready) {
                            1.
                        } else {
                            let t = ((elapsed - if self.options.start_open { 3. } else { 0. })
                                % slide.period_seconds)
                                / (slide.period_seconds / 2.);
                            let progress = if t <= 1. { t } else { 2. - t };
                            if self.options.start_open {
                                1. - progress
                            } else {
                                progress
                            }
                        }
                    } else {
                        1.
                    };
                    let eased = progress * progress * (3. - 2. * progress);
                    scene.opacity = eased;
                    let y = slide.closed_y + (slide.open_y - slide.closed_y) * eased;
                    scene.offset.1 = (y * scale).round() / scale;
                }
            }
            s.offset = scene.offset;
            renderer.prepare(scene, scale)?;
            if let Some(viewport) = &s.viewport {
                s.layer.wl_surface().set_buffer_scale(1);
                viewport.set_destination(s.width as i32, s.height as i32);
            } else {
                s.layer
                    .wl_surface()
                    .set_buffer_scale((s.scale120 / 120) as i32);
            }
            #[cfg(feature = "gpu")]
            if _kind == RendererKind::FemtovgGl {
                use raw_window_handle::{
                    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
                };
                use std::ptr::NonNull;
                let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(self.conn.backend().display_ptr().cast())
                        .ok_or("native display missing")?,
                ));
                let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(s.layer.wl_surface().id().as_ptr().cast())
                        .ok_or("native surface missing")?,
                ));
                if self.gl.is_none() {
                    // SAFETY: the connection outlives Transport and surfaces.
                    self.gl = Some(
                        unsafe { bead_render::gl::Transport::new(display) }
                            .map_err(|e| GpuUnavailable(e.to_string()))?,
                    );
                }
                let transport = self.gl.as_mut().unwrap();
                if s.gl.is_none() {
                    // SAFETY: EGL surface is dropped before the retained wl_surface.
                    s.gl = Some(
                        unsafe { transport.create_surface(window, w as u32, h as u32) }
                            .map_err(|e| GpuUnavailable(e.to_string()))?,
                    );
                    println!(
                        "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GL",
                        s.output.id().protocol_id(),
                        s.width,
                        s.height,
                        w,
                        h
                    );
                }
                let surface = s.gl.as_ref().unwrap();
                let activate_started = Instant::now();
                transport.activate(surface, w as u32, h as u32)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.activate_ms
                        .push(activate_started.elapsed().as_secs_f64() * 1000.);
                }
                let start = Instant::now();
                renderer.draw(
                    scene,
                    RenderTarget::Gl(bead_render::gl::GlTarget {
                        display: &transport.display,
                        width: w as u32,
                        height: h as u32,
                    }),
                    Damage::full(w as u32, h as u32),
                )?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                }
                s.layer.wl_surface().damage_buffer(0, 0, w, h);
                s.layer
                    .wl_surface()
                    .frame(qh, FrameCallbackData(s.layer.wl_surface().clone()));
                s.frame_pending = true;
                if let Some(p) = &self.presentation {
                    p.feedback(
                        s.layer.wl_surface(),
                        qh,
                        PresentationData {
                            update: self.ui_serial,
                            surface: s.layer.wl_surface().clone(),
                            generation: scene.resource_revision,
                        },
                    );
                }
                let swap_started = Instant::now();
                transport.present(surface)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.swap_ms.push(swap_started.elapsed().as_secs_f64() * 1000.);
                }
                s.frames += 1;
                return Ok(());
            }
            #[cfg(feature = "gpu")]
            if renderer.wants_gpu() {
                if self.gpu.is_none() {
                    self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
                }
                let transport = self.gpu.as_mut().unwrap();
                if s.gpu.is_none() {
                    use raw_window_handle::{
                        RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle,
                        WaylandWindowHandle,
                    };
                    use std::ptr::NonNull;
                    let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                        NonNull::new(self.conn.backend().display_ptr().cast())
                            .ok_or("native display missing")?,
                    ));
                    let surface = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                        NonNull::new(s.layer.wl_surface().id().as_ptr().cast())
                            .ok_or("native surface missing")?,
                    ));
                    // SAFETY: Surface::drop destroys GPU surface before protocol surface;
                    // app clears surfaces while the connection is still alive.
                    let created = unsafe {
                        transport.create_surface(display, surface, w as u32, h as u32, _kind)
                    };
                    match created {
                        Ok(gpu) => s.gpu = Some(gpu),
                        Err(e) => {
                            if !self.options.auto_fallback {
                                return Err(e);
                            }
                            eprintln!("FALLBACK GPU unavailable: {e}");
                            *renderer = bead_render::renderer(RendererKind::Cpu)?;
                            self.options.renderer = RendererKind::Cpu;
                            self.gpu = None;
                        }
                    }
                    if s.gpu.is_some() {
                        println!(
                            "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GPU",
                            s.output.id().protocol_id(),
                            s.width,
                            s.height,
                            w,
                            h
                        );
                    }
                }
                if let Some(gpu) = s.gpu.as_mut() {
                    let transport = self.gpu.as_mut().unwrap();
                    transport.resize(gpu, w as u32, h as u32);
                    let dev = &transport.context.devices[gpu.dev_id];
                    use bead_render::gpu::{GpuTarget, wgpu};
                    let acquired = Instant::now();
                    let frame = match gpu.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(f)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                        other => return Err(format!("GPU acquire failed: {other:?}").into()),
                    };
                    let acquire_ms = acquired.elapsed().as_secs_f64() * 1000.;
                    let start = Instant::now();
                    renderer.draw(
                        scene,
                        RenderTarget::Gpu(GpuTarget {
                            device: &dev.device,
                            queue: &dev.queue,
                            texture: &frame.texture,
                            target_view: gpu.target.as_ref().map(|(_, view, _)| view),
                            width: w as u32,
                            height: h as u32,
                        }),
                        Damage::full(w as u32, h as u32),
                    )?;
                    if let Some((_, view, blitter)) = &gpu.target {
                        let mut encoder = dev.device.create_command_encoder(&Default::default());
                        blitter.copy(
                            &dev.device,
                            &mut encoder,
                            view,
                            &frame.texture.create_view(&Default::default()),
                        );
                        dev.queue.submit([encoder.finish()]);
                    }
                    if self.started.elapsed().as_secs_f64() > 2. {
                        s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                        s.acquire_ms.push(acquire_ms);
                    }
                    s.layer.wl_surface().damage_buffer(0, 0, w, h);
                    s.layer
                        .wl_surface()
                        .frame(qh, FrameCallbackData(s.layer.wl_surface().clone()));
                    s.frame_pending = true;
                    if let Some(p) = &self.presentation {
                        p.feedback(
                            s.layer.wl_surface(),
                            qh,
                            PresentationData {
                                update: self.ui_serial,
                                surface: s.layer.wl_surface().clone(),
                                generation: scene.resource_revision,
                            },
                        );
                    }
                    dev.queue.present(frame);
                    let _ = dev.device.poll(wgpu::PollType::Poll);
                    s.frames += 1;
                    return Ok(());
                }
                // An explicitly permitted automatic fallback uses the same retained Scene.
                renderer.prepare(scene, scale)?;
            }
            let format = if cfg!(target_endian = "little")
                && !self.options.force_argb
                && self.shm.formats().contains(&wl_shm::Format::Abgr8888)
            {
                PixelFormat::Rgba
            } else {
                PixelFormat::Bgra
            };
            if !cfg!(target_endian = "little") {
                return Err("CPU renderer currently requires little endian".into());
            }
            if s.geometry != (w, h) || s.format != format {
                s.buffers.clear();
                s.geometry = (w, h);
                s.format = format;
                s.previous = None;
                s.pool.resize(size * 3)?;
                for _ in 0..3 {
                    let (buffer, _) = s.pool.create_buffer(
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )?;
                    s.buffers.push(Reusable {
                        buffer,
                        pending: Damage::full(w as u32, h as u32),
                    });
                    s.allocations += 1;
                }
                println!(
                    "BUFFER output={} logical={}x{} physical={}x{} scale={scale} format={format:?} allocation_count={}",
                    s.output.id().protocol_id(),
                    s.width,
                    s.height,
                    w,
                    h,
                    s.allocations
                );
            }
            let current = scene.bounds();
            let d = if self.options.damage {
                s.previous.map_or(Damage::full(w as u32, h as u32), |old| {
                    old.union(current).pixels(scale, w as u32, h as u32)
                })
            } else {
                Damage::full(w as u32, h as u32)
            };
            for b in &mut s.buffers {
                b.pending = b.pending.union(d);
            }
            let b = s
                .buffers
                .iter_mut()
                .find(|b| !b.buffer.slot().has_active_buffers())
                .ok_or("three buffers still in use at callback")?;
            let damage = b.pending;
            let start = Instant::now();
            renderer.draw(
                scene,
                RenderTarget::Shm(ShmTarget {
                    bytes: b
                        .buffer
                        .canvas(&mut s.pool)
                        .ok_or("compositor owns buffer")?,
                    width: w as u32,
                    height: h as u32,
                    scale,
                    format,
                }),
                damage,
            )?;
            if self.started.elapsed().as_secs_f64() > 2. {
                s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
            s.layer.wl_surface().damage_buffer(
                damage.x as i32,
                damage.y as i32,
                damage.w as i32,
                damage.h as i32,
            );
            s.layer
                .wl_surface()
                .frame(qh, FrameCallbackData(s.layer.wl_surface().clone()));
            s.frame_pending = true;
            if let Some(p) = &self.presentation {
                p.feedback(
                    s.layer.wl_surface(),
                    qh,
                    PresentationData {
                        update: self.ui_serial,
                        surface: s.layer.wl_surface().clone(),
                        generation: scene.resource_revision,
                    },
                );
            }
            b.buffer.attach_to(s.layer.wl_surface())?;
            s.layer.commit();
            b.pending = Damage::full(0, 0);
            s.previous = Some(current);
            s.frames += 1;
            Ok(())
        })();
        if let Err(e) = result {
            #[cfg(feature = "gpu")]
            if self.options.auto_fallback
                && !self.surfaces[index].companion
                && e.downcast_ref::<GpuUnavailable>().is_some()
            {
                eprintln!("FALLBACK GPU unavailable: {e}");
                if let Ok(renderer) = bead_render::renderer(RendererKind::Cpu) {
                    self.renderer = renderer;
                }
                for surface in &mut self.surfaces {
                    drop(surface.gl.take());
                    drop(surface.gpu.take());
                    surface.frame_pending = false;
                }
                self.gl = None;
                self.gpu = None;
                self.options.renderer = RendererKind::Cpu;
                for i in 0..self.surfaces.len() {
                    self.draw(i, qh);
                }
                return;
            }
            self.fail(e);
        }
    }
}
fn buffer_dimensions(
    w: u32,
    h: u32,
    scale120: u32,
) -> Result<(i32, i32, i32, usize), Box<dyn Error>> {
    if w == 0 || h == 0 || scale120 == 0 {
        return Err("empty geometry or scale".into());
    }
    // Ceil ensures every edge pixel is covered; viewporter maps to logical size.
    let w = (u64::from(w) * u64::from(scale120)).div_ceil(120);
    let h = (u64::from(h) * u64::from(scale120)).div_ceil(120);
    let stride = w.checked_mul(4).ok_or("stride overflow")?;
    let size = stride.checked_mul(h).ok_or("size overflow")?;
    if size > 64 * 1024 * 1024 {
        return Err("surface buffer exceeds 64 MiB".into());
    }
    Ok((
        i32::try_from(w)?,
        i32::try_from(h)?,
        i32::try_from(stride)?,
        usize::try_from(size)?,
    ))
}
impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        scale: i32,
    ) {
        if let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
            && self.surfaces[i].fractional.is_none()
        {
            self.surfaces[i].scale120 = scale.max(1) as u32 * 120;
            self.draw(i, qh);
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(i) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        {
            let s = &mut self.surfaces[i];
            s.frame_pending = false;
            let now = Instant::now();
            if self.started.elapsed().as_secs_f64() > 2.
                && let Some(last) = s.last_callback
            {
                s.callbacks_ms
                    .push(now.duration_since(last).as_secs_f64() * 1000.);
            }
            s.last_callback = Some(now);
            if !s.ready {
                s.ready = true;
                println!(
                    "READY output={} elapsed_ms={:.3}",
                    s.output.id().protocol_id(),
                    self.started.elapsed().as_secs_f64() * 1000.
                );
            }
            if self
                .options
                .seconds
                .is_some_and(|secs| self.started.elapsed().as_secs_f64() >= secs)
            {
                self.signal.stop();
            } else if self.options.animate
                || s.redraw
                || self.animations.active(self.started.elapsed().as_secs_f64())
            {
                self.draw(i, qh);
            }
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl Dispatch<WpFractionalScaleV1, wl_surface::WlSurface> for App {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some(i) = state
                .surfaces
                .iter()
                .position(|s| s.layer.wl_surface() == surface)
        {
            state.surfaces[i].scale120 = scale;
            state.draw(i, qh);
        }
    }
}
impl Dispatch<WpPresentation, ()> for App {
    fn event(
        _: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            println!("PRESENTATION clock_id={clk_id}");
        }
    }
}
impl Dispatch<WpPresentationFeedback, PresentationData> for App {
    fn event(
        state: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &PresentationData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(s) = state
            .surfaces
            .iter_mut()
            .find(|s| s.layer.wl_surface() == &data.surface)
        {
            match event {
                wp_presentation_feedback::Event::Presented {
                    tv_sec_hi,
                    tv_sec_lo,
                    tv_nsec,
                    refresh,
                    flags,
                    ..
                } => {
                    let ns = ((u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo)) * 1_000_000_000
                        + u64::from(tv_nsec);
                    if state.started.elapsed().as_secs_f64() > 2.
                        && let Some(last) = s.last_presented
                        && ns > last
                    {
                        s.presented_ms.push((ns - last) as f64 / 1_000_000.);
                    }
                    if s.presented_generation != Some(data.update) {
                        println!(
                            "UI_PRESENTED generation={} monotonic_ns={} refresh_ns={} companion={} update={}",
                            data.generation, ns, refresh, s.companion, data.update
                        );
                        s.presented_generation = Some(data.update);
                    }
                    s.last_presented = Some(ns);
                    s.refresh_ns = refresh;
                    s.presentation_kind = flags.into();
                }
                wp_presentation_feedback::Event::Discarded => s.discarded += 1,
                _ => {}
            }
        }
    }
}
delegate_noop!(App: ignore WpFractionalScaleManagerV1);
delegate_noop!(App: ignore WpViewporter);
delegate_noop!(App: ignore WpViewport);
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(qh, output);
    }
    fn update_output(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.add_output(qh, output);
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.surfaces.retain(|s| s.output != output);
        println!("OUTPUT_REMOVED remaining={}", self.surfaces.len());
    }
}
impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.surfaces.retain(|s| s.layer != *layer);
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        config: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(i) = self.surfaces.iter().position(|s| s.layer == *layer) {
            let s = &mut self.surfaces[i];
            s.width = config.new_size.0;
            s.height = config.new_size.1;
            s.configured = true;
            if let Some(bridge) = &mut self.accessibility {
                let bounds = accesskit::Rect {
                    x0: 0.,
                    y0: 0.,
                    x1: f64::from(s.width),
                    y1: f64::from(s.height),
                };
                bridge.adapter.set_root_window_bounds(bounds, bounds);
            }

            // One shared view fits the smallest output; anonymous layout boxes never own state.
            let viewport = self
                .surfaces
                .iter()
                .filter_map(|surface| {
                    self.output
                        .info(&surface.output)
                        .and_then(|info| info.logical_size)
                })
                .fold(None, |old: Option<(i32, i32)>, size| {
                    Some(old.map_or(size, |old| (old.0.min(size.0), old.1.min(size.1))))
                });
            let viewport_changed =
                if let (Some(runtime), Some((w, h))) = (&mut self.runtime, viewport) {
                    runtime.viewport(w.max(1) as u32, h.max(1) as u32)
                } else {
                    false
                };
            if self.runtime.is_some() && !self.ui_initialized {
                self.ui_initialized = true;
                self.ui(UiEvent::Reload);
            } else if viewport_changed {
                self.ui(UiEvent::Relayout);
            } else {
                self.draw(i, qh);
            }
        }
    }
}
delegate_registry!(App);
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}
smithay_client_toolkit::delegate_dispatch2!(App);
impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if !self.options.interactive {
            return;
        }
        match capability {
            Capability::Keyboard if self.keyboard.is_none() => {
                if self.text_input.is_none()
                    && let Some(manager) = &self.text_input_manager
                {
                    self.text_input = Some(manager.get_text_input(&seat, qh, ()));
                    println!("IME_PROTOCOL text_input_v3=true");
                }

                match self.seat.get_keyboard(qh, &seat, None) {
                    Ok(k) => self.keyboard = Some(k),
                    Err(e) => self.fail(e),
                }
            }
            Capability::Pointer if self.pointer.is_none() => match self.seat.get_pointer(qh, &seat)
            {
                Ok(p) => {
                    if let Some(manager) = &self.cursor_manager {
                        self.cursor_device = Some(manager.get_pointer(&p, qh, ()));
                    }
                    self.pointer = Some(p)
                }
                Err(e) => self.fail(e),
            },
            _ => {}
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard => {
                if let Some(k) = self.keyboard.take() {
                    k.release();
                    self.keyboard_surface = None;
                }
            }
            Capability::Pointer => {
                if let Some(p) = self.pointer.take() {
                    p.release();
                }
            }
            _ => {}
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(surface) = self
                .surfaces
                .iter()
                .find(|s| s.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            let x = event.position.0 as f32 - surface.offset.0;
            let y = event.position.1 as f32 - surface.offset.1;
            if !surface.configured {
                continue;
            }
            if let PointerEventKind::Enter { serial, .. } = event.kind {
                self.cursor_enter_serial = serial;
            }
            if let Some(device) = &self.cursor_device
                && self.cursor_enter_serial != 0
            {
                let shape = match self.runtime.as_ref().map(|r| r.pointer_cursor(x, y)) {
                    Some("text") => wp_cursor_shape_device_v1::Shape::Text,
                    Some("pointer") => wp_cursor_shape_device_v1::Shape::Pointer,
                    _ => wp_cursor_shape_device_v1::Shape::Default,
                };
                device.set_shape(self.cursor_enter_serial, shape);
            }
            match event.kind {
                PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                    self.ui(UiEvent::Hover { x, y })
                }
                PointerEventKind::Leave { .. } => self.ui(UiEvent::Hover { x: -1., y: -1. }),
                PointerEventKind::Press { button: 0x110, .. } => self.ui(UiEvent::Click { x, y }),
                PointerEventKind::Press { button: 0x111, .. } => {
                    self.ui(UiEvent::ContextMenu { x, y })
                }
                PointerEventKind::Axis { vertical, .. } => self.ui(UiEvent::Scroll {
                    x,
                    y,
                    delta: vertical.absolute,
                }),
                _ => {}
            }
        }
    }
}
impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        if self.options.trace_state {
            println!("INPUT_ENTER surface={}", surface.id().protocol_id());
        }
        self.keyboard_surface = Some(surface.clone());
        if let Some(bridge) = &mut self.accessibility {
            bridge.adapter.update_window_focus_state(true);
        }
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.keyboard_surface = None;
        if let Some(bridge) = &mut self.accessibility {
            bridge.adapter.update_window_focus_state(false);
        }
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if self.options.trace_state {
            println!(
                "INPUT_KEY sym={:?} focused_surface={}",
                event.keysym,
                self.keyboard_surface.is_some()
            );
        }
        if self.keyboard_surface.as_ref().is_some_and(|surface| {
            self.surfaces
                .iter()
                .any(|s| s.layer.wl_surface() == surface)
        }) {
            let command = match event.keysym {
                Keysym::Tab | Keysym::ISO_Left_Tab => Some("tab"),
                Keysym::Return | Keysym::KP_Enter => Some("enter"),
                Keysym::Left => Some("left"),
                Keysym::Right => Some("right"),
                Keysym::Up => Some("up"),
                Keysym::Down => Some("down"),
                Keysym::Home => Some("home"),
                Keysym::End => Some("end"),
                Keysym::Delete => Some("delete"),
                Keysym::a if self.modifiers.ctrl => Some("select-all"),
                Keysym::c if self.modifiers.ctrl => Some("clipboard-copy"),
                Keysym::x if self.modifiers.ctrl => Some("clipboard-cut"),
                Keysym::v if self.modifiers.ctrl => Some("clipboard-paste"),
                _ => None,
            };
            if let Some(command) = command {
                self.ui(UiEvent::Command {
                    command: command.into(),
                    shift: self.modifiers.shift || event.keysym == Keysym::ISO_Left_Tab,
                });
            } else if !self.modifiers.ctrl && !self.modifiers.alt {
                self.ui(UiEvent::Key {
                    text: event.utf8.unwrap_or_default(),
                    backspace: event.keysym == Keysym::BackSpace,
                });
            }
        }
    }
    fn repeat_key(
        &mut self,
        c: &Connection,
        qh: &QueueHandle<Self>,
        k: &wl_keyboard::WlKeyboard,
        s: u32,
        event: KeyEvent,
    ) {
        self.press_key(c, qh, k, s, event);
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_dimensions_cover_edges_without_integer_inflation() {
        assert_eq!(
            buffer_dimensions(1920, 1200, 180).unwrap(),
            (2880, 1800, 11520, 20736000)
        );
        assert_eq!(
            buffer_dimensions(641, 719, 180).unwrap(),
            (962, 1079, 3848, 4151992)
        );
    }
    #[test]
    fn rejects_unbounded_geometry() {
        for (w, h, s) in [
            (0, 32, 120),
            (512, 0, 120),
            (512, 32, 0),
            (u32::MAX, 32, 240),
            (3840, 2160, 960),
        ] {
            assert!(buffer_dimensions(w, h, s).is_err());
        }
    }
}
impl Dispatch<ZwpTextInputV3, ()> for App {
    fn event(
        state: &mut Self,
        input: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                state.ime_surface = Some(surface);
                state.sync_ime();
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                state.ime_surface = None;
                input.disable();
                input.commit();
                state.ime_last = None;
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                state.ime_pending.0 = text.unwrap_or_default()
            }
            zwp_text_input_v3::Event::CommitString { text } => state.ime_pending.1 = text,
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                state.ime_pending.2 = before_length;
                state.ime_pending.3 = after_length;
            }
            zwp_text_input_v3::Event::Done { .. } => {
                let (preedit, commit, before, after) = std::mem::take(&mut state.ime_pending);
                state.ui(UiEvent::Ime {
                    preedit,
                    commit,
                    before,
                    after,
                });
            }
            _ => {}
        }
    }
}
wayland_client::delegate_noop!(App:ZwpTextInputManagerV3);

wayland_client::delegate_noop!(App:WpCursorShapeManagerV1);
wayland_client::delegate_noop!(App:WpCursorShapeDeviceV1);

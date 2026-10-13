// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor-neutral layer surfaces with fractional scaling and reusable SHM.
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
    protocol::{wl_output, wl_shm, wl_surface},
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
#[derive(Clone)]
pub struct Options {
    pub namespace: String,
    pub animate: bool,
    pub damage: bool,
    pub force_argb: bool,
    pub seconds: Option<f64>,
    pub panel: bool,
    pub content_size: bool,
    pub renderer: RendererKind,
    pub output: Option<String>,
    pub auto_fallback: bool,
    pub started: Instant,
    pub start_open: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            namespace: "bead-dev".into(),
            animate: false,
            damage: true,
            force_argb: false,
            seconds: None,
            panel: false,
            content_size: false,
            renderer: RendererKind::Cpu,
            output: None,
            auto_fallback: false,
            started: Instant::now(),
            start_open: false,
        }
    }
}
struct Reusable {
    buffer: Buffer,
    pending: Damage,
}
struct Surface {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    #[cfg(feature = "gpu")]
    gpu: Option<bead_render::gpu::Surface>,
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
    format: PixelFormat,
    previous: Option<Rect>,
    allocations: usize,
    frames: usize,
    render_ms: Vec<f64>,
    acquire_ms: Vec<f64>,
    callbacks_ms: Vec<f64>,
    last_callback: Option<Instant>,
    presented_ms: Vec<f64>,
    last_presented: Option<u64>,
    refresh_ns: u32,
    discarded: usize,
}
impl Drop for Surface {
    fn drop(&mut self) {
        #[cfg(feature = "gpu")]
        drop(self.gpu.take());
        if let Some(f) = &self.fractional {
            f.destroy();
        }
        if let Some(v) = &self.viewport {
            v.destroy();
        }
    }
}
struct App {
    #[cfg(feature = "gpu")]
    gpu: Option<bead_render::gpu::Transport>,
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
    let mut app = App {
        #[cfg(feature = "gpu")]
        gpu: None,
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
            "METRICS output={} frames={} allocations={} render_p50_ms={:.3} render_p99_ms={:.3} callback_p50_ms={:.3} callback_p99_ms={:.3} presented_p50_ms={:.3} presented_p99_ms={:.3} presented_samples={} refresh_ns={} discarded={}",
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
            s.discarded
        );
    }
    app.surfaces.clear();
    let _ = conn.flush();
    println!("STOP surfaces=0");
    result?;
    if let Some(e) = app.failure {
        return Err(e.into());
    }
    Ok(())
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
        if self.surfaces.iter().any(|s| s.output == output) {
            return;
        }
        let result = (|| -> Result<Surface, Box<dyn Error>> {
            let surface = self.compositor.create_surface(qh);
            let empty = Region::new(&self.compositor)?;
            surface.set_input_region(Some(empty.wl_region()));
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
            let content = self.options.panel && self.options.content_size;
            layer.set_anchor(if content {
                Anchor::TOP
            } else if self.options.panel {
                Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
            });
            if content {
                let height = self.scene.bounds.y
                    + self.scene.bounds.h
                    + self.scene.slide.map_or(0., |s| s.open_y);
                layer.set_size(
                    self.scene.bounds.w.ceil() as u32,
                    height.ceil().max(1.) as u32,
                );
            } else {
                layer.set_size(0, if self.options.panel { 0 } else { 32 });
            }
            layer.set_exclusive_zone(if self.options.panel { -1 } else { 0 });
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            let scale120 = self
                .output
                .info(&output)
                .map_or(120, |i| i.scale_factor.max(1) as u32 * 120);
            Ok(Surface {
                output,
                layer,
                pool: SlotPool::new(4096, &self.shm)?,
                #[cfg(feature = "gpu")]
                gpu: None,
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
                format: PixelFormat::Bgra,
                previous: None,
                allocations: 0,
                frames: 0,
                render_ms: Vec::new(),
                acquire_ms: Vec::new(),
                callbacks_ms: Vec::new(),
                last_callback: None,
                presented_ms: Vec::new(),
                last_presented: None,
                refresh_ns: 0,
                discarded: 0,
            })
        })();
        match result {
            Ok(s) => self.surfaces.push(s),
            Err(e) => self.fail(e),
        }
    }
    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let result = (|| -> Result<(), Box<dyn Error>> {
            let s = &mut self.surfaces[index];
            if !s.configured {
                return Ok(());
            }
            if s.frame_pending {
                s.redraw = true;
                return Ok(());
            }
            s.redraw = false;
            let scale = s.scale120 as f32 / 120.;
            let (w, h, stride, size) = buffer_dimensions(s.width, s.height, s.scale120)?;
            if !self.options.panel {
                self.scene.bounds.w = s.width as f32;
                self.scene.bounds.h = s.height as f32;
                self.scene.width = s.width as f32;
            }
            if self.options.panel {
                self.scene.offset.0 = ((s.width as f32 - self.scene.width) / 2.).max(0.);
                if let Some(slide) = self.scene.slide {
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
                    self.scene.opacity = eased;
                    let y = slide.closed_y + (slide.open_y - slide.closed_y) * eased;
                    self.scene.offset.1 = (y * scale).round() / scale;
                }
            }
            self.renderer.prepare(&self.scene, scale)?;
            if let Some(viewport) = &s.viewport {
                s.layer.wl_surface().set_buffer_scale(1);
                viewport.set_destination(s.width as i32, s.height as i32);
            } else {
                s.layer
                    .wl_surface()
                    .set_buffer_scale((s.scale120 / 120) as i32);
            }
            #[cfg(feature = "gpu")]
            if self.renderer.wants_gpu() {
                if self.gpu.is_none() {
                    self.gpu = Some(bead_render::gpu::Transport::default());
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
                        transport.create_surface(
                            display,
                            surface,
                            w as u32,
                            h as u32,
                            self.options.renderer,
                        )
                    };
                    match created {
                        Ok(gpu) => s.gpu = Some(gpu),
                        Err(e) => {
                            if !self.options.auto_fallback {
                                return Err(e);
                            }
                            eprintln!("FALLBACK GPU unavailable: {e}");
                            self.renderer = bead_render::renderer(RendererKind::Cpu)?;
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
                    self.renderer.draw(
                        &self.scene,
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
                        p.feedback(s.layer.wl_surface(), qh, s.layer.wl_surface().clone());
                    }
                    dev.queue.present(frame);
                    let _ = dev.device.poll(wgpu::PollType::Poll);
                    s.frames += 1;
                    return Ok(());
                }
                // An explicitly permitted automatic fallback uses the same retained Scene.
                self.renderer.prepare(&self.scene, scale)?;
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
            let current = self.scene.bounds();
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
            self.renderer.draw(
                &self.scene,
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
                p.feedback(s.layer.wl_surface(), qh, s.layer.wl_surface().clone());
            }
            b.buffer.attach_to(s.layer.wl_surface())?;
            s.layer.commit();
            b.pending = Damage::full(0, 0);
            s.previous = Some(current);
            s.frames += 1;
            Ok(())
        })();
        if let Err(e) = result {
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
            } else if self.options.animate || s.redraw {
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
impl Dispatch<WpPresentationFeedback, wl_surface::WlSurface> for App {
    fn event(
        state: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(s) = state
            .surfaces
            .iter_mut()
            .find(|s| s.layer.wl_surface() == surface)
        {
            match event {
                wp_presentation_feedback::Event::Presented {
                    tv_sec_hi,
                    tv_sec_lo,
                    tv_nsec,
                    refresh,
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
                    s.last_presented = Some(ns);
                    s.refresh_ns = refresh;
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
            self.draw(i, qh);
        }
    }
}
delegate_registry!(App);
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
smithay_client_toolkit::delegate_dispatch2!(App);
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

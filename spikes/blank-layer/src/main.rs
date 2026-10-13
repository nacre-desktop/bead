// SPDX-License-Identifier: MIT OR Apache-2.0
// Protocol plumbing follows SCTK's MIT examples; see LICENSE-SCTK.
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::{ptr::NonNull, time::Instant};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};

struct Gpu {
    context: vello::util::RenderContext,
    surface: vello::util::RenderSurface<'static>,
    renderer: Option<vello::Renderer>,
    canvas: Option<femtovg::Canvas<femtovg::renderer::WGPURenderer>>,
}

struct Spike {
    registry: RegistryState,
    output: OutputState,
    shm: Shm,
    pool: SlotPool,
    gpu: Option<Gpu>,
    layer: LayerSurface,
    conn: Connection,
    mode: String,
    start: Instant,
    ready: bool,
    exit: bool,
}

impl Spike {
    fn draw(&mut self, qh: &QueueHandle<Self>, width: u32, height: u32) {
        if self.mode == "tiny-skia" {
            let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
            pixmap.fill(tiny_skia::Color::from_rgba8(32, 32, 32, 255));
            let (buffer, canvas) = self
                .pool
                .create_buffer(
                    width as i32,
                    height as i32,
                    width as i32 * 4,
                    wl_shm::Format::Argb8888,
                )
                .unwrap();
            // Wayland ARGB8888 is BGRA in little-endian memory; this neutral fill
            // is identical in RGBA/BGRA. Do not generalize this to colored pixels.
            canvas.copy_from_slice(pixmap.data());
            self.layer
                .wl_surface()
                .damage_buffer(0, 0, width as i32, height as i32);
            self.layer
                .wl_surface()
                .frame(qh, FrameCallbackData(self.layer.wl_surface().clone()));
            buffer.attach_to(self.layer.wl_surface()).unwrap();
            self.layer.commit();
        } else {
            if self.gpu.is_none() {
                let mut context = vello::util::RenderContext::new();
                let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(self.conn.backend().display_ptr().cast()).unwrap(),
                ));
                let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(self.layer.wl_surface().id().as_ptr().cast()).unwrap(),
                ));
                // SAFETY: connection and layer outlive the GPU surface. main
                // explicitly drops the GPU before destroying the protocol objects.
                let surface = unsafe {
                    context
                        .instance
                        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                            raw_display_handle: Some(raw_display_handle),
                            raw_window_handle,
                        })
                        .unwrap()
                };
                let surface = pollster::block_on(context.create_render_surface(
                    surface,
                    width,
                    height,
                    wgpu::PresentMode::Fifo,
                ))
                .unwrap();
                let dev = &context.devices[surface.dev_id];
                eprintln!("ADAPTER {:?}", dev.adapter().get_info());
                let (renderer, canvas) = if self.mode == "vello" {
                    (
                        Some(
                            vello::Renderer::new(&dev.device, vello::RendererOptions::default())
                                .unwrap(),
                        ),
                        None,
                    )
                } else {
                    let renderer =
                        femtovg::renderer::WGPURenderer::new(dev.device.clone(), dev.queue.clone());
                    (None, Some(femtovg::Canvas::new(renderer).unwrap()))
                };
                self.gpu = Some(Gpu {
                    context,
                    surface,
                    renderer,
                    canvas,
                });
            }
            let gpu = self.gpu.as_mut().unwrap();
            gpu.context.resize_surface(&mut gpu.surface, width, height);
            let dev = &gpu.context.devices[gpu.surface.dev_id];
            let frame = match gpu.surface.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f)
                | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                other => panic!("Cannot present spike: {other:?}"),
            };
            self.layer
                .wl_surface()
                .frame(qh, FrameCallbackData(self.layer.wl_surface().clone()));
            if let Some(renderer) = &mut gpu.renderer {
                renderer
                    .render_to_texture(
                        &dev.device,
                        &dev.queue,
                        &vello::Scene::new(),
                        &gpu.surface.target_view,
                        &vello::RenderParams {
                            base_color: vello::peniko::Color::from_rgb8(32, 32, 32),
                            width,
                            height,
                            antialiasing_method: vello::AaConfig::Area,
                        },
                    )
                    .unwrap();
                let mut encoder = dev.device.create_command_encoder(&Default::default());
                gpu.surface.blitter.copy(
                    &dev.device,
                    &mut encoder,
                    &gpu.surface.target_view,
                    &frame.texture.create_view(&Default::default()),
                );
                dev.queue.submit([encoder.finish()]);
            } else if let Some(canvas) = &mut gpu.canvas {
                canvas.set_size(width, height, 1.0);
                canvas.clear_rect(0, 0, width, height, femtovg::Color::rgb(32, 32, 32));
                dev.queue.submit(canvas.flush_to_output(&frame.texture));
            }
            dev.queue.present(frame);
            dev.device.poll(wgpu::PollType::Poll).unwrap();
        }
        println!(
            "COMMITTED {:.3}",
            self.start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

fn main() {
    let start = Instant::now();
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tiny-skia".into());
    assert!(["tiny-skia", "vello", "femtovg"].contains(&mode.as_str()));
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut events) = registry_queue_init(&conn).unwrap();
    let qh = events.handle();
    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let shell = LayerShell::bind(&globals, &qh).unwrap();
    let shm = Shm::bind(&globals, &qh).unwrap();
    let layer = shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Top,
        Some("bead-dev-spike"),
        None,
    );
    layer.set_anchor(Anchor::TOP);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(512, 32);
    layer.set_exclusive_zone(0);
    layer.commit();
    let pool = SlotPool::new(512 * 32 * 4, &shm).unwrap();
    let mut state = Spike {
        registry: RegistryState::new(&globals),
        output: OutputState::new(&globals, &qh),
        shm,
        pool,
        gpu: None,
        layer,
        conn,
        mode,
        start,
        ready: false,
        exit: false,
    };
    while !state.exit {
        events.blocking_dispatch(&mut state).unwrap();
    }
    drop(state.gpu.take());
}

impl CompositorHandler for Spike {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        if !self.ready {
            self.ready = true;
            println!("READY {:.3}", self.start.elapsed().as_secs_f64() * 1000.0);
        }
        // Deliberately no next frame request: static surfaces must sleep.
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
impl OutputHandler for Spike {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl ShmHandler for Spike {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl LayerShellHandler for Spike {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        config: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.draw(
            qh,
            if config.new_size.0 == 0 {
                512
            } else {
                config.new_size.0
            },
            if config.new_size.1 == 0 {
                32
            } else {
                config.new_size.1
            },
        );
    }
}
delegate_registry!(Spike);
impl ProvidesRegistryState for Spike {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
smithay_client_toolkit::delegate_dispatch2!(Spike);

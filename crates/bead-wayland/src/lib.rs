// SPDX-License-Identifier: MIT OR Apache-2.0
//! Minimal compositor-neutral layer-surface lifecycle. No shell policy or IPC.
use calloop::{
    EventLoop, LoopSignal,
    signals::{Signal, Signals},
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
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::{error::Error, time::Instant};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};

/// Safe development defaults: a click-through strip that does not reserve space.
pub struct Options {
    pub namespace: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            namespace: "bead-dev".into(),
        }
    }
}

struct Strip {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    width: u32,
    height: u32,
    scale: i32,
    configured: bool,
    ready: bool,
}

struct App {
    registry: RegistryState,
    output: OutputState,
    compositor: CompositorState,
    layers: LayerShell,
    shm: Shm,
    strips: Vec<Strip>,
    options: Options,
    signal: LoopSignal,
    failure: Option<String>,
    started: Instant,
}

/// Run until SIGINT/SIGTERM, including output add/remove and configure events.
pub fn run(options: Options) -> Result<(), Box<dyn Error>> {
    if options.namespace.is_empty()
        || options.namespace.len() > 128
        || options.namespace.contains('\0')
    {
        return Err("namespace must contain 1–128 bytes and no NUL".into());
    }
    let started = Instant::now();
    let conn = Connection::connect_to_env()?;
    let (globals, events) = registry_queue_init(&conn)?;
    let qh = events.handle();
    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), events).insert(event_loop.handle())?;
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?;
    event_loop
        .handle()
        .insert_source(signals, |_, _, app| app.signal.stop())?;
    let mut app = App {
        registry: RegistryState::new(&globals),
        output: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh)?,
        layers: LayerShell::bind(&globals, &qh)?,
        shm: Shm::bind(&globals, &qh)?,
        strips: Vec::new(),
        options,
        signal: event_loop.get_signal(),
        failure: None,
        started,
    };
    println!(
        "START pid={} namespace={}",
        std::process::id(),
        app.options.namespace
    );
    // Initial outputs already discovered during registry initialization.
    for output in app.output.outputs() {
        app.add_output(&qh, output);
    }
    let dispatched = event_loop.run(None, &mut app, |_| {});
    // Destroy protocol surfaces while the connection is still alive; flush their
    // destructor requests before returning, rather than relying on process death.
    app.strips.clear();
    let _ = conn.flush();
    println!("STOP surfaces=0");
    dispatched?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}

impl App {
    fn fail(&mut self, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        self.signal.stop();
    }

    fn add_output(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.strips.iter().any(|strip| strip.output == output) {
            return;
        }
        let result = (|| -> Result<Strip, Box<dyn Error>> {
            let surface = self.compositor.create_surface(qh);
            let empty = Region::new(&self.compositor)?;
            surface.set_input_region(Some(empty.wl_region()));
            let layer = self.layers.create_layer_surface(
                qh,
                surface,
                Layer::Top,
                Some(&self.options.namespace),
                Some(&output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
            layer.set_size(0, 32);
            layer.set_exclusive_zone(0);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            let scale = self
                .output
                .info(&output)
                .map_or(1, |info| info.scale_factor.max(1));
            Ok(Strip {
                output,
                layer,
                pool: SlotPool::new(4096, &self.shm)?,
                width: 0,
                height: 32,
                scale,
                configured: false,
                ready: false,
            })
        })();
        match result {
            Ok(strip) => self.strips.push(strip),
            Err(error) => self.fail(error),
        }
    }

    fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let result = (|| -> Result<(), Box<dyn Error>> {
            let strip = &mut self.strips[index];
            if !strip.configured {
                return Ok(());
            }
            let (width, height, stride, _) =
                buffer_dimensions(strip.width, strip.height, strip.scale)?;
            let mut pixels = tiny_skia::Pixmap::new(width as u32, height as u32)
                .ok_or("could not allocate strip pixels")?;
            pixels.fill(tiny_skia::Color::from_rgba8(32, 32, 32, 255));
            let (buffer, canvas) =
                strip
                    .pool
                    .create_buffer(width, height, stride, wl_shm::Format::Argb8888)?;
            // Neutral grayscale RGBA is also valid native-endian ARGB8888 on our
            // little-endian Linux target. Colored content belongs to Feature 2.
            #[cfg(target_endian = "little")]
            canvas.copy_from_slice(pixels.data());
            #[cfg(target_endian = "big")]
            for (dst, src) in canvas
                .chunks_exact_mut(4)
                .zip(pixels.data().chunks_exact(4))
            {
                dst.copy_from_slice(&[src[3], src[0], src[1], src[2]]);
            }
            strip.layer.wl_surface().set_buffer_scale(strip.scale);
            strip.layer.wl_surface().damage_buffer(0, 0, width, height);
            strip
                .layer
                .wl_surface()
                .frame(qh, FrameCallbackData(strip.layer.wl_surface().clone()));
            buffer.attach_to(strip.layer.wl_surface())?;
            strip.layer.commit();
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(error);
        }
    }
}

fn buffer_dimensions(
    width: u32,
    height: u32,
    scale: i32,
) -> Result<(i32, i32, i32, usize), Box<dyn Error>> {
    let scale = u32::try_from(scale).map_err(|_| "invalid output scale")?;
    if width == 0 || height == 0 || scale == 0 {
        return Err("empty surface or scale".into());
    }
    let w = width.checked_mul(scale).ok_or("buffer width overflow")?;
    let h = height.checked_mul(scale).ok_or("buffer height overflow")?;
    let stride = w.checked_mul(4).ok_or("buffer stride overflow")?;
    let size = u64::from(stride) * u64::from(h);
    if size > 64 * 1024 * 1024 {
        return Err("strip buffer exceeds 64 MiB".into());
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
        if let Some(index) = self
            .strips
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        {
            self.strips[index].scale = scale.max(1);
            self.draw(index, qh);
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
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some(strip) = self
            .strips
            .iter_mut()
            .find(|s| s.layer.wl_surface() == surface)
            && !strip.ready
        {
            strip.ready = true;
            println!(
                "READY output={} size={}x{} scale={} elapsed_ms={:.3}",
                strip.output.id().protocol_id(),
                strip.width,
                strip.height,
                strip.scale,
                self.started.elapsed().as_secs_f64() * 1000.0
            );
        }
        // No animation or polling: do not request another frame while idle.
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
        self.strips.retain(|s| s.output != output);
        println!("OUTPUT_REMOVED remaining={}", self.strips.len());
    }
}
impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.strips.retain(|s| s.layer != *layer);
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        config: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let Some(index) = self.strips.iter().position(|s| s.layer == *layer) {
            let strip = &mut self.strips[index];
            strip.width = config.new_size.0;
            strip.height = config.new_size.1;
            strip.configured = true;
            self.draw(index, qh);
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
    use super::buffer_dimensions;
    #[test]
    fn scales_real_output_without_overflow() {
        assert_eq!(
            buffer_dimensions(3840, 32, 2).unwrap(),
            (7680, 64, 30720, 1966080)
        );
    }
    #[test]
    fn rejects_invalid_or_unbounded_compositor_geometry() {
        for (w, h, s) in [
            (0, 32, 1),
            (512, 0, 1),
            (512, 32, -1),
            (u32::MAX, 32, 2),
            (3840, 2160, 8),
        ] {
            assert!(buffer_dimensions(w, h, s).is_err());
        }
    }
}

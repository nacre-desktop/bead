// SPDX-License-Identifier: GPL-3.0-only
//! `bead-wallpaper-video`: a small, product-neutral motion-wallpaper renderer. It is a
//! separate process on purpose (the shell UI never links or decodes video); a crash here
//! only removes this process's surfaces, so the static wallpaper underneath stays visible.
//!
//! One libmpv decoder serves every output, on per-output background layer surfaces (no
//! input). This keeps motion wallpaper in niri's shared overview backdrop; the shell's static
//! wallpaper is mapped first on the same layer and remains visible beneath poster/fade frames.
//! With a GPU the decoder's dmabuf frames are drawn by mpv's OpenGL render API into
//! an EGL window surface (no CPU pixel work); without one, mpv's software render API fills
//! shared-memory buffers from a copy-back decoder. Either falls back to software decoding on
//! its own. Protocol: `bead_wallpaper::protocol` JSON lines on stdin/stdout.
mod gl;
mod mpv;

use bead_wallpaper::protocol::{self, Command, Decoder, Event, Source};
use calloop::{
    EventLoop, LoopHandle, LoopSignal,
    channel::{self, Channel},
    ping::make_ping,
    signals::{Signal, Signals},
    timer::{TimeoutAction, Timer},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
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
    collections::BTreeSet,
    io::{BufRead, Write},
    sync::Arc,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::registry_queue_init,
    protocol::{wl_callback, wl_output, wl_shm, wl_surface},
};
use wayland_protocols::wp::alpha_modifier::v1::client::{
    wp_alpha_modifier_surface_v1::WpAlphaModifierSurfaceV1, wp_alpha_modifier_v1::WpAlphaModifierV1,
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};

const NAMESPACE: &str = "bead-wallpaper-video";
/// A frame callback that does not arrive within this long means the output is not being
/// presented (switched off, or fully occluded): stop decoding until it comes back.
const STARVED_AFTER: Duration = Duration::from_millis(1500);
const MAX_BUFFERS: usize = 2;
const FADE_DURATION: Duration = Duration::from_millis(120);
const FADE_TICK: Duration = Duration::from_millis(16);
const FADE_START: f32 = 0.08;
const ALPHA_MAX: u32 = u32::MAX;

/// `*` in the render list stands for every output (used when the controller knows none).
fn wants(render: &BTreeSet<String>, name: &str) -> bool {
    render.contains(name) || render.contains("*")
}

fn fade_factor(alpha: f32) -> u32 {
    (f64::from(alpha.clamp(0.0, 1.0)) * f64::from(ALPHA_MAX)) as u32
}

fn emit(event: &Event) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(protocol::encode(event).as_bytes());
    let _ = out.flush();
}

/// Report the decoder in use, once per player, after its first frame (it is unknown earlier).
fn report_hwdec(player: &mpv::Player, reported: &mut bool) {
    if !*reported && let Some(mode) = player.hwdec() {
        *reported = true;
        emit(&Event::Hwdec { mode });
    }
}

/// Size and scale a surface for its next frame and ask for its frame callback.
fn place(
    viewport: Option<&WpViewport>,
    logical: (i32, i32),
    scale: i32,
    surface: &wl_surface::WlSurface,
    qh: &QueueHandle<Engine>,
    generation: u64,
) {
    if let Some(viewport) = viewport {
        viewport.set_destination(logical.0.max(1), logical.1.max(1));
        surface.set_buffer_scale(1);
    } else {
        surface.set_buffer_scale(scale.max(1));
    }
    surface.frame(qh, Frame(surface.clone(), generation));
}

struct Out {
    output: wl_output::WlOutput,
    name: String,
    layer: LayerSurface,
    viewport: Option<WpViewport>,
    pool: SlotPool,
    /// EGL window surface when frames are presented through the GPU path.
    egl: Option<gl::Window>,
    buffers: Vec<Buffer>,
    /// Index of the buffer last attached.
    current: Option<usize>,
    /// Buffer size in pixels and the logical surface size it is shown at.
    pixels: (u32, u32),
    logical: (i32, i32),
    scale: i32,
    configured: bool,
    shown: bool,
    pending: Option<Instant>,
    stale: bool,
    first_frame: bool,
    first_frame_pending: Option<u64>,
    alpha: Option<WpAlphaModifierSurfaceV1>,
    fade: Option<(Instant, u64)>,
    opacity: f32,
}

/// Which presentation path the renderer uses. `Untried` waits for a configured output.
enum GlPath {
    Untried,
    Ready(Box<gl::Gl>),
    Off,
}

enum GlStatus {
    Ready,
    Wait,
    Off,
}

struct Engine {
    conn: Connection,
    gl: GlPath,
    registry: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layers: LayerShell,
    shm: Shm,
    viewporter: Option<WpViewporter>,
    alpha_modifier: Option<WpAlphaModifierV1>,
    qh: QueueHandle<Engine>,
    handle: LoopHandle<'static, Engine>,
    signal: LoopSignal,
    wake: Arc<calloop::ping::Ping>,
    hwdec: &'static str,
    /// `--render=shm` never tries EGL.
    shm_only: bool,
    vaapi_device: Option<String>,
    /// The decoder in use is reported after each source's first rendered frame.
    hwdec_reported: bool,
    outs: Vec<Out>,
    source: Option<Source>,
    source_generation: u64,
    decoder: Decoder,
    render: BTreeSet<String>,
    player: Option<mpv::Player>,
    /// Suppress the previous source frame until mpv confirms the replacement loaded.
    source_loaded: bool,
    frame_after_load: bool,
    /// Where to restart after the decoder was released.
    resume_s: f64,
    /// Failed to start or crashed; no retry until a new source.
    failed: bool,
    starved: bool,
    timer_armed: bool,
    fade_timer_armed: bool,
}

impl Engine {
    fn command(&mut self, command: Command) {
        match command {
            Command::Quit => self.signal.stop(),
            Command::Source { source } => {
                self.source_generation = self.source_generation.wrapping_add(1);
                // Keep mpv and its EGL context warm across wallpaper changes.
                let reuse = match (&source, self.player.as_ref()) {
                    (Some(Source::Video { .. }), Some(player)) => {
                        !player.is_gl() || self.bind_any()
                    }
                    (None, Some(player)) => {
                        player.set_paused(true);
                        player.stop();
                        true
                    }
                    _ => false,
                };
                let loaded = if reuse && source.is_some() {
                    let player = self.player.as_ref().unwrap();
                    player.set_paused(true);
                    let _ = player.notices();
                    if player.frame_ready() {
                        player.skip();
                    }
                    let Some(Source::Video { path, fit, looped }) = &source else {
                        unreachable!()
                    };
                    player.load(path, *fit, *looped, 0.).is_ok()
                } else {
                    false
                };
                if !loaded && source.is_some() {
                    self.drop_player();
                } else if loaded {
                    eprintln!("bead-wallpaper-video: reusing mpv render context");
                }
                self.source_loaded = false;
                self.frame_after_load = false;
                self.hwdec_reported = false;
                self.resume_s = 0.0;
                self.failed = false;
                self.starved = false;
                self.render.clear();
                self.decoder = Decoder::Release;
                self.unmap_all();
                for out in &mut self.outs {
                    out.first_frame = false;
                    out.first_frame_pending = None;
                    out.pending = None;
                }
                self.source = source;
                if self.source.is_some() {
                    self.ensure_outputs();
                }
            }
            Command::State { decoder, render } => {
                let new: BTreeSet<String> = render.into_iter().collect();
                for out in &mut self.outs {
                    if wants(&new, &out.name) && !wants(&self.render, &out.name) {
                        out.stale = true;
                    }
                }
                self.render = new;
                self.decoder = decoder;
                self.reconcile();
                for index in 0..self.outs.len() {
                    self.draw(index);
                }
            }
        }
    }

    /// Bring the decoder in line with the wanted state.
    fn reconcile(&mut self) {
        let wanted = self.source.is_some() && self.decoder != Decoder::Release && !self.failed;
        if !wanted {
            if self.source.is_some() && self.player.is_some() {
                self.resume_s = self.drop_player().unwrap_or(self.resume_s);
                self.compact();
            }
            return;
        }
        if self.player.is_none() {
            self.start_player();
        }
        if let Some(player) = &self.player {
            player.set_paused(self.decoder == Decoder::Pause || self.starved);
        }
    }

    /// Free the decoder (with its GL context current when it renders through EGL) and
    /// return where it was.
    fn drop_player(&mut self) -> Option<f64> {
        let player = self.player.take()?;
        if player.is_gl() {
            self.bind_any();
        }
        player.position()
    }

    /// Make the EGL context current on any output window (libmpv needs it to render or free).
    fn bind_any(&mut self) -> bool {
        let Self { gl, outs, .. } = self;
        let GlPath::Ready(gl) = gl else { return false };
        outs.iter_mut().any(|out| {
            let size = out.pixels;
            out.egl.as_mut().is_some_and(|window| gl.bind(window, size))
        })
    }

    /// Bring up EGL on the first configured output, once. Any failure (no GPU, software GL,
    /// no libEGL) turns the GPU path off for good and shared memory takes over.
    fn ensure_gl(&mut self) -> GlStatus {
        match self.gl {
            GlPath::Ready(_) => return GlStatus::Ready,
            GlPath::Off => return GlStatus::Off,
            GlPath::Untried => {}
        }
        if self.shm_only || !gl::render_node_present() {
            if !self.shm_only {
                eprintln!("bead-wallpaper-video: shared-memory presentation (no GPU render node)");
            }
            self.gl = GlPath::Off;
            return GlStatus::Off;
        }
        let Some(out) = self
            .outs
            .iter_mut()
            .find(|o| o.configured && o.pixels.0 > 0 && o.pixels.1 > 0)
        else {
            return GlStatus::Wait;
        };
        let surface = out.layer.wl_surface().id().as_ptr().cast();
        let display = self.conn.backend().display_ptr().cast();
        // SAFETY: both are live libwayland proxies owned by this process's connection.
        match unsafe { gl::Gl::new(display, surface, out.pixels) } {
            Ok((gl, window)) => {
                out.egl = Some(window);
                self.gl = GlPath::Ready(Box::new(gl));
                GlStatus::Ready
            }
            Err(error) => {
                eprintln!("bead-wallpaper-video: shared-memory presentation ({error})");
                self.disable_gl();
                GlStatus::Off
            }
        }
    }

    fn disable_gl(&mut self) {
        for out in &mut self.outs {
            out.egl = None;
            out.stale = true;
        }
        self.gl = GlPath::Off;
    }

    fn start_player(&mut self) {
        let Some(Source::Video { path, fit, looped }) = self.source.clone() else {
            return;
        };
        let use_gl = match self.ensure_gl() {
            GlStatus::Wait => return,
            GlStatus::Ready => self.bind_any(),
            GlStatus::Off => false,
        };
        let init = match (&self.gl, use_gl) {
            (GlPath::Ready(gl), true) => Some(mpv::GlInit {
                get_proc_address: gl::get_proc_address,
                ctx: gl.proc_context(),
                wl_display: gl.wl_display(),
            }),
            _ => None,
        };
        let result = mpv::Player::new(
            self.hwdec,
            self.vaapi_device.as_deref(),
            init.as_ref(),
            self.wake.clone(),
        )
        .and_then(|player| {
            player
                .load(&path, fit, looped, self.resume_s)
                .map(|()| player)
        });
        match result {
            Ok(player) => {
                self.source_loaded = false;
                self.frame_after_load = false;
                self.hwdec_reported = false;
                self.player = Some(player);
            }
            Err(message) if use_gl => {
                // The GPU path could not start; try again through shared memory.
                eprintln!("bead-wallpaper-video: shared-memory presentation ({message})");
                self.disable_gl();
                self.start_player();
            }
            Err(message) => {
                self.failed = true;
                emit(&Event::Error { message });
            }
        }
    }

    /// The mpv wakeup: events and a possible new frame.
    fn woken(&mut self) {
        let Some(player) = &self.player else { return };
        let mut failure = None;
        for notice in player.notices() {
            match notice {
                mpv::Notice::Loaded => self.source_loaded = true,
                mpv::Notice::Failed(message) => failure = Some(message),
            }
        }
        if let Some(message) = failure {
            self.drop_player();
            self.failed = true;
            emit(&Event::Error { message });
            return;
        }
        if player.frame_ready() {
            if !self.source_loaded {
                player.skip();
                return;
            }
            self.frame_after_load = true;
            for out in &mut self.outs {
                if wants(&self.render, &out.name) {
                    out.stale = true;
                }
            }
            let mut drawn = false;
            for index in 0..self.outs.len() {
                drawn |= self.draw(index);
            }
            if !drawn && let Some(player) = &self.player {
                // Consume the frame so mpv keeps its clock moving.
                player.skip();
            }
        }
    }

    fn draw(&mut self, index: usize) -> bool {
        if !self.frame_after_load {
            return false;
        }
        let Self {
            player,
            outs,
            render,
            hwdec_reported,
            gl,
            qh,
            source_generation,
            ..
        } = self;
        let (Some(player), Some(out)) = (player.as_ref(), outs.get_mut(index)) else {
            return false;
        };
        if !out.configured || !out.stale || out.pending.is_some() || !wants(render, &out.name) {
            return false;
        }
        let (width, height) = out.pixels;
        if width == 0 || height == 0 {
            return false;
        }
        let surface = out.layer.wl_surface().clone();
        let opacity_available =
            matches!(&*gl, GlPath::Ready(context) if context.has_opacity_fade());
        let begin_fade =
            !out.first_frame && (out.alpha.is_some() || player.is_gl() && opacity_available);
        if begin_fade && out.alpha.is_none() {
            out.opacity = FADE_START;
        }
        if let (true, GlPath::Ready(gl)) = (player.is_gl(), &*gl) {
            if out.egl.is_none() {
                // SAFETY: the surface proxy and display belong to this live connection.
                match unsafe { gl.window(surface.id().as_ptr().cast(), out.pixels) } {
                    Ok(window) => out.egl = Some(window),
                    Err(_) => return false,
                }
            }
            let Some(window) = out.egl.as_mut() else {
                return false;
            };
            if !gl.bind(window, (width, height)) || !player.render_gl(width, height) {
                return false;
            }
            report_hwdec(player, hwdec_reported);
            if begin_fade && let Some(alpha) = &out.alpha {
                alpha.set_multiplier(fade_factor(FADE_START));
            }
            if out.alpha.is_none() && out.opacity < 1.0 {
                gl.apply_opacity(out.opacity, (width, height));
            }
            // The swap attaches the new dmabuf and commits these requests with it.
            place(
                out.viewport.as_ref(),
                out.logical,
                out.scale,
                &surface,
                qh,
                *source_generation,
            );
            if !gl.swap(window) {
                return false;
            }
        } else {
            let stride = (width as usize * 4).next_multiple_of(64);
            let mut chosen = None;
            for (i, buffer) in out.buffers.iter_mut().enumerate() {
                if buffer.canvas(&mut out.pool).is_some() {
                    chosen = Some(i);
                    break;
                }
            }
            if chosen.is_none() && out.buffers.len() < MAX_BUFFERS {
                match out.pool.create_buffer(
                    width as i32,
                    height as i32,
                    stride as i32,
                    wl_shm::Format::Xrgb8888,
                ) {
                    Ok((buffer, _)) => {
                        out.buffers.push(buffer);
                        chosen = Some(out.buffers.len() - 1);
                    }
                    Err(_) => return false,
                }
            }
            let Some(i) = chosen else { return false };
            let Some(canvas) = out.buffers[i].canvas(&mut out.pool) else {
                return false;
            };
            if !player.render(width, height, stride, canvas) {
                return false;
            }
            report_hwdec(player, hwdec_reported);
            if begin_fade && let Some(alpha) = &out.alpha {
                alpha.set_multiplier(fade_factor(FADE_START));
            }
            place(
                out.viewport.as_ref(),
                out.logical,
                out.scale,
                &surface,
                qh,
                *source_generation,
            );
            if out.buffers[i].attach_to(&surface).is_err() {
                return false;
            }
            surface.damage_buffer(0, 0, width as i32, height as i32);
            out.layer.commit();
            out.current = Some(i);
        }
        out.shown = true;
        out.stale = false;
        out.pending = Some(Instant::now());
        if !out.first_frame {
            out.first_frame = true;
            out.first_frame_pending = Some(*source_generation);
            if begin_fade {
                out.fade = Some((Instant::now(), *source_generation));
            }
        }
        self.arm_starve_timer();
        if begin_fade {
            self.arm_fade_timer();
        }
        true
    }

    fn arm_starve_timer(&mut self) {
        if self.timer_armed {
            return;
        }
        self.timer_armed = true;
        let _ = self
            .handle
            .insert_source(Timer::from_duration(STARVED_AFTER), |_, _, engine| {
                engine.timer_armed = false;
                engine.check_starved();
                TimeoutAction::Drop
            });
    }

    fn arm_fade_timer(&mut self) {
        if self.fade_timer_armed || !self.outs.iter().any(|out| out.fade.is_some()) {
            return;
        }
        self.fade_timer_armed = true;
        let _ = self
            .handle
            .insert_source(Timer::from_duration(FADE_TICK), |_, _, engine| {
                if engine.update_fades() {
                    TimeoutAction::ToDuration(FADE_TICK)
                } else {
                    engine.fade_timer_armed = false;
                    TimeoutAction::Drop
                }
            });
    }

    fn update_fades(&mut self) -> bool {
        let now = Instant::now();
        let mut active = false;
        for out in &mut self.outs {
            let Some((started, generation)) = out.fade else {
                continue;
            };
            if generation != self.source_generation {
                out.fade = None;
                continue;
            }
            let elapsed = now.duration_since(started);
            let progress = (elapsed.as_secs_f32() / FADE_DURATION.as_secs_f32()).min(1.0);
            let factor = FADE_START + (1.0 - FADE_START) * progress;
            if let Some(alpha) = &out.alpha {
                alpha.set_multiplier(fade_factor(factor));
                out.layer.wl_surface().commit();
            } else {
                out.opacity = factor;
                out.stale = true;
            }
            if progress >= 1.0 {
                out.fade = None;
            } else {
                active = true;
            }
        }
        if self.player.as_ref().is_some_and(mpv::Player::is_gl) {
            for index in 0..self.outs.len() {
                if self.outs[index].alpha.is_none() && self.outs[index].stale {
                    self.draw(index);
                }
            }
        }
        active
    }

    fn check_starved(&mut self) {
        let now = Instant::now();
        let stalled = self.decoder == Decoder::Play
            && self.player.is_some()
            && self.outs.iter().any(|out| {
                wants(&self.render, &out.name)
                    && out
                        .pending
                        .is_some_and(|since| now.duration_since(since) >= STARVED_AFTER)
            });
        if stalled && !self.starved {
            self.starved = true;
            self.reconcile();
        } else if self.player.is_some() && self.outs.iter().any(|o| o.pending.is_some()) {
            self.arm_starve_timer();
        }
    }

    /// Drop the spare buffer and shrink the pool to the one frame that stays on screen.
    fn compact(&mut self) {
        for out in &mut self.outs {
            let Some(current) = out.current.filter(|_| out.buffers.len() > 1) else {
                continue;
            };
            let (width, height) = out.pixels;
            let stride = (width as usize * 4).next_multiple_of(64);
            let len = stride * height as usize;
            let Ok(mut pool) = SlotPool::new(len.max(4096), &self.shm) else {
                continue;
            };
            let Ok((buffer, canvas)) = pool.create_buffer(
                width as i32,
                height as i32,
                stride as i32,
                wl_shm::Format::Xrgb8888,
            ) else {
                continue;
            };
            let slot = out.buffers[current].slot();
            let old = out.pool.raw_data_mut(&slot);
            let count = old.len().min(canvas.len());
            canvas[..count].copy_from_slice(&old[..count]);
            let surface = out.layer.wl_surface().clone();
            if buffer.attach_to(&surface).is_err() {
                continue;
            }
            surface.damage_buffer(0, 0, width as i32, height as i32);
            out.layer.commit();
            out.pool = pool;
            out.buffers = vec![buffer];
            out.current = Some(0);
        }
    }

    fn unmap_all(&mut self) {
        for out in &mut self.outs {
            if out.shown {
                if let Some(alpha) = &out.alpha {
                    alpha.set_multiplier(ALPHA_MAX);
                }
                let surface = out.layer.wl_surface();
                surface.attach(None, 0, 0);
                out.layer.commit();
                out.configured = false;
            }
            out.buffers.clear();
            out.current = None;
            out.shown = false;
            out.pending = None;
            out.stale = true;
            out.fade = None;
            out.opacity = 1.0;
            if let Ok(pool) = SlotPool::new(4096, &self.shm) {
                out.pool = pool;
            }
        }
    }

    fn ensure_outputs(&mut self) {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        for output in outputs {
            self.ensure_output(&output);
        }
    }

    fn ensure_output(&mut self, output: &wl_output::WlOutput) {
        if self.source.is_none() {
            return;
        }
        let Some(info) = self.output_state.info(output) else {
            return;
        };
        let Some(name) = info.name.clone() else {
            return;
        };
        let scale = info.scale_factor.max(1);
        let mode = info
            .modes
            .iter()
            .find(|mode| mode.current)
            .map(|mode| mode.dimensions);
        if let Some(index) = self.outs.iter().position(|o| o.output == *output) {
            let out = &mut self.outs[index];
            out.scale = scale;
            if !out.configured {
                // A layer surface needs a fresh empty commit and configure after unmap.
                out.layer
                    .set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
                out.layer.set_size(0, 0);
                out.layer.set_exclusive_zone(-1);
                out.layer
                    .set_keyboard_interactivity(KeyboardInteractivity::None);
                out.layer.commit();
            }
            let pixels = Self::pixels(out.viewport.is_some(), mode, out.logical, scale);
            if pixels != out.pixels {
                out.pixels = pixels;
                out.buffers.clear();
                out.current = None;
                out.stale = true;
                if let Ok(pool) = SlotPool::new(4096, &self.shm) {
                    out.pool = pool;
                }
            }
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        if let Ok(empty) = Region::new(&self.compositor) {
            surface.set_input_region(Some(empty.wl_region()));
        }
        let viewport = self
            .viewporter
            .as_ref()
            .map(|manager| manager.get_viewport(&surface, &self.qh, ()));
        let alpha = self
            .alpha_modifier
            .as_ref()
            .map(|manager| manager.get_surface(&surface, &self.qh, ()));
        let layer = self.layers.create_layer_surface(
            &self.qh,
            surface,
            Layer::Background,
            Some(NAMESPACE),
            Some(output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        let Ok(pool) = SlotPool::new(4096, &self.shm) else {
            return;
        };
        self.outs.push(Out {
            output: output.clone(),
            name,
            layer,
            viewport,
            pool,
            egl: None,
            buffers: Vec::new(),
            current: None,
            pixels: (0, 0),
            logical: (0, 0),
            scale,
            configured: false,
            shown: false,
            pending: None,
            stale: true,
            first_frame: false,
            first_frame_pending: None,
            alpha,
            fade: None,
            opacity: 1.0,
        });
    }

    /// With a viewporter the buffer is the output's native mode, scaled to the logical
    /// size by the compositor; without one it is the logical size times the integer scale.
    fn pixels(
        viewporter: bool,
        mode: Option<(i32, i32)>,
        logical: (i32, i32),
        scale: i32,
    ) -> (u32, u32) {
        let (w, h) = match (viewporter, mode) {
            (true, Some(mode)) => mode,
            _ => (logical.0 * scale, logical.1 * scale),
        };
        if w <= 0 || h <= 0 || w > 16_384 || h > 16_384 {
            return (0, 0);
        }
        (w as u32, h as u32)
    }

    fn frame_done(&mut self, surface: &wl_surface::WlSurface, generation: u64) {
        if generation != self.source_generation {
            return;
        }
        let Some(index) = self
            .outs
            .iter()
            .position(|o| o.layer.wl_surface() == surface)
        else {
            return;
        };
        self.outs[index].pending = None;
        if self.outs[index].first_frame_pending.take() == Some(generation) {
            emit(&Event::FirstFrame {
                output: self.outs[index].name.clone(),
            });
        }
        if self.starved {
            self.starved = false;
            self.reconcile();
        }
        self.draw(index);
    }

    fn remove_layer(&mut self, layer: &LayerSurface) {
        self.outs.retain(|out| out.layer != *layer);
    }
}

impl CompositorHandler for Engine {
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
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
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
/// Frame callbacks carry their surface so presentation stalls can be told per output.
struct Frame(wl_surface::WlSurface, u64);
impl Dispatch<wl_callback::WlCallback, Frame> for Engine {
    fn event(
        engine: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        data: &Frame,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            engine.frame_done(&data.0, data.1);
        }
    }
}
delegate_noop!(Engine: ignore WpViewporter);
delegate_noop!(Engine: ignore WpViewport);
delegate_noop!(Engine: ignore WpAlphaModifierV1);
delegate_noop!(Engine: ignore WpAlphaModifierSurfaceV1);
impl OutputHandler for Engine {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.ensure_output(&output);
    }
    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.ensure_output(&output);
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.outs.retain(|out| out.output != output);
    }
}
impl ShmHandler for Engine {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}
impl LayerShellHandler for Engine {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.remove_layer(layer);
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self.outs.iter().position(|o| o.layer == *layer) else {
            return;
        };
        let mode = self
            .output_state
            .info(&self.outs[index].output)
            .and_then(|info| info.modes.iter().find(|m| m.current).map(|m| m.dimensions));
        let out = &mut self.outs[index];
        let logical = (configure.new_size.0 as i32, configure.new_size.1 as i32);
        let pixels = Self::pixels(out.viewport.is_some(), mode, logical, out.scale);
        if pixels != out.pixels {
            out.buffers.clear();
            out.current = None;
            if let Ok(pool) = SlotPool::new(4096, &self.shm) {
                out.pool = pool;
            }
            out.stale = true;
        }
        out.logical = logical;
        out.pixels = pixels;
        out.configured = true;
        // A start that was waiting for a first output to put EGL on can go ahead now.
        self.reconcile();
        self.draw(index);
    }
}
delegate_registry!(Engine);
impl ProvidesRegistryState for Engine {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}
smithay_client_toolkit::delegate_dispatch2!(Engine);

fn run(
    hwdec: &'static str,
    vaapi_device: Option<String>,
    shm_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init(&conn)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<Engine> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone())?;
    let (ping, ping_source) = make_ping()?;
    handle.insert_source(ping_source, |(), _, engine| engine.woken())?;
    handle.insert_source(
        Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?,
        |_, _, engine| engine.signal.stop(),
    )?;
    let (sender, receiver): (_, Channel<Command>) = channel::channel();
    handle.insert_source(receiver, |event, _, engine| match event {
        channel::Event::Msg(command) => engine.command(command),
        channel::Event::Closed => engine.signal.stop(),
    })?;
    std::thread::Builder::new()
        .name("wallpaper-stdin".into())
        .spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                match protocol::decode::<Command>(&line) {
                    Ok(command) => {
                        if sender.send(command).is_err() {
                            return;
                        }
                    }
                    Err(error) => emit(&Event::Error {
                        message: format!("bad command: {error}"),
                    }),
                }
            }
            let _ = sender.send(Command::Quit);
        })?;
    let alpha_modifier = globals.bind(&qh, 1..=1, ()).ok();
    if alpha_modifier.is_none() {
        eprintln!(
            "bead-wallpaper-video: compositor lacks alpha-modifier-v1; GL opacity fallback will be used when available"
        );
    }
    let mut engine = Engine {
        conn: conn.clone(),
        gl: GlPath::Untried,
        shm_only,
        registry: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh)?,
        layers: LayerShell::bind(&globals, &qh)?,
        shm: Shm::bind(&globals, &qh)?,
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        alpha_modifier,
        qh,
        handle: handle.clone(),
        signal: event_loop.get_signal(),
        wake: Arc::new(ping),
        hwdec,
        vaapi_device,
        hwdec_reported: false,
        outs: Vec::new(),
        source: None,
        source_generation: 0,
        decoder: Decoder::Release,
        render: BTreeSet::new(),
        player: None,
        source_loaded: false,
        frame_after_load: false,
        resume_s: 0.0,
        failed: false,
        starved: false,
        timer_armed: false,
        fade_timer_armed: false,
    };
    emit(&Event::Ready {
        pid: std::process::id(),
    });
    event_loop.run(None, &mut engine, |_| {})?;
    // libmpv frees its GL objects with the EGL context current, so it goes first.
    engine.drop_player();
    engine.disable_gl();
    Ok(())
}

fn main() {
    // Large frame buffers should be mapped and returned immediately, and a couple of arenas
    // keep `malloc_trim` effective, so releasing the decoder really gives its memory back.
    // SAFETY: mallopt before any other thread exists.
    unsafe {
        libc::mallopt(-3, 256 * 1024); // M_MMAP_THRESHOLD (also disables the dynamic raise)
        libc::mallopt(-8, 2); // M_ARENA_MAX
    }
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    if options.check {
        // Whether this host can play at all (libmpv present); for diagnostics.
        if let Err(error) = mpv::available() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run(options.hwdec, options.vaapi_device, options.shm_only) {
        eprintln!("bead-wallpaper-video: {error}");
        std::process::exit(1);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    hwdec: &'static str,
    /// `--render=shm`: never try the EGL/GPU path.
    shm_only: bool,
    vaapi_device: Option<String>,
    check: bool,
}

const USAGE: &str = "usage: bead-wallpaper-video [--hwdec=vaapi|auto|no] [--render=auto|shm] [--vaapi-device=/dev/dri/renderD128] [--check]";

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, &'static str> {
    let mut options = Options {
        // VA-API is the default; `auto` lets mpv pick any API.
        hwdec: "vaapi",
        shm_only: false,
        vaapi_device: None,
        check: false,
    };
    for arg in args {
        match arg.as_str() {
            "--hwdec=no" => options.hwdec = "no",
            "--hwdec=vaapi" => options.hwdec = "vaapi",
            "--hwdec=auto" => options.hwdec = "auto",
            "--render=shm" => options.shm_only = true,
            "--render=auto" => options.shm_only = false,
            "--check" => options.check = true,
            device if device.starts_with("--vaapi-device=/") => {
                options.vaapi_device = Some(device["--vaapi-device=".len()..].to_string());
            }
            _ => return Err(USAGE),
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, &'static str> {
        parse_args(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn defaults_are_vaapi_with_the_gpu_path_allowed() {
        let options = parse(&[]).unwrap();
        assert_eq!(options.hwdec, "vaapi");
        assert!(!options.shm_only && options.vaapi_device.is_none() && !options.check);
    }

    #[test]
    fn render_and_decoder_flags() {
        let options = parse(&["--render=shm", "--hwdec=auto"]).unwrap();
        assert!(options.shm_only);
        assert_eq!(options.hwdec, "auto");
        assert!(!parse(&["--render=shm", "--render=auto"]).unwrap().shm_only);
        assert_eq!(parse(&["--hwdec=no"]).unwrap().hwdec, "no");
        assert_eq!(
            parse(&["--vaapi-device=/dev/dri/renderD129"])
                .unwrap()
                .vaapi_device
                .as_deref(),
            Some("/dev/dri/renderD129")
        );
    }

    #[test]
    fn unknown_or_relative_arguments_are_refused() {
        assert!(parse(&["--hwdec=cuda"]).is_err());
        assert!(parse(&["--vaapi-device=renderD129"]).is_err());
        assert!(parse(&["--render=gl"]).is_err());
    }
}

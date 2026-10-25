// SPDX-License-Identifier: MIT OR Apache-2.0
//! B0045: one on-demand xdg toplevel window hosting a product `SceneSource`.
//!
//! The framework knows nothing about the window's content: the product supplies a
//! factory (run on the script worker, like a panel), a model mapper for deep-link
//! arguments and the title/app id/size limits. Nothing is resident until the first
//! `bead.window open`; closing drops the source, renderer and every buffer.
use super::*;
use smithay_client_toolkit::shell::xdg::{
    XdgShell,
    window::{Window, WindowConfigure, WindowDecorations, WindowHandler},
};
use std::sync::Arc;

pub type WindowFactory =
    Arc<dyn Fn(&[serde_json::Value]) -> Result<Box<dyn SceneSource>, String> + Send + Sync>;
/// Maps `open` arguments to a store patch, delivered to a window that is already open.
pub type WindowModel = Arc<
    dyn Fn(&[serde_json::Value]) -> std::collections::BTreeMap<String, bead_core::Value>
        + Send
        + Sync,
>;

pub type WindowSizing = Arc<dyn Fn((u32, u32)) -> (u32, u32) + Send + Sync>;

#[derive(Clone)]
pub struct WindowSpec {
    pub app_id: String,
    pub title: String,
    /// Initial logical size when the compositor does not suggest one.
    pub size: (u32, u32),
    pub min_size: (u32, u32),
    /// Optional product sizing policy, evaluated against the selected logical output.
    pub size_for_output: Option<WindowSizing>,
    pub fallback: Scene,
    pub factory: WindowFactory,
    pub model: WindowModel,
}

pub(super) struct Win {
    window: Window,
    epoch: u64,
    source: Box<dyn SceneSource>,
    scene: Scene,
    renderer: Box<dyn Renderer>,
    pool: Option<SlotPool>,
    buffers: Vec<Buffer>,
    geometry: (i32, i32),
    size: (u32, u32),
    preferred_size: (u32, u32),
    scale120: u32,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    configured: bool,
    pressed: bool,
    cursor_serial: u32,
    last_draw: Option<Instant>,
    timer: bool,
    drawn: u64,
    first_frame_texts: Vec<String>,
}
impl Win {
    pub(super) fn scroll_regions(&self) -> Vec<Rect> {
        self.source.scroll_regions()
    }
    pub(super) fn wheel_step(&self) -> f64 {
        self.source.wheel_step()
    }
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }
    pub(super) fn has_fractional(&self) -> bool {
        self.fractional.is_some()
    }
}
impl Drop for Win {
    fn drop(&mut self) {
        if let Some(f) = self.fractional.take() {
            f.destroy();
        }
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
    }
}

/// Shared by layer surfaces and windows: the editing keys the UI understands.
pub(super) fn key_command(keysym: Keysym, ctrl: bool) -> Option<&'static str> {
    Some(match keysym {
        Keysym::Escape => "escape",
        Keysym::Tab | Keysym::ISO_Left_Tab => "tab",
        Keysym::Return | Keysym::KP_Enter => "enter",
        Keysym::Left => "left",
        Keysym::Right => "right",
        Keysym::Up => "up",
        Keysym::Down => "down",
        Keysym::Home => "home",
        Keysym::End => "end",
        Keysym::Delete => "delete",
        Keysym::a if ctrl => "select-all",
        Keysym::c if ctrl => "clipboard-copy",
        Keysym::x if ctrl => "clipboard-cut",
        Keysym::v if ctrl => "clipboard-paste",
        _ => return None,
    })
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(id) = self.slot_of_surface(window.wl_surface()) {
            if let Err(error) = self.close_pearl(id) {
                eprintln!("PEARL_CLOSE_ERROR id={id} {error}");
            }
            return;
        }
        if self.window.as_ref().is_some_and(|w| &w.window == window) {
            self.close_window();
        }
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        if let Some(id) = self.slot_of_surface(window.wl_surface()) {
            self.in_slot(id, |app| app.native_window_configure(window, configure));
            return;
        }
        let Some(win) = self.window.as_mut().filter(|w| &w.window == window) else {
            return;
        };
        let spec = self.options.window.as_ref();
        let fallback = win.preferred_size;
        let min = spec.map_or((1, 1), |s| s.min_size);
        let width = configure
            .new_size
            .0
            .map_or(fallback.0, |v| v.get())
            .max(min.0);
        let height = configure
            .new_size
            .1
            .map_or(fallback.1, |v| v.get())
            .max(min.1);
        let changed = win.size != (width, height) || !win.configured;
        win.size = (width, height);
        win.configured = true;
        if changed {
            if let Some(v) = &win.viewport {
                v.set_destination(width as i32, height as i32);
            }
            win.source.viewport(width, height);
            self.window_ui(UiEvent::Relayout);
        }
    }
}

impl App {
    pub(super) fn bind_xdg(
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<Self>,
    ) -> Option<XdgShell> {
        XdgShell::bind(globals, qh).ok()
    }
    pub(super) fn window_surface(&self) -> Option<&wl_surface::WlSurface> {
        self.window.as_ref().map(|w| w.window.wl_surface())
    }
    pub(super) fn window_command(&mut self, request: std::sync::Arc<Remote>) -> bool {
        let call = &request.call;
        if call.target != "bead.window" {
            return false;
        }
        if call.method == "state" && std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() {
            let reply = bead_services::ipc::Reply::ok(
                call.id,
                serde_json::json!({
                    "size": self.window.as_ref().map(|w| w.size),
                    "drawn": self.window.as_ref().map(|w| w.drawn),
                    "first_frame_texts": self.window.as_ref().map(|w| &w.first_frame_texts),
                    "regions": self.window.as_ref().map(|w| w.source.input_regions().into_iter().map(|r|
                        serde_json::json!({"id":r.id,"rect":[r.rect.x,r.rect.y,r.rect.w,r.rect.h]})).collect::<Vec<_>>()),
                    "stores": self.window.as_ref().and_then(|w| serde_json::from_str::<serde_json::Value>(&w.source.state_summary()).ok())
                }),
            );
            let _ = request.reply.send(reply);
            return true;
        }
        let result = match call.method.as_str() {
            "open" => self.open_window(&call.args),
            "close" => {
                self.close_window();
                Ok(())
            }
            _ => Err("unknown window method".into()),
        };
        let reply = match result {
            Ok(()) => bead_services::ipc::Reply::ok(
                call.id,
                serde_json::json!({"open": self.window.is_some()}),
            ),
            Err(e) => bead_services::ipc::Reply::error(call.id, "window", e),
        };
        let _ = request.reply.send(reply);
        true
    }
    fn open_window(&mut self, args: &[serde_json::Value]) -> Result<(), String> {
        let spec = self.options.window.clone().ok_or("window not configured")?;
        if self.window.is_some() {
            let patch = (spec.model)(args);
            if !patch.is_empty() {
                self.window_ui(UiEvent::Model(patch));
            }
            return Ok(());
        }
        let xdg = self.xdg.as_ref().ok_or("compositor has no xdg-shell")?;
        let qh = self.qh.clone();
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
        let window = xdg.create_window(surface, WindowDecorations::RequestServer, &qh);
        window.set_title(spec.title.clone());
        window.set_app_id(spec.app_id.clone());
        window.set_min_size(Some(spec.min_size));
        window.commit();
        self.window_epoch = self.window_epoch.wrapping_add(1);
        let epoch = self.window_epoch;
        let factory = spec.factory.clone();
        let factory_args = args.to_vec();
        let mut source: Box<dyn SceneSource> = Box::new(worker::Source::new(
            Box::new(move || factory(&factory_args)),
            spec.fallback.clone(),
        ));
        let sender = self
            .event_sender
            .clone()
            .ok_or("UI event channel unavailable")?;
        let notify = sender.clone();
        source.services(Box::new(move |event| {
            let _ = notify.send(UiEvent::Window(epoch, Box::new(event)));
        }));
        source
            .watch(Box::new(move |result| {
                let event = match result {
                    Ok(()) => UiEvent::Reload,
                    Err(e) => UiEvent::WatchError(e),
                };
                let _ = sender.send(UiEvent::Window(epoch, Box::new(event)));
            }))
            .map_err(|e| e.to_string())?;
        let output_size = self
            .output
            .outputs()
            .filter_map(|o| self.output.info(&o))
            .filter(|info| {
                self.options
                    .output
                    .as_ref()
                    .is_none_or(|name| info.name.as_ref() == Some(name))
            })
            .find_map(|info| info.logical_size)
            .map(|(w, h)| (w.max(1) as u32, h.max(1) as u32));
        let size = match (&spec.size_for_output, output_size) {
            (Some(policy), Some(output)) => policy(output),
            _ => spec.size,
        };
        source.viewport(size.0, size.1);
        let renderer = bead_render::renderer(RendererKind::Cpu).map_err(|e| e.to_string())?;
        self.window = Some(Win {
            window,
            epoch,
            source,
            scene: spec.fallback.clone(),
            renderer,
            pool: None,
            buffers: Vec::new(),
            geometry: (0, 0),
            size,
            preferred_size: size,
            scale120: 120,
            fractional,
            viewport,
            configured: false,
            pressed: false,
            cursor_serial: 0,
            last_draw: None,
            timer: false,
            drawn: 0,
            first_frame_texts: Vec::new(),
        });
        self.window_ui(UiEvent::Reload);
        if let Some(palette) = self.wallpaper_palette.clone() {
            self.window_ui(UiEvent::WallpaperPalette(palette));
        }
        let patch = (spec.model)(args);
        if !patch.is_empty() {
            self.window_ui(UiEvent::Model(patch));
        }
        println!("WINDOW_OPEN epoch={epoch}");
        Ok(())
    }
    /// App mode start: open the window now, or queue the default panel for the first output.
    pub(super) fn app_start(&mut self) {
        let Some(args) = self.options.app.clone() else {
            return;
        };
        if self.options.window.is_some() {
            if let Err(error) = self.open_window(&args) {
                self.fail(error);
            }
        } else {
            self.app_pending = Some(args);
            self.app_open_pending();
        }
    }
    /// Called on output changes: open the pending app panel on the first output that can place it.
    pub(super) fn app_open_pending(&mut self) {
        if self.app_pending.is_none() {
            return;
        }
        let Some(info) = self
            .output
            .outputs()
            .filter_map(|o| self.output.info(&o))
            .find(|i| {
                i.logical_size.is_some_and(|(w, h)| w > 0 && h > 0)
                    && self
                        .options
                        .output
                        .as_ref()
                        .is_none_or(|n| i.name.as_ref() == Some(n))
            })
        else {
            return;
        };
        self.app_pending = None;
        if self.options.app_as_pearl {
            let view = self
                .options
                .panels
                .as_ref()
                .and_then(|panels| panels.resolve(None).ok())
                .map(|(view, _)| view.to_owned());
            if let Some(view) = view
                && let Err(error) = self.open_pearl(&view, info.name.as_deref(), None)
            {
                self.fail(error);
            }
            return;
        }
        if let Err(error) = self.request_panel_at(None, info.name.as_deref(), true, None) {
            self.fail(error);
        }
    }
    pub(super) fn close_window(&mut self) {
        if self.window.take().is_none() {
            return;
        }
        if self.options.app.is_some() {
            self.signal.stop();
        }
        if self
            .keyboard_surface
            .as_ref()
            .is_some_and(|s| !s.is_alive())
        {
            self.keyboard_surface = None;
        }
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        // SAFETY: allocator trim only; closing releases the source, renderer and buffers.
        unsafe {
            libc::malloc_trim(0);
        }
        // The accessibility bridge keeps the last tree it was given: hand it the bar's again
        // so the closed window's nodes are not retained.
        let tree = self
            .output_sessions
            .values()
            .find_map(|s| s.source.as_ref().and_then(|s| s.accessibility_tree()))
            .or_else(|| self.runtime.as_ref().and_then(|r| r.accessibility_tree()));
        if let (Some(bridge), Some(tree)) = (&mut self.accessibility, tree) {
            bridge.update(tree);
        }
        // Worker packets queued before the close still own their rasters until the event
        // loop discards them: trim once more after that queue has drained (as panels do).
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let _ = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(150)),
            |_, _, app| {
                if app.window.is_none() {
                    // SAFETY: allocator trim only.
                    unsafe {
                        libc::malloc_trim(0);
                    }
                }
                TimeoutAction::Drop
            },
        );
        println!("WINDOW_CLOSED resident=0");
    }
    pub(super) fn window_scale(&mut self, scale120: u32) {
        if let Some(win) = self.window.as_mut()
            && win.scale120 != scale120
        {
            win.scale120 = scale120;
            self.window_ui(UiEvent::Relayout);
        }
    }
    /// Service wakeups and input for the window; the product source stages them.
    pub(super) fn window_ui(&mut self, event: UiEvent) {
        let Some(win) = self.window.as_mut() else {
            return;
        };
        win.source.scales(vec![win.scale120 as f32 / 120.]);
        let scale = win.scale120 as f32 / 120.;
        let staged = win.source.stage(event).and_then(|scene| {
            if let Some(scene) = &scene {
                win.renderer.prepare(scene, scale)?;
            }
            win.source.validate()?;
            Ok(scene)
        });
        let scene = match staged {
            Ok(Some(scene)) => {
                if !win.source.commit() {
                    return;
                }
                if let (Some(bridge), Some(tree)) =
                    (&mut self.accessibility, win.source.accessibility_tree())
                {
                    bridge.update(tree);
                }
                scene
            }
            Ok(None) => return,
            Err(e) => {
                eprintln!("UI_ERROR {e}");
                win.source.error_scene(e.to_string())
            }
        };
        win.scene = scene;
        self.window_draw();
    }
    fn window_draw(&mut self) {
        let Some(win) = self.window.as_mut() else {
            return;
        };
        if !win.configured {
            return;
        }
        if let Some(last) = win.last_draw
            && last.elapsed() < Duration::from_millis(8)
        {
            if !win.timer {
                win.timer = true;
                let epoch = win.epoch;
                let _ = self.loop_handle.insert_source(
                    Timer::from_duration(Duration::from_millis(8)),
                    move |_, _, app| {
                        if let Some(w) = app.window.as_mut().filter(|w| w.epoch == epoch) {
                            w.timer = false;
                            app.window_draw();
                        }
                        TimeoutAction::Drop
                    },
                );
            }
            return;
        }
        if let Err(e) = self.window_render() {
            eprintln!("WINDOW_DRAW_ERROR {e}");
        }
    }
    fn window_render(&mut self) -> Result<(), Box<dyn Error>> {
        let rgba = cfg!(target_endian = "little")
            && !self.options.force_argb
            && self.shm.formats().contains(&wl_shm::Format::Abgr8888);
        let format = if rgba {
            PixelFormat::Rgba
        } else {
            PixelFormat::Bgra
        };
        let win = self.window.as_mut().ok_or("window closed")?;
        let scale = win.scale120 as f32 / 120.;
        let (w, h, stride, size) = buffer_dimensions(win.size.0, win.size.1, win.scale120)?;
        let scene = &mut win.scene;
        scene.offset = (0., 0.);
        scene.slide = None;
        scene.bounds.x = 0.;
        scene.bounds.y = 0.;
        scene.bounds.w = win.size.0 as f32;
        scene.bounds.h = win.size.1 as f32;
        scene.width = win.size.0 as f32;
        if win.geometry != (w, h) || win.buffers.is_empty() {
            win.buffers.clear();
            let mut pool = SlotPool::new(size.next_multiple_of(64) * 3, &self.shm)?;
            for _ in 0..3 {
                let (buffer, _) = pool.create_buffer(
                    w,
                    h,
                    stride,
                    if rgba {
                        wl_shm::Format::Abgr8888
                    } else {
                        wl_shm::Format::Argb8888
                    },
                )?;
                win.buffers.push(buffer);
            }
            win.pool = Some(pool);
            win.geometry = (w, h);
        }
        let pool = win.pool.as_mut().ok_or("window pool missing")?;
        let Some(slot) = win
            .buffers
            .iter()
            .position(|b| !b.slot().has_active_buffers())
        else {
            // All buffers still held by the compositor: try again shortly.
            if !win.timer {
                win.timer = true;
                let epoch = win.epoch;
                let _ = self.loop_handle.insert_source(
                    Timer::from_duration(Duration::from_millis(8)),
                    move |_, _, app| {
                        if let Some(w) = app.window.as_mut().filter(|w| w.epoch == epoch) {
                            w.timer = false;
                            app.window_draw();
                        }
                        TimeoutAction::Drop
                    },
                );
            }
            return Ok(());
        };
        win.renderer.prepare(scene, scale)?;
        let canvas = win.buffers[slot]
            .canvas(pool)
            .ok_or("compositor owns buffer")?;
        win.renderer.draw(
            scene,
            RenderTarget::Shm(ShmTarget {
                bytes: &mut *canvas,
                width: w as u32,
                height: h as u32,
                scale,
                format,
            }),
            Damage::full(w as u32, h as u32),
        )?;
        let surface = win.window.wl_surface();
        if win.viewport.is_some() {
            surface.set_buffer_scale(1);
        } else {
            surface.set_buffer_scale((win.scale120 / 120).max(1) as i32);
        }
        surface.damage_buffer(0, 0, w, h);
        win.buffers[slot].attach_to(surface)?;
        win.window.commit();
        win.last_draw = Some(Instant::now());
        if win.drawn == 0 && std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() {
            win.first_frame_texts = win
                .scene
                .primitives
                .iter()
                .filter_map(|primitive| match primitive {
                    bead_render::Primitive::Text { text, .. } if !text.is_empty() => {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .take(32)
                .collect();
        }
        win.drawn += 1;
        Ok(())
    }
    pub(super) fn window_key(&mut self, event: &KeyEvent) {
        if self.modifiers.ctrl && matches!(event.keysym, Keysym::w | Keysym::q) {
            self.close_window();
            return;
        }
        match key_command(event.keysym, self.modifiers.ctrl) {
            Some(command) => self.window_ui(UiEvent::Command {
                command: command.into(),
                shift: self.modifiers.shift || event.keysym == Keysym::ISO_Left_Tab,
            }),
            None if !self.modifiers.ctrl && !self.modifiers.alt => self.window_ui(UiEvent::Key {
                text: event.utf8.clone().unwrap_or_default(),
                backspace: event.keysym == Keysym::BackSpace,
            }),
            None => {}
        }
    }
    pub(super) fn window_pointer(&mut self, event: &PointerEvent) {
        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
        let Some(win) = self.window.as_mut() else {
            return;
        };
        if let PointerEventKind::Enter { serial, .. } = event.kind {
            win.cursor_serial = serial;
        }
        if let Some(device) = &self.cursor_device
            && win.cursor_serial != 0
        {
            let shape = match win.source.pointer_cursor(x, y) {
                "text" => wp_cursor_shape_device_v1::Shape::Text,
                "pointer" => wp_cursor_shape_device_v1::Shape::Pointer,
                _ => wp_cursor_shape_device_v1::Shape::Default,
            };
            device.set_shape(win.cursor_serial, shape);
        }
        let pressed = win.pressed;
        let ev = match event.kind {
            PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => Some(if pressed {
                UiEvent::Drag { x, y }
            } else {
                UiEvent::Hover { x, y }
            }),
            PointerEventKind::Leave { .. } => {
                win.pressed = false;
                Some(UiEvent::Hover { x: -1., y: -1. })
            }
            PointerEventKind::Press { button: 0x110, .. } => {
                win.pressed = true;
                Some(UiEvent::Click { x, y })
            }
            PointerEventKind::Release { button: 0x110, .. } => {
                win.pressed = false;
                Some(UiEvent::Release)
            }
            PointerEventKind::Axis {
                vertical,
                horizontal,
                ..
            } => {
                let target = crate::scroll::Target::Window(win.epoch);
                self.axis_scroll(target.clone(), false, (x, y), vertical);
                self.axis_scroll(target, true, (x, y), horizontal);
                None
            }
            _ => None,
        };
        if let Some(ev) = ev {
            self.window_ui(ev);
        }
    }
}

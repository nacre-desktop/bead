// SPDX-License-Identifier: MIT OR Apache-2.0
//! Actor state, worker transactions and per-surface drawing lifecycle.
use super::*;
pub(super) struct App {
    pub(super) subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    pub(super) background: smithay_client_toolkit::background_effect::BackgroundEffectState,
    pub(super) clipboard: clipboard::Clipboard,
    pub(super) last_input_serial: u32,
    pub(super) event_sender: Option<calloop::channel::Sender<UiEvent>>,
    pub(super) dismiss: Option<panel::Dismiss>,
    pub(super) panel_runtime: Option<Box<dyn SceneSource>>,
    pub(super) panel_epoch: u64,
    pub(super) panel_name: Option<String>,
    pub(super) panel_center: Option<f32>,
    pub(super) panel_close_epoch: u64,
    pub(super) panel_reload_pending: bool,
    pub(super) panel_target: bool,
    pub(super) panel_hovered: Option<String>,
    pub(super) panel_output: Option<wl_output::WlOutput>,
    pub(super) processing_panel: bool,
    pub(super) runtime: Option<Box<dyn SceneSource>>,
    pub(super) seat: SeatState,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) keyboard_surface: Option<wl_surface::WlSurface>,
    pub(super) modifiers: Modifiers,
    pub(super) accessibility: Option<accessibility::Bridge>,
    pub(super) cursor_manager: Option<WpCursorShapeManagerV1>,
    pub(super) cursor_device: Option<WpCursorShapeDeviceV1>,
    pub(super) cursor_enter_serial: u32,
    pub(super) pointer_pressed: bool,
    pub(super) text_input_manager: Option<ZwpTextInputManagerV3>,
    pub(super) text_input: Option<ZwpTextInputV3>,
    pub(super) ime_surface: Option<wl_surface::WlSurface>,
    pub(super) ime_pending: (String, Option<String>, u32, u32),
    pub(super) ime_last: Option<(String, i32, i32)>,
    pub(super) reload_pending: bool,
    pub(super) ui_initialized: bool,
    pub(super) ui_serial: u64,
    pub(super) primary_ui_serial: u64,
    pub(super) panel_ui_serial: u64,
    pub(super) qh: QueueHandle<App>,
    pub(super) loop_handle: LoopHandle<'static, App>,
    pub(super) release_timer: bool,
    pub(super) companion_policy: policy::Policy,
    #[cfg(feature = "wgpu")]
    pub(super) gpu: Option<bead_render::gpu::Transport>,
    #[cfg(feature = "native-gl")]
    pub(super) gl: Option<bead_render::gl::Transport>,
    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    pub(super) conn: Connection,
    pub(super) registry: RegistryState,
    pub(super) output: OutputState,
    pub(super) compositor: CompositorState,
    pub(super) layers: LayerShell,
    pub(super) shm: Shm,
    pub(super) fractional: Option<WpFractionalScaleManagerV1>,
    pub(super) viewporter: Option<WpViewporter>,
    pub(super) presentation: Option<WpPresentation>,
    pub(super) surfaces: Vec<Surface>,
    pub(super) report_surfaces: Vec<serde_json::Value>,
    pub(super) options: Options,
    pub(super) signal: LoopSignal,
    pub(super) failure: Option<String>,
    pub(super) started: Instant,
    pub(super) scene: Scene,
    pub(super) renderer: Box<dyn Renderer>,
    pub(super) companion: Option<(Scene, Box<dyn Renderer>, RendererKind)>,
    pub(super) animations: bead_core::animation::Animations,
}
pub fn run_runtime(
    mut options: Options,
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
    let panel_name = if let Some(panels) = &options.panels {
        let (name, spec) = panels.resolve(None)?;
        options.panel_spec = Some(spec.clone());
        Some(name.to_owned())
    } else {
        None
    };
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
        .map(|(s, k)| {
            let effective = if options.lazy_gl && *k == RendererKind::FemtovgGl {
                RendererKind::Cpu
            } else {
                *k
            };
            bead_render::renderer(effective).map(|r| (s.clone(), r, effective))
        })
        .transpose()?;
    let background =
        smithay_client_toolkit::background_effect::BackgroundEffectState::new(&globals, &qh);
    println!(
        "BACKGROUND_EFFECT requested={} advertised={}",
        options.background_blur,
        background.ext_background_effect_manager_v1().is_ok()
    );
    let mut app = App {
        subcompositor: globals.bind(&qh, 1..=1, ()).ok(),
        background,
        clipboard: clipboard::Clipboard::new(&globals, &qh),
        last_input_serial: 0,
        event_sender: None,
        dismiss: None,
        panel_runtime: None,
        panel_epoch: 0,
        panel_name,
        panel_center: None,
        panel_close_epoch: 0,
        panel_reload_pending: false,
        panel_target: false,
        panel_hovered: None,
        panel_output: None,
        processing_panel: false,
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
        pointer_pressed: false,
        text_input_manager: globals.bind(&qh, 1..=1, ()).ok(),
        text_input: None,
        ime_surface: None,
        ime_pending: Default::default(),
        ime_last: None,
        reload_pending: false,
        ui_initialized: false,
        ui_serial: 0,
        primary_ui_serial: 0,
        panel_ui_serial: 0,
        qh: qh.clone(),
        loop_handle: loop_.handle(),
        release_timer: false,
        companion_policy: Default::default(),
        #[cfg(feature = "wgpu")]
        gpu: None,
        #[cfg(feature = "native-gl")]
        gl: None,
        #[cfg(any(feature = "native-gl", feature = "wgpu"))]
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
        report_surfaces: Vec::new(),
        options,
        signal: loop_.get_signal(),
        failure: None,
        started,
        scene,
        renderer,
        companion,
        animations: Default::default(),
    };
    if app.options.interactive {
        for seat in app.seat.seats() {
            app.clipboard.seat(&seat, &qh);
        }
    }
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
        app.event_sender = Some(sender.clone());
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
    let _commands = if let Some(path) = &app.options.command_socket {
        let (sender, receiver) = calloop::channel::channel::<UiEvent>();
        loop_.handle().insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(event) = event {
                app.ui(event);
            }
        })?;
        let command_map = app.options.command_map.clone();
        Some(bead_services::ipc::Server::bind(
            path.clone(),
            move |call| {
                let call = if let Some(map) = &command_map {
                    match map(call) {
                        Ok(c) => c,
                        Err(reply) => return reply,
                    }
                } else {
                    call
                };
                let id = call.id;
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                if sender
                    .send(UiEvent::Remote(std::sync::Arc::new(Remote {
                        call,
                        reply: tx,
                        context: Default::default(),
                    })))
                    .is_err()
                {
                    return bead_services::ipc::Reply::error(id, "closed", "UI stopped");
                }
                rx.recv_timeout(Duration::from_secs(1)).unwrap_or_else(|_| {
                    bead_services::ipc::Reply::error(
                        id,
                        "timeout",
                        "command execution deadline exceeded",
                    )
                })
            },
        )?)
    } else {
        None
    };
    for output in app.output.outputs() {
        app.add_output(&qh, output);
    }
    if let Some(ms) = app.options.debug_luau_stall_ms {
        loop_.handle().insert_source(
            Timer::from_duration(Duration::from_millis(4100)),
            move |_, _, app| {
                app.ui(UiEvent::DebugScriptStall(ms));
                TimeoutAction::Drop
            },
        )?;
    }
    if app.options.debug_policy_cycle {
        for (seconds, active) in [(4, true), (6, false)] {
            let qh = qh.clone();
            loop_.handle().insert_source(
                Timer::from_duration(Duration::from_secs(seconds)),
                move |_, _, app| {
                    app.options.animate = active;
                    for i in 0..app.surfaces.len() {
                        app.surfaces[i].callbacks.invalidate();
                        app.draw(i, &qh);
                    }
                    TimeoutAction::Drop
                },
            )?;
        }
    }
    if std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() == Ok("1")
        && let Ok(value) = std::env::var("BEAD_PRIVATE_MAIN_LOOP_STALL_MS")
        && let Ok(ms) = value.parse::<u64>()
        && (6000..=10000).contains(&ms)
    {
        loop_.handle().insert_source(
            Timer::from_duration(Duration::from_secs(2)),
            move |_, _, _| {
                eprintln!(
                    "WATCHDOG_PROBE main_stall_begin monotonic_ns={}",
                    monotonic_ns()
                );
                std::thread::sleep(Duration::from_millis(ms));
                TimeoutAction::Drop
            },
        )?;
    }
    let watchdog = bead_core::recovery::Watchdog::start(Duration::from_secs(6))?;
    // Five seconds stalled plus one dispatch interval of grace: sleeping is not
    // charged as a stall. No high-frequency heartbeat while idle.
    let result = loop_.run(Some(Duration::from_secs(1)), &mut app, |_| watchdog.beat());
    drop(watchdog);
    for s in &app.surfaces {
        println!(
            "ACQUIRE p50_ms={:.3} p99_ms={:.3}",
            percentile(&s.acquire_ms, 0.5),
            percentile(&s.acquire_ms, 0.99)
        );
        println!(
            "METRICS companion={} output={} frames={} allocations={} render_p50_ms={:.3} render_p99_ms={:.3} callback_p50_ms={:.3} callback_p99_ms={:.3} presented_p50_ms={:.3} presented_p99_ms={:.3} presented_samples={} refresh_ns={} kind={} discarded={} swap_p50_ms={:.3} swap_p99_ms={:.3} swap_max_ms={:.3} activate_p99_ms={:.3} frame_p50_ms={:.3} frame_p99_ms={:.3} frame_max_ms={:.3}",
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
            percentile(&s.activate_ms, 0.99),
            percentile(&s.frame_ms, 0.50),
            percentile(&s.frame_ms, 0.99),
            s.frame_ms
                .iter()
                .copied()
                .reduce(f64::max)
                .unwrap_or(f64::NAN)
        );
    }
    if let Some(path) = &app.options.report {
        report::save(&app, path)?;
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
impl App {
    pub(super) fn fail(&mut self, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        self.signal.stop();
    }
    pub(super) fn add_output(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let primary = self.options.output.as_ref().is_none_or(|name| {
            self.output.info(&output).and_then(|i| i.name).as_ref() == Some(name)
        });
        let selected = self.options.panel_spec.is_none()
            || (self.panel_target && self.panel_output.as_ref() == Some(&output));
        if !primary && !(selected && self.companion.is_some()) {
            return;
        }
        let count = self.surfaces.iter().filter(|s| s.output == output).count()
            + if primary {
                0
            } else {
                self.options.surface_count
            };
        let total = self.options.surface_count + usize::from(self.companion.is_some() && selected);
        let companion = count == self.options.surface_count;
        if count >= total {
            return;
        }
        let result = (|| -> Result<Surface, Box<dyn Error>> {
            let surface = self.compositor.create_surface(qh);
            let empty = Region::new(&self.compositor)?;
            if !self.options.interactive
                || (companion
                    && (self.options.panel_spec.is_none()
                        || self.options.panel_spec.as_ref().is_some_and(|s| s.passive)))
            {
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
                match scene
                    .motion
                    .as_ref()
                    .map(|m| m.edge)
                    .unwrap_or(self.options.edge)
                {
                    bead_core::Edge::Left => Anchor::LEFT | Anchor::TOP,
                    bead_core::Edge::Right => Anchor::RIGHT | Anchor::TOP,
                    bead_core::Edge::Bottom => Anchor::BOTTOM,
                    bead_core::Edge::Top => Anchor::TOP,
                }
            } else if self.options.panel {
                Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
            } else {
                match self.options.edge {
                    bead_core::Edge::Top => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
                    bead_core::Edge::Bottom => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                    bead_core::Edge::Left => Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                    bead_core::Edge::Right => Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                }
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
                self.position_panel_surface(&layer, &output, scene);
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
            layer.set_keyboard_interactivity(
                if !self.options.test_passive
                    && ((companion
                        && self
                            .options
                            .panel_spec
                            .as_ref()
                            .is_some_and(|s| s.keyboard_focus))
                        || (self.options.test_keyboard && !companion))
                {
                    KeyboardInteractivity::Exclusive
                } else if self.options.interactive && self.options.keyboard_focus && !companion {
                    KeyboardInteractivity::OnDemand
                } else {
                    KeyboardInteractivity::None
                },
            );
            layer.commit();
            let scale120 = self
                .output
                .info(&output)
                .map_or(120, |i| i.scale_factor.max(1) as u32 * 120);
            Ok(Surface {
                decorations: None,
                background: None,
                background_size: None,
                logical_size: (0, 0),
                background_visible: false,
                companion,
                output: output.clone(),
                layer,
                pool: SlotPool::new(4096, &self.shm)?,
                #[cfg(feature = "wgpu")]
                gpu: None,
                #[cfg(feature = "native-gl")]
                gl: None,
                buffers: Vec::new(),
                width: 0,
                height: 32,
                scale120,
                configured: false,
                ready: false,
                callbacks: Default::default(),
                input_visible: None,
                input_rect: None,
                keyboard_visible: None,
                redraw: false,
                fractional,
                viewport,
                geometry: (0, 0),
                offset: (0., 0.),
                format: PixelFormat::Bgra,
                previous: None,
                visual_damage: Default::default(),
                allocations: 0,
                frames: 0,
                render_ms: Vec::new(),
                frame_ms: Vec::new(),
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
                recovered_generation: None,
                ui_serial: if companion {
                    self.panel_ui_serial
                } else {
                    self.primary_ui_serial
                },
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
}

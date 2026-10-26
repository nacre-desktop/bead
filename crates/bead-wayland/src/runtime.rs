// SPDX-License-Identifier: MIT OR Apache-2.0
//! Actor state, worker transactions and per-surface drawing lifecycle.
use super::*;
const MAX_WALLPAPER_RGBA_BYTES: usize = 64 * 1024 * 1024;
const MAX_WALLPAPER_DIMENSION: u32 = 16_384;
pub(super) struct App {
    pub(super) scrolls: scroll::State,
    pub(super) subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    pub(super) background: smithay_client_toolkit::background_effect::BackgroundEffectState,
    pub(super) clipboard: clipboard::Clipboard,
    pub(super) capture: Option<capture::Capturer>,
    pub(super) last_input_serial: u32,
    pub(super) event_sender: Option<calloop::channel::Sender<UiEvent>>,
    pub(super) dismiss: Option<panel::Dismiss>,
    pub(super) hidden: bool,
    pub(super) bar_off: bool,
    pub(super) decoration_variant: u64,
    pub(super) decoration_cache: super::decorations::RasterCache,
    pub(super) edge_surfaces: Vec<edges::EdgeSurface>,
    pub(super) edge: edges::EdgeState,
    pub(super) panel_runtime: Option<Box<dyn SceneSource>>,
    pub(super) panel_epoch: u64,
    pub(super) panel_name: Option<String>,
    pub(super) panel_center: Option<f32>,
    pub(super) panel_anchor: Option<PanelAnchor>,
    pub(super) panel_canvas_owner: Option<u64>,
    pub(super) panel_close_epoch: u64,
    pub(super) panel_cover: Option<(u64, Vec<Surface>)>,
    pub(super) panel_dock_transition: Option<pearls::DockTransition>,
    /// The panel's own scene has been committed at least once (no dark/empty first frame before).
    pub(super) panel_ready: bool,
    pub(super) panel_reload_pending: bool,
    pub(super) panel_target: bool,
    pub(super) panel_gpu_suspended: bool,
    #[cfg(feature = "native-gl")]
    pub(super) panel_cpu_cached: bool,
    #[cfg(feature = "native-gl")]
    pub(super) panel_snapshot_pending: bool,
    /// The shown view was offered by a service (see `UiEvent::CompanionOffer`).
    pub(super) panel_ambient: bool,
    pub(super) panel_hovered: Option<String>,
    pub(super) panel_output: Option<wl_output::WlOutput>,
    pub(super) processing_panel: bool,
    pub(super) runtime: Option<Box<dyn SceneSource>>,
    /// Per-output primary sessions used when `primary_factory` is configured.
    pub(super) output_sessions: std::collections::BTreeMap<String, OutputSession>,
    pub(super) active_output: Option<String>,
    pub(super) active_output_epoch: Option<u64>,
    pub(super) next_session_epoch: u64,
    pub(super) active_input_output: Option<String>,
    pub(super) seat: SeatState,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) relative_manager: Option<ZwpRelativePointerManagerV1>,
    pub(super) relative_pointer: Option<ZwpRelativePointerV1>,
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
    pub(super) ime_sensitive: bool,
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
    #[cfg(feature = "native-gl")]
    pub(super) panel_gl_prewarm_error: Option<String>,
    #[cfg(feature = "native-gl")]
    pub(super) gl_preparation: Option<rendering::GlPreparation>,
    pub(super) conn: Connection,
    pub(super) registry: RegistryState,
    pub(super) output: OutputState,
    pub(super) compositor: CompositorState,
    pub(super) layers: LayerShell,
    pub(super) xdg: Option<smithay_client_toolkit::shell::xdg::XdgShell>,
    pub(super) window: Option<window::Win>,
    pub(super) window_epoch: u64,
    /// App mode: the open request still waiting for the first usable output (panel apps).
    pub(super) app_pending: Option<Vec<serde_json::Value>>,
    pub(super) overlay: Option<overlay::Overlay>,
    pub(super) overlay_epoch: u64,
    pub(super) canvases: std::collections::BTreeMap<(String, String), canvas::Canvas>,
    pub(super) canvas_epoch: u64,
    pub(super) shm: Shm,
    pub(super) fractional: Option<WpFractionalScaleManagerV1>,
    pub(super) viewporter: Option<WpViewporter>,
    pub(super) presentation: Option<WpPresentation>,
    pub(super) surfaces: Vec<Surface>,
    pub(super) wallpaper_surfaces: Vec<WallpaperSurface>,
    pub(super) wallpaper_sender: Option<calloop::channel::Sender<UiEvent>>,
    pub(super) wallpaper_jobs: std::collections::BTreeSet<String>,
    pub(super) wallpaper_dirty: std::collections::BTreeSet<String>,
    pub(super) wallpaper_palette: Option<std::collections::BTreeMap<String, [u8; 4]>>,
    pub(super) wallpaper_palette_outputs: Option<PendingPalette>,
    pub(super) wallpaper_pending_frames: Vec<WallpaperFrame>,
    pub(super) wallpaper_unconfirmed: Vec<crate::WallpaperFrameConfirmation>,
    pub(super) report_surfaces: Vec<serde_json::Value>,
    pub(super) options: Options,
    pub(super) signal: LoopSignal,
    pub(super) failure: Option<String>,
    pub(super) started: Instant,
    pub(super) scene: Scene,
    pub(super) renderer: Box<dyn Renderer>,
    pub(super) companion: Option<(Scene, Box<dyn Renderer>, RendererKind)>,
    pub(super) animations: bead_core::animation::Animations,
    pub(super) panel_animations: bead_core::animation::Animations,
    /// Set while a pearl (or guide) is swapped in as the resident panel.
    pub(super) pearl: Option<pearls::Info>,
    pub(super) reserved_pearl_id: Option<u32>,
    pub(super) prewarm_attempted: bool,
    pub(super) slots: Vec<pearls::Group>,
    pub(super) pearl_seq: u32,
    pub(super) epoch_seq: u64,
    pub(super) gesture: Option<pearls::Gesture>,
    pub(super) guide_ids: std::collections::BTreeMap<String, u32>,
    pub(super) guide_last: std::collections::BTreeMap<String, std::sync::Arc<Guide>>,
    pub(super) pending_pearls: Vec<bead_core::pearl::Entry>,
    pub(super) pearl_watch: Option<Box<dyn Send>>,
    pub(super) pearl_save_pending: bool,
    pub(super) pearl_saved: Option<bead_core::pearl::Layout>,
}

pub(super) struct OutputSession {
    pub(super) id: String,
    pub(super) epoch: u64,
    pub(super) source: Option<Box<dyn SceneSource>>,
    pub(super) scene: Scene,
    pub(super) ui_serial: u64,
    pub(super) initialized: bool,
    pub(super) animations: bead_core::animation::Animations,
}

pub(super) fn frame_interval_from_refresh_millihz(refresh_rate: i32) -> Duration {
    match u64::try_from(refresh_rate) {
        Ok(refresh_rate) if refresh_rate > 0 => {
            Duration::from_nanos(1_000_000_000_000 / refresh_rate)
        }
        _ => Duration::from_millis(16),
    }
}

pub(super) struct PendingPalette {
    pub(super) palette: std::collections::BTreeMap<String, [u8; 4]>,
    pub(super) expected: std::collections::BTreeSet<String>,
    pub(super) settled: std::collections::BTreeSet<String>,
    pub(super) accepted: bool,
    pub(super) panel_epoch: Option<u64>,
    pub(super) panel_settled: bool,
}
impl PendingPalette {
    pub(super) fn join_panel(&mut self, epoch: u64) {
        self.panel_epoch = Some(epoch);
        self.panel_settled = false;
    }
    fn settle_panel(&mut self, epoch: u64, accepted: bool) -> bool {
        if self.panel_epoch != Some(epoch) {
            return false;
        }
        self.panel_settled = true;
        self.accepted &= accepted;
        true
    }
    pub(super) fn ready(&self) -> bool {
        self.expected.is_subset(&self.settled) && (self.panel_epoch.is_none() || self.panel_settled)
    }
}

fn frame_matches_palette(
    frame: &WallpaperFrame,
    palette: &std::collections::BTreeMap<String, [u8; 4]>,
) -> bool {
    frame.palette.0.len() == palette.len()
        && frame
            .palette
            .0
            .iter()
            .all(|(key, value)| palette.get(key) == Some(value))
}

fn same_wallpaper_frame(
    frame: &WallpaperFrame,
    output: &str,
    revision: u64,
    token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
) -> bool {
    frame.output == output
        && frame.revision == revision
        && std::sync::Arc::ptr_eq(&frame.commit_token, token)
}

fn frame_is_unconfirmed(
    frame: &WallpaperFrame,
    unconfirmed: &[crate::WallpaperFrameConfirmation],
) -> bool {
    unconfirmed.iter().any(|pending| {
        same_wallpaper_frame(
            frame,
            &pending.output,
            pending.revision,
            &pending.commit_token,
        )
    })
}

fn has_unconfirmed_palette_frame(
    frames: &[WallpaperFrame],
    unconfirmed: &[crate::WallpaperFrameConfirmation],
    palette: &std::collections::BTreeMap<String, [u8; 4]>,
) -> bool {
    frames.iter().any(|frame| {
        frame_matches_palette(frame, palette) && frame_is_unconfirmed(frame, unconfirmed)
    })
}

fn accepted_palette_waits_for_confirmation(
    accepted: bool,
    frames: &[WallpaperFrame],
    unconfirmed: &[crate::WallpaperFrameConfirmation],
    palette: &std::collections::BTreeMap<String, [u8; 4]>,
) -> bool {
    accepted && has_unconfirmed_palette_frame(frames, unconfirmed, palette)
}

fn wallpaper_palette_group_ready(
    waiting: &PendingPalette,
    frames: &[WallpaperFrame],
    unconfirmed: &[crate::WallpaperFrameConfirmation],
) -> bool {
    waiting.ready() && !has_unconfirmed_palette_frame(frames, unconfirmed, &waiting.palette)
}

fn aborted_palette_retry_outputs(
    failed_output: &str,
    has_newer_request: bool,
    frames: &[WallpaperFrame],
    palette: &std::collections::BTreeMap<String, [u8; 4]>,
) -> Vec<String> {
    if !has_newer_request {
        return Vec::new();
    }
    frames
        .iter()
        .filter(|frame| frame.output != failed_output && frame_matches_palette(frame, palette))
        .map(|frame| frame.output.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn output_event_is_current(current_epoch: Option<u64>, event_epoch: u64) -> bool {
    current_epoch == Some(event_epoch)
}

pub(crate) fn output_scope_matches(active_output: Option<&str>, surface_output: &str) -> bool {
    active_output.is_none_or(|active| active == surface_output)
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
        scrolls: Default::default(),
        subcompositor: globals.bind(&qh, 1..=1, ()).ok(),
        background,
        clipboard: clipboard::Clipboard::new(&globals, &qh),
        capture: options
            .capture
            .clone()
            .map(|hub| capture::Capturer::new(hub, &globals, &qh)),
        last_input_serial: 0,
        event_sender: None,
        dismiss: None,
        hidden: false,
        // App mode never creates the bar-shaped primary surface; its one window or panel is the UI.
        bar_off: options.app.is_some(),
        decoration_variant: 0,
        decoration_cache: Default::default(),
        edge_surfaces: Vec::new(),
        edge: Default::default(),
        panel_runtime: None,
        panel_epoch: 0,
        panel_name,
        panel_center: None,
        panel_anchor: None,
        panel_canvas_owner: None,
        panel_close_epoch: 0,
        panel_cover: None,
        panel_dock_transition: None,
        panel_ready: true,
        panel_reload_pending: false,
        panel_target: false,
        panel_gpu_suspended: false,
        #[cfg(feature = "native-gl")]
        panel_cpu_cached: false,
        #[cfg(feature = "native-gl")]
        panel_snapshot_pending: false,
        panel_ambient: false,
        panel_hovered: None,
        panel_output: None,
        processing_panel: false,
        runtime,
        output_sessions: Default::default(),
        active_output: None,
        active_output_epoch: None,
        next_session_epoch: 1,
        active_input_output: None,
        seat: SeatState::new(&globals, &qh),
        keyboard: None,
        pointer: None,
        relative_manager: globals.bind(&qh, 1..=1, ()).ok(),
        relative_pointer: None,
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
        ime_sensitive: false,
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
        #[cfg(feature = "native-gl")]
        panel_gl_prewarm_error: None,
        #[cfg(feature = "native-gl")]
        gl_preparation: None,
        conn: conn.clone(),
        registry: RegistryState::new(&globals),
        output: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh)?,
        layers: LayerShell::bind(&globals, &qh)?,
        xdg: App::bind_xdg(&globals, &qh),
        window: None,
        window_epoch: 0,
        app_pending: None,
        overlay: None,
        overlay_epoch: 0,
        canvases: Default::default(),
        canvas_epoch: 0,
        shm: Shm::bind(&globals, &qh)?,
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        presentation: globals.bind(&qh, 1..=2, ()).ok(),
        surfaces: Vec::new(),
        wallpaper_surfaces: Vec::new(),
        wallpaper_sender: None,
        wallpaper_jobs: Default::default(),
        wallpaper_dirty: Default::default(),
        wallpaper_palette: None,
        wallpaper_palette_outputs: None,
        wallpaper_pending_frames: Vec::new(),
        wallpaper_unconfirmed: Vec::new(),
        report_surfaces: Vec::new(),
        options,
        signal: loop_.get_signal(),
        failure: None,
        started,
        scene,
        renderer,
        companion,
        animations: Default::default(),
        panel_animations: Default::default(),
        pearl: None,
        reserved_pearl_id: None,
        prewarm_attempted: false,
        slots: Vec::new(),
        pearl_seq: 0,
        epoch_seq: 0,
        gesture: None,
        guide_ids: Default::default(),
        guide_last: Default::default(),
        pending_pearls: Vec::new(),
        pearl_watch: None,
        pearl_save_pending: false,
        pearl_saved: None,
    };
    app.load_pearls();
    if let Some(hub) = app.options.capture.clone() {
        app.capture_init(&hub);
    }
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
    if app.runtime.is_some() || app.options.primary_factory.is_some() || app.options.app.is_some() {
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
    if let Some(provider) = app.options.wallpaper.clone() {
        let (sender, receiver) = calloop::channel::channel::<UiEvent>();
        loop_.handle().insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(event) = event {
                app.ui(event);
            }
        })?;
        let changed = sender.clone();
        provider.watch(Box::new(move || {
            let _ = changed.send(UiEvent::WallpaperRefresh);
        }))?;
        app.wallpaper_sender = Some(sender);
    }
    if let Some(feed) = app.options.palette_feed.clone() {
        let (sender, receiver) = calloop::channel::channel::<UiEvent>();
        loop_.handle().insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(event) = event {
                app.ui(event);
            }
        })?;
        app.wallpaper_palette = feed.current();
        feed.watch(Box::new(move |palette| {
            let _ = sender.send(UiEvent::LivePalette(palette));
        }));
    }
    if let Some(control) = app.options.control.clone() {
        let (sender, receiver) = calloop::channel::channel::<UiEvent>();
        loop_.handle().insert_source(receiver, |event, _, app| {
            if let calloop::channel::Event::Msg(event) = event {
                app.ui(event);
            }
        })?;
        control.watch(Box::new(move || {
            let _ = sender.send(UiEvent::ControlChanged);
        }));
        app.bar_off = !control.bar_enabled();
        app.decoration_variant = control.decoration_variant();
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
    app.app_start();
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
    pub(super) fn output_frame_interval(&self, output: &wl_output::WlOutput) -> Duration {
        self.output
            .info(output)
            .and_then(|info| {
                info.modes
                    .iter()
                    .find(|mode| mode.current)
                    .map(|mode| frame_interval_from_refresh_millihz(mode.refresh_rate))
            })
            .unwrap_or_else(|| Duration::from_millis(16))
    }
    fn presented_output_frame_interval(&self, output: &wl_output::WlOutput) -> Duration {
        self.surfaces
            .iter()
            .filter(|surface| surface.output == *output && !surface.companion)
            .filter_map(|surface| (surface.refresh_ns > 0).then_some(surface.refresh_ns))
            .min()
            .map_or_else(|| self.output_frame_interval(output), |ns| Duration::from_nanos(u64::from(ns)))
    }
    fn shared_primary_frame_interval(&self) -> Duration {
        self.output
            .outputs()
            .map(|output| self.presented_output_frame_interval(&output))
            .min()
            .unwrap_or_else(|| Duration::from_millis(16))
    }
    pub(super) fn output_mode_changed(&mut self, output: &wl_output::WlOutput) {
        for surface in self.surfaces.iter_mut().filter(|surface| surface.output == *output) {
            surface.refresh_ns = 0;
        }
        for canvas in self.canvases.values_mut().filter(|canvas| canvas.output == *output) {
            canvas.refresh_ns = 0;
        }
        let name = self.output_id(output);
        let interval = self.output_frame_interval(output);
        if let Some(session) = self.output_sessions.get_mut(&name)
            && let Some(source) = &mut session.source
        {
            source.frame_interval(interval);
        }
        if self.active_output.as_deref() == Some(&name)
            && let Some(source) = &mut self.runtime
        {
            source.frame_interval(interval);
        }
        if self.options.primary_factory.is_none() {
            let shared = self.shared_primary_frame_interval();
            if let Some(source) = &mut self.runtime {
                source.frame_interval(shared);
            }
        }
        if self.panel_output.as_ref() == Some(output)
            && let Some(source) = &mut self.panel_runtime
        {
            source.frame_interval(interval);
        }
        self.pearl_output_mode_changed(output, interval);
        for canvas in self.canvases.values_mut().filter(|canvas| canvas.output == *output) {
            canvas.source.frame_interval(interval);
        }
        self.window_output_mode_changed(&name, interval);
    }
    pub(super) fn output_presented(
        &mut self,
        output: &wl_output::WlOutput,
        output_name: &str,
        companion: bool,
        refresh_ns: u32,
    ) {
        if refresh_ns == 0 {
            return;
        }
        let interval = Duration::from_nanos(u64::from(refresh_ns));
        if companion {
            if self.panel_output.as_ref() == Some(output)
                && let Some(source) = &mut self.panel_runtime
            {
                source.frame_interval(interval);
            }
        } else if self.options.primary_factory.is_some() {
            if self.active_output.as_deref() == Some(output_name) {
                if let Some(source) = &mut self.runtime {
                    source.frame_interval(interval);
                }
            } else if let Some(source) = self
                .output_sessions
                .get_mut(output_name)
                .and_then(|session| session.source.as_mut())
            {
                source.frame_interval(interval);
            }
        } else {
            let fastest = self.shared_primary_frame_interval();
            if let Some(source) = &mut self.runtime {
                source.frame_interval(fastest);
            }
        }
    }
    pub(super) fn output_id(&self, output: &wl_output::WlOutput) -> String {
        self.output
            .info(output)
            .and_then(|i| i.name)
            .unwrap_or_else(|| format!("output-{}", output.id().protocol_id()))
    }

    pub(super) fn fail(&mut self, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        self.signal.stop();
    }
    pub(super) fn ensure_wallpaper(
        &mut self,
        output: &wl_output::WlOutput,
        qh: &QueueHandle<Self>,
    ) {
        let Some(provider) = self.options.wallpaper.clone() else {
            return;
        };
        let Some(info) = self.output.info(output) else {
            return;
        };
        let Some(name) = info.name else {
            return;
        };
        let Some((width, height)) = info
            .modes
            .iter()
            .find(|mode| mode.current)
            .map(|mode| mode.dimensions)
        else {
            return;
        };
        if width <= 0
            || height <= 0
            || width as u32 > MAX_WALLPAPER_DIMENSION
            || height as u32 > MAX_WALLPAPER_DIMENSION
            || (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .is_none_or(|bytes| bytes > MAX_WALLPAPER_RGBA_BYTES)
        {
            return;
        }
        if let Some(surface) = self
            .wallpaper_surfaces
            .iter_mut()
            .find(|surface| surface.output == *output)
        {
            let scale_factor = info.scale_factor.max(1);
            if (surface.width, surface.height, surface.scale_factor)
                == (width as u32, height as u32, scale_factor)
            {
                return;
            }
            surface.width = width as u32;
            surface.height = height as u32;
            surface.scale_factor = scale_factor;
            surface.revision = 0;
            surface.viewport_destination = None;
            surface.pixels = None;
            surface.transition = None;
            surface.buffers.clear();
            surface.staged = None;
            if let Ok(pool) = SlotPool::new(4096, &self.shm) {
                surface.pool = pool;
            } else {
                eprintln!("WALLPAPER_SURFACE_ERROR code=pool_recreate");
            }
            surface.release_wait_ms = 0;
            surface.release_timer_pending = false;
            let _ = surface;
            self.prepare_wallpaper(provider, name, width as u32, height as u32);
            return;
        }
        let result = (|| -> Result<WallpaperSurface, Box<dyn Error>> {
            let surface = self.compositor.create_surface(qh);
            let empty = Region::new(&self.compositor)?;
            surface.set_input_region(Some(empty.wl_region()));
            let viewport = self
                .viewporter
                .as_ref()
                .map(|manager| manager.get_viewport(&surface, qh, ()));
            let layer = self.layers.create_layer_surface(
                qh,
                surface,
                Layer::Background,
                Some("bead-wallpaper"),
                Some(output),
            );
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_size(0, 0);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            Ok(WallpaperSurface {
                palette: None,
                output: output.clone(),
                name: name.clone(),
                width: width as u32,
                height: height as u32,
                revision: 0,
                release_wait_ms: 0,
                release_timer_pending: false,
                scale_factor: info.scale_factor.max(1),
                layer,
                pool: SlotPool::new(4096, &self.shm)?,
                buffers: Vec::with_capacity(2),
                staged: None,
                viewport,
                viewport_destination: None,
                pixels: None,
                transition: None,
            })
        })();
        match result {
            Ok(surface) => {
                println!(
                    "WALLPAPER_SURFACE_CREATED output={} layer=background input=empty",
                    surface.name
                );
                self.wallpaper_surfaces.push(surface)
            }
            Err(_) => {
                eprintln!("WALLPAPER_SURFACE_ERROR code=create");
                return;
            }
        }
        self.prepare_wallpaper(provider, name, width as u32, height as u32);
    }

    fn prepare_wallpaper(
        &mut self,
        provider: std::sync::Arc<dyn WallpaperProvider>,
        name: String,
        width: u32,
        height: u32,
    ) {
        if !self.wallpaper_jobs.insert(name.clone()) {
            self.wallpaper_dirty.insert(name);
            return;
        }
        let Some(sender) = self.wallpaper_sender.clone() else {
            self.wallpaper_jobs.remove(&name);
            return;
        };
        let failed_name = name.clone();
        // A previous raster is needed only after idle released it. During rapid
        // navigation the surface already owns the transition pixels.
        let animate = self.options.animate
            && self
                .wallpaper_surfaces
                .iter()
                .find(|surface| surface.name == name)
                .is_none_or(|surface| surface.pixels.is_none() && surface.transition.is_none());
        let confirmed_prepare = self.options.primary_factory.is_some() && !animate;
        std::thread::spawn(move || {
            if confirmed_prepare {
                let mut identity = None;
                let mut duplicate = false;
                let result = provider.prepare_staged(name.clone(), width, height, &mut |frame| {
                    if identity.is_some() || frame.output != name {
                        duplicate = true;
                        return;
                    }
                    identity = Some(crate::WallpaperFrameConfirmation {
                        output: frame.output.clone(),
                        revision: frame.revision,
                        commit_token: frame.commit_token.clone(),
                        accepted: false,
                    });
                    let _ = sender.send(UiEvent::WallpaperFrame(
                        crate::WallpaperFrameEvent::Staged(std::sync::Arc::new(frame)),
                    ));
                });
                match (result, identity, duplicate) {
                    (Ok(Some(frame)), None, false) => {
                        let _ = sender.send(UiEvent::WallpaperFrame(
                            crate::WallpaperFrameEvent::Ready(std::sync::Arc::new(frame)),
                        ));
                    }
                    (Ok(None), Some(mut confirmation), false) => {
                        confirmation.accepted = true;
                        let _ = sender.send(UiEvent::WallpaperFrame(
                            crate::WallpaperFrameEvent::Confirmation(confirmation),
                        ));
                    }
                    (_result, Some(mut confirmation), _) => {
                        confirmation.accepted = false;
                        let _ = sender.send(UiEvent::WallpaperFrame(
                            crate::WallpaperFrameEvent::Confirmation(confirmation),
                        ));
                    }
                    _ => {
                        let _ = sender.send(UiEvent::WallpaperFailed(failed_name));
                    }
                }
            } else {
                match provider.prepare(name.clone(), width, height) {
                    Ok(mut frame) => {
                        if animate {
                            frame.previous_rgba = provider.prepare_transition(&name, width, height);
                        }
                        let _ = sender.send(UiEvent::WallpaperFrame(
                            crate::WallpaperFrameEvent::Ready(std::sync::Arc::new(frame)),
                        ));
                    }
                    Err(_) => {
                        let _ = sender.send(UiEvent::WallpaperFailed(failed_name));
                    }
                }
            }
        });
    }

    pub(super) fn refresh_wallpapers(&mut self) {
        let outputs: Vec<String> = self
            .wallpaper_surfaces
            .iter()
            .map(|surface| surface.name.clone())
            .collect();
        for output in outputs {
            self.request_wallpaper(&output);
        }
    }

    fn request_wallpaper(&mut self, name: &str) {
        let Some(provider) = self.options.wallpaper.clone() else {
            return;
        };
        let Some(surface) = self
            .wallpaper_surfaces
            .iter()
            .find(|surface| surface.name == name)
        else {
            return;
        };
        let Some((width, height)) = self.output.info(&surface.output).and_then(|info| {
            info.modes
                .iter()
                .find(|mode| mode.current)
                .map(|mode| mode.dimensions)
        }) else {
            return;
        };
        self.prepare_wallpaper(provider, name.to_owned(), width as u32, height as u32);
    }

    pub(super) fn complete_wallpaper(&mut self, frame: WallpaperFrame) {
        self.complete_wallpaper_inner(frame, false);
    }

    pub(super) fn stage_wallpaper_frame(&mut self, frame: WallpaperFrame) {
        self.wallpaper_unconfirmed
            .push(crate::WallpaperFrameConfirmation {
                output: frame.output.clone(),
                revision: frame.revision,
                commit_token: frame.commit_token.clone(),
                accepted: false,
            });
        self.complete_wallpaper_inner(frame, true);
    }

    fn complete_wallpaper_inner(&mut self, frame: WallpaperFrame, staged: bool) {
        if !staged {
            self.wallpaper_jobs.remove(&frame.output);
        }
        let output_name = frame.output.clone();
        let current = self
            .options
            .wallpaper
            .as_ref()
            .is_none_or(|provider| provider.is_current(&frame));
        if !current {
            if let Some(provider) = &self.options.wallpaper {
                provider.discarded(&frame.output, frame.revision);
            }
            self.clear_staged_wallpaper_frame(&frame);
            // A newer request may have arrived while this superseded frame was
            // decoding. Retain coalescing, but do not strand that latest request.
            self.request_dirty_wallpaper(&output_name);
            return;
        }
        let palette_key: std::collections::BTreeMap<String, [u8; 4]> = frame
            .palette
            .0
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect();
        let stage_revision = frame.revision;
        let stage_token = frame.commit_token.clone();
        if self.options.primary_factory.is_some() {
            if self.wallpaper_palette.as_ref() == Some(&palette_key)
                || self.output_sessions.is_empty()
            {
                if staged {
                    self.wallpaper_pending_frames.push(frame);
                    if self.wallpaper_palette_outputs.is_none() {
                        self.stage_pending_wallpaper(&output_name, stage_revision, &stage_token);
                    }
                    return;
                }
                if let Some(control) = &self.options.control {
                    control.palette_changed(&palette_key);
                }
                self.wallpaper_palette = Some(palette_key);
                self.install_wallpaper(frame);
                self.request_dirty_wallpaper(&output_name);
                return;
            }
            if self
                .wallpaper_palette_outputs
                .as_ref()
                .is_some_and(|pending| pending.palette == palette_key)
            {
                self.wallpaper_pending_frames.push(frame);
                self.stage_pending_wallpaper(&output_name, stage_revision, &stage_token);
                self.request_dirty_wallpaper(&output_name);
                return;
            }
            if self.wallpaper_palette_outputs.is_some() {
                self.discard_pending_wallpapers();
            }
            self.wallpaper_pending_frames.push(frame);
            self.prepare_wallpaper_palette_group(palette_key);
            self.stage_pending_wallpaper(&output_name, stage_revision, &stage_token);
            self.request_dirty_wallpaper(&output_name);
            return;
        }
        if self.wallpaper_palette.as_ref() == Some(&palette_key) || self.runtime.is_none() {
            self.set_wallpaper_palette(palette_key);
            self.install_wallpaper(frame);
        } else {
            if self
                .wallpaper_pending_frames
                .first()
                .is_some_and(|pending| pending.palette.0 != frame.palette.0)
            {
                self.discard_pending_wallpapers();
            }
            if self
                .wallpaper_pending_frames
                .first()
                .is_some_and(|pending| pending.palette.0 == frame.palette.0)
            {
                self.wallpaper_pending_frames.push(frame);
                self.stage_pending_wallpaper(&output_name, stage_revision, &stage_token);
                if self.wallpaper_dirty.remove(&output_name) {
                    self.request_wallpaper(&output_name);
                }
                return;
            }
            let palette = frame.palette.clone();
            self.wallpaper_pending_frames.push(frame);
            match self
                .runtime
                .as_mut()
                .map(|runtime| runtime.wallpaper_palette(palette))
            {
                Some(Ok(Some(scene))) => {
                    let committed = self
                        .runtime
                        .as_mut()
                        .is_some_and(|runtime| runtime.commit());
                    if committed {
                        self.scene = scene;
                        self.wallpaper_palette_settled(&palette_key, true);
                        let qh = self.qh.clone();
                        for index in 0..self.surfaces.len() {
                            self.draw(index, &qh);
                        }
                    } else {
                        self.discard_pending_wallpapers();
                    }
                }
                Some(Ok(None)) => {
                    let staged_async = self
                        .runtime
                        .as_ref()
                        .is_some_and(|runtime| runtime.wallpaper_palette_pending());
                    if !staged_async {
                        self.wallpaper_palette_settled(&palette_key, true);
                    }
                }
                Some(Err(_)) => self.discard_pending_wallpapers(),
                None => {
                    self.set_wallpaper_palette(palette_key.clone());
                    self.commit_pending_wallpapers(&palette_key);
                }
            }
            self.stage_pending_wallpaper(&output_name, stage_revision, &stage_token);
        }
        self.request_dirty_wallpaper(&output_name);
    }

    pub(super) fn confirm_staged_wallpaper(
        &mut self,
        confirmation: crate::WallpaperFrameConfirmation,
    ) {
        let Some(index) = self.wallpaper_unconfirmed.iter().position(|pending| {
            pending.output == confirmation.output
                && pending.revision == confirmation.revision
                && std::sync::Arc::ptr_eq(&pending.commit_token, &confirmation.commit_token)
        }) else {
            return;
        };
        self.wallpaper_unconfirmed.remove(index);
        let frame_index = self.wallpaper_pending_frames.iter().position(|frame| {
            frame.output == confirmation.output
                && frame.revision == confirmation.revision
                && std::sync::Arc::ptr_eq(&frame.commit_token, &confirmation.commit_token)
        });
        if let Some(frame_index) = frame_index
            && self
                .wallpaper_palette_outputs
                .as_ref()
                .is_some_and(|waiting| {
                    frame_matches_palette(
                        &self.wallpaper_pending_frames[frame_index],
                        &waiting.palette,
                    )
                })
        {
            if !confirmation.accepted {
                let retries =
                    self.wallpaper_palette_outputs
                        .as_ref()
                        .map_or_else(Vec::new, |waiting| {
                            aborted_palette_retry_outputs(
                                &confirmation.output,
                                self.wallpaper_dirty.contains(&confirmation.output),
                                &self.wallpaper_pending_frames,
                                &waiting.palette,
                            )
                        });
                if let Some(waiting) = self.wallpaper_palette_outputs.take() {
                    self.wallpaper_palette_settled(&waiting.palette, false);
                }
                for output in retries {
                    self.request_wallpaper(&output);
                }
            } else {
                self.try_finish_wallpaper_palette_group();
            }
            self.finish_staged_wallpaper_job(&confirmation.output);
            return;
        }
        if let Some(frame_index) = frame_index {
            let frame = self.wallpaper_pending_frames.remove(frame_index);
            if confirmation.accepted {
                self.complete_wallpaper(frame);
                return;
            } else {
                self.clear_staged_wallpaper_frame(&frame);
                if let Some(provider) = &self.options.wallpaper {
                    provider.discarded(&frame.output, frame.revision);
                }
            }
        }
        self.finish_staged_wallpaper_job(&confirmation.output);
    }

    fn finish_staged_wallpaper_job(&mut self, output: &str) {
        self.wallpaper_jobs.remove(output);
        self.request_dirty_wallpaper(output);
    }

    fn stage_pending_wallpaper(
        &mut self,
        output: &str,
        revision: u64,
        token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
    ) {
        let Some(frame_index) = self.wallpaper_pending_frames.iter().position(|frame| {
            frame.output == output
                && frame.revision == revision
                && std::sync::Arc::ptr_eq(&frame.commit_token, token)
        }) else {
            return;
        };
        let Some(surface_index) = self
            .wallpaper_surfaces
            .iter()
            .position(|surface| surface.name == output)
        else {
            return;
        };
        let (frames, surfaces) = (&self.wallpaper_pending_frames, &mut self.wallpaper_surfaces);
        let frame = &frames[frame_index];
        let surface = &mut surfaces[surface_index];
        if frame.revision == 0
            || surface.transition.is_some()
            || surface
                .palette
                .as_ref()
                .is_some_and(|palette| palette.0 == frame.palette.0)
            || (surface.width, surface.height) != (frame.width, frame.height)
        {
            return;
        }
        let _span = bead_core::profile::Span::new("wallpaper_stage_buffer");
        let staged = if let Ok(index) = prepare_wallpaper_buffer(
            &mut surface.pool,
            &mut surface.buffers,
            frame.width,
            frame.height,
            &frame.rgba,
            None,
        ) {
            surface.staged = Some(StagedWallpaperBuffer {
                revision,
                token: token.clone(),
                width: frame.width,
                height: frame.height,
                index,
            });
            true
        } else {
            false
        };
        if staged {
            self.schedule_wallpaper_release_poll(output.to_owned());
        }
    }

    fn clear_staged_wallpaper_frame(&mut self, frame: &WallpaperFrame) {
        let cleared = if let Some(surface) = self
            .wallpaper_surfaces
            .iter_mut()
            .find(|surface| surface.name == frame.output)
        {
            if surface.staged.as_ref().is_some_and(|staged| {
                staged.matches(
                    frame.revision,
                    &frame.commit_token,
                    frame.width,
                    frame.height,
                )
            }) {
                surface.staged = None;
                true
            } else {
                false
            }
        } else {
            false
        };
        if cleared {
            self.schedule_wallpaper_release_poll(frame.output.clone());
        }
    }

    fn request_dirty_wallpaper(&mut self, output: &str) {
        if self.wallpaper_dirty.remove(output) {
            self.request_wallpaper(output);
        }
    }

    pub(super) fn wallpaper_failed(&mut self, output: &str) {
        self.wallpaper_jobs.remove(output);
        if self.wallpaper_dirty.remove(output) {
            self.request_wallpaper(output);
        }
    }

    pub(super) fn commit_pending_wallpapers(
        &mut self,
        palette: &std::collections::BTreeMap<String, [u8; 4]>,
    ) {
        let frames = std::mem::take(&mut self.wallpaper_pending_frames);
        let mut keep = Vec::new();
        for frame in frames {
            let same = frame
                .palette
                .0
                .iter()
                .map(|(key, value)| (key.clone(), *value))
                .collect::<std::collections::BTreeMap<_, _>>()
                == *palette;
            if !same {
                keep.push(frame);
                continue;
            }
            let current = self
                .options
                .wallpaper
                .as_ref()
                .is_none_or(|provider| provider.is_current(&frame));
            if same && current {
                if frame_is_unconfirmed(&frame, &self.wallpaper_unconfirmed) {
                    keep.push(frame);
                    continue;
                }
                self.install_wallpaper(frame);
            } else {
                self.clear_staged_wallpaper_frame(&frame);
                if let Some(provider) = &self.options.wallpaper {
                    provider.discarded(&frame.output, frame.revision);
                }
            }
        }
        self.wallpaper_pending_frames = keep;
    }

    pub(super) fn discard_pending_wallpapers(&mut self) {
        self.wallpaper_palette_outputs = None;
        for frame in std::mem::take(&mut self.wallpaper_pending_frames) {
            self.clear_staged_wallpaper_frame(&frame);
            if let Some(provider) = &self.options.wallpaper {
                provider.discarded(&frame.output, frame.revision);
            }
        }
    }

    /// Record the committed presentation palette and give an open panel the same
    /// colors; a panel opened later receives it in `start_panel`.
    pub(super) fn set_wallpaper_palette(
        &mut self,
        palette: std::collections::BTreeMap<String, [u8; 4]>,
    ) {
        self.apply_wallpaper_palette(palette, true);
    }

    fn apply_wallpaper_palette(
        &mut self,
        palette: std::collections::BTreeMap<String, [u8; 4]>,
        notify_panel: bool,
    ) {
        if self.wallpaper_palette.as_ref() == Some(&palette) {
            return;
        }
        self.wallpaper_palette = Some(palette.clone());
        if let Some(control) = &self.options.control {
            control.palette_changed(&palette);
        }
        if notify_panel && self.panel_runtime.is_some() {
            self.panel_ui(UiEvent::WallpaperPalette(palette.clone()));
        }
        if self.overlay.is_some() {
            self.overlay_ui(UiEvent::WallpaperPalette(palette));
        }
    }

    pub(super) fn wallpaper_palette_settled(
        &mut self,
        palette: &std::collections::BTreeMap<String, [u8; 4]>,
        accepted: bool,
    ) {
        if let Some(output) = self.active_output.clone()
            && let Some(waiting) = &mut self.wallpaper_palette_outputs
        {
            if waiting.palette != *palette || !waiting.expected.contains(&output) {
                return;
            }
            waiting.settled.insert(output);
            waiting.accepted &= accepted;
            self.try_finish_wallpaper_palette_group();
            return;
        }
        let matching: Vec<_> = self
            .wallpaper_pending_frames
            .iter()
            .filter(|frame| frame_matches_palette(frame, palette))
            .collect();
        if matching.is_empty() {
            if accepted && self.wallpaper_palette.as_ref() != Some(palette) {
                self.restore_wallpaper_palette();
            } else if accepted {
                self.draw_palette_group();
            }
            return;
        }
        if accepted_palette_waits_for_confirmation(
            accepted,
            &self.wallpaper_pending_frames,
            &self.wallpaper_unconfirmed,
            palette,
        ) {
            return;
        }
        let has_current = matching.iter().any(|frame| {
            self.options
                .wallpaper
                .as_ref()
                .is_none_or(|provider| provider.is_current(frame))
        });
        if accepted && has_current {
            bead_core::profile::mark("wallpaper_palette_accepted");
            // The grouped primary flow already committed the current panel epoch.
            // Do not queue a second palette job after that fence has settled.
            self.apply_wallpaper_palette(palette.clone(), self.options.primary_factory.is_none());
            self.commit_pending_wallpapers(palette);
            // The new scenes were prepared while their previous buffers stayed
            // visible. Submit the ready group in this turn, ignoring callbacks
            // belonging to those old buffers.
            self.draw_palette_group();
        } else {
            let mut keep = Vec::new();
            for frame in std::mem::take(&mut self.wallpaper_pending_frames) {
                let same = frame
                    .palette
                    .0
                    .iter()
                    .map(|(key, value)| (key.clone(), *value))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    == *palette;
                if same {
                    self.clear_staged_wallpaper_frame(&frame);
                    if let Some(provider) = &self.options.wallpaper {
                        provider.discarded(&frame.output, frame.revision);
                    }
                } else {
                    keep.push(frame);
                }
            }
            self.wallpaper_pending_frames = keep;
            self.restore_wallpaper_palette();
        }
    }

    fn try_finish_wallpaper_palette_group(&mut self) {
        let Some(waiting) = self.wallpaper_palette_outputs.as_ref() else {
            return;
        };
        if !wallpaper_palette_group_ready(
            waiting,
            &self.wallpaper_pending_frames,
            &self.wallpaper_unconfirmed,
        ) {
            return;
        }
        let waiting = self.wallpaper_palette_outputs.take().unwrap();
        self.wallpaper_palette_settled(&waiting.palette, waiting.accepted);
    }

    fn draw_palette_group(&mut self) {
        let qh = self.qh.clone();
        for surface in &mut self.surfaces {
            surface.callbacks.invalidate();
        }
        for index in 0..self.surfaces.len() {
            self.draw(index, &qh);
        }
    }

    fn prepare_wallpaper_palette_group(
        &mut self,
        palette_key: std::collections::BTreeMap<String, [u8; 4]>,
    ) {
        let mut participants: Vec<_> = self
            .output_sessions
            .iter()
            .map(|(id, session)| (id.clone(), session.epoch))
            .collect();
        let active = self
            .active_output
            .clone()
            .filter(|_| self.runtime.is_some());
        if let Some(id) = &active
            && !participants.iter().any(|(output, _)| output == id)
        {
            participants.push((id.clone(), self.active_output_epoch.unwrap_or_default()));
        }
        if participants.is_empty() && self.panel_runtime.is_none() {
            self.wallpaper_palette_outputs = None;
            return;
        }
        self.wallpaper_palette_outputs = Some(PendingPalette {
            palette: palette_key.clone(),
            expected: participants.iter().map(|(id, _)| id.clone()).collect(),
            settled: Default::default(),
            accepted: true,
            panel_epoch: self.panel_runtime.as_ref().map(|_| self.panel_epoch),
            panel_settled: false,
        });
        // Keep the old buffers visible while bar and companion prepare together.
        if self.panel_runtime.is_some() {
            self.panel_ui(UiEvent::WallpaperPalette(palette_key.clone()));
            if !self
                .panel_runtime
                .as_ref()
                .is_some_and(|source| source.wallpaper_palette_pending())
            {
                self.wallpaper_panel_palette_settled(self.panel_epoch, &palette_key, true);
            }
        }
        for (id, epoch) in participants {
            if active.as_ref() == Some(&id) {
                self.ui(UiEvent::WallpaperPalette(palette_key.clone()));
                if !self
                    .runtime
                    .as_ref()
                    .is_some_and(|source| source.wallpaper_palette_pending())
                {
                    self.wallpaper_palette_settled(&palette_key, true);
                }
            } else {
                self.ui(UiEvent::Output(
                    epoch,
                    id,
                    Box::new(UiEvent::WallpaperPalette(palette_key.clone())),
                ));
            }
        }
    }

    pub(super) fn wallpaper_panel_palette_settled(
        &mut self,
        epoch: u64,
        palette: &std::collections::BTreeMap<String, [u8; 4]>,
        accepted: bool,
    ) {
        let settled = self
            .wallpaper_palette_outputs
            .as_mut()
            .is_some_and(|waiting| {
                waiting.palette == *palette && waiting.settle_panel(epoch, accepted)
            });
        if settled {
            self.try_finish_wallpaper_palette_group();
        }
    }

    pub(super) fn wallpaper_panel_released(&mut self, epoch: u64) {
        let palette = self
            .wallpaper_palette_outputs
            .as_ref()
            .filter(|waiting| waiting.panel_epoch == Some(epoch))
            .map(|waiting| waiting.palette.clone());
        if let Some(palette) = palette {
            self.wallpaper_panel_palette_settled(epoch, &palette, true);
        }
    }

    fn restore_wallpaper_palette(&mut self) {
        let Some(previous) = self.wallpaper_palette.clone() else {
            return;
        };
        if self.options.primary_factory.is_some() {
            // Rollback is also a prepare fence: a failed participant must not
            // expose another participant's staged colors while restoration runs.
            self.prepare_wallpaper_palette_group(previous);
        } else if let Some(runtime) = &mut self.runtime {
            let _ = runtime.wallpaper_palette(bead_render::Palette(std::sync::Arc::new(
                previous.into_iter().collect(),
            )));
        }
    }

    pub(super) fn install_wallpaper(&mut self, frame: WallpaperFrame) {
        let current = self
            .options
            .wallpaper
            .as_ref()
            .is_none_or(|provider| provider.is_current(&frame));
        if !current {
            if let Some(provider) = &self.options.wallpaper {
                provider.discarded(&frame.output, frame.revision);
            }
            self.clear_staged_wallpaper_frame(&frame);
            return;
        }
        let Some(index) = self
            .wallpaper_surfaces
            .iter()
            .position(|surface| surface.name == frame.output)
        else {
            return;
        };
        if (
            self.wallpaper_surfaces[index].width,
            self.wallpaper_surfaces[index].height,
        ) != (frame.width, frame.height)
            || frame.revision == 0
        {
            return;
        }
        if frame.width > MAX_WALLPAPER_DIMENSION || frame.height > MAX_WALLPAPER_DIMENSION {
            return;
        }
        let expected = (frame.width as usize)
            .checked_mul(frame.height as usize)
            .and_then(|n| n.checked_mul(4));
        if expected != Some(frame.rgba.len())
            || expected.is_none_or(|bytes| bytes > MAX_WALLPAPER_RGBA_BYTES)
            || frame.width == 0
            || frame.height == 0
        {
            return;
        }
        let output = self.wallpaper_surfaces[index].output.clone();
        let logical = self.output.info(&output).and_then(|info| info.logical_size);
        let surface = &mut self.wallpaper_surfaces[index];
        let staged_matches = surface.staged.as_ref().is_some_and(|staged| {
            staged.matches(
                frame.revision,
                &frame.commit_token,
                frame.width,
                frame.height,
            )
        });
        if surface.staged.is_some() && !staged_matches {
            surface.staged = None;
        }
        let staged_index = surface.staged.as_ref().map(|staged| staged.index);
        let wallpaper_provider = self.options.wallpaper.clone();
        let upload_token = frame.commit_token.clone();
        let upload_output = frame.output.clone();
        let upload_revision = frame.revision;
        let old_pixels = surface
            .transition
            .take()
            .and_then(|transition| transition.target.is_some().then_some(transition.pixels))
            .or_else(|| surface.pixels.take())
            .or(frame.previous_rgba);
        // A colour-scheme change is one coordinated swap. Fading only the
        // wallpaper would leave the new chrome over the old image for 14 frames.
        let transitioning = self.options.animate
            && old_pixels.is_some()
            && surface
                .palette
                .as_ref()
                .is_some_and(|palette| palette.0 == frame.palette.0);
        if transitioning {
            // A staged final image cannot substitute an intermediate blend.
            surface.staged = None;
            surface.transition = Some(WallpaperTransition {
                commit: Some((frame.revision, frame.commit_token.clone())),
                palette: frame.palette.clone(),
                pixels: old_pixels.unwrap(),
                target: Some(frame.rgba),
                remaining: 14,
                uploaded: false,
            });
            surface.pixels = None;
        } else {
            if attach_wallpaper_buffer(
                WallpaperBufferTarget {
                    pool: &mut surface.pool,
                    layer: &surface.layer,
                    buffers: &mut surface.buffers,
                    viewport: surface.viewport.as_ref(),
                    scale_factor: surface.scale_factor,
                    width: surface.width,
                    height: surface.height,
                    logical,
                    reserved_buffer: staged_index,
                    prepared_buffer: staged_index.filter(|_| staged_matches),
                    feedback: self
                        .presentation
                        .as_ref()
                        .map(|p| (p, &self.qh, frame.revision)),
                },
                &frame.rgba,
                || {
                    if let Some(provider) = &wallpaper_provider {
                        provider.uploaded(&upload_output, upload_revision, &upload_token);
                    }
                },
            )
            .is_err()
            {
                // The accepted pair must survive buffer backpressure. Reuse the
                // transition retry slot without a second raster or a fade.
                surface.transition = Some(WallpaperTransition {
                    commit: Some((frame.revision, frame.commit_token)),
                    palette: frame.palette,
                    pixels: frame.rgba,
                    target: None,
                    remaining: 0,
                    uploaded: false,
                });
                surface.pixels = old_pixels;
                surface.release_wait_ms = 0;
                self.schedule_wallpaper_tick(frame.output);
                return;
            }
            if staged_matches {
                surface.staged = None;
            }
            surface.pixels = self.options.animate.then_some(frame.rgba);
        }
        surface.palette = Some(frame.palette.clone());
        surface.release_wait_ms = 0;
        surface.revision = frame.revision;
        let output = frame.output.clone();
        let revision = frame.revision;
        if transitioning {
            // A retry retains the notification until a target buffer really attaches.
            self.wallpaper_tick(&output);
        } else {
            self.wallpaper_committed(&output, revision, &frame.commit_token, false);
            self.schedule_wallpaper_release_poll(output);
        }
    }

    fn wallpaper_committed(
        &self,
        output: &str,
        revision: u64,
        token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
        transitioning: bool,
    ) {
        if let Some(provider) = &self.options.wallpaper {
            provider.committed(output, revision, token);
        }
        bead_core::profile::mark("wallpaper_surface_committed");
        if let Some(surface) = self
            .wallpaper_surfaces
            .iter()
            .find(|surface| surface.name == output)
        {
            println!(
                "WALLPAPER_SURFACE_COMMITTED output={} revision={} width={} height={} transition={}",
                output, revision, surface.width, surface.height, transitioning
            );
        }
    }

    pub(super) fn wallpaper_tick(&mut self, output: &str) {
        let Some(index) = self
            .wallpaper_surfaces
            .iter()
            .position(|surface| surface.name == output)
        else {
            return;
        };
        let output_handle = self.wallpaper_surfaces[index].output.clone();
        let logical = self
            .output
            .info(&output_handle)
            .and_then(|info| info.logical_size);
        let wallpaper_provider = self.options.wallpaper.clone();
        let (finished, commit) = {
            let surface = &mut self.wallpaper_surfaces[index];
            let Some(transition) = surface.transition.as_mut() else {
                return;
            };
            let prepared_buffer = transition
                .commit
                .as_ref()
                .filter(|_| transition.target.is_none())
                .and_then(|(revision, token)| {
                    surface.staged.as_ref().and_then(|staged| {
                        staged
                            .matches(*revision, token, surface.width, surface.height)
                            .then_some(staged.index)
                    })
                });
            let upload_event = transition
                .commit
                .as_ref()
                .map(|(revision, token)| (*revision, token.clone()));
            let first_upload = !transition.uploaded;
            if let Some(target) = &transition.target {
                let remaining = transition.remaining.max(1) as u16;
                for (current, target) in transition.pixels.iter_mut().zip(target.iter()) {
                    *current =
                        ((*current as u16 * (remaining - 1) + *target as u16) / remaining) as u8;
                }
                transition.remaining = transition.remaining.saturating_sub(1);
                if transition.remaining == 0 {
                    transition.pixels.copy_from_slice(target);
                }
            }
            if attach_wallpaper_buffer(
                WallpaperBufferTarget {
                    pool: &mut surface.pool,
                    layer: &surface.layer,
                    buffers: &mut surface.buffers,
                    viewport: surface.viewport.as_ref(),
                    scale_factor: surface.scale_factor,
                    width: surface.width,
                    height: surface.height,
                    logical,
                    reserved_buffer: surface.staged.as_ref().map(|staged| staged.index),
                    prepared_buffer,
                    feedback: self
                        .presentation
                        .as_ref()
                        .zip(transition.commit.as_ref())
                        .map(|(p, (revision, _))| (p, &self.qh, *revision)),
                },
                &transition.pixels,
                || {
                    if first_upload
                        && let (Some(provider), Some((revision, token))) =
                            (&wallpaper_provider, &upload_event)
                    {
                        provider.uploaded(output, *revision, token);
                    }
                },
            )
            .is_err()
            {
                (false, None)
            } else {
                transition.uploaded = true;
                if prepared_buffer.is_some() {
                    surface.staged = None;
                }
                surface.palette = Some(transition.palette.clone());
                if let Some((revision, _)) = &transition.commit {
                    surface.revision = *revision;
                }
                let commit = transition
                    .commit
                    .take()
                    .map(|(revision, token)| (revision, token, transition.target.is_some()));
                if transition.remaining == 0 {
                    let transition = surface.transition.take().unwrap();
                    surface.pixels = Some(transition.pixels);
                    (true, commit)
                } else {
                    (false, commit)
                }
            }
        };
        if let Some((revision, token, fading)) = commit {
            self.wallpaper_committed(output, revision, &token, fading);
        }
        if !finished {
            self.schedule_wallpaper_tick(output.to_owned());
        }
        self.schedule_wallpaper_release_poll(output.to_owned());
    }

    fn schedule_wallpaper_tick(&self, output: String) {
        let handle = self.loop_handle.clone();
        if handle
            .insert_source(
                Timer::from_duration(Duration::from_millis(15)),
                move |_, _, app| {
                    app.wallpaper_tick(&output);
                    TimeoutAction::Drop
                },
            )
            .is_err()
        {
            eprintln!("WALLPAPER_SURFACE_ERROR code=transition_timer");
        }
    }

    fn schedule_wallpaper_release_poll(&mut self, output: String) {
        let Some(surface) = self
            .wallpaper_surfaces
            .iter_mut()
            .find(|surface| surface.name == output)
        else {
            return;
        };
        if surface.release_timer_pending {
            return;
        }
        surface.release_timer_pending = true;
        let handle = self.loop_handle.clone();
        let callback_output = output.clone();
        if handle
            .insert_source(
                Timer::from_duration(Duration::from_millis(100)),
                move |_, _, app| {
                    app.release_wallpaper_buffers(&callback_output);
                    TimeoutAction::Drop
                },
            )
            .is_err()
        {
            if let Some(surface) = self
                .wallpaper_surfaces
                .iter_mut()
                .find(|surface| surface.name == output)
            {
                surface.release_timer_pending = false;
            }
            eprintln!("WALLPAPER_SURFACE_ERROR code=release_timer");
        }
    }

    fn release_wallpaper_buffers(&mut self, output: &str) {
        let Some(index) = self
            .wallpaper_surfaces
            .iter()
            .position(|surface| surface.name == output)
        else {
            return;
        };
        self.wallpaper_surfaces[index].release_timer_pending = false;
        if self.wallpaper_surfaces[index].staged.is_some() {
            let surface = &mut self.wallpaper_surfaces[index];
            surface.release_wait_ms = surface.release_wait_ms.saturating_add(100);
            if surface.release_wait_ms <= 3000 {
                self.schedule_wallpaper_release_poll(output.to_owned());
            }
            return;
        }
        if self.wallpaper_surfaces[index].buffers.is_empty() {
            return;
        }
        let released = self.wallpaper_surfaces[index]
            .buffers
            .iter_mut()
            .all(|buffer| !buffer.slot().has_active_buffers());
        let replacement_pool = if released {
            match SlotPool::new(4096, &self.shm) {
                Ok(pool) => Some(pool),
                Err(_) => {
                    eprintln!("WALLPAPER_SURFACE_ERROR code=pool_recreate");
                    None
                }
            }
        } else {
            None
        };
        let surface = &mut self.wallpaper_surfaces[index];
        if released {
            surface.buffers.clear();
            surface.staged = None;
            surface.pixels = None;
            if let Some(pool) = replacement_pool {
                surface.pool = pool;
            }
            surface.release_wait_ms = 0;
            surface.release_timer_pending = false;
            if surface.transition.is_none() {
                #[cfg(all(target_os = "linux", target_env = "gnu"))]
                unsafe {
                    libc::malloc_trim(0);
                }
            }
            println!(
                "WALLPAPER_BUFFER_RELEASED output={} revision={}",
                surface.name, surface.revision
            );
        } else {
            surface.release_wait_ms = surface.release_wait_ms.saturating_add(100);
            if surface.release_wait_ms <= 3000 {
                self.schedule_wallpaper_release_poll(output.to_owned());
            } else if surface.release_wait_ms == 3100 {
                let compositor = if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
                    "hyprland"
                } else if std::env::var_os("NIRI_SOCKET").is_some() {
                    "niri"
                } else {
                    "unknown"
                };
                eprintln!(
                    "WALLPAPER_BUFFER_RETAINED output={} compositor={} reason=no_release",
                    surface.name, compositor
                );
            }
        }
    }

    pub(super) fn add_output(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.ensure_wallpaper(&output, qh);
        if self.hidden {
            return;
        }
        let output_id = self
            .output
            .info(&output)
            .and_then(|i| i.name)
            .unwrap_or_else(|| format!("output-{}", output.id().protocol_id()));
        let eligible = self.options.output.as_ref().is_none_or(|name| {
            self.output.info(&output).and_then(|i| i.name).as_ref() == Some(name)
        });
        let per_output = self.options.primary_factory.is_some() && eligible;
        let primary = eligible;
        if per_output
            && let Some(previous) = self
                .surfaces
                .iter()
                .find(|s| s.output == output)
                .map(|s| s.output_name.clone())
            && previous != output_id
            && let Some(mut session) = self.output_sessions.remove(&previous)
        {
            let epoch = session.epoch;
            session.id = output_id.clone();
            self.output_sessions.insert(output_id.clone(), session);
            for surface in self.surfaces.iter_mut().filter(|s| s.output == output) {
                surface.output_name = output_id.clone();
            }
            if self.active_input_output.as_ref() == Some(&previous) {
                self.active_input_output = Some(output_id.clone());
            }
            if let Some(pending) = &mut self.wallpaper_palette_outputs {
                if pending.expected.remove(&previous) {
                    pending.expected.insert(output_id.clone());
                }
                if pending.settled.remove(&previous) {
                    pending.settled.insert(output_id.clone());
                }
            }
            println!("UI_SESSION_NAMED output={output_id} previous={previous} epoch={epoch}");
            self.ui(UiEvent::Output(
                epoch,
                output_id.clone(),
                Box::new(UiEvent::OutputName(output_id.clone())),
            ));
        }
        // An output transaction temporarily owns its session in the active
        // slot. Opening its companion can re-enter here before it is restored.
        if per_output
            && self.active_output.as_ref() != Some(&output_id)
            && !self.output_sessions.contains_key(&output_id)
        {
            let factory = self.options.primary_factory.as_ref().unwrap();
            let epoch = self.next_session_epoch;
            self.next_session_epoch = self.next_session_epoch.wrapping_add(1).max(1);
            match factory(output_id.clone()) {
                Ok(mut source) => {
                    if let Some(sender) = self.event_sender.clone() {
                        let service_sender = sender.clone();
                        let service_id = output_id.clone();
                        source.services(Box::new(move |event| {
                            let _ = service_sender.send(UiEvent::Output(
                                epoch,
                                service_id.clone(),
                                Box::new(event),
                            ));
                        }));
                        let watch_sender = sender;
                        let watch_id = output_id.clone();
                        if let Err(error) = source.watch(Box::new(move |result| {
                            let event = match result {
                                Ok(()) => UiEvent::Reload,
                                Err(error) => UiEvent::WatchError(error),
                            };
                            let _ = watch_sender.send(UiEvent::Output(
                                epoch,
                                watch_id.clone(),
                                Box::new(event),
                            ));
                        })) {
                            eprintln!(
                                "OUTPUT_SESSION_WATCH_ERROR output={output_id} error={error}"
                            );
                        }
                    }
                    source.frame_interval(self.output_frame_interval(&output));
                    self.output_sessions.insert(
                        output_id.clone(),
                        OutputSession {
                            id: output_id.clone(),
                            epoch,
                            source: Some(source),
                            scene: self.scene.clone(),
                            ui_serial: 0,
                            initialized: false,
                            animations: Default::default(),
                        },
                    );
                    println!("UI_SESSION_CREATED output={output_id} epoch={epoch}");
                    let pending_palette =
                        self.wallpaper_palette_outputs.as_mut().and_then(|pending| {
                            pending
                                .expected
                                .insert(output_id.clone())
                                .then(|| pending.palette.clone())
                        });
                    if let Some(palette) = pending_palette {
                        self.ui(UiEvent::Output(
                            epoch,
                            output_id.clone(),
                            Box::new(UiEvent::WallpaperPalette(palette)),
                        ));
                    }
                }
                Err(error) => {
                    eprintln!("OUTPUT_SESSION_ERROR output={output_id} error={error}");
                    return;
                }
            }
        }
        let selected = self.options.panel_spec.is_none()
            || (self.panel_target && self.panel_output.as_ref() == Some(&output));
        if !primary && !(selected && self.companion.is_some()) {
            return;
        }
        // With the bar off its surfaces are never created but still counted, so the
        // next surface on this output is recognised as the panel's.
        let count = self.surfaces.iter().filter(|s| s.output == output).count()
            + if primary && !self.bar_off {
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
                    && (!self.panel_target
                        || self.options.panel_spec.is_none()
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
                if companion
                    && self
                        .options
                        .panel_spec
                        .as_ref()
                        .is_some_and(|spec| spec.overlay)
                {
                    Layer::Overlay
                } else {
                    Layer::Top
                },
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
                        self.bar_thickness()
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
                self.bar_thickness() as i32
            } else if self.options.panel || companion {
                -1
            } else {
                0
            });
            layer.set_keyboard_interactivity(
                if self.options.test_passive || (companion && !self.panel_target) {
                    KeyboardInteractivity::None
                } else if !self.options.test_passive
                    && ((companion
                        && self.pearl.is_none()
                        && self
                            .panel_runtime
                            .as_ref()
                            .is_none_or(|source| source.outside_dismiss())
                        && self
                            .options
                            .panel_spec
                            .as_ref()
                            .is_some_and(|s| s.keyboard_focus))
                        || (self.options.test_keyboard && !companion))
                {
                    KeyboardInteractivity::Exclusive
                } else if (self.options.interactive && self.options.keyboard_focus && !companion)
                    || (companion
                        && (self.pearl.is_some()
                            || self
                                .panel_runtime
                                .as_ref()
                                .is_some_and(|source| !source.outside_dismiss()))
                        && self
                            .options
                            .panel_spec
                            .as_ref()
                            .is_some_and(|s| s.keyboard_focus))
                {
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
                pointer_visual: None,
                decorations: None,
                background: None,
                background_size: None,
                logical_size: (0, 0),
                background_visible: false,
                companion,
                output: output.clone(),
                output_name: output_id.clone(),
                layer,
                panel_position: std::cell::Cell::new(None),
                native: None,
                prepared_native: None,
                handoff: None,
                pool: SlotPool::new(4096, &self.shm)?,
                #[cfg(feature = "wgpu")]
                gpu: None,
                #[cfg(feature = "native-gl")]
                gl: None,
                #[cfg(feature = "native-gl")]
                gl_prepared: false,
                #[cfg(feature = "native-gl")]
                gl_prepare_pending: false,
                buffers: Vec::new(),
                width: 0,
                height: 32,
                native_configure_size: None,
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
                input_content_offset: (0., 0.),
                format: PixelFormat::Bgra,
                previous: None,
                visual_damage: Default::default(),
                allocations: 0,
                shm_trim_pending: false,
                heap_trimmed_ui_serial: u64::MAX,
                heap_trim_generation: None,
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
                last_submitted_ui_serial: if companion {
                    self.panel_ui_serial
                } else {
                    self.primary_ui_serial
                },
                scale_relayout_pending: false,
                resources_retry_pending: false,
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

struct WallpaperBufferTarget<'a> {
    pool: &'a mut SlotPool,
    layer: &'a LayerSurface,
    buffers: &'a mut Vec<smithay_client_toolkit::shm::slot::Buffer>,
    viewport: Option<&'a WpViewport>,
    scale_factor: i32,
    width: u32,
    height: u32,
    logical: Option<(i32, i32)>,
    reserved_buffer: Option<usize>,
    prepared_buffer: Option<usize>,
    feedback: Option<(&'a WpPresentation, &'a QueueHandle<App>, u64)>,
}

fn prepare_wallpaper_buffer(
    pool: &mut SlotPool,
    buffers: &mut Vec<smithay_client_toolkit::shm::slot::Buffer>,
    width: u32,
    height: u32,
    rgba: &[u8],
    reserved: Option<usize>,
) -> Result<usize, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("wallpaper frame too large")?;
    if width == 0
        || height == 0
        || width > MAX_WALLPAPER_DIMENSION
        || height > MAX_WALLPAPER_DIMENSION
        || expected > MAX_WALLPAPER_RGBA_BYTES
        || rgba.len() != expected
    {
        return Err("wallpaper frame size mismatch".into());
    }
    let stride = i32::try_from(width.checked_mul(4).ok_or("wallpaper stride overflow")?)
        .map_err(|_| "wallpaper stride overflow")?;
    let buffer_width = i32::try_from(width).map_err(|_| "wallpaper width overflow")?;
    let buffer_height = i32::try_from(height).map_err(|_| "wallpaper height overflow")?;
    for (index, buffer) in buffers.iter_mut().enumerate() {
        if Some(index) != reserved
            && let Some(bytes) = buffer.canvas(pool)
        {
            write_bgra(bytes, rgba);
            return Ok(index);
        }
    }
    if buffers.len() < 2 {
        let (buffer, bytes) = pool
            .create_buffer(
                buffer_width,
                buffer_height,
                stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|_| "wallpaper SHM allocation failed")?;
        write_bgra(bytes, rgba);
        buffers.push(buffer);
        return Ok(buffers.len() - 1);
    }
    Err("wallpaper buffers busy".into())
}

fn attach_wallpaper_buffer(
    target: WallpaperBufferTarget<'_>,
    rgba: &[u8],
    on_upload: impl FnOnce(),
) -> Result<(), String> {
    let WallpaperBufferTarget {
        pool,
        layer,
        buffers,
        viewport,
        scale_factor,
        width,
        height,
        logical,
        reserved_buffer,
        prepared_buffer,
        feedback,
    } = target;
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("wallpaper frame too large")?;
    if rgba.len() != expected {
        return Err("wallpaper frame size mismatch".into());
    }
    let selected = if let Some(index) = prepared_buffer {
        if Some(index) != reserved_buffer || index >= buffers.len() {
            return Err("staged wallpaper buffer unavailable".into());
        }
        index
    } else {
        prepare_wallpaper_buffer(pool, buffers, width, height, rgba, reserved_buffer)?
    };
    let buffer = buffers
        .get(selected)
        .ok_or("wallpaper buffer unavailable")?;
    if let Some(viewport) = viewport {
        if let Some((width, height)) = logical {
            viewport.set_destination(width.max(1), height.max(1));
        }
        layer.wl_surface().set_buffer_scale(1);
    } else {
        layer.wl_surface().set_buffer_scale(scale_factor.max(1));
    }
    buffer
        .attach_to(layer.wl_surface())
        .map_err(|_| "wallpaper buffer still active")?;
    on_upload();
    if let Some((presentation, qh, revision)) = feedback {
        presentation.feedback(
            layer.wl_surface(),
            qh,
            PresentationData {
                surface: layer.wl_surface().clone(),
                generation: revision,
                source_epoch: 0,
                visibility: 0,
                update: revision,
            },
        );
    }
    layer.wl_surface().damage_buffer(
        0,
        0,
        i32::try_from(width).map_err(|_| "wallpaper width overflow")?,
        i32::try_from(height).map_err(|_| "wallpaper height overflow")?,
    );
    layer.commit();
    Ok(())
}

fn write_bgra(bytes: &mut [u8], rgba: &[u8]) {
    let (pixels, pixel_remainder) = bytes.as_chunks_mut::<4>();
    let (colors, color_remainder) = rgba.as_chunks::<4>();
    debug_assert!(pixel_remainder.is_empty() && color_remainder.is_empty());
    for (pixel, color) in pixels.iter_mut().zip(colors) {
        *pixel = [color[2], color[1], color[0], color[3]];
    }
}

#[cfg(test)]
mod palette_barrier_tests {
    use super::{
        PendingPalette, StagedWallpaperBuffer, aborted_palette_retry_outputs,
        accepted_palette_waits_for_confirmation, frame_is_unconfirmed,
        frame_interval_from_refresh_millihz, has_unconfirmed_palette_frame,
        wallpaper_palette_group_ready,
    };
    use std::sync::Arc;

    #[test]
    fn output_refresh_millihertz_maps_to_deadline_period() {
        assert_eq!(
            frame_interval_from_refresh_millihz(120_000),
            std::time::Duration::from_nanos(8_333_333)
        );
        assert_eq!(
            frame_interval_from_refresh_millihz(60_000),
            std::time::Duration::from_nanos(16_666_666)
        );
        assert_eq!(
            frame_interval_from_refresh_millihz(0),
            std::time::Duration::from_millis(16)
        );
    }

    #[test]
    fn staged_wallpaper_requires_exact_frame_token_revision_and_dimensions() {
        let token: Arc<dyn std::any::Any + Send + Sync> = Arc::new("boot-pair");
        let staged = StagedWallpaperBuffer {
            revision: 9,
            token: token.clone(),
            width: 2880,
            height: 1800,
            index: 1,
        };
        let other: Arc<dyn std::any::Any + Send + Sync> = Arc::new("boot-pair");
        assert!(staged.matches(9, &token, 2880, 1800));
        assert!(!staged.matches(9, &other, 2880, 1800));
        assert!(!staged.matches(10, &token, 2880, 1800));
        assert!(!staged.matches(9, &token, 1920, 1200));
    }

    #[test]
    fn palette_group_waits_for_both_output_and_companion_in_either_order() {
        for panel_first in [false, true] {
            let mut group = PendingPalette {
                palette: Default::default(),
                expected: ["output".to_owned()].into_iter().collect(),
                settled: Default::default(),
                accepted: true,
                panel_epoch: Some(7),
                panel_settled: false,
            };
            assert!(!group.ready());
            if panel_first {
                group.panel_settled = true;
            } else {
                group.settled.insert("output".to_owned());
            }
            assert!(!group.ready());
            group.panel_settled = true;
            group.settled.insert("output".to_owned());
            assert!(group.ready());
        }
    }

    #[test]
    fn panel_opened_or_replaced_during_palette_prepare_joins_current_group() {
        let mut group = PendingPalette {
            palette: Default::default(),
            expected: ["output".to_owned()].into_iter().collect(),
            settled: ["output".to_owned()].into_iter().collect(),
            accepted: true,
            panel_epoch: None,
            panel_settled: false,
        };
        assert!(group.ready());
        group.join_panel(7);
        assert!(!group.ready());
        group.join_panel(8);
        assert!(!group.settle_panel(7, true));
        assert!(!group.ready());
        assert!(group.settle_panel(8, true));
        assert!(group.ready());
    }

    #[test]
    fn palette_group_without_companion_only_waits_for_its_outputs() {
        let mut group = PendingPalette {
            palette: Default::default(),
            expected: ["output".to_owned()].into_iter().collect(),
            settled: Default::default(),
            accepted: true,
            panel_epoch: None,
            panel_settled: false,
        };
        assert!(!group.ready());
        group.settled.insert("output".to_owned());
        assert!(group.ready());
    }

    #[test]
    fn wallpaper_palette_waits_for_only_the_exact_staged_frame_confirmation() {
        let token: Arc<dyn std::any::Any + Send + Sync> = Arc::new("frame-a");
        let palette = std::collections::BTreeMap::from([("background".into(), [1, 2, 3, 255])]);
        let frame = super::WallpaperFrame {
            output: "DP-1".into(),
            revision: 9,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
            previous_rgba: None,
            palette: bead_render::Palette(Arc::new(palette.clone().into_iter().collect())),
            commit_token: token.clone(),
        };
        let pending = crate::WallpaperFrameConfirmation {
            output: "DP-1".into(),
            revision: 9,
            commit_token: token.clone(),
            accepted: false,
        };
        assert!(has_unconfirmed_palette_frame(
            std::slice::from_ref(&frame),
            std::slice::from_ref(&pending),
            &palette,
        ));
        assert!(frame_is_unconfirmed(&frame, std::slice::from_ref(&pending)));
        assert!(accepted_palette_waits_for_confirmation(
            true,
            std::slice::from_ref(&frame),
            std::slice::from_ref(&pending),
            &palette,
        ));
        assert!(!accepted_palette_waits_for_confirmation(
            false,
            std::slice::from_ref(&frame),
            std::slice::from_ref(&pending),
            &palette,
        ));
        let stale_token: Arc<dyn std::any::Any + Send + Sync> = Arc::new("frame-a");
        let stale = crate::WallpaperFrameConfirmation {
            commit_token: stale_token,
            ..pending.clone()
        };
        assert!(!has_unconfirmed_palette_frame(
            std::slice::from_ref(&frame),
            std::slice::from_ref(&stale),
            &palette,
        ));
        assert!(!frame_is_unconfirmed(&frame, std::slice::from_ref(&stale)));
        assert!(!has_unconfirmed_palette_frame(
            std::slice::from_ref(&frame),
            std::slice::from_ref(&stale),
            &Default::default(),
        ));
        for workers_first in [false, true] {
            let mut group = PendingPalette {
                palette: palette.clone(),
                expected: ["DP-1".to_owned()].into_iter().collect(),
                settled: Default::default(),
                accepted: true,
                panel_epoch: Some(4),
                panel_settled: false,
            };
            let mut pending_frame = Some(pending.clone());
            if workers_first {
                group.settled.insert("DP-1".into());
                group.panel_settled = true;
                assert!(!wallpaper_palette_group_ready(
                    &group,
                    std::slice::from_ref(&frame),
                    std::slice::from_ref(pending_frame.as_ref().unwrap()),
                ));
                pending_frame = None;
                assert!(wallpaper_palette_group_ready(
                    &group,
                    std::slice::from_ref(&frame),
                    pending_frame.as_slice(),
                ));
            } else {
                group.panel_settled = true;
                pending_frame = None;
                assert!(!wallpaper_palette_group_ready(
                    &group,
                    std::slice::from_ref(&frame),
                    pending_frame.as_slice(),
                ));
                group.settled.insert("DP-1".into());
                assert!(wallpaper_palette_group_ready(
                    &group,
                    std::slice::from_ref(&frame),
                    pending_frame.as_slice(),
                ));
            }
        }
    }

    #[test]
    fn aborted_group_retries_unique_matching_outputs_only_for_a_newer_request() {
        let palette = std::collections::BTreeMap::from([("background".into(), [1, 2, 3, 255])]);
        let other_palette =
            std::collections::BTreeMap::from([("background".into(), [7, 8, 9, 255])]);
        let make_frame = |output: &str, color: [u8; 4], revision: u64| super::WallpaperFrame {
            output: output.into(),
            revision,
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
            previous_rgba: None,
            palette: bead_render::Palette(Arc::new(std::collections::HashMap::from([(
                "background".into(),
                color,
            )]))),
            commit_token: Arc::new(revision),
        };
        let frames = [
            make_frame("DP-1", [1, 2, 3, 255], 1),
            make_frame("DP-2", [1, 2, 3, 255], 2),
            make_frame("DP-2", [1, 2, 3, 255], 3),
            make_frame("DP-3", [4, 5, 6, 255], 4),
        ];
        assert_eq!(
            aborted_palette_retry_outputs("DP-1", true, &frames, &palette),
            vec!["DP-2".to_owned()]
        );
        assert!(aborted_palette_retry_outputs("DP-1", false, &frames, &palette).is_empty());
        assert!(aborted_palette_retry_outputs("DP-1", true, &frames, &other_palette).is_empty());
    }
}

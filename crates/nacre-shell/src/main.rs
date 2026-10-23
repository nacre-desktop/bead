// SPDX-License-Identifier: GPL-3.0-only
mod backends;
mod bar_routes;
mod calendar;
mod chat;
mod commands;
mod contour;
mod dashboard;
mod desktop;
mod font_sources;
mod headless;
mod i18n;
mod i18n_source;
mod key_routes;
mod launcher;
mod launcher_panel;
mod memory_policy;
mod nacre_source;
mod notice_service;
mod notifications;
mod placement;
mod player_art;
mod popouts;
mod prefs;
mod reload;
mod session_menu;
mod settings;
#[cfg(test)]
mod settings_tests;
mod shell_control;
mod sound;
mod state;
mod status_views;
mod tray;
mod ui_provider;
mod wallpaper;
#[cfg(test)]
mod wallpaper_decode;
mod wallpaper_motion;
mod workspace_rows;
use bead::render::{Palette, Primitive, Rect, Scene, Slide};
use bead::wayland::{Options, run};
use std::path::{Path, PathBuf};
fn main() {
    memory_policy::configure();
    std::panic::set_hook(Box::new(|_| {
        eprintln!("PANIC code=ui_panic");
    }));
    if std::env::args().any(|a| a == "--panic-probe")
        && std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() == Ok("1")
    {
        eprintln!("PANIC thread=main private panic hook proof");
        panic!("private panic hook proof");
    }
    let mut health = None;
    let mut ui_started = false;
    if let Err(e) = start(&mut health, &mut ui_started) {
        if ui_started {
            eprintln!("nacre-shell: fatal code=runtime_error");
        } else {
            eprintln!("nacre-shell: {e}");
        }
        std::process::exit(1);
    }
    if let Some(health) = &mut health
        && health.finish().is_err()
    {
        eprintln!("RECOVERY_ERROR code=clean_shutdown");
    }
}
fn start(
    health: &mut Option<bead::core::recovery::Run>,
    ui_started: &mut bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cli: Vec<String> = std::env::args().skip(1).collect();
    if cli.first().is_some_and(|a| a == "--check-daemon") {
        return nacre_source::check(
            cli.get(1)
                .ok_or("--check-daemon needs an explicit private socket")?
                .into(),
        );
    }
    if cli.first().is_some_and(|a| a == "--check") {
        return headless::check(Path::new(cli.get(1).ok_or("--check needs FILE")?));
    }
    if cli.first().is_some_and(|a| a == "--generate-types") {
        std::fs::write(
            cli.get(1).ok_or("--generate-types needs FILE")?,
            bead::ui::schema::luau(),
        )?;
        return Ok(());
    }

    if cli.first().is_some_and(|a| a == "helper") {
        return commands::helper_client(&cli[1..]);
    }
    if cli.first().is_some_and(|a| a == "ipc") {
        return commands::client(&cli[1..]);
    }
    if cli.first().is_some_and(|a| a == "command-catalog") {
        println!("{}", include_str!("../../../shell/command-catalog.json"));
        return Ok(());
    }

    if std::env::args().any(|arg| arg == "--services-ui-probe") {
        return reload::private_service_proof();
    }
    if std::env::args().any(|arg| arg == "--backend-probe") {
        return backends::probe::run();
    }
    let started = std::time::Instant::now();
    let mut profile = false;
    let mut bar_panel = false;
    let mut audio_panel = false;
    let mut launcher_panel = false;
    let mut core_views = false;
    let mut notification_bus = None;
    let mut notification_history = None;
    let mut notice_daemon: Option<std::path::PathBuf> = None;
    let mut notice_provider = false;
    let mut notice_action_on_click = false;
    let mut panel_edge = bead::core::Edge::Right;
    let default_panel_renderer = if cfg!(feature = "native-gl") {
        bead::render::RendererKind::FemtovgGl
    } else {
        bead::render::RendererKind::Cpu
    };
    let mut panel_renderer = None;
    let mut options = Options {
        command_map: Some(std::sync::Arc::new(commands::map)),
        ..Options::default()
    };
    select_renderer(&mut options, "auto")?;
    let mut ui: Option<std::path::PathBuf> = None;
    let mut menu_fixture: Option<PathBuf> = None;
    let mut dashboard_fixtures: Option<PathBuf> = None;
    let mut daemon_socket: Option<PathBuf> = None;
    let mut register_ui_provider = false;
    let mut ui_notification_socket = None;
    let mut backend_enabled = false;
    if let Ok(name) = std::env::var("BEAD_RENDERER") {
        select_renderer(&mut options, &name)?;
    }
    let mut palette = PathBuf::from("shell/palette.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bar-panel" => bar_panel = true,
            "--notification-bus" | "--notification-daemon" => {
                let path = std::path::PathBuf::from(
                    args.next()
                        .ok_or("--notification-bus/--notification-daemon needs a socket path")?,
                );
                if arg == "--notification-bus" {
                    notification_bus = Some(path);
                } else {
                    notice_daemon = Some(path);
                }
                ui = Some("notifications/main.luau".into());
                options.panel = true;
                options.content_size = true;
                options.interactive = true;
                options.keyboard_focus = false;
                options.edge = bead::core::Edge::Right;
                options.renderer = bead::render::RendererKind::Cpu;
                options.auto_fallback = false;
            }
            "--notification-provider" => notice_provider = true,
            "--notification-action-on-click" => notice_action_on_click = true,
            "--notification-history" => {
                notification_history = Some(std::path::PathBuf::from(
                    args.next().ok_or("--notification-history needs path")?,
                ))
            }
            "--audio-panel" => audio_panel = true,
            "--launcher-panel" => launcher_panel = true,
            "--core-views" => core_views = true,
            "--report" => {
                options.report = Some(args.next().ok_or("--report needs FILE")?.into());
            }
            "--command-socket" => {
                options.command_socket = Some(
                    args.next()
                        .ok_or("--command-socket needs private path")?
                        .into(),
                )
            }
            "--edge" => {
                options.edge = serde_json::from_value(serde_json::Value::String(
                    args.next().ok_or("--edge needs top/bottom/left/right")?,
                ))?
            }
            "--panel-edge" => {
                panel_edge = serde_json::from_value(serde_json::Value::String(
                    args.next()
                        .ok_or("--panel-edge needs top/bottom/left/right")?,
                ))?
            }
            "--lazy-gl" => options.lazy_gl = true,
            "--debug-policy-cycle" => {
                if std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() != Ok("1") {
                    return Err("policy cycle requires private proof environment".into());
                }
                options.debug_policy_cycle = true;
            }
            "--panel-renderer" => {
                let mut panel_options = Options::default();
                select_renderer(
                    &mut panel_options,
                    &args.next().ok_or("--panel-renderer needs value")?,
                )?;
                panel_renderer = Some(panel_options.renderer);
            }
            "--bar" => {
                options.bar = true;
                options.keyboard_focus = false;
                options.interactive = true;
                ui = Some("bar/main.luau".into());
                backend_enabled = true;
            }
            "--profile-scene" => {
                profile = true;
                options.panel = true;
            }
            "--menu-fixture" => {
                menu_fixture = Some(args.next().ok_or("--menu-fixture needs path")?.into())
            }
            "--dashboard-fixtures" => {
                dashboard_fixtures = Some(
                    args.next()
                        .ok_or("--dashboard-fixtures needs directory")?
                        .into(),
                )
            }
            "--ui-provider" => register_ui_provider = true,
            "--ui-notification-socket" => {
                ui_notification_socket = Some(PathBuf::from(
                    args.next().ok_or("--ui-notification-socket needs path")?,
                ))
            }
            "--daemon-socket" => {
                daemon_socket = Some(args.next().ok_or("--daemon-socket needs path")?.into())
            }
            "--ui" => {
                ui = Some(args.next().ok_or("--ui needs bundle-relative path")?.into());
                options.panel = true;
                options.content_size = true;
                options.interactive = true;
            }
            "--backend" => {
                let value = args.next().ok_or("--backend needs auto or none")?;
                backend_enabled = match value.as_str() {
                    "auto" => true,
                    "none" => false,
                    _ => return Err("backend must be auto/none".into()),
                };
            }
            "--no-keyboard-focus" => options.keyboard_focus = false,
            "--test-keyboard" => options.test_keyboard = true,
            "--test-passive" => options.test_passive = true,
            "--trace-state" => options.trace_state = true,
            "--memory-usage" => options.memory_usage = true,
            "--surfaces" => {
                options.surface_count = args.next().ok_or("--surfaces needs value")?.parse()?;
                if !(1..=3).contains(&options.surface_count) {
                    return Err("surfaces must be 1..3".into());
                }
            }
            "--renderer" => {
                select_renderer(&mut options, &args.next().ok_or("--renderer needs value")?)?
            }
            "--debug-continuous" => options.debug_continuous = true,
            "--debug-progress" => {
                options.debug_progress = true;
                options.debug_continuous = true;
                options.panel = true;
                options.content_size = true;
            }
            "--debug-luau-stall-ms" => {
                let ms: u64 = args
                    .next()
                    .ok_or("--debug-luau-stall-ms needs milliseconds")?
                    .parse()?;
                if ms > 5000 {
                    return Err("debug script stall must be at most 5000ms".into());
                }
                options.debug_luau_stall_ms = Some(ms);
            }
            "--start-open" => options.start_open = true,
            "--content-size" => options.content_size = true,
            "--output" => options.output = Some(args.next().ok_or("--output needs value")?),
            "--namespace" => options.namespace = args.next().ok_or("--namespace needs value")?,
            "--panel" => options.panel = true,
            "--animate" => {
                options.animate = true;
                options.panel = true;
            }
            "--background-blur" => options.background_blur = true,
            "--no-damage" => options.damage = false,
            "--argb" => options.force_argb = true,
            "--seconds" => {
                let s: f64 = args.next().ok_or("--seconds needs value")?.parse()?;
                if !s.is_finite() || s <= 0. {
                    return Err("seconds must be positive".into());
                }
                options.seconds = Some(s);
            }
            "--palette" => palette = args.next().ok_or("--palette needs path")?.into(),
            "--help" | "-h" => {
                println!(
                    "nacre-shell [--bar [--audio-panel|--launcher-panel|--core-views] [--panel-renderer cpu|femtovg-gl]] [--menu-fixture FILE|--daemon-socket SOCKET] [--dashboard-fixtures DIR] [--ui-provider --command-socket SOCKET [--ui-notification-socket SOCKET]] [--ui ui/main.json|--panel|--animate] [--renderer cpu|femtovg-gl|femtovg|vello|auto] [--test-passive] [--memory-usage] [--surfaces 1..3] [--content-size] [--start-open] [--output NAME] [--background-blur] [--no-damage] [--argb] [--seconds N] [--palette PATH] [--namespace NAME]\nFeature 3 stateful UI preview. Auto keeps bars on CPU and warms panel GL lazily; other views use native GL with CPU fallback. UI accepts focus on demand; stress panels are click-through. Stop with Ctrl+C or SIGTERM."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if dashboard_fixtures.is_some() && !core_views {
        return Err("--dashboard-fixtures requires --core-views".into());
    }
    if core_views {
        launcher_panel = true;
    }
    if launcher_panel {
        options.command_map = Some(std::sync::Arc::new(if core_views {
            commands::map_views
        } else {
            commands::map_launcher
        }));
    }
    let launcher_memory = launcher_panel.then(launcher::Memory::default);
    if options.report.is_some() && options.seconds.is_none() {
        options.seconds = Some(30.);
    }
    if options.bar {
        options.panel = false;
        options.content_size = false;
    }
    if options.test_passive {
        options.interactive = false;
        options.keyboard_focus = false;
        options.test_keyboard = false;
    }
    if options.bar && options.auto_fallback {
        options.renderer = bead::render::RendererKind::Cpu;
        options.lazy_gl = true;
    }
    let colors = Palette::read(&palette).or_else(|_| {
        eprintln!("RECOVERY code=palette_fallback");
        Palette::parse(include_bytes!("../../../shell/palette.json"))
    })?;
    if bar_panel {
        if !options.bar {
            return Err("--bar-panel requires --bar".into());
        }
        let mut companion = panel(&colors, Path::new("shell"))?;
        companion.font_files = font_sources::select(&companion)?;
        companion.offset = (32., 32.);
        companion.slide = None;
        companion.motion = Some(bead::render::Motion {
            id: "companion".into(),
            open: true,
            duration: 0.2,
            spring: false,
            curve: None,
            edge: panel_edge,
            slide_distance: None,
        });
        options.companion = Some((companion, panel_renderer.unwrap_or(default_panel_renderer)));
    }
    let mut scene = if ui.is_some() {
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 1100.,
            slide: Some(Slide {
                closed_y: 32.,
                open_y: 32.,
                period_seconds: 2.,
            }),
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., 1164., 654.),
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(0., 0., 1100., 590.),
                radius: 12.,
                fill: colors.get("surface")?,
                border: None,
            }],
        }
    } else if options.panel {
        panel(&colors, Path::new("shell"))?
    } else {
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: Vec::new(),
            width: 20000.,
            slide: None,
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(0., 0., 20000., 32.),
                radius: 0.,
                fill: [32, 32, 32, 255],
                border: None,
            }],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 20000., 32.),
        }
    };
    if options.debug_progress {
        scene.primitives.push(Primitive::Rounded {
            rect: Rect::new(64., 540., 300., 12.),
            radius: 6.,
            fill: colors.get("primary")?,
            border: None,
        });
    }
    if ui.is_none() {
        scene.font_files = font_sources::select(&scene)?;
    }
    println!(
        "PROFILE stage=cli_scene ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.
    );
    if profile {
        use bead::render::{CpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.5)?;
        let width = (scene.bounds.w * 1.5).ceil() as u32;
        let height = (scene.bounds.h * 1.5).ceil() as u32;
        let mut bytes = vec![0; width as usize * height as usize * 4];
        let t = std::time::Instant::now();
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut bytes,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )?;
        println!(
            "PROFILE stage=raster ms={:.3} total_ms={:.3}",
            t.elapsed().as_secs_f64() * 1000.,
            started.elapsed().as_secs_f64() * 1000.
        );
        return Ok(());
    }
    if notification_bus.is_some() && notice_daemon.is_some() {
        return Err("--notification-bus and --notification-daemon are mutually exclusive".into());
    }
    if (notice_provider || notice_action_on_click) && notice_daemon.is_none() {
        return Err("--notification-provider and --notification-action-on-click require --notification-daemon".into());
    }
    let notification_mode = notification_bus.is_some() || notice_daemon.is_some();
    if notification_mode && (audio_panel || options.bar) {
        return Err("notification service mode uses independent private notice/OSD surfaces; combine no bar/audio mode flags".into());
    }
    if launcher_panel && (!options.bar || audio_panel || notification_mode) {
        return Err("--launcher-panel requires a bar and no other panel family".into());
    }
    if audio_panel && !options.bar {
        return Err("--audio-panel requires --bar".into());
    }
    *ui_started = true;
    let mut forced_safe_mode = false;
    if let Ok(path) = state::File::path()
        && let Some(parent) = path.parent()
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .min(u64::MAX as u128) as u64;
        match bead::core::recovery::Run::begin(parent.join("ui-health.json"), now) {
            Ok(run_health) => *health = Some(run_health),
            Err(_) => {
                forced_safe_mode = true;
                eprintln!("RECOVERY_ERROR code=crash_history");
            }
        }
    }
    if forced_safe_mode || health.as_ref().is_some_and(|h| h.safe_mode) {
        eprintln!("RECOVERY code=safe_mode");
        options.bar = true;
        options.panel = false;
        options.interactive = false;
        options.keyboard_focus = false;
        options.panel_spec = None;
        options.companion = None;
        options.command_socket = None;
        options.command_map = None;
        options.renderer = bead::render::RendererKind::Cpu;
        options.lazy_gl = false;
        options.background_blur = false;
        let mut scene = bead::ui::recovery::safe_bar(&colors);
        scene.font_files = font_sources::select(&scene)?;
        return run(options, scene);
    }
    let desktop_owner = std::sync::Arc::new(std::sync::OnceLock::new());
    let tray_selection = tray::Selection::default();
    let panel_trace = options.trace_state;
    if core_views && options.bar {
        options.decorations = Some(
            placement::Chrome::new(
                placement::Attachment::Attached(bead::core::Edge::Top),
                &colors,
            )?
            .decorations(),
        );
    }
    if legacy_sound_panel_enabled(audio_panel, core_views) {
        let owner = desktop_owner.clone();
        let panel_palette = palette.clone();
        let panel_colors = colors.clone();
        let fallback = sound::fallback(colors.get("surface")?);
        let cold = fallback.clone();
        options.panel_spec = Some(bead::wayland::PanelSpec {
            position: Some(
                placement::Frame::default()
                    .positioner(placement::Attachment::Attached(bead::core::Edge::Right)),
            ),
            engages_decorations: true,
            passive: false,
            keyboard_focus: true,
            initial_open: state::File::path()
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })
                .and_then(state::File::open)
                .is_ok_and(|(_, saved)| {
                    saved.get("panel.open") == Some(&bead::core::Value::Bool(true))
                }),
            fallback,
            // This modest transient view does not amortize the native driver residency.
            renderer: panel_renderer.unwrap_or(bead::render::RendererKind::Cpu),
            factory: std::sync::Arc::new(move |open| {
                let session = bead::ui::Session::new(
                    "shell".into(),
                    "sound/main.luau".into(),
                    panel_palette.clone(),
                    panel_colors.clone(),
                    cold.clone(),
                );
                let desktop = owner.get().cloned().ok_or("bar services not ready")?;
                reload::Runtime::panel_session(session, open)
                    .with_trace(panel_trace)
                    .with_frame(
                        placement::Attachment::Attached(bead::core::Edge::Right),
                        &panel_colors,
                    )
                    .and_then(|runtime| runtime.with_shared_desktop(desktop, false))
                    .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                    .map_err(|e| e.to_string())
            }),
        });
    }
    if register_ui_provider && (!launcher_panel || menu_fixture.is_some()) {
        return Err("--ui-provider requires the bar launcher with its daemon Source".into());
    }
    if ui_notification_socket.is_some() && !register_ui_provider {
        return Err("--ui-notification-socket requires --ui-provider".into());
    }
    let osd_socket = ui_notification_socket.clone();
    let _ui_provider = if register_ui_provider {
        Some(
            ui_provider::start(
                daemon_socket
                    .clone()
                    .ok_or("--ui-provider requires --daemon-socket")?,
                options
                    .command_socket
                    .clone()
                    .ok_or("--ui-provider requires --command-socket")?,
                ui_notification_socket.clone(),
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    // N0035 catalog for the `bead` namespace; without the topic the built-in English stays.
    i18n::init();
    let _i18n_service = _ui_provider.as_ref().and_then(|client| {
        let topic = client.topic("i18n").ok()?;
        let sink: i18n_source::Sink = std::sync::Arc::new(bead::ui::i18n::install);
        i18n_source::Service::start(std::sync::Arc::new(topic), sink).ok()
    });
    // B0035: the owner's Nacre settings, read-only and live. Without the topics (no
    // daemon, older daemon) every default is Nacre's, so nothing changes.
    let gate = shell_control::Gate::new();
    let _prefs = _ui_provider.as_ref().and_then(|client| {
        let topic = |name| {
            client
                .topic(name)
                .ok()
                .map(|t| std::sync::Arc::new(t) as std::sync::Arc<dyn bead::core::source::Source>)
        };
        prefs::Service::start_with(
            topic(prefs::DESKTOP)?,
            topic(prefs::DASHBOARD)?,
            topic(prefs::PROFILE),
            topic(prefs::LAUNCHER_LAYOUT),
            {
                let gate = gate.clone();
                std::sync::Arc::new(move |p: &prefs::Prefs| {
                    bead::core::animation::set_reduced_motion(p.reduce_motion);
                    bead::ui::text_style::set(p.text_style());
                    placement::set_sheen(p.frame_sheen);
                    gate.apply(p);
                })
            },
        )
        .ok()
    });
    if _prefs.is_some() && options.bar {
        options.control = Some(gate);
    }
    let core_sound = if core_views {
        Some(popouts::core_sound(
            _ui_provider.clone(),
            palette.clone(),
            colors.clone(),
            panel_trace,
        )?)
    } else {
        None
    };
    let tray_preview = _ui_provider.is_none();
    let connection_status = _ui_provider.as_ref().map(|client| {
        std::sync::Arc::new(client.connection()) as std::sync::Arc<dyn bead::core::source::Source>
    });
    let shared_menu_source = _ui_provider
        .as_ref()
        .map(|client| {
            client.topic("menu").map(|topic| {
                std::sync::Arc::new(topic) as std::sync::Arc<dyn bead::core::source::Source>
            })
        })
        .transpose()
        .map_err(|e| e.to_string())?;
    if launcher_panel {
        options.panel_spec = Some(launcher_panel::spec(
            colors.get("surface")?,
            palette.clone(),
            colors.clone(),
            launcher::SourceChoice {
                fixture: menu_fixture.clone(),
                socket: daemon_socket.clone(),
                shared: shared_menu_source,
            },
            panel_trace,
            launcher_memory.clone().ok_or("launcher state missing")?,
            _prefs.as_ref().map(|(_, source)| source.clone()),
        ));
    }
    if core_views {
        let launcher = options.panel_spec.clone().ok_or("launcher view missing")?;
        let calendar = popouts::calendar(
            palette.clone(),
            colors.clone(),
            desktop_owner.clone(),
            panel_trace,
        )?;
        let mut views = std::collections::BTreeMap::from([
            ("launcher".into(), launcher),
            ("sound".into(), core_sound.ok_or("sound view missing")?),
            ("calendar".into(), calendar),
        ]);
        if tray_preview {
            views.insert(
                "tray".into(),
                tray::spec(
                    palette.clone(),
                    colors.clone(),
                    desktop_owner.clone(),
                    tray_selection.clone(),
                    panel_trace,
                )?,
            );
        }
        if let Some(client) = &_ui_provider {
            views.insert(
                "session".into(),
                popouts::session(client.clone(), palette.clone(), colors.clone(), panel_trace)?,
            );
            views.insert(
                "chat".into(),
                popouts::chat(
                    client.clone(),
                    palette.clone(),
                    colors.clone(),
                    Default::default(),
                    panel_trace,
                )?,
            );
        }
        let status_colors = std::sync::Arc::new(colors.clone());
        let status_palette = std::sync::Arc::new(palette.clone());
        let status_scale: serde_json::Value =
            serde_json::from_str(include_str!("../../bead-ui/assets/size-scale.json"))?;
        let status_radius = status_scale["radii"]["medium"]
            .as_f64()
            .ok_or("status radius missing from shared scale")? as f32;
        for kind in status_views::Kind::ALL {
            views.insert(
                kind.name().into(),
                popouts::status(
                    kind,
                    status_palette.clone(),
                    status_colors.clone(),
                    desktop_owner.clone(),
                    _ui_provider.clone(),
                    status_radius,
                    panel_trace,
                )?,
            );
        }
        for kind in [
            status_views::Kind::Network,
            status_views::Kind::Bluetooth,
            status_views::Kind::Battery,
            status_views::Kind::Audio,
        ] {
            views.insert(
                format!("peek-{}", kind.name()),
                popouts::status_peek(
                    kind,
                    status_palette.clone(),
                    status_colors.clone(),
                    desktop_owner.clone(),
                    _ui_provider.clone(),
                    status_radius,
                    panel_trace,
                )?,
            );
        }
        for page in dashboard::Page::ALL
            .into_iter()
            .chain(dashboard::Page::EXTRAS)
        {
            views.insert(
                format!("dashboard-{}", page.name()),
                dashboard::spec(
                    page,
                    status_palette.clone(),
                    status_colors.clone(),
                    dashboard::Choice {
                        client: _ui_provider.clone(),
                        fixtures: dashboard_fixtures.clone(),
                        prefs: _prefs.as_ref().map(|(_, source)| source.clone()),
                    },
                    desktop_owner.clone(),
                    panel_trace,
                )?,
            );
        }
        options.panels = Some(bead::wayland::Panels::new("launcher".into(), views)?);
        options.window = Some(settings::window_spec(
            palette.clone(),
            colors.clone(),
            _ui_provider.clone(),
            panel_trace,
        )?);
    }
    let notification_owner = std::sync::Arc::new(std::sync::OnceLock::new());
    let notice_service = match &notice_daemon {
        Some(socket) => {
            let service = notice_service::Service::start(
                socket.clone(),
                notice_service::Options {
                    provider: notice_provider,
                    action_on_click: notice_action_on_click,
                },
            )?;
            let dynamic: std::sync::Arc<dyn notifications::Notices> = service;
            let _ = notification_owner.set(dynamic.clone());
            Some(dynamic)
        }
        None => None,
    };
    if notification_mode {
        let owner = notification_owner.clone();
        let colors = colors.clone();
        let palette = palette.clone();
        let mut fallback = sound::fallback(colors.get("surface")?);
        fallback.width = 340.;
        fallback.bounds = Rect::new(-32., -32., 404., 144.);
        fallback.primitives.clear();
        if let Some(m) = &mut fallback.motion {
            m.id = "osd".into();
            m.edge = bead::core::Edge::Bottom;
        }
        let cold = fallback.clone();
        options.panel_spec = Some(bead::wayland::PanelSpec {
            position: None,
            engages_decorations: false,
            // The daemon OSD takes pointer input (hover holds it open, as in QML); the
            // private compatibility owner stays click-through.
            passive: notice_daemon.is_none(),
            keyboard_focus: false,
            initial_open: false,
            fallback,
            renderer: bead::render::RendererKind::Cpu,
            factory: std::sync::Arc::new(move |open| {
                let session = bead::ui::Session::new(
                    "shell".into(),
                    "osd/main.luau".into(),
                    palette.clone(),
                    colors.clone(),
                    cold.clone(),
                );
                let owner = owner.get().cloned().ok_or("notification owner not ready")?;
                reload::Runtime::panel_session(session, open)
                    .with_trace(panel_trace)
                    .with_notification_owner(owner, open)
                    .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                    .map_err(|e| e.to_string())
            }),
        });
        scene.primitives.clear();
        scene.width = 400.;
        scene.bounds = Rect::new(-32., -32., 464., 65.);
        scene.motion = None;
    }
    let mut wallpaper_consumer = None;
    let mut _motion_wallpaper = None;
    if let Some(socket) = daemon_socket.clone() {
        match wallpaper::Consumer::start(socket) {
            Ok(provider) => {
                wallpaper_consumer = Some(provider.clone());
                options.wallpaper = Some(provider);
            }
            Err(_) => eprintln!("WALLPAPER_DISABLED code=contract_unavailable"),
        }
    }
    // Service owners outlive individual output sessions and their workers.
    let shared_backend = if backend_enabled {
        Some(backends::Native::from_env()?)
    } else {
        None
    };
    let shared_desktop = options.bar.then(bead::services::Desktop::start);
    if let Some(desktop) = &shared_desktop {
        let _ = desktop_owner.set(desktop.clone());
    }
    // B0046: motion wallpaper control; the decoder runs in `bead-wallpaper-video`.
    if let Some(wallpaper) = wallpaper_consumer.take() {
        _motion_wallpaper = wallpaper_motion::Controller::start(wallpaper_motion::Deps {
            picker: wallpaper.picker(),
            wallpaper,
            backend: shared_backend
                .clone()
                .map(|b| b as std::sync::Arc<dyn bead::backends::Backend>),
            desktop: shared_desktop.clone(),
        });
    }
    if let Some(entry) = ui {
        let ui_entry_is_launcher = entry == Path::new("ui/main.luau");
        let fallback = scene.clone();
        let bar = options.bar;
        let brightness_client = _ui_provider.clone().filter(|_| core_views && bar);
        let brightness_osd = osd_socket.clone();
        // B0022: with a daemon, bar actions are N0031 calls and the tray is its topic.
        let bar_routes = brightness_client
            .as_ref()
            .map(|client| {
                client.topic("tray").map(|topic| {
                    (
                        client.clone(),
                        std::sync::Arc::new(topic)
                            as std::sync::Arc<dyn bead::core::source::Source>,
                    )
                })
            })
            .transpose()
            .map_err(|e| e.to_string())?;
        let trace = options.trace_state;
        let prefs_source = _prefs
            .as_ref()
            .map(|(_, source)| source.clone())
            .filter(|_| bar);
        let make_source = move |output: Option<String>| {
            let prefs_source = prefs_source.clone();
            let brightness_client = brightness_client.clone();
            let brightness_osd = brightness_osd.clone();
            let bar_routes = bar_routes.clone();
            let entry = entry.clone();
            let palette = palette.clone();
            let colors = colors.clone();
            let fallback = fallback.clone();
            let connection_status = connection_status.clone();
            let launcher_memory = launcher_memory.clone();
            let tray_selection = tray_selection.clone();
            let menu_fixture = menu_fixture.clone();
            let daemon_socket = daemon_socket.clone();
            let notification_bus = notification_bus.clone();
            let notification_history = notification_history.clone();
            let notification_owner = notification_owner.clone();
            let notice_service = notice_service.clone();
            let shared_backend = shared_backend.clone();
            let shared_desktop = shared_desktop.clone();
            let desktop_owner = desktop_owner.clone();
            let worker_fallback = fallback.clone();
            let factory: bead::wayland::worker::Factory = Box::new(move || {
                let result =
                (|| -> Result<Box<dyn bead::wayland::SceneSource>, Box<dyn std::error::Error>> {
                    let session =
                        bead::ui::Session::new("shell".into(), entry, palette, colors, fallback);
                    let runtime = reload::Runtime::for_output(session, output).with_trace(trace);
                    let runtime = if let Some(source) = connection_status {
                        runtime.with_connection_status(source)?
                    } else {
                        runtime
                    };
                    let runtime = if launcher_panel && bar {
                        runtime.with_launcher_trigger(
                            launcher_memory.clone().ok_or("launcher state missing")?,
                        )?
                    } else {
                        runtime
                    };
                    let runtime = if core_views && bar {
                        runtime
                            .with_popout_trigger()?
                            .with_tray_bar(tray_selection.clone(), tray_preview)
                    } else {
                        runtime
                    };
                    let runtime = if let Some(client) = brightness_client {
                        runtime.with_brightness_keys(client, brightness_osd)?
                    } else {
                        runtime
                    };
                    let runtime = if ui_entry_is_launcher {
                        let source =
                            launcher::source(menu_fixture.as_deref(), daemon_socket.as_deref())?;
                        runtime.with_launcher(source)?
                    } else {
                        runtime
                    };
                    let runtime = if let Some(service) = notice_service {
                        runtime.with_notification_service(service)?
                    } else if let Some(socket) = notification_bus {
                        let history = notification_history.ok_or(
                            "--notification-bus requires --notification-history (private state)",
                        )?;
                        runtime
                            .with_notifications(socket, history)?
                            .with_shared_desktop(bead::services::Desktop::start(), false)?
                    } else {
                        runtime
                    };
                    if let Some(owner) = runtime.notification_owner() {
                        let _ = notification_owner.set(owner);
                    }
                    let runtime = if let Some(backend) = shared_backend {
                        runtime.with_backend(backend)?
                    } else {
                        runtime
                    };
                    let runtime = if let Some(desktop) = shared_desktop {
                        runtime.with_shared_desktop(desktop, true)?
                    } else {
                        runtime
                    };
                    if let Some(desktop) = runtime.desktop_owner() {
                        let _ = desktop_owner.set(desktop);
                    }
                    let runtime = if let Some(source) = prefs_source {
                        runtime.with_prefs(source)?
                    } else {
                        runtime
                    };
                    let runtime = if let Some((client, tray)) = bar_routes {
                        runtime
                            .with_bar_routes(client.clone(), tray)?
                            .with_workspace_origins(&client)?
                    } else {
                        runtime
                    };
                    Ok(Box::new(runtime))
                })();
                result.map_err(|e| e.to_string())
            });
            Box::new(bead::wayland::worker::Source::new(factory, worker_fallback))
                as Box<dyn bead::wayland::SceneSource>
        };
        let renderer = bead::render::renderer(options.renderer)?;
        if bar {
            options.primary_factory = Some(std::sync::Arc::new(move |output| {
                Ok(make_source(Some(output)))
            }));
            bead::wayland::run_runtime(options, scene, renderer, None)
        } else {
            let runtime = make_source(None);
            bead::wayland::run_runtime(options, scene, renderer, Some(runtime))
        }
    } else {
        run(options, scene)
    }
}
fn panel(p: &Palette, assets: &Path) -> Result<Scene, Box<dyn std::error::Error>> {
    let body = p.get("surface")?;
    let raised = p.get("surfaceContainer")?;
    let ink = p.get("onSurface")?;
    let muted = p.get("onSurfaceVariant")?;
    let accent = p.get("primary")?;
    let outline = p.get("outlineVariant")?;
    let mut v = vec![
        Primitive::Shadow {
            rect: Rect::new(-30., -30., 1160., 650.),
            radius: 12.,
            blur: 10.,
        },
        Primitive::Rounded {
            rect: Rect::new(0., 0., 1100., 590.),
            radius: 12.,
            fill: body,
            border: Some((outline, 1.)),
        },
        Primitive::Rounded {
            rect: Rect::new(182., 20., 898., 48.),
            radius: 13.,
            fill: raised,
            border: None,
        },
        Primitive::Rounded {
            rect: Rect::new(12., 84., 150., 40.),
            radius: 10.,
            fill: raised,
            border: None,
        },
    ];
    {
        let mut text = |x, y, w, h, s: &str, size, color| {
            v.push(Primitive::Text {
                rect: Rect::new(x, y, w, h),
                text: s.into(),
                family: "IBM Plex Sans".into(),
                align: None,
                size,
                color,
            })
        };
        text(202., 30., 860., 32., "Search applications…", 18., muted);
        for (i, label) in [
            "Home",
            "Applications",
            "System",
            "Settings",
            "Files",
            "Wallpaper",
        ]
        .iter()
        .enumerate()
        {
            text(
                28.,
                93. + i as f32 * 48.,
                120.,
                26.,
                label,
                16.,
                if i == 0 { accent } else { ink },
            );
        }
        text(202., 83., 860., 34., "Applications", 22., ink);
        for row in 0..9 {
            for col in 0..3 {
                let n = row * 3 + col + 1;
                let x = 202. + col as f32 * 290.;
                let y = 128. + row as f32 * 43.;
                text(
                    x,
                    y,
                    270.,
                    23.,
                    &format!("Application {n:02} — Open"),
                    16.,
                    ink,
                );
                text(
                    x,
                    y + 21.,
                    270.,
                    20.,
                    "System tools and everyday tasks",
                    12.,
                    muted,
                );
            }
        }
        text(
            202.,
            535.,
            820.,
            30.,
            "Font fallback: Ελληνικά · العربية · 日本語 · 😀 🌈 🚀",
            16.,
            ink,
        );
    }
    v.push(Primitive::Image {
        blur: 0.,
        rect: Rect::new(28., 422., 48., 48.),
        path: assets.join("sample.svg"),
    });
    v.push(Primitive::Image {
        blur: 0.,
        rect: Rect::new(96., 422., 48., 48.),
        path: assets.join("sample.png"),
    });
    let theme = if std::env::var_os("BEAD_ICON_PATH").is_none() {
        freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "hicolor".into())
    } else {
        "hicolor".into()
    };
    let icon = std::env::var_os("BEAD_ICON_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            [theme.as_str(), "hicolor", "Adwaita"]
                .into_iter()
                .find_map(|theme| {
                    freedesktop_icons::lookup("folder")
                        .with_size(72)
                        .with_theme(theme)
                        .with_cache()
                        .find()
                })
        });
    if let Some(path) = icon {
        println!("ICON folder resolved=true");
        v.push(Primitive::Image {
            blur: 0.,
            rect: Rect::new(62., 490., 48., 48.),
            path,
        });
    } else {
        eprintln!("ICON folder unavailable in installed themes");
    }
    Ok(Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: Vec::new(),
        width: 1100.,
        slide: Some(Slide {
            closed_y: -650.,
            open_y: 30.,
            period_seconds: 2.,
        }),
        primitives: v,
        offset: (0., 0.),
        bounds: Rect::new(-32., -32., 1164., 654.),
    })
}

fn legacy_sound_panel_enabled(audio_panel: bool, core_views: bool) -> bool {
    audio_panel && !core_views
}

fn select_renderer(options: &mut Options, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use bead::render::RendererKind;
    options.auto_fallback = name == "auto";
    options.renderer = match name {
        "cpu" => RendererKind::Cpu,
        "femtovg" => RendererKind::Femtovg,
        "femtovg-gl" => RendererKind::FemtovgGl,
        "vello" => RendererKind::Vello,
        "auto" => {
            if cfg!(feature = "native-gl") {
                RendererKind::FemtovgGl
            } else {
                RendererKind::Cpu
            }
        }
        _ => return Err("renderer must be cpu, femtovg-gl, femtovg, vello or auto".into()),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_views_never_select_legacy_sound_factory() {
        assert!(!legacy_sound_panel_enabled(true, true));
        assert!(legacy_sound_panel_enabled(true, false));
    }

    #[test]
    fn checked_in_ui_bundle_and_assets_are_complete() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let palette_path = root.join("palette.json");
        let palette = Palette::read(&palette_path).unwrap();
        let fallback = panel(&palette, &root).unwrap();
        let mut session = bead::ui::Session::new(
            root,
            "legacy-ui/main.json".into(),
            palette_path,
            palette,
            fallback,
        );
        let scene = session.reload().unwrap();
        session.check_sources().unwrap();
        assert!(scene.primitives.len() > 40);
        assert!(!scene.asset_data.is_empty());
        session.commit(scene);
        let state: serde_json::Value = serde_json::from_str(&session.state_summary()).unwrap();
        assert_eq!(state["launcher.search.text"], "");
        assert_eq!(state["launcher.open"], true);
        session = bead::ui::Session::new(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell"),
            "ui/main.luau".into(),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"),
            Palette::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"))
                .unwrap(),
            session.error_scene(),
        );
        let scene = session.reload().unwrap();
        // Core navigation replaces the old six-entry placeholder sidebar. Check
        // its actual content and input contract rather than a primitive count.
        for label in ["Nacre", "Favorites", "Applications", "Actions"] {
            assert!(
                scene
                    .primitives
                    .iter()
                    .any(|p| matches!(p,Primitive::Text{text,..} if text==label))
            );
        }
        assert!(!scene.asset_data.is_empty());
        session.commit(scene);
        assert!(
            session
                .view_nodes()
                .iter()
                .any(|n| n.input.as_deref() == Some("launcher.search.text"))
        );
    }
}

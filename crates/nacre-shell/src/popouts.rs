// SPDX-License-Identifier: GPL-3.0-only
//! Lazy popout factories share the bar's existing hosted service.
use bead::{
    render::{Palette, Primitive, Rect, RendererKind},
    wayland::PanelSpec,
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};
pub fn calendar(
    palette: PathBuf,
    colors: Palette,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 310.;
    fallback.bounds = Rect::new(-32., -32., 374., 376.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 310., 312.),
        radius: 12.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "popout".into();
        motion.edge = bead::core::Edge::Top;
        motion.duration = 0.23;
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "popouts/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let desktop = owner.get().cloned().ok_or("bar services not ready")?;
            let source = Arc::new(bead::source::Clock::attach(desktop).map_err(|e| e.to_string())?);
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .and_then(|runtime| runtime.with_calendar(source))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

/// Closed views retain only a small factory and fallback geometry. Fonts, Luau,
/// service subscriptions and data snapshots are created by the panel worker.
pub fn status(
    kind: crate::status_views::Kind,
    palette: Arc<PathBuf>,
    colors: Arc<Palette>,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    client: Option<crate::nacre_source::Client>,
    radius: f32,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    status_at_root_mode(
        kind,
        palette,
        colors,
        owner,
        client,
        (radius, trace, StatusPanelMode::Popout),
        (PathBuf::from("shell"), None),
    )
}

/// Dedicated Sound popout in core mode. Its audio topic lease is acquired only
/// while the top-edge popout is open and is owned by that Runtime.
pub fn core_sound(
    client: Option<crate::nacre_source::Client>,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    core_sound_with_root(PathBuf::from("shell"), client, palette, colors, trace)
}

pub(crate) fn core_sound_with_root(
    root: PathBuf,
    client: Option<crate::nacre_source::Client>,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 382.;
    fallback.bounds = Rect::new(-32., -32., 446., 350.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 382., 286.),
        radius: 16.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "sound-output".into();
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.edge = bead::core::Edge::Top;
        motion.duration = 0.23;
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let source: Arc<dyn bead::core::source::Source> = match client.as_ref() {
                Some(client) => Arc::new(client.lease_topic("audio").map_err(|e| e.to_string())?),
                None => crate::dashboard::unavailable_audio_source().map_err(|e| e.to_string())?,
            };
            let session = bead::ui::Session::new(
                root.clone(),
                "sound/core.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("top")?
                .with_panel_geometry(382., 286., 0.23)
                .map_err(|error| error.to_string())?
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .and_then(|runtime| runtime.with_core_audio_status(source.clone(), client.clone()))
                .and_then(|runtime| {
                    runtime.with_status_controls(client.clone(), vec![("audio", source)])
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

/// Lazy power dropdown. Widget clicks and the session route share the existing guarded session controller.
pub fn session(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let (width, height) = (310., 332.);
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = width;
    fallback.bounds = Rect::new(-32., -32., width + 64., height + 64.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., width, height),
        radius: 16.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "session".into();
        motion.edge = bead::core::Edge::Top;
        motion.duration = 0.23;
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                PathBuf::from("shell"),
                "session/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("top")?
                .with_panel_geometry(width as f64, height as f64, 0.23)
                .map_err(|error| error.to_string())?
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .and_then(|runtime| runtime.with_session_menu(client.clone()))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

pub(crate) type StatusInit = Arc<dyn Fn(&mut crate::reload::Runtime) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusPanelMode {
    #[cfg(test)]
    Legacy,
    Peek,
    Popout,
}

pub(crate) fn status_peek(
    kind: crate::status_views::Kind,
    palette: Arc<PathBuf>,
    colors: Arc<Palette>,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    client: Option<crate::nacre_source::Client>,
    radius: f32,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    status_at_root_mode(
        kind,
        palette,
        colors,
        owner,
        client,
        (radius, trace, StatusPanelMode::Peek),
        (PathBuf::from("shell"), None),
    )
}

/// Private fixtures supply a root and scoped initialization without changing
/// the process environment or the factory used in production.
#[cfg(test)]
pub(crate) fn status_at_root(
    kind: crate::status_views::Kind,
    palette: Arc<PathBuf>,
    colors: Arc<Palette>,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    client: Option<crate::nacre_source::Client>,
    style: (f32, bool),
    fixture: (PathBuf, Option<StatusInit>),
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    status_at_root_mode(
        kind,
        palette,
        colors,
        owner,
        client,
        (style.0, style.1, StatusPanelMode::Legacy),
        fixture,
    )
}

fn status_at_root_mode(
    kind: crate::status_views::Kind,
    palette: Arc<PathBuf>,
    colors: Arc<Palette>,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    client: Option<crate::nacre_source::Client>,
    style: (f32, bool, StatusPanelMode),
    fixture: (PathBuf, Option<StatusInit>),
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let (radius, trace, mode) = style;
    let peek = mode == StatusPanelMode::Peek;
    let (root, initialize) = fixture;
    let fill = colors.get("surface")?;
    let (panel_width, panel_height) = if peek {
        // QML's 280 px content sits in a 15 px inset on every side.
        (310., 110.)
    } else if matches!(kind, crate::status_views::Kind::Network) && client.is_some() {
        // QML's 352 px body sits in a 15 px inset inside the framed popup.
        (382., crate::status_views::NETWORK_BASE_HEIGHT as f32)
    } else if matches!(kind, crate::status_views::Kind::Bluetooth) && client.is_some() {
        // QML's 352 px body sits in a 15 px inset inside the framed popup.
        (382., 100.)
    } else if matches!(kind, crate::status_views::Kind::Audio) {
        (382., 286.)
    } else if matches!(kind, crate::status_views::Kind::Battery) {
        (230., 224.)
    } else {
        (382., 222.)
    };
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = panel_width;
    fallback.bounds = Rect::new(-32., -32., panel_width + 64., panel_height + 64.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., panel_width, panel_height),
        radius,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "status-popout".into();
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.edge = bead::core::Edge::Top;
        motion.duration = 0.23;
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: !peek,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: !peek,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let controlled_network =
                matches!(kind, crate::status_views::Kind::Network) && client.is_some();
            let controlled_bluetooth =
                matches!(kind, crate::status_views::Kind::Bluetooth) && client.is_some();
            let session = bead::ui::Session::new(
                root.clone(),
                if peek {
                    "status/peek.luau"
                } else if controlled_network {
                    "status/network.luau"
                } else if controlled_bluetooth {
                    "status/bluetooth.luau"
                } else if matches!(kind, crate::status_views::Kind::Updates) && !peek {
                    "status/updates.luau"
                } else if matches!(kind, crate::status_views::Kind::Battery) {
                    "status/battery.luau"
                } else {
                    "status/main.luau"
                }
                .into(),
                (*palette).clone(),
                (*colors).clone(),
                cold.clone(),
            );
            let typed_topic = match kind {
                crate::status_views::Kind::Network => Some("network"),
                crate::status_views::Kind::Bluetooth => Some("bluetooth"),
                crate::status_views::Kind::Battery => Some("battery"),
                crate::status_views::Kind::Audio => Some("audio"),
                crate::status_views::Kind::Updates => Some("dashboard.updates"),
            }
            .filter(|_| client.is_some());
            let source: Arc<dyn bead::core::source::Source> = if let Some(topic) = typed_topic {
                let client = client.as_ref().ok_or("daemon client unavailable")?;
                Arc::new(client.lease_topic(topic).map_err(|e| e.to_string())?)
            } else {
                let desktop = owner.get().cloned().ok_or("bar services not ready")?;
                Arc::new(
                    bead::source::DesktopView::attach(desktop, kind.projection())
                        .map_err(|e| e.to_string())?,
                )
            };
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("top")?
                .with_panel_geometry(panel_width as f64, panel_height as f64, 0.23)
                .map_err(|error| error.to_string())?
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .and_then(|runtime| match typed_topic {
                    Some("network") if controlled_network => runtime.with_core_network_control(
                        source,
                        client
                            .as_ref()
                            .expect("controlled network has shared daemon client")
                            .clone(),
                    ),
                    Some("network") => runtime.with_core_network_status(source),
                    Some("bluetooth") if controlled_bluetooth => runtime
                        .with_core_bluetooth_control(
                            source,
                            client
                                .as_ref()
                                .expect("controlled Bluetooth has shared daemon client")
                                .clone(),
                        ),
                    Some("bluetooth") => runtime.with_core_bluetooth_status(source),
                    Some("battery") => {
                        let profiles: Arc<dyn bead::core::source::Source> = Arc::new(
                            client
                                .as_ref()
                                .ok_or("daemon client unavailable")?
                                .lease_topic("power.profiles")?,
                        );
                        runtime
                            .with_core_battery_status(source)?
                            .with_status_controls(
                                client.clone(),
                                vec![("power.profiles", profiles)],
                            )
                    }
                    Some("audio") => runtime.with_core_audio_status(source, client.clone()),
                    Some("dashboard.updates") => runtime.with_core_updates_status(
                        source,
                        client.as_ref().ok_or("daemon client unavailable")?.clone(),
                    ),
                    _ => runtime.with_status(kind, source),
                })
                .map(|mut runtime| {
                    if let Some(initialize) = &initialize {
                        initialize(&mut runtime);
                    }
                    Box::new(runtime) as Box<dyn bead::wayland::SceneSource>
                })
                .map_err(|e| e.to_string())
        }),
    })
}

/// Draft text and section survive a closed panel inside this process only; typed text
/// is never written to disk (`state.rs`). Cleared by the shell exiting.
pub type ChatMemory = Arc<std::sync::Mutex<std::collections::BTreeMap<String, bead::core::Value>>>;

/// AI sidebar (B0043), the QML left drawer: 470 wide on the left edge. The `ai.chat`
/// topic lease, the layout caches and the heartbeat exist only for the open panel.
pub fn chat(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    memory: ChatMemory,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let (width, height) = (crate::chat::WIDTH as f32, 650.);
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = width;
    fallback.bounds = Rect::new(-32., -32., width + 64., height + 64.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., width, height),
        radius: 18.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "chat".into();
        motion.edge = bead::core::Edge::Left;
        motion.duration = 0.4;
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.reveal = true;
        motion.fade = false;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::Attached(bead::core::Edge::Left),
        )),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let topic = client.lease_topic("ai.chat").map_err(|e| e.to_string())?;
            let session = bead::ui::Session::new(
                PathBuf::from("shell"),
                "chat/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let chat = crate::chat::Chat::new(
                Some(client.clone()),
                Arc::new(topic),
                crate::chat::system_meter(),
            )
            .with_memory(memory.clone());
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("left")?
                .with_frame(
                    crate::placement::Attachment::Attached(bead::core::Edge::Left),
                    &colors,
                )
                .and_then(|runtime| runtime.with_chat(chat))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

/// Control center (B0049), the QML right sidebar: 424 wide on the right edge under the pill.
/// The topic leases behind its data sources exist only while the panel is open.
pub fn control_center(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    views: Vec<String>,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let (width, height) = (424., 300.);
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = width;
    fallback.bounds = Rect::new(-32., -32., width + 64., height + 64.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., width, height),
        radius: 16.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "control".into();
        motion.edge = bead::core::Edge::Right;
        motion.duration = 0.28;
        motion.slide_distance = Some(width);
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default()
                .positioner(crate::placement::Attachment::ControlCenter),
        ),
        engages_decorations: true,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let topics = crate::control_center::TOPICS
                .iter()
                .filter_map(|name| client.view_topic(name).ok().map(|topic| (*name, topic)))
                .collect();
            let store = client
                .lease_topic(crate::control_center::STORE_TOPIC)
                .ok()
                .map(|topic| Arc::new(topic) as Arc<dyn bead::core::source::Source>);
            let control = crate::control_center::ControlCenter::new(
                Some(client.clone()),
                topics,
                store,
                views.clone(),
            );
            let session = bead::ui::Session::new(
                PathBuf::from("shell"),
                "control/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("right")?
                .with_frame(crate::placement::Attachment::ControlCenter, &colors)
                .and_then(|runtime| runtime.with_control_center(control))
                .and_then(|runtime| runtime.with_docked_details(client.clone()))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

/// Compact notification hover peek. It uses the same ControlCenter source and
/// notification projection as the full drawer, with only its notification topic leased.
pub fn notification_peek(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    notification_panel(client, palette, colors, trace, true)
}

pub fn notification_history(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    notification_panel(client, palette, colors, trace, false)
}

fn notification_panel(
    client: crate::nacre_source::Client,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
    peek: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let (width, height) = if peek { (310., 74.) } else { (410., 110.) };
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = width;
    fallback.bounds = Rect::new(-32., -32., width + 64., height + 64.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., width, height),
        radius: 12.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "notification-peek".into();
        motion.edge = bead::core::Edge::Top;
        motion.duration = 0.23;
        motion.curve = Some(bead::core::animation::Curve::EaseInOut);
        motion.fade = false;
        motion.reveal = true;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: !peek,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: !peek,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let topics = ["notifications", "controls"]
                .iter()
                .filter(|name| !peek || **name == "notifications")
                .filter_map(|name| client.view_topic(name).ok().map(|topic| (*name, topic)))
                .collect();
            let control = crate::control_center::ControlCenter::new(
                Some(client.clone()),
                topics,
                None,
                Vec::new(),
            );
            let session = bead::ui::Session::new(
                PathBuf::from("shell"),
                if peek {
                    "control/peek.luau"
                } else {
                    "control/main.luau"
                }
                .into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_view_edge("top")?
                .with_control_center(control)
                .and_then(|runtime| {
                    runtime.with_frame(crate::placement::Attachment::Popout, &colors)
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

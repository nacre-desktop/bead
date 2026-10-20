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
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        passive: false,
        initial_open: false,
        fallback,
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
    status_at_root(
        kind,
        palette,
        colors,
        owner,
        client,
        (radius, trace),
        ("shell".into(), None),
    )
}

/// Dedicated Sound route in core mode. The audio topic lease is acquired only
/// when the right-side panel opens and is owned by that Runtime.
pub fn core_sound(
    client: Option<crate::nacre_source::Client>,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    core_sound_with_root("shell".into(), client, palette, colors, trace)
}

pub(crate) fn core_sound_with_root(
    root: String,
    client: Option<crate::nacre_source::Client>,
    palette: PathBuf,
    colors: Palette,
    trace: bool,
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    if let Some(motion) = &mut fallback.motion {
        motion.id = "sound-output".into();
        motion.edge = bead::core::Edge::Right;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(crate::placement::Frame::default().positioner(
            crate::placement::Attachment::Attached(bead::core::Edge::Right),
        )),
        engages_decorations: true,
        passive: false,
        initial_open: false,
        fallback,
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
                .with_frame(
                    crate::placement::Attachment::Attached(bead::core::Edge::Right),
                    &colors,
                )
                .and_then(|runtime| runtime.with_core_audio_status(source))
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

pub(crate) type StatusInit = Arc<dyn Fn(&mut crate::reload::Runtime) + Send + Sync>;

/// Private fixtures supply a root and scoped initialization without changing
/// the process environment or the factory used in production.
pub(crate) fn status_at_root(
    kind: crate::status_views::Kind,
    palette: Arc<PathBuf>,
    colors: Arc<Palette>,
    owner: Arc<OnceLock<Arc<bead::services::Desktop>>>,
    client: Option<crate::nacre_source::Client>,
    style: (f32, bool),
    fixture: (PathBuf, Option<StatusInit>),
) -> Result<PanelSpec, Box<dyn std::error::Error>> {
    let (radius, trace) = style;
    let (root, initialize) = fixture;
    let fill = colors.get("surface")?;
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 310.;
    fallback.bounds = Rect::new(-32., -32., 374., 286.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 310., 222.),
        radius,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "status-popout".into();
        motion.edge = bead::core::Edge::Top;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: true,
        passive: false,
        initial_open: false,
        fallback,
        renderer: RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let controlled_network =
                matches!(kind, crate::status_views::Kind::Network) && client.is_some();
            let controlled_bluetooth =
                matches!(kind, crate::status_views::Kind::Bluetooth) && client.is_some();
            let session = bead::ui::Session::new(
                root.clone(),
                if controlled_network {
                    "status/network.luau"
                } else if controlled_bluetooth {
                    "status/bluetooth.luau"
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
                crate::status_views::Kind::Audio => None,
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
                    Some("battery") => runtime.with_core_battery_status(source),
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

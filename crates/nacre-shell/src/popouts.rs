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
            let session = bead::ui::Session::new(
                "shell".into(),
                "status/main.luau".into(),
                (*palette).clone(),
                (*colors).clone(),
                cold.clone(),
            );
            let typed_topic = match kind {
                crate::status_views::Kind::Network => Some("network"),
                crate::status_views::Kind::Bluetooth => Some("bluetooth"),
                crate::status_views::Kind::Battery => None,
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
                .and_then(|runtime| {
                    match typed_topic {
                        Some("network") => runtime.with_core_network_status(source),
                        Some("bluetooth") => runtime.with_core_bluetooth_status(source),
                        _ => runtime.with_status(kind, source),
                    }
                })
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

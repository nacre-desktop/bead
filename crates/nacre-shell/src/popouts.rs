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
                .with_calendar(source)
                .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    })
}

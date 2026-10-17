// SPDX-License-Identifier: GPL-3.0-only
//! The existing bounded panel lifecycle hosts the launcher without warm rasters.
use bead::{
    core::Edge,
    render::{Palette, Primitive, Rect},
    wayland::PanelSpec,
};
use std::path::PathBuf;
pub fn spec(
    fill: [u8; 4],
    palette: PathBuf,
    colors: Palette,
    source: crate::launcher::SourceChoice,
    trace: bool,
    memory: crate::launcher::Memory,
) -> PanelSpec {
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 1100.;
    // Match the final content geometry; layout uses output logical dimensions.
    fallback.bounds = Rect::new(-32., -32., 1164., 654.);
    fallback.primitives = vec![Primitive::Rounded {
        rect: Rect::new(0., 0., 1100., 590.),
        radius: 12.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "launcher".into();
        motion.edge = Edge::Top;
    }
    let cold = fallback.clone();
    PanelSpec {
        position: Some(
            crate::placement::Frame::default()
                .positioner(crate::placement::Attachment::Attached(Edge::Top)),
        ),
        engages_decorations: false,
        passive: false,
        initial_open: false,
        fallback,
        renderer: bead::render::RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "ui/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let source = source.connect().map_err(|e| e.to_string())?;
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(trace)
                .with_frame(crate::placement::Attachment::Attached(Edge::Top), &colors)
                .and_then(|r| r.with_launcher(source))
                .and_then(|r| r.with_launcher_memory(memory.clone()))
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    }
}

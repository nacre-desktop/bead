// SPDX-License-Identifier: MIT OR Apache-2.0
//! Trusted minimal scene; no script, service, input or application state dependency.
use bead_render::{Palette, Primitive, Rect, Scene};
pub fn safe_bar(palette: &Palette) -> Scene {
    Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: 1200.,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 1200., 50.),
        primitives: vec![
            Primitive::Rounded {
                rect: Rect::new(0., 0., 16384., 50.),
                radius: 0.,
                fill: palette.get("surface").unwrap_or([20, 20, 20, 255]),
                border: None,
            },
            Primitive::Text {
                rect: Rect::new(16., 8., 1000., 36.),
                text: "Bead safe mode — repeated UI crashes; services are not started".into(),
                family: "sans-serif".into(),
                size: 16.,
                color: palette.get("onSurface").unwrap_or([240, 240, 240, 255]),
                align: None,
            },
        ],
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//! A real output-volume view, sharing the bar's native service owner.
use bead::{
    core::Edge,
    render::{Motion, Primitive, Rect, Scene},
};
pub fn fallback(fill: [u8; 4]) -> Scene {
    Scene {
        opacity: 1.,
        motion: Some(Motion {
            id: "sound".into(),
            open: true,
            duration: 0.2,
            spring: false,
            curve: None,
            edge: Edge::Right,
            slide_distance: None,
            fade: true,
            reveal: false,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        }),
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: 400.,
        slide: None,
        offset: (32., 32.),
        // Match the kit's 32px shadow inset so CPU startup does not allocate
        // a second SHM geometry as soon as the script becomes ready.
        bounds: Rect::new(-32., -32., 464., 350.),
        primitives: vec![Primitive::Rounded {
            rect: Rect::new(0., 0., 400., 286.),
            radius: 16.,
            fill,
            border: None,
        }],
    }
}

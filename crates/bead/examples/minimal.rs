// SPDX-License-Identifier: MIT OR Apache-2.0
use bead::render::{Primitive, Rect, Scene};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: 240.,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., 240., 80.),
        primitives: vec![Primitive::Rounded {
            rect: Rect::new(0., 0., 240., 80.),
            radius: 12.,
            fill: [40, 70, 90, 255],
            border: None,
        }],
    };
    if std::env::args().any(|a| a == "--check") {
        let mut renderer = bead::render::renderer(Default::default())?;
        renderer.prepare(&scene, 1.)?;
        println!("FACADE_CHECK_OK");
        return Ok(());
    }
    bead::wayland::run(
        bead::wayland::Options {
            namespace: "bead-minimal-example".into(),
            ..Default::default()
        },
        scene,
    )
}

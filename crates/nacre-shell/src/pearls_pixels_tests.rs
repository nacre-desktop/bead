// SPDX-License-Identifier: GPL-3.0-only
//! B0048 goldens: the same panel docked to its edge, mid-drag with the guide, and
//! detached (at rest and under the pointer), at scale 1 and 1.5.
use super::*;
use crate::{
    chat::tests::{offline, root, serial, state, topic},
    placement::{Attachment, Chrome},
};
use bead::{
    core::{Stores, pearl},
    render::{Damage, PixelFormat, RendererKind, ShmTarget},
};
use bead_test::{Frame as Pixels, Result};

const DESKTOP: [u8; 3] = [0x34, 0x37, 0x3f];
const CANVAS: (u32, u32) = (1000, 760);

fn palette() -> Palette {
    Palette::read(&root().join("palette.json")).unwrap()
}
fn chat_scene(scale: f32) -> Result<Scene> {
    let (_source, chat) = offline(topic(
        &[
            ("u1", "user", "Why does the release build fail on CI?"),
            (
                "a1",
                "assistant",
                "The linker runs out of memory because the job builds with too many parallel jobs. Limit it to two.",
            ),
        ],
        state(false, "", None),
    ));
    let out = chat.project(&Stores::default());
    let h = crate::chat::tests::harness(out, scale)?;
    Ok(h.scene().clone())
}
/// Premultiplied pixels of one scene.
fn rasterize(scene: &Scene, size: (u32, u32), scale: f32) -> Vec<u8> {
    let (w, h) = (
        (size.0 as f32 * scale).ceil() as u32,
        (size.1 as f32 * scale).ceil() as u32,
    );
    let mut bytes = vec![0; w as usize * h as usize * 4];
    let mut renderer = bead::render::renderer(RendererKind::Cpu).unwrap();
    renderer.prepare(scene, scale).unwrap();
    renderer
        .render(
            scene,
            ShmTarget {
                bytes: &mut bytes,
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(w, h),
        )
        .unwrap();
    bytes
}
fn desktop(size: (u32, u32), scale: f32) -> Pixels {
    let (width, height) = (
        (size.0 as f32 * scale).ceil() as u32,
        (size.1 as f32 * scale).ceil() as u32,
    );
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..width * height {
        pixels.extend_from_slice(&[DESKTOP[0], DESKTOP[1], DESKTOP[2], 255]);
    }
    Pixels {
        width,
        height,
        pixels,
    }
}
/// Source-over of premultiplied `layer` onto the opaque canvas at logical (x, y).
fn over(canvas: &mut Pixels, layer: &[u8], size: (u32, u32), at: (f32, f32), scale: f32) {
    let (lw, lh) = (
        (size.0 as f32 * scale).ceil() as i32,
        (size.1 as f32 * scale).ceil() as i32,
    );
    let (ox, oy) = ((at.0 * scale).round() as i32, (at.1 * scale).round() as i32);
    for y in 0..lh {
        for x in 0..lw {
            let (cx, cy) = (x + ox, y + oy);
            if cx < 0 || cy < 0 || cx >= canvas.width as i32 || cy >= canvas.height as i32 {
                continue;
            }
            let s = ((y * lw + x) * 4) as usize;
            let d = ((cy as u32 * canvas.width + cx as u32) * 4) as usize;
            let a = u16::from(layer[s + 3]);
            for c in 0..3 {
                canvas.pixels[d + c] = (u16::from(layer[s + c])
                    + (u16::from(canvas.pixels[d + c]) * (255 - a) + 127) / 255)
                    .min(255) as u8;
            }
        }
    }
}
fn golden(name: &str, frame: &Pixels, scale: f32) -> Result<()> {
    let path = root().join(format!("tests/goldens/pearl-{name}-{scale}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("bead-pearl-{name}-{scale}-diff")),
    )
}
fn panel_shot(scene: &Scene, scale: f32) -> Pixels {
    let size = (534, 714);
    let mut canvas = desktop(size, scale);
    over(
        &mut canvas,
        &rasterize(scene, size, scale),
        size,
        (0., 0.),
        scale,
    );
    canvas
}

#[test]
fn docked_and_detached_goldens() -> Result<()> {
    let _guard = serial();
    let colors = palette();
    for scale in [1., 1.5] {
        let base = chat_scene(scale)?;
        let mut docked = base.clone();
        Chrome::new(Attachment::Attached(bead::core::Edge::Left), &colors)?
            .decorate(&mut docked)
            .unwrap();
        golden("docked", &panel_shot(&docked, scale), scale)?;
        let mut rest = base.clone();
        let mut chrome = Chrome::new(Attachment::Pearl, &colors)?;
        chrome.decorate(&mut rest).unwrap();
        golden("detached", &panel_shot(&rest, scale), scale)?;
        // Decorating again must not pile up assets or move the body.
        chrome.decorate(&mut rest).unwrap();
        assert_eq!(
            rest.asset_data
                .keys()
                .filter(|p| p.to_string_lossy().starts_with("@pearl-body-"))
                .count(),
            1
        );
        let mut hot = base.clone();
        chrome.set_engaged(true);
        chrome.decorate(&mut hot).unwrap();
        golden("detached-engaged", &panel_shot(&hot, scale), scale)?;
    }
    Ok(())
}

/// The panel dragged towards its (left) edge with the guide the drag state machine draws.
fn dragging(scale: f32, at: f64) -> Result<Pixels> {
    let colors = palette();
    let ink = Ink::new(&colors)?;
    let mut panel = chat_scene(scale)?;
    Chrome::new(Attachment::Pearl, &colors)?
        .decorate(&mut panel)
        .unwrap();
    let body = crate::placement::body(&panel);
    // The surface paints the body at its scene offset; put the body `at` from the edge.
    let pearl_rect = pearl::Rect::new(10. + at, 60., body.w as f64, body.h as f64);
    let monitor = pearl::Monitor {
        name: "A".into(),
        rect: pearl::Rect::new(0., 0., CANVAS.0 as f64, CANVAS.1 as f64),
        frame: pearl::Insets {
            top: 50.,
            right: 10.,
            bottom: 10.,
            left: 10.,
        },
    };
    let tuning = pearl::Tuning::default();
    let start = pearl::Rect::new(10., 60., body.w as f64, body.h as f64);
    let mut drag = pearl::Drag::begin(
        pearl::Source::Docked,
        bead::core::Edge::Left,
        "A",
        start,
        pearl::Point { x: 100., y: 300. },
        std::slice::from_ref(&monitor),
        &tuning,
    );
    drag.update(
        pearl::Point {
            x: 100. + at,
            y: 330.,
        },
        std::slice::from_ref(&monitor),
        &tuning,
    );
    let landing = pearl::landing(bead::core::Edge::Left, &monitor);
    let guide = Guide {
        edge: bead::core::Edge::Left,
        output: (CANVAS.0 as i32, CANVAS.1 as i32),
        frame: [50., 10., 10., 10.],
        landing: Some((landing.origin.x, landing.origin.y)),
        film: drag.magnet.as_ref().map(|m| bead::wayland::Film {
            origin: (m.origin.x, m.origin.y),
            tip: (m.tip.x, m.tip.y),
            armed: m.armed,
        }),
    };
    let mut canvas = desktop(CANVAS, scale);
    let strip = strip(&guide);
    let scene = guide_scene(&guide, &ink);
    over(
        &mut canvas,
        &rasterize(&scene, (strip.w as u32, strip.h as u32), scale),
        (strip.w as u32, strip.h as u32),
        (strip.x, strip.y),
        scale,
    );
    over(
        &mut canvas,
        &rasterize(&panel, (534, 714), scale),
        (534, 714),
        (
            pearl_rect.x as f32 - panel.offset.0,
            pearl_rect.y as f32 - panel.offset.1,
        ),
        scale,
    );
    Ok(canvas)
}

#[test]
fn drag_guide_goldens() -> Result<()> {
    let _guard = serial();
    for scale in [1., 1.5] {
        // 140 px from the edge: the film stretches thin and is not yet armed.
        golden("drag-pulling", &dragging(scale, 140.)?, scale)?;
        // 40 px: inside the arming distance, the film thickens and brightens.
        golden("drag-armed", &dragging(scale, 40.)?, scale)?;
    }
    Ok(())
}

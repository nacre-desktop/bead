// SPDX-License-Identifier: GPL-3.0-only
//! B0216: measure product icon ink independently of background/hover colours.
use bead::render::{CpuRenderer, Damage, PixelFormat, Primitive, Rect, Renderer, ShmTarget};
use bead_test::{Harness, Result};

pub(crate) fn assert_buttons(h: &Harness, scale: f32, media: bool) -> Result<usize> {
    let mut checked = 0;
    for layer in &h.scene().layers {
        let interactive = h.session.view_nodes().iter().any(|n| n.id == layer.id);
        let transport = media
            && (layer.id.contains(".transport.")
                || layer.id.ends_with(".seek-back")
                || layer.id.ends_with(".seek-forward"));
        if !interactive && !transport {
            continue;
        }
        let primitives: Vec<_> = h
            .scene()
            .layers
            .iter()
            .filter(|child| child.id == layer.id || child.id.starts_with(&format!("{}.", layer.id)))
            .flat_map(|child| h.scene().primitives[child.own.clone()].iter())
            .filter(|p| matches!(p, Primitive::Text { .. } | Primitive::Image { .. }))
            .collect();
        let [Primitive::Text { family, .. }] = primitives.as_slice() else {
            continue;
        };
        if family != "Bead Test Symbols" && family != "Material Symbols Rounded" {
            continue;
        }
        let mut icon = (*primitives[0]).clone();
        let button = layer.bounds;
        let diagnostic = match &icon {
            Primitive::Text { text, rect, .. } => (text.clone(), *rect),
            _ => unreachable!(),
        };
        // Keep the authored pose, including its fractional physical-pixel phase.
        if let Primitive::Text { color, shadow, .. } = &mut icon {
            *color = [255; 4];
            *shadow = None;
        }
        let mut scene = h.scene().clone();
        scene.primitives = vec![icon];
        scene.layers.clear();
        scene.clips.clear();
        scene.prepared.clear();
        scene.motion = None;
        scene.slide = None;
        scene.offset = (0., 0.);
        scene.opacity = 1.;
        let width = ((scene.bounds.x + scene.bounds.w)
            .max(scene.width)
            .max(button.x + button.w + 32.)
            * scale)
            .ceil() as u32;
        let height = ((scene.bounds.y + scene.bounds.h).max(button.y + button.h + 32.) * scale)
            .ceil() as u32;
        scene.bounds = Rect::new(0., 0., width as f32 / scale, height as f32 / scale);
        let mut pixels = vec![0; (width * height * 4) as usize];
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, scale)?;
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )?;
        let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
        for y in 0..height {
            for x in 0..width {
                if pixels[((y * width + x) * 4 + 3) as usize] != 0 {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x);
                    bottom = bottom.max(y);
                }
            }
        }
        assert!(
            left <= right && top <= bottom,
            "{}: missing icon ink",
            layer.id
        );
        let x = (left + right + 1) as f32 / (2. * scale);
        let y = (top + bottom + 1) as f32 / (2. * scale);
        let cx = button.x + button.w / 2.;
        let cy = button.y + button.h / 2.;
        assert!(
            (x - cx).abs() <= 0.5 && (y - cy).abs() <= 0.5,
            "{}: ink centre ({x}, {y}) differs from shape centre ({cx}, {cy}), scale {scale}; text={:?}, box={:?}, button={:?}",
            layer.id,
            diagnostic.0,
            diagnostic.1,
            button
        );
        checked += 1;
    }
    Ok(checked)
}

#[test]
fn service_tray_navigation_icon_ink_is_centred() -> Result<()> {
    use bead::core::Value;
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };
    crate::i18n::init();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    for scale in [1., 1.5] {
        let mut h = Harness::load(
            &root,
            Path::new("tray/main.luau"),
            &root.join("palette.json"),
            BTreeMap::from([
                ("tray.title".into(), Value::Text("Application menu".into())),
                ("tray.rows".into(), Value::list(vec![])),
                ("tray.height".into(), Value::Number(96.)),
                ("tray.back".into(), Value::Bool(true)),
                ("tray.empty".into(), Value::Bool(true)),
            ]),
            bead_test::Config {
                width: 364,
                height: 160,
                scale,
            },
        )?;
        assert_eq!(assert_buttons(&h, scale, false)?, 2);
        let frame = h.render()?;
        let golden = root.join(format!("tests/goldens/tray-navigation-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-tray-centre-{scale}")),
        )?;
    }
    Ok(())
}

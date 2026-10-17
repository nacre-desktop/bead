// SPDX-License-Identifier: GPL-3.0-only
//! Placement-only masks. Luau supplies actual body sizes; Qt supplies the
//! independent contour/rim reference. Existing populated content goldens remain.
use super::*;
use bead::render::{Palette, PixelFormat, ShmTarget};
use bead_test::{Config, Frame as Pixels, Harness, Result, Tolerance};
use std::{collections::BTreeMap, path::Path};

fn tile(canvas: &mut Pixels, rect: Rect, svg: &[u8], scale: f32) -> Result<()> {
    let width = (rect.w * scale).ceil() as u32;
    let height = (rect.h * scale).ceil() as u32;
    let mut bytes = vec![0; width as usize * height as usize * 4];
    bead::render::svg_into(
        svg,
        ShmTarget {
            bytes: &mut bytes,
            width,
            height,
            scale,
            format: PixelFormat::Rgba,
        },
    )?;
    let (left, top) = (
        (rect.x * scale).round() as i32,
        (rect.y * scale).round() as i32,
    );
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let dx = x + left;
            let dy = y + top;
            if dx < 0 || dy < 0 || dx >= canvas.width as i32 || dy >= canvas.height as i32 {
                continue;
            }
            let source = ((y as u32 * width + x as u32) * 4) as usize;
            let target = ((dy as u32 * canvas.width + dx as u32) * 4) as usize;
            let a = bytes[source + 3] as u16;
            for channel in 0..3 {
                canvas.pixels[target + channel] = (bytes[source + channel] as u16
                    + (canvas.pixels[target + channel] as u16 * (255 - a) + 127) / 255)
                    .min(255) as u8;
            }
            canvas.pixels[target + 3] = 255;
        }
    }
    Ok(())
}
fn assert_interior(qml: &Pixels, bead: &Pixels, fill: [u8; 4], name: &str) -> Result<()> {
    let desktop = [0x34, 0x37, 0x3f, 255];
    let mut wrong = 0usize;
    for (i, (a, b)) in qml
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bead.pixels.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if a == b || (*a != fill && *a != desktop) {
            continue;
        }
        let x = i % qml.width as usize;
        let y = i / qml.width as usize;
        if x < 2 || y < 2 || x + 2 >= qml.width as usize || y + 2 >= qml.height as usize {
            continue;
        }
        // A 5x5 solid neighbourhood excludes antialiasing/stroke boundaries.
        // No amount of whole-output tolerance may hide a stale corner inside it.
        if (-2..=2).all(|dy| {
            (-2..=2).all(|dx| {
                let n = (i as isize + dy * qml.width as isize + dx) as usize;
                qml.pixels[n * 4..n * 4 + 4] == a[..]
            })
        }) {
            wrong += 1;
        }
    }
    if wrong > 0 {
        return Err(format!("{name}: {wrong} stable-interior pixels differ from QML").into());
    }
    Ok(())
}
#[test]
fn all_popout_and_launcher_placement_masks_match_qml() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = Palette::read(&root.join("palette.json"))?;
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../shell/tests/fixtures/placement-qt.json"
    ))?;
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let golden = root.join("tests/goldens/placement");
    let artifacts = std::env::temp_dir().join("bead-placement-pixel-diffs");
    for scale in [1.5] {
        for case in cases.as_array().ok_or("Invalid Qt reference")? {
            let name = case["name"].as_str().ok_or("Missing case name")?;
            let edge = case["edge"].as_str().ok_or("Missing edge")?;
            let entry = match name {
                "calendar" => "popouts/main.luau",
                "network" | "bluetooth" | "battery" => "status/main.luau",
                "sound" | "sound-left" => "sound/main.luau",
                _ => "ui/main.luau",
            };
            let fixture = if entry == "sound/main.luau" {
                crate::desktop::values(&bead::services::Snapshot::default(), "")
            } else {
                BTreeMap::new()
            };
            let mut h = Harness::load(
                &root,
                Path::new(entry),
                &root.join("palette.json"),
                fixture,
                Config {
                    width: 1920,
                    height: 1200,
                    scale,
                },
            )?;
            h.render()?;
            let actual = body(h.scene());
            // Boundary probes reuse the status body, not a launcher-sized mask.
            let size = if name.starts_with("corner-") || name == "left-clamp" {
                (310., 222.)
            } else {
                (actual.w, actual.h)
            };
            assert_eq!(
                size,
                (
                    case["width"].as_f64().ok_or("width")? as f32,
                    case["height"].as_f64().ok_or("height")? as f32
                ),
                "Actual Luau root size changed: {name}"
            );
            let attachment = if edge == "popout" {
                Attachment::Popout
            } else {
                Attachment::Attached(match edge {
                    "right" => Edge::Right,
                    "left" => Edge::Left,
                    "bottom" => Edge::Bottom,
                    _ => Edge::Top,
                })
            };
            let mut scene = crate::sound::fallback(palette.get("surface")?);
            scene.primitives = vec![bead::render::Primitive::Rounded {
                rect: Rect::new(0., 0., size.0, size.1),
                radius: 16.,
                fill: palette.get("surface")?,
                border: None,
            }];
            let mut chrome = Chrome::new(attachment, &palette)?;
            chrome.center = case["center"].as_f64().map(|n| n as f32);
            chrome.decorate(&mut scene)?;
            let position = Frame::default().positioner(attachment);
            let (x, y) = position(&scene, chrome.center, (1920, 1200));
            assert_eq!(
                x as f32 + scene.offset.0,
                case["x"].as_f64().ok_or("x")? as f32
            );
            assert_eq!(
                y as f32 + scene.offset.1,
                case["y"].as_f64().ok_or("y")? as f32
            );
            let width = (1920. * scale) as u32;
            let height = (1200. * scale) as u32;
            let mut pixels = Pixels {
                width,
                height,
                pixels: [0x34, 0x37, 0x3f, 255].repeat(width as usize * height as usize),
            };
            let fill = palette.get("surface")?;
            for pixel in pixels.pixels[..(width as f32 * 50. * scale) as usize * 4]
                .as_chunks_mut::<4>()
                .0
            {
                *pixel = fill;
            }
            let factory = Chrome::new(Attachment::Attached(Edge::Top), &palette)?.decorations();
            for decoration in factory(
                (1920, 1200),
                chrome.engaged,
                Some(bead::wayland::DecorationPanel {
                    scene: &scene,
                    origin: (x, y),
                }),
            )
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?
            {
                tile(&mut pixels, decoration.rect, &decoration.svg, scale)?;
            }
            for primitive in &scene.primitives {
                if let bead::render::Primitive::Image { rect, path, .. } = primitive {
                    tile(
                        &mut pixels,
                        rect.translated(x as f32 + scene.offset.0, y as f32 + scene.offset.1),
                        scene.asset_data.get(path).ok_or("Missing contour")?,
                        scale,
                    )?;
                }
            }
            let filename = format!("bead-{name}-{scale}.png");
            if update {
                pixels.save(&golden.join(&filename))?;
            }
            pixels.compare(
                &golden.join(&filename),
                Tolerance::default(),
                &artifacts.join(format!("self-{name}")),
            )?;
            pixels.compare(
                &golden.join(format!("qml-{name}-{scale}.png")),
                Tolerance {
                    channel: 4,
                    changed_fraction: 0.001,
                },
                &artifacts.join(format!("qml-{name}")),
            )?;
            let qml = Pixels::load(&golden.join(format!("qml-{name}-{scale}.png")))?;
            assert_interior(&qml, &pixels, fill, name)?;
            if update {
                let mut pair = Pixels {
                    width: width * 2,
                    height,
                    pixels: vec![0; width as usize * 2 * height as usize * 4],
                };
                for row in 0..height as usize {
                    let start = row * width as usize * 4;
                    let out = row * width as usize * 8;
                    pair.pixels[out..out + width as usize * 4]
                        .copy_from_slice(&qml.pixels[start..start + width as usize * 4]);
                    pair.pixels[out + width as usize * 4..out + width as usize * 8]
                        .copy_from_slice(&pixels.pixels[start..start + width as usize * 4]);
                }
                pair.save(&golden.join(format!("pair-{name}-{scale}.png")))?;
            }
        }
    }
    Ok(())
}

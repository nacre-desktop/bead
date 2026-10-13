// SPDX-License-Identifier: GPL-3.0-only
mod font_sources;
use bead_render::{Palette, Primitive, Rect, Scene, Slide};
use bead_wayland::{Options, run};
use std::path::{Path, PathBuf};
fn main() {
    if let Err(e) = start() {
        eprintln!("nacre-shell: {e}");
        std::process::exit(1);
    }
}
fn start() -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut profile = false;
    let mut options = Options::default();
    if let Ok(name) = std::env::var("BEAD_RENDERER") {
        select_renderer(&mut options, &name)?;
    }
    let mut palette = PathBuf::from("shell/palette.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile-scene" => {
                profile = true;
                options.panel = true;
            }
            "--renderer" => {
                select_renderer(&mut options, &args.next().ok_or("--renderer needs value")?)?
            }
            "--start-open" => options.start_open = true,
            "--content-size" => options.content_size = true,
            "--output" => options.output = Some(args.next().ok_or("--output needs value")?),
            "--namespace" => options.namespace = args.next().ok_or("--namespace needs value")?,
            "--panel" => options.panel = true,
            "--animate" => {
                options.animate = true;
                options.panel = true;
            }
            "--no-damage" => options.damage = false,
            "--argb" => options.force_argb = true,
            "--seconds" => {
                let s: f64 = args.next().ok_or("--seconds needs value")?.parse()?;
                if !s.is_finite() || s <= 0. {
                    return Err("seconds must be positive".into());
                }
                options.seconds = Some(s);
            }
            "--palette" => palette = args.next().ok_or("--palette needs path")?.into(),
            "--help" | "-h" => {
                println!(
                    "nacre-shell [--panel|--animate] [--renderer cpu|femtovg|vello|auto] [--content-size] [--start-open] [--output NAME] [--no-damage] [--argb] [--seconds N] [--palette PATH] [--namespace NAME]\nFeature 2 drawing preview. Click-through, no reserved space. Stop with Ctrl+C or SIGTERM."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let colors = Palette::read(&palette)?;
    let mut scene = if options.panel {
        panel(&colors, Path::new("shell"))?
    } else {
        Scene {
            opacity: 1.,
            font_files: Vec::new(),
            width: 20000.,
            slide: None,
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(0., 0., 20000., 32.),
                radius: 0.,
                fill: [32, 32, 32, 255],
                border: None,
            }],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 20000., 32.),
        }
    };
    scene.font_files = font_sources::select(&scene)?;
    println!(
        "PROFILE stage=cli_scene ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.
    );
    if profile {
        use bead_render::{CpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.5)?;
        let width = (scene.bounds.w * 1.5).ceil() as u32;
        let height = (scene.bounds.h * 1.5).ceil() as u32;
        let mut bytes = vec![0; width as usize * height as usize * 4];
        let t = std::time::Instant::now();
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut bytes,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )?;
        println!(
            "PROFILE stage=raster ms={:.3} total_ms={:.3}",
            t.elapsed().as_secs_f64() * 1000.,
            started.elapsed().as_secs_f64() * 1000.
        );
        return Ok(());
    }
    run(options, scene)
}
fn panel(p: &Palette, assets: &Path) -> Result<Scene, Box<dyn std::error::Error>> {
    let body = p.get("surface")?;
    let raised = p.get("surfaceContainer")?;
    let ink = p.get("onSurface")?;
    let muted = p.get("onSurfaceVariant")?;
    let accent = p.get("primary")?;
    let outline = p.get("outlineVariant")?;
    let mut v = vec![
        Primitive::Shadow {
            rect: Rect::new(-30., -30., 1160., 650.),
            radius: 12.,
            blur: 10.,
        },
        Primitive::Rounded {
            rect: Rect::new(0., 0., 1100., 590.),
            radius: 12.,
            fill: body,
            border: Some((outline, 1.)),
        },
        Primitive::Rounded {
            rect: Rect::new(182., 20., 898., 48.),
            radius: 13.,
            fill: raised,
            border: None,
        },
        Primitive::Rounded {
            rect: Rect::new(12., 84., 150., 40.),
            radius: 10.,
            fill: raised,
            border: None,
        },
    ];
    {
        let mut text = |x, y, w, h, s: &str, size, color| {
            v.push(Primitive::Text {
                rect: Rect::new(x, y, w, h),
                text: s.into(),
                family: "IBM Plex Sans".into(),
                size,
                color,
            })
        };
        text(202., 30., 860., 32., "Search applications…", 18., muted);
        for (i, label) in [
            "Home",
            "Applications",
            "System",
            "Settings",
            "Files",
            "Wallpaper",
        ]
        .iter()
        .enumerate()
        {
            text(
                28.,
                93. + i as f32 * 48.,
                120.,
                26.,
                label,
                16.,
                if i == 0 { accent } else { ink },
            );
        }
        text(202., 83., 860., 34., "Applications", 22., ink);
        for row in 0..9 {
            for col in 0..3 {
                let n = row * 3 + col + 1;
                let x = 202. + col as f32 * 290.;
                let y = 128. + row as f32 * 43.;
                text(
                    x,
                    y,
                    270.,
                    23.,
                    &format!("Application {n:02} — Open"),
                    16.,
                    ink,
                );
                text(
                    x,
                    y + 21.,
                    270.,
                    20.,
                    "System tools and everyday tasks",
                    12.,
                    muted,
                );
            }
        }
        text(
            202.,
            535.,
            820.,
            30.,
            "Font fallback: Ελληνικά · العربية · 日本語 · 😀 🌈 🚀",
            16.,
            ink,
        );
    }
    v.push(Primitive::Image {
        rect: Rect::new(28., 422., 48., 48.),
        path: assets.join("sample.svg"),
    });
    v.push(Primitive::Image {
        rect: Rect::new(96., 422., 48., 48.),
        path: assets.join("sample.png"),
    });
    let theme = if std::env::var_os("BEAD_ICON_PATH").is_none() {
        freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "hicolor".into())
    } else {
        "hicolor".into()
    };
    let icon = std::env::var_os("BEAD_ICON_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            [theme.as_str(), "hicolor", "Adwaita"]
                .into_iter()
                .find_map(|theme| {
                    freedesktop_icons::lookup("folder")
                        .with_size(72)
                        .with_theme(theme)
                        .with_cache()
                        .find()
                })
        });
    if let Some(path) = icon {
        println!("ICON folder resolved=true");
        v.push(Primitive::Image {
            rect: Rect::new(62., 490., 48., 48.),
            path,
        });
    } else {
        eprintln!("ICON folder unavailable in installed themes");
    }
    Ok(Scene {
        opacity: 1.,
        font_files: Vec::new(),
        width: 1100.,
        slide: Some(Slide {
            closed_y: -650.,
            open_y: 30.,
            period_seconds: 2.,
        }),
        primitives: v,
        offset: (0., 0.),
        bounds: Rect::new(-32., -32., 1164., 654.),
    })
}

fn select_renderer(options: &mut Options, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use bead_render::RendererKind;
    options.auto_fallback = name == "auto";
    options.renderer = match name {
        "cpu" => RendererKind::Cpu,
        "femtovg" => RendererKind::Femtovg,
        "vello" => RendererKind::Vello,
        "auto" => {
            if cfg!(feature = "gpu") {
                RendererKind::Femtovg
            } else {
                RendererKind::Cpu
            }
        }
        _ => return Err("renderer must be cpu, femtovg, vello or auto".into()),
    };
    Ok(())
}

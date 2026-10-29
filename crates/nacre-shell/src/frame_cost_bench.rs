// SPDX-License-Identifier: GPL-3.0-only
//! B0185: per-step CPU cost of one panel slide frame through the UI renderer.
//!
//! Run with `cargo test --release -p nacre-shell frame_cost_bench -- --ignored --nocapture`.
//! Each route's real shell/ entry is staged open, baked once like the panel worker,
//! then every 120 Hz frame of an open and a close slide applies the presenter's
//! pose (timed: `panel_offset`, including reveal artwork) and times the UI-thread steps: `prepare_on_ui`,
//! semantic damage and the CPU repaint of the owed regions (current + previous
//! frame, the steady state of the three-buffer SHM pool) at scale 1.5.

use bead::core::source::Source;
use bead::render::{
    Damage, PixelFormat, Renderer, Scene, ShmTarget, damage::FrameDamage, tree::TreeCpuRenderer,
};
use bead_test::{Config, Harness, source::FakeSource};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const SCALE: f32 = 1.5;
const RUNS: usize = 5;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

fn json(path: &str) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(root().join(path)).expect("fixture")).expect("json")
}

fn harness(entry: &str, models: BTreeMap<String, bead::core::Value>) -> Harness {
    let root = root();
    Harness::load(
        &root,
        Path::new(entry),
        &root.join("palette.json"),
        models,
        Config {
            width: 1920,
            height: 1200,
            scale: SCALE,
        },
    )
    .expect("shell entry loads")
}

fn open_scene(mut runtime: crate::reload::Runtime) -> Scene {
    runtime.session.stores = runtime
        .session
        .stores
        .reconcile(&BTreeMap::from([(
            "panel.open".into(),
            bead::core::Value::Bool(true),
        )]))
        .expect("panel.open");
    runtime.warm_frame().expect("open scene")
}

fn scenes() -> Vec<(&'static str, Scene)> {
    let colors = bead::render::Palette::read(&root().join("palette.json")).expect("palette");
    let source = |value: serde_json::Value| -> Arc<dyn Source> {
        Arc::new(FakeSource::new(value).expect("fake source"))
    };
    let dashboard =
        crate::reload::Runtime::new(harness("dashboard/today/main.luau", BTreeMap::new()).session)
            .with_dashboard_view(crate::dashboard::Dashboard::new(
                crate::dashboard::Page::Today,
                source(json("tests/fixtures/dashboard/calendar.json")),
                Some(source(json("tests/fixtures/dashboard/weather.json"))),
                true,
            ))
            .and_then(|r| {
                r.with_frame(
                    crate::placement::Attachment::Attached(bead::core::Edge::Top),
                    &colors,
                )
            })
            .expect("dashboard runtime")
            .with_panel_geometry(1920., 1200., 0.36)
            .expect("dashboard geometry");
    let launcher = crate::reload::Runtime::new(harness("ui/main.luau", BTreeMap::new()).session)
        .with_frame(
            crate::placement::Attachment::Attached(bead::core::Edge::Bottom),
            &colors,
        )
        .expect("launcher runtime");
    let unavailable = serde_json::json!({"health":{"available":false,"stale":false}});
    let control = crate::control_center::ControlCenter::new(
        None,
        vec![
            (
                "audio",
                source(json("tests/fixtures/dashboard-extras/audio.json")),
            ),
            (
                "network",
                source(json("tests/fixtures/dashboard-extras/network.json")),
            ),
            (
                "bluetooth",
                source(json("tests/fixtures/dashboard-extras/bluetooth.json")),
            ),
            ("controls", source(unavailable.clone())),
            ("vpn", source(unavailable.clone())),
            ("power.profiles", source(unavailable.clone())),
            ("displays", source(unavailable)),
        ],
        None,
        Vec::new(),
    );
    let control = crate::reload::Runtime::new(
        harness(
            "control/main.luau",
            BTreeMap::from([("cc.page".into(), bead::core::Value::Text("home".into()))]),
        )
        .session,
    )
    .with_control_center(control)
    .and_then(|r| r.with_frame(crate::placement::Attachment::ControlCenter, &colors))
    .expect("control runtime")
    .with_view_edge("right")
    .expect("control edge")
    .with_panel_geometry(1920., 1200., 0.28)
    .expect("control geometry");
    let calendar =
        crate::reload::Runtime::new(harness("popouts/main.luau", BTreeMap::new()).session)
            .with_frame(crate::placement::Attachment::Popout, &colors)
            .expect("calendar runtime");
    vec![
        ("dashboard", open_scene(dashboard)),
        ("launcher", open_scene(launcher)),
        ("control", open_scene(control)),
        ("calendar", open_scene(calendar)),
    ]
}

#[derive(Default)]
struct Steps {
    frames: usize,
    pose: Vec<f64>,
    prepare: Vec<f64>,
    damage: Vec<f64>,
    paint: Vec<f64>,
    pixels: Vec<f64>,
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}

fn p(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() as f64 * q).ceil() as usize).clamp(1, sorted.len()) - 1]
}

/// `Animations::value`'s curves (bead-core), sampled at normalized time `t`.
fn ease(curve: bead::core::animation::Curve, t: f32) -> f32 {
    use bead::core::animation::Curve;
    match curve {
        Curve::EaseOut => 1. - (1. - t).powi(3),
        Curve::EaseInOut if t < 0.5 => 4. * t.powi(3),
        Curve::EaseInOut => 1. - (-2. * t + 2.).powi(3) / 2.,
        Curve::EmphasizedAccelerate => t.powi(4),
        Curve::Spring => {
            let x = 8. * t;
            (1. - (1. + x) * (-x).exp()) / (1. - 9. * (-8_f32).exp())
        }
    }
}

/// One slide as `draw_current` poses it, timing each UI-thread step per frame.
fn slide(open_scene: &Scene, opening: bool) -> Steps {
    let motion = open_scene.motion.as_ref().expect("panel motion");
    let (duration, curve) = motion.transition(opening);
    let frames = (duration * 120.).round() as usize;
    let mut base = open_scene.clone();
    // The resource worker bakes the accepted open scene once (worker.rs `bake`).
    let mut cache = bead::render::tree::TreeCache::default();
    cache.prepare(&base, SCALE).expect("worker bake");
    base.prepared
        .insert(SCALE.to_bits(), cache.frame.clone().expect("baked frame"));
    let mut renderer = TreeCpuRenderer::default();
    let (w, h) = (
        (base.bounds.w * SCALE).ceil() as u32,
        (base.bounds.h * SCALE).ceil() as u32,
    );
    let mut bytes = vec![0u8; w as usize * h as usize * 4];
    let mut damage = FrameDamage::default();
    let mut previous: Vec<Damage> = Vec::new();
    let mut steps = Steps {
        frames,
        ..Default::default()
    };
    for frame in 0..=frames {
        let t = frame as f32 / frames.max(1) as f32;
        let eased = ease(curve, t);
        let progress = if opening { eased } else { 1. - eased };
        let mut scene = base.clone();
        let start = Instant::now();
        let prior_x = scene.offset.0;
        let motion = scene.motion.as_ref().unwrap().clone();
        scene.opacity = motion.opacity(progress);
        let (dx, dy) = scene.panel_offset(progress, opening);
        scene.offset = if !opening
            && matches!(
                motion.edge,
                bead::core::Edge::Top | bead::core::Edge::Bottom
            ) {
            (prior_x, 32. + dy)
        } else {
            (32. + dx, 32. + dy)
        };
        steps.pose.push(ms(start));
        let opaque = dx != 0. || dy != 0. || opening;
        renderer.presentation_motion(opaque);
        let start = Instant::now();
        renderer
            .prepare_on_ui(&scene, SCALE)
            .expect("prepare_on_ui");
        steps.prepare.push(ms(start));
        let start = Instant::now();
        let regions = match renderer.prepared_frame() {
            Some(prepared) => damage
                .update(&prepared, scene.offset, scene.opacity, w, h)
                .expect("damage"),
            None => vec![Damage::full(w, h)],
        };
        steps.damage.push(ms(start));
        let owed: Vec<Damage> = regions.iter().chain(previous.iter()).copied().collect();
        let start = Instant::now();
        for region in &owed {
            renderer
                .render(
                    &scene,
                    ShmTarget {
                        bytes: &mut bytes,
                        width: w,
                        height: h,
                        scale: SCALE,
                        format: PixelFormat::Rgba,
                    },
                    *region,
                )
                .expect("render");
        }
        steps.paint.push(ms(start));
        steps.pixels.push(
            owed.iter()
                .map(|d| f64::from(d.w) * f64::from(d.h))
                .sum::<f64>()
                / (f64::from(w) * f64::from(h)),
        );
        std::hint::black_box(&bytes);
        previous = regions;
    }
    steps
}

#[test]
#[ignore = "manual B0185 frame-cost benchmark; release build on a build host"]
fn frame_cost_bench() {
    crate::i18n::init();
    // B0202: building the routes again is what a panel reopen pays (new sessions).
    let started = Instant::now();
    drop(scenes());
    let first = started.elapsed().as_secs_f64() * 1e3;
    let started = Instant::now();
    let scenes = scenes();
    println!(
        "B0202_SETUP first_ms={first:.2} again_ms={:.2}",
        started.elapsed().as_secs_f64() * 1e3
    );
    for (name, scene) in scenes {
        for opening in [true, false] {
            let mut all = Steps::default();
            for _ in 0..RUNS {
                let steps = slide(&scene, opening);
                all.frames = steps.frames;
                // The first frame of a slide is reported separately: it may prepare.
                all.pose.extend(&steps.pose[1..]);
                all.prepare.extend(&steps.prepare[1..]);
                all.damage.extend(&steps.damage[1..]);
                all.paint.extend(&steps.paint[1..]);
                all.pixels.extend(&steps.pixels[1..]);
                println!(
                    "B0185_FIRST name={name} dir={} prepare_ms={:.3} paint_ms={:.3}",
                    if opening { "open" } else { "close" },
                    steps.prepare[0],
                    steps.paint[0]
                );
            }
            let total: Vec<f64> = (0..all.paint.len())
                .map(|i| all.pose[i] + all.prepare[i] + all.damage[i] + all.paint[i])
                .collect();
            println!(
                "B0185_FRAME name={name} dir={} prims={} surface={}x{} frames={} pose_p50={:.3} prepare_p50={:.3} damage_p50={:.3} paint_p50={:.3} paint_p95={:.3} owed_share_p50={:.2} total_p50={:.3} total_p95={:.3} total_max={:.3}",
                if opening { "open" } else { "close" },
                scene.primitives.len(),
                (scene.bounds.w * SCALE).ceil(),
                (scene.bounds.h * SCALE).ceil(),
                all.frames,
                p(&all.pose, 0.5),
                p(&all.prepare, 0.5),
                p(&all.damage, 0.5),
                p(&all.paint, 0.5),
                p(&all.paint, 0.95),
                p(&all.pixels, 0.5),
                p(&total, 0.5),
                p(&total, 0.95),
                p(&total, 1.),
            );
        }
    }
}

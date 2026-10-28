// SPDX-License-Identifier: GPL-3.0-only
//! Manual CPU benchmark for B0177's prepared reveal frames.
//!
//! Run with `cargo test -p nacre-shell b0177_cpu_bench -- --ignored --nocapture`.
//! The open scene is prepared and rendered before the timed loop. Each timed
//! sample applies the production reveal pose, then times the renderer's UI
//! preparation and CPU composition separately at monitor pixel dimensions.

use bead::core::source::Source;
use bead::render::{Damage, PixelFormat, Renderer, ShmTarget, tree::TreeCpuRenderer};
use bead_test::{Config, Harness, source::FakeSource};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

const LOGICAL_WIDTH: u32 = 1920;
const LOGICAL_HEIGHT: u32 = 1200;
const SCALE: f32 = 1.5;
const SURFACE_WIDTH: u32 = 2880;
const SURFACE_HEIGHT: u32 = 1800;
const WARMUPS: usize = 24;
const ITERATIONS: usize = 240;

fn shell_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}

fn dashboard_scene() -> Result<bead::render::Scene, Box<dyn std::error::Error>> {
    let root = shell_root();
    let palette_path = root.join("palette.json");
    let harness = Harness::load(
        &root,
        Path::new("dashboard/today/main.luau"),
        &palette_path,
        BTreeMap::new(),
        Config {
            width: LOGICAL_WIDTH,
            height: LOGICAL_HEIGHT,
            scale: SCALE,
        },
    )?;
    let calendar: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("tests/fixtures/dashboard/calendar.json"),
    )?)?;
    let weather: serde_json::Value = serde_json::from_slice(&std::fs::read(
        root.join("tests/fixtures/dashboard/weather.json"),
    )?)?;
    let calendar_source: std::sync::Arc<dyn Source> =
        std::sync::Arc::new(FakeSource::new(calendar)?);
    let weather_source: std::sync::Arc<dyn Source> =
        std::sync::Arc::new(FakeSource::new(weather)?);
    let colors = bead::render::Palette::read(&palette_path)?;
    let mut runtime = crate::reload::Runtime::new(harness.session)
        .with_dashboard_view(crate::dashboard::Dashboard::new(
            crate::dashboard::Page::Today,
            calendar_source,
            Some(weather_source),
            true,
        ))?
        .with_frame(
            crate::placement::Attachment::Attached(bead::core::Edge::Top),
            &colors,
        )?
        .with_panel_geometry(f64::from(LOGICAL_WIDTH), f64::from(LOGICAL_HEIGHT), 0.36)?;
    runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([(
        "panel.open".into(),
        bead::core::Value::Bool(true),
    )]))?;
    let scene = runtime.warm_frame()?;
    assert!(
        scene.motion.as_ref().is_some_and(|motion| motion.reveal),
        "production dashboard Today setup must produce a revealing panel scene"
    );
    Ok(scene)
}

fn fixture(root: &Path, name: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(
        root.join(format!("tests/fixtures/dashboard-extras/{name}.json")),
    )?)?)
}

fn control_center_scene() -> Result<bead::render::Scene, Box<dyn std::error::Error>> {
    let root = shell_root();
    let palette_path = root.join("palette.json");
    let harness = Harness::load(
        &root,
        Path::new("control/main.luau"),
        &palette_path,
        BTreeMap::from([(
            "cc.page".into(),
            bead::core::Value::Text("home".into()),
        )]),
        Config {
            width: LOGICAL_WIDTH,
            height: LOGICAL_HEIGHT,
            scale: SCALE,
        },
    )?;

    // The Home page's visible tiles use the same published fixture shapes as
    // the control-center source tests. Unused sources remain explicitly offline.
    let unavailable = serde_json::json!({"health":{"available":false,"stale":false}});
    let topic_data = [
        ("audio", fixture(&root, "audio")?),
        ("controls", unavailable.clone()),
        ("network", fixture(&root, "network")?),
        ("bluetooth", fixture(&root, "bluetooth")?),
        ("vpn", unavailable.clone()),
        ("power.profiles", unavailable.clone()),
        (
            "battery",
            serde_json::json!({"health":{"available":true,"stale":false},"percentage":78.0,"charging":false,"state":2,"time_to_empty_seconds":7380,"on_battery":true}),
        ),
        (
            "notifications",
            serde_json::from_slice(&std::fs::read(
                root.join("tests/fixtures/dashboard-extras/notifications.json"),
            )?)?,
        ),
        ("displays", unavailable),
    ];
    let topics = topic_data
        .into_iter()
        .map(|(topic, data)| {
            Ok((
                topic,
                std::sync::Arc::new(FakeSource::new(data)?)
                    as std::sync::Arc<dyn bead::core::source::Source>,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let stored = std::sync::Arc::new(FakeSource::new(serde_json::json!({
        "store":"control-center",
        "rev":3,
        "exists":true,
        "data":{"version":2,"defaultsRevision":2,"tiles":["volume","network","dnd"]},
        "dirty":false
    }))?);
    let control = crate::control_center::ControlCenter::new(
        None,
        topics,
        Some(stored),
        [
            "network",
            "bluetooth",
            "battery",
            "session",
            "dashboard-clipboard",
            "dashboard-system",
            "dashboard-notifications",
        ]
        .map(str::to_owned)
        .to_vec(),
    );
    let colors = bead::render::Palette::read(&palette_path)?;
    let mut runtime = crate::reload::Runtime::new(harness.session)
        .with_control_center(control)?
        .with_frame(crate::placement::Attachment::ControlCenter, &colors)?
        .with_view_edge("right")?
        .with_panel_geometry(f64::from(LOGICAL_WIDTH), f64::from(LOGICAL_HEIGHT), 0.28)?;
    runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([(
        "panel.open".into(),
        bead::core::Value::Bool(true),
    )]))?;
    Ok(runtime.warm_frame()?)
}

fn quantile(sorted_ns: &[u128], percentile: usize) -> u128 {
    let rank = (sorted_ns.len() * percentile).div_ceil(100).max(1);
    sorted_ns[rank - 1]
}

fn summarize(samples: &[u128]) -> (u128, u128, u128, u128) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let mean = samples.iter().sum::<u128>() / samples.len() as u128;
    (
        quantile(&sorted, 50),
        quantile(&sorted, 95),
        quantile(&sorted, 99),
        mean,
    )
}

fn benchmark(scene_name: &str, mut open: bead::render::Scene) -> bead::render::Result<()> {
    let motion = open
        .motion
        .as_mut()
        .expect("shell panel UI must provide a reveal motion");
    assert!(motion.reveal, "benchmark requires the clipped reveal pose");
    motion.open = true;
    motion.progress = 1.;
    open.opacity = 1.;
    open.reveal(1.);

    // Model the production resource worker: prepare the complete open scene,
    // attach its immutable reveal artifact, then hand the prepared frame to the
    // UI renderer before any samples. The attach call is candidate-only; omit
    // that one line when overlaying this harness on baseline 4756b652.
    let worker_scene = open.clone();
    let mut renderer = std::thread::spawn(move || {
        let mut worker = TreeCpuRenderer::default();
        worker
            .prepare(&worker_scene, SCALE)
            .map_err(|error| error.to_string())?;
        // Candidate-only attach: baseline has no prepared reveal artifact API.
        worker
            .cache
            .attach_reveal_artifact(&worker_scene, SCALE)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(worker)
    })
    .join()
    .map_err(|_| "prepared reveal worker panicked")?
    .map_err(std::io::Error::other)?;
    open.prepared.insert(
        SCALE.to_bits(),
        renderer
            .prepared_frame()
            .expect("worker renderer must publish its prepared frame"),
    );
    let artifact_bytes = open
        .prepared
        .get(&SCALE.to_bits())
        .and_then(|frame| frame.reveal_artifact.as_ref())
        .map_or(0, |artifact| artifact.retained_bytes());
    let mut pixels = vec![0; SURFACE_WIDTH as usize * SURFACE_HEIGHT as usize * 4];
    let full_damage = Damage::full(SURFACE_WIDTH, SURFACE_HEIGHT);

    // Paint the complete worker-baked open frame before any timed sample.
    renderer.render(
        &open,
        ShmTarget {
            bytes: &mut pixels,
            width: SURFACE_WIDTH,
            height: SURFACE_HEIGHT,
            scale: SCALE,
            format: PixelFormat::Rgba,
        },
        full_damage,
    )?;
    std::hint::black_box(&pixels);

    let mut prepare_samples = Vec::with_capacity(ITERATIONS);
    let mut render_samples = Vec::with_capacity(ITERATIONS);
    for iteration in 0..WARMUPS + ITERATIONS {
        let progress = ((iteration % 119) + 1) as f32 / 120.;
        let mut pose = open.clone();
        if let Some(motion) = pose.motion.as_mut() {
            motion.progress = progress;
        }
        pose.reveal(progress);

        let prepare_start = Instant::now();
        renderer.prepare_on_ui(&pose, SCALE)?;
        let prepare_ns = prepare_start.elapsed().as_nanos();

        let render_start = Instant::now();
        renderer.render(
            &pose,
            ShmTarget {
                bytes: &mut pixels,
                width: SURFACE_WIDTH,
                height: SURFACE_HEIGHT,
                scale: SCALE,
                format: PixelFormat::Rgba,
            },
            full_damage,
        )?;
        let render_ns = render_start.elapsed().as_nanos();
        std::hint::black_box(&pixels);
        if iteration >= WARMUPS {
            prepare_samples.push(prepare_ns);
            render_samples.push(render_ns);
        }
    }

    let (prep50, prep95, prep99, prep_mean) = summarize(&prepare_samples);
    let (draw50, draw95, draw99, draw_mean) = summarize(&render_samples);
    println!(
        "B0177_CPU_BENCH scene={scene_name} logical={LOGICAL_WIDTH}x{LOGICAL_HEIGHT} surface_px={SURFACE_WIDTH}x{SURFACE_HEIGHT} scale={SCALE:.1} backend=bead-render-cpu iterations={ITERATIONS} warmups={WARMUPS} artifact_bytes={artifact_bytes} prepare_p50_ns={prep50} prepare_p95_ns={prep95} prepare_p99_ns={prep99} prepare_mean_ns={prep_mean} render_p50_ns={draw50} render_p95_ns={draw95} render_p99_ns={draw99} render_mean_ns={draw_mean}"
    );
    Ok(())
}

#[test]
#[ignore = "manual B0177 CPU benchmark; run on the development host"]
fn b0177_dashboard_today_cpu_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    benchmark("dashboard_today", dashboard_scene()?)?;
    Ok(())
}

#[test]
#[ignore = "manual B0177 CPU benchmark; run on the development host"]
fn b0177_control_center_home_cpu_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    benchmark("control_center_home", control_center_scene()?)?;
    Ok(())
}

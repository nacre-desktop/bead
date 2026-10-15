// SPDX-License-Identifier: MIT
use bead_core::recovery::{Bundle, Run, Watchdog};
use bead_test::{Config, Harness, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "bead-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        std::fs::create_dir(path.join("source"))?;
        for name in ["main.luau", "behavior.luau", "palette.json"] {
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(name),
                path.join("source").join(name),
            )?;
        }
        Ok(Self(path))
    }
    fn load(&self) -> Result<Harness> {
        Harness::load_with_recovery(
            &self.0.join("source"),
            Path::new("main.luau"),
            &self.0.join("source/palette.json"),
            BTreeMap::new(),
            Config {
                width: 384,
                height: 244,
                scale: 1.,
            },
            self.0.join("state/good.json"),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn startup_last_good_requires_first_frame_and_retains_original_reload_target() -> Result<()> {
    let f = Fixture::new()?;
    let good = f.0.join("state/good.json");
    let original = std::fs::read(f.0.join("source/main.luau"))?;
    let palette = std::fs::read(f.0.join("source/palette.json"))?;
    let mut ui = f.load()?;
    assert!(!good.exists());
    ui.session.presented(u64::MAX)?;
    assert!(!good.exists());
    let before = ui.render()?;
    ui.presented()?;
    let saved = std::fs::read(&good)?;
    ui.update(BTreeMap::from([(
        "query".into(),
        bead_core::Value::Text("private query never saved".into()),
    )]))?;
    ui.presented()?;
    assert!(
        std::fs::read(&good)? == saved,
        "view state must not change saved sources"
    );
    assert!(!String::from_utf8_lossy(&saved).contains("private query never saved"));
    drop(ui);
    std::fs::write(f.0.join("source/main.luau"), "invalid Luau !!!!")?;
    std::fs::write(f.0.join("source/palette.json"), "invalid palette")?;
    let mut recovered = f.load()?;
    assert!(recovered.session.recovered_on_start());
    assert!(
        before.pixels == recovered.render()?.pixels,
        "last-good pixels differ"
    );
    let stale_revision = recovered.scene().resource_revision;
    std::fs::write(f.0.join("source/main.luau"), original)?;
    std::fs::write(f.0.join("source/palette.json"), palette)?;
    recovered.reload()?;
    assert!(!recovered.session.recovered_on_start());
    assert!(
        before.pixels == recovered.render()?.pixels,
        "last-good pixels differ"
    );
    let unchanged = std::fs::read(&good)?;
    recovered.session.presented(stale_revision)?;
    assert!(
        std::fs::read(&good)? == unchanged,
        "stale presentation saved an unpresented generation"
    );
    recovered.presented()?;
    let old = std::fs::read(&good)?;
    std::fs::write(f.0.join("source/main.luau"), "invalid again !!!")?;
    assert!(recovered.reload().is_err());
    recovered.presented()?;
    assert!(
        std::fs::read(&good)? == old,
        "last-good bundle changed after rejected reload"
    );
    Ok(())
}
#[test]
fn unpresented_invalid_bundle_and_path_escape_never_replace_last_good() -> Result<()> {
    let f = Fixture::new()?;
    let ui = f.load()?;
    drop(ui);
    std::fs::write(f.0.join("source/main.luau"), "bad !!!")?;
    assert!(
        f.load().is_err(),
        "unpresented startup must not create a last-good bundle"
    );
    let bundle = Bundle {
        version: 1,
        entry: "../escape.luau".into(),
        palette: vec![],
        files: BTreeMap::from([("../escape.luau".into(), vec![1])]),
    };
    assert!(bundle.save(&f.0.join("state/good.json")).is_err());
    assert!(!f.0.parent().ok_or("parent")?.join("escape.luau").exists());
    let mut history = Run::begin(f.0.join("state/health.json"), 1000)?;
    history.finish()?;
    for now in [2000, 3000, 4000] {
        let run = Run::begin(f.0.join("state/health.json"), now)?;
        assert!(!run.safe_mode);
    }
    let mut run = Run::begin(f.0.join("state/health.json"), 5000)?;
    assert!(run.safe_mode);
    run.finish()?;
    assert!(!Run::begin(f.0.join("state/health.json"), 200000)?.safe_mode);
    // Pure boundary policy is deterministic; the real abort path is tested below.
    assert!(!bead_core::recovery::stalled(
        1,
        5_000_000_001,
        5_000_000_000
    ));
    assert!(bead_core::recovery::stalled(
        1,
        5_000_000_002,
        5_000_000_000
    ));
    Ok(())
}
#[test]
#[ignore = "subprocess entry only"]
fn watchdog_child() {
    if std::env::var("BEAD_WATCHDOG_CHILD").as_deref() != Ok("1") {
        return;
    }
    let _watch = Watchdog::start(Duration::from_millis(80)).expect("watchdog starts");
    std::thread::sleep(Duration::from_secs(10));
    panic!("watchdog failed to abort");
}
#[test]
fn watchdog_aborts_stalled_process_and_healthy_stop_is_prompt() -> Result<()> {
    use std::os::unix::process::ExitStatusExt;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "watchdog_child", "--ignored", "--nocapture"])
        .env("BEAD_WATCHDOG_CHILD", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() > until {
            let _ = child.kill();
            let _ = child.wait();
            return Err("watchdog child did not terminate".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert_eq!(output.status.signal(), Some(6));
    let report = String::from_utf8(output.stderr)?;
    assert!(report.contains("WATCHDOG code=main_loop_timeout"));
    assert!(report.len() < 256);
    let watch = Watchdog::start(Duration::from_millis(200))?;
    for _ in 0..5 {
        watch.beat();
        std::thread::sleep(Duration::from_millis(10));
    }
    let stopped = Instant::now();
    drop(watch);
    assert!(stopped.elapsed() < Duration::from_secs(1));
    Ok(())
}
#[test]
fn safe_mode_bar_is_a_headless_cpu_scene_without_script_or_layout() -> Result<()> {
    use bead_render::{Damage, PixelFormat, Renderer, ShmTarget};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let palette = bead_render::Palette::read(&root.join("tests/fixtures/palette.json"))?;
    let before = bead_ui::layout::layout_passes();
    let mut scene = bead_ui::recovery::safe_bar(&palette);
    assert!(scene.motion.is_none());
    assert_eq!(scene.primitives.len(), 2);
    scene.font_files = vec![root.join("fonts/IBMPlexSans.ttf")];
    for primitive in &mut scene.primitives {
        if let bead_render::Primitive::Text { family, .. } = primitive {
            *family = "IBM Plex Sans".into();
        }
    }
    let mut renderer = bead_render::tree::TreeCpuRenderer::default();
    renderer.prepare(&scene, 1.)?;
    let mut bytes = vec![0; 1200 * 50 * 4];
    renderer.render(
        &scene,
        ShmTarget {
            bytes: &mut bytes,
            width: 1200,
            height: 50,
            scale: 1.,
            format: PixelFormat::Rgba,
        },
        Damage::full(1200, 50),
    )?;
    assert!(bytes.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    assert_eq!(bead_ui::layout::layout_passes(), before);
    let frame = bead_test::Frame {
        width: 1200,
        height: 50,
        pixels: bytes,
    };
    let golden = root.join("tests/goldens/safe-bar.png");
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&golden)?;
    }
    frame.compare(
        &golden,
        bead_test::Tolerance::default(),
        &std::env::temp_dir().join("bead-safe-bar-diff"),
    )?;
    Ok(())
}
#[test]
fn late_presentation_saves_its_own_validated_bundle_and_never_rolls_back() -> Result<()> {
    let f = Fixture::new()?;
    let mut ui = f.load()?;
    let revision = ui.scene().resource_revision;
    let source = f.0.join("source/main.luau");
    let original = std::fs::read(&source)?;
    let changed =
        String::from_utf8(original.clone())?.replace("Bead foundation", "Next generation");
    std::fs::write(&source, &changed)?;
    ui.reload()?;
    let newer = ui.scene().resource_revision;
    assert!(newer > revision);
    let saved = f.0.join("state/good.json");
    ui.session.presented(revision)?;
    assert!(
        Bundle::load(&saved)?.files[Path::new("main.luau")] == original,
        "late first frame must save its own sources"
    );
    ui.session.presented(newer)?;
    assert!(Bundle::load(&saved)?.files[Path::new("main.luau")] == changed.as_bytes());
    ui.session.presented(revision)?;
    assert!(
        Bundle::load(&saved)?.files[Path::new("main.luau")] == changed.as_bytes(),
        "old feedback rolled back last good"
    );
    Ok(())
}
#[test]
fn pending_capture_budget_rejects_reload_until_presentation_and_counter_is_exclusive() -> Result<()>
{
    let f = Fixture::new()?;
    let mut ui = f.load()?;
    let source = f.0.join("source/main.luau");
    let original = std::fs::read_to_string(&source)?;
    for i in 0..31 {
        std::fs::write(
            &source,
            original.replace("Bead foundation", &format!("Generation {i}")),
        )?;
        ui.reload()?;
    }
    std::fs::write(&source, original.replace("Bead foundation", "Budget full"))?;
    assert!(
        ui.reload().is_err(),
        "unpresented source snapshots must have a finite budget"
    );
    ui.session.presented(ui.scene().resource_revision)?;
    ui.reload()?;
    let path = f.0.join("state/exclusive.json");
    let mut owner = Run::begin(path.clone(), 1000)?;
    assert!(Run::begin(path.clone(), 1001).is_err());
    owner.finish()?;
    let mut next = Run::begin(path, 1002)?;
    assert!(!next.safe_mode);
    next.finish()?;
    Ok(())
}

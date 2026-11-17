// SPDX-License-Identifier: MIT
//! Private-compositor proof host for the wallpaper crossfade: a generated wallpaper that changes
//! every `BEAD_FADE_INTERVAL_MS` (default 3000) with alternating palettes; `BEAD_FADE_KIND=grow`
//! grows the new image from the output centre. `BEAD_FADE_WARM=<n>` runs warm mode (B0317) for n
//! changes (`BEAD_FADE_RETARGET_MS=<ms>` adds a second change that long after each warm one) between `BEAD_FADE_COLD` (default 3) cold ones, `BEAD_FADE_CYCLES` (default 1) times,
//! and prints `PROOF_MEM` (RSS and anonymous memory) before every change and after each switch.
use bead_wayland::{
    Options, ShellControl, WallpaperFade, WallpaperFadeKind, WallpaperFrame, WallpaperProvider, run,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[derive(Default)]
struct Provider {
    /// The committed revision per output: every output of a change fades.
    revision: Mutex<BTreeMap<String, u64>>,
    changed: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}
impl Provider {
    fn next(&self, output: &str) -> u64 {
        self.revision
            .lock()
            .unwrap()
            .get(output)
            .copied()
            .unwrap_or(0)
            + 1
    }
    fn image(&self, width: u32, height: u32, revision: u64) -> Vec<u8> {
        let tint = [[255u8, 60, 40], [30, 90, 255]][(revision % 2) as usize];
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let (u, v) = (x * 255 / width, y * 255 / height);
                rgba.extend([
                    (u * u32::from(tint[0]) / 255) as u8,
                    (v * u32::from(tint[1]) / 255) as u8,
                    ((u + v) / 2 * u32::from(tint[2]) / 255) as u8,
                    255,
                ]);
            }
        }
        rgba
    }
}
impl WallpaperProvider for Provider {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String> {
        *self.changed.lock().unwrap() = Some(changed);
        Ok(())
    }
    fn prepare(&self, output: String, width: u32, height: u32) -> Result<WallpaperFrame, String> {
        let revision = self.next(&output);
        let rgba = self.image(width, height, revision);
        let colour = [[200u8, 80, 60, 255], [60, 100, 220, 255]][(revision % 2) as usize];
        Ok(WallpaperFrame {
            output,
            revision,
            width,
            height,
            rgba,
            previous_rgba: None,
            palette: bead_render::Palette(Arc::new([("accent".to_owned(), colour)].into())),
            commit_token: Arc::new(()),
        })
    }
    fn is_current(&self, frame: &WallpaperFrame) -> bool {
        frame.revision == self.next(&frame.output)
    }
    fn committed(&self, output: &str, revision: u64, _: &Arc<dyn std::any::Any + Send + Sync>) {
        self.revision
            .lock()
            .unwrap()
            .insert(output.to_owned(), revision);
        println!("PROOF_COMMITTED output={output} revision={revision}");
    }
    fn discarded(&self, _: &str, _: u64) {}
    fn output_removed(&self, _: &str) {}
}
/// The warm switch of the proof host, flipped by the schedule below.
#[derive(Default)]
struct Warm {
    on: AtomicBool,
    changed: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}
impl ShellControl for Warm {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) {
        *self.changed.lock().unwrap() = Some(changed);
    }
    fn wallpaper_warm(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }
}
fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
/// RSS and anonymous memory of this process.
fn proof_mem(label: &str) {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let kb = |key: &str| {
        rollup
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("0")
            .to_owned()
    };
    println!(
        "PROOF_MEM {label} rss_kb={} anon_kb={}",
        kb("Rss:"),
        kb("Anonymous:")
    );
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(Provider::default());
    let every = std::env::var("BEAD_FADE_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let ticker = provider.clone();
    let warm_changes = env_number("BEAD_FADE_WARM", 0);
    let warm = Arc::new(Warm::default());
    let switch = warm.clone();
    let (cold, cycles) = (
        env_number("BEAD_FADE_COLD", 3),
        env_number("BEAD_FADE_CYCLES", 1),
    );
    std::thread::spawn(move || {
        // Per cycle: `cold` changes with warm off, then `warm_changes` with it on; one more cold run.
        let period = (cold + warm_changes).max(1);
        for change in 0..=usize::MAX {
            std::thread::sleep(std::time::Duration::from_millis(every));
            let want = warm_changes > 0 && change < cycles * period && change % period >= cold;
            proof_mem(&format!("change={change} warm={}", u8::from(want)));
            if want != switch.on.swap(want, Ordering::SeqCst)
                && let Some(changed) = &*switch.changed.lock().unwrap()
            {
                changed();
                std::thread::sleep(std::time::Duration::from_millis(600));
                proof_mem(&format!("change={change} switched={}", u8::from(want)));
            }
            if let Some(changed) = &*ticker.changed.lock().unwrap() {
                changed();
                // B0325: a second frame mid-fade, a retarget of the run the first one started.
                let retarget = env_number("BEAD_FADE_RETARGET_MS", 0);
                if want && retarget > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(retarget as u64));
                    changed();
                }
            }
        }
    });
    let mut options = Options::default();
    options.control = (warm_changes > 0).then_some(warm as Arc<dyn ShellControl>);
    options.wallpaper = Some(provider);
    options.wallpaper_fade = Some(WallpaperFade {
        kind: if std::env::var("BEAD_FADE_KIND").as_deref() == Ok("grow") {
            WallpaperFadeKind::Grow { origin: (0.5, 0.5) }
        } else {
            WallpaperFadeKind::Fade
        },
        ..WallpaperFade::default()
    });
    run(options, bead_render::Scene::empty(100., 32.))
}

// SPDX-License-Identifier: MIT
//! Private-compositor proof host for the wallpaper crossfade: a generated wallpaper that changes
//! every `BEAD_FADE_INTERVAL_MS` (default 3000) with alternating palettes; `BEAD_FADE_KIND=grow`
//! grows the new image from the output centre.
use bead_wayland::{
    Options, WallpaperFade, WallpaperFadeKind, WallpaperFrame, WallpaperProvider, run,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
#[derive(Default)]
struct Provider {
    revision: AtomicU64,
    changed: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}
impl Provider {
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
        let revision = self.revision.load(Ordering::SeqCst) + 1;
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
        frame.revision == self.revision.load(Ordering::SeqCst) + 1
    }
    fn committed(&self, output: &str, revision: u64, _: &Arc<dyn std::any::Any + Send + Sync>) {
        self.revision.store(revision, Ordering::SeqCst);
        println!("PROOF_COMMITTED output={output} revision={revision}");
    }
    fn discarded(&self, _: &str, _: u64) {}
    fn output_removed(&self, _: &str) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(Provider::default());
    let every = std::env::var("BEAD_FADE_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let ticker = provider.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(every));
            if let Some(changed) = &*ticker.changed.lock().unwrap() {
                changed();
            }
        }
    });
    let mut options = Options::default();
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

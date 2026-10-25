// SPDX-License-Identifier: GPL-3.0-only
//! Idle launcher icon raster warming, owned by the persistent bar runtime.
use bead::{
    core::{Stores, source::Source},
    render::{ImageFit, Palette, Primitive, Rect},
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};

const SAMPLE_ICON: &[u8] = include_bytes!("../../../shell/sample.svg");
const ICON_LIMIT: usize = 64;

fn notify(sender: &SyncSender<()>) {
    match sender.try_send(()) {
        Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
    }
}

pub(crate) struct IconWarmer {
    source: Arc<dyn Source>,
    palette_path: PathBuf,
    scale: Arc<AtomicU32>,
    started: bool,
    stop: Arc<AtomicBool>,
    wake: Option<SyncSender<()>>,
    join: Option<thread::JoinHandle<()>>,
}

impl IconWarmer {
    pub(crate) fn new(source: Arc<dyn Source>, palette_path: PathBuf) -> Self {
        Self {
            source,
            palette_path,
            scale: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            started: false,
            stop: Arc::new(AtomicBool::new(false)),
            wake: None,
            join: None,
        }
    }

    pub(crate) fn set_scale(&self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            let bits = scale.to_bits();
            if self.scale.swap(bits, Ordering::Relaxed) != bits
                && let Some(wake) = &self.wake
            {
                notify(wake);
            }
        }
    }

    pub(crate) fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let (wake, receiver) = mpsc::sync_channel(1);
        self.wake = Some(wake.clone());
        let source = self.source.clone();
        let palette_path = self.palette_path.clone();
        let scale = self.scale.clone();
        let stop = self.stop.clone();
        self.join = thread::Builder::new()
            .name("bead-icon-warm".into())
            .spawn(move || {
                // Lower only this worker's scheduling priority; failures leave the shell functional.
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 10);
                }
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let Ok(subscription) = source.subscribe(Arc::new({
                    let wake = wake.clone();
                    move |_| notify(&wake)
                })) else {
                    return;
                };
                notify(&wake);
                while !stop.load(Ordering::Relaxed) {
                    match receiver.recv() {
                        Ok(()) => {}
                        Err(_) => break,
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    warm_pages(
                        &runtime,
                        source.as_ref(),
                        &palette_path,
                        f32::from_bits(scale.load(Ordering::Relaxed)),
                        &stop,
                    );
                }
                drop(subscription);
            })
            .ok();
    }
}

impl Drop for IconWarmer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(wake) = &self.wake {
            notify(wake);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn warm_pages(
    runtime: &tokio::runtime::Runtime,
    source: &dyn Source,
    palette_path: &std::path::Path,
    scale: f32,
    stop: &AtomicBool,
) {
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let mut query_errors = 0;
    let preferences = runtime
        .block_on(source.call(
            "menu.preferences".into(),
            json!({"action":"load"}),
            Duration::from_millis(250),
        ))
        .and_then(|value| crate::launcher::recent_ids(&value));
    let recent = preferences.unwrap_or_else(|_| {
        query_errors += 1;
        Vec::new()
    });
    let mut pages = Vec::new();
    for request in [
        json!({"query":"","mode":"apps","route":"home","limit":32}),
        json!({"query":"","mode":"apps","limit":if recent.is_empty(){64}else{nacre_protocol::menu::MAX_ITEMS}}),
    ] {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let reply =
            runtime.block_on(source.call("menu.query".into(), request, Duration::from_millis(250)));
        let Ok(reply) = reply else {
            query_errors += 1;
            pages.push(Vec::new());
            continue;
        };
        let Ok(rows) = crate::launcher::rows(reply) else {
            query_errors += 1;
            pages.push(Vec::new());
            continue;
        };
        pages.push(rows);
    }
    let icons = likely_icons(&pages[0], &pages[1], &recent);
    drop(pages);
    let Ok(sizes) = settings_sizes(std::path::Path::new("shell"), palette_path) else {
        bead::core::vprintln!("ICON_WARM error=ui_settings");
        return;
    };
    let Some(grid) = sizes.first().map(|rect| Rect::new(0., 0., rect.w, rect.h)) else {
        return;
    };
    let result = sizes.get(1).map(|rect| Rect::new(0., 0., rect.w, rect.h));
    let icon_count = icons.len();
    let mut warmed = 0;
    let mut result_warmed = 0;
    let mut failures = 0;
    for icon in icons.into_iter().take(ICON_LIMIT) {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let path = crate::reload::resolve_icon_path(&icon);
        let (path, bytes) = asset(path);
        if path.as_path() == std::path::Path::new("@bead-icon-fallback.svg") {
            failures += 1;
        }
        if bead::render::prewarm_image(&path, &bytes, grid, scale, ImageFit::Fill).is_ok() {
            warmed += 1;
        }
        if result_warmed < 8
            && let Some(result) = result
            && bead::render::prewarm_image(&path, &bytes, result, scale, ImageFit::Fill).is_ok()
        {
            result_warmed += 1;
        }
    }
    let stats = bead::render::image_cache_stats();
    bead::core::vprintln!(
        "ICON_WARM icons={} raster_success={} result_rasters={} fallback_or_error={} query_errors={} raster_bytes={} charged_bytes={} limit_bytes={} scale={:.3}",
        icon_count,
        warmed,
        result_warmed,
        failures,
        query_errors,
        stats.raster_bytes,
        stats.charged_bytes,
        stats.limit_bytes,
        scale
    );
}

// Select only cache candidates; engine IDs, app ordering and ranking stay opaque.
fn likely_icons(
    home: &[serde_json::Value],
    apps: &[serde_json::Value],
    recent: &[String],
) -> Vec<String> {
    let recent_rows = recent
        .iter()
        .filter_map(|id| apps.iter().find(|row| row["id"] == *id));
    let mut seen = std::collections::BTreeSet::new();
    home.iter()
        .chain(recent_rows)
        .chain(apps.iter().take(64))
        .filter_map(|row| row["icon"].as_str())
        .filter(|icon| !icon.is_empty() && seen.insert((*icon).to_owned()))
        .take(ICON_LIMIT)
        .map(str::to_owned)
        .collect()
}

fn settings_sizes(
    root: &std::path::Path,
    palette_path: &std::path::Path,
) -> Result<Vec<Rect>, String> {
    let (generation, stores) = bead::ui::Generation::load(
        root,
        std::path::Path::new("ui/icon-warm.luau"),
        &Stores::default(),
        1,
    )
    .map_err(|error| error.to_string())?;
    let palette = Palette::read(palette_path).map_err(|error| error.to_string())?;
    let (scene, _) = generation
        .materialize(&stores, &palette, (1100., 590.))
        .map_err(|error| error.to_string())?;
    Ok(scene
        .primitives
        .into_iter()
        .filter_map(|primitive| match primitive {
            Primitive::Image { rect, .. } => Some(rect),
            _ => None,
        })
        .take(2)
        .collect())
}

fn asset(path: PathBuf) -> (PathBuf, Arc<[u8]>) {
    if let Ok(bytes) = std::fs::read(&path)
        && bead::render::validate_asset(&path, &bytes).is_ok()
    {
        return (path, Arc::from(bytes));
    }
    (
        PathBuf::from("@bead-icon-fallback.svg"),
        Arc::from(SAMPLE_ICON),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_icons_outside_the_first_apps_page_are_warmed_without_reordering_apps() {
        let apps: Vec<_> = (0..100)
            .map(|i| json!({"id":format!("app{i}"),"icon":format!("icon{i}")}))
            .collect();
        let selected = likely_icons(&apps[0..1], &apps, &["app99".into()]);
        assert_eq!(&selected[..4], &["icon0", "icon99", "icon1", "icon2"]);
        assert_eq!(selected.len(), ICON_LIMIT);
    }

    #[test]
    fn offscreen_warm_scene_uses_launcher_ui_image_sizes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let sizes = settings_sizes(&root, &root.join("palette.json")).unwrap();
        assert_eq!(sizes.len(), 2);
        assert_eq!((sizes[0].w, sizes[0].h), (36., 36.));
        assert_eq!((sizes[1].w, sizes[1].h), (30., 30.));
        assert_eq!(ICON_LIMIT, 64);
    }
}

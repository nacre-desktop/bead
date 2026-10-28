// SPDX-License-Identifier: GPL-3.0-only
//! Predecode visible wallpaper rasters from the same Luau scene the picker renders.
use bead::{
    core::{Stores, Value, source::Source},
    render::{ImageFit, Palette, Primitive},
};
use serde_json::{Value as Json, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

type WarmIdentity = (String, String, String, String, u32, u32, u32, u32, u32);
type DesiredRaster = (Option<PathBuf>, Option<PathBuf>, u32, u32, ImageFit);
const WARM_CATALOGUE_LIMIT: usize = bead::core::cache::MIB;

pub(crate) struct PreparedCatalogue {
    pub(crate) boot: String,
    pub(crate) active_path: String,
    pub(crate) kind: String,
    pub(crate) layout: String,
    viewport: (u32, u32),
    pub(crate) entries: Arc<[Json]>,
    scene: bead::render::Scene,
    _charge: bead::core::cache::Charge,
}

fn prepared_catalogue_slot() -> &'static std::sync::Mutex<Option<Arc<PreparedCatalogue>>> {
    static CATALOGUE: std::sync::OnceLock<std::sync::Mutex<Option<Arc<PreparedCatalogue>>>> =
        std::sync::OnceLock::new();
    CATALOGUE.get_or_init(|| std::sync::Mutex::new(None))
}

fn prepared_catalogue_budget() -> &'static bead::core::cache::Budget {
    static BUDGET: std::sync::OnceLock<bead::core::cache::Budget> = std::sync::OnceLock::new();
    BUDGET.get_or_init(|| {
        bead::core::cache::Budget::new("wallpaper-warm-catalogue", WARM_CATALOGUE_LIMIT)
    })
}

pub(crate) fn prepared_catalogue() -> Option<Arc<PreparedCatalogue>> {
    prepared_catalogue_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub(crate) fn prepared_fallback_scene(viewport: (u32, u32)) -> Option<bead::render::Scene> {
    let Some(prepared) = prepared_catalogue() else {
        bead::core::vprintln!("WALLPAPER_FALLBACK ready=false reason=cache_missing");
        return None;
    };
    if prepared.layout != "spotlight" || prepared.viewport != viewport {
        bead::core::vprintln!(
            "WALLPAPER_FALLBACK ready=false reason=identity layout={} prepared={}x{} requested={}x{}",
            prepared.layout,
            prepared.viewport.0,
            prepared.viewport.1,
            viewport.0,
            viewport.1
        );
        return None;
    }
    let mut scene = prepared.scene.clone();
    if let Some(motion) = &mut scene.motion {
        // Keep the first CPU snapshot visible while native GL prepares its own image set.
        motion.fade = false;
    }
    bead::core::vprintln!(
        "WALLPAPER_FALLBACK ready=true primitives={} viewport={}x{}",
        scene.primitives.len(),
        viewport.0,
        viewport.1
    );
    Some(scene)
}

fn publish_catalogue(
    identity: &WarmIdentity,
    entries: Vec<Json>,
    scene: bead::render::Scene,
) -> bool {
    let Ok(bytes) = serde_json::to_vec(&entries) else {
        return false;
    };
    let Ok(scene_bytes) = serde_json::to_vec(&scene.primitives) else {
        return false;
    };
    let charged_bytes = bytes.len().saturating_add(scene_bytes.len());
    if charged_bytes > WARM_CATALOGUE_LIMIT {
        return false;
    }
    let mut slot = prepared_catalogue_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *slot = None;
    let Ok(charge) = prepared_catalogue_budget().reserve(charged_bytes) else {
        return false;
    };
    *slot = Some(Arc::new(PreparedCatalogue {
        boot: identity.0.clone(),
        active_path: identity.1.clone(),
        kind: identity.2.clone(),
        layout: identity.3.clone(),
        viewport: (
            f32::from_bits(identity.4) as u32,
            f32::from_bits(identity.5) as u32,
        ),
        entries: entries.into(),
        scene,
        _charge: charge,
    }));
    true
}
struct WarmResult {
    warmed: usize,
    requested: usize,
    hydrate_ids: Vec<String>,
    fallback_scene: Option<bead::render::Scene>,
}
#[derive(Clone, Copy)]
struct WarmGeometry {
    viewport: (f32, f32),
    inner: (f32, f32),
    scale: f32,
}

fn cached_file(path: &Path, bytes: &mut HashMap<PathBuf, Vec<u8>>) -> Option<PathBuf> {
    let file = path.to_path_buf();
    let data = bytes
        .entry(file.clone())
        .or_insert_with(|| crate::extras::read_wallpaper_image(path).unwrap_or_default());
    (!data.is_empty()).then_some(file)
}

fn notify(wake: &SyncSender<()>) {
    match wake.try_send(()) {
        Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
    }
}

pub(crate) struct Warmer {
    width: Arc<AtomicU32>,
    height: Arc<AtomicU32>,
    inner_width: Arc<AtomicU32>,
    inner_height: Arc<AtomicU32>,
    scale: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    wake: SyncSender<()>,
    join: Option<thread::JoinHandle<()>>,
}

impl Warmer {
    pub(crate) fn new(
        client: crate::nacre_source::Client,
        source: Arc<dyn Source>,
        settings: Arc<dyn Source>,
        root: PathBuf,
        palette: Palette,
    ) -> Self {
        let width = Arc::new(AtomicU32::new(0));
        let height = Arc::new(AtomicU32::new(0));
        let inner_width = Arc::new(AtomicU32::new(0));
        let inner_height = Arc::new(AtomicU32::new(0));
        let scale = Arc::new(AtomicU32::new(0.0f32.to_bits()));
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("bead-wallpaper-warm".into())
            .spawn({
                let width = width.clone();
                let height = height.clone();
                let inner_width = inner_width.clone();
                let inner_height = inner_height.clone();
                let scale = scale.clone();
                let stop = stop.clone();
                let wake_signal = wake.clone();
                move || {
                    unsafe { libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 10) };
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    else {
                        return;
                    };
                    let Ok(_connection) = client.connection().subscribe(Arc::new({
                        let wake = wake_signal.clone();
                        move |_| notify(&wake)
                    })) else {
                        return;
                    };
                    let Ok(_topic) = source.subscribe(Arc::new({
                        let wake = wake_signal.clone();
                        move |_| notify(&wake)
                    })) else {
                        return;
                    };
                    let Ok(_settings) = settings.subscribe(Arc::new({
                        let wake = wake_signal.clone();
                        move |_| notify(&wake)
                    })) else {
                        return;
                    };
                    notify(&wake_signal);

                    let mut warmed: Option<(WarmIdentity, Instant)> = None;
                    while !stop.load(Ordering::Relaxed) {
                        let timeout = if warmed.is_some() {
                            Duration::from_secs(90)
                        } else {
                            Duration::from_secs(2)
                        };
                        match receiver.recv_timeout(timeout) {
                            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        while receiver.try_recv().is_ok() {}
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }

                        let w = f32::from_bits(width.load(Ordering::Relaxed));
                        let h = f32::from_bits(height.load(Ordering::Relaxed));
                        let inner_w = f32::from_bits(inner_width.load(Ordering::Relaxed));
                        let inner_h = f32::from_bits(inner_height.load(Ordering::Relaxed));
                        let s = f32::from_bits(scale.load(Ordering::Relaxed));
                        if [w, h, inner_w, inner_h, s]
                            .iter()
                            .any(|value| !value.is_finite() || *value <= 0.)
                        {
                            continue;
                        }
                        let Some(boot) = client.status().boot else {
                            continue;
                        };
                        let active = source.snapshot();
                        let preferences = settings.snapshot().data["data"].clone();
                        let kind = if preferences["kind"] == "dynamic" {
                            "dynamic"
                        } else {
                            "static"
                        };
                        let layout = match preferences["layout"].as_str() {
                            Some("spotlight") => "spotlight",
                            Some("hexagons") => "hexagons",
                            _ => "carousel",
                        };
                        let active_path = active.data["current"]["motion"]["path"]
                            .as_str()
                            .or_else(|| active.data["current"]["path"].as_str())
                            .unwrap_or_default()
                            .to_owned();
                        let identity = (
                            boot.clone(),
                            active_path,
                            kind.to_owned(),
                            layout.to_owned(),
                            w.to_bits(),
                            h.to_bits(),
                            inner_w.to_bits(),
                            inner_h.to_bits(),
                            s.to_bits(),
                        );
                        if warmed
                            .as_ref()
                            .is_some_and(|(previous, at)| previous == &identity && at.elapsed() < Duration::from_secs(90))
                        {
                            continue;
                        }

                        // The UI adapter owns refreshes and persists the catalogue snapshot.
                        // Never start a second catalogue-id scan from this background warmer.
                        let Some(entries) = crate::wallpaper_catalogue_cache::load() else {
                            warmed = None;
                            continue;
                        };
                        let mut catalogue = entries;
                        let Some(mut result) = warm_catalogue(
                            &root,
                            &palette,
                            &catalogue,
                            kind,
                            layout,
                            WarmGeometry {
                                viewport: (w, h),
                                inner: (inner_w, inner_h),
                                scale: s,
                            },
                        ) else {
                            warmed = None;
                            continue;
                        };
                        if !result.hydrate_ids.is_empty() {
                            let ids = result
                                .hydrate_ids
                                .iter()
                                .cloned()
                                .map(Json::String)
                                .collect::<Vec<_>>();
                            let Ok(reply) = runtime.block_on(client.call(
                                "wallpaper.catalogue".into(),
                                json!({"ids": ids}),
                                Duration::from_secs(2),
                            )) else {
                                warmed = None;
                                continue;
                            };
                            let Some(hydrated) = reply["entries"].as_array() else {
                                warmed = None;
                                continue;
                            };
                            for updated in hydrated {
                                let Some(id) = updated["id"].as_str() else {
                                    continue;
                                };
                                if let Some(entry) = catalogue
                                    .iter_mut()
                                    .find(|entry| entry["id"].as_str() == Some(id))
                                {
                                    for field in ["thumbnail", "preview"] {
                                        if updated[field]
                                            .as_str()
                                            .is_some_and(|path| !path.is_empty())
                                        {
                                            entry[field] = updated[field].clone();
                                        }
                                    }
                                }
                            }
                            let Some(hydrated_result) = warm_catalogue(
                                &root,
                                &palette,
                                &catalogue,
                                kind,
                                layout,
                                WarmGeometry {
                                    viewport: (w, h),
                                    inner: (inner_w, inner_h),
                                    scale: s,
                                },
                            ) else {
                                warmed = None;
                                continue;
                            };
                            result = hydrated_result;
                        }
                        let mut complete = result.warmed == result.requested
                            && result.hydrate_ids.is_empty()
                            && result.fallback_scene.is_some();
                        if complete
                            && let Some(scene) = result.fallback_scene.take()
                        {
                            complete = publish_catalogue(&identity, catalogue, scene);
                        }
                        let cache = bead::render::large_image_cache_stats();
                        bead::core::vprintln!(
                            "WALLPAPER_WARM ready={} rasters={} requested={} cache_bytes={} cache_limit={} layout={} viewport={:.0}x{:.0} size={:.0}x{:.0} scale={:.3}",
                            complete,
                            result.warmed,
                            result.requested,
                            cache.charged_bytes,
                            cache.limit_bytes,
                            layout,
                            w,
                            h,
                            inner_w,
                            inner_h,
                            s
                        );
                        warmed = complete.then(|| (identity, Instant::now()));
                    }
                }
            })
            .ok();
        Self {
            width,
            height,
            inner_width,
            inner_height,
            scale,
            stop,
            wake,
            join,
        }
    }

    pub(crate) fn set_viewport(
        &self,
        width: f32,
        height: f32,
        inner_width: f32,
        inner_height: f32,
    ) {
        self.width.store(width.to_bits(), Ordering::Relaxed);
        self.height.store(height.to_bits(), Ordering::Relaxed);
        self.inner_width
            .store(inner_width.to_bits(), Ordering::Relaxed);
        self.inner_height
            .store(inner_height.to_bits(), Ordering::Relaxed);
        notify(&self.wake);
    }

    pub(crate) fn set_scale(&self, scale: f32) {
        if scale.is_finite()
            && scale > 0.
            && self.scale.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits()
        {
            notify(&self.wake);
        }
    }
}

impl Drop for Warmer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        notify(&self.wake);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn warm_catalogue(
    root: &Path,
    palette: &Palette,
    entries: &[Json],
    kind: &str,
    layout: &str,
    geometry: WarmGeometry,
) -> Option<WarmResult> {
    let selectable: Vec<_> = entries
        .iter()
        .filter(|item| wallpaper_item_matches(item, kind))
        .collect();
    if selectable.is_empty() {
        // Catalogue can be valid but empty during daemon startup. Do not mark
        // this identity warm: no topic signals when catalogue indexing ends.
        bead::core::vprintln!(
            "WALLPAPER_WARM waiting_catalogue entries={} kind={}",
            entries.len(),
            kind
        );
        return None;
    }
    let selected = selectable
        .iter()
        .position(|item| item["active"] == true)
        .unwrap_or(0);
    let mut selection_indices = vec![selected];
    if layout == "spotlight" && selectable.len() > 1 {
        selection_indices.push((selected + selectable.len() - 1) % selectable.len());
        selection_indices.push((selected + 1) % selectable.len());
    }
    selection_indices.sort_unstable();
    selection_indices.dedup();

    let mut rows = entries.to_vec();
    for item in &mut rows {
        let id = item["id"].as_str()?.to_owned();
        item["thumbnail"] = format!("wallpaper:thumbnail:0:{id}").into();
        item["preview"] = format!("wallpaper:preview:0:{id}").into();
    }

    let mut requests = Vec::new();
    let mut request_keys = HashSet::new();
    let mut hydrate_ids = std::collections::BTreeSet::new();
    let mut fallback_scene = None;
    let (mut scene_images, mut wallpaper_handles, mut matched_rows) = (0, 0, 0);
    for selected_index in selection_indices {
        // A fresh generation makes ui.animate materialize at this selection's
        // settled geometry instead of inheriting the previous warm probe.
        let (generation, defaults) =
            bead::ui::Generation::load(root, Path::new("extras/main.luau"), &Stores::default(), 1)
                .ok()?;
        let item = selectable[selected_index];
        let selected_id = item["id"].as_str()?;
        let mut stores = defaults;
        stores
            .commit(BTreeMap::from([
                ("panel.open".into(), Value::Bool(true)),
                ("extras.kind".into(), Value::Text("wallpaper".into())),
                ("extras.available".into(), Value::Bool(true)),
                ("extras.message".into(), Value::Text(String::new())),
                ("extras.query".into(), Value::Text(String::new())),
                ("extras.rows".into(), Value::list(rows.clone())),
                ("extras.wallpaper.kind".into(), Value::Text(kind.into())),
                ("extras.wallpaper.layout".into(), Value::Text(layout.into())),
                ("extras.wallpaper.loading".into(), Value::Bool(false)),
                (
                    "extras.wallpaper.selection_id".into(),
                    Value::Text(selected_id.into()),
                ),
                (
                    "extras.wallpaper.carousel_position".into(),
                    Value::Number((selected_index + 1) as f64),
                ),
                (
                    "extras.viewport.width".into(),
                    Value::Number(geometry.inner.0 as f64),
                ),
                (
                    "extras.viewport.height".into(),
                    Value::Number(geometry.inner.1 as f64),
                ),
            ]))
            .ok()?;
        // The generation's behavior.luau computes extras.filtered from these
        // committed fixture values using the live selection/filter policy.
        let (scene, _) = generation
            .materialize(&stores, palette, geometry.viewport)
            .ok()?;
        if selected_index == selected {
            let mut prepared = scene.clone();
            resolve_wallpaper_handles(&mut prepared, entries);
            fallback_scene = Some(prepared);
        }
        for primitive in &scene.primitives {
            let Primitive::Image {
                path, rect, fit, ..
            } = primitive
            else {
                continue;
            };
            scene_images += 1;
            let Some(handle) = path
                .to_str()
                .and_then(|path| path.strip_prefix("@icon:wallpaper:"))
            else {
                continue;
            };
            wallpaper_handles += 1;
            let Some((id, field)) = crate::extras::parse_wallpaper_handle(handle) else {
                continue;
            };
            if selected_index != selected && id != selected_id {
                continue;
            }
            let Some(entry) = entries.iter().find(|entry| entry["id"] == id) else {
                continue;
            };
            matched_rows += 1;
            let primary = entry[field]
                .as_str()
                .filter(|path| !path.is_empty())
                .map(PathBuf::from);
            let fallback = (field == "preview")
                .then(|| {
                    entry["thumbnail"]
                        .as_str()
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from)
                })
                .flatten()
                .filter(|fallback| Some(fallback) != primary.as_ref());
            if primary.is_none() && fallback.is_none() {
                hydrate_ids.insert(id.to_owned());
            }
            let key = (
                primary.clone(),
                fallback.clone(),
                rect.w.to_bits(),
                rect.h.to_bits(),
                *fit,
            );
            if request_keys.insert(key) {
                requests.push((primary, fallback, *rect, *fit));
            }
        }
    }
    if requests.is_empty() {
        bead::core::vprintln!(
            "WALLPAPER_WARM no_rasters entries={} selectable={} scene_images={} handles={} rows={} kind={} layout={}",
            entries.len(),
            selectable.len(),
            scene_images,
            wallpaper_handles,
            matched_rows,
            kind,
            layout
        );
        return None;
    }

    let mut byte_cache = HashMap::<PathBuf, Vec<u8>>::new();
    let mut request_results = HashMap::<DesiredRaster, bool>::new();
    for (primary, fallback, rect, fit) in requests {
        let request = (
            primary.clone(),
            fallback.clone(),
            rect.w.to_bits(),
            rect.h.to_bits(),
            fit,
        );
        let file = primary
            .as_deref()
            .and_then(|path| cached_file(path, &mut byte_cache))
            .or_else(|| {
                fallback
                    .as_deref()
                    .and_then(|path| cached_file(path, &mut byte_cache))
            });
        let Some(file) = file else {
            request_results.insert(request, false);
            continue;
        };
        let success = byte_cache.get(&file).is_some_and(|bytes| {
            bead::render::prewarm_image(&file, bytes, rect, geometry.scale, fit).is_ok()
        });
        request_results.insert(request, success);
    }
    let warmed = request_results.values().filter(|success| **success).count();
    Some(WarmResult {
        warmed,
        requested: request_results.len(),
        hydrate_ids: hydrate_ids.into_iter().collect(),
        fallback_scene,
    })
}

fn resolve_wallpaper_handles(scene: &mut bead::render::Scene, entries: &[Json]) {
    for primitive in &mut scene.primitives {
        let Primitive::Image { path, .. } = primitive else {
            continue;
        };
        let Some(handle) = path
            .to_str()
            .and_then(|path| path.strip_prefix("@icon:wallpaper:"))
        else {
            continue;
        };
        let Some((id, field)) = crate::extras::parse_wallpaper_handle(handle) else {
            continue;
        };
        let Some(entry) = entries.iter().find(|entry| entry["id"] == id) else {
            continue;
        };
        let source = entry[field]
            .as_str()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                (field == "preview")
                    .then(|| {
                        entry["thumbnail"]
                            .as_str()
                            .filter(|value| !value.is_empty())
                    })
                    .flatten()
            });
        if let Some(source) = source {
            *path = PathBuf::from(source);
        }
    }
}

fn wallpaper_item_matches(item: &Json, kind: &str) -> bool {
    let animated = item["animated"] == true;
    if kind == "dynamic" {
        item["kind"] == "video" || animated
    } else {
        item["kind"] == "image" && !animated
    }
}

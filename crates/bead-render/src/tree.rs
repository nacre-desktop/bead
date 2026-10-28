// SPDX-License-Identifier: MIT OR Apache-2.0
//! Retained subtree ownership and immutable local rasters, independent of pose/clip.
use crate::{
    CpuRenderer, Damage, PixelFormat, Primitive, Rect, Renderer, Result, Scene, ShmTarget,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, OnceLock, Weak},
};
use tiny_skia::{
    FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform,
};

// Bound each prepared frame at 68 MiB; old and candidate frames may overlap until presentation.
// The 136 MiB per-owner overlap cap leaves the independent process-wide cap unchanged.
pub(crate) const STEADY_RASTER_LIMIT: usize = 68 * bead_core::cache::MIB;
const LIVE_RASTER_LIMIT: usize = 2 * STEADY_RASTER_LIMIT;

fn validate_frame_rasters(items: &[Item]) -> Result<()> {
    let bytes = items
        .iter()
        .filter_map(|item| item.raster.as_ref())
        .try_fold(0usize, |total, raster| {
            total.checked_add(raster.pixels.data().len())
        })
        .ok_or("candidate retained rasters overflow")?;
    let mut seen = BTreeSet::new();
    let upload_bytes = items
        .iter()
        .filter_map(|item| item.image.as_ref())
        .filter(|image| seen.insert(Arc::as_ptr(image) as usize))
        .try_fold(0usize, |total, image| {
            let upload = image.upload.lock().map_err(|_| "image upload poisoned")?;
            total
                .checked_add(
                    upload
                        .as_ref()
                        .map_or(0, |upload| upload.pixels.data().len()),
                )
                .ok_or("image upload bytes overflow")
        })?;
    let bytes = bytes
        .checked_add(upload_bytes)
        .ok_or("image frame bytes overflow")?;
    if bytes > STEADY_RASTER_LIMIT {
        let mut kind_bytes = [0usize; 4];
        let mut largest = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let Some(raster) = &item.raster else { continue };
            let kind = match &item.primitive {
                Primitive::Image { .. } => 0,
                Primitive::Text { .. } => 1,
                Primitive::Shadow { .. } => 2,
                _ => 3,
            };
            let size = raster.pixels.data().len();
            kind_bytes[kind] += size;
            largest.push((
                size,
                index,
                raster.pixels.width(),
                raster.pixels.height(),
                kind,
            ));
        }
        largest.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
        let largest = largest
            .iter()
            .take(3)
            .map(|(size, index, width, height, kind)| {
                format!(
                    "{}#{index}={width}x{height}/{size}",
                    ["Image", "Text", "Shadow", "Other"][*kind]
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        return Err(format!(
            "candidate retained rasters exceed steady cache budget: total={bytes}, Image={}, Text={}, Shadow={}, Other={}, largest=[{largest}]",
            kind_bytes[0], kind_bytes[1], kind_bytes[2], kind_bytes[3]
        )
        .into());
    }
    Ok(())
}

/// Only immutable image/text rasters wholly outside a persistent clip are omitted.
/// Reveal-window clips are animated poses and must never decide raster lifetime.
fn raster_hidden_by_own_clip(scene: &Scene, index: usize, primitive: &Primitive) -> bool {
    if !matches!(primitive, Primitive::Image { .. } | Primitive::Text { .. }) {
        return false;
    }
    let Some(clip) = scene.clips.get(&index) else {
        return false;
    };
    let offset = scene.content_offset(index);
    let bounds = rect(primitive).translated(offset.0, offset.1);
    let clip = clip.translated(offset.0, offset.1);
    let visible = bounds.intersect(clip);
    visible.w <= 0. || visible.h <= 0.
}

/// A caller-supplied prepared pose may be reused only if its image/text raster
/// attachments still match the scene's persistent clipping state.
fn prepared_rasters_match_visibility(scene: &Scene, frame: &Frame) -> bool {
    // Imported frames can intentionally have a different item vector than
    // the source scene. Preserve that existing path; visibility correspondence
    // is only defined for the generated one-item-per-primitive frame.
    frame.items.len() != scene.primitives.len()
        || scene
            .primitives
            .iter()
            .enumerate()
            .all(|(index, primitive)| {
                if matches!(primitive, Primitive::Image { .. } | Primitive::Text { .. }) {
                    let should_have_raster = !raster_hidden_by_own_clip(scene, index, primitive);
                    (frame.items[index].raster.is_some() || frame.items[index].image.is_some())
                        == should_have_raster
                } else {
                    true
                }
            })
}

#[derive(Clone)]
pub struct Raster {
    pub pixels: Arc<Pixmap>,
    /// Integer pixels trimmed from the original raster's left/top edges.
    pub origin: (u32, u32),
    pub key: u64,
    pub base_key: Option<u64>,
    /// Includes removed texels beyond the current extent; uploads must clip.
    pub changed: Damage,
    pub(crate) _charge: bead_core::cache::Charge,
}
/// Worker-prepared image pixels are consumed after upload; the immutable recipe
/// survives context recreation without retaining a second decoded copy.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ImagePolicy {
    #[default]
    Dense,
    UploadOnly,
    /// Presentation may create recipes, but only the resource worker decodes.
    DeferredUpload,
}
pub(crate) struct AssetLease {
    bytes: Arc<[u8]>,
    _charge: bead_core::cache::Charge,
}
pub struct ImagePayload {
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub native_cover: bool,
    primitive: Primitive,
    encoded: Arc<AssetLease>,
    scale: f32,
    pub(crate) upload: std::sync::Mutex<Option<UploadPixels>>,
    budget: bead_core::cache::Budget,
    scratch_budget: bead_core::cache::Budget,
    _recipe_charge: bead_core::cache::Charge,
    requested: std::sync::atomic::AtomicBool,
    failure: std::sync::Mutex<Option<String>>,
    pub(crate) compact: std::sync::Mutex<Option<Arc<ImageLayout>>>,
}
#[cfg_attr(not(feature = "native-gl"), allow(dead_code))]
pub(crate) struct ImageLayout {
    pub regions: Vec<crate::compact::CompactRegion>,
    pub fills: Vec<crate::compact::CompactFill>,
    _charge: bead_core::cache::Charge,
}
pub(crate) struct UploadPixels {
    pub pixels: Pixmap,
    _charge: bead_core::cache::Charge,
}
impl ImagePayload {
    /// Call on the resource worker only, never from presentation.
    pub fn prepare(&self) -> Result<()> {
        if self
            .upload
            .lock()
            .map_err(|_| "image upload poisoned")?
            .is_some()
        {
            return Ok(());
        }
        let Primitive::Image {
            rect,
            path,
            fit,
            clip_polygon,
            ..
        } = &self.primitive
        else {
            return Err("invalid upload recipe".into());
        };
        let decoded_bytes = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("image upload dimensions overflow")?;
        let source_bytes = if path.extension().is_some_and(|e| e == "svg") {
            0
        } else {
            let (w, h) = image::ImageReader::new(std::io::Cursor::new(self.encoded.bytes.as_ref()))
                .with_guessed_format()?
                .into_dimensions()?;
            (w as usize)
                .checked_mul(h as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or("image source dimensions overflow")?
        };
        // Resvg's full target and decoder/source intermediates are temporary;
        // retain only the final upload representation against the view ceiling.
        let scratch_bytes = if self.native_cover {
            decoded_bytes.checked_mul(2)
        } else {
            decoded_bytes
                .checked_add(source_bytes)
                .and_then(|bytes| bytes.checked_mul(2))
        }
        .ok_or("image scratch dimensions overflow")?;
        let _scratch = self.scratch_budget.reserve(scratch_bytes)?;
        let mut pixels = if self.native_cover {
            crate::decode_native_image(&self.encoded.bytes)?
        } else {
            crate::load_image(path, *rect, self.scale, Some(&self.encoded.bytes), *fit)?
        };
        if let Some(points) = clip_polygon {
            crate::apply_polygon_clip(&mut pixels, points)?;
        }
        let compact = if path.extension().is_some_and(|e| e == "svg") {
            crate::compact::CompactRaster::from_pixmap(&pixels)
        } else {
            None
        };
        let layout = if let Some(compact) = compact {
            debug_assert_eq!((compact.width, compact.height), (self.width, self.height));
            let charge = self
                .budget
                .reserve(compact.retained_bytes() - compact.atlas.data().len())?;
            pixels = compact.atlas;
            Some(Arc::new(ImageLayout {
                regions: compact.regions,
                fills: compact.fills,
                _charge: charge,
            }))
        } else {
            None
        };
        let pixel_charge = self.budget.reserve(pixels.data().len())?;
        bead_core::vprintln!(
            "PROFILE stage=image_bake_scratch reserved_bytes={scratch_bytes} retained_pixel_bytes={} key={}",
            pixels.data().len(),
            self.key
        );
        *self.compact.lock().map_err(|_| "image layout poisoned")? = layout;
        *self.upload.lock().map_err(|_| "image upload poisoned")? = Some(UploadPixels {
            pixels,
            _charge: pixel_charge,
        });
        Ok(())
    }
    #[cfg(any(feature = "native-gl", test))]
    pub(crate) fn reserve_texture(&self, bytes: usize) -> Result<bead_core::cache::Charge> {
        Ok(self.budget.reserve(bytes)?)
    }
    pub fn mark_requested(&self) -> bool {
        self.requested
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
    pub fn finish_request(&self, error: Option<String>) {
        if let Ok(mut failure) = self.failure.lock() {
            *failure = error;
        }
        self.requested
            .store(false, std::sync::atomic::Ordering::Release);
    }
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().ok().and_then(|failure| failure.clone())
    }
    pub fn pending(&self) -> bool {
        self.upload.lock().map_or(true, |upload| upload.is_none())
    }
    #[cfg(test)]
    pub(crate) fn take_upload(&self) -> Result<UploadPixels> {
        self.upload
            .lock()
            .map_err(|_| "image upload poisoned")?
            .take()
            .ok_or_else(|| "image upload not prepared".into())
    }
}
/// Presentation must ask the source's worker to hydrate these recipes and retry.
pub struct ResourcesPending {
    pub images: Vec<Arc<ImagePayload>>,
}
impl std::fmt::Debug for ResourcesPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourcesPending")
            .field("count", &self.images.len())
            .finish()
    }
}
impl std::fmt::Display for ResourcesPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} image resources pending", self.images.len())
    }
}
impl std::error::Error for ResourcesPending {}
#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub primitive: Primitive,
    pub clip: Option<Rect>,
    pub raster: Option<Arc<Raster>>,
    pub image: Option<Arc<ImagePayload>>,
}
// Prepared frames are shared through Arc; metadata has one allocation owner.
#[derive(Clone)]
pub struct Frame {
    pub items: Vec<Item>,
    pub scale: f32,
    pub key: u64,
    pub(crate) native_resource_budget: Option<bead_core::cache::Budget>,
    pub(crate) _assets: Vec<Arc<AssetLease>>,
    pub layer_count: usize,
    pub layers: Vec<LayerPose>,
    /// Worker-baked open picture used by reveal poses.
    pub reveal_artifact: Option<Arc<PreparedReveal>>,
    pub(crate) _accounting: Option<bead_core::cache::Charge>,
}

/// Immutable, worker-prepared open-pose pixels for a revealing panel.
/// Once created, presentation only translates these pixels and intersects them
/// with the current reveal window; it does not rebuild primitives or rasters.
#[derive(Clone)]
pub struct PreparedReveal {
    pixels: Arc<Pixmap>,
    bounds: Rect,
    scale: f32,
    key: u64,
    _charge: bead_core::cache::Charge,
}
type PreparedRevealCacheEntry = ((u64, u32), Arc<PreparedReveal>);
type PreparedRevealCache = std::sync::Mutex<VecDeque<PreparedRevealCacheEntry>>;

impl PreparedReveal {
    fn cache() -> &'static PreparedRevealCache {
        static CACHE: OnceLock<PreparedRevealCache> = OnceLock::new();
        CACHE.get_or_init(|| std::sync::Mutex::new(VecDeque::new()))
    }

    fn from_frame(frame: &Frame, scene: &Scene, scale: f32) -> Result<Self> {
        let (w, h) = (
            (scene.bounds.w * scale).ceil() as u32,
            (scene.bounds.h * scale).ceil() as u32,
        );
        if w == 0 || h == 0 {
            return Err("empty reveal bounds".into());
        }
        let mut pixels = Pixmap::new(w, h).ok_or("prepared reveal pixels")?;
        paint(
            frame,
            (-scene.bounds.x, -scene.bounds.y),
            ShmTarget {
                bytes: pixels.data_mut(),
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(w, h),
        )?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        frame.key.hash(&mut hasher);
        scale.to_bits().hash(&mut hasher);
        for value in [
            scene.bounds.x,
            scene.bounds.y,
            scene.bounds.w,
            scene.bounds.h,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        static REVEAL_ARTIFACT_BUDGET: OnceLock<bead_core::cache::Budget> = OnceLock::new();
        let charge = REVEAL_ARTIFACT_BUDGET
            .get_or_init(|| {
                bead_core::cache::Budget::new("prepared-reveal-artifacts", LIVE_RASTER_LIMIT)
            })
            .reserve(pixels.data().len() + 192)?;
        Ok(Self {
            pixels: Arc::new(pixels),
            bounds: scene.bounds,
            scale,
            key: hasher.finish(),
            _charge: charge,
        })
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    pub fn key(&self) -> u64 {
        self.key
    }
    pub fn retained_bytes(&self) -> usize {
        self.pixels.data().len() + 192
    }

    /// Composite the finished open picture at a logical picture offset,
    /// clipped to a destination-space reveal window. The destination is
    /// premultiplied RGBA/BGRA SHM.
    pub fn composite(
        &self,
        target: ShmTarget<'_>,
        damage: Damage,
        picture_offset: (f32, f32),
        clip: Rect,
    ) -> Result<()> {
        self.composite_with_opacity(target, damage, picture_offset, clip, 1.)
    }

    pub fn composite_with_opacity(
        &self,
        target: ShmTarget<'_>,
        damage: Damage,
        picture_offset: (f32, f32),
        clip: Rect,
        opacity: f32,
    ) -> Result<()> {
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        if scale.to_bits() != self.scale.to_bits() {
            return Err("prepared reveal scale mismatch".into());
        }
        if damage.x.checked_add(damage.w).is_none_or(|x| x > width)
            || damage.y.checked_add(damage.h).is_none_or(|y| y > height)
        {
            return Err("damage outside SHM".into());
        }
        let src_x = ((self.bounds.x + picture_offset.0) * scale).round() as i64;
        let src_y = ((self.bounds.y + picture_offset.1) * scale).round() as i64;
        let clip = clip.pixels(scale, width, height);
        let image_bounds = Rect::new(
            self.bounds.x + picture_offset.0,
            self.bounds.y + picture_offset.1,
            self.bounds.w,
            self.bounds.h,
        )
        .pixels(scale, width, height);
        let area = intersection(intersection(damage, clip), image_bounds);
        if area.w == 0 || area.h == 0 {
            return Ok(());
        }
        let source = self.pixels.data();
        let source_width = self.pixels.width() as usize;
        for y in area.y..area.y + area.h {
            for x in area.x..area.x + area.w {
                let sx = i64::from(x) - src_x;
                let sy = i64::from(y) - src_y;
                if sx < 0
                    || sy < 0
                    || sx >= self.pixels.width() as i64
                    || sy >= self.pixels.height() as i64
                {
                    continue;
                }
                let si = (sy as usize * source_width + sx as usize) * 4;
                let di = (y as usize * width as usize + x as usize) * 4;
                let mut pixel: [u8; 4] = source[si..si + 4].try_into().unwrap();
                if format == PixelFormat::Bgra {
                    pixel.swap(0, 2);
                }
                let opacity = opacity.clamp(0., 1.);
                if opacity < 1. {
                    for channel in &mut pixel {
                        *channel = (f32::from(*channel) * opacity + 0.5) as u8;
                    }
                }
                match pixel[3] {
                    0 => {}
                    255 => bytes[di..di + 4].copy_from_slice(&pixel),
                    alpha => {
                        let inverse = 1. - f32::from(alpha) / 255.;
                        for channel in 0..4 {
                            bytes[di + channel] = (f32::from(bytes[di + channel]) * inverse
                                + f32::from(pixel[channel])
                                + 0.5) as u8;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
impl Frame {
    /// Composite the worker-baked open picture using only the live offset and
    /// reveal window. Returns false when this frame has no reveal artifact.
    pub fn composite_reveal(
        &self,
        target: ShmTarget<'_>,
        damage: Damage,
        picture_offset: (f32, f32),
        destination_window: Rect,
    ) -> Result<bool> {
        let Some(artifact) = &self.reveal_artifact else {
            return Ok(false);
        };
        artifact.composite(target, damage, picture_offset, destination_window)?;
        Ok(true)
    }
    /// Keep immutable raster work when a scene drops primitives without changing their pixels.
    pub(crate) fn filtered(&self, keep: &[bool]) -> Option<Self> {
        if keep.len() != self.items.len() || !self.scale.is_finite() || self.scale <= 0. {
            return None;
        }
        let kept_indices = keep
            .iter()
            .enumerate()
            .filter_map(|(index, keep)| keep.then_some(index))
            .collect::<Vec<_>>();
        if kept_indices.len() == keep.len() {
            return None;
        }
        let kept_ids = kept_indices
            .iter()
            .map(|index| self.items[*index].id.as_str())
            .collect::<BTreeSet<_>>();
        let changed_layers = self
            .layers
            .iter()
            .map(|pose| {
                let own = pose
                    .layer
                    .own
                    .iter()
                    .filter(|id| kept_ids.contains(id.as_str()))
                    .collect::<Vec<_>>();
                (!own.iter().copied().eq(pose.layer.own.iter())).then_some(own)
            })
            .collect::<Vec<_>>();

        let mut bytes = std::mem::size_of::<Frame>()
            .checked_add(
                kept_indices
                    .len()
                    .checked_mul(std::mem::size_of::<Item>())?,
            )?
            .checked_add(
                self.layers
                    .len()
                    .checked_mul(std::mem::size_of::<LayerPose>())?,
            )?;
        for index in &kept_indices {
            let item = &self.items[*index];
            bytes = bytes
                .checked_add(item.id.capacity())?
                .checked_add(crate::primitive_bytes(&item.primitive))?;
        }
        for (pose, own) in self.layers.iter().zip(&changed_layers) {
            let Some(own) = own else { continue };
            let layer = &pose.layer;
            let own_bytes = own
                .iter()
                .try_fold(0usize, |total, id| total.checked_add(id.capacity()))?;
            let layer_bytes = std::mem::size_of::<RetainedLayer>()
                .checked_add(layer.id.capacity())?
                .checked_add(layer.parent.as_ref().map_or(0, String::capacity))?
                .checked_add(
                    layer
                        .children
                        .capacity()
                        .checked_mul(std::mem::size_of::<String>())?,
                )?
                .checked_add(
                    layer
                        .children
                        .iter()
                        .try_fold(0usize, |total, child| total.checked_add(child.capacity()))?,
                )?
                .checked_add(own.len().checked_mul(std::mem::size_of::<String>())?)?
                .checked_add(own_bytes)?;
            bytes = bytes.checked_add(layer_bytes)?;
        }
        bytes = bytes.checked_add(
            self._assets
                .capacity()
                .checked_mul(std::mem::size_of::<Arc<AssetLease>>())?,
        )?;
        static FILTERED_FRAME_BUDGET: OnceLock<bead_core::cache::Budget> = OnceLock::new();
        let accounting = self
            .native_resource_budget
            .as_ref()
            .unwrap_or_else(|| {
                FILTERED_FRAME_BUDGET.get_or_init(|| {
                    bead_core::cache::Budget::new(
                        "filtered-frame-metadata",
                        8 * bead_core::cache::MIB,
                    )
                })
            })
            .reserve(bytes)
            .ok()?;

        let items: Vec<_> = kept_indices
            .iter()
            .map(|index| self.items[*index].clone())
            .collect();
        let layers: Vec<_> = self
            .layers
            .iter()
            .zip(changed_layers)
            .map(|(pose, own)| {
                let Some(own) = own else {
                    return pose.clone();
                };
                let own = own.into_iter().cloned().collect::<Vec<_>>();
                let mut key = std::collections::hash_map::DefaultHasher::new();
                pose.layer.key.hash(&mut key);
                own.hash(&mut key);
                LayerPose {
                    layer: Arc::new(RetainedLayer {
                        id: pose.layer.id.clone(),
                        parent: pose.layer.parent.clone(),
                        children: pose.layer.children.clone(),
                        key: key.finish(),
                        own,
                        _charge: accounting.clone(),
                    }),
                    bounds: pose.bounds,
                }
            })
            .collect();
        let mut key = std::collections::hash_map::DefaultHasher::new();
        self.key.hash(&mut key);
        kept_indices.hash(&mut key);
        Some(Self {
            native_resource_budget: self.native_resource_budget.clone(),
            _assets: self._assets.clone(),
            layer_count: if self.layers.is_empty() {
                items.len()
            } else {
                self.layer_count
            },
            items,
            scale: self.scale,
            key: key.finish(),
            layers,
            reveal_artifact: self.reveal_artifact.clone(),
            _accounting: Some(accounting),
        })
    }
}
/// A retained subtree packet; unchanged row content reuses this allocation.
pub struct RetainedLayer {
    pub id: String,
    pub parent: Option<String>,
    pub own: Vec<String>,
    pub children: Vec<String>,
    pub key: u64,
    _charge: bead_core::cache::Charge,
}
#[derive(Clone)]
pub struct LayerPose {
    pub layer: Arc<RetainedLayer>,
    pub bounds: Rect,
}
struct Entry {
    raster: Arc<Raster>,
    /// One prior image extent can be reused when subpixel carousel motion
    /// alternates between adjacent raster sizes. Other primitive kinds stay single-entry.
    previous: Option<Arc<Raster>>,
    touched: u64,
}
#[derive(Default)]
pub struct TreeCache {
    pub frame: Option<Arc<Frame>>,
    pub builds: usize,
    image_policy: ImagePolicy,
    image_payloads: BTreeMap<u64, Weak<ImagePayload>>,
    asset_leases: BTreeMap<(usize, usize), Weak<AssetLease>>,
    upload_budget: Option<bead_core::cache::Budget>,
    resource_budget: Option<bead_core::cache::Budget>,
    bake_scratch_budget: Option<bead_core::cache::Budget>,
    entries: BTreeMap<(String, u32), Entry>,
    raster: CpuRenderer,
    font_files: Vec<std::path::PathBuf>,
    serial: u64,
    nodes: BTreeMap<(String, u32), (Arc<RetainedLayer>, u64)>,
    pub node_updates: usize,
    raster_budget: Option<bead_core::cache::Budget>,
    node_budget: Option<bead_core::cache::Budget>,
    frame_budget: Option<bead_core::cache::Budget>,
    /// B0117: pre-rasterised growing edges (None: unusable, draw that piece per frame); the artwork and a shadow.
    edges: Vec<(u64, Option<EdgeTemplate>)>,
    /// The last frame derived for a reveal pose (frame key, progress bits, scale bits): an unchanged pose is not rebuilt.
    revealed_pose: Option<((u64, u32, u32), Arc<Frame>)>,
    /// Uncomposited generated frame retained while reveal poses use cached content runs.
    reveal_source: Option<Arc<Frame>>,
    /// B0174: revealing content as prepared layers (content key, scale bits), composed with each pose's
    /// slide offset and reveal clip instead of re-painting every item.
    reveal_content: Option<(u64, u32, Arc<[ContentRun]>)>,
}
/// One piece of a revealing panel's content, in paint order.
enum ContentRun {
    /// A run of items painted once into one trimmed raster (None: fully transparent).
    Layer(usize, Option<Arc<Raster>>),
    /// An item that keeps its own GPU image payload.
    Item(usize),
}

/// Exact content identity for worker-prepared or frozen CPU panel content.
/// Resource revisions can advance during reveal ticks, so worker reuse compares
/// the prepared pixels and asset bytes directly.
struct SnapshotFingerprint {
    scale_bits: u32,
    resource_revision: u64,
    metadata: Vec<u8>,
    assets: Vec<(PathBuf, Weak<[u8]>)>,
}

fn cpu_content_scene(scene: &Scene) -> Scene {
    let mut content = scene.clone();
    if let Some(motion) = content.motion.as_ref().filter(|motion| motion.reveal) {
        for (index, original) in &motion.parts {
            if let Some(primitive) = content.primitives.get_mut(*index) {
                *primitive = original.clone();
            }
        }
        if motion.art.is_some() {
            content
                .asset_data
                .retain(|path, _| !path.to_string_lossy().starts_with("@reveal-art-"));
        }
    }
    content.motion = None;
    content
}

impl SnapshotFingerprint {
    fn new(scene: &Scene, scale: f32) -> Result<Self> {
        if !scale.is_finite() || scale <= 0. {
            return Err("invalid layer scale".into());
        }
        let content = cpu_content_scene(scene);
        let layers = content
            .layers
            .iter()
            .map(|layer| {
                (
                    &layer.id,
                    &layer.parent,
                    layer.own.start,
                    layer.own.end,
                    layer.subtree_end,
                    [
                        layer.bounds.x.to_bits(),
                        layer.bounds.y.to_bits(),
                        layer.bounds.w.to_bits(),
                        layer.bounds.h.to_bits(),
                    ],
                )
            })
            .collect::<Vec<_>>();
        let clips = content
            .clips
            .iter()
            .map(|(index, clip)| {
                (
                    *index,
                    [
                        clip.x.to_bits(),
                        clip.y.to_bits(),
                        clip.w.to_bits(),
                        clip.h.to_bits(),
                    ],
                )
            })
            .collect::<Vec<_>>();
        let metadata = serde_json::to_vec(&(
            scale.to_bits(),
            content.width.to_bits(),
            [
                content.bounds.x.to_bits(),
                content.bounds.y.to_bits(),
                content.bounds.w.to_bits(),
                content.bounds.h.to_bits(),
            ],
            &content.font_files,
            layers,
            clips,
            &content.primitives,
        ))?;
        let assets = content
            .asset_data
            .iter()
            .map(|(path, bytes)| (path.clone(), Arc::downgrade(bytes)))
            .collect();
        Ok(Self {
            scale_bits: scale.to_bits(),
            resource_revision: scene.resource_revision,
            metadata,
            assets,
        })
    }

    fn matches_content(&self, scene: &Scene, scale: f32) -> Result<bool> {
        if self.scale_bits != scale.to_bits() {
            return Ok(false);
        }
        let current = Self::new(scene, scale)?;
        Ok(self.metadata == current.metadata
            && self.assets.len() == current.assets.len()
            && self.assets.iter().zip(&current.assets).all(
                |((old_path, old_bytes), (path, bytes))| {
                    old_path == path
                        && (Weak::ptr_eq(old_bytes, bytes)
                            || matches!((old_bytes.upgrade(), bytes.upgrade()),
                                (Some(old), Some(current)) if old.as_ref() == current.as_ref()))
                },
            ))
    }

    fn matches_snapshot(&self, scene: &Scene, scale: f32) -> Result<bool> {
        // A flattened closed snapshot has lost the body/content item boundary
        // needed to derive reveal poses. Route reveals through worker preparation
        // and retain that frame; TreeCache then reuses its prepared content runs.
        // Snapshot reuse also preserves resource-revision invalidation for unresolved assets.
        if scene.motion.as_ref().is_some_and(|motion| motion.reveal)
            || self.resource_revision != scene.resource_revision
        {
            return Ok(false);
        }
        self.matches_content(scene, scale)
    }
}
/// The artwork's growing edge, rasterised once per size and scale and slid along the growth axis every frame.
struct EdgeTemplate {
    pixels: Arc<Pixmap>,
    /// Leading edge in the artwork raster's pixel grid, and the first band row/column of the template.
    lead: f32,
    start: i64,
    key: u64,
    charge: bead_core::cache::Charge,
}
#[derive(Clone, Copy)]
enum EdgeKind<'a> {
    Art(&'a crate::RevealArt),
    Shadow { radius: f32, blur: f32 },
}
struct EdgeSpec<'a> {
    kind: EdgeKind<'a>,
    edge: bead_core::Edge,
    /// The body whose reveal drives the piece (the shadow's rectangle less its 30 px margin).
    body: Rect,
    /// The piece's open-state rectangle and its raster extent along the growth axis.
    f: Rect,
    vertical: bool,
    far: bool,
    origin: f32,
    extent: u32,
    /// The piece's leading edge for a revealed body.
    lead_of: Box<dyn Fn(Rect) -> f32 + 'a>,
    length: f32,
}
/// Logical extent of the template on the body side (covers the corner radius) and outside the edge (rim).
const EDGE_INSIDE: f32 = 44.;
const EDGE_OUTSIDE: f32 = 8.;
/// Revealed length below which the top join and the growing edge interact: drawn the slow way.
const EDGE_MIN_REVEALED: f32 = EDGE_INSIDE + 40.;
pub fn rect(primitive: &Primitive) -> Rect {
    match primitive {
        Primitive::Arc { rect, .. }
        | Primitive::Polyline { rect, .. }
        | Primitive::Rounded { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => *rect,
    }
}
fn local(primitive: &Primitive) -> Primitive {
    let mut p = primitive.clone();
    match &mut p {
        Primitive::Arc { rect, .. }
        | Primitive::Polyline { rect, .. }
        | Primitive::Rounded { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => {
            rect.x = 0.;
            rect.y = 0.;
        }
    }
    p
}
/// Hash every field bit-exactly without allocating; `at_origin` ignores the position.
pub(crate) fn hash_primitive(primitive: &Primitive, at_origin: bool, hash: &mut impl Hasher) {
    let rect = |r: &Rect, hash: &mut _| {
        let (x, y) = if at_origin { (0., 0.) } else { (r.x, r.y) };
        for value in [x, y, r.w, r.h] {
            f32::to_bits(value).hash(hash);
        }
    };
    std::mem::discriminant(primitive).hash(hash);
    match primitive {
        Primitive::Arc {
            rect: r,
            start_angle,
            sweep_angle,
            stroke_width,
            color,
            round_caps,
        } => {
            rect(r, hash);
            (
                start_angle.to_bits(),
                sweep_angle.to_bits(),
                stroke_width.to_bits(),
                color,
                round_caps,
            )
                .hash(hash);
        }
        Primitive::Polyline {
            rect: r,
            points,
            closed,
            stroke_width,
            color,
            round_caps,
            fill,
        } => {
            rect(r, hash);
            for point in points {
                (point[0].to_bits(), point[1].to_bits()).hash(hash);
            }
            (closed, stroke_width.to_bits(), color, round_caps, fill).hash(hash);
        }
        Primitive::Rounded {
            rect: r,
            radius,
            fill,
            border,
        } => {
            rect(r, hash);
            (radius.to_bits(), fill).hash(hash);
            border
                .map(|(color, width)| (color, width.to_bits()))
                .hash(hash);
        }
        Primitive::Text {
            weight,
            align,
            rect: r,
            text,
            family,
            size,
            color,
            shadow,
        } => {
            rect(r, hash);
            let shadow = shadow.map(|shadow| {
                (
                    shadow.color,
                    shadow.blur.to_bits(),
                    shadow.offset_y.to_bits(),
                )
            });
            (weight, align, text, family, size.to_bits(), color, shadow).hash(hash);
        }
        Primitive::Image {
            blur,
            fit,
            clip_polygon,
            rect: r,
            path,
        } => {
            rect(r, hash);
            (blur.to_bits(), fit, path).hash(hash);
            clip_polygon.is_some().hash(hash);
            for coordinate in clip_polygon.iter().flatten().flatten() {
                coordinate.to_bits().hash(hash);
            }
        }
        Primitive::Shadow {
            rect: r,
            radius,
            blur,
        } => {
            rect(r, hash);
            (radius.to_bits(), blur.to_bits()).hash(hash);
        }
    }
}
fn signature(
    scene: &Scene,
    primitive: &Primitive,
    scale: f32,
    assets: &mut BTreeMap<std::path::PathBuf, u64>,
) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    scale.to_bits().hash(&mut hash);
    hash_primitive(primitive, true, &mut hash);
    if matches!(primitive, Primitive::Text { .. }) {
        scene.font_files.hash(&mut hash);
    }
    if let Primitive::Image { path, .. } = primitive {
        if !scene.asset_data.contains_key(path) {
            scene.resource_revision.hash(&mut hash);
        }
        // Icons repeat across tiles; hash each asset's bytes once per prepare.
        if !assets.contains_key(path) {
            let mut bytes = std::collections::hash_map::DefaultHasher::new();
            scene.asset_data.get(path).hash(&mut bytes);
            assets.insert(path.clone(), bytes.finish());
        }
        assets[path].hash(&mut hash);
    }
    hash.finish()
}
fn pixel_difference(before: &Pixmap, after: &Pixmap) -> Damage {
    let width = before.width().max(after.width());
    let height = before.height().max(after.height());
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    let mut changed = |x: u32, y: u32| {
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + 1);
        bottom = bottom.max(y + 1);
    };
    if before.width() == after.width() && before.height() == after.height() {
        for (index, (old, new)) in before
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after.data().as_chunks::<4>().0.iter())
            .enumerate()
        {
            if old != new {
                changed(index as u32 % width, index as u32 / width);
            }
        }
    } else {
        // Missing texels are transparent. Include removed tails as well as new
        // ink; the CPU repair clears and repaints the union of both extents.
        let sample = |image: &Pixmap, x: u32, y: u32| {
            if x < image.width() && y < image.height() {
                let i = (y * image.width() + x) as usize * 4;
                let pixel = &image.data()[i..i + 4];
                [pixel[0], pixel[1], pixel[2], pixel[3]]
            } else {
                [0; 4]
            }
        };
        for y in 0..height {
            for x in 0..width {
                if sample(before, x, y) != sample(after, x, y) {
                    changed(x, y);
                }
            }
        }
    }
    Damage {
        x: left.min(right),
        y: top.min(bottom),
        w: right.saturating_sub(left),
        h: bottom.saturating_sub(top),
    }
}
fn trim_text(pixels: Pixmap) -> Result<(Pixmap, (u32, u32))> {
    let (mut left, mut top, mut right, mut bottom) = (pixels.width(), pixels.height(), 0, 0);
    for (index, pixel) in pixels.pixels().iter().enumerate() {
        if pixel.alpha() != 0 {
            let x = index as u32 % pixels.width();
            let y = index as u32 / pixels.width();
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 {
        return Ok((Pixmap::new(1, 1).ok_or("empty text raster")?, (0, 0)));
    }
    if left == 0 && top == 0 && right == pixels.width() && bottom == pixels.height() {
        return Ok((pixels, (0, 0)));
    }
    let bounds = tiny_skia::IntRect::from_xywh(left as i32, top as i32, right - left, bottom - top)
        .ok_or("text ink bounds")?;
    Ok((
        pixels.clone_rect(bounds).ok_or("trimmed text raster")?,
        (left, top),
    ))
}
impl TreeCache {
    /// Worker-only completion step for reveal panels. Call after `prepare` and
    /// before publishing the frame; presentation never invokes this method.
    pub fn attach_reveal_artifact(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if !scene.motion.as_ref().is_some_and(|motion| motion.reveal) {
            return Err("scene is not a reveal".into());
        }
        let frame = self.frame.as_mut().ok_or("prepared reveal frame missing")?;
        if frame.items.iter().any(|item| item.image.is_some()) {
            // GPU-oriented upload-only frames do not own CPU pixels. The CPU
            // fallback worker will attach an artifact after dense preparation.
            return Ok(());
        }
        let cache_key = (frame.key, scale.to_bits());
        let artifact = {
            let mut cache = PreparedReveal::cache()
                .lock()
                .map_err(|_| "reveal artifact cache poisoned")?;
            if let Some(index) = cache.iter().position(|(key, _)| *key == cache_key) {
                let entry = cache.remove(index).expect("cached reveal artifact index");
                let artifact = entry.1.clone();
                cache.push_front(entry);
                artifact
            } else {
                while cache.len() >= 2 {
                    cache.pop_back();
                }
                let artifact = Arc::new(PreparedReveal::from_frame(frame, scene, scale)?);
                cache.push_front((cache_key, artifact.clone()));
                artifact
            }
        };
        Arc::make_mut(frame).reveal_artifact = Some(artifact);
        Ok(())
    }

    fn asset_lease(&mut self, bytes: Arc<[u8]>) -> Result<Arc<AssetLease>> {
        let key = (Arc::as_ptr(&bytes) as *const u8 as usize, bytes.len());
        if let Some(lease) = self.asset_leases.get(&key).and_then(Weak::upgrade) {
            return Ok(lease);
        }
        let charge = self
            .resource_budget
            .as_ref()
            .ok_or("native asset budget missing")?
            .reserve(bytes.len() + std::mem::size_of::<AssetLease>())?;
        let lease = Arc::new(AssetLease {
            bytes,
            _charge: charge,
        });
        self.asset_leases.insert(key, Arc::downgrade(&lease));
        Ok(lease)
    }

    pub fn set_image_policy(&mut self, policy: ImagePolicy) {
        self.image_policy = policy;
    }
    pub fn with_image_policy(mut self, policy: ImagePolicy) -> Self {
        self.image_policy = policy;
        self
    }
    /// Retire raster/glyph storage for scales no longer used by this view.
    /// Presented frames retain their own resources until the caller drops them.
    pub fn retain_scales(&mut self, scales: &[f32]) {
        let current = |bits: u32| scales.iter().any(|scale| scale.to_bits() == bits);
        let stale_frame = self
            .frame
            .as_ref()
            .is_some_and(|frame| !current(frame.scale.to_bits()));
        let discarded = stale_frame || self.entries.keys().any(|(_, bits)| !current(*bits));
        self.entries.retain(|(_, bits), _| current(*bits));
        self.nodes.retain(|(_, bits), _| current(*bits));
        if stale_frame {
            self.frame = None;
            self.edges.clear();
            self.revealed_pose = None;
            self.reveal_content = None;
            self.reveal_source = None;
        }
        if discarded {
            let fonts = self.raster.fonts.take();
            self.raster = CpuRenderer::default();
            self.raster.fonts = fonts;
        }
    }
    /// Bake counter: entries touched by a later bake have a greater serial.
    pub fn serial(&self) -> u64 {
        self.serial
    }
    /// Release the entries no bake since `serial` has used (a look that was replaced).
    pub fn drop_unused_since(&mut self, serial: u64) {
        self.entries.retain(|_, entry| entry.touched > serial);
        for entry in self.entries.values_mut() {
            entry.previous = None;
        }
        self.nodes.retain(|_, (_, touched)| *touched > serial);
        // Finished rasters own their pixels. The baker's fonts, decoded textures
        // and glyph scratch must not keep a replaced view resident.
        self.raster = CpuRenderer::default();
    }
    /// Keep finished active rasters, release scratch and offscreen cache entries.
    pub fn trim_idle(&mut self) {
        self.raster = CpuRenderer::default();
        self.entries.retain(|_, entry| entry.touched == self.serial);
        for entry in self.entries.values_mut() {
            entry.previous = None;
        }
        self.nodes.retain(|_, (_, touched)| *touched == self.serial);
    }
    /// Retire all pixel-bearing owners after a frozen snapshot has been built.
    fn release_pixel_storage(&mut self) {
        self.frame = None;
        self.entries.clear();
        self.nodes.clear();
        self.raster = CpuRenderer::default();
        self.font_files.clear();
        self.edges.clear();
        self.revealed_pose = None;
        self.reveal_content = None;
        self.reveal_source = None;
    }
    fn evict_previous_variant(&mut self) -> bool {
        let victim = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.previous.is_some())
            .min_by_key(|(_, entry)| entry.touched)
            .map(|(key, _)| key.clone());
        let Some(victim) = victim else {
            return false;
        };
        self.entries.get_mut(&victim).unwrap().previous = None;
        true
    }
    /// B0117: the geometry of a growing piece (the joined-frame artwork or the panel's shadow) for the edge
    /// template, or None when it is not worth or not possible to slide a template (small reveals).
    fn edge_spec<'a>(scene: &'a Scene, item: &Item, kind: EdgeKind<'a>) -> Option<EdgeSpec<'a>> {
        let motion = scene.motion.as_ref()?;
        let base = item.raster.as_ref()?;
        let (ix, iy) = motion.edge.inward();
        let (vertical, far) = (iy != 0., ix > 0. || iy > 0.);
        let f = rect(&item.primitive);
        let margin = match kind {
            EdgeKind::Art(_) => 0.,
            EdgeKind::Shadow { .. } => 30.,
        };
        let body = match kind {
            EdgeKind::Art(art) => art.body,
            EdgeKind::Shadow { .. } => Rect::new(f.x + 30., f.y + 30., f.w - 60., f.h - 60.),
        };
        let (origin, extent) = if vertical {
            (f.y, base.pixels.height())
        } else {
            (f.x, base.pixels.width())
        };
        let lead_of = move |r: Rect| {
            let at = match (vertical, far) {
                (true, true) => r.y + r.h,
                (true, false) => r.y,
                (false, true) => r.x + r.w,
                (false, false) => r.x,
            };
            at + if far { margin } else { -margin }
        };
        Some(EdgeSpec {
            kind,
            edge: motion.edge,
            body,
            f,
            vertical,
            far,
            origin,
            extent,
            lead_of: Box::new(lead_of),
            length: if vertical { body.h } else { body.w },
        })
    }
    /// B0117: the reveal artwork (or shadow) of a growing panel without rasterising anything per frame. The
    /// open-state raster (already prepared) shows up to the growing edge; the edge itself is a band rasterised
    /// once and slid by whole pixels. Returns the clipped raster item and the band item, or None to draw the
    /// piece the slow way (small reveals, templates that do not match the open-state raster).
    fn edge_items(
        &mut self,
        scene: &Scene,
        item: &Item,
        kind: EdgeKind<'_>,
        scale: f32,
    ) -> Result<Option<(Item, Item)>> {
        let Some(motion) = scene.motion.as_ref() else {
            return Ok(None);
        };
        let Some(spec) = Self::edge_spec(scene, item, kind) else {
            return Ok(None);
        };
        let body = spec.body;
        let full = (spec.lead_of)(body);
        let now = (spec.lead_of)(Scene::revealed(body, motion.edge, motion.progress));
        let revealed = spec.length - (now - full).abs();
        if revealed < EDGE_MIN_REVEALED
            || spec.length < 2. * EDGE_MIN_REVEALED
            || motion.progress >= 1.
        {
            return Ok(None);
        }
        self.ensure_edge(scene, item, kind, scale);
        let key = Self::edge_key(&spec, item, scale);
        let Some(template) = self
            .edges
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, t)| t.as_ref())
        else {
            return Ok(None);
        };
        let (vertical, far, f, origin) = (spec.vertical, spec.far, spec.f, spec.origin);
        let shift = ((now - origin) * scale - template.lead).round() as i64;
        let band = if vertical {
            template.pixels.height()
        } else {
            template.pixels.width()
        } as i64;
        let start = template.start + shift;
        if start < 0 || start + band > spec.extent as i64 {
            return Ok(None);
        }
        const M: f32 = 4096.;
        let (cut_start, cut_end) = (start as f32 / scale, (start + band) as f32 / scale);
        let clip = match (vertical, far) {
            (true, true) => Rect::new(f.x - M, f.y - M, f.w + 2. * M, origin + cut_start - f.y + M),
            (true, false) => Rect::new(
                f.x - M,
                origin + cut_end,
                f.w + 2. * M,
                f.y + f.h - origin - cut_end + M,
            ),
            (false, true) => {
                Rect::new(f.x - M, f.y - M, origin + cut_start - f.x + M, f.h + 2. * M)
            }
            (false, false) => Rect::new(
                origin + cut_end,
                f.y - M,
                f.x + f.w - origin - cut_end + M,
                f.h + 2. * M,
            ),
        };
        let mut clipped = item.clone();
        clipped.clip = Some(clip);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (template.key, start).hash(&mut hasher);
        let (w, h) = (template.pixels.width(), template.pixels.height());
        let piece = Item {
            image: None,
            id: format!("{}/edge", item.id),
            primitive: item.primitive.clone(),
            clip: None,
            raster: Some(Arc::new(Raster {
                pixels: template.pixels.clone(),
                origin: if vertical {
                    (0, start as u32)
                } else {
                    (start as u32, 0)
                },
                key: hasher.finish(),
                base_key: None,
                changed: Damage::full(w, h),
                _charge: template.charge.clone(),
            })),
        };
        Ok(Some((clipped, piece)))
    }
    fn edge_key(spec: &EdgeSpec<'_>, item: &Item, scale: f32) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let base = item.raster.as_ref().map_or(0, |r| r.key);
        (
            base,
            scale.to_bits(),
            spec.edge as u8,
            matches!(spec.kind, EdgeKind::Shadow { .. }),
        )
            .hash(&mut hash);
        for v in [
            spec.body.x,
            spec.body.y,
            spec.body.w,
            spec.body.h,
            spec.f.x,
            spec.f.y,
            spec.f.w,
            spec.f.h,
        ] {
            v.to_bits().hash(&mut hash);
        }
        hash.finish()
    }
    /// Build the edge template for this piece now unless it is already cached (called when a prepared frame
    /// arrives, so the first reveal frame does not pay for it).
    fn ensure_edge(&mut self, scene: &Scene, item: &Item, kind: EdgeKind<'_>, scale: f32) {
        let Some(spec) = Self::edge_spec(scene, item, kind) else {
            return;
        };
        let key = Self::edge_key(&spec, item, scale);
        if self.edges.iter().all(|(k, _)| *k != key) {
            let built = self.build_edge(&spec, item, scale, key).ok().flatten();
            self.edges.retain(|(k, _)| *k != key);
            if self.edges.len() >= 2 {
                self.edges.remove(0);
            }
            self.edges.push((key, built));
        }
    }
    /// Rasterise the growing edge once, at a ghost progress where it is clear of the origin join, and check that
    /// it is a faithful copy of the open-state raster's own edge (else None: the slow path stays correct).
    fn build_edge(
        &mut self,
        spec: &EdgeSpec<'_>,
        item: &Item,
        scale: f32,
        key: u64,
    ) -> Result<Option<EdgeTemplate>> {
        let _profile = bead_core::profile::Span::new("reveal_edge_build");
        let Some(base) = item.raster.as_ref() else {
            return Ok(None);
        };
        let EdgeSpec {
            kind,
            edge,
            body,
            f,
            vertical,
            far,
            origin,
            extent,
            ref lead_of,
            length,
        } = *spec;
        let inside = EDGE_INSIDE
            + match kind {
                EdgeKind::Art(_) => 0.,
                EdgeKind::Shadow { .. } => 30.,
            };
        let ghost_progress = ((EDGE_MIN_REVEALED + 24.) / length).clamp(0.5, 0.9);
        let ghost_body = Scene::revealed(body, edge, ghost_progress);
        let ghost = lead_of(ghost_body);
        let (low, high) = if far {
            (ghost - inside, ghost + EDGE_OUTSIDE)
        } else {
            (ghost - EDGE_OUTSIDE, ghost + inside)
        };
        let first = (((low - origin) * scale).floor() as i64).max(0);
        let last = (((high - origin) * scale).ceil() as i64).min(extent as i64);
        if last - first < 4 {
            return Ok(None);
        }
        let (w, h) = if vertical {
            (base.pixels.width(), (last - first) as u32)
        } else {
            ((last - first) as u32, base.pixels.height())
        };
        let pixels = match kind {
            EdgeKind::Art(art) => {
                let (a, b) = (first as f32 / scale, last as f32 / scale);
                let crop = if vertical {
                    Rect::new(f.x, origin + a, f.w, b - a)
                } else {
                    Rect::new(origin + a, f.y, b - a, f.h)
                };
                let (_, bytes) = (art.draw)(ghost_body, Some(crop));
                crate::svg_pixmap(&bytes, w, h)?
            }
            EdgeKind::Shadow { radius, blur } => {
                let grown = Rect::new(
                    ghost_body.x - 30.,
                    ghost_body.y - 30.,
                    ghost_body.w + 60.,
                    ghost_body.h + 60.,
                );
                let whole = crate::shadow_pixmap(grown, radius, blur, scale)?;
                // The ghost raster starts at its own origin; the band is cut at the item's pixel rows/columns.
                let offset = if vertical {
                    ((grown.y - origin) * scale).round() as i64
                } else {
                    ((grown.x - origin) * scale).round() as i64
                };
                let mut band = Pixmap::new(w, h).ok_or("edge band allocation")?;
                let source = whole.data();
                let (sw, sh) = (whole.width() as i64, whole.height() as i64);
                for y in 0..h as i64 {
                    for x in 0..w as i64 {
                        let (sx, sy) = if vertical {
                            (x, y + first - offset)
                        } else {
                            (x + first - offset, y)
                        };
                        if sx >= 0 && sx < sw && sy >= 0 && sy < sh {
                            let i = (sy * sw + sx) as usize * 4;
                            let o = (y * i64::from(w) + x) as usize * 4;
                            band.data_mut()[o..o + 4].copy_from_slice(&source[i..i + 4]);
                        }
                    }
                }
                band
            }
        };
        let at = |p: &Pixmap, x: u32, y: u32| {
            let i = (y * p.width() + x) as usize * 4;
            [
                p.data()[i],
                p.data()[i + 1],
                p.data()[i + 2],
                p.data()[i + 3],
            ]
        };
        let close = |a: [u8; 4], b: [u8; 4], tolerance: i32| {
            a.iter()
                .zip(b)
                .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= tolerance)
        };
        // The band must start in plain interior (rows beside the body side are identical) ...
        let (inner, next) = if far {
            (0, 1)
        } else {
            (last - first - 1, last - first - 2)
        };
        let uniform = if vertical {
            (0..w).all(|x| close(at(&pixels, x, inner as u32), at(&pixels, x, next as u32), 2))
        } else {
            (0..h).all(|y| close(at(&pixels, inner as u32, y), at(&pixels, next as u32, y), 2))
        };
        // ... and equal (colour gradient aside) to the open-state raster around its own edge.
        let shift = (((lead_of(body) - ghost) * scale).round() as i64).max(-first);
        // The edge may sit half a pixel off the raster's own (whole-pixel shifts), so each template channel
        // must lie between the raster's neighbouring pixels along the growth axis (plus a gradient margin).
        let within = |template: [u8; 4], around: [[u8; 4]; 3]| {
            (0..4).all(|c| {
                let low = around.iter().map(|p| i32::from(p[c])).min().unwrap_or(0) - 28;
                let high = around.iter().map(|p| i32::from(p[c])).max().unwrap_or(0) + 28;
                (low..=high).contains(&i32::from(template[c]))
            })
        };
        let matches = shift + last < extent as i64
            && first + shift >= 1
            && if vertical {
                (0..h).all(|y| {
                    (0..w).step_by(3).all(|x| {
                        let row = (i64::from(y) + first + shift) as u32;
                        within(
                            at(&pixels, x, y),
                            [
                                at(&base.pixels, x, row - 1),
                                at(&base.pixels, x, row),
                                at(&base.pixels, x, row + 1),
                            ],
                        )
                    })
                })
            } else {
                (0..w).all(|x| {
                    (0..h).step_by(3).all(|y| {
                        let column = (i64::from(x) + first + shift) as u32;
                        within(
                            at(&pixels, x, y),
                            [
                                at(&base.pixels, column - 1, y),
                                at(&base.pixels, column, y),
                                at(&base.pixels, column + 1, y),
                            ],
                        )
                    })
                })
            };
        if !uniform || !matches {
            bead_core::vprintln!(
                "REVEAL_EDGE unusable uniform={uniform} matches={matches} band={first}..{last} shift={shift} extent={extent} ghost={ghost}"
            );
            return Ok(None);
        }
        let charge = self
            .raster_budget
            .get_or_insert_with(|| {
                bead_core::cache::Budget::new("retained-cpu-rasters", LIVE_RASTER_LIMIT)
            })
            .reserve(pixels.data().len() + 192)?;
        Ok(Some(EdgeTemplate {
            pixels: Arc::new(pixels),
            lead: (ghost - origin) * scale,
            start: first,
            key,
            charge,
        }))
    }
    /// B0174: paint the content items of `frame` from `first` once per prepared frame. Each run without
    /// a GPU image payload becomes one raster trimmed to its ink in scene `bounds` space.
    fn reveal_content(
        &mut self,
        frame: &Frame,
        scene: &Scene,
        first: usize,
    ) -> Result<Arc<[ContentRun]>> {
        let bounds = scene.bounds;
        let normalize_geometry = |index: usize, primitive: &Primitive| {
            let source = rect(primitive);
            let open = rect(&scene.primitives[index]);
            primitive.translated(open.x - source.x, open.y - source.y)
        };
        let mut content_hash = std::collections::hash_map::DefaultHasher::new();
        frame.scale.to_bits().hash(&mut content_hash);
        first.hash(&mut content_hash);
        for value in [bounds.x, bounds.y, bounds.w, bounds.h] {
            value.to_bits().hash(&mut content_hash);
        }
        for (index, item) in frame.items.iter().enumerate().skip(first) {
            item.id.hash(&mut content_hash);
            let primitive = normalize_geometry(index, &item.primitive);
            hash_primitive(&primitive, false, &mut content_hash);
            let own_clip = scene.clips.get(&index).copied();
            own_clip.is_some().hash(&mut content_hash);
            if let Some(clip) = own_clip {
                for value in [clip.x, clip.y, clip.w, clip.h] {
                    value.to_bits().hash(&mut content_hash);
                }
            }
            item.raster
                .as_ref()
                .map(|raster| raster.key)
                .hash(&mut content_hash);
            item.image
                .as_ref()
                .map(|image| image.key)
                .hash(&mut content_hash);
        }
        let content_key = content_hash.finish();
        if let Some((key, scale, runs)) = &self.reveal_content
            && *key == content_key
            && *scale == frame.scale.to_bits()
        {
            return Ok(runs.clone());
        }
        let _profile = bead_core::profile::Span::new("reveal_content_build");
        let (w, h) = (
            (bounds.w * frame.scale).ceil() as u32,
            (bounds.h * frame.scale).ceil() as u32,
        );
        let mut runs = Vec::new();
        let mut start = first;
        while start < frame.items.len() {
            if frame.items[start].image.is_some() {
                runs.push(ContentRun::Item(start));
                start += 1;
                continue;
            }
            let end = frame.items[start..]
                .iter()
                .position(|item| item.image.is_some())
                .map_or(frame.items.len(), |n| start + n);
            let mut run_items = frame.items[start..end].to_vec();
            for (offset, item) in run_items.iter_mut().enumerate() {
                let index = start + offset;
                item.primitive = normalize_geometry(index, &item.primitive);
                item.clip = scene.clips.get(&index).copied();
            }
            let run = Frame {
                items: run_items,
                scale: frame.scale,
                key: 0,
                native_resource_budget: None,
                _assets: Vec::new(),
                layer_count: 0,
                layers: Vec::new(),
                reveal_artifact: None,
                _accounting: None,
            };
            let (w, h) = (w.max(1), h.max(1));
            let mut pixels = Pixmap::new(w, h).ok_or("reveal content layer")?;
            paint(
                &run,
                (-bounds.x, -bounds.y),
                ShmTarget {
                    bytes: pixels.data_mut(),
                    width: w,
                    height: h,
                    scale: frame.scale,
                    format: PixelFormat::Rgba,
                },
                Damage::full(w, h),
            )?;
            let raster = match ink_bounds(&pixels) {
                None => None,
                Some(ink) => {
                    let trimmed = crop_pixmap(&pixels, ink).ok_or("reveal content crop")?;
                    let charge = self
                        .raster_budget
                        .get_or_insert_with(|| {
                            bead_core::cache::Budget::new("retained-cpu-rasters", LIVE_RASTER_LIMIT)
                        })
                        .reserve(trimmed.data().len() + 192)?;
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    ("reveal-content", content_key, start).hash(&mut hasher);
                    Some(Arc::new(Raster {
                        changed: Damage::full(trimmed.width(), trimmed.height()),
                        pixels: Arc::new(trimmed),
                        origin: (ink.x, ink.y),
                        key: hasher.finish(),
                        base_key: None,
                        _charge: charge,
                    }))
                }
            };
            runs.push(ContentRun::Layer(start, raster));
            start = end;
        }
        let runs: Arc<[ContentRun]> = runs.into();
        self.reveal_content = Some((content_key, frame.scale.to_bits(), runs.clone()));
        Ok(runs)
    }
    /// B0067: a worker-prepared frame of a panel that is still growing out of its edge. The prepared items keep their
    /// open-state clips and shapes, so the presenter derives the frame for this progress: the body fill and its
    /// shadow take the reshaped primitives of `Scene::reveal` (the shadow gets a raster of its new size), everything
    /// else is clipped to the reveal window. None when the scene is not revealing or does not match the frame.
    fn revealed_frame(
        &mut self,
        scene: &Scene,
        frame: &Arc<Frame>,
        scale: f32,
    ) -> Result<Option<Frame>> {
        let Some(motion) = scene
            .motion
            .as_ref()
            .filter(|m| m.reveal && m.progress < 1.)
        else {
            return Ok(None);
        };
        let Some(_window) = scene.reveal_window() else {
            return Ok(None);
        };
        if frame.items.len() != scene.primitives.len() {
            return Ok(None);
        }
        // Everything above the body parts is content; it is composited from prepared layers.
        let first_content = motion.parts.iter().map(|(i, _)| i + 1).max().unwrap_or(0);
        let mut items = frame.items.clone();
        let mut edge_pieces = Vec::new();
        for (index, item) in items.iter_mut().enumerate().take(first_content) {
            if motion.parts.iter().any(|(i, _)| *i == index) {
                let kind = match (&item.primitive, motion.art.as_ref()) {
                    (_, Some(art)) if art.index == index => Some(EdgeKind::Art(art)),
                    (Primitive::Shadow { radius, blur, .. }, _) => Some(EdgeKind::Shadow {
                        radius: *radius,
                        blur: *blur,
                    }),
                    _ => None,
                };
                if let Some(kind) = kind
                    && let Some((clipped, piece)) = self.edge_items(scene, item, kind, scale)?
                {
                    *item = clipped;
                    edge_pieces.push((index + 1, piece));
                    continue;
                }
                let shaped = scene.primitives[index].clone();
                if matches!(shaped, Primitive::Shadow { .. } | Primitive::Image { .. }) {
                    let content = Scene {
                        opacity: 1.,
                        motion: None,
                        resource_revision: scene.resource_revision,
                        asset_data: if matches!(&shaped, Primitive::Image { .. }) {
                            scene.asset_data.clone()
                        } else {
                            Default::default()
                        },
                        clips: Default::default(),
                        layers: Vec::new(),
                        prepared: Default::default(),
                        font_files: Vec::new(),
                        width: scene.width,
                        slide: None,
                        primitives: vec![local(&shaped)],
                        offset: (0., 0.),
                        bounds: scene.bounds,
                    };
                    let raster_profile = bead_core::profile::Span::new("reveal_art_raster");
                    let sliced = match (&shaped, motion.art.as_ref()) {
                        (Primitive::Image { rect, .. }, Some(art)) if art.index == index => {
                            let open = Scene::revealed(art.body, motion.edge, motion.progress);
                            if open.w.min(open.h) < 1. {
                                // The closing tail: under a pixel of panel shows nothing (a blank pixel).
                                Pixmap::new(1, 1)
                            } else {
                                sliced_art(art, motion.edge, motion.progress, *rect, scale)
                            }
                        }
                        _ => None,
                    };
                    let (pixels, key) = if let Some(pixels) = sliced {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        (
                            motion.progress.to_bits(),
                            scale.to_bits(),
                            pixels.width(),
                            pixels.height(),
                        )
                            .hash(&mut hasher);
                        (pixels, hasher.finish())
                    } else {
                        self.raster.prepare(&content, scale)?;
                        let texture = crate::key(&content.primitives[0], scale);
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        texture.hash(&mut hasher);
                        let key = hasher.finish();
                        let pixels = self
                            .raster
                            .textures
                            .remove(&texture)
                            .ok_or("missing prepared reveal raster")?;
                        (pixels, key)
                    };
                    drop(raster_profile);
                    let bytes = pixels.data().len() + 192;
                    let charge = self
                        .raster_budget
                        .get_or_insert_with(|| {
                            bead_core::cache::Budget::new("retained-cpu-rasters", LIVE_RASTER_LIMIT)
                        })
                        .reserve(bytes)?;
                    item.image = None;
                    item.raster = Some(Arc::new(Raster {
                        changed: Damage::full(pixels.width(), pixels.height()),
                        pixels: Arc::new(pixels),
                        origin: (0, 0),
                        key,
                        base_key: None,
                        _charge: charge,
                    }));
                }
                item.primitive = shaped;
            } else if index < first_content {
                let (dx, dy) = scene.content_offset(index);
                item.primitive = scene.primitives[index].translated(dx, dy);
                item.clip = scene.clip_at(index);
            }
        }
        // The content slides as prepared layers under the reveal window (B0174).
        let (dx, dy) = scene.content_offset(first_content);
        let window = scene.reveal_window();
        let content = self.reveal_content(frame, scene, first_content)?;
        let mut composed = Vec::with_capacity(content.len());
        for run in content.iter() {
            match run {
                ContentRun::Layer(_, None) => {}
                ContentRun::Layer(start, Some(raster)) => composed.push(Item {
                    id: format!("reveal-content/{start}"),
                    primitive: Primitive::Rounded {
                        rect: scene.bounds.translated(dx, dy),
                        radius: 0.,
                        fill: [0; 4],
                        border: None,
                    },
                    clip: window,
                    raster: Some(raster.clone()),
                    image: None,
                }),
                ContentRun::Item(index) => {
                    let mut item = items[*index].clone();
                    item.primitive = scene.primitives[*index].translated(dx, dy);
                    item.clip = scene.clip_at(*index);
                    composed.push(item);
                }
            }
        }
        items.truncate(first_content);
        items.extend(composed);
        for (at, piece) in edge_pieces.into_iter().rev() {
            items.insert(at, piece);
        }
        let mut key = std::collections::hash_map::DefaultHasher::new();
        frame.key.hash(&mut key);
        motion.progress.to_bits().hash(&mut key);
        Ok(Some(Frame {
            native_resource_budget: frame.native_resource_budget.clone(),
            _assets: frame._assets.clone(),
            items,
            scale: frame.scale,
            key: key.finish(),
            layer_count: frame.layer_count,
            layers: frame.layers.clone(),
            reveal_artifact: frame.reveal_artifact.clone(),
            _accounting: None,
        }))
    }
    pub fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        let revealing = scene
            .motion
            .as_ref()
            .is_some_and(|motion| motion.reveal && motion.progress < 1.);
        if let Some(source) = self.reveal_source.take() {
            self.frame = Some(source);
        }
        if !revealing {
            self.reveal_content = None;
        }
        if self.image_policy != ImagePolicy::Dense && self.resource_budget.is_none() {
            let budget = bead_core::cache::Budget::new(
                "retained-view-resources",
                40 * bead_core::cache::MIB,
            );
            self.raster_budget = Some(budget.clone());
            self.upload_budget = Some(budget.clone());
            self.node_budget = Some(budget.clone());
            self.frame_budget = Some(budget.clone());
            self.resource_budget = Some(budget);
            self.bake_scratch_budget = Some(bead_core::cache::Budget::new(
                "image-bake-scratch",
                128 * bead_core::cache::MIB,
            ));
        }
        if let Some(frame) = scene
            .prepared
            .get(&scale.to_bits())
            .filter(|frame| prepared_rasters_match_visibility(scene, frame))
            .filter(|frame| {
                self.image_policy != ImagePolicy::Dense
                    || frame.items.iter().all(|item| item.image.is_none())
            })
        {
            if let Some(motion) = scene.motion.as_ref().filter(|m| m.reveal) {
                // Build the growing-edge templates as the prepared frame arrives, not in the first reveal frame.
                for (index, _) in &motion.parts {
                    let Some(item) = frame.items.get(*index) else {
                        continue;
                    };
                    match (&item.primitive, motion.art.as_ref()) {
                        (_, Some(art)) if art.index == *index => {
                            self.ensure_edge(scene, item, EdgeKind::Art(art), scale);
                        }
                        (Primitive::Shadow { radius, blur, .. }, _) => self.ensure_edge(
                            scene,
                            item,
                            EdgeKind::Shadow {
                                radius: *radius,
                                blur: *blur,
                            },
                            scale,
                        ),
                        _ => {}
                    }
                }
            }
            let pose = scene
                .motion
                .as_ref()
                .filter(|m| m.reveal && m.progress < 1.)
                .map(|m| (frame.key, m.progress.to_bits(), scale.to_bits()));
            let candidate = if let (Some(pose), Some((last, revealed))) =
                (pose, &self.revealed_pose)
                && *last == pose
            {
                // This exact pose was validated before it entered the cache.
                revealed.clone()
            } else {
                let candidate = match self.revealed_frame(scene, frame, scale)? {
                    Some(revealed) => Arc::new(revealed),
                    None => frame.clone(),
                };
                validate_frame_rasters(&candidate.items)?;
                candidate
            };
            // Validate before retiring the last good fallback. Worker-prepared
            // frames own their rasters and nodes after this hand-off.
            if !self.entries.is_empty() || !self.nodes.is_empty() {
                self.entries.clear();
                self.nodes.clear();
                self.raster = CpuRenderer::default();
                self.font_files.clear();
            }
            self.frame = Some(candidate);
            self.reveal_source = pose.map(|_| frame.clone());
            self.revealed_pose = pose.zip(self.frame.clone());
            return Ok(());
        }
        if !scale.is_finite() || scale <= 0. {
            return Err("invalid layer scale".into());
        }
        self.asset_leases
            .retain(|_, lease| lease.strong_count() > 0);
        let assets = if self.resource_budget.is_some() {
            scene
                .asset_data
                .values()
                .cloned()
                .map(|bytes| self.asset_lease(bytes))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        self.image_payloads
            .retain(|_, payload| payload.strong_count() > 0);
        self.serial += 1;
        if self.font_files != scene.font_files {
            self.raster = CpuRenderer::default();
            self.font_files = scene.font_files.clone();
        }
        let mut owners = vec![String::new(); scene.primitives.len()];
        for layer in &scene.layers {
            for index in layer.own.clone() {
                let owner = owners
                    .get_mut(index)
                    .ok_or("invalid layer primitive range")?;
                if !owner.is_empty() {
                    return Err("overlapping layer ownership".into());
                }
                *owner = layer.id.clone();
            }
        }
        let mut ordinals = BTreeMap::<(String, &'static str), usize>::new();
        let mut items = Vec::with_capacity(scene.primitives.len());
        let mut keys = Vec::with_capacity(scene.primitives.len());
        let mut asset_keys = BTreeMap::new();
        let mut frame_hash = std::collections::hash_map::DefaultHasher::new();
        let mut raster_scene = None;
        let mut fresh_rasters: BTreeMap<u64, (Arc<Pixmap>, (u32, u32))> = BTreeMap::new();
        serde_json::to_vec(&scene.bounds)?.hash(&mut frame_hash);
        for (index, primitive) in scene.primitives.iter().enumerate() {
            let kind = match primitive {
                Primitive::Arc { .. } => "arc",
                Primitive::Polyline { .. } => "polyline",
                Primitive::Rounded { .. } => "fill",
                Primitive::Text { .. } => "text",
                Primitive::Image { .. } => "image",
                Primitive::Shadow { .. } => "shadow",
            };
            let owner = if owners[index].is_empty() {
                format!("primitive-{index}")
            } else {
                owners[index].clone()
            };
            let ordinal = ordinals.entry((owner.clone(), kind)).or_default();
            let id = format!("{owner}/{kind}/{ordinal}");
            *ordinal += 1;
            let key = signature(scene, primitive, scale, &mut asset_keys);
            keys.push(key);
            id.hash(&mut frame_hash);
            key.hash(&mut frame_hash);
            hash_primitive(primitive, false, &mut frame_hash);
            let clip = scene.clip_at(index);
            if let Some(clip) = clip {
                for value in [clip.x, clip.y, clip.w, clip.h] {
                    value.to_bits().hash(&mut frame_hash);
                }
            }
            let raster_hidden = raster_hidden_by_own_clip(scene, index, primitive);
            // Reshaped reveal artwork uses the existing exact growing-edge path.
            let reshaped_reveal = scene.motion.as_ref().is_some_and(|motion| {
                motion.reveal && motion.parts.iter().any(|(part, _)| *part == index)
            });
            let image = if self.image_policy != ImagePolicy::Dense
                && !raster_hidden
                && !reshaped_reveal
            {
                if let Primitive::Image {
                    path,
                    blur,
                    fit,
                    clip_polygon,
                    ..
                } = primitive
                    && *blur == 0.
                {
                    if let Some(payload) = self.image_payloads.get(&key).and_then(Weak::upgrade) {
                        Some(payload)
                    } else {
                        let encoded = scene
                            .asset_data
                            .get(path)
                            .cloned()
                            .map(Ok)
                            .unwrap_or_else(|| std::fs::read(path).map(Arc::from))?;
                        let native_cover = *fit == crate::ImageFit::Cover
                            && clip_polygon.is_none()
                            && !path.extension().is_some_and(|e| e == "svg");
                        let (width, height) = if native_cover {
                            image::ImageReader::new(std::io::Cursor::new(encoded.as_ref()))
                                .with_guessed_format()?
                                .into_dimensions()?
                        } else {
                            crate::dimensions(rect(primitive), scale)
                        };
                        let target = crate::dimensions(rect(primitive), scale);
                        let native_cover = native_cover
                            && u64::from(width) * u64::from(height)
                                <= u64::from(target.0) * u64::from(target.1);
                        let (width, height) = if native_cover {
                            (width, height)
                        } else {
                            target
                        };
                        let encoded = self.asset_lease(encoded)?;
                        let budget = self
                            .upload_budget
                            .get_or_insert_with(|| {
                                bead_core::cache::Budget::new(
                                    "image-upload-pixels",
                                    LIVE_RASTER_LIMIT,
                                )
                            })
                            .clone();
                        let recipe_charge = budget.reserve(
                            std::mem::size_of::<ImagePayload>() + crate::primitive_bytes(primitive),
                        )?;
                        let payload = Arc::new(ImagePayload {
                            key: {
                                let mut hash = std::collections::hash_map::DefaultHasher::new();
                                key.hash(&mut hash);
                                "upload-image".hash(&mut hash);
                                hash.finish()
                            },
                            width,
                            height,
                            native_cover,
                            primitive: local(primitive),
                            encoded,
                            scale,
                            requested: Default::default(),
                            failure: Default::default(),
                            compact: Default::default(),
                            upload: Default::default(),
                            budget,
                            scratch_budget: self
                                .bake_scratch_budget
                                .as_ref()
                                .ok_or("image bake scratch budget missing")?
                                .clone(),
                            _recipe_charge: recipe_charge,
                        });
                        if self.image_policy == ImagePolicy::UploadOnly {
                            payload.prepare()?;
                        }
                        self.image_payloads.insert(key, Arc::downgrade(&payload));
                        Some(payload)
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let raster = if kind == "fill"
                || kind == "arc"
                || kind == "polyline"
                || raster_hidden
                || image.is_some()
            {
                None
            } else {
                let slot = (id.clone(), scale.to_bits());
                if kind == "image"
                    && let Some(entry) = self.entries.get_mut(&slot)
                    && entry.raster.key != key
                    && entry
                        .previous
                        .as_ref()
                        .is_some_and(|raster| raster.key == key)
                {
                    let previous = entry.previous.take().unwrap();
                    let current = std::mem::replace(&mut entry.raster, previous);
                    entry.previous = Some(current);
                }
                if self
                    .entries
                    .get(&slot)
                    .is_none_or(|entry| entry.raster.key != key)
                {
                    let previous_frame_item = self
                        .frame
                        .as_ref()
                        .filter(|frame| frame.scale == scale)
                        .and_then(|frame| frame.items.get(index))
                        .filter(|item| {
                            let before = rect(&item.primitive);
                            let after = rect(primitive);
                            before.x == after.x && before.y == after.y
                        })
                        .map(|item| (item.id.clone(), item.raster.clone()));
                    let previous_frame_slot = previous_frame_item
                        .as_ref()
                        .map(|(id, _)| (id.clone(), scale.to_bits()));
                    let previous_frame_raster = previous_frame_item
                        .as_ref()
                        .and_then(|(_, raster)| raster.clone());
                    let previous_raster = previous_frame_raster
                        .clone()
                        .or_else(|| self.entries.get(&slot).map(|entry| entry.raster.clone()));
                    let cached_variant = (kind == "image")
                        .then(|| {
                            previous_frame_slot
                                .as_ref()
                                .and_then(|previous_slot| self.entries.get(previous_slot))
                                .and_then(|entry| {
                                    if entry.raster.key == key {
                                        Some(entry.raster.clone())
                                    } else {
                                        entry
                                            .previous
                                            .as_ref()
                                            .filter(|raster| raster.key == key)
                                            .cloned()
                                    }
                                })
                        })
                        .flatten();
                    let started = std::time::Instant::now();
                    let existing = fresh_rasters.get(&key).cloned();
                    let reused = existing.is_some() || cached_variant.is_some();
                    let (pixels, origin) = if let Some(raster) = existing {
                        raster
                    } else if let Some(raster) = cached_variant {
                        (raster.pixels.clone(), raster.origin)
                    } else {
                        // A cache miss bakes one primitive, not the rest of its
                        // scene. Reuse its immutable context across missing rasters.
                        let content = raster_scene.get_or_insert_with(|| Scene {
                            opacity: 1.,
                            motion: scene.motion.clone(),
                            resource_revision: scene.resource_revision,
                            asset_data: scene.asset_data.clone(),
                            clips: Default::default(),
                            layers: Vec::new(),
                            prepared: Default::default(),
                            font_files: scene.font_files.clone(),
                            width: scene.width,
                            slide: scene.slide,
                            primitives: Vec::with_capacity(1),
                            offset: (0., 0.),
                            bounds: scene.bounds,
                        });
                        content.primitives.clear();
                        content.primitives.push(local(primitive));
                        self.raster.prepare(content, scale)?;
                        let pixels = self
                            .raster
                            .textures
                            .remove(&crate::key(&content.primitives[0], scale))
                            .ok_or("missing prepared raster")?;
                        let (pixels, origin) = if kind == "text" {
                            trim_text(pixels)?
                        } else {
                            (pixels, (0, 0))
                        };
                        let raster = (Arc::new(pixels), origin);
                        fresh_rasters.insert(key, raster.clone());
                        raster
                    };
                    fresh_rasters
                        .entry(key)
                        .or_insert_with(|| (pixels.clone(), origin));
                    // Prefer the last ordered paint slot: a replacement row can
                    // have a new state ID while most of its raster is unchanged.
                    // The presenter uses this delta only when its exact previous
                    // raster key, geometry, clip and opacity match.
                    let base_key = previous_raster.as_deref().map(|old| old.key);
                    let changed = previous_raster.as_deref().map_or_else(
                        || Damage::full(pixels.width(), pixels.height()),
                        |old| pixel_difference(&old.pixels, &pixels),
                    );
                    // Retain the same conservative per-entry byte cap even when
                    // identical nodes in this prepare share immutable pixels.
                    let bytes = pixels.data().len() + id.capacity() + 192;
                    let budget = self.raster_budget.get_or_insert_with(|| {
                        bead_core::cache::Budget::new("retained-cpu-rasters", LIVE_RASTER_LIMIT)
                    });
                    let charge = loop {
                        match budget.reserve(bytes) {
                            Ok(c) => break c,
                            Err(e) => {
                                let previous_variant = self
                                    .entries
                                    .iter()
                                    .filter(|(_, entry)| entry.previous.is_some())
                                    .min_by_key(|(_, entry)| entry.touched)
                                    .map(|(key, _)| key.clone());
                                if let Some(previous_variant) = previous_variant {
                                    self.entries.get_mut(&previous_variant).unwrap().previous =
                                        None;
                                    continue;
                                }
                                let victim = self
                                    .entries
                                    .iter()
                                    .filter(|(_, e)| e.touched != self.serial)
                                    .min_by_key(|(_, e)| e.touched)
                                    .map(|(k, _)| k.clone());
                                if let Some(victim) = victim {
                                    self.entries.remove(&victim);
                                } else {
                                    return Err(e.into());
                                }
                            }
                        }
                    };
                    let raster = Arc::new(Raster {
                        _charge: charge,
                        pixels,
                        origin,
                        key,
                        base_key,
                        changed,
                    });
                    let previous = if kind == "image" {
                        self.entries
                            .remove(&slot)
                            .map(|entry| entry.raster)
                            .or(previous_frame_raster)
                    } else {
                        self.entries.remove(&slot);
                        None
                    };
                    self.entries.insert(
                        slot.clone(),
                        Entry {
                            raster,
                            previous,
                            touched: self.serial,
                        },
                    );
                    if !reused {
                        self.builds += 1;
                        bead_core::vprintln!(
                            "PROFILE stage=layer_bake id={id} total_ms={:.3} builds={}",
                            started.elapsed().as_secs_f64() * 1000.,
                            self.builds
                        );
                    }
                }
                let entry = self.entries.get_mut(&slot).ok_or("layer cache missing")?;
                entry.touched = self.serial;
                let raster = entry.raster.clone();
                let previous = self
                    .frame
                    .as_ref()
                    .filter(|frame| frame.scale == scale)
                    .and_then(|frame| frame.items.get(index))
                    .filter(|item| {
                        let before = rect(&item.primitive);
                        let after = rect(primitive);
                        before.x == after.x && before.y == after.y
                    })
                    .and_then(|item| item.raster.as_ref());
                Some(
                    if let Some(previous) = previous
                        && previous.key != raster.key
                        && raster.base_key != Some(previous.key)
                    {
                        // The pixels can be cached while their comparison base is
                        // from an older frame. Refresh only frame-owned metadata.
                        let mut delta = (*raster).clone();
                        delta.base_key = Some(previous.key);
                        delta.changed = pixel_difference(&previous.pixels, &raster.pixels);
                        Arc::new(delta)
                    } else {
                        raster
                    },
                )
            };
            items.push(Item {
                image,
                id,
                primitive: {
                    let (dx, dy) = scene.content_offset(index);
                    primitive.translated(dx, dy)
                },
                clip,
                raster,
            });
        }
        let mut children = BTreeMap::<String, Vec<String>>::new();
        for layer in &scene.layers {
            if let Some(parent) = &layer.parent {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(layer.id.clone());
            }
        }
        let mut layers = Vec::with_capacity(scene.layers.len());
        for layer in &scene.layers {
            let mut key = std::collections::hash_map::DefaultHasher::new();
            let mut own = Vec::new();
            for index in layer.own.clone() {
                let item = items.get(index).ok_or("retained node range invalid")?;
                let r = rect(&item.primitive);
                for f in [r.x - layer.bounds.x, r.y - layer.bounds.y, r.w, r.h] {
                    f.to_bits().hash(&mut key);
                }
                keys[index].hash(&mut key);
                own.push(item.id.clone());
            }
            let child_ids = children.remove(&layer.id).unwrap_or_default();
            own.hash(&mut key);
            child_ids.hash(&mut key);
            layer.parent.hash(&mut key);
            let key = key.finish();
            let slot = (layer.id.clone(), scale.to_bits());
            if self.nodes.get(&slot).is_none_or(|(old, _)| old.key != key) {
                let bytes = 192
                    + 2 * layer.id.capacity()
                    + layer.parent.as_ref().map_or(0, String::capacity)
                    + (own.capacity() + child_ids.capacity()) * std::mem::size_of::<String>()
                    + own
                        .iter()
                        .chain(&child_ids)
                        .map(String::capacity)
                        .sum::<usize>();
                let budget = self.node_budget.get_or_insert_with(|| {
                    bead_core::cache::Budget::new(
                        "retained-node-metadata",
                        8 * bead_core::cache::MIB,
                    )
                });
                let charge = loop {
                    match budget.reserve(bytes) {
                        Ok(c) => break c,
                        Err(e) => {
                            let victim = self
                                .nodes
                                .iter()
                                .filter(|(_, (_, age))| *age != self.serial)
                                .min_by_key(|(_, (_, age))| *age)
                                .map(|(k, _)| k.clone());
                            if let Some(k) = victim {
                                self.nodes.remove(&k);
                            } else {
                                return Err(e.into());
                            }
                        }
                    }
                };
                self.nodes.insert(
                    slot.clone(),
                    (
                        Arc::new(RetainedLayer {
                            _charge: charge,
                            id: layer.id.clone(),
                            parent: layer.parent.clone(),
                            own,
                            children: child_ids,
                            key,
                        }),
                        self.serial,
                    ),
                );
                self.node_updates += 1;
            }
            let (retained, touched) = self.nodes.get_mut(&slot).ok_or("retained node missing")?;
            *touched = self.serial;
            layers.push(LayerPose {
                layer: retained.clone(),
                bounds: layer.bounds,
            });
        }
        while self.nodes.len() > 4096 {
            let victim = self
                .nodes
                .iter()
                .filter(|(_, (_, touched))| *touched != self.serial)
                .min_by_key(|(_, (_, touched))| *touched)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else {
                return Err("active layer tree exceeds node budget".into());
            };
            self.nodes.remove(&victim);
        }
        let bytes = items
            .iter()
            .map(|i| {
                i.id.capacity() + crate::primitive_bytes(&i.primitive) + std::mem::size_of::<Item>()
                    // Conservatively cover a frame-owned raster delta wrapper,
                    // including frames that share the cache's original wrapper.
                    + i.raster.as_ref().map_or(0, |_| std::mem::size_of::<Raster>())
            })
            .sum::<usize>()
            + layers.capacity() * std::mem::size_of::<LayerPose>()
            + assets.capacity() * std::mem::size_of::<Arc<AssetLease>>();
        validate_frame_rasters(&items)?;
        // Validate the complete candidate while the previous frame remains
        // available. Old entries may be needed for the current paint and are
        // retired only after every fallible prepare step succeeds.
        loop {
            let retained_bytes = self
                .entries
                .values()
                .map(|entry| {
                    entry.raster.pixels.data().len()
                        + entry
                            .previous
                            .as_ref()
                            .map_or(0, |previous| previous.pixels.data().len())
                })
                .sum::<usize>();
            if self.entries.len() <= 1024 && retained_bytes <= STEADY_RASTER_LIMIT {
                break;
            }
            if self.evict_previous_variant() {
                continue;
            }
            let victim = self
                .entries
                .iter()
                .filter(|(_, e)| e.touched != self.serial)
                .min_by_key(|(_, e)| e.touched)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else {
                return Err("active retained rasters exceed cache budget".into());
            };
            self.entries.remove(&victim);
        }
        let accounting = Some(
            self.frame_budget
                .get_or_insert_with(|| {
                    bead_core::cache::Budget::new(
                        "prepared-frame-metadata",
                        8 * bead_core::cache::MIB,
                    )
                })
                .reserve(bytes)?,
        );
        let frame = Arc::new(Frame {
            native_resource_budget: self.resource_budget.clone(),
            _assets: assets,
            _accounting: accounting,
            items,
            scale,
            key: frame_hash.finish(),
            layer_count: if scene.layers.is_empty() {
                scene.primitives.len()
            } else {
                scene.layers.len()
            },
            layers,
            reveal_artifact: None,
        });
        let frame = if revealing {
            self.reveal_source = Some(frame.clone());
            match self.revealed_frame(scene, &frame, scale)? {
                Some(revealed) => Arc::new(revealed),
                None => frame,
            }
        } else {
            frame
        };
        self.frame = Some(frame);
        // A scroll source already materializes its visible rows and a small
        // neighbour window. Keeping every previously visited row here defeats
        // that bound: raster and node identities survive until the much larger
        // byte/entry LRU limits are reached. Keep the current frame's entries,
        // including its one exact prior image extent, and release stale rows.
        let scale_bits = scale.to_bits();
        self.entries
            .retain(|(_, bits), entry| *bits != scale_bits || entry.touched == self.serial);
        self.nodes
            .retain(|(_, bits), (_, touched)| *bits != scale_bits || *touched == self.serial);
        Ok(())
    }
}
fn intersection(a: Damage, b: Damage) -> Damage {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Damage {
        x,
        y,
        w: (a.x + a.w).min(b.x + b.w).saturating_sub(x),
        h: (a.y + a.h).min(b.y + b.h).saturating_sub(y),
    }
}
fn fill_covers(primitive: &Primitive, offset: (f32, f32), scale: f32, clip: Damage) -> bool {
    let Primitive::Rounded {
        rect: r,
        radius,
        border,
        ..
    } = primitive
    else {
        return false;
    };
    let left = (r.x + offset.0) * scale;
    let top = (r.y + offset.1) * scale;
    let right = left + r.w * scale;
    let bottom = top + r.h * scale;
    let inset = border.as_ref().map_or(0., |(_, w)| *w * scale * 0.5) + 2.;
    let radius = radius.max(0.).min(r.w.min(r.h) * 0.5) * scale;
    let (x, y, x1, y1) = (
        clip.x as f32,
        clip.y as f32,
        (clip.x + clip.w) as f32,
        (clip.y + clip.h) as f32,
    );
    x >= left + inset
        && x1 <= right - inset
        && y >= top + inset
        && y1 <= bottom - inset
        && ((y >= top + radius + inset && y1 <= bottom - radius - inset)
            || (x >= left + radius + inset && x1 <= right - radius - inset))
}
fn opaque_start(
    frame: &Frame,
    offset: (f32, f32),
    scale: f32,
    damage: Damage,
    width: u32,
    height: u32,
) -> Option<usize> {
    frame.items.iter().rposition(|item| {
        item.raster.is_none()
            && matches!(&item.primitive,Primitive::Rounded{fill,..}if fill[3]==255)
            && fill_covers(&item.primitive, offset, scale, damage)
            && item.clip.is_none_or(|r| {
                intersection(
                    damage,
                    r.translated(offset.0, offset.1)
                        .pixels(scale, width, height),
                ) == damage
            })
    })
}
/// Paint into a borrowed SHM sub-strip: damage height limits raster traversal too.
/// Temporary pixels are repair-sized, including the curve guard, never canvas-sized
/// merely to clip output. The owner releases them after a quiet interval.
#[derive(Default)]
struct RepairScratch {
    pixels: Option<Pixmap>,
    mask: Option<Mask>,
    budget: Option<bead_core::cache::Budget>,
    charge: Option<bead_core::cache::Charge>,
    used: Option<std::time::Instant>,
    timer: bool,
}
impl RepairScratch {
    const CAP: usize = 16 * bead_core::cache::MIB;
    const IDLE: std::time::Duration = std::time::Duration::from_millis(500);
    fn charge_mask(&mut self, bytes: usize) -> Result<bead_core::cache::Charge> {
        Ok(self
            .budget
            .get_or_insert_with(|| bead_core::cache::Budget::new("cpu-repair-scratch", Self::CAP))
            .reserve(bytes)?)
    }
    fn obtain(&mut self, w: u32, h: u32) -> Result<(&mut Pixmap, &mut Mask)> {
        let size = tiny_skia::IntSize::from_wh(w, h).ok_or("empty repair scratch")?;
        let len = (w as usize)
            .checked_mul(h as usize)
            .ok_or("repair scratch overflow")?;
        let rgba = len.checked_mul(4).ok_or("repair scratch overflow")?;
        let mut pixels = self.pixels.take().map_or_else(Vec::new, Pixmap::take);
        let mut mask = self.mask.take().map_or_else(Vec::new, Mask::take);
        let total = rgba
            .max(pixels.capacity())
            .checked_add(len.max(mask.capacity()))
            .ok_or("repair scratch overflow")?;
        if total > Self::CAP {
            return Err("repair scratch exceeds budget".into());
        }
        if self.charge.as_ref().is_none_or(|c| c.bytes() != total) {
            self.charge = None;
            self.charge = Some(
                self.budget
                    .get_or_insert_with(|| {
                        bead_core::cache::Budget::new("cpu-repair-scratch", Self::CAP)
                    })
                    .reserve(total)?,
            );
        }
        if pixels.capacity() < rgba {
            pixels.reserve_exact(rgba - pixels.len());
        }
        pixels.resize(rgba, 0);
        if mask.capacity() < len {
            mask.reserve_exact(len - mask.len());
        }
        mask.resize(len, 0);
        mask.fill(0);
        self.pixels = Pixmap::from_vec(pixels, size);
        self.mask = Mask::from_vec(mask, size);
        self.used = Some(std::time::Instant::now());
        Ok((
            self.pixels.as_mut().ok_or("repair scratch pixels")?,
            self.mask.as_mut().ok_or("repair scratch mask")?,
        ))
    }
    fn release(&mut self) {
        self.pixels = None;
        self.mask = None;
        self.charge = None;
        self.budget = None;
        self.used = None;
        self.timer = false;
    }
    fn trim_idle(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        let remaining = Self::IDLE.saturating_sub(now.saturating_duration_since(self.used?));
        if remaining.is_zero() {
            self.release();
            None
        } else {
            Some(remaining)
        }
    }
}

/// The smallest pixel rectangle holding non-transparent pixels, or None if all pixels are transparent.
fn ink_bounds(pixels: &Pixmap) -> Option<Damage> {
    let (w, h) = (pixels.width() as usize, pixels.height() as usize);
    let data = pixels.data();
    let row_ink = |y: usize| {
        data[y * w * 4..(y + 1) * w * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .position(|p| p[3] != 0)
            .map(|left| {
                let right = data[y * w * 4..(y + 1) * w * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .rposition(|p| p[3] != 0)
                    .unwrap_or(left);
                (left, right)
            })
    };
    let top = (0..h).find(|&y| row_ink(y).is_some())?;
    let bottom = (top..h).rev().find(|&y| row_ink(y).is_some())?;
    let (left, right) = (top..=bottom)
        .filter_map(row_ink)
        .fold((w, 0), |(l, r), (a, b)| (l.min(a), r.max(b)));
    Some(Damage {
        x: left as u32,
        y: top as u32,
        w: (right - left + 1) as u32,
        h: (bottom - top + 1) as u32,
    })
}
fn crop_pixmap(pixels: &Pixmap, area: Damage) -> Option<Pixmap> {
    let mut out = Pixmap::new(area.w, area.h)?;
    let (sw, row) = (pixels.width() as usize, area.w as usize * 4);
    for y in 0..area.h as usize {
        let from = ((area.y as usize + y) * sw + area.x as usize) * 4;
        out.data_mut()[y * row..(y + 1) * row].copy_from_slice(&pixels.data()[from..from + row]);
    }
    Some(out)
}
/// B0117: the artwork of a small reveal without rasterising its whole length across the growth axis. Both ends
/// (corners, joins) are drawn, the middle stays uniform along that length and is one drawn line stretched
/// across. None when the artwork is too short for that or its middle is not uniform (the caller draws the
/// whole crop).
fn sliced_art(
    art: &crate::RevealArt,
    edge: bead_core::Edge,
    progress: f32,
    crop: Rect,
    scale: f32,
) -> Option<Pixmap> {
    const END: f32 = 96.;
    const MID: f32 = 16.;
    // Vertical growth leaves the width as the long side; horizontal growth the height.
    let across = edge.inward().1 != 0.;
    let (w, h) = (
        (crop.w * scale).ceil() as u32,
        (crop.h * scale).ceil() as u32,
    );
    let (end, mid) = (
        (END * scale).round() as u32,
        ((MID * scale).round() as u32).max(2),
    );
    let long = if across { w } else { h };
    if long < 2 * end + mid + 8 || w == 0 || h == 0 {
        return None;
    }
    let revealed = Scene::revealed(art.body, edge, progress);
    let slice = |at: u32, size: u32| {
        let rect = if across {
            Rect::new(
                crop.x + at as f32 / scale,
                crop.y,
                size as f32 / scale,
                crop.h,
            )
        } else {
            Rect::new(
                crop.x,
                crop.y + at as f32 / scale,
                crop.w,
                size as f32 / scale,
            )
        };
        let (_, bytes) = (art.draw)(revealed, Some(rect));
        let (sw, sh) = if across { (size, h) } else { (w, size) };
        crate::svg_pixmap(&bytes, sw, sh).ok()
    };
    let (first, last, middle) = (slice(0, end)?, slice(long - end, end)?, slice(end, mid)?);
    let pixel = |p: &Pixmap, x: u32, y: u32| {
        let i = (y * p.width() + x) as usize * 4;
        [
            p.data()[i],
            p.data()[i + 1],
            p.data()[i + 2],
            p.data()[i + 3],
        ]
    };
    let close = |a: [u8; 4], b: [u8; 4]| {
        a.iter()
            .zip(b)
            .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= 4)
    };
    let uniform = if across {
        (0..h).all(|y| close(pixel(&middle, 0, y), pixel(&middle, mid - 1, y)))
    } else {
        (0..w).all(|x| close(pixel(&middle, x, 0), pixel(&middle, x, mid - 1)))
    };
    if !uniform {
        bead_core::vprintln!("REVEAL_SLICE middle not uniform width={w} height={h}");
        return None;
    }
    let mut out = Pixmap::new(w, h)?;
    let stride = w as usize * 4;
    if across {
        let e = end as usize * 4;
        for y in 0..h as usize {
            let row = &mut out.data_mut()[y * stride..(y + 1) * stride];
            row[..e].copy_from_slice(&first.data()[y * e..(y + 1) * e]);
            row[stride - e..].copy_from_slice(&last.data()[y * e..(y + 1) * e]);
            let fill = pixel(&middle, mid / 2, y as u32);
            for chunk in row[e..stride - e].as_chunks_mut::<4>().0 {
                *chunk = fill;
            }
        }
    } else {
        let data = out.data_mut();
        data[..end as usize * stride].copy_from_slice(first.data());
        data[(h - end) as usize * stride..].copy_from_slice(last.data());
        let line = &middle.data()[(mid / 2) as usize * stride..(mid / 2 + 1) as usize * stride];
        for y in end as usize..(h - end) as usize {
            data[y * stride..(y + 1) * stride].copy_from_slice(line);
        }
    }
    Some(out)
}
/// Premultiplied source-over of a raster into a damage strip (identical results to tiny-skia's pipeline); opaque and empty source pixels short-circuit, which is most of a panel artwork.
fn blend_over(
    strip: &mut [u8],
    width: u32,
    strip_y: u32,
    clip: Damage,
    raster: &Raster,
    dx: i64,
    dy: i64,
) {
    let source_width = raster.pixels.width() as usize;
    let source = raster.pixels.data();
    for row in clip.y..clip.y + clip.h {
        let from =
            ((i64::from(row) - dy) as usize * source_width + (i64::from(clip.x) - dx) as usize) * 4;
        let to = ((row - strip_y) as usize * width as usize + clip.x as usize) * 4;
        let len = clip.w as usize * 4;
        let (source, target) = (&source[from..from + len], &mut strip[to..to + len]);
        for (s, d) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(target.as_chunks_mut::<4>().0)
        {
            match s[3] {
                255 => *d = *s,
                0 => {}
                alpha => {
                    // tiny-skia's float pipeline, bit for bit: source + destination x (1 - alpha), rounded.
                    let inverse = 1. - f32::from(alpha) / 255.;
                    for c in 0..4 {
                        d[c] = (f32::from(d[c]) * inverse + f32::from(s[c]) + 0.5) as u8;
                    }
                }
            }
        }
    }
}
pub fn paint(
    frame: &Frame,
    offset: (f32, f32),
    target: ShmTarget<'_>,
    damage: Damage,
) -> Result<()> {
    paint_with_scratch(frame, offset, target, damage, &mut RepairScratch::default())
}
fn paint_with_scratch(
    frame: &Frame,
    offset: (f32, f32),
    target: ShmTarget<'_>,
    damage: Damage,
    repair_scratch: &mut RepairScratch,
) -> Result<()> {
    if frame.items.iter().any(|item| item.image.is_some()) {
        return Err("CPU paint requires worker-materialized dense images".into());
    }
    let ShmTarget {
        bytes,
        width,
        height,
        scale,
        format,
    } = target;
    let damage = intersection(damage, Damage::full(width, height));
    if damage.w == 0 || damage.h == 0 {
        return Ok(());
    }
    let damage_mask_bytes = (width as usize)
        .checked_mul(damage.h as usize)
        .ok_or("repair mask overflow")?;
    if damage_mask_bytes > RepairScratch::CAP {
        return Err("repair mask exceeds budget".into());
    }
    let start = opaque_start(frame, offset, scale, damage, width, height);
    if start.is_none() {
        let clear_profile = bead_core::profile::Span::new("paint_clear");
        for y in damage.y..damage.y + damage.h {
            let i = (y * width + damage.x) as usize * 4;
            bytes[i..i + damage.w as usize * 4].fill(0);
        }
        drop(clear_profile);
    }
    let begin = damage.y as usize * width as usize * 4;
    let end = (damage.y + damage.h) as usize * width as usize * 4;
    let mut mask = Mask::new(width, damage.h).ok_or("damage mask")?;
    let mut mask_clip = None;
    let mut painted = false;
    for item in &frame.items[start.unwrap_or(0)..] {
        let _item_profile = bead_core::profile::Span::new(match item.primitive {
            Primitive::Arc { .. } => "paint_arc",
            Primitive::Polyline { .. } => "paint_polyline",
            Primitive::Rounded { .. } => "paint_rounded",
            Primitive::Text { .. } => "paint_text",
            Primitive::Image { .. } => "paint_image",
            Primitive::Shadow { .. } => "paint_shadow",
        });
        if item.raster.is_none()
            && matches!(&item.primitive,Primitive::Rounded{fill,border,..} if fill[3]==0 && border.as_ref().is_none_or(|(color,_)|color[3]==0))
        {
            continue;
        }
        if matches!(&item.primitive, Primitive::Arc { color, .. } if color[3] == 0) {
            continue;
        }
        if matches!(&item.primitive, Primitive::Polyline { color, fill, .. } if color[3] == 0 && fill.is_none_or(|c| c[3] == 0))
        {
            continue;
        }

        let mut clip = item.clip.map_or(damage, |r| {
            intersection(
                damage,
                r.translated(offset.0, offset.1)
                    .pixels(scale, width, height),
            )
        });
        if clip.w == 0 || clip.h == 0 {
            continue;
        }
        if let Primitive::Rounded { rect, border, .. } = &item.primitive {
            let outset = border.as_ref().map_or(0., |(_, w)| *w * 0.5) + 2. / scale;
            let bounds = Rect::new(
                rect.x - outset,
                rect.y - outset,
                rect.w + 2. * outset,
                rect.h + 2. * outset,
            )
            .translated(offset.0, offset.1)
            .pixels(scale, width, height);
            let overlap = intersection(clip, bounds);
            if overlap.w == 0 || overlap.h == 0 {
                continue;
            }
            clip = overlap;
        }
        if let Primitive::Arc {
            rect, stroke_width, ..
        } = &item.primitive
        {
            let outset = stroke_width * 0.5 + 2. / scale;
            let bounds = Rect::new(
                rect.x - outset,
                rect.y - outset,
                rect.w + 2. * outset,
                rect.h + 2. * outset,
            )
            .translated(offset.0, offset.1)
            .pixels(scale, width, height);
            let overlap = intersection(clip, bounds);
            if overlap.w == 0 || overlap.h == 0 {
                continue;
            }
            clip = overlap;
        }
        if let Some(raster) = &item.raster {
            let r = rect(&item.primitive);
            let x = ((r.x + offset.0) * scale).round() as i64 + i64::from(raster.origin.0);
            let y = ((r.y + offset.1) * scale).round() as i64 + i64::from(raster.origin.1);
            let left = i64::from(clip.x).max(x);
            let top = i64::from(clip.y).max(y);
            let right = i64::from(clip.x + clip.w).min(x + i64::from(raster.pixels.width()));
            let bottom = i64::from(clip.y + clip.h).min(y + i64::from(raster.pixels.height()));
            if right <= left || bottom <= top {
                continue;
            }
            // Raster texels outside their ink bounds are transparent. Exclude
            // them before mask setup, including rasters outside this repair.
            clip = Damage {
                x: left as u32,
                y: top as u32,
                w: (right - left) as u32,
                h: (bottom - top) as u32,
            };
        }
        // Interior coverage excludes every antialiased corner and stroke.
        // Only fully opaque interior pixels can bypass the masked blend pipeline.
        if item.raster.is_none()
            && let Primitive::Rounded { fill, .. } = &item.primitive
            && fill[3] == 255
            && fill_covers(&item.primitive, offset, scale, clip)
        {
            let mut strip = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
                .ok_or("invalid SHM strip")?;
            for row in clip.y..clip.y + clip.h {
                let start = ((row - damage.y) * width + clip.x) as usize * 4;
                for pixel in strip.data_mut()[start..start + clip.w as usize * 4]
                    .as_chunks_mut::<4>()
                    .0
                {
                    pixel.copy_from_slice(fill);
                }
            }
            painted = true;
            continue;
        }
        if !painted && let Some(raster) = &item.raster {
            // Damage was cleared above. Source-over onto zero is an exact copy,
            // including partial alpha; avoid blending a full shadow-sized image.
            let mut strip = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
                .ok_or("invalid SHM strip")?;
            let r = rect(&item.primitive);
            let dx = ((r.x + offset.0) * scale).round() as i32 + raster.origin.0 as i32;
            let dy = ((r.y + offset.1) * scale).round() as i32 + raster.origin.1 as i32;
            let x = clip.x.max(dx.max(0) as u32);
            let y = clip.y.max(dy.max(0) as u32);
            let right = (clip.x + clip.w)
                .min((i64::from(dx) + i64::from(raster.pixels.width())).max(0) as u32);
            let bottom = (clip.y + clip.h)
                .min((i64::from(dy) + i64::from(raster.pixels.height())).max(0) as u32);
            if right > x && bottom > y {
                for row in y..bottom {
                    let src = (((i64::from(row) - i64::from(dy)) as usize)
                        * raster.pixels.width() as usize
                        + (i64::from(x) - i64::from(dx)) as usize)
                        * 4;
                    let dst = ((row - damage.y) * width + x) as usize * 4;
                    let len = (right - x) as usize * 4;
                    strip.data_mut()[dst..dst + len]
                        .copy_from_slice(&raster.pixels.data()[src..src + len]);
                }
                painted = true;
            }
            continue;
        }
        // Rasters are blended into their clip rectangle directly; only drawn primitives use the mask.
        let mask_needed = item.raster.is_none();
        if mask_needed && mask_clip != Some(clip) {
            if let Some(previous) = mask_clip {
                for y in previous.y..previous.y + previous.h {
                    let i = ((y - damage.y) * width + previous.x) as usize;
                    mask.data_mut()[i..i + previous.w as usize].fill(0);
                }
            }
            for y in clip.y..clip.y + clip.h {
                let i = ((y - damage.y) * width + clip.x) as usize;
                mask.data_mut()[i..i + clip.w as usize].fill(255);
            }
            mask_clip = Some(clip);
        }
        let r = rect(&item.primitive);
        if let Some(raster) = &item.raster {
            painted = true;
            // `clip` is already inside the raster and the damage: a plain source-over of that rectangle.
            blend_over(
                &mut bytes[begin..end],
                width,
                damage.y,
                clip,
                raster,
                ((r.x + offset.0) * scale).round() as i64 + i64::from(raster.origin.0),
                ((r.y + offset.1) * scale).round() as i64 + i64::from(raster.origin.1),
            );
        } else if let Primitive::Arc {
            start_angle,
            sweep_angle,
            stroke_width,
            color,
            round_caps,
            ..
        } = &item.primitive
            && let Some(points) = crate::arc_points(r, *stroke_width, *start_angle, *sweep_angle)
        {
            let mut path = PathBuilder::new();
            path.move_to(points[0].0, points[0].1);
            for &(x, y) in &points[1..] {
                path.line_to(x, y);
            }
            if sweep_angle.abs() == 360. {
                path.close();
            }
            if let Some(path) = path.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
                let stroke = Stroke {
                    width: *stroke_width,
                    line_cap: if *round_caps {
                        tiny_skia::LineCap::Round
                    } else {
                        tiny_skia::LineCap::Butt
                    },
                    ..Stroke::default()
                };
                // Arc tessellation and antialiasing can depend on target origin
                // and dimensions. Rasterize into a deterministic, padded scratch
                // bound for every paint, then composite only the clipped damage.
                // The scratch is transient and rebuilt for each progress value.
                let paint_bounds = r
                    .translated(offset.0, offset.1)
                    .pixels(scale, width, height);
                let padding = 2;
                let x = paint_bounds.x.saturating_sub(padding);
                let y = paint_bounds.y.saturating_sub(padding);
                let right = (paint_bounds.x + paint_bounds.w)
                    .saturating_add(padding)
                    .min(width);
                let bottom = (paint_bounds.y + paint_bounds.h)
                    .saturating_add(padding)
                    .min(height);
                let (w, h) = (right - x, bottom - y);
                let (scratch, _) = repair_scratch.obtain(w, h)?;
                scratch.data_mut().fill(0);
                let transform = Transform::from_scale(scale, scale)
                    .post_translate(offset.0 * scale - x as f32, offset.1 * scale - y as f32);
                scratch.stroke_path(&path, &paint, &stroke, transform, None);
                let mut strip = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
                    .ok_or("invalid arc SHM strip")?;
                strip.draw_pixmap(
                    x as i32,
                    y as i32 - damage.y as i32,
                    scratch.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    mask_needed.then_some(&mask),
                );
                painted = true;
            }
        } else if let Primitive::Polyline {
            points,
            closed,
            stroke_width,
            color,
            round_caps,
            fill,
            ..
        } = &item.primitive
            && let Some(points) = crate::polyline_points(r, *stroke_width, points)
        {
            let mut path = PathBuilder::new();
            path.move_to(points[0].0, points[0].1);
            for &(x, y) in &points[1..] {
                path.line_to(x, y);
            }
            if *closed {
                path.close();
            }
            if let Some(path) = path.finish() {
                // As with arcs, paint the complete vector into a deterministic
                // scratch bound before clipping a partial repair strip.
                let bounds = r
                    .translated(offset.0, offset.1)
                    .pixels(scale, width, height);
                let x = bounds.x.saturating_sub(2);
                let y = bounds.y.saturating_sub(2);
                let right = (bounds.x + bounds.w).saturating_add(2).min(width);
                let bottom = (bounds.y + bounds.h).saturating_add(2).min(height);
                let (scratch, _) = repair_scratch.obtain(right - x, bottom - y)?;
                scratch.data_mut().fill(0);
                let transform = Transform::from_scale(scale, scale)
                    .post_translate(offset.0 * scale - x as f32, offset.1 * scale - y as f32);
                if let Some(fill) = fill.filter(|c| *closed && c[3] != 0) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                    scratch.fill_path(&path, &paint, FillRule::Winding, transform, None);
                }
                if *stroke_width > 0. && color[3] != 0 {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
                    scratch.stroke_path(
                        &path,
                        &paint,
                        &Stroke {
                            width: *stroke_width,
                            line_cap: if *round_caps {
                                tiny_skia::LineCap::Round
                            } else {
                                tiny_skia::LineCap::Butt
                            },
                            line_join: tiny_skia::LineJoin::Round,
                            ..Stroke::default()
                        },
                        transform,
                        None,
                    );
                }
                let mut strip = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
                    .ok_or("invalid polyline SHM strip")?;
                strip.draw_pixmap(
                    x as i32,
                    y as i32 - damage.y as i32,
                    scratch.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    mask_needed.then_some(&mask),
                );
                painted = true;
            }
        } else if let Primitive::Rounded {
            radius,
            fill,
            border,
            ..
        } = &item.primitive
            && let Some(path) = crate::rounded(r, *radius)
        {
            painted = true;
            let transform = Transform::from_scale(scale, scale)
                .post_translate(offset.0 * scale, offset.1 * scale);
            if damage == Damage::full(width, height) {
                let mut full =
                    PixmapMut::from_bytes(bytes, width, height).ok_or("rounded canvas")?;
                let mut color = Paint::default();
                color.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                full.fill_path(
                    &path,
                    &color,
                    FillRule::Winding,
                    transform,
                    mask_needed.then_some(&mask),
                );
                if let Some((rgba, line)) = border {
                    color.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                    full.stroke_path(
                        &path,
                        &color,
                        &Stroke {
                            width: *line,
                            ..Stroke::default()
                        },
                        transform,
                        mask_needed.then_some(&mask),
                    );
                }
            } else {
                // Match `crate::rounded`: the effective corner radius cannot exceed
                // either half-dimension, even when a theme uses a pill radius like 9999.
                let radius = radius.min(r.w * 0.5).min(r.h * 0.5).max(0.);
                let padding = ((radius * scale).ceil() as u32)
                    .saturating_add(
                        border
                            .as_ref()
                            .map_or(0, |(_, w)| (*w * scale).ceil() as u32),
                    )
                    .saturating_add(4);
                let x = clip.x.saturating_sub(padding);
                let y = clip.y.saturating_sub(padding);
                let right = (clip.x + clip.w).saturating_add(padding).min(width);
                let bottom = (clip.y + clip.h).saturating_add(padding).min(height);
                let (w, h) = (right - x, bottom - y);
                match repair_scratch.obtain(w, h) {
                    Ok((scratch, scratch_mask)) => {
                        for row in y..bottom {
                            let source = ((row * width + x) * 4) as usize;
                            let dest = ((row - y) * w * 4) as usize;
                            scratch.data_mut()[dest..dest + w as usize * 4]
                                .copy_from_slice(&bytes[source..source + w as usize * 4]);
                        }
                        for row in clip.y..clip.y + clip.h {
                            let start = ((row - y) * w + clip.x - x) as usize;
                            scratch_mask.data_mut()[start..start + clip.w as usize].fill(255);
                        }
                        let transform = Transform::from_scale(scale, scale).post_translate(
                            offset.0 * scale - x as f32,
                            offset.1 * scale - y as f32,
                        );
                        let mut color = Paint::default();
                        color.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                        scratch.fill_path(
                            &path,
                            &color,
                            FillRule::Winding,
                            transform,
                            Some(&*scratch_mask),
                        );
                        if let Some((rgba, line)) = border {
                            color.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                            scratch.stroke_path(
                                &path,
                                &color,
                                &Stroke {
                                    width: *line,
                                    ..Stroke::default()
                                },
                                transform,
                                Some(&*scratch_mask),
                            );
                        }
                        for row in clip.y..clip.y + clip.h {
                            let source = (((row - y) * w + clip.x - x) * 4) as usize;
                            let dest = ((row * width + clip.x) * 4) as usize;
                            bytes[dest..dest + clip.w as usize * 4].copy_from_slice(
                                &scratch.data()[source..source + clip.w as usize * 4],
                            );
                        }
                    }
                    Err(_) => {
                        // A curve guard can span most of the surface. Drop both the
                        // reusable RGBA scratch and strip mask, then paint directly into
                        // SHM with a charged full-canvas mask so tiny-skia sees the full
                        // curve while writes remain restricted to this clip.
                        drop(std::mem::replace(
                            &mut mask,
                            Mask::new(1, 1).ok_or("repair mask placeholder")?,
                        ));
                        repair_scratch.release();
                        let full_mask_bytes = (width as usize)
                            .checked_mul(height as usize)
                            .ok_or("repair mask overflow")?;
                        let mask_charge = repair_scratch.charge_mask(full_mask_bytes)?;
                        let mut full_mask = Mask::new(width, height).ok_or("full repair mask")?;
                        for row in clip.y..clip.y + clip.h {
                            let start = (row * width + clip.x) as usize;
                            full_mask.data_mut()[start..start + clip.w as usize].fill(255);
                        }
                        {
                            let mut full = PixmapMut::from_bytes(bytes, width, height)
                                .ok_or("full SHM repair target")?;
                            let transform = Transform::from_scale(scale, scale)
                                .post_translate(offset.0 * scale, offset.1 * scale);
                            let mut color = Paint::default();
                            color.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                            full.fill_path(
                                &path,
                                &color,
                                FillRule::Winding,
                                transform,
                                Some(&full_mask),
                            );
                            if let Some((rgba, line)) = border {
                                color.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                                full.stroke_path(
                                    &path,
                                    &color,
                                    &Stroke {
                                        width: *line,
                                        ..Stroke::default()
                                    },
                                    transform,
                                    Some(&full_mask),
                                );
                            }
                        }
                        drop(full_mask);
                        drop(mask_charge);
                        mask = Mask::new(width, damage.h).ok_or("restored damage mask")?;
                        mask_clip = None;
                    }
                }
            }
        }
    }
    let _format_profile = bead_core::profile::Span::new("paint_format");
    if format == PixelFormat::Bgra {
        for y in damage.y..damage.y + damage.h {
            for x in damage.x..damage.x + damage.w {
                bytes[(y * width + x) as usize * 4..(y * width + x) as usize * 4 + 4].swap(0, 2);
            }
        }
    }
    Ok(())
}
/// Opaque composition can be compared without changing scene preparation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpaqueComposition {
    /// Measured direct opaque composition; alpha fades still reuse a group.
    #[default]
    Auto,
    Direct,
    Cached,
}
/// Immutable CPU fallback work, run only by the resource actor.
pub struct CpuPreparation {
    scene: Scene,
    scale: f32,
    fingerprint: SnapshotFingerprint,
    queued: std::sync::atomic::AtomicBool,
    result: std::sync::Mutex<Option<std::result::Result<PreparedCpuSnapshot, String>>>,
}
impl CpuPreparation {
    pub fn mark_requested(&self) -> bool {
        !self.queued.swap(true, std::sync::atomic::Ordering::AcqRel)
    }
    pub fn prepare(&self) -> std::result::Result<(), String> {
        let result = TreeCpuRenderer::prepare_snapshot(&self.scene, self.scale)
            .map_err(|error| error.to_string());
        let status = result.as_ref().map(|_| ()).map_err(Clone::clone);
        *self.result.lock().map_err(|_| "CPU preparation poisoned")? = Some(result);
        status
    }
}
#[derive(Debug)]
pub struct ResourcePreparationFailed(pub String);
impl std::fmt::Display for ResourcePreparationFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ResourcePreparationFailed {}
/// Worker-only intermediate: it cannot paint before retained-cache admission.
pub struct PreparedCpuSnapshot {
    renderer: TreeCpuRenderer,
    native_budget: Option<bead_core::cache::Budget>,
    release_cache: bool,
}
impl PreparedCpuSnapshot {
    pub fn accept(mut self) -> Result<TreeCpuRenderer> {
        if let Some(budget) = self.native_budget {
            let bytes = self
                .renderer
                .flattened
                .as_ref()
                .map_or(0, |(_, _, pixels)| pixels.bytes());
            match budget.reserve(bytes + 64) {
                Ok(charge) => self.renderer.group_charge = Some(charge),
                Err("retained cache byte budget exceeded") => {
                    // A prepared GPU picker can already occupy its full native view
                    // allowance. Keep the prepared CPU frame and paint it directly
                    // rather than turning an optional flat-composite cache into a UI error.
                    self.renderer.flattened = None;
                    self.renderer.group_charge = None;
                    self.renderer.snapshot_fingerprint = None;
                    self.renderer.opaque_composition = OpaqueComposition::Direct;
                    self.renderer.group_budget = Some(budget);
                    return Ok(self.renderer);
                }
                Err(error) => return Err(error.into()),
            }
            self.renderer.group_budget = Some(budget);
        }
        if self.release_cache {
            self.renderer.cache.release_pixel_storage();
        }
        Ok(self.renderer)
    }
}
pub struct CpuResourcesPending {
    pub request: Arc<CpuPreparation>,
}
impl std::fmt::Debug for CpuResourcesPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CpuResourcesPending")
    }
}
impl std::fmt::Display for CpuResourcesPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CPU worker preparation pending")
    }
}
impl std::error::Error for CpuResourcesPending {}
#[derive(Default)]
pub struct TreeCpuRenderer {
    pub opaque_composition: OpaqueComposition,
    pub cache: TreeCache,
    /// Number of content-group composites; pose/opacity changes reuse each group.
    pub opacity_builds: usize,
    /// Opaque frames compose primitives directly rather than reuse an opacity group.
    pub direct_composites: usize,
    repair_scratch: RepairScratch,
    flattened: Option<(u64, f32, FlattenedPixels)>,
    group_budget: Option<bead_core::cache::Budget>,
    group_charge: Option<bead_core::cache::Charge>,
    keep_hidden_group: bool,
    sparse_snapshot: bool,
    snapshot_fingerprint: Option<SnapshotFingerprint>,
    snapshot_layers: usize,
    cpu_preparation: Option<Arc<CpuPreparation>>,
    worker_fingerprint: Option<SnapshotFingerprint>,
    active_reveal: Option<ActiveReveal>,
    warm_reveal: Option<(u64, Arc<PreparedReveal>)>,
}
struct ActiveReveal {
    motion_id: String,
    open: bool,
    artifact: Arc<PreparedReveal>,
}
enum FlattenedPixels {
    Dense(Pixmap),
    Sparse(SparsePixels),
}
struct SparsePixels {
    width: u32,
    height: u32,
    rows: Vec<usize>,
    words: Vec<u32>,
}
impl FlattenedPixels {
    fn bytes(&self) -> usize {
        match self {
            Self::Dense(pixels) => pixels.data().len(),
            Self::Sparse(sparse) => sparse.bytes(),
        }
    }
}
impl TreeCpuRenderer {
    fn pin_reveal(&mut self, scene: &Scene, scale: f32) -> bool {
        let Some(motion) = scene.motion.as_ref().filter(|motion| motion.reveal) else {
            self.active_reveal = None;
            return false;
        };
        if let Some(active) = &self.active_reveal
            && active.motion_id == motion.id
            && active.open == motion.open
            && active.artifact.scale().to_bits() == scale.to_bits()
        {
            return true;
        }
        self.active_reveal = None;
        let prepared = scene.prepared.get(&scale.to_bits());
        let candidate = prepared
            .and_then(|frame| {
                frame
                    .reveal_artifact
                    .as_ref()
                    .map(|artifact| (frame.key, artifact.clone()))
            })
            .or_else(|| {
                self.cache.frame.as_ref().and_then(|frame| {
                    frame
                        .reveal_artifact
                        .as_ref()
                        .map(|artifact| (frame.key, artifact.clone()))
                })
            });
        if let Some((frame_key, artifact)) =
            candidate.filter(|(_, artifact)| artifact.scale().to_bits() == scale.to_bits())
        {
            self.warm_reveal = Some((frame_key, artifact.clone()));
            self.active_reveal = Some(ActiveReveal {
                motion_id: motion.id.clone(),
                open: motion.open,
                artifact,
            });
            return true;
        }
        let frame_key = prepared
            .map(|frame| frame.key)
            .or_else(|| self.cache.frame.as_ref().map(|frame| frame.key));
        if let Some(frame_key) = frame_key
            && let Some((warm_key, artifact)) = &self.warm_reveal
            && *warm_key == frame_key
            && artifact.scale().to_bits() == scale.to_bits()
        {
            self.active_reveal = Some(ActiveReveal {
                motion_id: motion.id.clone(),
                open: motion.open,
                artifact: artifact.clone(),
            });
            return true;
        }
        false
    }

    /// Build the immutable open-pose image for a reveal on the resource worker.
    /// Callers retain the returned artifact across repeated opens.
    pub fn prepare_reveal_artifact(scene: &Scene, scale: f32) -> Result<PreparedReveal> {
        if !scale.is_finite() || scale <= 0. {
            return Err("invalid reveal scale".into());
        }
        if !scene.motion.as_ref().is_some_and(|motion| motion.reveal) {
            return Err("scene is not a reveal".into());
        }
        let mut open = cpu_content_scene(scene);
        open.opacity = 1.;
        open.offset = (0., 0.);
        open.prepared.clear();
        open.reveal(1.);
        let mut cache = TreeCache::default();
        cache.prepare(&open, scale)?;
        let frame = cache
            .frame
            .as_ref()
            .ok_or("prepared reveal frame missing")?;
        PreparedReveal::from_frame(frame, &open, scale)
    }

    /// Freeze prepared CPU content for immediate painting after retiring a GPU backend.
    /// Position and opacity remain live; the cached group also survives the hidden pose.
    pub fn snapshot(scene: &Scene, scale: f32) -> Result<Self> {
        Self::prepare_snapshot(scene, scale)?.accept()
    }
    pub fn prepare_snapshot(scene: &Scene, scale: f32) -> Result<PreparedCpuSnapshot> {
        let content = cpu_content_scene(scene);
        let mut renderer = Self::with_opaque_composition(OpaqueComposition::Cached);
        renderer.keep_hidden_group = true;
        renderer.sparse_snapshot = true;
        // Dense intermediates live only on this worker until the flat snapshot
        // is complete; the finished allocation rejoins the native view ceiling.
        let native_budget = scene
            .prepared
            .get(&scale.to_bits())
            .and_then(|frame| frame.native_resource_budget.clone());
        if native_budget.is_some() {
            let scratch =
                bead_core::cache::Budget::new("cpu-snapshot-scratch", 128 * bead_core::cache::MIB);
            renderer.cache.raster_budget = Some(scratch.clone());
            renderer.cache.node_budget = Some(scratch.clone());
            renderer.cache.frame_budget = Some(scratch.clone());
            renderer.group_budget = Some(scratch);
        }

        let reveal = scene.motion.as_ref().is_some_and(|motion| motion.reveal);
        let mut worker_content = content.clone();
        if reveal {
            worker_content.prepared.clear();
            worker_content.reveal(1.);
        }
        renderer.prepare(&worker_content, scale)?;
        if reveal {
            renderer.cache.attach_reveal_artifact(scene, scale)?;
            renderer.opaque_composition = OpaqueComposition::Direct;
        } else {
            renderer.cache_content(&worker_content)?;
        }
        if !reveal {
            let fingerprint = SnapshotFingerprint::new(&content, scale)?;
            renderer.snapshot_layers = renderer
                .cache
                .frame
                .as_ref()
                .map_or(0, |frame| frame.layer_count);
            renderer.snapshot_fingerprint = Some(fingerprint);
        }
        Ok(PreparedCpuSnapshot {
            renderer,
            native_budget,
            release_cache: !scene.motion.as_ref().is_some_and(|motion| motion.reveal),
        })
    }

    fn cache_content(&mut self, scene: &Scene) -> Result<()> {
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        if self
            .flattened
            .as_ref()
            .is_some_and(|(key, scale, _)| *key == frame.key && *scale == frame.scale)
        {
            return Ok(());
        }
        let (w, h) = (
            (scene.bounds.w * frame.scale).ceil() as u32,
            (scene.bounds.h * frame.scale).ceil() as u32,
        );
        let full_bytes = u64::from(w) * u64::from(h) * 4;
        if full_bytes > 64 * 1024 * 1024 {
            return Err("opacity group exceeds budget".into());
        }
        self.flattened = None;
        self.group_charge = None;
        let full_charge = self
            .group_budget
            .get_or_insert_with(|| {
                bead_core::cache::Budget::new("cpu-composite", 64 * bead_core::cache::MIB)
            })
            .reserve(w as usize * h as usize * 4 + 64)?;
        let mut pixels = Pixmap::new(w, h).ok_or("opacity group")?;
        paint(
            frame,
            (-scene.bounds.x, -scene.bounds.y),
            ShmTarget {
                bytes: pixels.data_mut(),
                width: w,
                height: h,
                scale: frame.scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(w, h),
        )?;
        let pixels = if self.sparse_snapshot {
            compress_rows(pixels)
        } else {
            FlattenedPixels::Dense(pixels)
        };
        if self.sparse_snapshot {
            bead_core::vprintln!(
                "CPU_SNAPSHOT full_bytes={full_bytes} retained_bytes={}",
                pixels.bytes()
            );
        }
        drop(full_charge);
        self.group_charge = Some(
            self.group_budget
                .get_or_insert_with(|| {
                    bead_core::cache::Budget::new("cpu-composite", 64 * bead_core::cache::MIB)
                })
                .reserve(pixels.bytes() + 64)?,
        );
        self.opacity_builds += 1;
        self.flattened = Some((frame.key, frame.scale, pixels));
        Ok(())
    }
    pub fn with_opaque_composition(mode: OpaqueComposition) -> Self {
        Self {
            opaque_composition: mode,
            ..Self::default()
        }
    }
}
impl Renderer for TreeCpuRenderer {
    fn take_transient_timer(&mut self) -> bool {
        if self.repair_scratch.used.is_none() || self.repair_scratch.timer {
            return false;
        }
        self.repair_scratch.timer = true;
        true
    }
    fn trim_transient_idle(&mut self) -> Option<std::time::Duration> {
        self.repair_scratch.trim_idle(std::time::Instant::now())
    }
    fn prepared_frame(&self) -> Option<std::sync::Arc<crate::tree::Frame>> {
        self.cache.frame.clone()
    }
    fn resource_counts(&self) -> (usize, usize) {
        (
            self.cache
                .frame
                .as_ref()
                .map_or(self.snapshot_layers, |frame| frame.layer_count),
            0,
        )
    }
    fn reusable_scene(&self, scene: &Scene, scale: f32) -> Result<bool> {
        if let Some(fingerprint) = self.worker_fingerprint.as_ref() {
            return fingerprint.matches_content(scene, scale);
        }
        if let Some(fingerprint) = self.snapshot_fingerprint.as_ref() {
            return fingerprint.matches_snapshot(scene, scale);
        }
        Ok(false)
    }
    fn prepare_on_ui(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        let revealing = scene
            .motion
            .as_ref()
            .is_some_and(|motion| motion.reveal && motion.progress < 1.);
        if revealing {
            if self.pin_reveal(scene, scale) {
                return Ok(());
            }
        } else if scene
            .motion
            .as_ref()
            .is_none_or(|motion| !motion.reveal || motion.progress >= 1.)
        {
            self.active_reveal = None;
        }
        if self.reusable_scene(scene, scale)? {
            // The selected immutable artifact owns all reveal pixels; no pose frame is built here.
            if self.worker_fingerprint.is_some()
                && let Some(motion) = scene.motion.as_ref().filter(|motion| motion.reveal)
            {
                if motion.progress < 1. && self.pin_reveal(scene, scale) {
                    return Ok(());
                }
                if motion.progress < 1. {
                    return Err("worker reveal artifact missing".into());
                }
            }
            return Ok(());
        }
        let dense = scene.prepared.get(&scale.to_bits()).is_some_and(|frame| {
            prepared_rasters_match_visibility(scene, frame)
                && frame.items.iter().all(|item| item.image.is_none())
        }) && !scene.motion.as_ref().is_some_and(|motion| {
            motion.reveal
                && motion.progress < 1.
                && motion
                    .parts
                    .iter()
                    .any(|(_, primitive)| matches!(primitive, Primitive::Image { .. }))
        });
        if dense && !revealing {
            return self.prepare(scene, scale);
        }
        if self.snapshot_fingerprint.is_none()
            && self.worker_fingerprint.is_none()
            && !revealing
            && !scene
                .primitives
                .iter()
                .any(|p| matches!(p, Primitive::Image { .. }))
        {
            return self.prepare(scene, scale);
        }
        if self.cpu_preparation.as_ref().is_none_or(|request| {
            !request
                .fingerprint
                .matches_content(scene, scale)
                .unwrap_or(false)
        }) {
            self.cpu_preparation = Some(Arc::new(CpuPreparation {
                scene: scene.clone(),
                scale,
                fingerprint: SnapshotFingerprint::new(scene, scale)?,
                queued: Default::default(),
                result: Default::default(),
            }));
        }
        let request = self.cpu_preparation.as_ref().unwrap().clone();
        let ready = {
            let mut result = request
                .result
                .lock()
                .map_err(|_| "CPU preparation poisoned")?;
            match result.as_ref() {
                Some(Err(error)) => return Err(Box::new(ResourcePreparationFailed(error.clone()))),
                Some(Ok(_)) => result.take(),
                None => None,
            }
        };
        if let Some(Ok(mut prepared)) = ready {
            // Keep the prepared frame: reveals animate it instead of a frozen composite.
            prepared.release_cache = false;
            let mut renderer = prepared.accept()?;
            renderer.snapshot_fingerprint = None;
            renderer.worker_fingerprint = Some(SnapshotFingerprint::new(scene, scale)?);
            if let Some(motion) = scene.motion.as_ref().filter(|motion| motion.reveal)
                && motion.progress < 1.
                && !renderer.pin_reveal(scene, scale)
            {
                return Err("worker reveal artifact missing".into());
            }
            *self = renderer;
            return Ok(());
        }
        Err(Box::new(CpuResourcesPending { request }))
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if let Some(snapshot) = &self.snapshot_fingerprint {
            if snapshot.matches_snapshot(scene, scale)? {
                return Ok(());
            }

            // Build replacement content off to the side. A failed reprepare
            // keeps the last accepted sparse snapshot available for paint.
            let fingerprint = (!scene.motion.as_ref().is_some_and(|motion| motion.reveal))
                .then(|| SnapshotFingerprint::new(scene, scale))
                .transpose()?;
            let mut replacement = Self::with_opaque_composition(OpaqueComposition::Cached);
            replacement.keep_hidden_group = self.keep_hidden_group;
            replacement.sparse_snapshot = self.sparse_snapshot;
            replacement.opacity_builds = self.opacity_builds;
            replacement.direct_composites = self.direct_composites;
            replacement.cache.prepare(scene, scale)?;
            replacement.cache_content(scene)?;
            if let Some(fingerprint) = fingerprint {
                replacement.snapshot_layers = replacement
                    .cache
                    .frame
                    .as_ref()
                    .map_or(0, |frame| frame.layer_count);
                replacement.cache.release_pixel_storage();
                replacement.snapshot_fingerprint = Some(fingerprint);
            }
            *self = replacement;
            return Ok(());
        }
        self.cache.prepare(scene, scale)
    }
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, damage: Damage) -> Result<()> {
        if damage
            .x
            .checked_add(damage.w)
            .is_none_or(|x| x > target.width)
            || damage
                .y
                .checked_add(damage.h)
                .is_none_or(|y| y > target.height)
        {
            return Err("damage outside SHM".into());
        }
        if scene
            .motion
            .as_ref()
            .is_some_and(|motion| motion.reveal && motion.progress < 1.)
        {
            let width = target.width;
            {
                let bytes = &mut *target.bytes;
                for y in damage.y..damage.y + damage.h {
                    let start = (y * width + damage.x) as usize * 4;
                    bytes[start..start + damage.w as usize * 4].fill(0);
                }
            }
            if scene.opacity <= 0. {
                return Ok(());
            }
            let Some(artifact) = self
                .active_reveal
                .as_ref()
                .map(|active| &active.artifact)
                .filter(|artifact| artifact.scale().to_bits() == target.scale.to_bits())
            else {
                return Ok(());
            };
            let window = scene.reveal_window().ok_or("reveal window missing")?;
            let slide_offset = scene.content_offset(usize::MAX);
            let picture_offset = (
                scene.offset.0 + slide_offset.0,
                scene.offset.1 + slide_offset.1,
            );
            let window = window.translated(scene.offset.0, scene.offset.1);
            return artifact.composite_with_opacity(
                target,
                damage,
                picture_offset,
                window,
                scene.opacity,
            );
        }
        let frozen_snapshot = self.snapshot_fingerprint.is_some();
        let frame = if frozen_snapshot {
            None
        } else {
            Some(self.cache.frame.as_ref().ok_or("unprepared layer tree")?)
        };
        let cached_opaque = match self.opaque_composition {
            OpaqueComposition::Auto => false,
            OpaqueComposition::Direct => false,
            OpaqueComposition::Cached => true,
        };
        if !frozen_snapshot && scene.opacity >= 1. && !cached_opaque {
            self.direct_composites += 1;
            self.flattened = None;
            self.group_charge = None;
            return paint_with_scratch(
                frame.expect("non-snapshot frame checked"),
                scene.offset,
                target,
                damage,
                &mut self.repair_scratch,
            );
        }
        if scene.opacity <= 0. {
            if !self.keep_hidden_group {
                self.flattened = None;
                self.group_charge = None;
            }
            let ShmTarget { bytes, width, .. } = target;
            for y in damage.y..damage.y + damage.h {
                let i = (y * width + damage.x) as usize * 4;
                bytes[i..i + damage.w as usize * 4].fill(0);
            }
            return Ok(());
        }
        // Group opacity preserves previous semantics; compose cached local rasters,
        // never reload fonts or re-bake glyphs for an animated pose.
        if !frozen_snapshot {
            self.cache_content(scene)?;
        }
        let (_, _, pixels) = self.flattened.as_ref().ok_or("opacity group missing")?;
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        let origin = (
            ((scene.offset.0 + scene.bounds.x) * scale).round() as i32,
            ((scene.offset.1 + scene.bounds.y) * scale).round() as i32,
        );
        match pixels {
            FlattenedPixels::Dense(pixels) => blit_with_opacity(
                bytes,
                (width, height),
                damage,
                pixels.as_ref(),
                origin,
                scene.opacity.clamp(0., 1.),
                format == PixelFormat::Bgra,
            ),
            FlattenedPixels::Sparse(pixels) => pixels.blit(
                bytes,
                (width, height),
                damage,
                origin,
                scene.opacity.clamp(0., 1.),
                format == PixelFormat::Bgra,
            ),
        }
        Ok(())
    }
}

/// Store exact repeated pixels compactly, including transparent and dimmer borders.
fn compress_rows(pixels: Pixmap) -> FlattenedPixels {
    let (width, height) = (pixels.width(), pixels.height());
    // Compression is an optional CPU snapshot optimization. Keep its per-job
    // scan bounded; large wallpaper and picker snapshots stay dense instead of
    // monopolizing the resource worker while scanning millions of pixels.
    const MAX_COMPRESSION_PIXELS: u64 = 4 * 1024 * 1024;
    if u64::from(width) * u64::from(height) > MAX_COMPRESSION_PIXELS {
        return FlattenedPixels::Dense(pixels);
    }
    let mut rows = Vec::with_capacity(height as usize + 1);
    let mut words = Vec::new();
    for y in 0..height as usize {
        rows.push(words.len());
        let row = &pixels.data()[y * width as usize * 4..(y + 1) * width as usize * 4];
        let pixel = |x: usize| u32::from_ne_bytes(row[x * 4..x * 4 + 4].try_into().unwrap());
        let mut x = 0;
        while x < width as usize {
            let mut run = 1;
            while x + run < width as usize && pixel(x + run) == pixel(x) {
                run += 1;
            }
            if run >= 3 {
                words.extend([0x8000_0000 | run as u32, pixel(x)]);
                x += run;
            } else {
                let start = x;
                x += run;
                while x < width as usize {
                    let mut next = 1;
                    while x + next < width as usize && pixel(x + next) == pixel(x) {
                        next += 1;
                    }
                    if next >= 3 {
                        break;
                    }
                    x += next;
                }
                words.push((x - start) as u32);
                words.extend((start..x).map(pixel));
            }
        }
    }
    rows.push(words.len());
    rows.shrink_to_fit();
    words.shrink_to_fit();
    let sparse = SparsePixels {
        width,
        height,
        rows,
        words,
    };
    if sparse.bytes() >= pixels.data().len() {
        FlattenedPixels::Dense(pixels)
    } else {
        FlattenedPixels::Sparse(sparse)
    }
}
impl SparsePixels {
    fn bytes(&self) -> usize {
        self.words.capacity() * 4
            + self.rows.capacity() * std::mem::size_of::<usize>()
            + std::mem::size_of::<Self>()
    }
    fn blit(
        &self,
        bytes: &mut [u8],
        size: (u32, u32),
        damage: Damage,
        origin: (i32, i32),
        opacity: f32,
        swap_red_blue: bool,
    ) {
        let (width, height) = size;
        let alpha = u32::from((opacity * 255.).round() as u8);
        let scale = |v: u8| (((u32::from(v) * alpha + 128) * 257) >> 16) as u8;
        for y in damage.y.min(height)..(damage.y + damage.h).min(height) {
            let x0 = damage.x.min(width) as i64;
            let x1 = (damage.x + damage.w).min(width) as i64;
            let out = &mut bytes[(y as usize * width as usize + x0 as usize) * 4
                ..(y as usize * width as usize + x1 as usize) * 4];
            out.fill(0);
            let sy = i64::from(y) - i64::from(origin.1);
            if sy < 0 || sy >= i64::from(self.height) {
                continue;
            }
            let sx0 = x0.max(i64::from(origin.0)).max(0) - i64::from(origin.0);
            let sx1 = x1
                .min(i64::from(origin.0) + i64::from(self.width))
                .min(i64::from(width))
                - i64::from(origin.0);
            let (mut x, mut index) = (0_i64, self.rows[sy as usize]);
            while index < self.rows[sy as usize + 1] {
                let command = self.words[index];
                index += 1;
                let repeat = command & 0x8000_0000 != 0;
                let len = (command & 0x7fff_ffff) as i64;
                let start = x.max(sx0);
                let end = (x + len).min(sx1);
                if start < end {
                    if repeat {
                        let [r, g, b, a] = self.words[index].to_ne_bytes();
                        let color = [
                            if swap_red_blue { scale(b) } else { scale(r) },
                            scale(g),
                            if swap_red_blue { scale(r) } else { scale(b) },
                            scale(a),
                        ];
                        let from = (y as usize * width as usize
                            + (i64::from(origin.0) + start) as usize)
                            * 4;
                        let to = from + (end - start) as usize * 4;
                        bytes[from..to].as_chunks_mut::<4>().0.fill(color);
                    } else {
                        for sx in start..end {
                            let [r, g, b, a] = self.words[index + (sx - x) as usize].to_ne_bytes();
                            let dst = (y as usize * width as usize
                                + (i64::from(origin.0) + sx) as usize)
                                * 4;
                            bytes[dst..dst + 4].copy_from_slice(&[
                                if swap_red_blue { scale(b) } else { scale(r) },
                                scale(g),
                                if swap_red_blue { scale(r) } else { scale(b) },
                                scale(a),
                            ]);
                        }
                    }
                }
                if repeat {
                    index += 1;
                } else {
                    index += len as usize;
                }
                x += len;
            }
        }
    }
}
/// Write premultiplied pixels into cleared damage, with opacity and optional BGRA swap.
fn blit_with_opacity(
    bytes: &mut [u8],
    (width, height): (u32, u32),
    damage: Damage,
    pixels: tiny_skia::PixmapRef<'_>,
    (dx, dy): (i32, i32),
    opacity: f32,
    swap_red_blue: bool,
) {
    let alpha = u32::from((opacity * 255.).round() as u8);
    let (src_w, src_h) = (pixels.width() as i64, pixels.height() as i64);
    let src = pixels.data();
    for y in damage.y.min(height)..(damage.y + damage.h).min(height) {
        let row = y as usize * width as usize;
        let x0 = damage.x.min(width) as usize;
        let x1 = (damage.x + damage.w).min(width) as usize;
        let out = &mut bytes[(row + x0) * 4..(row + x1) * 4];
        let sy = i64::from(y) - i64::from(dy);
        // Source columns covering target columns x0..x1.
        let first = (i64::from(dx) - x0 as i64).clamp(0, (x1 - x0) as i64) as usize;
        let last =
            (i64::from(dx) + src_w - x0 as i64).clamp(first as i64, (x1 - x0) as i64) as usize;
        if sy < 0 || sy >= src_h || first == last {
            out.fill(0);
            continue;
        }
        out[..first * 4].fill(0);
        out[last * 4..].fill(0);
        let from = ((sy * src_w + (x0 + first) as i64 - i64::from(dx)) * 4) as usize;
        let source = &src[from..from + (last - first) * 4];
        for (d, s) in out[first * 4..last * 4]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(source.as_chunks::<4>().0)
        {
            let scale = |v: u8| (((u32::from(v) * alpha + 128) * 257) >> 16) as u8;
            let (r, b) = (scale(s[0]), scale(s[2]));
            d[0] = if swap_red_blue { b } else { r };
            d[1] = scale(s[1]);
            d[2] = if swap_red_blue { r } else { b };
            d[3] = scale(s[3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_snapshot_accept_falls_back_when_native_view_budget_is_full() {
        let scene = scene();
        let mut cache = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        cache.prepare(&scene, 1.).unwrap();
        let mut scene = scene;
        scene
            .prepared
            .insert(1_f32.to_bits(), cache.frame.as_ref().unwrap().clone());

        let prepared = TreeCpuRenderer::prepare_snapshot(&scene, 1.).unwrap();
        let budget = prepared.native_budget.as_ref().unwrap();
        let remaining = 40 * bead_core::cache::MIB - budget.used();
        let _fill = budget.reserve(remaining).unwrap();
        let renderer = prepared
            .accept()
            .expect("optional flattened snapshot must fall back to CPU layers");
        assert!(renderer.flattened.is_none());
        assert!(renderer.group_charge.is_none());
        assert!(renderer.cache.frame.is_some());
        assert!(renderer.snapshot_fingerprint.is_none());
    }

    #[test]
    fn raster_blend_matches_the_masked_pixmap_composite() {
        let mut source = Pixmap::new(24, 12).unwrap();
        for (i, px) in source
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let a = match i % 5 {
                0 => 0,
                1 => 255,
                _ => (i * 29 % 256) as u8,
            };
            *px = [
                (i * 5 % 256) as u8 % a.max(1),
                (i * 3 % 256) as u8 % a.max(1),
                (i % 256) as u8 % a.max(1),
                a,
            ];
        }
        let budget = bead_core::cache::Budget::new("blend-test", 1 << 20);
        let raster = Raster {
            pixels: Arc::new(source.clone()),
            origin: (0, 0),
            key: 1,
            base_key: None,
            changed: Damage::full(24, 12),
            _charge: budget.reserve(source.data().len()).unwrap(),
        };
        let (width, rows) = (40u32, 20u32);
        let clip = Damage {
            x: 6,
            y: 3,
            w: 15,
            h: 7,
        };
        let (dx, dy) = (4i64, 2i64);
        let background: Vec<u8> = (0..width * rows * 4)
            .map(|i| (i * 11 % 251) as u8)
            .collect();
        let mut expected = background.clone();
        let mut mask = Mask::new(width, rows).unwrap();
        for y in clip.y..clip.y + clip.h {
            let i = (y * width + clip.x) as usize;
            mask.data_mut()[i..i + clip.w as usize].fill(255);
        }
        PixmapMut::from_bytes(&mut expected, width, rows)
            .unwrap()
            .draw_pixmap(
                dx as i32,
                dy as i32,
                source.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        let mut actual = background;
        blend_over(&mut actual, width, 0, clip, &raster, dx, dy);
        let worst = expected
            .iter()
            .zip(&actual)
            .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
            .max()
            .unwrap();
        assert_eq!(worst, 0, "blend differs from tiny-skia");
    }
    #[test]
    fn opacity_blit_matches_the_masked_pixmap_composite_and_swaps_channels() {
        let mut source = Pixmap::new(20, 10).unwrap();
        for (i, px) in source
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            let a = (i * 13 % 256) as u8;
            *px = [
                (i as u8).min(a),
                (i * 7 % 256) as u8 % a.max(1),
                (i * 3 % 256) as u8 % a.max(1),
                a,
            ];
        }
        let (width, height) = (40, 30);
        let damage = Damage {
            x: 3,
            y: 4,
            w: 30,
            h: 20,
        };
        let origin = (8, 9);
        for opacity in [0.25, 0.5, 1.0] {
            let mut expected = vec![0xAA_u8; (width * height * 4) as usize];
            let mut mask = Mask::new(width, height).unwrap();
            for y in damage.y..damage.y + damage.h {
                let i = (y * width + damage.x) as usize;
                mask.data_mut()[i..i + damage.w as usize].fill(255);
                expected[i * 4..(i + damage.w as usize) * 4].fill(0);
            }
            PixmapMut::from_bytes(&mut expected, width, height)
                .unwrap()
                .draw_pixmap(
                    origin.0,
                    origin.1,
                    source.as_ref(),
                    &PixmapPaint {
                        opacity,
                        ..Default::default()
                    },
                    Transform::identity(),
                    Some(&mask),
                );
            let mut rgba = vec![0xAA_u8; (width * height * 4) as usize];
            blit_with_opacity(
                &mut rgba,
                (width, height),
                damage,
                source.as_ref(),
                origin,
                opacity,
                false,
            );
            let mut bgra = vec![0xAA_u8; (width * height * 4) as usize];
            blit_with_opacity(
                &mut bgra,
                (width, height),
                damage,
                source.as_ref(),
                origin,
                opacity,
                true,
            );
            for ((e, r), b) in expected
                .as_chunks::<4>()
                .0
                .iter()
                .zip(rgba.as_chunks::<4>().0)
                .zip(bgra.as_chunks::<4>().0)
            {
                for c in 0..4 {
                    assert!(
                        e[c].abs_diff(r[c]) <= 2,
                        "opacity {opacity}: {e:?} vs {r:?}"
                    );
                }
                assert_eq!([r[0], r[1], r[2], r[3]], [b[2], b[1], b[0], b[3]]);
            }
        }
    }
    #[test]
    fn unclipped_arc_remains_visible_without_a_damage_mask() {
        let mut scene = scene();
        scene.layers.clear();
        scene.primitives = vec![Primitive::Arc {
            rect: Rect::new(0., 0., 24., 24.),
            start_angle: 0.,
            sweep_angle: 270.,
            stroke_width: 4.,
            color: [255, 255, 255, 255],
            round_caps: true,
        }];
        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare(&scene, 1.).unwrap();
        let mut bytes = vec![0; 24 * 24 * 4];
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut bytes,
                    width: 24,
                    height: 24,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                Damage::full(24, 24),
            )
            .unwrap();
        assert!(bytes.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn repair_scratch_is_region_sized_reused_capped_and_released_when_idle() {
        let mut scratch = RepairScratch::default();
        let now = std::time::Instant::now();
        let pointer = {
            let (p, _) = scratch.obtain(60, 40).unwrap();
            p.data().as_ptr()
        };
        let bytes = scratch.charge.as_ref().unwrap().bytes();
        assert_eq!(bytes, 60 * 40 * 5);
        assert_eq!(scratch.obtain(40, 20).unwrap().0.data().as_ptr(), pointer);
        assert_eq!(scratch.charge.as_ref().unwrap().bytes(), bytes);
        scratch.used = Some(now);
        scratch.timer = true;
        assert_eq!(
            scratch.trim_idle(now + std::time::Duration::from_millis(499)),
            Some(std::time::Duration::from_millis(1))
        );
        assert_eq!(scratch.trim_idle(now + RepairScratch::IDLE), None);
        assert!(
            scratch.pixels.is_none()
                && scratch.mask.is_none()
                && scratch.charge.is_none()
                && !scratch.timer
        );
        assert!(scratch.obtain(4096, 4096).is_err());
        assert!(scratch.pixels.is_none() && scratch.mask.is_none());
    }

    #[test]
    fn fullscreen_translucent_repair_matches_full_canvas_with_bounded_mask() {
        let (width, height, scale) = (2880, 1800, 1.5_f32);
        let mut scene = scene();
        scene.width = 1920.;
        scene.bounds = Rect::new(0., 0., 1920., 1200.);
        scene.layers.clear();
        scene.asset_data.clear();
        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(0., 0., 1920., 1200.),
                radius: 12.,
                fill: [25, 40, 70, 112],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(80., 100., 260., 34.),
                radius: 9999.,
                fill: [210, 40, 70, 220],
                border: Some(([255, 255, 255, 180], 2.)),
            },
        ];
        let mut cache = TreeCache::default();
        cache.prepare(&scene, scale).unwrap();
        let frame = cache.frame.as_ref().unwrap();
        let damage = Damage {
            x: 3,
            y: 2,
            w: width - 6,
            h: height - 4,
        };
        let full_scratch = width as usize * height as usize * 5;
        let mask_only = width as usize * damage.h as usize;
        assert!(full_scratch > RepairScratch::CAP);
        assert!(mask_only <= RepairScratch::CAP);

        let mut full = vec![0; width as usize * height as usize * 4];
        paint(
            frame,
            (0., 0.),
            ShmTarget {
                bytes: &mut full,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        let outside = [17_u8, 29, 41, 233];
        let mut partial = outside.repeat(width as usize * height as usize);
        let mut scratch = RepairScratch::default();
        paint_with_scratch(
            frame,
            (0., 0.),
            ShmTarget {
                bytes: &mut partial,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            damage,
            &mut scratch,
        )
        .unwrap();
        for y in 0..height {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let inside = x >= damage.x
                    && x < damage.x + damage.w
                    && y >= damage.y
                    && y < damage.y + damage.h;
                if inside {
                    assert_eq!(
                        &partial[at..at + 4],
                        &full[at..at + 4],
                        "partial pixel differs at ({x},{y})"
                    );
                } else {
                    assert_eq!(
                        &partial[at..at + 4],
                        &outside,
                        "pixels outside damage must remain untouched"
                    );
                }
            }
        }
        assert!(
            scratch
                .charge
                .as_ref()
                .is_some_and(|charge| charge.bytes() < bead_core::cache::MIB)
        );
        let used = scratch.used.unwrap();
        assert_eq!(scratch.trim_idle(used + RepairScratch::IDLE), None);
        assert!(scratch.pixels.is_none() && scratch.mask.is_none() && scratch.charge.is_none());
    }

    #[test]
    fn trimmed_ink_preserves_fractional_pose_clips_alpha_and_partial_damage() {
        let mut original = Pixmap::new(30, 18).unwrap();
        for y in 3..11 {
            for x in 5..17 {
                original.pixels_mut()[y * 30 + x] =
                    tiny_skia::Color::from_rgba8(40, 180, 90, if x % 3 == 0 { 1 } else { 128 })
                        .premultiply()
                        .to_color_u8();
            }
        }
        let (trimmed, origin) = trim_text(original.clone()).unwrap();
        assert_eq!(origin, (5, 3));
        assert_eq!((trimmed.width(), trimmed.height()), (12, 8));
        assert!(trimmed.data().len() < original.data().len() / 4);
        let (empty, empty_origin) = trim_text(Pixmap::new(30, 18).unwrap()).unwrap();
        assert_eq!(
            (empty.width(), empty.height(), empty_origin),
            (1, 1, (0, 0))
        );
        assert!(empty.data().iter().all(|byte| *byte == 0));
        let budget = bead_core::cache::Budget::new("trimmed-text-test", 65536);
        for scale in [1., 1.25, 1.5] {
            for offset in [(0., 0.), (0.3333, -0.5), (-0.6667, 0.25)] {
                for background in [false, true] {
                    let frame = |pixels: &Pixmap, origin| {
                        let mut items = Vec::new();
                        if background {
                            items.push(Item {
                                image: None,
                                id: "background".into(),
                                primitive: Primitive::Rounded {
                                    rect: Rect::new(0., 0., 48., 40.),
                                    radius: 0.,
                                    fill: [20, 30, 40, 255],
                                    border: None,
                                },
                                clip: None,
                                raster: None,
                            });
                        }
                        items.push(Item {
                            image: None,
                            id: "text".into(),
                            primitive: Primitive::Text {
                                rect: Rect::new(2.25, 3.75, 30., 18.),
                                text: "fixture".into(),
                                family: "unused".into(),
                                size: 12.,
                                color: [40, 180, 90, 128],
                                weight: 400,
                                align: Some(crate::TextAlign::Left),
                                shadow: None,
                            },
                            clip: Some(Rect::new(4., 3., 20., 14.)),
                            raster: Some(Arc::new(Raster {
                                pixels: Arc::new(pixels.clone()),
                                origin,
                                key: 1,
                                base_key: None,
                                changed: Damage::full(pixels.width(), pixels.height()),
                                _charge: budget.reserve(pixels.data().len()).unwrap(),
                            })),
                        });
                        Frame {
                            native_resource_budget: None,
                            _assets: Vec::new(),
                            items,
                            scale,
                            key: 1,
                            layer_count: 0,
                            layers: vec![],
                            reveal_artifact: None,
                            _accounting: None,
                        }
                    };
                    let full = frame(&original, (0, 0));
                    let cropped = frame(&trimmed, origin);
                    for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
                        for damage in [
                            Damage::full(48, 40),
                            Damage {
                                x: 7,
                                y: 5,
                                w: 20,
                                h: 16,
                            },
                        ] {
                            let mut a = vec![31; 48 * 40 * 4];
                            let mut b = a.clone();
                            paint(
                                &full,
                                offset,
                                ShmTarget {
                                    bytes: &mut a,
                                    width: 48,
                                    height: 40,
                                    scale,
                                    format,
                                },
                                damage,
                            )
                            .unwrap();
                            paint(
                                &cropped,
                                offset,
                                ShmTarget {
                                    bytes: &mut b,
                                    width: 48,
                                    height: 40,
                                    scale,
                                    format,
                                },
                                damage,
                            )
                            .unwrap();
                            assert!(
                                a == b,
                                "cropped text changed pixels at {scale}/{offset:?}/{background}/{damage:?}"
                            );
                        }
                        let moved = frame(&trimmed, (8, 4));
                        let mut tracker = crate::damage::FrameDamage::default();
                        tracker.update(&cropped, offset, 1., 48, 40).unwrap();
                        let dirty = tracker
                            .update(&moved, offset, 1., 48, 40)
                            .unwrap()
                            .into_iter()
                            .fold(Damage::full(0, 0), Damage::union);
                        let mut repaired = vec![0; 48 * 40 * 4];
                        let mut expected = repaired.clone();
                        for (frame, bytes, damage) in [
                            (&cropped, &mut repaired, Damage::full(48, 40)),
                            (&moved, &mut expected, Damage::full(48, 40)),
                        ] {
                            paint(
                                frame,
                                offset,
                                ShmTarget {
                                    bytes,
                                    width: 48,
                                    height: 40,
                                    scale,
                                    format,
                                },
                                damage,
                            )
                            .unwrap();
                        }
                        paint(
                            &moved,
                            offset,
                            ShmTarget {
                                bytes: &mut repaired,
                                width: 48,
                                height: 40,
                                scale,
                                format,
                            },
                            dirty,
                        )
                        .unwrap();
                        assert!(repaired == expected, "ink-origin change left stale pixels");
                    }
                }
            }
        }
    }
    #[test]
    fn opaque_interior_partial_damage_preserves_edges_and_border() {
        let scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 20.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 20., 20.),
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(0., 0., 20., 20.),
                radius: 6.,
                fill: [70, 80, 90, 255],
                border: Some(([200, 30, 40, 255], 2.)),
            }],
        };
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.).unwrap();
        let frame = cache.frame.as_ref().unwrap();
        let mut full = vec![0; 1600];
        paint(
            frame,
            (0., 0.),
            ShmTarget {
                bytes: &mut full,
                width: 20,
                height: 20,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(20, 20),
        )
        .unwrap();
        for d in [
            Damage {
                x: 8,
                y: 8,
                w: 4,
                h: 4,
            },
            Damage {
                x: 1,
                y: 1,
                w: 5,
                h: 5,
            },
        ] {
            let mut partial = full.clone();
            for y in d.y..d.y + d.h {
                for x in d.x..d.x + d.w {
                    partial[((y * 20 + x) * 4) as usize..((y * 20 + x) * 4 + 4) as usize]
                        .copy_from_slice(&[1, 2, 3, 255]);
                }
            }
            paint(
                frame,
                (0., 0.),
                ShmTarget {
                    bytes: &mut partial,
                    width: 20,
                    height: 20,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                d,
            )
            .unwrap();
            assert_eq!(partial, full, "damage {d:?}");
        }
    }
    #[test]
    fn reused_scissors_preserve_alternating_clips_and_partial_damage() {
        let mut scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 10.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 10., 6.),
            primitives: vec![],
        };
        for (color, clip) in [
            ([220, 30, 40, 255], Rect::new(0., 0., 5., 6.)),
            ([250, 250, 250, 128], Rect::new(0., 0., 5., 6.)),
            ([0, 255, 0, 255], Rect::new(0., 0., 10., 3.)),
            ([0, 0, 255, 255], Rect::new(0., 0., 5., 6.)),
            ([255, 255, 0, 0], Rect::new(5., 0., 5., 6.)),
        ] {
            scene.clips.insert(scene.primitives.len(), clip);
            scene.primitives.push(Primitive::Rounded {
                rect: Rect::new(0., 0., 10., 6.),
                radius: 0.,
                fill: color,
                border: None,
            });
        }
        scene
            .clips
            .insert(scene.primitives.len(), Rect::new(5., 0., 5., 6.));
        scene.primitives.push(Primitive::Rounded {
            rect: Rect::new(8., 4., 2., 2.),
            radius: 0.,
            fill: [0, 255, 255, 255],
            border: None,
        });
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.).unwrap();
        let frame = Arc::get_mut(cache.frame.as_mut().unwrap()).unwrap();
        let mut image = Pixmap::new(10, 6).unwrap();
        image.fill(tiny_skia::Color::from_rgba8(200, 40, 60, 128));
        let charge = bead_core::cache::Budget::new("test-first-raster", 1024)
            .reserve(240)
            .unwrap();
        frame.items.insert(
            0,
            Item {
                image: None,
                id: "seed".into(),
                primitive: Primitive::Rounded {
                    rect: Rect::new(0., 0., 10., 6.),
                    radius: 0.,
                    fill: [0; 4],
                    border: None,
                },
                clip: None,
                raster: Some(Arc::new(Raster {
                    pixels: Arc::new(image),
                    origin: (0, 0),
                    key: 0,
                    base_key: None,
                    changed: Damage::full(10, 6),
                    _charge: charge,
                })),
            },
        );
        for damage in [
            Damage::full(10, 6),
            Damage {
                x: 2,
                y: 1,
                w: 7,
                h: 4,
            },
        ] {
            let mut bytes = [77, 66, 55, 255].repeat(60);
            paint(
                frame,
                (0., 0.),
                ShmTarget {
                    bytes: &mut bytes,
                    width: 10,
                    height: 6,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                damage,
            )
            .unwrap();
            for y in 0..6 {
                for x in 0..10 {
                    let expected = if x < damage.x
                        || x >= damage.x + damage.w
                        || y < damage.y
                        || y >= damage.y + damage.h
                    {
                        [77, 66, 55, 255]
                    } else if x < 5 {
                        [0, 0, 255, 255]
                    } else if y < 3 {
                        [0, 255, 0, 255]
                    } else if x >= 8 && y >= 4 {
                        [0, 255, 255, 255]
                    } else {
                        [100, 20, 30, 128]
                    };
                    assert_eq!(
                        &bytes[((y * 10 + x) * 4) as usize..((y * 10 + x) * 4 + 4) as usize],
                        &expected,
                        "pixel {x},{y} damage {damage:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn whole_frame_masked_replacement_fits_after_superseded_gpu_retirement() {
        let _serial = LARGE_RASTER_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut scene = scene();
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        let old = worker.frame.clone().unwrap();
        let payload = old.items[0].image.as_ref().unwrap();
        drop(payload.take_upload().unwrap());
        let budget = payload.budget.clone();
        let gpu = payload.reserve_texture(28 * bead_core::cache::MIB).unwrap();
        scene.primitives.clear();
        scene.layers.clear();
        scene.asset_data.clear();
        for index in 0..25 {
            let path = PathBuf::from(format!("thumb-{index}.png"));
            let mut pixels = Pixmap::new(8, 8).unwrap();
            pixels.fill(tiny_skia::Color::from_rgba8(index * 9, 50, 100, 255));
            scene
                .asset_data
                .insert(path.clone(), Arc::from(pixels.encode_png().unwrap()));
            scene.primitives.push(Primitive::Image {
                rect: Rect::new(
                    (index % 5) as f32 * 512.,
                    (index / 5) as f32 * 432.,
                    512.,
                    432.,
                ),
                path,
                fit: crate::ImageFit::Cover,
                blur: 0.,
                clip_polygon: Some(vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]]),
            });
        }
        assert!(worker.prepare(&scene, 1.).is_err());
        assert!(Arc::ptr_eq(&old, worker.frame.as_ref().unwrap()));
        drop(gpu); // The accepted compositor buffer is independent of these textures.
        worker.prepare(&scene, 1.).unwrap();
        let frame = worker.frame.as_ref().unwrap();
        let cpu_bytes = frame
            .items
            .iter()
            .map(|item| {
                item.image
                    .as_ref()
                    .unwrap()
                    .upload
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .pixels
                    .data()
                    .len()
            })
            .sum::<usize>();
        assert!(cpu_bytes > 21 * bead_core::cache::MIB);
        let mut textures = Vec::new();
        for item in &frame.items {
            let payload = item.image.as_ref().unwrap();
            let upload = payload.take_upload().unwrap();
            textures.push(
                payload
                    .reserve_texture(upload.pixels.data().len() + 192)
                    .unwrap(),
            );
            assert!(budget.used() <= 40 * bead_core::cache::MIB);
            drop(upload);
        }
        assert!(budget.used() <= 40 * bead_core::cache::MIB);
    }
    #[test]
    fn uhd_cover_uses_smaller_exact_target_and_bounded_decode_scratch() {
        let _serial = LARGE_RASTER_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut scene = scene();
        let mut source = Pixmap::new(3840, 2160).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(20, 40, 80, 255));
        scene
            .asset_data
            .insert("proof.png".into(), Arc::from(source.encode_png().unwrap()));
        drop(source);
        if let Primitive::Image { rect, fit, .. } = &mut scene.primitives[0] {
            *rect = Rect::new(0., 0., 1920., 1080.);
            *fit = crate::ImageFit::Cover;
        }
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        let payload = worker.frame.as_ref().unwrap().items[0]
            .image
            .as_ref()
            .unwrap();
        assert!(!payload.native_cover);
        assert_eq!((payload.width, payload.height), (1920, 1080));
        assert_eq!(payload.scratch_budget.used(), 0);
        assert!(payload.budget.used() < 40 * bead_core::cache::MIB);
    }
    #[test]
    fn ui_cpu_prepare_reuses_exact_snapshot_and_defers_native_or_changed_images() {
        let mut scene = scene();
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        scene
            .prepared
            .insert(1.0f32.to_bits(), worker.frame.clone().unwrap());
        let mut renderer = TreeCpuRenderer::default();
        let error = renderer.prepare_on_ui(&scene, 1.).unwrap_err();
        assert!(
            renderer.cache.frame.is_none(),
            "UI never materializes native pixels"
        );
        let request = error
            .downcast_ref::<CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        assert!(request.mark_requested());
        assert!(!request.mark_requested());
        std::thread::scope(|scope| scope.spawn(|| request.prepare().unwrap()).join().unwrap());
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        let builds = renderer.opacity_builds;
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        assert_eq!(
            renderer.opacity_builds, builds,
            "warm exact fingerprint is reused"
        );
        scene.prepared.clear();
        let mut dense_worker = TreeCache::default();
        dense_worker.prepare(&scene, 1.).unwrap();
        scene
            .prepared
            .insert(1.0f32.to_bits(), dense_worker.frame.clone().unwrap());
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        assert!(
            renderer.cpu_preparation.is_none(),
            "normal Dense import never queues another snapshot"
        );
        let accepted = renderer.flattened.as_ref().unwrap().0;
        scene.prepared.clear();
        scene
            .asset_data
            .insert("proof.png".into(), Arc::from(&b"invalid replacement"[..]));
        let error = renderer.prepare_on_ui(&scene, 1.).unwrap_err();
        let request = error
            .downcast_ref::<CpuResourcesPending>()
            .unwrap()
            .request
            .clone();
        assert!(request.prepare().is_err());
        assert!(renderer.prepare_on_ui(&scene, 1.).is_err());
        assert_eq!(
            renderer.flattened.as_ref().unwrap().0,
            accepted,
            "failed worker replacement keeps accepted pixels"
        );
    }

    #[test]
    fn ui_cpu_dense_reveal_prepares_the_pose_without_a_flattened_worker_snapshot() {
        let mut scene = scene();
        scene.width = 160.;
        scene.bounds = Rect::new(0., 0., 160., 160.);
        scene.asset_data.clear();
        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(32., 32., 96., 96.),
                radius: 4.,
                fill: [10, 20, 30, 255],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(40., 40., 10., 10.),
                radius: 2.,
                fill: [200, 0, 0, 255],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(60., 60., 10., 10.),
                radius: 2.,
                fill: [0, 200, 0, 160],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(40., 110., 10., 10.),
                radius: 2.,
                fill: [0, 0, 200, 255],
                border: None,
            },
        ];
        scene.layers[0].own = 0..4;
        scene.layers[0].subtree_end = 4;
        scene.layers[0].bounds = scene.bounds;
        scene.motion = Some(crate::Motion {
            id: "reveal-test".into(),
            open: true,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: Some(40.),
            fade: false,
            reveal: true,
            progress: 0.2,
            parts: Vec::new(),
            art: None,
        });
        scene.reveal(0.2);
        let mut expected = TreeCache::default();
        expected.prepare(&scene, 1.).unwrap();
        let expected_key = expected.frame.as_ref().unwrap().key;

        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare_on_ui(&scene, 1.).unwrap();

        assert!(renderer.cpu_preparation.is_none());
        assert_eq!(renderer.opaque_composition, OpaqueComposition::Direct);
        assert_eq!(renderer.cache.frame.as_ref().unwrap().key, expected_key);
        let frame = renderer.prepared_frame().unwrap();
        assert_eq!(frame.items.len(), 2, "body plus one prepared content layer");
        let content = frame.items[1].raster.as_ref().unwrap().clone();
        scene.reveal(0.45);
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        assert!(
            Arc::ptr_eq(
                &content,
                renderer.prepared_frame().unwrap().items[1]
                    .raster
                    .as_ref()
                    .unwrap()
            ),
            "dense reveal ticks reuse their prepared content layer"
        );
    }

    #[test]
    fn ui_cpu_prepare_reuses_content_across_reveal_progress() {
        let mut scene = scene();
        scene.width = 160.;
        scene.bounds = Rect::new(0., 0., 160., 160.);
        let square = |x: f32, y: f32, fill: [u8; 4]| Primitive::Rounded {
            rect: Rect::new(x, y, 10., 10.),
            radius: 2.,
            fill,
            border: None,
        };
        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(32., 32., 96., 96.),
                radius: 4.,
                fill: [10, 20, 30, 255],
                border: None,
            },
            square(40., 40., [200, 0, 0, 255]),
            Primitive::Image {
                blur: 0.,
                fit: crate::ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(80., 40., 10., 10.),
                path: "proof.png".into(),
            },
            square(60., 60., [0, 200, 0, 160]),
        ];
        scene.layers[0].own = 0..4;
        scene.layers[0].subtree_end = 4;
        scene.layers[0].bounds = scene.bounds;
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        scene
            .prepared
            .insert(1.0f32.to_bits(), worker.frame.clone().unwrap());
        scene.motion = Some(crate::Motion {
            id: "reveal-test".into(),
            open: true,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            fade: false,
            reveal: true,
            progress: 0.2,
            parts: vec![(0, scene.primitives[0].clone())],
            art: None,
        });
        scene.reveal(0.2);

        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        assert!(renderer.cpu_preparation.is_none());
        assert_eq!(renderer.opaque_composition, OpaqueComposition::Direct);
        let initial = renderer.prepared_frame().unwrap();
        assert_eq!(initial.items.len(), 2);
        let content_layer = initial.items[1].raster.as_ref().unwrap().clone();
        let mut expected = TreeCache::default();
        expected.prepare(&scene, 1.).unwrap();
        assert_eq!(
            initial.key,
            expected.frame.as_ref().unwrap().key,
            "the first frame uses the current reveal pose"
        );

        scene.reveal(0.45);
        expected = TreeCache::default();
        expected.prepare(&scene, 1.).unwrap();
        let expected_key = expected.frame.as_ref().unwrap().key;
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        assert_eq!(renderer.cache.frame.as_ref().unwrap().key, expected_key);
        let pose = renderer.prepared_frame().unwrap();
        assert!(
            Arc::ptr_eq(&content_layer, pose.items[1].raster.as_ref().unwrap()),
            "content layer is reused between reveal poses"
        );

        scene.reveal(0.7);
        renderer.prepare_on_ui(&scene, 1.).unwrap();
        // The reused content still takes each frame's reveal pose.
        assert!(!Arc::ptr_eq(&renderer.prepared_frame().unwrap(), &pose));
        assert!(
            Arc::ptr_eq(
                &content_layer,
                renderer.prepared_frame().unwrap().items[1]
                    .raster
                    .as_ref()
                    .unwrap()
            ),
            "later reveal poses reuse the accepted content layer"
        );
        assert_eq!(renderer.opaque_composition, OpaqueComposition::Direct);
    }
    #[test]
    fn compact_svg_upload_reconstructs_exact_dense_pixels() {
        let mut scene = scene();
        scene.asset_data.clear();
        scene.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="256"><defs><linearGradient id="rim"><stop stop-color="#123456"/><stop offset="1" stop-color="#abcdef"/></linearGradient></defs><path fill="url(#rim)" fill-rule="evenodd" d="M0 0H512V256H0Z M32 32V224H480V32Z"/></svg>"##[..]));
        scene.primitives[0] = Primitive::Image {
            rect: Rect::new(2., 2., 512., 256.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: crate::ImageFit::Fill,
            clip_polygon: None,
        };
        let mut dense = TreeCache::default();
        dense.prepare(&scene, 1.5).unwrap();
        let expected = &dense.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap()
            .pixels;
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.5).unwrap();
        let payload = worker.frame.as_ref().unwrap().items[0]
            .image
            .as_ref()
            .unwrap();
        let upload = payload.take_upload().unwrap();
        let compact = payload.compact.lock().unwrap();
        let layout = compact.as_ref().expect("large uniform SVG compacts");
        let mut rebuilt = Pixmap::new(payload.width, payload.height).unwrap();
        for fill in &layout.fills {
            for y in fill.rect.y..fill.rect.y + fill.rect.h {
                for x in fill.rect.x..fill.rect.x + fill.rect.w {
                    let at = ((y * payload.width + x) * 4) as usize;
                    rebuilt.data_mut()[at..at + 4].copy_from_slice(&fill.color);
                }
            }
        }
        for region in &layout.regions {
            for y in 0..region.source.h {
                let source =
                    (((region.atlas.y + y) * upload.pixels.width() + region.atlas.x) * 4) as usize;
                let dest = (((region.source.y + y) * payload.width + region.source.x) * 4) as usize;
                let len = region.source.w as usize * 4;
                rebuilt.data_mut()[dest..dest + len]
                    .copy_from_slice(&upload.pixels.data()[source..source + len]);
            }
        }
        assert_eq!(rebuilt.data(), expected.data());
        assert!(upload.pixels.data().len() < expected.data().len());
    }
    #[test]
    fn upload_policy_keeps_reshaped_reveal_art_on_the_exact_dense_path() {
        let mut scene = scene();
        scene.motion = Some(crate::Motion {
            id: "test".into(),
            open: true,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            fade: false,
            reveal: true,
            progress: 1.,
            parts: vec![(0, scene.primitives[0].clone())],
            art: None,
        });
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        let item = &worker.frame.as_ref().unwrap().items[0];
        assert!(item.raster.is_some());
        assert!(item.image.is_none());
    }
    #[test]
    fn deferred_upload_prepares_recipes_without_decoded_pixels() {
        let mut cache = TreeCache::default().with_image_policy(ImagePolicy::DeferredUpload);
        cache.prepare(&scene(), 1.).unwrap();
        let frame = cache.frame.as_ref().unwrap();
        let payload = frame.items[0].image.as_ref().unwrap();
        assert!(payload.pending());
        assert!(frame.items[0].raster.is_none());
        assert!(payload.upload.lock().unwrap().is_none());
        payload.prepare().unwrap();
        assert!(!payload.pending());
    }
    #[test]
    fn compact_replacement_uses_bounded_scratch_while_active_gpu_is_held() {
        let _serial = LARGE_RASTER_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut scene = scene();
        scene.asset_data.clear();
        scene.primitives[0] = Primitive::Image {
            rect: Rect::new(0., 0., 2300., 2200.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: crate::ImageFit::Fill,
            clip_polygon: None,
        };
        scene.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="2300" height="2200"><rect width="2300" height="2200" fill="#123456"/></svg>"##[..]));
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.).unwrap();
        let old = worker.frame.clone().unwrap();
        let payload = old.items[0].image.as_ref().unwrap();
        drop(payload.take_upload().unwrap());
        let gpu = payload.reserve_texture(28 * bead_core::cache::MIB).unwrap();
        let scratch = payload.scratch_budget.clone();
        let retained = payload.budget.clone();
        scene.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="2300" height="2200"><rect width="2300" height="2200" fill="#654321"/></svg>"##[..]));
        worker
            .prepare(&scene, 1.)
            .expect("large SVG target scratch must not consume retained headroom");
        let accepted = worker.frame.clone().unwrap();
        let upload = accepted.items[0]
            .image
            .as_ref()
            .unwrap()
            .upload
            .lock()
            .unwrap();
        assert!(upload.as_ref().unwrap().pixels.data().len() < 2 * bead_core::cache::MIB);
        drop(upload);
        assert_eq!(scratch.used(), 0);
        assert!(retained.used() <= 40 * bead_core::cache::MIB);
        // Every tile of this gradient is nonuniform, so retaining the dense
        // candidate would exceed the view ceiling beside the active textures.
        scene.asset_data.insert("proof.svg".into(), Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="2300" height="2200"><defs><linearGradient id="g"><stop stop-color="#123456"/><stop offset="1" stop-color="#abcdef"/></linearGradient></defs><rect width="2300" height="2200" fill="url(#g)"/></svg>"##[..]));
        assert!(worker.prepare(&scene, 1.).is_err());
        assert!(Arc::ptr_eq(&accepted, worker.frame.as_ref().unwrap()));
        assert_eq!(scratch.used(), 0);
        assert!(retained.used() <= 40 * bead_core::cache::MIB);
        drop(gpu);
    }
    #[test]
    fn encoded_assets_are_charged_once_and_follow_held_frames_including_unused_assets() {
        let mut scene = scene();
        let encoded = scene.asset_data.values().next().unwrap().clone();
        scene
            .asset_data
            .insert("unused-alias.png".into(), encoded.clone());
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::DeferredUpload);
        worker.prepare(&scene, 1.).unwrap();
        let first = worker.frame.clone().unwrap();
        assert_eq!(first._assets.len(), 2);
        assert!(Arc::ptr_eq(&first._assets[0], &first._assets[1]));
        assert!(Arc::ptr_eq(
            &first._assets[0],
            &first.items[0].image.as_ref().unwrap().encoded
        ));
        assert_eq!(
            first._assets[0]._charge.bytes(),
            encoded.len() + std::mem::size_of::<AssetLease>()
        );
        worker.prepare(&scene, 1.).unwrap();
        let second = worker.frame.clone().unwrap();
        assert!(Arc::ptr_eq(&first._assets[0], &second._assets[0]));
        let retained = worker.resource_budget.as_ref().unwrap().clone();
        let before = retained.used();
        scene.asset_data.insert(
            "unused-alias.png".into(),
            Arc::from(vec![0u8; bead_core::cache::MIB]),
        );
        worker.prepare(&scene, 1.).unwrap();
        let third = worker.frame.clone().unwrap();
        assert!(retained.used() >= before + bead_core::cache::MIB);
        let previous = worker.frame.clone().unwrap();
        scene.asset_data.insert(
            "unused-alias.png".into(),
            Arc::from(vec![0u8; 40 * bead_core::cache::MIB]),
        );
        assert!(worker.prepare(&scene, 1.).is_err());
        assert!(Arc::ptr_eq(&previous, worker.frame.as_ref().unwrap()));
        drop(previous);
        drop(worker);
        assert!(
            retained.used() >= bead_core::cache::MIB,
            "unused encoded asset remains charged through its held frame"
        );
        drop(first);
        drop(second);
        drop(third);
        assert_eq!(retained.used(), 0);
    }
    #[test]
    fn native_view_budget_is_shared_with_gpu_and_held_frames() {
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene(), 1.).unwrap();
        let frame = worker.frame.clone().unwrap();
        let payload = frame.items[0].image.clone().unwrap();
        let budget = payload.budget.clone();
        let available = 40 * bead_core::cache::MIB - budget.used();
        let gpu = payload.reserve_texture(available).unwrap();
        assert!(payload.reserve_texture(1).is_err());
        assert!(
            frame
                ._accounting
                .as_ref()
                .unwrap()
                .reserve_related(1)
                .is_err()
        );
        drop(worker);
        assert!(budget.used() > gpu.bytes(), "held frame remains admitted");
        drop(payload);
        drop(frame);
        assert_eq!(budget.used(), gpu.bytes());
        drop(gpu);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn upload_payload_consumption_reuse_and_worker_rehydration() {
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        let scene = scene();
        worker.prepare(&scene, 1.5).unwrap();
        let frame = worker.frame.clone().unwrap();
        assert!(frame.items[0].raster.is_none());
        let payload = frame.items[0].image.clone().unwrap();
        let pixels = payload.take_upload().unwrap();
        let expected = pixels.pixels.data().to_vec();
        let budget = payload.budget.clone();
        let nonpixel_bytes = budget.used() - expected.len();
        drop(pixels);
        assert_eq!(budget.used(), nonpixel_bytes);
        assert!(payload.pending());
        worker.prepare(&scene, 1.5).unwrap();
        assert!(Arc::ptr_eq(
            &payload,
            worker.frame.as_ref().unwrap().items[0]
                .image
                .as_ref()
                .unwrap()
        ));
        assert!(
            payload.pending(),
            "normal prepare must not decode a consumed GPU hit"
        );
        let nonpixel_bytes = budget.used();
        std::thread::scope(|scope| {
            scope.spawn(|| payload.prepare().unwrap()).join().unwrap();
        });
        assert_eq!(payload.take_upload().unwrap().pixels.data(), expected);
        assert_eq!(budget.used(), nonpixel_bytes);
        payload.prepare().unwrap();
        drop(worker);
        assert!(
            budget.used() > expected.len(),
            "held frame owns upload pixels and metadata"
        );
        drop(payload);
        drop(frame);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn upload_frames_materialize_for_cpu_and_invalid_assets_keep_previous_frame() {
        let mut scene = scene();
        if let Primitive::Image { clip_polygon, .. } = &mut scene.primitives[0] {
            *clip_polygon = Some(vec![[0., 0.], [1., 0.], [0.5, 1.]]);
        }
        let mut worker = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        worker.prepare(&scene, 1.5).unwrap();
        let previous = worker.frame.clone().unwrap();
        let expected = previous.items[0]
            .image
            .as_ref()
            .unwrap()
            .take_upload()
            .unwrap()
            .pixels;
        scene.prepared.insert(1.5_f32.to_bits(), previous.clone());
        let mut cpu = TreeCache::default();
        cpu.prepare(&scene, 1.5).unwrap();
        let dense = cpu.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap();
        assert_eq!(dense.pixels.data(), expected.data());
        scene.prepared.clear();
        scene
            .asset_data
            .insert("proof.png".into(), Arc::from(&b"invalid image"[..]));
        assert!(worker.prepare(&scene, 1.5).is_err());
        assert!(Arc::ptr_eq(&previous, worker.frame.as_ref().unwrap()));
    }
    #[test]
    fn prepared_frame_keeps_raster_charge_after_lookup_drop() {
        let mut cache = TreeCache::default();
        cache.prepare(&scene(), 1.).unwrap();
        let budget = cache.raster_budget.as_ref().unwrap().clone();
        let held = cache.frame.as_ref().unwrap().clone();
        let bytes = budget.used();
        assert!(bytes > 0);
        drop(cache);
        assert_eq!(budget.used(), bytes);
        drop(held);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn scale_retirement_releases_old_rasters_after_the_presented_frame() {
        let mut cache = TreeCache::default();
        let scene = scene();
        cache.prepare(&scene, 1.5).unwrap();
        let held = cache.frame.as_ref().unwrap().clone();
        let raster = held.items[0].raster.as_ref().unwrap();
        let old = Arc::downgrade(raster);
        let pixels = raster.pixels.data().to_vec();
        cache.retain_scales(&[1.25]);
        cache.prepare(&scene, 1.25).unwrap();
        assert_eq!(cache.frame.as_ref().unwrap().scale, 1.25);
        assert!(
            old.upgrade().is_some(),
            "presented frame still owns its raster"
        );
        drop(held);
        assert!(old.upgrade().is_none(), "retired scale is no longer cached");
        cache.retain_scales(&[1.5]);
        cache.prepare(&scene, 1.5).unwrap();
        assert_eq!(
            cache.frame.as_ref().unwrap().items[0]
                .raster
                .as_ref()
                .unwrap()
                .pixels
                .data(),
            pixels.as_slice(),
            "returning to the original scale preserves pixels"
        );
    }
    #[test]
    fn imported_frame_releases_obsolete_local_fallback_caches() {
        let mut display = TreeCache::default();
        display.prepare(&scene(), 1.5).unwrap();
        let old = Arc::downgrade(
            display.frame.as_ref().unwrap().items[0]
                .raster
                .as_ref()
                .unwrap(),
        );
        let mut worker = TreeCache::default();
        let mut ready = scene();
        worker.prepare(&ready, 1.5).unwrap();
        let imported = worker.frame.as_ref().unwrap().clone();
        ready.prepared.insert(1.5_f32.to_bits(), imported.clone());
        display.prepare(&ready, 1.5).unwrap();
        assert!(
            old.upgrade().is_none(),
            "startup fallback is no longer retained"
        );
        assert!(Arc::ptr_eq(display.frame.as_ref().unwrap(), &imported));
        assert_eq!(display.raster_budget.as_ref().unwrap().used(), 0);
        assert_eq!(display.node_budget.as_ref().unwrap().used(), 0);
    }
    // These tests share the process-wide admission cap.
    static LARGE_RASTER_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn large_imported_frame(budget: &bead_core::cache::Budget, count: usize) -> Arc<Frame> {
        let items = (0..count)
            .map(|index| {
                let pixels = Arc::new(Pixmap::new(3000, 3000).unwrap());
                let charge = budget.reserve(pixels.data().len()).unwrap();
                Item {
                    image: None,
                    id: format!("large-{index}"),
                    primitive: Primitive::Rounded {
                        rect: Rect::new(0., 0., 3000., 3000.),
                        radius: 0.,
                        fill: [0; 4],
                        border: None,
                    },
                    clip: None,
                    raster: Some(Arc::new(Raster {
                        changed: Damage::full(3000, 3000),
                        pixels,
                        origin: (0, 0),
                        key: index as u64,
                        base_key: None,
                        _charge: charge,
                    })),
                }
            })
            .collect();
        Arc::new(Frame {
            native_resource_budget: None,
            _assets: Vec::new(),
            items,
            scale: 1.5,
            key: count as u64,
            layer_count: count,
            layers: Vec::new(),
            reveal_artifact: None,
            _accounting: None,
        })
    }
    #[test]
    fn imported_replacement_allows_live_overlap_and_retires_charge_after_last_frame() {
        let _serial = LARGE_RASTER_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let budget = bead_core::cache::Budget::new("test-raster-overlap", LIVE_RASTER_LIMIT);
        let old = large_imported_frame(&budget, 1);
        let old_bytes = budget.used();
        let mut cache = TreeCache {
            raster_budget: Some(budget.clone()),
            frame: Some(old.clone()),
            ..TreeCache::default()
        };
        let candidate = large_imported_frame(&budget, 1);
        let mut scene = scene();
        scene.prepared.insert(1.5_f32.to_bits(), candidate.clone());
        cache.prepare(&scene, 1.5).unwrap();
        assert!(Arc::ptr_eq(cache.frame.as_ref().unwrap(), &candidate));
        assert!(budget.used() > STEADY_RASTER_LIMIT);
        assert!(budget.used() > old_bytes);
        assert!(budget.used() <= LIVE_RASTER_LIMIT);
        drop(old);
        assert_eq!(budget.used(), old_bytes);
    }
    #[test]
    fn normal_bake_replacement_allows_live_overlap_and_reuses_candidate_pixels() {
        let _serial = LARGE_RASTER_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let mut scene = scene();
        scene.asset_data.clear();
        scene.width = 2000.;
        scene.bounds = Rect::new(0., 0., 2000., 2000.);
        scene.primitives = vec![Primitive::Shadow {
            rect: Rect::new(0., 0., 2000., 2000.),
            radius: 35.,
            blur: 10.,
        }];
        scene.layers = vec![crate::SceneLayer {
            id: "large-shadow".into(),
            parent: None,
            own: 0..1,
            subtree_end: 1,
            bounds: scene.bounds,
        }];
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let old_frame = cache.frame.as_ref().unwrap().clone();
        let old_raster = old_frame.items[0].raster.as_ref().unwrap().clone();
        let old_raster_weak = Arc::downgrade(&old_raster);
        let old_pixels = old_raster.pixels.data().to_vec();
        let mut replacement = scene.clone();
        let Primitive::Shadow { radius, .. } = &mut replacement.primitives[0] else {
            unreachable!();
        };
        *radius = 37.;
        cache.prepare(&replacement, 1.5).unwrap();
        let new_frame = cache.frame.as_ref().unwrap().clone();
        let new_raster = new_frame.items[0].raster.as_ref().unwrap().clone();
        let budget = cache.raster_budget.as_ref().unwrap().clone();
        assert!(!Arc::ptr_eq(&old_raster, &new_raster));
        assert_ne!(old_pixels, new_raster.pixels.data());
        let overlap_bytes = budget.used();
        assert!(overlap_bytes > STEADY_RASTER_LIMIT);
        assert!(overlap_bytes <= LIVE_RASTER_LIMIT);

        drop(old_frame);
        drop(old_raster);
        assert!(old_raster_weak.upgrade().is_none());
        assert!(budget.used() < overlap_bytes);
        let before_reuse = new_raster.clone();
        cache.prepare(&replacement, 1.5).unwrap();
        assert!(Arc::ptr_eq(
            &before_reuse,
            cache.frame.as_ref().unwrap().items[0]
                .raster
                .as_ref()
                .unwrap()
        ));
    }
    #[test]
    fn entry_cap_failure_keeps_the_previous_frame() {
        let mut cache = TreeCache::default();
        let previous_scene = scene();
        cache.prepare(&previous_scene, 1.).unwrap();
        let previous = cache.frame.as_ref().unwrap().clone();
        let previous_pixels = frame_pixels(&previous, &previous_scene, 1.);

        let mut replacement = scene();
        let Primitive::Image { rect, .. } = &replacement.primitives[0] else {
            unreachable!();
        };
        let image = Primitive::Image {
            rect: *rect,
            path: "proof.png".into(),
            blur: 0.,
            fit: crate::ImageFit::Fill,
            clip_polygon: None,
        };
        replacement.primitives = (0..1025)
            .map(|index| {
                let mut primitive = image.clone();
                if let Primitive::Image { rect, .. } = &mut primitive {
                    rect.x = index as f32 * 8.;
                }
                primitive
            })
            .collect();
        replacement.layers = (0..1025)
            .map(|index| crate::SceneLayer {
                id: format!("row-{index}"),
                parent: None,
                own: index..index + 1,
                subtree_end: index + 1,
                bounds: Rect::new(index as f32 * 8., 0., 8., 8.),
            })
            .collect();

        assert!(cache.prepare(&replacement, 1.).is_err());
        assert!(Arc::ptr_eq(cache.frame.as_ref().unwrap(), &previous));
        assert_eq!(
            frame_pixels(cache.frame.as_ref().unwrap(), &previous_scene, 1.),
            previous_pixels,
            "failed replacement keeps the last accepted pixels"
        );
    }
    #[test]
    fn oversized_imported_replacement_keeps_previous_frame() {
        let _serial = LARGE_RASTER_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let budget = bead_core::cache::Budget::new("test-raster-oversize", LIVE_RASTER_LIMIT);
        let old = large_imported_frame(&budget, 1);
        let mut cache = TreeCache {
            raster_budget: Some(budget.clone()),
            frame: Some(old.clone()),
            ..TreeCache::default()
        };
        let previous = cache.frame.as_ref().unwrap().clone();
        let candidate = large_imported_frame(&budget, 2);
        let mut scene = scene();
        scene.prepared.insert(1.5_f32.to_bits(), candidate);
        assert!(cache.prepare(&scene, 1.5).is_err());
        assert!(Arc::ptr_eq(cache.frame.as_ref().unwrap(), &previous));
        assert!(Arc::ptr_eq(&old, &previous));
    }
    #[test]
    fn duplicate_text_shares_immutable_pixels_and_keeps_its_own_delta() {
        let mut view = scene();
        view.layers.clear();
        view.font_files = vec![
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../bead-test/fonts/IBMPlexSans.ttf"),
        ];
        let text = Primitive::Text {
            rect: Rect::new(0., 0., 64., 24.),
            text: "same".into(),
            family: "IBM Plex Sans".into(),
            size: 14.,
            color: [255, 255, 255, 255],
            weight: 400,
            align: None,
            shadow: None,
        };
        let mut other = text.clone();
        if let Primitive::Text { rect, .. } = &mut other {
            rect.x = 80.;
        }
        view.primitives = vec![text, other];
        let mut cache = TreeCache::default();
        cache.prepare(&view, 1.).unwrap();
        let before = cache.frame.as_ref().unwrap().clone();
        let first = before.items[0].raster.as_ref().unwrap();
        let second = before.items[1].raster.as_ref().unwrap();
        assert_eq!(cache.builds, 1);
        assert!(!Arc::ptr_eq(first, second));
        assert!(Arc::ptr_eq(&first.pixels, &second.pixels));
        let original = first.pixels.data().to_vec();
        if let Primitive::Text { text, .. } = &mut view.primitives[0] {
            *text = "other".into();
        }
        cache.prepare(&view, 1.).unwrap();
        let after = cache.frame.as_ref().unwrap();
        assert_eq!(cache.builds, 2);
        assert_eq!(first.pixels.data(), original);
        assert!(Arc::ptr_eq(
            &after.items[1].raster.as_ref().unwrap().pixels,
            &second.pixels
        ));
        assert_eq!(
            after.items[0].raster.as_ref().unwrap().base_key,
            Some(first.key)
        );
        assert_eq!(after.items[1].raster.as_ref().unwrap().base_key, None);
    }

    #[test]
    fn a_moved_paint_slot_does_not_compare_unusable_pixels() {
        let before = scene();
        let mut after = before.clone();
        after.layers[0].id = "moved-row".into();
        after.layers[0].bounds.x += 8.;
        if let Primitive::Image { rect, .. } = &mut after.primitives[0] {
            rect.x += 8.;
        }
        let mut image =
            Pixmap::decode_png(&before.asset_data[std::path::Path::new("proof.png")]).unwrap();
        image.pixels_mut()[0] =
            tiny_skia::PremultipliedColorU8::from_rgba(200, 10, 20, 255).unwrap();
        after
            .asset_data
            .insert("proof.png".into(), Arc::from(image.encode_png().unwrap()));
        let mut cache = TreeCache::default();
        cache.prepare(&before, 1.).unwrap();
        cache.prepare(&after, 1.).unwrap();
        let raster = cache.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap();
        assert_eq!(raster.base_key, None);
        assert_eq!(raster.changed, Damage::full(8, 8));
    }

    #[test]
    fn replacement_raster_delta_uses_previous_paint_slot_and_repairs_exactly() {
        for scale in [1., 1.5] {
            for crop in [false, true] {
                let before = scene();
                let mut after = before.clone();
                after.layers[0].id = "replacement-row".into();
                after.resource_revision += 1;
                let mut image =
                    Pixmap::decode_png(&before.asset_data[std::path::Path::new("proof.png")])
                        .unwrap();
                if crop {
                    image = image
                        .clone_rect(tiny_skia::IntRect::from_xywh(0, 0, 6, 8).unwrap())
                        .unwrap();
                    if let Primitive::Image { rect, .. } = &mut after.primitives[0] {
                        rect.w = 6.;
                    }
                } else {
                    image.pixels_mut()[3 * 8 + 2] =
                        tiny_skia::PremultipliedColorU8::from_rgba(200, 10, 20, 255).unwrap();
                }
                after
                    .asset_data
                    .insert("proof.png".into(), Arc::from(image.encode_png().unwrap()));
                let mut cache = TreeCache::default();
                cache.prepare(&before, scale).unwrap();
                let old = cache.frame.as_ref().unwrap().clone();
                cache.trim_idle();
                cache.prepare(&after, scale).unwrap();
                let new = cache.frame.as_ref().unwrap().clone();
                assert_ne!(old.items[0].id, new.items[0].id);
                let old_raster = old.items[0].raster.as_ref().unwrap();
                let new_raster = new.items[0].raster.as_ref().unwrap();
                assert_eq!(new_raster.base_key, Some(old_raster.key));
                assert!(
                    new_raster.changed.w > 0 && new_raster.changed.w < new_raster.pixels.width()
                );
                let builds = cache.builds;
                cache.prepare(&before, scale).unwrap();
                let reverted = cache.frame.as_ref().unwrap().clone();
                assert_eq!(
                    cache.builds, builds,
                    "cached pixels must not be baked again"
                );
                assert_eq!(
                    reverted.items[0].raster.as_ref().unwrap().base_key,
                    Some(new_raster.key)
                );
                for (old, new) in [(&old, &new), (&new, &reverted)] {
                    for offset in [(0., 0.), (0.3333, -0.5)] {
                        let mut tracker = crate::damage::FrameDamage::default();
                        tracker.update(old, offset, 1., 40, 40).unwrap();
                        let damage = tracker
                            .update(new, offset, 1., 40, 40)
                            .unwrap()
                            .into_iter()
                            .fold(Damage::full(0, 0), Damage::union);
                        for format in [crate::PixelFormat::Rgba, crate::PixelFormat::Bgra] {
                            let mut repaired = vec![0; 40 * 40 * 4];
                            let mut expected = repaired.clone();
                            for (frame, bytes, damage) in [
                                (old.as_ref(), repaired.as_mut_slice(), Damage::full(40, 40)),
                                (new.as_ref(), expected.as_mut_slice(), Damage::full(40, 40)),
                            ] {
                                paint(
                                    frame,
                                    offset,
                                    ShmTarget {
                                        bytes,
                                        width: 40,
                                        height: 40,
                                        scale,
                                        format,
                                    },
                                    damage,
                                )
                                .unwrap();
                            }
                            paint(
                                new.as_ref(),
                                offset,
                                ShmTarget {
                                    bytes: &mut repaired,
                                    width: 40,
                                    height: 40,
                                    scale,
                                    format,
                                },
                                damage,
                            )
                            .unwrap();
                            assert!(
                                repaired == expected,
                                "replacement raster repair at scale{scale}, offset{offset:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    fn scene() -> Scene {
        let mut image = Pixmap::new(8, 8).unwrap();
        image.fill(tiny_skia::Color::from_rgba8(20, 40, 80, 255));
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: BTreeMap::from([(
                "proof.png".into(),
                Arc::from(image.encode_png().unwrap()),
            )]),
            clips: BTreeMap::new(),
            layers: vec![crate::SceneLayer {
                id: "row".into(),
                parent: None,
                own: 0..1,
                subtree_end: 1,
                bounds: Rect::new(2., 2., 8., 8.),
            }],
            prepared: Default::default(),
            font_files: vec![],
            width: 24.,
            slide: None,
            primitives: vec![
                Primitive::Image {
                    blur: 0.,
                    fit: crate::ImageFit::Fill,
                    clip_polygon: None,
                    rect: Rect::new(2., 2., 8., 8.),
                    path: "proof.png".into(),
                },
                Primitive::Rounded {
                    rect: Rect::new(2., 12., 8., 3.),
                    radius: 0.,
                    fill: [40, 100, 30, 255],
                    border: None,
                },
            ],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 24., 20.),
        }
    }

    #[test]
    fn picker_snapshot_opens_with_twenty_four_4k_image_entries_at_scale_1_5() {
        let _serial = LARGE_RASTER_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let mut source = Pixmap::new(3840, 2160).unwrap();
        source.fill(tiny_skia::Color::from_rgba8(24, 48, 72, 255));
        let encoded: Arc<[u8]> = Arc::from(source.encode_png().unwrap());
        drop(source);

        let mut picker = scene();
        picker.width = 2880.;
        picker.bounds = Rect::new(0., 0., 2880., 1800.);
        picker.layers.clear();
        picker.asset_data.clear();
        picker.primitives.clear();
        for index in 0..24 {
            let path = PathBuf::from(format!("wallpaper-{index}.png"));
            picker.asset_data.insert(path.clone(), encoded.clone());
            let col = index % 6;
            let row = index / 6;
            picker.primitives.push(Primitive::Image {
                rect: Rect::new(col as f32 * 480., row as f32 * 420., 460., 380.),
                path,
                fit: crate::ImageFit::Cover,
                blur: 0.,
                clip_polygon: None,
            });
        }

        let mut image_cache = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        image_cache
            .prepare(&picker, 1.5)
            .expect("the 24-card visible set fits its 40 MiB owner budget");
        let budget = image_cache.resource_budget.as_ref().unwrap().clone();
        assert!(budget.used() <= 40 * bead_core::cache::MIB);
        picker
            .prepared
            .insert(1.5f32.to_bits(), image_cache.frame.clone().unwrap());

        // Each visible thumbnail is 690×570 RGBA at the real 1.5 effective
        // scale (~1.50 MiB); 24 entries total about 36 MiB, within the picker
        // owner's 40 MiB retained budget. The 4K source files are decoded to
        // their card extents before retention, leaving roughly 4 MiB for
        // encoded thumbnails and metadata.
        let snapshot = TreeCpuRenderer::prepare_snapshot(&picker, 1.5)
            .expect("cache pressure must not reject a picker-sized image set");
        let mut renderer = snapshot.accept().expect("snapshot stays admitted");
        assert_eq!(picker.primitives.len(), 24);
        assert!(
            renderer.flattened.is_some() || renderer.cache.frame.is_some(),
            "the flattened snapshot or retained CPU frame is ready"
        );
        let mut output = Pixmap::new(4320, 2700).unwrap();
        renderer
            .render(
                &picker,
                ShmTarget {
                    bytes: output.data_mut(),
                    width: 4320,
                    height: 2700,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                Damage::full(4320, 2700),
            )
            .unwrap();
        assert!(output.data().iter().any(|byte| *byte != 0));
    }

    fn clipped_wallpaper_scene(count: usize, first_x: f32) -> Scene {
        let mut scene = scene();
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 24.);
        scene.primitives.clear();
        scene.clips.clear();
        scene.font_files = vec![
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../bead-test/fonts/IBMPlexSans.ttf"),
        ];
        for row in 0..count {
            let x = if row == 0 {
                first_x
            } else {
                100. + row as f32 * 20.
            };
            scene.primitives.push(Primitive::Image {
                blur: 0.,
                fit: crate::ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(x, 2., 512., 512.),
                path: "proof.png".into(),
            });
            scene.clips.insert(row, Rect::new(0., 0., 24., 24.));
        }
        scene.primitives.push(Primitive::Text {
            rect: Rect::new(first_x, 16., 16., 6.),
            text: "Wallpaper 0".into(),
            family: "IBM Plex Sans".into(),
            size: 5.,
            color: [255, 255, 255, 255],
            weight: 400,
            align: None,
            shadow: None,
        });
        scene.clips.insert(count, Rect::new(0., 0., 24., 24.));
        let length = scene.primitives.len();
        scene.layers = vec![crate::SceneLayer {
            id: "viewport".into(),
            parent: None,
            own: 0..length,
            subtree_end: length,
            bounds: Rect::new(0., 0., 24., 24.),
        }];
        scene.prepared.clear();
        scene
    }

    fn set_primitive_x(primitive: &mut Primitive, x: f32) {
        match primitive {
            Primitive::Image { rect, .. } | Primitive::Text { rect, .. } => rect.x = x,
            _ => unreachable!(),
        }
    }

    fn frame_raster_bytes(frame: &Frame) -> usize {
        frame
            .items
            .iter()
            .filter_map(|item| item.raster.as_ref())
            .map(|raster| raster.pixels.data().len())
            .sum()
    }

    fn frame_raster_charge(frame: &Frame) -> usize {
        frame
            .items
            .iter()
            .filter_map(|item| item.raster.as_ref())
            .map(|raster| raster._charge.bytes())
            .sum()
    }

    fn cache_frame_raster_charge(cache: &TreeCache, frame: &Frame) -> usize {
        let mut seen = std::collections::BTreeSet::new();
        let mut bytes = 0;
        for raster in cache
            .entries
            .values()
            .flat_map(|entry| std::iter::once(&entry.raster).chain(entry.previous.iter()))
            .chain(frame.items.iter().filter_map(|item| item.raster.as_ref()))
        {
            if seen.insert(Arc::as_ptr(raster) as usize) {
                bytes += raster._charge.bytes();
            }
        }
        bytes
    }

    fn frame_pixels(frame: &Frame, scene: &Scene, scale: f32) -> Vec<u8> {
        let width = (scene.bounds.w * scale).ceil() as u32;
        let height = (scene.bounds.h * scale).ceil() as u32;
        let mut bytes = vec![0; width as usize * height as usize * 4];
        paint(
            frame,
            scene.offset,
            ShmTarget {
                bytes: &mut bytes,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        bytes
    }

    fn render_cpu_frame(
        renderer: &mut TreeCpuRenderer,
        scene: &Scene,
        scale: f32,
        format: PixelFormat,
    ) -> Vec<u8> {
        let width = (scene.bounds.w * scale).ceil() as u32;
        let height = (scene.bounds.h * scale).ceil() as u32;
        let mut bytes = vec![0; width as usize * height as usize * 4];
        renderer
            .prepare(scene, scale)
            .and_then(|()| {
                renderer.render(
                    scene,
                    ShmTarget {
                        bytes: &mut bytes,
                        width,
                        height,
                        scale,
                        format,
                    },
                    Damage::full(width, height),
                )
            })
            .unwrap();
        bytes
    }

    fn assert_snapshot_pixels_match_reference(
        snapshot: &mut TreeCpuRenderer,
        scene: &Scene,
        scale: f32,
    ) {
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            let actual = render_cpu_frame(snapshot, scene, scale, format);
            let mut reference = TreeCpuRenderer::with_opaque_composition(OpaqueComposition::Cached);
            let expected = render_cpu_frame(&mut reference, scene, scale, format);
            assert_eq!(
                actual, expected,
                "snapshot pixels at scale={scale} format={format:?}"
            );
        }
    }

    fn scrolling_wallpaper_window(total: usize, first: usize) -> Scene {
        let mut scene = scene();
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 24.);
        scene.primitives.clear();
        scene.clips.clear();
        scene.layers.clear();
        scene.prepared.clear();
        for row in first..(first + 3).min(total) {
            let index = scene.primitives.len();
            let y = (row - first) as f32 * 8.;
            scene.primitives.push(Primitive::Image {
                blur: 0.,
                fit: crate::ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(2., y, 12., 6.),
                path: "proof.png".into(),
            });
            scene.clips.insert(index, Rect::new(0., 0., 32., 24.));
            scene.layers.push(crate::SceneLayer {
                id: format!("wallpaper-{row}"),
                parent: None,
                own: index..index + 1,
                subtree_end: index + 1,
                bounds: Rect::new(0., y, 24., 8.),
            });
        }
        scene
    }

    #[test]
    fn scrolling_retention_tracks_only_the_accepted_visible_window_and_reverses_pixels() {
        for scale in [1., 1.5] {
            for total in [80, 800] {
                let mut cache = TreeCache::default();
                let mut windows: Vec<_> = (0..total).step_by(3).collect();
                let mut reverse: Vec<_> = windows.iter().rev().skip(1).copied().collect();
                windows.append(&mut reverse);
                for first in windows {
                    let scene = scrolling_wallpaper_window(total, first);
                    let previous = cache.frame.clone();
                    cache.prepare(&scene, scale).unwrap();
                    let frame = cache.frame.as_ref().unwrap();
                    let raster_count = frame
                        .items
                        .iter()
                        .filter(|item| item.raster.is_some())
                        .count();
                    assert_eq!(raster_count, scene.primitives.len());
                    assert_eq!(cache.entries.len(), raster_count);
                    assert_eq!(cache.nodes.len(), scene.layers.len());
                    assert!(cache.entries.keys().all(|(id, bits)| {
                        *bits == scale.to_bits()
                            && scene.layers.iter().any(|layer| id.starts_with(&layer.id))
                    }));
                    if let Some(previous) = previous {
                        let current_frame_charge = frame._accounting.as_ref().unwrap().bytes();
                        let previous_frame_charge = previous._accounting.as_ref().unwrap().bytes();
                        let current_node_charge: usize = cache
                            .nodes
                            .values()
                            .map(|(layer, _)| layer._charge.bytes())
                            .sum();
                        let previous_node_charge: usize = previous
                            .layers
                            .iter()
                            .map(|pose| pose.layer._charge.bytes())
                            .sum();
                        assert_eq!(
                            cache.raster_budget.as_ref().unwrap().used(),
                            cache_frame_raster_charge(&cache, &previous),
                            "external prior frame charges remain accounted until released"
                        );
                        assert_eq!(
                            cache.frame_budget.as_ref().unwrap().used(),
                            current_frame_charge + previous_frame_charge
                        );
                        assert_eq!(
                            cache.node_budget.as_ref().unwrap().used(),
                            current_node_charge + previous_node_charge
                        );
                        drop(previous);
                        let current = cache.frame.as_ref().unwrap();
                        assert_eq!(
                            cache.raster_budget.as_ref().unwrap().used(),
                            cache_frame_raster_charge(&cache, current),
                            "only current entries and image variants remain charged"
                        );
                        assert_eq!(
                            cache.frame_budget.as_ref().unwrap().used(),
                            current_frame_charge,
                            "retiring the prior frame releases its frame metadata charge"
                        );
                        assert_eq!(
                            cache.node_budget.as_ref().unwrap().used(),
                            current_node_charge,
                            "retiring the prior frame releases its retained node charges"
                        );
                    }

                    let expected_scene = scrolling_wallpaper_window(total, first);
                    let mut expected = TreeCache::default();
                    expected.prepare(&expected_scene, scale).unwrap();
                    assert_eq!(
                        frame_pixels(frame, &scene, scale),
                        frame_pixels(expected.frame.as_ref().unwrap(), &expected_scene, scale),
                        "window first={first}, total={total}, scale={scale}"
                    );
                }
                let previous = cache.frame.as_ref().unwrap().clone();
                let previous_raster_charge = frame_raster_charge(&previous);
                let previous_frame_charge = previous._accounting.as_ref().unwrap().bytes();
                let previous_node_charge: usize = previous
                    .layers
                    .iter()
                    .map(|pose| pose.layer._charge.bytes())
                    .sum();
                let empty = scrolling_wallpaper_window(total, total);
                cache.prepare(&empty, scale).unwrap();
                let current = cache.frame.as_ref().unwrap();
                assert!(cache.entries.is_empty());
                assert!(cache.nodes.is_empty());
                assert_eq!(
                    cache.raster_budget.as_ref().unwrap().used(),
                    previous_raster_charge,
                    "the external frame keeps its raster charges after lookup retirement"
                );
                assert_eq!(
                    cache.frame_budget.as_ref().unwrap().used(),
                    previous_frame_charge + current._accounting.as_ref().unwrap().bytes()
                );
                assert_eq!(
                    cache.node_budget.as_ref().unwrap().used(),
                    previous_node_charge
                );
                drop(previous);
                assert_eq!(cache.raster_budget.as_ref().unwrap().used(), 0);
                assert_eq!(cache.frame_budget.as_ref().unwrap().used(), 0);
                assert_eq!(cache.node_budget.as_ref().unwrap().used(), 0);
            }
        }
    }

    #[test]
    fn clipped_wallpaper_rows_keep_descriptions_but_rasterize_only_visible_image_and_text() {
        for scale in [1., 1.5] {
            for count in [80, 800] {
                let mut scene = clipped_wallpaper_scene(count, 100.);
                const { assert!(80 * 512 * 512 * 4 > STEADY_RASTER_LIMIT) };
                let mut cache = TreeCache::default();
                cache.prepare(&scene, scale).unwrap();
                let hidden = cache.frame.as_ref().unwrap().clone();
                assert_eq!(hidden.items.len(), count + 1);
                assert_eq!(hidden.layers.len(), 1);
                assert!(hidden.items.iter().all(|item| item.raster.is_none()));
                assert_eq!(frame_raster_bytes(&hidden), 0);

                // A supplied prepared frame from the old pose must not hide the
                // raster when this row enters the clip.
                scene.prepared.insert(scale.to_bits(), hidden.clone());
                set_primitive_x(&mut scene.primitives[0], 2.);
                set_primitive_x(&mut scene.primitives[count], 2.);
                cache.prepare(&scene, scale).unwrap();
                let visible = cache.frame.as_ref().unwrap().clone();
                assert_eq!(visible.items.len(), count + 1);
                assert_eq!(visible.layers.len(), 1);
                assert!(visible.items[0].raster.is_some());
                assert!(
                    visible.items[1..count]
                        .iter()
                        .all(|item| item.raster.is_none()),
                    "only the overscan row now inside the clip needs pixels"
                );
                assert!(visible.items[count].raster.is_some());
                assert!(frame_raster_bytes(&visible) < 64 * bead_core::cache::MIB);

                let expected_scene = clipped_wallpaper_scene(1, 2.);
                let mut expected_cache = TreeCache::default();
                expected_cache.prepare(&expected_scene, scale).unwrap();
                assert_eq!(
                    frame_pixels(&visible, &scene, scale),
                    frame_pixels(
                        expected_cache.frame.as_ref().unwrap(),
                        &expected_scene,
                        scale
                    ),
                    "visible row pixels at scale {scale}"
                );

                // A stale prepared visible pose must not keep its raster attached
                // after the row returns outside the persistent scroll clip.
                scene.prepared.insert(scale.to_bits(), visible.clone());
                set_primitive_x(&mut scene.primitives[0], 100.);
                set_primitive_x(&mut scene.primitives[count], 100.);
                cache.prepare(&scene, scale).unwrap();
                let hidden_again = cache.frame.as_ref().unwrap();
                assert_eq!(hidden_again.items.len(), count + 1);
                assert_eq!(hidden_again.layers.len(), 1);
                assert!(hidden_again.items.iter().all(|item| item.raster.is_none()));
                assert_eq!(frame_raster_bytes(hidden_again), 0);
                assert_eq!(
                    frame_pixels(hidden_again, &scene, scale),
                    vec![
                        0;
                        (scene.bounds.w * scale).ceil() as usize
                            * (scene.bounds.h * scale).ceil() as usize
                            * 4
                    ],
                    "hidden rows paint no pixels at scale {scale}"
                );
            }
        }
    }

    #[test]
    fn reveal_window_does_not_discard_a_raster_inside_its_persistent_clip() {
        for scale in [1., 1.5] {
            let mut scene = scene();
            scene.width = 160.;
            scene.bounds = Rect::new(0., 0., 160., 160.);
            scene.primitives.truncate(1);
            scene.layers[0].own = 0..1;
            scene.layers[0].subtree_end = 1;
            scene.layers[0].bounds = scene.bounds;
            scene.clips.insert(0, scene.bounds);
            if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
                *rect = Rect::new(40., 100., 8., 8.);
            }
            scene.motion = Some(crate::Motion {
                id: "reveal".into(),
                open: true,
                duration: 0.3,
                return_duration: None,
                spring: false,
                curve: None,
                return_curve: None,
                edge: bead_core::Edge::Top,
                slide_distance: None,
                fade: false,
                reveal: true,
                progress: 1.,
                parts: Vec::new(),
                art: None,
            });
            let mut cache = TreeCache::default();
            cache.prepare(&scene, scale).unwrap();
            let side = (160. * scale).ceil() as usize;
            assert!(
                frame_pixels(cache.frame.as_ref().unwrap(), &scene, scale)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] != 0)
            );
            let canonical_open = cache.frame.as_ref().unwrap().clone();
            assert!(canonical_open.items[0].raster.is_some());
            scene.prepared.insert(scale.to_bits(), canonical_open);

            scene.reveal(0.);
            cache.prepare(&scene, scale).unwrap();
            let closed = cache.frame.as_ref().unwrap().clone();
            assert!(closed.items[0].raster.is_some());
            assert_eq!(
                frame_pixels(&closed, &scene, scale),
                vec![0; side * side * 4]
            );
            scene.reveal(1.);
            cache.prepare(&scene, scale).unwrap();
            let opened = cache.frame.as_ref().unwrap().clone();
            assert!(opened.items[0].raster.is_some());
            assert_eq!(cache.builds, 1, "opening reused the retained image raster");
            assert!(
                frame_pixels(&opened, &scene, scale)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] != 0)
            );
        }
    }

    #[test]
    fn reveal_composites_prepared_content_instead_of_painting_each_item() {
        let scale = 1.5;
        let mut scene = scene();
        scene.width = 160.;
        scene.bounds = Rect::new(0., 0., 160., 160.);
        let square = |x: f32, y: f32, fill: [u8; 4]| Primitive::Rounded {
            rect: Rect::new(x, y, 10., 10.),
            radius: 2.,
            fill,
            border: None,
        };
        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(32., 32., 96., 96.),
                radius: 4.,
                fill: [10, 20, 30, 255],
                border: None,
            },
            square(40., 40., [200, 0, 0, 255]),
            Primitive::Image {
                blur: 0.,
                fit: crate::ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(80., 40., 10., 10.),
                path: "proof.png".into(),
            },
            square(60., 60., [0, 200, 0, 160]),
            square(40., 110., [0, 0, 200, 255]),
        ];
        scene.layers[0].own = 0..5;
        scene.layers[0].subtree_end = 5;
        scene.layers[0].bounds = scene.bounds;
        scene.motion = Some(crate::Motion {
            id: "reveal".into(),
            open: true,
            duration: 0.3,
            return_duration: None,
            spring: false,
            curve: None,
            return_curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: Some(40.),
            fade: false,
            reveal: true,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });
        let mut cache = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
        cache.prepare(&scene, scale).unwrap();
        let open = cache.frame.as_ref().unwrap().clone();
        let mut dense_scene = scene.clone();
        dense_scene.prepared.clear();
        let mut dense_cache = TreeCache::default();
        dense_cache.prepare(&dense_scene, scale).unwrap();
        let dense_open_frame = dense_cache.frame.as_ref().unwrap().clone();
        scene.prepared.insert(scale.to_bits(), open);
        scene.reveal(0.5);
        dense_scene.reveal(0.5);
        cache.prepare(&scene, scale).unwrap();
        dense_scene.prepared.clear();
        dense_cache.prepare(&dense_scene, scale).unwrap();
        let mut imported_scene = dense_scene.clone();
        imported_scene
            .prepared
            .insert(scale.to_bits(), dense_open_frame);
        let mut imported_cache = TreeCache::default();
        imported_cache.prepare(&imported_scene, scale).unwrap();
        let half = cache.frame.as_ref().unwrap().clone();
        // Content before and after the native image is grouped in paint order.
        assert_eq!(half.items.len(), 4);
        assert!(half.items[1].raster.is_some());
        assert!(half.items[2].image.is_some());
        assert!(half.items[3].raster.is_some());
        assert_eq!(rect(&half.items[1].primitive).y, -20.);
        let layer = half.items[1].raster.clone().unwrap();
        let dense_half = dense_cache.frame.as_ref().unwrap();
        let pixels = frame_pixels(dense_half, &dense_scene, scale);
        let imported_pixels = frame_pixels(
            imported_cache.frame.as_ref().unwrap(),
            &imported_scene,
            scale,
        );
        let row = (160. * scale) as usize * 4;
        // Generated and imported open frames agree after the same nonzero slide offset.
        let edge = ((32. + 48.) * scale) as usize;
        let visible_len = (edge - 8) * row;
        let max_error = pixels[..visible_len]
            .iter()
            .zip(&imported_pixels[..visible_len])
            .map(|(actual, expected)| actual.abs_diff(*expected))
            .max()
            .unwrap_or(0);
        assert!(
            max_error <= 1,
            "prepared reveal layer differs from open pixels by {max_error}/255"
        );
        assert!(pixels[(edge + 2) * row..].iter().all(|byte| *byte == 0));
        scene.reveal(0.75);
        cache.prepare(&scene, scale).unwrap();
        let later = cache.frame.as_ref().unwrap().clone();
        assert!(
            Arc::ptr_eq(&layer, later.items[1].raster.as_ref().unwrap()),
            "a later pose reuses the prepared content layer"
        );
        assert!(
            later.items[2].image.is_some(),
            "native image keeps its paint slot"
        );
        assert!(
            Arc::ptr_eq(
                half.items[3].raster.as_ref().unwrap(),
                later.items[3].raster.as_ref().unwrap()
            ),
            "content after the native image is reused in order"
        );
        let old_layer = later.items[1].raster.as_ref().unwrap().clone();
        if let Primitive::Rounded { fill, .. } = &mut scene.primitives[1] {
            *fill = [180, 30, 20, 255];
        }
        scene.prepared.clear();
        cache.prepare(&scene, scale).unwrap();
        let changed = cache.frame.as_ref().unwrap();
        assert!(
            !Arc::ptr_eq(&old_layer, changed.items[1].raster.as_ref().unwrap()),
            "content edits invalidate prepared reveal layers"
        );
    }

    #[test]
    fn worker_reveal_artifact_matches_open_pose_and_survives_generated_and_imported_poses() {
        let scale = 1.5;
        let mut source = scene();
        source.width = 160.;
        source.bounds = Rect::new(0., 0., 160., 160.);
        source.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(20., 20., 120., 120.),
                radius: 12.,
                fill: [30, 80, 140, 255],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(45., 55., 30., 24.),
                radius: 4.,
                fill: [220, 40, 20, 255],
                border: None,
            },
        ];
        source.layers[0].own = 0..2;
        source.layers[0].subtree_end = 2;
        source.layers[0].bounds = source.bounds;
        source.motion = Some(crate::Motion {
            id: "prepared-artifact".into(),
            open: true,
            duration: 0.3,
            return_duration: None,
            spring: false,
            curve: None,
            return_curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: Some(24.),
            fade: false,
            reveal: true,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });

        let generated = TreeCpuRenderer::prepare_reveal_artifact(&source, scale).unwrap();
        let mut imported_pose = source.clone();
        imported_pose.reveal(0.4); // imports the body's open geometry into Motion::parts
        let imported = TreeCpuRenderer::prepare_reveal_artifact(&imported_pose, scale).unwrap();
        assert_eq!(generated.key(), imported.key());
        assert_eq!(generated.pixels.data(), imported.pixels.data());

        let (width, height) = (
            (source.bounds.w * scale) as u32,
            (source.bounds.h * scale) as u32,
        );
        let mut actual = vec![0; width as usize * height as usize * 4];
        imported
            .composite(
                ShmTarget {
                    bytes: &mut actual,
                    width,
                    height,
                    scale,
                    format: PixelFormat::Rgba,
                },
                Damage::full(width, height),
                (0., 0.),
                source.bounds,
            )
            .unwrap();
        let mut expected = vec![0; width as usize * height as usize * 4];
        let mut open = source.clone();
        open.reveal(1.);
        let mut cache = TreeCache::default();
        cache.prepare(&open, scale).unwrap();
        paint(
            cache.frame.as_ref().unwrap(),
            (0., 0.),
            ShmTarget {
                bytes: &mut expected,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        assert_eq!(
            actual, expected,
            "prepared open pixels remain pose-independent"
        );

        let mut pose = source.clone();
        pose.reveal(0.4);
        pose.offset = (3., 4.);
        let mut renderer = TreeCpuRenderer {
            active_reveal: Some(ActiveReveal {
                motion_id: "prepared-artifact".into(),
                open: true,
                artifact: Arc::new(imported.clone()),
            }),
            cache: TreeCache {
                builds: 17,
                ..TreeCache::default()
            },
            ..TreeCpuRenderer::default()
        };
        renderer.prepare_on_ui(&pose, scale).unwrap();
        assert_eq!(
            renderer.cache.builds, 17,
            "reveal pose does not rebuild the tree"
        );
        let mut rendered = vec![255; width as usize * height as usize * 4];
        renderer
            .render(
                &pose,
                ShmTarget {
                    bytes: &mut rendered,
                    width,
                    height,
                    scale,
                    format: PixelFormat::Rgba,
                },
                Damage::full(width, height),
            )
            .unwrap();
        let mut expected_pose = vec![0; width as usize * height as usize * 4];
        let slide_offset = pose.content_offset(usize::MAX);
        imported
            .composite(
                ShmTarget {
                    bytes: &mut expected_pose,
                    width,
                    height,
                    scale,
                    format: PixelFormat::Rgba,
                },
                Damage::full(width, height),
                (
                    pose.offset.0 + slide_offset.0,
                    pose.offset.1 + slide_offset.1,
                ),
                pose.reveal_window()
                    .unwrap()
                    .translated(pose.offset.0, pose.offset.1),
            )
            .unwrap();
        assert_eq!(
            rendered, expected_pose,
            "render clears before offset/clip composition"
        );
    }

    #[test]
    fn filtered_frame_reuses_kept_rasters_and_unchanged_layers() {
        let mut source = scene();
        source.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(2., 2., 8., 8.),
                radius: 1.,
                fill: [20, 40, 80, 255],
                border: None,
            },
            Primitive::Text {
                rect: Rect::new(2., 12., 18., 8.),
                text: "retained café".into(),
                family: "IBM Plex Sans".into(),
                size: 12.,
                color: [240, 230, 210, 255],
                weight: 400,
                align: None,
                shadow: None,
            },
            Primitive::Text {
                rect: Rect::new(2., 2., 18., 8.),
                text: "other retained text".into(),
                family: "IBM Plex Sans".into(),
                size: 12.,
                color: [210, 220, 240, 255],
                weight: 400,
                align: None,
                shadow: None,
            },
        ];
        source.font_files = vec![
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../bead-test/fonts/IBMPlexSans.ttf"),
        ];
        source.layers = vec![
            crate::SceneLayer {
                id: "root".into(),
                parent: None,
                own: 0..2,
                subtree_end: 3,
                bounds: source.bounds,
            },
            crate::SceneLayer {
                id: "kept".into(),
                parent: Some("root".into()),
                own: 2..3,
                subtree_end: 3,
                bounds: Rect::new(2., 2., 18., 8.),
            },
        ];
        let mut fresh_scene = source.clone();
        fresh_scene.primitives.remove(0);
        fresh_scene.layers[0].own = 0..1;
        fresh_scene.layers[0].subtree_end = 2;
        fresh_scene.layers[1].own = 1..2;
        fresh_scene.layers[1].subtree_end = 2;
        for scale in [1., 1.5] {
            let mut cache = TreeCache::default();
            cache.prepare(&source, scale).unwrap();
            let original = cache.frame.as_ref().unwrap().clone();
            let filtered = original.filtered(&[false, true, true]).unwrap();
            let mut fresh_cache = TreeCache::default();
            fresh_cache.prepare(&fresh_scene, scale).unwrap();
            let fresh = fresh_cache.frame.as_ref().unwrap();

            assert_eq!(original.items.len(), 3, "source frame remains unchanged");
            assert_eq!(filtered.items.len(), 2);
            assert!(Arc::ptr_eq(
                filtered.items[0].raster.as_ref().unwrap(),
                original.items[1].raster.as_ref().unwrap(),
            ));
            assert_eq!(
                filtered.items[0].raster.as_ref().unwrap().pixels.data(),
                fresh.items[0].raster.as_ref().unwrap().pixels.data(),
                "filtered text raster matches a fresh bake at scale {scale}"
            );
            assert!(Arc::ptr_eq(
                filtered.items[1].raster.as_ref().unwrap(),
                original.items[2].raster.as_ref().unwrap(),
            ));
            assert_eq!(
                filtered.items[1].raster.as_ref().unwrap().pixels.data(),
                fresh.items[1].raster.as_ref().unwrap().pixels.data(),
                "second filtered text raster matches a fresh bake at scale {scale}"
            );
            assert!(!Arc::ptr_eq(
                &filtered.layers[0].layer,
                &original.layers[0].layer,
            ));
            assert_eq!(filtered.layers[0].layer.own.len(), 1);
            assert!(Arc::ptr_eq(
                &filtered.layers[1].layer,
                &original.layers[1].layer,
            ));
            assert_eq!(filtered.layers[1].layer.own, original.layers[1].layer.own);

            let width = (source.bounds.w * scale).ceil() as u32;
            let height = (source.bounds.h * scale).ceil() as u32;
            let mut filtered_pixels = vec![0; (width * height * 4) as usize];
            let mut fresh_pixels = filtered_pixels.clone();
            let damage = Damage::full(width, height);
            paint(
                &filtered,
                (0., 0.),
                ShmTarget {
                    bytes: &mut filtered_pixels,
                    width,
                    height,
                    scale,
                    format: PixelFormat::Rgba,
                },
                damage,
            )
            .unwrap();
            paint(
                fresh,
                (0., 0.),
                ShmTarget {
                    bytes: &mut fresh_pixels,
                    width,
                    height,
                    scale,
                    format: PixelFormat::Rgba,
                },
                damage,
            )
            .unwrap();
            assert_eq!(
                filtered_pixels, fresh_pixels,
                "masked scene pixels at {scale}"
            );
        }
    }
    #[test]
    fn frozen_cpu_content_survives_hidden_pose_and_reopens_without_repainting() {
        let mut scene = scene();
        let mut frozen = TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        scene.offset = (3., 4.);
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            for opacity in [0., 0.2, 1.] {
                scene.opacity = opacity;
                let mut reference =
                    TreeCpuRenderer::with_opaque_composition(OpaqueComposition::Cached);
                reference.prepare(&scene, 1.).unwrap();
                let mut expected = vec![255; 40 * 40 * 4];
                let mut actual = expected.clone();
                for (renderer, bytes) in
                    [(&mut reference, &mut expected), (&mut frozen, &mut actual)]
                {
                    renderer
                        .render(
                            &scene,
                            ShmTarget {
                                bytes,
                                width: 40,
                                height: 40,
                                scale: 1.,
                                format,
                            },
                            Damage::full(40, 40),
                        )
                        .unwrap();
                }
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(
            frozen.opacity_builds, 1,
            "hidden and reopened poses reuse the frozen pixels"
        );
        assert!(frozen.cache.frame.is_none());
        assert!(frozen.prepared_frame().is_none());
        assert!(frozen.snapshot_fingerprint.is_some());
    }

    #[test]
    fn closed_snapshot_drops_full_rasters_and_keeps_the_first_frame_pixels() {
        let mut scene = scene();
        let mut source_cache = TreeCache::default();
        source_cache.prepare(&scene, 1.).unwrap();
        let source_frame = source_cache.frame.as_ref().unwrap().clone();
        let weak_pixels = Arc::downgrade(&source_frame.items[0].raster.as_ref().unwrap().pixels);
        let expected = frame_pixels(&source_frame, &scene, 1.);
        scene.prepared.insert(1_f32.to_bits(), source_frame);

        let mut snapshot = TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        assert!(snapshot.snapshot_fingerprint.is_some());
        assert!(snapshot.cache.frame.is_none());
        assert!(snapshot.cache.entries.is_empty());
        assert!(snapshot.cache.nodes.is_empty());
        assert!(snapshot.prepared_frame().is_none());

        // The caller adopts the snapshot and drops its prepared-scene owner.
        scene.prepared.clear();
        drop(source_cache);
        assert!(
            weak_pixels.upgrade().is_none(),
            "no full image raster owner remains"
        );

        let mut actual = vec![0; 24 * 20 * 4];
        snapshot.prepare(&scene, 1.).unwrap();
        snapshot
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut actual,
                    width: 24,
                    height: 20,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                Damage::full(24, 20),
            )
            .unwrap();
        assert_eq!(
            actual, expected,
            "flattened pixels preserve the accepted UI"
        );
    }

    #[test]
    fn closed_snapshot_fingerprint_ignores_pose_but_rebuilds_changed_content_and_scale() {
        let mut scene = scene();
        let mut snapshot = TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        let initial_builds = snapshot.opacity_builds;

        scene.opacity = 0.4;
        scene.offset = (2.25, -1.5);
        snapshot.prepare(&scene, 1.).unwrap();
        assert_eq!(snapshot.opacity_builds, initial_builds);
        assert!(
            snapshot.cache.frame.is_none(),
            "pose alone keeps the sparse snapshot"
        );
        assert_snapshot_pixels_match_reference(&mut snapshot, &scene, 1.);

        let mut changed_asset = scene.clone();
        let mut image =
            Pixmap::decode_png(&changed_asset.asset_data[std::path::Path::new("proof.png")])
                .unwrap();
        image.pixels_mut()[0] =
            tiny_skia::PremultipliedColorU8::from_rgba(200, 10, 20, 255).unwrap();
        changed_asset
            .asset_data
            .insert("proof.png".into(), Arc::from(image.encode_png().unwrap()));
        changed_asset.resource_revision += 1;
        snapshot.prepare(&changed_asset, 1.).unwrap();
        assert!(snapshot.snapshot_fingerprint.is_some());
        assert!(snapshot.cache.frame.is_none());
        assert_eq!(snapshot.opacity_builds, initial_builds + 1);
        let changed_key = snapshot.flattened.as_ref().unwrap().0;
        assert_snapshot_pixels_match_reference(&mut snapshot, &changed_asset, 1.);

        let mut changed_clip = changed_asset.clone();
        changed_clip.clips.insert(0, Rect::new(4., 4., 3., 3.));
        snapshot.prepare(&changed_clip, 1.).unwrap();
        assert!(snapshot.snapshot_fingerprint.is_some());
        assert!(snapshot.cache.frame.is_none());
        assert_eq!(snapshot.opacity_builds, initial_builds + 2);
        let clip_key = snapshot.flattened.as_ref().unwrap().0;
        assert_ne!(changed_key, clip_key, "clip edits rebuild snapshot pixels");
        assert_snapshot_pixels_match_reference(&mut snapshot, &changed_clip, 1.);

        snapshot.prepare(&changed_clip, 1.5).unwrap();
        assert_eq!(snapshot.flattened.as_ref().unwrap().1, 1.5);
        assert_eq!(snapshot.opacity_builds, initial_builds + 3);
        assert!(
            snapshot.cache.frame.is_none(),
            "the new scale is snapshotted then trimmed"
        );
        let scaled_key = snapshot.flattened.as_ref().unwrap().0;
        assert_snapshot_pixels_match_reference(&mut snapshot, &changed_clip, 1.5);

        let mut broken = changed_clip.clone();
        broken.asset_data.remove(std::path::Path::new("proof.png"));
        assert!(snapshot.prepare(&broken, 1.5).is_err());
        assert!(snapshot.snapshot_fingerprint.is_some());
        assert!(snapshot.cache.frame.is_none());
        assert_eq!(snapshot.opacity_builds, initial_builds + 3);
        assert_eq!(snapshot.flattened.as_ref().unwrap().1, 1.5);
        assert_eq!(snapshot.flattened.as_ref().unwrap().0, scaled_key);
        assert_snapshot_pixels_match_reference(&mut snapshot, &changed_clip, 1.5);
    }

    #[test]
    fn closed_snapshot_keeps_reveal_frames_on_the_existing_path() {
        let mut scene = scene();
        scene.motion = Some(crate::Motion {
            id: "test-reveal".into(),
            open: false,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Bottom,
            slide_distance: Some(20.),
            fade: false,
            reveal: true,
            progress: 0.,
            parts: Vec::new(),
            art: None,
        });
        let snapshot = TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        assert!(snapshot.snapshot_fingerprint.is_none());
        assert!(snapshot.cache.frame.is_some());
        assert!(!snapshot.cache.entries.is_empty());
    }
    #[test]
    fn rle_snapshot_matches_dense_opacity_blits_and_saves_uniform_margins() {
        let mut scene = scene();
        scene.width = 100.;
        scene.bounds = Rect::new(0., 0., 100., 80.);
        scene.layers.clear();
        scene.primitives.insert(
            0,
            Primitive::Rounded {
                rect: Rect::new(1., 1., 98., 78.),
                radius: 0.,
                fill: [20, 25, 35, 160],
                border: None,
            },
        );
        scene.offset = (0., 0.);
        let mut sparse = TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        let mut dense = TreeCpuRenderer::with_opaque_composition(OpaqueComposition::Cached);
        dense.prepare(&scene, 1.).unwrap();
        dense.cache_content(&scene).unwrap();
        let Some((_, _, FlattenedPixels::Sparse(pixels))) = sparse.flattened.as_ref() else {
            panic!("snapshot should RLE-compress its repeated pixels");
        };
        let stored = pixels.bytes();
        assert!(
            stored * 2 < (pixels.width * pixels.height * 4) as usize,
            "stored={stored}, full={}x{}",
            pixels.width,
            pixels.height
        );
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            for opacity in [0., 0.2, 1.] {
                scene.opacity = opacity;
                for offset in [(0., 0.), (-3.25, 4.5), (3.25, -4.5), (-3.25, -4.5)] {
                    scene.offset = offset;
                    for damage in [
                        Damage {
                            x: 0,
                            y: 0,
                            w: 12,
                            h: 20,
                        },
                        Damage {
                            x: 7,
                            y: 9,
                            w: 23,
                            h: 17,
                        },
                        Damage {
                            x: 28,
                            y: 0,
                            w: 12,
                            h: 40,
                        },
                    ] {
                        let mut expected = vec![0x5a; 40 * 40 * 4];
                        let mut actual = expected.clone();
                        for (renderer, bytes) in
                            [(&mut dense, &mut expected), (&mut sparse, &mut actual)]
                        {
                            renderer
                                .render(
                                    &scene,
                                    ShmTarget {
                                        bytes,
                                        width: 40,
                                        height: 40,
                                        scale: 1.,
                                        format,
                                    },
                                    damage,
                                )
                                .unwrap();
                        }
                        assert_eq!(
                            actual, expected,
                            "format={format:?} opacity={opacity} offset={offset:?} damage={damage:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn large_high_entropy_cpu_snapshot_skips_unbounded_compression_work() {
        let _serial = LARGE_RASTER_TESTS.lock().unwrap_or_else(|p| p.into_inner());
        let mut source = Pixmap::new(2, 2).unwrap();
        source.data_mut().copy_from_slice(&[
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
        ]);
        let encoded: Arc<[u8]> = Arc::from(source.encode_png().unwrap());
        let mut large = scene();
        large.width = 2880.;
        large.bounds = Rect::new(0., 0., 2880., 1800.);
        large.layers.clear();
        large.asset_data = BTreeMap::from([(PathBuf::from("wallpaper.png"), encoded)]);
        large.primitives = vec![Primitive::Image {
            rect: large.bounds,
            path: PathBuf::from("wallpaper.png"),
            fit: crate::ImageFit::Fill,
            blur: 0.,
            clip_polygon: None,
        }];

        let started = std::time::Instant::now();
        let prepared = TreeCpuRenderer::prepare_snapshot(&large, 1.5)
            .expect("large snapshot stays within the bounded dense path");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "large CPU snapshot exceeded its 15-second test bound"
        );
        let renderer = prepared.accept().unwrap();
        let Some((_, _, FlattenedPixels::Dense(pixels))) = renderer.flattened.as_ref() else {
            panic!("large high-entropy snapshot must skip sparse compression");
        };
        assert_eq!((pixels.width(), pixels.height()), (4320, 2700));
        assert!(pixels.data().len() <= 64 * bead_core::cache::MIB);
    }
    #[test]
    fn partial_repairs_match_full_pixels_with_opaque_background_corners_clip_and_alpha() {
        let mut scene = scene();
        scene.width = 80.;
        scene.bounds = Rect::new(0., 0., 80., 64.);
        scene.layers.clear();
        scene.asset_data.clear();
        scene.primitives = vec![
            Primitive::Shadow {
                rect: Rect::new(0., 0., 80., 64.),
                radius: 12.,
                blur: 6.,
            },
            Primitive::Rounded {
                rect: Rect::new(6., 6., 64., 48.),
                radius: 9.,
                fill: [40, 70, 100, 255],
                border: Some(([200, 30, 40, 180], 2.)),
            },
            Primitive::Rounded {
                rect: Rect::new(30., 20., 20., 16.),
                radius: 4.,
                fill: [80, 20, 140, 100],
                border: None,
            },
        ];
        for scale in [1_f32, 1.25, 1.5, 2.] {
            for clipped in [false, true] {
                scene.clips = if clipped {
                    BTreeMap::from([(1, Rect::new(14., 10., 48., 32.))])
                } else {
                    BTreeMap::new()
                };
                let mut cache = TreeCache::default();
                cache.prepare(&scene, scale).unwrap();
                let frame = cache.frame.as_ref().unwrap();
                let (width, height) = ((80. * scale).round() as u32, (64. * scale).round() as u32);
                for offset in [(0., 0.), (0.5, -0.25), (-2.25, 1.125), (0.3333, -0.6667)] {
                    for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
                        let mut full = vec![0; width as usize * height as usize * 4];
                        paint(
                            frame,
                            offset,
                            ShmTarget {
                                bytes: &mut full,
                                width,
                                height,
                                scale,
                                format,
                            },
                            Damage::full(width, height),
                        )
                        .unwrap();
                        let mut damages = vec![
                            Rect::new(24., 22., 12., 8.),
                            Rect::new(6., 6., 10., 10.),
                            Rect::new(8., 18., 14., 18.),
                            Rect::new(0., 0., 3., 3.),
                        ];
                        damages.extend((0..96).map(|i| {
                            Rect::new(
                                ((i * 17 + 3) % 77) as f32,
                                ((i * 11 + 1) % 61) as f32,
                                1. + (i % 13) as f32,
                                1. + (i % 9) as f32,
                            )
                        }));
                        for (n, logical) in damages.into_iter().enumerate() {
                            let damage = logical.pixels(scale, width, height);
                            if n == 0 {
                                assert_eq!(
                                    opaque_start(frame, offset, scale, damage, width, height),
                                    Some(1),
                                    "interior test must exercise coverage skip"
                                );
                            }
                            if n == 3 {
                                assert!(
                                    opaque_start(frame, offset, scale, damage, width, height)
                                        .is_none(),
                                    "canvas corner must retain shadow/clear"
                                );
                            }
                            let before =
                                [17_u8, 29, 41, 233].repeat(width as usize * height as usize);
                            let mut partial = before.clone();
                            paint(
                                frame,
                                offset,
                                ShmTarget {
                                    bytes: &mut partial,
                                    width,
                                    height,
                                    scale,
                                    format,
                                },
                                damage,
                            )
                            .unwrap();
                            for y in 0..height {
                                for x in 0..width {
                                    let i = ((y * width + x) * 4) as usize;
                                    let inside = x >= damage.x
                                        && x < damage.x + damage.w
                                        && y >= damage.y
                                        && y < damage.y + damage.h;
                                    let expected = if inside {
                                        &full[i..i + 4]
                                    } else {
                                        &before[i..i + 4]
                                    };
                                    assert_eq!(
                                        &partial[i..i + 4],
                                        expected,
                                        "partial pixels differ at {scale}/{offset:?}/{format:?}/clip{clipped}/{damage:?}/({x},{y})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn scrolling_reuses_the_row_packet_and_raster_while_clip_and_pose_change() {
        let mut scene = scene();
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let before = cache.frame.clone().unwrap();
        let builds = cache.builds;
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.y -= 4.;
        }
        scene.layers[0].bounds.y -= 4.;
        scene.clips.insert(0, Rect::new(0., 0., 24., 8.));
        scene.resource_revision += 1;
        cache.prepare(&scene, 1.5).unwrap();
        let after = cache.frame.as_ref().unwrap();
        assert_eq!(cache.builds, builds);
        assert!(Arc::ptr_eq(
            before.items[0].raster.as_ref().unwrap(),
            after.items[0].raster.as_ref().unwrap()
        ));
        assert!(Arc::ptr_eq(&before.layers[0].layer, &after.layers[0].layer));
        assert_ne!(before.layers[0].bounds, after.layers[0].bounds);
    }
    #[test]
    fn image_raster_cache_reuses_one_previous_extent_exactly() {
        let mut scene = scene();
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let first = cache.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap()
            .pixels
            .data()
            .to_vec();

        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 9.;
        }
        cache.prepare(&scene, 1.5).unwrap();
        let second = cache.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap()
            .pixels
            .data()
            .to_vec();
        assert_ne!(first, second, "the altered extent has its own raster");
        assert_eq!(cache.builds, 2);

        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 8.;
        }
        cache.prepare(&scene, 1.5).unwrap();
        assert_eq!(
            cache.builds, 2,
            "returning to either exact key reuses its variant"
        );
        assert_eq!(
            cache.frame.as_ref().unwrap().items[0]
                .raster
                .as_ref()
                .unwrap()
                .pixels
                .data(),
            first
        );
        let mut fresh = TreeCache::default();
        fresh.prepare(&scene, 1.5).unwrap();
        assert_eq!(
            fresh.frame.as_ref().unwrap().items[0]
                .raster
                .as_ref()
                .unwrap()
                .pixels
                .data(),
            first,
            "a reused image raster matches fresh rendering"
        );
    }

    #[test]
    fn image_raster_variants_release_on_idle_and_look_retirement() {
        let mut scene = scene();
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let slot = ("row/image/0".to_owned(), 1.5_f32.to_bits());
        let first = Arc::downgrade(&cache.entries[&slot].raster);
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 9.;
        }
        cache.prepare(&scene, 1.5).unwrap();
        assert!(
            first.upgrade().is_some(),
            "the prior extent is retained while active"
        );
        let builds = cache.builds;
        cache.trim_idle();
        assert!(
            first.upgrade().is_none(),
            "idle trim releases the extra variant"
        );
        cache.prepare(&scene, 1.5).unwrap();
        assert_eq!(
            cache.builds, builds,
            "the current extent survives idle trim"
        );

        let since = cache.serial();
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 8.;
        }
        cache.prepare(&scene, 1.5).unwrap();
        let retired = Arc::downgrade(cache.entries[&slot].previous.as_ref().unwrap());
        assert!(retired.upgrade().is_some());
        cache.drop_unused_since(since);
        assert!(
            retired.upgrade().is_none(),
            "look retirement releases extra variants"
        );
    }

    #[test]
    fn image_variant_budget_discards_prior_extent_before_failing() {
        let mut scene = scene();
        let budget = bead_core::cache::Budget::new("test-image-variants", 1024);
        let mut cache = TreeCache {
            raster_budget: Some(budget.clone()),
            ..TreeCache::default()
        };
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 7.;
        }
        cache.prepare(&scene, 1.).unwrap();
        let first = Arc::downgrade(&cache.entries[&("row/image/0".into(), 1_f32.to_bits())].raster);
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 8.;
        }
        cache.prepare(&scene, 1.).unwrap();
        assert!(first.upgrade().is_some());
        if let Primitive::Image { rect, .. } = &mut scene.primitives[0] {
            rect.h = 9.;
        }
        cache.prepare(&scene, 1.).unwrap();
        assert!(
            first.upgrade().is_none(),
            "budget pressure evicts the previous variant first"
        );
        assert!(budget.used() <= 1024);
    }

    #[test]
    fn same_path_asset_update_has_one_pixel_delta_and_dynamic_fill_never_bakes() {
        let mut scene = scene();
        let mut cache = TreeCache::default();
        cache.prepare(&scene, 1.).unwrap();
        let builds = cache.builds;
        if let Primitive::Rounded { rect, .. } = &mut scene.primitives[1] {
            rect.w += 3.;
        }
        cache.prepare(&scene, 1.).unwrap();
        assert_eq!(cache.builds, builds);
        let mut pixels = Pixmap::new(8, 8).unwrap();
        pixels.fill(tiny_skia::Color::from_rgba8(20, 40, 80, 255));
        pixels.pixels_mut()[2 * 8 + 3] =
            tiny_skia::PremultipliedColorU8::from_rgba(100, 0, 0, 255).unwrap();
        scene
            .asset_data
            .insert("proof.png".into(), Arc::from(pixels.encode_png().unwrap()));
        cache.prepare(&scene, 1.).unwrap();
        let changed = cache.frame.as_ref().unwrap().items[0]
            .raster
            .as_ref()
            .unwrap();
        assert_eq!(cache.builds, builds + 1);
        assert_eq!(
            changed.changed,
            Damage {
                x: 3,
                y: 2,
                w: 1,
                h: 1
            }
        );
        let builds = cache.builds;
        let bytes = scene.asset_data[std::path::Path::new("proof.png")].clone();
        scene.asset_data.insert("proof-renamed.png".into(), bytes);
        if let Primitive::Image { path, .. } = &mut scene.primitives[0] {
            *path = "proof-renamed.png".into();
        }
        cache.prepare(&scene, 1.).unwrap();
        assert_eq!(
            cache.builds,
            builds + 1,
            "a changed image path remains a full-signature miss"
        );
    }
    #[test]
    fn generation_retirement_keeps_current_rasters_at_every_scale() {
        let scene = scene();
        let mut cache = TreeCache::default();
        let serial = cache.serial();
        let mut frames = Vec::new();
        for scale in [1., 1.5] {
            cache.prepare(&scene, scale).unwrap();
            frames.push(cache.frame.as_ref().unwrap().clone());
        }
        cache.drop_unused_since(serial);
        let builds = cache.builds;
        for (scale, before) in [1., 1.5].into_iter().zip(frames) {
            cache.prepare(&scene, scale).unwrap();
            let after = cache.frame.as_ref().unwrap();
            assert!(Arc::ptr_eq(
                before.items[0].raster.as_ref().unwrap(),
                after.items[0].raster.as_ref().unwrap()
            ));
            assert_eq!(cache.builds, builds);
        }
    }
    #[test]
    fn idle_trim_keeps_visible_pixels_and_rebuilds_changed_assets() {
        let mut scene = scene();
        let mut renderer = TreeCpuRenderer::default();
        let mut pixels = vec![0; 36 * 30 * 4];
        let draw = |renderer: &mut TreeCpuRenderer, scene: &Scene, pixels: &mut Vec<u8>| {
            renderer.prepare(scene, 1.5).unwrap();
            renderer
                .render(
                    scene,
                    ShmTarget {
                        bytes: pixels,
                        width: 36,
                        height: 30,
                        scale: 1.5,
                        format: PixelFormat::Rgba,
                    },
                    Damage::full(36, 30),
                )
                .unwrap();
        };
        draw(&mut renderer, &scene, &mut pixels);
        let before = pixels.clone();
        renderer.cache.trim_idle();
        draw(&mut renderer, &scene, &mut pixels);
        assert_eq!(pixels, before);
        let mut image = Pixmap::new(8, 8).unwrap();
        image.fill(tiny_skia::Color::from_rgba8(200, 10, 30, 255));
        scene
            .asset_data
            .insert("proof.png".into(), Arc::from(image.encode_png().unwrap()));
        draw(&mut renderer, &scene, &mut pixels);
        assert_ne!(pixels, before);
        let mut fresh = vec![0; pixels.len()];
        draw(&mut TreeCpuRenderer::default(), &scene, &mut fresh);
        assert_eq!(pixels, fresh);
    }
    #[test]
    fn partial_shm_and_group_opacity_match_full_and_previous_composition() {
        let mut scene = scene();
        let mut renderer = TreeCpuRenderer::default();
        let (w, h) = (36, 30);
        let mut partial = vec![0; w as usize * h as usize * 4];
        let mut full = partial.clone();
        let mut tracker = crate::damage::FrameDamage::default();
        for step in 0..6 {
            if let Primitive::Rounded { rect, .. } = &mut scene.primitives[1] {
                rect.w = 6. + step as f32;
            }
            renderer.prepare(&scene, 1.5).unwrap();
            let frame = renderer.prepared_frame().unwrap();
            let regions = tracker
                .update(&frame, scene.offset, scene.opacity, w, h)
                .unwrap();
            let damage = regions.into_iter().fold(Damage::full(0, 0), Damage::union);
            renderer
                .render(
                    &scene,
                    ShmTarget {
                        bytes: &mut partial,
                        width: w,
                        height: h,
                        scale: 1.5,
                        format: PixelFormat::Bgra,
                    },
                    damage,
                )
                .unwrap();
            renderer
                .render(
                    &scene,
                    ShmTarget {
                        bytes: &mut full,
                        width: w,
                        height: h,
                        scale: 1.5,
                        format: PixelFormat::Bgra,
                    },
                    Damage::full(w, h),
                )
                .unwrap();
            assert_eq!(partial, full);
        }
        scene.opacity = 0.5;
        renderer.prepare(&scene, 1.5).unwrap();
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut partial,
                    width: w,
                    height: h,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                Damage::full(w, h),
            )
            .unwrap();
        let mut previous = crate::retained::RetainedCpuRenderer::default();
        previous.prepare(&scene, 1.5).unwrap();
        previous
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut full,
                    width: w,
                    height: h,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                Damage::full(w, h),
            )
            .unwrap();
        assert_eq!(partial, full);
    }

    #[test]
    fn translated_fractional_scale_polyline_matches_retained_pixels_inside_damage_clip() {
        use crate::{CpuRenderer, PixelFormat, Renderer, ShmTarget};
        let mut scene = scene();
        scene.offset = (4., 2.);
        scene.bounds = Rect::new(0., 0., 48., 42.);
        scene.layers.clear();
        scene.primitives = vec![Primitive::Polyline {
            rect: Rect::new(5., 4., 32., 28.).translated(0.25, 0.5),
            points: vec![[0.05, 0.1], [0.48, 0.9], [0.95, 0.08]],
            closed: true,
            stroke_width: 4.,
            color: [240, 80, 20, 255],
            round_caps: true,
            fill: Some([20, 100, 220, 140]),
        }];
        scene.clips.insert(0, Rect::new(9., 8., 25., 20.));
        let (w, h, scale) = (90, 75, 1.5);
        let damage = Damage {
            x: 16,
            y: 14,
            w: 34,
            h: 30,
        };
        let mut direct = vec![0; (w * h * 4) as usize];
        let mut retained = direct.clone();
        let mut cpu = CpuRenderer::default();
        cpu.prepare(&scene, scale).unwrap();
        cpu.render(
            &scene,
            ShmTarget {
                bytes: &mut direct,
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            damage,
        )
        .unwrap();
        let mut tree = TreeCpuRenderer::default();
        tree.prepare(&scene, scale).unwrap();
        tree.render(
            &scene,
            ShmTarget {
                bytes: &mut retained,
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            damage,
        )
        .unwrap();
        let max_error = direct
            .iter()
            .zip(&retained)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(
            max_error <= 1,
            "direct/vector composite differs by {max_error}/255"
        );
        let mut full = vec![0; direct.len()];
        tree.render(
            &scene,
            ShmTarget {
                bytes: &mut full,
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(w, h),
        )
        .unwrap();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if x < damage.x
                    || x >= damage.x + damage.w
                    || y < damage.y
                    || y >= damage.y + damage.h
                {
                    full[i..i + 4].fill(0);
                }
            }
        }
        assert!(
            retained == full,
            "partial repair must exactly match the clipped full frame"
        );
    }
}

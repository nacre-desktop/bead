// SPDX-License-Identifier: MIT OR Apache-2.0
//! Retained subtree ownership and immutable local rasters, independent of pose/clip.
use crate::{
    CpuRenderer, Damage, PixelFormat, Primitive, Rect, Renderer, Result, Scene, ShmTarget,
};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Arc,
};
use tiny_skia::{FillRule, Mask, Paint, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform};

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
#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub primitive: Primitive,
    pub clip: Option<Rect>,
    pub raster: Option<Arc<Raster>>,
}
// Prepared frames are shared through Arc; metadata has one allocation owner.
pub struct Frame {
    pub items: Vec<Item>,
    pub scale: f32,
    pub key: u64,
    pub layer_count: usize,
    pub layers: Vec<LayerPose>,
    pub(crate) _accounting: Option<bead_core::cache::Charge>,
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
    touched: u64,
}
#[derive(Default)]
pub struct TreeCache {
    pub frame: Option<Arc<Frame>>,
    pub builds: usize,
    entries: BTreeMap<(String, u32), Entry>,
    raster: CpuRenderer,
    font_files: Vec<std::path::PathBuf>,
    serial: u64,
    nodes: BTreeMap<(String, u32), (Arc<RetainedLayer>, u64)>,
    pub node_updates: usize,
    raster_budget: Option<bead_core::cache::Budget>,
    node_budget: Option<bead_core::cache::Budget>,
    frame_budget: Option<bead_core::cache::Budget>,
}
pub fn rect(primitive: &Primitive) -> Rect {
    match primitive {
        Primitive::Rounded { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => *rect,
    }
}
fn local(primitive: &Primitive) -> Primitive {
    let mut p = primitive.clone();
    match &mut p {
        Primitive::Rounded { rect, .. }
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
            align,
            rect: r,
            text,
            family,
            size,
            color,
        } => {
            rect(r, hash);
            (align, text, family, size.to_bits(), color).hash(hash);
        }
        Primitive::Image {
            blur,
            rect: r,
            path,
        } => {
            rect(r, hash);
            (blur.to_bits(), path).hash(hash);
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
        self.nodes.retain(|_, (_, touched)| *touched > serial);
    }
    /// Keep finished active rasters, release scratch and offscreen cache entries.
    pub fn trim_idle(&mut self) {
        self.raster = CpuRenderer::default();
        self.entries.retain(|_, entry| entry.touched == self.serial);
        self.nodes.retain(|_, (_, touched)| *touched == self.serial);
    }
    pub fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if let Some(frame) = scene.prepared.get(&scale.to_bits()) {
            // Worker-prepared frames own their rasters and nodes. A locally
            // baked startup fallback is obsolete after this hand-off.
            if !self.entries.is_empty() || !self.nodes.is_empty() {
                self.entries.clear();
                self.nodes.clear();
                self.raster = CpuRenderer::default();
                self.font_files.clear();
            }
            self.frame = Some(frame.clone());
            return Ok(());
        }
        if !scale.is_finite() || scale <= 0. {
            return Err("invalid layer scale".into());
        }
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
        serde_json::to_vec(&scene.bounds)?.hash(&mut frame_hash);
        for (index, primitive) in scene.primitives.iter().enumerate() {
            let kind = match primitive {
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
            let raster = if kind == "fill" {
                None
            } else {
                let slot = (id.clone(), scale.to_bits());
                if self
                    .entries
                    .get(&slot)
                    .is_none_or(|entry| entry.raster.key != key)
                {
                    let started = std::time::Instant::now();
                    let mut content = scene.clone();
                    content.primitives = vec![local(primitive)];
                    content.clips.clear();
                    content.layers.clear();
                    content.prepared.clear();
                    content.offset = (0., 0.);
                    content.opacity = 1.;
                    self.raster.prepare(&content, scale)?;
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
                    let pixels = Arc::new(pixels);
                    // Prefer the last ordered paint slot: a replacement row can
                    // have a new state ID while most of its raster is unchanged.
                    // The presenter uses this delta only when its exact previous
                    // raster key, geometry, clip and opacity match.
                    let previous = self
                        .frame
                        .as_ref()
                        .filter(|frame| frame.scale == scale)
                        .and_then(|frame| frame.items.get(index))
                        .and_then(|item| item.raster.as_deref())
                        .or_else(|| self.entries.get(&slot).map(|old| old.raster.as_ref()));
                    let base_key = previous.map(|old| old.key);
                    let changed = previous.map_or_else(
                        || Damage::full(pixels.width(), pixels.height()),
                        |old| pixel_difference(&old.pixels, &pixels),
                    );
                    let bytes = pixels.data().len() + id.capacity() + 192;
                    let budget = self.raster_budget.get_or_insert_with(|| {
                        bead_core::cache::Budget::new(
                            "retained-cpu-rasters",
                            64 * bead_core::cache::MIB,
                        )
                    });
                    let charge = loop {
                        match budget.reserve(bytes) {
                            Ok(c) => break c,
                            Err(e) => {
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
                    self.entries.insert(
                        slot.clone(),
                        Entry {
                            raster,
                            touched: self.serial,
                        },
                    );
                    self.builds += 1;
                    println!(
                        "PROFILE stage=layer_bake id={id} total_ms={:.3} builds={}",
                        started.elapsed().as_secs_f64() * 1000.,
                        self.builds
                    );
                }
                let entry = self.entries.get_mut(&slot).ok_or("layer cache missing")?;
                entry.touched = self.serial;
                let raster = entry.raster.clone();
                let previous = self
                    .frame
                    .as_ref()
                    .filter(|frame| frame.scale == scale)
                    .and_then(|frame| frame.items.get(index))
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
                id,
                primitive: primitive.clone(),
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
            + layers.capacity() * std::mem::size_of::<LayerPose>();
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
        self.frame = Some(Arc::new(Frame {
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
        }));
        // Keep offscreen rows for scrolling, but cap cache residency and entry count.
        while self.entries.len() > 1024
            || self
                .entries
                .values()
                .map(|e| e.raster.pixels.data().len())
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
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
    let mut target = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
        .ok_or("invalid SHM strip")?;
    let mut mask = Mask::new(width, damage.h).ok_or("damage mask")?;
    let mut mask_clip = None;
    let mut painted = false;
    for item in &frame.items[start.unwrap_or(0)..] {
        let _item_profile = bead_core::profile::Span::new(match item.primitive {
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

        let clip = item.clip.map_or(damage, |r| {
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
        }
        // Interior coverage excludes every antialiased corner and stroke.
        // Only fully opaque interior pixels can bypass the masked blend pipeline.
        if item.raster.is_none()
            && let Primitive::Rounded { fill, .. } = &item.primitive
            && fill[3] == 255
            && fill_covers(&item.primitive, offset, scale, clip)
        {
            for row in clip.y..clip.y + clip.h {
                let start = ((row - damage.y) * width + clip.x) as usize * 4;
                for pixel in target.data_mut()[start..start + clip.w as usize * 4]
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
                    target.data_mut()[dst..dst + len]
                        .copy_from_slice(&raster.pixels.data()[src..src + len]);
                }
                painted = true;
            }
            continue;
        }
        let r = rect(&item.primitive);
        let mask_needed = if let Some(raster) = &item.raster {
            let x = ((r.x + offset.0) * scale).round() as i64 + i64::from(raster.origin.0);
            let y = ((r.y + offset.1) * scale).round() as i64 + i64::from(raster.origin.1);
            !(x >= i64::from(clip.x)
                && y >= i64::from(clip.y)
                && x + i64::from(raster.pixels.width()) <= i64::from(clip.x + clip.w)
                && y + i64::from(raster.pixels.height()) <= i64::from(clip.y + clip.h))
        } else {
            true
        };
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
            target.draw_pixmap(
                ((r.x + offset.0) * scale).round() as i32 + raster.origin.0 as i32,
                ((r.y + offset.1) * scale).round() as i32 + raster.origin.1 as i32
                    - damage.y as i32,
                raster.pixels.as_ref().as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                mask_needed.then_some(&mask),
            );
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
                let padding = ((*radius * scale).ceil() as u32)
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
                let (scratch, scratch_mask) = repair_scratch.obtain(w, h)?;
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
                let transform = Transform::from_scale(scale, scale)
                    .post_translate(offset.0 * scale - x as f32, offset.1 * scale - y as f32);
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
                    bytes[dest..dest + clip.w as usize * 4]
                        .copy_from_slice(&scratch.data()[source..source + clip.w as usize * 4]);
                }
            }
            target = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
                .ok_or("invalid restored SHM strip")?;
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
#[derive(Default)]
pub struct TreeCpuRenderer {
    pub opaque_composition: OpaqueComposition,
    pub cache: TreeCache,
    /// Number of content-group composites; pose/opacity changes reuse each group.
    pub opacity_builds: usize,
    /// Opaque frames compose primitives directly rather than reuse an opacity group.
    pub direct_composites: usize,
    repair_scratch: RepairScratch,
    flattened: Option<(u64, f32, Pixmap)>,
    group_budget: Option<bead_core::cache::Budget>,
    group_charge: Option<bead_core::cache::Charge>,
}
impl TreeCpuRenderer {
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
        (self.cache.frame.as_ref().map_or(0, |f| f.layer_count), 0)
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
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
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        let cached_opaque = match self.opaque_composition {
            OpaqueComposition::Auto => false,
            OpaqueComposition::Direct => false,
            OpaqueComposition::Cached => true,
        };
        if scene.opacity >= 1. && !cached_opaque {
            self.direct_composites += 1;
            self.flattened = None;
            self.group_charge = None;
            return paint_with_scratch(
                frame,
                scene.offset,
                target,
                damage,
                &mut self.repair_scratch,
            );
        }
        if scene.opacity <= 0. {
            self.flattened = None;
            self.group_charge = None;
            let ShmTarget { bytes, width, .. } = target;
            for y in damage.y..damage.y + damage.h {
                let i = (y * width + damage.x) as usize * 4;
                bytes[i..i + damage.w as usize * 4].fill(0);
            }
            return Ok(());
        }
        // Group opacity preserves previous semantics; compose cached local rasters,
        // never reload fonts or re-bake glyphs for an animated pose.
        if self
            .flattened
            .as_ref()
            .is_none_or(|(key, scale, _)| *key != frame.key || *scale != frame.scale)
        {
            let (w, h) = (
                (scene.bounds.w * frame.scale).ceil() as u32,
                (scene.bounds.h * frame.scale).ceil() as u32,
            );
            if u64::from(w) * u64::from(h) * 4 > 64 * 1024 * 1024 {
                return Err("opacity group exceeds budget".into());
            }
            self.flattened = None;
            self.group_charge = None;
            self.group_charge = Some(
                self.group_budget
                    .get_or_insert_with(|| {
                        bead_core::cache::Budget::new("cpu-composite", 64 * bead_core::cache::MIB)
                    })
                    .reserve(w as usize * h as usize * 4 + 64)?,
            );
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
            self.opacity_builds += 1;
            self.flattened = Some((frame.key, frame.scale, pixels));
        }
        let (_, _, pixels) = self.flattened.as_ref().ok_or("opacity group missing")?;
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        let mut mask = Mask::new(width, height).ok_or("opacity damage mask")?;
        for y in damage.y..damage.y + damage.h {
            let i = (y * width + damage.x) as usize;
            mask.data_mut()[i..i + damage.w as usize].fill(255);
            bytes[i * 4..(i + damage.w as usize) * 4].fill(0);
        }
        let mut target = PixmapMut::from_bytes(bytes, width, height).ok_or("SHM opacity target")?;
        target.draw_pixmap(
            ((scene.offset.0 + scene.bounds.x) * scale).round() as i32,
            ((scene.offset.1 + scene.bounds.y) * scale).round() as i32,
            pixels.as_ref(),
            &PixmapPaint {
                opacity: scene.opacity.clamp(0., 1.),
                ..Default::default()
            },
            Transform::identity(),
            Some(&mask),
        );
        if format == PixelFormat::Bgra {
            for y in damage.y..damage.y + damage.h {
                for x in damage.x..damage.x + damage.w {
                    bytes[(y * width + x) as usize * 4..(y * width + x) as usize * 4 + 4]
                        .swap(0, 2);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                            id: "text".into(),
                            primitive: Primitive::Text {
                                rect: Rect::new(2.25, 3.75, 30., 18.),
                                text: "fixture".into(),
                                family: "unused".into(),
                                size: 12.,
                                color: [40, 180, 90, 128],
                                align: Some(crate::TextAlign::Left),
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
                            items,
                            scale,
                            key: 1,
                            layer_count: 0,
                            layers: vec![],
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
}

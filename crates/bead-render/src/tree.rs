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
    pub key: u64,
    pub base_key: Option<u64>,
    pub changed: Damage,
}
#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub primitive: Primitive,
    pub clip: Option<Rect>,
    pub raster: Option<Arc<Raster>>,
}
#[derive(Clone)]
pub struct Frame {
    pub items: Vec<Item>,
    pub scale: f32,
    pub key: u64,
    pub layer_count: usize,
    pub layers: Vec<LayerPose>,
}
/// A retained subtree packet; unchanged row content reuses this allocation.
pub struct RetainedLayer {
    pub id: String,
    pub parent: Option<String>,
    pub own: Vec<String>,
    pub children: Vec<String>,
    pub key: u64,
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
fn signature(scene: &Scene, primitive: &Primitive, scale: f32) -> Result<u64> {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    scale.to_bits().hash(&mut hash);
    serde_json::to_vec(&local(primitive))?.hash(&mut hash);
    if matches!(primitive, Primitive::Text { .. }) {
        scene.font_files.hash(&mut hash);
    }
    if let Primitive::Image { path, .. } = primitive {
        scene.asset_data.get(path).hash(&mut hash);
    }
    Ok(hash.finish())
}
fn pixel_difference(before: &Pixmap, after: &Pixmap) -> Damage {
    if before.width() != after.width() || before.height() != after.height() {
        return Damage::full(after.width(), after.height());
    }
    let (mut left, mut top, mut right, mut bottom) = (after.width(), after.height(), 0, 0);
    for (index, (old, new)) in before
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.data().as_chunks::<4>().0.iter())
        .enumerate()
    {
        if old != new {
            let x = index as u32 % after.width();
            let y = index as u32 / after.width();
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    Damage {
        x: left.min(right),
        y: top.min(bottom),
        w: right.saturating_sub(left),
        h: bottom.saturating_sub(top),
    }
}
impl TreeCache {
    pub fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if let Some(frame) = scene.prepared.get(&scale.to_bits()) {
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
            let key = signature(scene, primitive, scale)?;
            id.hash(&mut frame_hash);
            key.hash(&mut frame_hash);
            serde_json::to_vec(primitive)?.hash(&mut frame_hash);
            let clip = scene.clips.get(&index).copied();
            if let Some(clip) = clip {
                serde_json::to_vec(&clip)?.hash(&mut frame_hash);
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
                    let pixels = Arc::new(
                        self.raster
                            .textures
                            .remove(&crate::key(&content.primitives[0], scale))
                            .ok_or("missing prepared raster")?,
                    );
                    let previous = self.entries.get(&slot);
                    let changed = previous.map_or_else(
                        || Damage::full(pixels.width(), pixels.height()),
                        |old| pixel_difference(&old.raster.pixels, &pixels),
                    );
                    let raster = Arc::new(Raster {
                        pixels,
                        key,
                        base_key: previous.map(|old| old.raster.key),
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
                Some(entry.raster.clone())
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
                signature(scene, &item.primitive, scale)?.hash(&mut key);
                own.push(item.id.clone());
            }
            let child_ids = children.remove(&layer.id).unwrap_or_default();
            own.hash(&mut key);
            child_ids.hash(&mut key);
            layer.parent.hash(&mut key);
            let key = key.finish();
            let slot = (layer.id.clone(), scale.to_bits());
            if self.nodes.get(&slot).is_none_or(|(old, _)| old.key != key) {
                self.nodes.insert(
                    slot.clone(),
                    (
                        Arc::new(RetainedLayer {
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
        self.frame = Some(Arc::new(Frame {
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
/// Paint into a borrowed SHM sub-strip: damage height limits raster traversal too.
pub fn paint(
    frame: &Frame,
    offset: (f32, f32),
    target: ShmTarget<'_>,
    damage: Damage,
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
    for y in damage.y..damage.y + damage.h {
        let i = (y * width + damage.x) as usize * 4;
        bytes[i..i + damage.w as usize * 4].fill(0);
    }
    let begin = damage.y as usize * width as usize * 4;
    let end = (damage.y + damage.h) as usize * width as usize * 4;
    let mut target = PixmapMut::from_bytes(&mut bytes[begin..end], width, damage.h)
        .ok_or("invalid SHM strip")?;
    let mut mask = Mask::new(width, damage.h).ok_or("damage mask")?;
    for item in &frame.items {
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
        mask.data_mut().fill(0);
        for y in clip.y..clip.y + clip.h {
            let i = ((y - damage.y) * width + clip.x) as usize;
            mask.data_mut()[i..i + clip.w as usize].fill(255);
        }
        let r = rect(&item.primitive);
        if let Some(raster) = &item.raster {
            target.draw_pixmap(
                ((r.x + offset.0) * scale).round() as i32,
                ((r.y + offset.1) * scale).round() as i32 - damage.y as i32,
                raster.pixels.as_ref().as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        } else if let Primitive::Rounded {
            radius,
            fill,
            border,
            ..
        } = &item.primitive
            && let Some(path) = crate::rounded(r, *radius)
        {
            let transform = Transform::from_scale(scale, scale)
                .post_translate(offset.0 * scale, offset.1 * scale - damage.y as f32);
            let mut color = Paint::default();
            color.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
            target.fill_path(&path, &color, FillRule::Winding, transform, Some(&mask));
            if let Some((rgba, line)) = border {
                color.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
                target.stroke_path(
                    &path,
                    &color,
                    &Stroke {
                        width: *line,
                        ..Stroke::default()
                    },
                    transform,
                    Some(&mask),
                );
            }
        }
    }
    if format == PixelFormat::Bgra {
        for y in damage.y..damage.y + damage.h {
            for x in damage.x..damage.x + damage.w {
                bytes[(y * width + x) as usize * 4..(y * width + x) as usize * 4 + 4].swap(0, 2);
            }
        }
    }
    Ok(())
}
#[derive(Default)]
pub struct TreeCpuRenderer {
    pub cache: TreeCache,
    flattened: Option<(u64, f32, Pixmap)>,
}
impl Renderer for TreeCpuRenderer {
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
        if scene.opacity >= 1. {
            return paint(frame, scene.offset, target, damage);
        }
        if scene.opacity <= 0. {
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
                opacity: scene.opacity,
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

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Immutable cached content, independent of animated position and opacity.
use crate::{CpuRenderer, Damage, PixelFormat, Renderer, Result, Scene, ShmTarget};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::Instant,
};
use tiny_skia::{Mask, Pixmap, PixmapMut, PixmapPaint, Transform};

#[derive(Clone)]
pub struct Layer {
    pub pixels: Arc<Pixmap>,
    pub scale: f32,
    pub key: u64,
    pub origin: (f32, f32),
}
#[derive(Default)]
pub struct LayerCache {
    pub layer: Option<Layer>,
    pub builds: usize,
    scales: std::collections::BTreeMap<u32, Layer>,
}
impl LayerCache {
    pub fn layer_count(&self) -> usize {
        self.scales.len()
    }
    pub fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        let key = content_key(scene, scale);
        if let Some(layer) = self
            .scales
            .get(&scale.to_bits())
            .filter(|layer| layer.key == key)
        {
            self.layer = Some(layer.clone());
            return Ok(());
        }
        let start = Instant::now();
        let mut content = scene.clone();
        content.offset = (-scene.bounds.x, -scene.bounds.y);
        content.opacity = 1.;
        let w = (scene.bounds.w * scale).ceil() as u32;
        let h = (scene.bounds.h * scale).ceil() as u32;
        if u64::from(w) * u64::from(h) * 4 > 64 * 1024 * 1024 {
            return Err("retained layer exceeds 64 MiB".into());
        }
        let mut pixels = Pixmap::new(w, h).ok_or("layer allocation")?;
        let mut raster = CpuRenderer::default();
        raster.prepare(&content, scale)?;
        let drawing = Instant::now();
        raster.render(
            &content,
            ShmTarget {
                bytes: pixels.data_mut(),
                width: w,
                height: h,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(w, h),
        )?;
        self.builds += 1;
        println!(
            "PROFILE stage=layer_bake raster_ms={:.3} total_ms={:.3} pixels={}x{} builds={}",
            drawing.elapsed().as_secs_f64() * 1000.,
            start.elapsed().as_secs_f64() * 1000.,
            w,
            h,
            self.builds
        );
        let layer = Layer {
            pixels: Arc::new(pixels),
            scale,
            key,
            origin: (scene.bounds.x, scene.bounds.y),
        };
        self.scales.insert(scale.to_bits(), layer.clone());
        while self.scales.len() > 8 {
            let key = *self
                .scales
                .keys()
                .find(|&&key| key != scale.to_bits())
                .unwrap();
            self.scales.remove(&key);
        }
        self.layer = Some(layer);
        Ok(())
    }
}
/// Hash deliberately excludes animated pose. Content changes invalidate the layer.
fn content_key(scene: &Scene, scale: f32) -> u64 {
    use crate::Primitive;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    scale.to_bits().hash(&mut h);
    scene.resource_revision.hash(&mut h);
    scene.font_files.hash(&mut h);
    // Native tray pixmaps can change while their stable path and bundle revision stay put.
    for (path, bytes) in &scene.asset_data {
        path.hash(&mut h);
        bytes.hash(&mut h);
    }
    for (index, rect) in &scene.clips {
        index.hash(&mut h);
        for value in [rect.x, rect.y, rect.w, rect.h] {
            value.to_bits().hash(&mut h);
        }
    }
    for f in [
        scene.bounds.x,
        scene.bounds.y,
        scene.bounds.w,
        scene.bounds.h,
    ] {
        f.to_bits().hash(&mut h);
    }
    for p in &scene.primitives {
        std::mem::discriminant(p).hash(&mut h);
        let r = match p {
            Primitive::Rounded { rect, .. }
            | Primitive::Text { rect, .. }
            | Primitive::Image { rect, .. }
            | Primitive::Shadow { rect, .. } => rect,
        };
        for f in [r.x, r.y, r.w, r.h] {
            f.to_bits().hash(&mut h);
        }
        match p {
            Primitive::Rounded {
                radius,
                fill,
                border,
                ..
            } => {
                radius.to_bits().hash(&mut h);
                fill.hash(&mut h);
                if let Some((c, w)) = border {
                    c.hash(&mut h);
                    w.to_bits().hash(&mut h);
                }
            }
            Primitive::Text {
                text,
                family,
                size,
                color,
                ..
            } => {
                text.hash(&mut h);
                family.hash(&mut h);
                size.to_bits().hash(&mut h);
                color.hash(&mut h);
            }
            Primitive::Image { path, .. } => path.hash(&mut h),
            Primitive::Shadow { radius, blur, .. } => {
                radius.to_bits().hash(&mut h);
                blur.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}
#[derive(Default)]
pub struct RetainedCpuRenderer {
    pub cache: LayerCache,
    mask: Option<Mask>,
    previous: Option<Damage>,
}
impl Renderer for RetainedCpuRenderer {
    fn resource_counts(&self) -> (usize, usize) {
        (self.cache.layer_count(), 0)
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        self.cache.prepare(scene, scale)
    }
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, d: Damage) -> Result<()> {
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        if d.w == 0 || d.h == 0 {
            return Ok(());
        }
        if d.x + d.w > width || d.y + d.h > height {
            return Err("damage outside SHM".into());
        }
        for y in d.y..d.y + d.h {
            let i = (y * width + d.x) as usize * 4;
            bytes[i..i + d.w as usize * 4].fill(0);
        }
        if self
            .mask
            .as_ref()
            .is_none_or(|m| m.width() != width || m.height() != height)
        {
            self.mask = Some(Mask::new(width, height).ok_or("clip allocation")?);
            self.previous = None;
        }
        let mask = self.mask.as_mut().unwrap();
        if let Some(old) = self.previous {
            for y in old.y..old.y + old.h {
                let i = (y * width + old.x) as usize;
                mask.data_mut()[i..i + old.w as usize].fill(0);
            }
        }
        for y in d.y..d.y + d.h {
            let i = (y * width + d.x) as usize;
            mask.data_mut()[i..i + d.w as usize].fill(255);
        }
        self.previous = Some(d);
        let layer = self.cache.layer.as_ref().ok_or("layer not prepared")?;
        let mut frame = PixmapMut::from_bytes(bytes, width, height).ok_or("invalid SHM")?;
        frame.draw_pixmap(
            ((scene.offset.0 + layer.origin.0) * scale).round() as i32,
            ((scene.offset.1 + layer.origin.1) * scale).round() as i32,
            layer.pixels.as_ref().as_ref(),
            &PixmapPaint {
                opacity: scene.opacity,
                ..PixmapPaint::default()
            },
            Transform::identity(),
            Some(mask),
        );
        if format == PixelFormat::Bgra {
            crate::swizzle(bytes, width, d);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Primitive, Rect};
    fn scene() -> Scene {
        Scene {
            font_files: vec![],
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            width: 40.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(-2., -2., 44., 34.),
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(0., 0., 40., 30.),
                radius: 6.,
                fill: [200, 30, 60, 255],
                border: None,
            }],
        }
    }
    #[test]
    fn same_path_new_asset_pixels_replace_the_retained_image() {
        let mut scene = scene();
        let path = std::path::PathBuf::from("tray.png");
        scene.primitives.push(crate::Primitive::Image {
            path: path.clone(),
            rect: crate::Rect::new(0., 0., 10., 10.),
        });
        let mut icon = tiny_skia::Pixmap::new(10, 10).unwrap();
        icon.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        scene.asset_data.insert(
            path.clone(),
            std::sync::Arc::from(icon.encode_png().unwrap()),
        );
        let mut cache = LayerCache::default();
        cache.prepare(&scene, 1.).unwrap();
        let before = cache.layer.as_ref().unwrap().pixels.pixel(2, 2).unwrap();
        icon.fill(tiny_skia::Color::from_rgba8(0, 255, 0, 255));
        scene
            .asset_data
            .insert(path, std::sync::Arc::from(icon.encode_png().unwrap()));
        cache.prepare(&scene, 1.).unwrap();
        let after = cache.layer.as_ref().unwrap().pixels.pixel(2, 2).unwrap();
        assert_eq!((before.red(), before.green()), (255, 0));
        assert_eq!((after.red(), after.green()), (0, 255));
    }
    #[test]
    fn multiple_scales_reuse_their_own_cached_layer() {
        let scene = scene();
        let mut cache = LayerCache::default();
        for _ in 0..3 {
            for scale in [1., 1.5, 2.] {
                cache.prepare(&scene, scale).unwrap();
            }
        }
        assert_eq!(cache.builds, 3);
    }
    #[test]
    fn pose_reuses_layer_and_content_edits_rebuild_it() {
        let mut s = scene();
        let mut cache = LayerCache::default();
        cache.prepare(&s, 1.5).unwrap();
        for i in 0..40 {
            s.offset = (i as f32, 0.);
            s.opacity = i as f32 / 40.;
            cache.prepare(&s, 1.5).unwrap();
        }
        assert_eq!(cache.builds, 1);
        if let Primitive::Rounded { fill, .. } = &mut s.primitives[0] {
            *fill = [30, 200, 60, 255];
        }
        cache.prepare(&s, 1.5).unwrap();
        assert_eq!(cache.builds, 2);
    }
    #[test]
    fn retained_partial_aged_buffers_match_full_composition() {
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            let mut retained = RetainedCpuRenderer::default();
            let mut reference_renderer = RetainedCpuRenderer::default();
            let mut s = scene();
            let mut buffers = [
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
            ];
            let mut pending = [Damage::full(120, 90); 3];
            let mut reference = vec![0; 120 * 90 * 4];
            let mut previous = s.bounds();
            for i in 0..30 {
                s.offset = ((i * 4 % 50) as f32, (i * 2 % 30) as f32);
                let d = previous.union(s.bounds()).pixels(1.5, 120, 90);
                previous = s.bounds();
                for p in &mut pending {
                    *p = p.union(d);
                }
                let b = i as usize % 3;
                retained.prepare(&s, 1.5).unwrap();
                reference_renderer.prepare(&s, 1.5).unwrap();
                retained
                    .render(
                        &s,
                        ShmTarget {
                            bytes: &mut buffers[b],
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        pending[b],
                    )
                    .unwrap();
                pending[b] = Damage::full(0, 0);
                reference_renderer
                    .render(
                        &s,
                        ShmTarget {
                            bytes: &mut reference,
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        Damage::full(120, 90),
                    )
                    .unwrap();
                let diffs: Vec<_> = buffers[b]
                    .iter()
                    .zip(&reference)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(idx, (a, b))| (idx, *a, *b))
                    .collect();
                assert!(
                    diffs.is_empty(),
                    "frame {i}, {format:?}, count={}, first={:?}, max={}",
                    diffs.len(),
                    diffs.first(),
                    diffs
                        .iter()
                        .map(|(_, a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap_or(0)
                );
            }
            assert_eq!(retained.cache.builds, 1);
        }
    }
    #[test]
    fn opaque_retained_source_matches_vector_when_fully_visible() {
        let s = scene();
        let mut retained = RetainedCpuRenderer::default();
        let mut direct = CpuRenderer::default();
        let mut a = vec![0; 120 * 90 * 4];
        let mut b = a.clone();
        retained.prepare(&s, 1.5).unwrap();
        direct.prepare(&s, 1.5).unwrap();
        retained
            .render(
                &s,
                ShmTarget {
                    bytes: &mut a,
                    width: 120,
                    height: 90,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                Damage::full(120, 90),
            )
            .unwrap();
        direct
            .render(
                &s,
                ShmTarget {
                    bytes: &mut b,
                    width: 120,
                    height: 90,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                Damage::full(120, 90),
            )
            .unwrap();
        assert!(a == b, "opaque source changed");
    }
}

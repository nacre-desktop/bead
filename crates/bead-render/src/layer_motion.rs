// SPDX-License-Identifier: MIT
//! Immutable pixels and presentation planes for optional compositor animation.
//!
//! A plan describes presentation, independent of a particular display protocol.
//! Preparing it performs the slide's sole client paint. A presenter keeps those
//! pixels attached until the compositor reports completion or cancellation.

use std::time::Duration;

use crate::{Damage, PixelFormat, Primitive, Rect, Renderer, Result, Scene, ShmTarget};

fn finite_rect(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.w.is_finite()
        && rect.h.is_finite()
        && (rect.x + rect.w).is_finite()
        && (rect.y + rect.h).is_finite()
        && rect.w >= 0.
        && rect.h >= 0.
}

fn extend_bounds(bounds: &mut Option<Rect>, next: Rect) -> Option<()> {
    if !finite_rect(next) {
        return None;
    }
    let merged = (*bounds).map_or(next, |current| current.union(next));
    if !finite_rect(merged) {
        return None;
    }
    *bounds = Some(merged);
    Some(())
}

fn bitmap_ink_bounds(
    rect: Rect,
    scale: f32,
    offset: (f32, f32),
    origin: (u32, u32),
    size: (u32, u32),
) -> Option<Rect> {
    // Match TreeCpuRenderer's bitmap placement exactly: round the combined
    // logical position after multiplying in f32, then add any raster origin.
    let x = ((rect.x + offset.0) * scale).round();
    let y = ((rect.y + offset.1) * scale).round();
    if !x.is_finite()
        || !y.is_finite()
        || x < i32::MIN as f32
        || x > i32::MAX as f32
        || y < i32::MIN as f32
        || y > i32::MAX as f32
    {
        return None;
    }
    let left = f64::from(x) + f64::from(origin.0);
    let top = f64::from(y) + f64::from(origin.1);
    let right = left + f64::from(size.0);
    let bottom = top + f64::from(size.1);
    let scale = f64::from(scale);
    let bounds = Rect::new(
        (left / scale) as f32,
        (top / scale) as f32,
        ((right - left) / scale) as f32,
        ((bottom - top) / scale) as f32,
    );
    finite_rect(bounds).then_some(bounds)
}

/// Logical viewport covered by the physical atlas after pixel rounding.
/// This may extend past the nominal logical size at fractional scales.
pub fn motion_atlas_viewport(width: u32, height: u32, scale: f32) -> Option<Rect> {
    if width == 0 || height == 0 || !scale.is_finite() || scale <= 0. {
        return None;
    }
    let width = (f64::from(width) * f64::from(scale)).ceil();
    let height = (f64::from(height) * f64::from(scale)).ceil();
    let scale = f64::from(scale);
    let viewport = Rect::new(0., 0., (width / scale) as f32, (height / scale) as f32);
    finite_rect(viewport).then_some(viewport)
}

/// Conservative surface-local paint bounds for a scene at `scale` and `offset`.
/// Clips are deliberately ignored. A missing or malformed extent is unsafe for
/// a viewport-cropped atlas and must retain the frame-driven path.
pub fn scene_ink_bounds(scene: &Scene, scale: f32, offset: (f32, f32)) -> Option<Rect> {
    if !scale.is_finite() || scale <= 0. || !offset.0.is_finite() || !offset.1.is_finite() {
        return None;
    }
    let mut bounds = None;
    for (index, primitive) in scene.primitives.iter().enumerate() {
        let primitive_offset = scene.content_offset(index);
        let translated_offset = (offset.0 + primitive_offset.0, offset.1 + primitive_offset.1);
        let ink = primitive_ink_bounds(primitive, scale, translated_offset)?;
        extend_bounds(&mut bounds, ink)?;
    }

    if let Some(frame) = scene.prepared.get(&scale.to_bits()) {
        if frame.scale.to_bits() != scale.to_bits() {
            return None;
        }
        for item in &frame.items {
            let primitive = if let Some(raster) = &item.raster {
                bitmap_ink_bounds(
                    crate::tree::rect(&item.primitive),
                    scale,
                    offset,
                    raster.origin,
                    (raster.pixels.width(), raster.pixels.height()),
                )?
            } else {
                primitive_ink_bounds(&item.primitive, scale, offset)?
            };
            extend_bounds(&mut bounds, primitive)?;
        }
    }

    bounds.or_else(|| Some(Rect::new(offset.0, offset.1, 0., 0.)))
}

fn primitive_ink_bounds(primitive: &Primitive, scale: f32, offset: (f32, f32)) -> Option<Rect> {
    let rect = crate::tree::rect(primitive);
    if !finite_rect(rect) {
        return None;
    }
    let antialias = 2. / scale;
    let bounds = match primitive {
        Primitive::Arc {
            start_angle,
            sweep_angle,
            stroke_width,
            ..
        } => {
            if !start_angle.is_finite()
                || !sweep_angle.is_finite()
                || !(-360. ..=360.).contains(start_angle)
                || !(-360. ..=360.).contains(sweep_angle)
                || !stroke_width.is_finite()
                || *stroke_width < 0.
            {
                return None;
            }
            let pad = *stroke_width * 0.5 + antialias;
            Rect::new(
                rect.x - pad,
                rect.y - pad,
                rect.w + 2. * pad,
                rect.h + 2. * pad,
            )
            .translated(offset.0, offset.1)
        }
        Primitive::Polyline {
            points,
            stroke_width,
            ..
        } => {
            if points.len() < 2
                || points.len() > 256
                || points
                    .iter()
                    .flatten()
                    .any(|value| !value.is_finite() || !(0. ..=1.).contains(value))
                || !stroke_width.is_finite()
                || *stroke_width < 0.
            {
                return None;
            }
            let pad = *stroke_width * 0.5 + antialias;
            Rect::new(
                rect.x - pad,
                rect.y - pad,
                rect.w + 2. * pad,
                rect.h + 2. * pad,
            )
            .translated(offset.0, offset.1)
        }
        Primitive::Rounded { radius, border, .. } => {
            if !radius.is_finite() {
                return None;
            }
            let pad = match border {
                Some((_, width)) if width.is_finite() && *width >= 0. => width * 0.5 + antialias,
                Some(_) => return None,
                None => antialias,
            };
            Rect::new(
                rect.x - pad,
                rect.y - pad,
                rect.w + 2. * pad,
                rect.h + 2. * pad,
            )
            .translated(offset.0, offset.1)
        }
        Primitive::Text { size, shadow, .. } => {
            if !size.is_finite()
                || *size < 0.
                || shadow.is_some_and(|shadow| {
                    !shadow.blur.is_finite() || shadow.blur < 0. || !shadow.offset_y.is_finite()
                })
            {
                return None;
            }
            bitmap_ink_bounds(rect, scale, offset, (0, 0), crate::dimensions(rect, scale))?
        }
        Primitive::Image { blur, .. } => {
            if !blur.is_finite() || *blur < 0. {
                return None;
            }
            bitmap_ink_bounds(rect, scale, offset, (0, 0), crate::dimensions(rect, scale))?
        }
        Primitive::Shadow { radius, blur, .. } => {
            if !radius.is_finite() || !blur.is_finite() || *radius < 0. || *blur < 0. {
                return None;
            }
            bitmap_ink_bounds(rect, scale, offset, (0, 0), crate::dimensions(rect, scale))?
        }
    };
    finite_rect(bounds).then_some(bounds)
}

/// Whether a translated, viewport-cropped source can present every pixel that
/// enters the viewport in the target pose.
pub fn translation_preserves_clip(
    source_bounds: Rect,
    viewport: Rect,
    closed_offset: (f32, f32),
) -> bool {
    if !source_bounds.x.is_finite()
        || !source_bounds.y.is_finite()
        || !source_bounds.w.is_finite()
        || !source_bounds.h.is_finite()
        || !viewport.x.is_finite()
        || !viewport.y.is_finite()
        || !viewport.w.is_finite()
        || !viewport.h.is_finite()
        || !closed_offset.0.is_finite()
        || !closed_offset.1.is_finite()
    {
        return false;
    }
    !((closed_offset.0 < 0. && source_bounds.x + source_bounds.w > viewport.x + viewport.w)
        || (closed_offset.0 > 0. && source_bounds.x < viewport.x)
        || (closed_offset.1 < 0. && source_bounds.y + source_bounds.h > viewport.y + viewport.h)
        || (closed_offset.1 > 0. && source_bounds.y < viewport.y))
}

/// One bounded, premultiplied pixel buffer, in the renderer's target format.
pub struct PixelBuffer {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub scale: f32,
    pub format: PixelFormat,
    pub pixels: Vec<u8>,
    _charge: bead_core::cache::Charge,
}

impl PixelBuffer {
    /// Transfer pixels to the surface transport without retaining a second copy.
    /// Keep this object alive while the corresponding transport buffer is in use
    /// so its storage remains accounted for until the animation retires.
    pub fn take_pixels(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pixels)
    }
}

/// Opacity can fade continuously or become opaque above a progress threshold.
/// The latter preserves opaque slides, including their transparent final pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpacityMode {
    Linear,
    OpaqueAbove(f32),
}

impl OpacityMode {
    pub fn apply(self, opacity: f32) -> f32 {
        match self {
            Self::OpaqueAbove(threshold) if opacity > threshold => 1.,
            _ => opacity,
        }
    }
}

/// Logical destination and clip in the stationary layer-surface coordinate frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneState {
    pub destination: Rect,
    pub clip: Rect,
    pub opacity: f32,
}

impl PlaneState {
    /// Sample with an already eased, normalized timeline progress.
    pub fn interpolate(self, to: Self, progress: f32, opacity_mode: OpacityMode) -> Self {
        let progress = progress.clamp(0., 1.);
        let scalar = |a: f32, b: f32| a + (b - a) * progress;
        let rect = |a: Rect, b: Rect| {
            Rect::new(
                scalar(a.x, b.x),
                scalar(a.y, b.y),
                scalar(a.w, b.w),
                scalar(a.h, b.h),
            )
        };
        Self {
            destination: rect(self.destination, to.destination),
            clip: rect(self.clip, to.clip),
            opacity: opacity_mode.apply(scalar(self.opacity, to.opacity)),
        }
    }
}

/// One source crop of the shared atlas and its two presentation poses.
#[derive(Clone, Debug)]
pub struct LayerMotionPlane {
    pub id: u32,
    pub source: Rect,
    pub from: PlaneState,
    pub to: PlaneState,
    pub opacity_mode: OpacityMode,
}

/// All planes share one duration, curve, start request and completion event.
pub struct LayerMotionPlan {
    pub duration: Duration,
    pub curve: bead_core::animation::Curve,
    pub retarget: bool,
    pub atlas: PixelBuffer,
    pub planes: Vec<LayerMotionPlane>,
}

impl LayerMotionPlan {
    /// Paint the complete open scene once for a whole-scene translation/fade.
    /// `scene.offset` must be the resting open placement in this surface.
    /// `width` and `height` are the logical viewport destination; atlas dimensions
    /// are rounded up to physical pixels without changing that destination.
    ///
    /// A revealing opening reshapes body artwork independently of content. Such
    /// a scene requires a decomposed atlas; flattening it here would change its
    /// silhouette. Return `None` before painting so callers retain their existing
    /// renderer for that case, reduced motion, and oversized surfaces.
    #[allow(clippy::too_many_arguments)]
    pub fn for_slide(
        scene: &Scene,
        opening: bool,
        width: u32,
        height: u32,
        scale: f32,
        format: PixelFormat,
        renderer: &mut dyn Renderer,
    ) -> Result<Option<Self>> {
        let Some(motion) = scene.motion.as_ref() else {
            return Ok(None);
        };
        if bead_core::animation::reduced_motion()
            || opening && motion.reveal
            || scene.slide.is_some()
            || width == 0
            || height == 0
            || !scale.is_finite()
            || scale <= 0.
        {
            return Ok(None);
        }
        let (duration, curve) = motion.transition(opening);
        if !duration.is_finite() || !(0.001..=10.).contains(&duration) {
            return Ok(None);
        }
        let closed_offset = motion.edge.closed_offset(
            motion.slide_distance.unwrap_or(scene.bounds.w),
            motion.slide_distance.unwrap_or(scene.bounds.h),
            0.,
        );
        let Some(ink_bounds) = scene_ink_bounds(scene, scale, scene.offset) else {
            return Ok(None);
        };
        let Some(atlas_viewport) = motion_atlas_viewport(width, height, scale) else {
            return Ok(None);
        };
        // The atlas contains only pixels inside this viewport. If the closed
        // pose moves hidden content into view, the compositor cannot recover
        // those pixels from the already-cropped source; keep the CPU path.
        if !translation_preserves_clip(ink_bounds, atlas_viewport, closed_offset) {
            return Ok(None);
        }
        let pixel_width = (width as f64 * scale as f64).ceil();
        let pixel_height = (height as f64 * scale as f64).ceil();
        if pixel_width > u32::MAX as f64 || pixel_height > u32::MAX as f64 {
            return Ok(None);
        }
        let (pixel_width, pixel_height) = (pixel_width as u32, pixel_height as u32);
        let Some(stride) = pixel_width.checked_mul(4) else {
            return Ok(None);
        };
        let Some(bytes) = (stride as usize).checked_mul(pixel_height as usize) else {
            return Ok(None);
        };
        // One slide buffer, charged for its entire lifetime. Match the CPU
        // composite size ceiling; this buffer is additional transient storage.
        const LIMIT: usize = 64 * bead_core::cache::MIB;
        if bytes > LIMIT {
            return Ok(None);
        }
        let budget = bead_core::cache::Budget::new("layer-motion-pixels", LIMIT);
        let charge = budget.reserve(bytes)?;
        let mut pixels = vec![0; bytes];
        let mut final_scene = scene.clone();
        final_scene.panel_offset(1., opening);
        // Motion is now entirely presentation state. It must not trigger the
        // CPU slide caches or per-frame body processing while painting the atlas.
        final_scene.motion = None;
        renderer.prepare_on_ui(&final_scene, scale)?;
        renderer.render(
            &final_scene,
            ShmTarget {
                bytes: &mut pixels,
                width: pixel_width,
                height: pixel_height,
                scale,
                format,
            },
            Damage::full(pixel_width, pixel_height),
        )?;

        let source = Rect::new(0., 0., width as f32, height as f32);
        let resting = PlaneState {
            destination: source,
            clip: source,
            opacity: 1.,
        };
        let closed = PlaneState {
            destination: source.translated(closed_offset.0, closed_offset.1),
            clip: source,
            opacity: 0.,
        };
        Ok(Some(Self {
            duration: Duration::from_secs_f64(duration),
            curve,
            retarget: false,
            atlas: PixelBuffer {
                width: pixel_width,
                height: pixel_height,
                stride,
                scale,
                format,
                pixels,
                _charge: charge,
            },
            planes: vec![LayerMotionPlane {
                id: 0,
                source,
                from: if opening { closed } else { resting },
                to: if opening { resting } else { closed },
                opacity_mode: if motion.fade {
                    OpacityMode::Linear
                } else {
                    OpacityMode::OpaqueAbove(0.001)
                },
            }],
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Motion, Primitive, tree::TreeCpuRenderer};

    struct NoPaint;
    impl Renderer for NoPaint {
        fn prepare(&mut self, _: &Scene, _: f32) -> Result<()> {
            panic!("unsupported animation must not prepare")
        }
        fn render(&mut self, _: &Scene, _: ShmTarget<'_>, _: Damage) -> Result<()> {
            panic!("unsupported animation must not paint")
        }
    }

    fn paint_scene(scene: &Scene, offset: (f32, f32), width: u32, height: u32) -> Result<Vec<u8>> {
        let mut scene = scene.clone();
        scene.motion = None;
        scene.offset = offset;
        let mut pixels = vec![0; width as usize * height as usize * 4];
        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare(&scene, 1.)?;
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )?;
        Ok(pixels)
    }

    fn scene(reveal: bool, fade: bool) -> Scene {
        Scene {
            opacity: 1.,
            motion: Some(Motion {
                id: "panel".into(),
                open: true,
                duration: 0.3,
                return_duration: None,
                return_curve: None,
                spring: false,
                curve: None,
                edge: bead_core::Edge::Bottom,
                slide_distance: Some(32.),
                fade,
                reveal,
                progress: 1.,
                parts: Vec::new(),
                art: None,
            }),
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: Vec::new(),
            width: 96.,
            slide: None,
            primitives: vec![Primitive::Rounded {
                rect: Rect::new(8., 8., 80., 56.),
                radius: 6.,
                fill: [35, 80, 120, 255],
                border: None,
            }],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 96., 72.),
        }
    }

    #[test]
    fn a_growing_body_is_rejected_before_any_prepare_or_paint() -> Result<()> {
        assert!(
            LayerMotionPlan::for_slide(
                &scene(true, false),
                true,
                96,
                72,
                1.5,
                PixelFormat::Rgba,
                &mut NoPaint,
            )?
            .is_none()
        );
        Ok(())
    }

    #[test]
    fn one_paint_reproduces_close_pixels_and_terminal_opacity_at_fractional_scale() -> Result<()> {
        struct CountPaint {
            renderer: TreeCpuRenderer,
            prepares: usize,
            paints: usize,
        }
        impl Renderer for CountPaint {
            fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
                self.prepares += 1;
                self.renderer.prepare(scene, scale)
            }
            fn render(
                &mut self,
                scene: &Scene,
                target: ShmTarget<'_>,
                damage: Damage,
            ) -> Result<()> {
                self.paints += 1;
                self.renderer.render(scene, target, damage)
            }
        }
        for fade in [false, true] {
            let scene = scene(true, fade);
            let mut renderer = CountPaint {
                renderer: TreeCpuRenderer::default(),
                prepares: 0,
                paints: 0,
            };
            let plan = LayerMotionPlan::for_slide(
                &scene,
                false,
                96,
                72,
                1.5,
                PixelFormat::Rgba,
                &mut renderer,
            )?
            .unwrap();
            let plane = &plan.planes[0];
            for progress in [0., 0.25, 0.5, 0.75, 0.9995, 1.] {
                let pose = plane
                    .from
                    .interpolate(plane.to, progress, plane.opacity_mode);
                let mut legacy = scene.clone();
                let open_progress = 1. - progress;
                legacy.offset = legacy.panel_offset(open_progress, false);
                legacy.opacity = legacy.motion.as_ref().unwrap().opacity(open_progress);
                assert!((pose.opacity - legacy.opacity).abs() < 0.00001);
                assert_eq!(pose.destination.y, legacy.offset.1);
                // At these integer-pixel translations GPU composition samples
                // the exact same immutable texels as the ordinary CPU painter.
                if progress == 0.9995 {
                    continue;
                }
                let image = tiny_skia::PixmapRef::from_bytes(&plan.atlas.pixels, 144, 108).unwrap();
                let mut composed = tiny_skia::Pixmap::new(144, 108).unwrap();
                composed.draw_pixmap(
                    (pose.destination.x * 1.5).round() as i32,
                    (pose.destination.y * 1.5).round() as i32,
                    image,
                    &tiny_skia::PixmapPaint {
                        opacity: pose.opacity,
                        ..Default::default()
                    },
                    tiny_skia::Transform::identity(),
                    None,
                );
                let mut expected = vec![0; 144 * 108 * 4];
                let mut direct = TreeCpuRenderer::default();
                direct.prepare(&legacy, 1.5)?;
                direct.render(
                    &legacy,
                    ShmTarget {
                        bytes: &mut expected,
                        width: 144,
                        height: 108,
                        scale: 1.5,
                        format: PixelFormat::Rgba,
                    },
                    Damage::full(144, 108),
                )?;
                let worst = composed
                    .data()
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                assert!(
                    worst <= 2,
                    "fade={fade} progress={progress} difference={worst}"
                );
            }
            assert_eq!((renderer.prepares, renderer.paints), (1, 1));
        }
        Ok(())
    }

    #[test]
    fn fractional_pixels_do_not_change_the_logical_viewport() -> Result<()> {
        let plan = LayerMotionPlan::for_slide(
            &scene(false, true),
            true,
            97,
            73,
            1.5,
            PixelFormat::Bgra,
            &mut TreeCpuRenderer::default(),
        )?
        .unwrap();
        assert_eq!((plan.atlas.width, plan.atlas.height), (146, 110));
        assert_eq!(plan.atlas.stride, 584);
        assert_eq!(plan.planes[0].source, Rect::new(0., 0., 97., 73.));
        assert_eq!(plan.planes[0].to.destination, plan.planes[0].source);
        Ok(())
    }

    #[test]
    fn bitmap_extent_uses_exact_shadow_pixels_at_aligned_fractional_scale() -> Result<()> {
        struct CountPaint {
            renderer: TreeCpuRenderer,
            paints: usize,
        }
        impl Renderer for CountPaint {
            fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
                self.renderer.prepare(scene, scale)
            }
            fn render(
                &mut self,
                scene: &Scene,
                target: ShmTarget<'_>,
                damage: Damage,
            ) -> Result<()> {
                self.paints += 1;
                self.renderer.render(scene, target, damage)
            }
        }

        let mut scene = scene(false, false);
        scene.width = 446.;
        scene.bounds = Rect::new(-32., -32., 446., 360.);
        scene.offset = (32., 32.);
        scene.motion.as_mut().unwrap().edge = bead_core::Edge::Top;
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Shadow {
            rect: Rect::new(-32., -2., 446., 330.),
            radius: 16.,
            blur: 12.,
        }];

        let ink = scene_ink_bounds(&scene, 1.5, scene.offset).unwrap();
        assert_eq!(ink, Rect::new(0., 30., 446., 330.));
        assert!(translation_preserves_clip(
            ink,
            motion_atlas_viewport(446, 360, 1.5).unwrap(),
            (0., -32.),
        ));

        let mut renderer = CountPaint {
            renderer: TreeCpuRenderer::default(),
            paints: 0,
        };
        let plan = LayerMotionPlan::for_slide(
            &scene,
            false,
            446,
            360,
            1.5,
            PixelFormat::Rgba,
            &mut renderer,
        )?;
        assert!(plan.is_some());
        assert_eq!(renderer.paints, 1);
        Ok(())
    }

    #[test]
    fn bitmap_extent_fits_odd_fractional_atlas_without_changing_logical_plane() -> Result<()> {
        struct CountPaint {
            renderer: TreeCpuRenderer,
            paints: usize,
        }
        impl Renderer for CountPaint {
            fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
                self.renderer.prepare(scene, scale)
            }
            fn render(
                &mut self,
                scene: &Scene,
                target: ShmTarget<'_>,
                damage: Damage,
            ) -> Result<()> {
                self.paints += 1;
                self.renderer.render(scene, target, damage)
            }
        }

        let mut scene = scene(false, false);
        scene.width = 97.;
        scene.bounds = Rect::new(0., 0., 97., 73.);
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Shadow {
            rect: Rect::new(0., 0., 97., 73.),
            radius: 8.,
            blur: 4.,
        }];
        let ink = scene_ink_bounds(&scene, 1.5, scene.offset).unwrap();
        assert_eq!(ink.w, 146. / 1.5);
        assert_eq!(ink.h, 110. / 1.5);

        let mut renderer = CountPaint {
            renderer: TreeCpuRenderer::default(),
            paints: 0,
        };
        let plan = LayerMotionPlan::for_slide(
            &scene,
            false,
            97,
            73,
            1.5,
            PixelFormat::Rgba,
            &mut renderer,
        )?
        .unwrap();
        assert_eq!((plan.atlas.width, plan.atlas.height), (146, 110));
        assert_eq!(plan.planes[0].source, Rect::new(0., 0., 97., 73.));
        assert_eq!(renderer.paints, 1);
        Ok(())
    }

    #[test]
    fn clipped_source_pixels_that_enter_from_closed_edge_fall_back_before_paint() -> Result<()> {
        struct NoPaint;
        impl Renderer for NoPaint {
            fn prepare(&mut self, _: &Scene, _: f32) -> Result<()> {
                panic!("unsafe clipped motion must not prepare")
            }
            fn render(&mut self, _: &Scene, _: ShmTarget<'_>, _: Damage) -> Result<()> {
                panic!("unsafe clipped motion must not paint")
            }
        }

        let mut clipped = scene(false, false);
        clipped.motion.as_mut().unwrap().edge = bead_core::Edge::Top;
        clipped.offset = (0., 0.);
        assert!(
            LayerMotionPlan::for_slide(
                &clipped,
                false,
                96,
                40,
                1.,
                PixelFormat::Rgba,
                &mut NoPaint,
            )?
            .is_none()
        );

        // The CPU path paints the full scene at the sampled offset before the
        // output clip, so pixels from y=40..72 can enter the visible viewport.
        let mut legacy = clipped.clone();
        legacy.offset = (0., -16.);
        legacy.motion = None;
        let mut pixels = vec![0; 96 * 40 * 4];
        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare(&legacy, 1.)?;
        renderer.render(
            &legacy,
            ShmTarget {
                bytes: &mut pixels,
                width: 96,
                height: 40,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(96, 40),
        )?;
        let visible_body_pixel = ((35 * 96 + 48) * 4) as usize;
        assert!(
            pixels[visible_body_pixel..visible_body_pixel + 4]
                .iter()
                .any(|channel| *channel != 0)
        );

        let mut cropped_source = clipped.clone();
        cropped_source.motion = None;
        let mut cropped_pixels = vec![0; 96 * 40 * 4];
        let mut crop_renderer = TreeCpuRenderer::default();
        crop_renderer.prepare(&cropped_source, 1.)?;
        crop_renderer.render(
            &cropped_source,
            ShmTarget {
                bytes: &mut cropped_pixels,
                width: 96,
                height: 40,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(96, 40),
        )?;
        let cropped_image = tiny_skia::PixmapRef::from_bytes(&cropped_pixels, 96, 40).unwrap();
        let mut translated_crop = tiny_skia::Pixmap::new(96, 40).unwrap();
        translated_crop.draw_pixmap(
            0,
            -16,
            cropped_image,
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
        assert_eq!(
            &translated_crop.data()[visible_body_pixel..visible_body_pixel + 4],
            &[0; 4]
        );
        Ok(())
    }

    #[test]
    fn clipped_source_that_moves_further_off_bottom_edge_remains_supported() -> Result<()> {
        struct CountPaint {
            renderer: TreeCpuRenderer,
            paints: usize,
        }
        impl Renderer for CountPaint {
            fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
                self.renderer.prepare(scene, scale)
            }
            fn render(
                &mut self,
                scene: &Scene,
                target: ShmTarget<'_>,
                damage: Damage,
            ) -> Result<()> {
                self.paints += 1;
                self.renderer.render(scene, target, damage)
            }
        }

        let mut clipped = scene(false, false);
        clipped.motion.as_mut().unwrap().edge = bead_core::Edge::Bottom;
        let mut renderer = CountPaint {
            renderer: TreeCpuRenderer::default(),
            paints: 0,
        };
        let plan = LayerMotionPlan::for_slide(
            &clipped,
            false,
            96,
            40,
            1.,
            PixelFormat::Rgba,
            &mut renderer,
        )?;
        assert!(plan.is_some());
        assert_eq!(renderer.paints, 1);
        Ok(())
    }

    #[test]
    fn underreported_scene_bounds_do_not_drop_ink_that_cpu_motion_reveals() -> Result<()> {
        let mut scene = scene(false, false);
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 32.);
        scene.motion.as_mut().unwrap().edge = bead_core::Edge::Left;
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Rounded {
            rect: Rect::new(40., 4., 12., 12.),
            radius: 1.,
            fill: [220, 70, 30, 255],
            border: None,
        }];

        // A 32x32 open-pose atlas clips this item completely, while the CPU
        // close pose translates it to x=24..36 and paints its x=24..32 portion.
        let open_pixels = paint_scene(&scene, (0., 0.), 32, 32)?;
        let closed_pixels = paint_scene(&scene, (-16., 0.), 32, 32)?;
        let visible = ((8 * 32 + 28) * 4) as usize;
        assert_eq!(&open_pixels[visible..visible + 4], &[0; 4]);
        assert!(
            closed_pixels[visible..visible + 4]
                .iter()
                .any(|channel| *channel != 0)
        );

        assert!(LayerMotionPlan::for_slide(
            &scene,
            false,
            32,
            32,
            1.,
            PixelFormat::Rgba,
            &mut NoPaint,
        )?
        .is_none());
        Ok(())
    }

    #[test]
    fn border_paint_outset_is_included_in_source_ink_bounds() -> Result<()> {
        let mut scene = scene(false, false);
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 32.);
        scene.motion.as_mut().unwrap().edge = bead_core::Edge::Left;
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Rounded {
            rect: Rect::new(28., 4., 4., 12.),
            radius: 1.,
            fill: [220, 70, 30, 255],
            border: Some(([255, 255, 255, 255], 4.)),
        }];

        let ink = scene_ink_bounds(&scene, 1., scene.offset).unwrap();
        assert!(ink.x + ink.w > 32.);
        assert!(LayerMotionPlan::for_slide(
            &scene,
            false,
            32,
            32,
            1.,
            PixelFormat::Rgba,
            &mut NoPaint,
        )?
        .is_none());
        Ok(())
    }

    #[test]
    fn fractional_bitmap_rounding_cannot_hide_entering_image_or_shadow_pixels() -> Result<()> {
        for scale in [1., 1.5] {
            for primitive in [
                Primitive::Image {
                    blur: 0.,
                    fit: crate::ImageFit::Fill,
                    clip_polygon: None,
                    rect: Rect::new(24.6, 4., 7.4, 12.),
                    path: "unused.png".into(),
                },
                Primitive::Shadow {
                    rect: Rect::new(24.6, 4., 7.4, 12.),
                    radius: 2.,
                    blur: 1.,
                },
            ] {
                let mut scene = scene(false, false);
                scene.width = 32.;
                scene.bounds = Rect::new(0., 0., 32., 32.);
                scene.motion.as_mut().unwrap().edge = bead_core::Edge::Left;
                scene.motion.as_mut().unwrap().slide_distance = Some(32.);
                scene.primitives = vec![primitive];

                let ink = scene_ink_bounds(&scene, scale, scene.offset).unwrap();
                assert!(ink.x + ink.w > 32., "scale={scale} ink={ink:?}");
                assert!(
                    LayerMotionPlan::for_slide(
                        &scene,
                        false,
                        32,
                        32,
                        scale,
                        PixelFormat::Rgba,
                        &mut NoPaint,
                    )?
                    .is_none()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn nonfinite_item_and_union_extents_fall_back() {
        let mut scene = scene(false, false);
        scene.primitives = vec![Primitive::Rounded {
            rect: Rect::new(f32::MAX, 0., f32::MAX, 8.),
            radius: 1.,
            fill: [255; 4],
            border: None,
        }];
        assert!(scene_ink_bounds(&scene, 1., scene.offset).is_none());

        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(-f32::MAX * 0.75, 0., 0., 8.),
                radius: 1.,
                fill: [255; 4],
                border: None,
            },
            Primitive::Rounded {
                rect: Rect::new(f32::MAX * 0.75, 0., 0., 8.),
                radius: 1.,
                fill: [255; 4],
                border: None,
            },
        ];
        assert!(scene_ink_bounds(&scene, 1., scene.offset).is_none());
    }

    #[test]
    fn prepared_raster_origin_and_pixels_are_checked_outside_primitive_bounds() -> Result<()> {
        let mut scene = scene(false, false);
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 32.);
        scene.motion.as_mut().unwrap().edge = bead_core::Edge::Left;
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Text {
            rect: Rect::new(20., 4., 8., 12.),
            text: "raster-backed".into(),
            family: "Noto Sans".into(),
            size: 12.,
            color: [255, 255, 255, 255],
            weight: 400,
            align: None,
            shadow: None,
        }];

        let mut pixels = tiny_skia::Pixmap::new(12, 12).unwrap();
        pixels.fill(tiny_skia::Color::from_rgba8(240, 80, 40, 255));
        let charge = bead_core::cache::Budget::new("layer-motion-test", 4096)
            .reserve(pixels.data().len() + 64)?;
        let raster = std::sync::Arc::new(crate::tree::Raster {
            pixels: std::sync::Arc::new(pixels),
            origin: (8, 0),
            key: 1,
            base_key: None,
            changed: Damage::full(12, 12),
            _charge: charge,
        });
        let frame = crate::tree::Frame {
            items: vec![crate::tree::Item {
                id: "prepared/text/0".into(),
                primitive: scene.primitives[0].clone(),
                clip: None,
                raster: Some(raster),
                image: None,
            }],
            scale: 1.,
            key: 1,
            native_resource_budget: None,
            _assets: Vec::new(),
            layer_count: 1,
            layers: Vec::new(),
            _accounting: None,
        };
        scene
            .prepared
            .insert(1_f32.to_bits(), std::sync::Arc::new(frame));

        let ink = scene_ink_bounds(&scene, 1., scene.offset).unwrap();
        assert_eq!(ink.x + ink.w, 40.);
        assert!(LayerMotionPlan::for_slide(
            &scene,
            false,
            32,
            32,
            1.,
            PixelFormat::Rgba,
            &mut NoPaint,
        )?
        .is_none());
        Ok(())
    }

    #[test]
    fn overflow_moving_further_offscreen_remains_supported() -> Result<()> {
        struct CountPaint {
            renderer: TreeCpuRenderer,
            paints: usize,
        }
        impl Renderer for CountPaint {
            fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
                self.renderer.prepare(scene, scale)
            }
            fn render(
                &mut self,
                scene: &Scene,
                target: ShmTarget<'_>,
                damage: Damage,
            ) -> Result<()> {
                self.paints += 1;
                self.renderer.render(scene, target, damage)
            }
        }

        let mut scene = scene(false, false);
        scene.width = 32.;
        scene.bounds = Rect::new(0., 0., 32., 32.);
        scene.motion.as_mut().unwrap().edge = bead_core::Edge::Left;
        scene.motion.as_mut().unwrap().slide_distance = Some(32.);
        scene.primitives = vec![Primitive::Rounded {
            rect: Rect::new(-8., 4., 16., 12.),
            radius: 1.,
            fill: [220, 70, 30, 255],
            border: None,
        }];
        let mut renderer = CountPaint {
            renderer: TreeCpuRenderer::default(),
            paints: 0,
        };
        assert!(
            LayerMotionPlan::for_slide(
                &scene,
                false,
                32,
                32,
                1.,
                PixelFormat::Rgba,
                &mut renderer,
            )?
            .is_some()
        );
        assert_eq!(renderer.paints, 1);
        Ok(())
    }
}

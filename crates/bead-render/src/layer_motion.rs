// SPDX-License-Identifier: MIT
//! Immutable pixels and presentation planes for optional compositor animation.
//!
//! A plan describes presentation, independent of a particular display protocol.
//! Preparing it performs the slide's sole client paint. A presenter keeps those
//! pixels attached until the compositor reports completion or cancellation.

use std::time::Duration;

use crate::{Damage, PixelFormat, Rect, Renderer, Result, Scene, ShmTarget};

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
        let closed_offset = motion.edge.closed_offset(
            motion.slide_distance.unwrap_or(scene.bounds.w),
            motion.slide_distance.unwrap_or(scene.bounds.h),
            0.,
        );
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
        struct NoPaint;
        impl Renderer for NoPaint {
            fn prepare(&mut self, _: &Scene, _: f32) -> Result<()> {
                panic!("unsupported animation must not prepare")
            }
            fn render(&mut self, _: &Scene, _: ShmTarget<'_>, _: Damage) -> Result<()> {
                panic!("unsupported animation must not paint")
            }
        }
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
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Small borrowed-target paint helpers for compositor-positioned slider visuals.

use super::{PixelFormat, Rect, ShmTarget};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PixmapMut, Point, SpreadMode, Stroke,
    Transform,
};

/// Paint a rounded rectangle with a left-to-right gradient directly over borrowed SHM pixels.
///
/// Colors are straight RGBA inputs. The destination remains in its declared pixel format;
/// BGRA targets use swapped source channels so tiny-skia's RGBA pixmap view blends correctly.
pub fn rounded_gradient(
    target: &mut ShmTarget<'_>,
    rect: Rect,
    radius: f32,
    from: [u8; 4],
    to: [u8; 4],
    border: Option<([u8; 4], f32)>,
) -> super::Result<()> {
    let byte_len = (target.width as usize)
        .checked_mul(target.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("rounded target dimensions overflow")?;
    if target.width == 0
        || target.height == 0
        || target.bytes.len() < byte_len
        || !target.scale.is_finite()
        || target.scale <= 0.
        || ![rect.x, rect.y, rect.w, rect.h, radius]
            .iter()
            .all(|n| n.is_finite())
        || rect.w <= 0.
        || rect.h <= 0.
        || radius < 0.
    {
        return Err("invalid rounded target geometry".into());
    }
    if let Some((_, width)) = border
        && (!width.is_finite() || width < 0.)
    {
        return Err("invalid rounded border width".into());
    }
    if border.is_some_and(|(_, width)| !(width * target.scale).is_finite()) {
        return Err("rounded border exceeds target scale".into());
    }
    let scaled = [
        rect.x * target.scale,
        rect.y * target.scale,
        rect.w * target.scale,
        rect.h * target.scale,
        radius * target.scale,
        (rect.x + rect.w) * target.scale,
        (rect.y + rect.h) * target.scale,
    ];
    if !scaled.iter().all(|n| n.is_finite()) {
        return Err("rounded geometry exceeds target scale".into());
    }

    let path = super::rounded(rect, radius).ok_or("rounded path construction failed")?;
    let scale = target.scale;
    let format = target.format;
    let transform = Transform::from_scale(scale, scale);
    let rgba = |color: [u8; 4]| match format {
        PixelFormat::Rgba => color,
        PixelFormat::Bgra => [color[2], color[1], color[0], color[3]],
    };
    let from = rgba(from);
    let to = rgba(to);
    let mut pixmap =
        PixmapMut::from_bytes(&mut target.bytes[..byte_len], target.width, target.height)
            .ok_or("invalid rounded SHM target")?;

    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    if from == to {
        paint.set_color_rgba8(from[0], from[1], from[2], from[3]);
    } else {
        // fill_path applies the geometry transform to its shader too, so these
        // endpoints stay in logical coordinates while the path is scaled.
        let y = rect.y + rect.h * 0.5;
        let x0 = rect.x;
        let x1 = rect.x + rect.w;
        paint.shader = LinearGradient::new(
            Point::from_xy(x0, y),
            Point::from_xy(x1, y),
            vec![
                GradientStop::new(0., Color::from_rgba8(from[0], from[1], from[2], from[3])),
                GradientStop::new(1., Color::from_rgba8(to[0], to[1], to[2], to[3])),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .ok_or("rounded gradient construction failed")?;
    }
    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

    if let Some((color, width)) = border
        && width > 0.
        && color[3] > 0
    {
        let color = rgba(color);
        paint.shader = tiny_skia::Shader::SolidColor(Color::from_rgba8(
            color[0], color[1], color[2], color[3],
        ));
        pixmap.stroke_path(
            &path,
            &paint,
            &Stroke {
                width,
                ..Stroke::default()
            },
            transform,
            None,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(format: PixelFormat) -> Vec<u8> {
        let width = 30u32;
        let height = 18u32;
        let byte_len = width as usize * height as usize * 4;
        let mut bytes = vec![0; byte_len + 8];
        bytes[byte_len..].fill(0xa5);
        let mut target = ShmTarget {
            bytes: &mut bytes,
            width,
            height,
            scale: 1.5,
            format,
        };
        rounded_gradient(
            &mut target,
            Rect::new(2., 2., 12., 8.),
            3.,
            [240, 20, 10, 255],
            [10, 30, 230, 255],
            None,
        )
        .unwrap();
        assert!(bytes[byte_len..].iter().all(|byte| *byte == 0xa5));
        bytes
    }

    #[test]
    fn gradient_runs_left_to_right_and_keeps_rounded_corners_clear_at_fractional_scale() {
        let bytes = render(PixelFormat::Rgba);
        let pixel = |x: usize, y: usize| &bytes[(y * 30 + x) * 4..(y * 30 + x) * 4 + 4];
        let left = pixel(5, 9);
        let center = pixel(12, 9);
        let right = pixel(19, 9);
        assert!(
            left[0] > left[2],
            "left endpoint should favor `from`: {left:?}"
        );
        assert!(
            (i16::from(center[0]) - i16::from(center[2])).abs() <= 16,
            "logical gradient midpoint should remain centered at 1.5x: {center:?}"
        );
        assert!(
            right[2] > right[0],
            "right endpoint should favor `to`: {right:?}"
        );
        assert_eq!(
            pixel(3, 3)[3],
            0,
            "outside rounded corner stays transparent"
        );
    }

    #[test]
    fn bgra_and_rgba_targets_have_the_same_visible_premultiplied_pixels() {
        let rgba = render(PixelFormat::Rgba);
        let bgra = render(PixelFormat::Bgra);
        for (rgba, bgra) in rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(bgra.as_chunks::<4>().0.iter())
        {
            assert_eq!(
                [rgba[0], rgba[1], rgba[2], rgba[3]],
                [bgra[2], bgra[1], bgra[0], bgra[3]]
            );
        }
    }
}

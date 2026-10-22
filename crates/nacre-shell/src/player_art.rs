// SPDX-License-Identifier: GPL-3.0-only
//! B0031: album art for the dashboard player card. The daemon re-encodes art to a
//! private PNG; the UI decodes it once to a small square thumbnail, keeps only the
//! thumbnail bytes while the card is visible and drops them when it is hidden.
use image::{ImageEncoder, ImageFormat, ImageReader, Limits, imageops::FilterType};
use std::{io::Cursor, path::Path, sync::Arc};

/// Longest side the UI will decode in-process (the daemon allows more; larger art
/// simply shows the placeholder instead of a transient multi-megabyte buffer).
const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 4_000_000;
const MAX_FILE: u64 = 4 * 1024 * 1024;
/// Twice the 220 px card, so scale 2 stays sharp.
pub(crate) const THUMB: u32 = 440;
const CACHE_DIR: &str = "nacre/shell/player-art";

/// A square PNG thumbnail of the daemon's cached art, or `None` for anything that
/// is not a plain regular PNG file in the daemon's art cache within the limits.
pub(crate) fn thumbnail(path: &str) -> Option<Arc<[u8]>> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.parent()?.ends_with(CACHE_DIR) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let (w, h) = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png)
        .into_dimensions()
        .ok()?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return None;
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let thumb = reader
        .decode()
        .ok()?
        .resize_to_fill(THUMB, THUMB, FilterType::Triangle)
        .to_rgba8();
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(&thumb, THUMB, THUMB, image::ExtendedColorType::Rgba8)
    .ok()?;
    Some(Arc::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, w: u32, h: u32) -> String {
        let dir = dir.join(CACHE_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
            .save_with_format(&path, ImageFormat::Png)
            .unwrap();
        path.to_str().unwrap().into()
    }

    #[test]
    fn art_becomes_a_small_square_png_and_anything_else_is_refused() {
        let root = std::env::temp_dir().join(format!("bead-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let wide = write(&root, "wide.png", 800, 400);
        let thumb = thumbnail(&wide).expect("thumbnail");
        let image = image::load_from_memory(&thumb).unwrap();
        assert_eq!((image.width(), image.height()), (THUMB, THUMB));
        assert!(thumb.len() < 64 * 1024, "{}", thumb.len());
        // Too many pixels, a relative path, a path outside the cache, a symlink
        // and a missing file never reach the decoder.
        assert!(thumbnail(&write(&root, "huge.png", 2100, 2100)).is_none());
        assert!(thumbnail("player-art/wide.png").is_none());
        let outside = root.join("outside.png");
        std::fs::copy(&wide, &outside).unwrap();
        assert!(thumbnail(outside.to_str().unwrap()).is_none());
        let link = root.join(CACHE_DIR).join("link.png");
        std::os::unix::fs::symlink(&wide, &link).unwrap();
        assert!(thumbnail(link.to_str().unwrap()).is_none());
        assert!(thumbnail(root.join(CACHE_DIR).join("none.png").to_str().unwrap()).is_none());
        std::fs::write(root.join(CACHE_DIR).join("junk.png"), b"not a png").unwrap();
        assert!(thumbnail(root.join(CACHE_DIR).join("junk.png").to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}

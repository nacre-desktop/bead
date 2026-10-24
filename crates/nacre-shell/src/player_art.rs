// SPDX-License-Identifier: GPL-3.0-only
//! B0031/B0034: daemon-cached PNGs (player art, screenshot thumbnails) for the dashboard.
//! The UI accepts only a regular, non-symlink PNG inside the daemon's cache directory,
//! decodes it once to display size, keeps only the thumbnail bytes while the card is
//! visible and drops them when it is hidden.
use image::{ImageEncoder, ImageFormat, ImageReader, Limits, imageops::FilterType};
use std::{io::Cursor, path::Path, sync::Arc};

/// Longest side the UI will decode in-process (the daemon allows more; larger art
/// simply shows the placeholder instead of a transient multi-megabyte buffer).
const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 4_000_000;
/// Where a cache lives and how large its PNGs may be; output is `w`x`h` (fill).
pub(crate) struct Cache {
    pub(crate) dir: &'static str,
    pub(crate) max_file: u64,
    pub(crate) w: u32,
    pub(crate) h: u32,
}
/// Twice the 220 px card, so scale 2 stays sharp.
pub(crate) const ART: Cache = Cache {
    dir: "nacre/shell/player-art",
    max_file: 4 * 1024 * 1024,
    w: 440,
    h: 440,
};
/// Twice the 72x48 row thumbnail; the daemon already caps these at 256 px.
pub(crate) const SHOT: Cache = Cache {
    dir: "nacre/shell/screenshot-thumbs",
    max_file: 1024 * 1024,
    w: 144,
    h: 96,
};

/// The bytes of a plain regular file directly inside the cache directory within its size cap.
fn read(cache: &Cache, path: &str) -> Option<Vec<u8>> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.parent()?.ends_with(cache.dir) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > cache.max_file {
        return None;
    }
    std::fs::read(path).ok()
}
/// N0056: the daemon's weather-photo cache. The daemon already decoded and bounded the image.
const PHOTO: Cache = Cache {
    dir: "nacre/daemon-weather-photo",
    max_file: 2 * 1024 * 1024,
    w: 0,
    h: 0,
};
/// The published city photo as bytes plus its size: a JPEG or PNG of at most 1400 per side
/// from the daemon's cache only. The renderer decodes it once to display size.
pub(crate) fn photo(path: &str) -> Option<(Arc<[u8]>, u32, u32)> {
    let bytes = read(&PHOTO, path)?;
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(reader.format(), Some(ImageFormat::Jpeg | ImageFormat::Png)) {
        return None;
    }
    let (w, h) = reader.into_dimensions().ok()?;
    // 1400 per side already bounds the pixels at the daemon's 1.96 million.
    if !(1..=1400).contains(&w) || !(1..=1400).contains(&h) {
        return None;
    }
    Some((Arc::from(bytes), w, h))
}

/// The decoded image of a plain regular PNG file directly inside the cache directory, or
/// `None` for anything else (relative or foreign path, symlink, oversized file or image).
pub(crate) fn decode(cache: &Cache, path: &str) -> Option<image::DynamicImage> {
    let bytes = read(cache, path)?;
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
    reader.decode().ok()
}
pub(crate) fn encode(image: &image::RgbaImage) -> Option<Arc<[u8]>> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        image,
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(Arc::from(out))
}
/// A display-size PNG thumbnail of the daemon's cached image, or `None` for anything
/// that is not a plain regular PNG file in that cache within the limits.
pub(crate) fn thumbnail(cache: &Cache, path: &str) -> Option<Arc<[u8]>> {
    let thumb = decode(cache, path)?
        .resize_to_fill(cache.w, cache.h, FilterType::Triangle)
        .to_rgba8();
    encode(&thumb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, w: u32, h: u32) -> String {
        let dir = dir.join(ART.dir);
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
        let thumb = thumbnail(&ART, &wide).expect("thumbnail");
        let image = image::load_from_memory(&thumb).unwrap();
        assert_eq!((image.width(), image.height()), (ART.w, ART.h));
        assert!(thumb.len() < 64 * 1024, "{}", thumb.len());
        // Too many pixels, a relative path, a path outside the cache, a symlink
        // and a missing file never reach the decoder.
        assert!(thumbnail(&ART, &write(&root, "huge.png", 2100, 2100)).is_none());
        assert!(thumbnail(&ART, "player-art/wide.png").is_none());
        let outside = root.join("outside.png");
        std::fs::copy(&wide, &outside).unwrap();
        assert!(thumbnail(&ART, outside.to_str().unwrap()).is_none());
        let link = root.join(ART.dir).join("link.png");
        std::os::unix::fs::symlink(&wide, &link).unwrap();
        assert!(thumbnail(&ART, link.to_str().unwrap()).is_none());
        assert!(thumbnail(&ART, root.join(ART.dir).join("none.png").to_str().unwrap()).is_none());
        std::fs::write(root.join(ART.dir).join("junk.png"), b"not a png").unwrap();
        assert!(thumbnail(&ART, root.join(ART.dir).join("junk.png").to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_weather_photo_is_raw_bytes_from_the_daemon_cache_within_its_limits() {
        let root = std::env::temp_dir().join(format!("bead-photo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join(PHOTO.dir);
        std::fs::create_dir_all(&dir).unwrap();
        let save = |name: &str, w: u32, h: u32, format: ImageFormat| {
            let path = dir.join(name);
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                w,
                h,
                image::Rgb([9, 90, 200]),
            ))
            .save_with_format(&path, format)
            .unwrap();
            path.to_str().unwrap().to_owned()
        };
        // The daemon's file name has no extension: the format comes from the bytes.
        let jpeg = save("city.image", 960, 600, ImageFormat::Jpeg);
        let (bytes, w, h) = photo(&jpeg).expect("jpeg");
        assert_eq!((w, h), (960, 600));
        assert_eq!(&bytes[..], &std::fs::read(&jpeg).unwrap()[..]);
        assert_eq!(
            photo(&save("a.png", 400, 300, ImageFormat::Png)).unwrap().1,
            400
        );
        assert!(photo(&save("wide.png", 1401, 10, ImageFormat::Png)).is_none());
        std::fs::write(dir.join("junk.image"), b"GIF89a not a photo").unwrap();
        assert!(photo(dir.join("junk.image").to_str().unwrap()).is_none());
        // Relative, foreign and symlinked paths never reach the decoder.
        assert!(photo("city.image").is_none());
        let outside = root.join("outside.png");
        std::fs::copy(&jpeg, &outside).unwrap();
        assert!(photo(outside.to_str().unwrap()).is_none());
        let link = dir.join("link.image");
        std::os::unix::fs::symlink(&jpeg, &link).unwrap();
        assert!(photo(link.to_str().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}

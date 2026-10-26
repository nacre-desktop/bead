// SPDX-License-Identifier: GPL-3.0-only
//! Decoder used only by the short-lived wallpaper worker and unit tests.
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::{FileExt, MetadataExt},
    },
    path::Path,
    sync::OnceLock,
};
const MAX_SOURCE_PIXELS: u64 = 100_000_000;
const MAX_OUTPUT_RGBA_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTPUT_DIMENSION: u32 = 16_384;
const MAX_NATIVE_ENCODED_BYTES: usize = 32 * 1024 * 1024;

struct DecoderSpan {
    stage: &'static str,
    start: Option<(u64, u64)>,
}

impl DecoderSpan {
    fn new(stage: &'static str) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| {
            std::env::var_os("BEAD_PROFILE_INPUT")
                .is_some_and(|value| value.as_os_str() != std::ffi::OsStr::new("clocks"))
        });
        let start = enabled.then(|| {
            (
                bead::core::profile::clock(libc::CLOCK_MONOTONIC),
                bead::core::profile::clock(libc::CLOCK_THREAD_CPUTIME_ID),
            )
        });
        Self { stage, start }
    }
}

impl Drop for DecoderSpan {
    fn drop(&mut self) {
        if let Some((wall, cpu)) = self.start {
            eprintln!(
                "INPUT_PROFILE stage={} wall_ms={:.4} cpu_ms={:.4}",
                self.stage,
                bead::core::profile::clock(libc::CLOCK_MONOTONIC).saturating_sub(wall) as f64 / 1e6,
                bead::core::profile::clock(libc::CLOCK_THREAD_CPUTIME_ID).saturating_sub(cpu)
                    as f64
                    / 1e6
            );
        }
    }
}

pub(crate) fn decode_pair(
    pair: ActiveWallpaper,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    pair.validate().map_err(|_| "invalid wallpaper pair")?;
    if width == 0 || height == 0 {
        return Err("wallpaper unavailable".into());
    }
    if pair.presentation == nacre_protocol::wallpaper::Presentation::Centre {
        return Err("centre presentation cannot be bounded by pixbuf loaders".into());
    }
    let mut file = open_validated(&pair)?;
    let before = file
        .metadata()
        .map_err(|_| "wallpaper metadata unavailable")?;
    if !pair.file_identity.matches(&before) || !before.is_file() {
        return Err("wallpaper identity changed".into());
    }
    let identity_fd = file
        .try_clone()
        .map_err(|_| "wallpaper descriptor unavailable")?;
    let output_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if width > MAX_OUTPUT_DIMENSION
        || height > MAX_OUTPUT_DIMENSION
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || output_bytes.is_none_or(|bytes| bytes > MAX_OUTPUT_RGBA_BYTES)
    {
        return Err("output dimensions exceed wallpaper bound".into());
    }
    let mut signature = [0; 8];
    let native = if before.len() <= MAX_NATIVE_ENCODED_BYTES as u64
        && file.read_at(&mut signature, 0).ok() == Some(signature.len())
    {
        if signature[..2] == [0xff, 0xd8] {
            decode_jpeg_fast(&mut file, width, height, pair.presentation)
        } else if signature == *b"\x89PNG\r\n\x1a\n" {
            decode_png_fast(&mut file, width, height, pair.presentation)
        } else {
            None
        }
    } else {
        None
    };
    let (pixbuf, maximum) = if let Some(decoded) = native {
        decoded
    } else {
        // Native and streamed decoders use the same pinned inode sequentially.
        file.seek(SeekFrom::Start(0))
            .map_err(|_| "wallpaper seek failed")?;
        load_streamed(&file, width, height, pair.presentation)?
    };
    let rgba = {
        let _span = DecoderSpan::new("wallpaper_rgba_output");
        finish_pixbuf(pixbuf, maximum, (width, height), pair.presentation)?
    };
    // Check the same still-open inode after decoding, before publishing bytes.
    let after = identity_fd
        .metadata()
        .map_err(|_| "wallpaper metadata unavailable")?;
    if !pair.file_identity.matches(&after) {
        return Err("wallpaper changed during decode".into());
    }
    Ok(rgba)
}

fn load_streamed(
    file: &File,
    width: u32,
    height: u32,
    presentation: nacre_protocol::wallpaper::Presentation,
) -> Result<(gdk_pixbuf::Pixbuf, (i32, i32)), String> {
    let max_dim = MAX_SOURCE_PIXELS;
    let loader = gdk_pixbuf::PixbufLoader::new();
    let source_size = std::rc::Rc::new(std::cell::Cell::new(None));
    let source_size_cb = source_size.clone();
    let requested_size = std::rc::Rc::new(std::cell::Cell::new((1, 1)));
    let requested_size_cb = requested_size.clone();
    use gdk_pixbuf::prelude::PixbufLoaderExt;
    loader.connect_size_prepared(move |loader, source_width, source_height| {
        source_size_cb.set(Some((source_width, source_height)));
        if source_width <= 0
            || source_height <= 0
            || u64::try_from(source_width)
                .unwrap_or(u64::MAX)
                .saturating_mul(u64::try_from(source_height).unwrap_or(u64::MAX))
                > max_dim
        {
            loader.set_size(1, 1);
        } else {
            // GdkPixbuf scales as the stream is decoded. For crop, ask for the
            // smallest aspect-preserving raster covering the output, then crop.
            if let Some(size) = scaled_dimensions(
                source_width as u32,
                source_height as u32,
                width,
                height,
                presentation,
                max_dim,
            ) {
                requested_size_cb.set(size);
                loader.set_size(size.0, size.1);
            } else {
                loader.set_size(1, 1);
            }
        }
    });
    let mut input = BufReader::new(file);
    let mut chunk = [0u8; 16 * 1024];
    let mut total = 0usize;
    loop {
        let count = input
            .read(&mut chunk)
            .map_err(|_| "wallpaper read failed")?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count).ok_or("wallpaper size overflow")?;
        if total > 134_217_728 {
            return Err("wallpaper exceeds encoded size bound".into());
        }
        loader
            .write(&chunk[..count])
            .map_err(|_| "wallpaper decode failed")?;
        if let Some(format) = loader.format()
            && format
                .name()
                .is_some_and(|name| !allowed_format(name.as_str()))
        {
            return Err("wallpaper format is not supported by the contract".into());
        }
    }
    loader.close().map_err(|_| "wallpaper decode failed")?;
    let (source_width, source_height) = source_size
        .get()
        .ok_or("wallpaper dimensions unavailable")?;
    if source_width <= 0
        || source_height <= 0
        || u64::try_from(source_width)
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(source_height).unwrap_or(u64::MAX))
            > max_dim
    {
        return Err("wallpaper source dimensions exceed contract bound".into());
    }
    let format = loader.format().ok_or("wallpaper format unavailable")?;
    let format_name = format.name().ok_or("wallpaper format unavailable")?;
    if !allowed_format(format_name.as_str()) {
        return Err("wallpaper format is not supported by the contract".into());
    }
    let pixbuf = loader
        .pixbuf()
        .ok_or("wallpaper decode produced no pixels")?;
    Ok((pixbuf, requested_size.get()))
}

fn finish_pixbuf(
    pixbuf: gdk_pixbuf::Pixbuf,
    (max_width, max_height): (i32, i32),
    (width, height): (u32, u32),
    presentation: nacre_protocol::wallpaper::Presentation,
) -> Result<Vec<u8>, String> {
    if pixbuf.width() <= 0
        || pixbuf.height() <= 0
        || pixbuf.width() > max_width
        || pixbuf.height() > max_height
        || pixbuf.width() as u32 > MAX_OUTPUT_DIMENSION
        || pixbuf.height() as u32 > MAX_OUTPUT_DIMENSION
        || (pixbuf.width() as usize)
            .checked_mul(pixbuf.height() as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_none_or(|bytes| bytes > MAX_OUTPUT_RGBA_BYTES)
        || u64::try_from(pixbuf.width())
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(pixbuf.height()).unwrap_or(u64::MAX))
            > MAX_SOURCE_PIXELS
        || (matches!(presentation, nacre_protocol::wallpaper::Presentation::Crop)
            && (pixbuf.width() < width as i32 || pixbuf.height() < height as i32))
    {
        return Err("wallpaper decoder exceeded its requested output bound".into());
    }
    let decoded = match presentation {
        nacre_protocol::wallpaper::Presentation::Crop => {
            let x = (pixbuf.width() - width as i32).max(0) / 2;
            let y = (pixbuf.height() - height as i32).max(0) / 2;
            pixbuf.new_subpixbuf(
                x,
                y,
                (width as i32).min(pixbuf.width()),
                (height as i32).min(pixbuf.height()),
            )
        }
        nacre_protocol::wallpaper::Presentation::Fill => pixbuf
            .scale_simple(
                width as i32,
                height as i32,
                gdk_pixbuf::InterpType::Bilinear,
            )
            .ok_or("wallpaper resize failed")?,
        nacre_protocol::wallpaper::Presentation::Fit => pixbuf,
        nacre_protocol::wallpaper::Presentation::Centre => unreachable!(),
    };
    let dx = if matches!(presentation, nacre_protocol::wallpaper::Presentation::Fit) {
        (width as i32 - decoded.width()).max(0) / 2
    } else {
        0
    };
    let dy = if matches!(presentation, nacre_protocol::wallpaper::Presentation::Fit) {
        (height as i32 - decoded.height()).max(0) / 2
    } else {
        0
    };
    let copy_width = (decoded.width() as u32).min(width.saturating_sub(dx as u32));
    let copy_height = (decoded.height() as u32).min(height.saturating_sub(dy as u32));
    let data = decoded.read_pixel_bytes();
    let channels = decoded.n_channels() as usize;
    let stride = decoded.rowstride() as usize;
    let mut rgba = vec![0; width as usize * height as usize * 4];
    for y in 0..copy_height as usize {
        let source_start = y * stride;
        let source = data
            .get(source_start..source_start + copy_width as usize * channels)
            .ok_or("wallpaper row unavailable")?;
        let target_start = ((y + dy as usize) * width as usize + dx as usize) * 4;
        let target = &mut rgba[target_start..target_start + copy_width as usize * 4];
        if channels == 4 {
            target.copy_from_slice(source);
        } else {
            for (source, target) in source
                .as_chunks::<3>()
                .0
                .iter()
                .zip(target.as_chunks_mut::<4>().0.iter_mut())
            {
                *target = [source[0], source[1], source[2], 255];
            }
        }
    }
    Ok(rgba)
}

fn scaled_dimensions(
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
    presentation: nacre_protocol::wallpaper::Presentation,
    max_dim: u64,
) -> Option<(i32, i32)> {
    if source_width == 0
        || source_height == 0
        || u64::from(source_width).saturating_mul(u64::from(source_height)) > max_dim
    {
        return None;
    }
    let scale = if presentation == nacre_protocol::wallpaper::Presentation::Crop {
        (width as f64 / source_width as f64).max(height as f64 / source_height as f64)
    } else {
        (width as f64 / source_width as f64)
            .min(height as f64 / source_height as f64)
            .min(1.0)
    };
    let w = ((source_width as f64 * scale).ceil() as i64).clamp(1, 100_000) as u32;
    let h = ((source_height as f64 * scale).ceil() as i64).clamp(1, 100_000) as u32;
    let bytes = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    (w <= MAX_OUTPUT_DIMENSION
        && h <= MAX_OUTPUT_DIMENSION
        && bytes <= MAX_OUTPUT_RGBA_BYTES
        && u64::from(w).saturating_mul(u64::from(h)) <= max_dim)
        .then_some((w as i32, h as i32))
}

fn decode_jpeg_fast(
    file: &mut File,
    width: u32,
    height: u32,
    presentation: nacre_protocol::wallpaper::Presentation,
) -> Option<(gdk_pixbuf::Pixbuf, (i32, i32))> {
    use image::ImageDecoder;
    let _span = DecoderSpan::new("wallpaper_jpeg");
    let encoded = read_native_encoded(file)?;
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(encoded)).ok()?;
    if decoder.icc_profile().ok()?.is_some() || decoder.color_type() != image::ColorType::Rgb8 {
        return None;
    }
    decode_native_pixels(decoder, width, height, presentation)
}

fn decode_png_fast(
    file: &mut File,
    width: u32,
    height: u32,
    presentation: nacre_protocol::wallpaper::Presentation,
) -> Option<(gdk_pixbuf::Pixbuf, (i32, i32))> {
    use image::ImageDecoder;
    let _span = DecoderSpan::new("wallpaper_png");
    let encoded = read_native_encoded(file)?;
    if !simple_png_chunks(&encoded)? {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_OUTPUT_RGBA_BYTES as u64);
    let mut decoder =
        image::codecs::png::PngDecoder::with_limits(std::io::Cursor::new(encoded), limits).ok()?;
    if decoder.is_apng().ok()?
        || decoder.icc_profile().ok()?.is_some()
        || !matches!(
            decoder.color_type(),
            image::ColorType::Rgb8 | image::ColorType::Rgba8
        )
    {
        return None;
    }
    decode_native_pixels(decoder, width, height, presentation)
}

fn read_native_encoded(file: &mut File) -> Option<Vec<u8>> {
    let mut encoded = Vec::new();
    Read::take(&mut *file, (MAX_NATIVE_ENCODED_BYTES + 1) as u64)
        .read_to_end(&mut encoded)
        .ok()?;
    (encoded.len() <= MAX_NATIVE_ENCODED_BYTES).then_some(encoded)
}

fn simple_png_chunks(encoded: &[u8]) -> Option<bool> {
    if encoded.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut offset = 8usize;
    let mut header = None;
    loop {
        let length_end = offset.checked_add(4)?;
        let type_end = length_end.checked_add(4)?;
        if type_end > encoded.len() {
            return None;
        }
        let chunk_length = usize::try_from(u32::from_be_bytes(
            encoded[offset..length_end].try_into().ok()?,
        ))
        .ok()?;
        let data_end = type_end.checked_add(chunk_length)?;
        let chunk_end = data_end.checked_add(4)?;
        if chunk_end > encoded.len() {
            return None;
        }
        let kind = &encoded[length_end..type_end];
        if header.is_none() {
            if kind != b"IHDR" || chunk_length != 13 {
                return None;
            }
            let data = &encoded[type_end..data_end];
            if data[8] != 8 || !matches!(data[9], 2 | 6) {
                return Some(false);
            }
            header = Some(());
        } else if kind == b"iCCP"
            || kind == b"gAMA"
            || kind == b"cHRM"
            || kind == b"sRGB"
            || kind == b"cICP"
            || kind == b"mDCV"
            || kind == b"cLLI"
            || kind == b"acTL"
            || kind == b"fcTL"
            || kind == b"fdAT"
        {
            return Some(false);
        }
        if kind == b"IEND" {
            return (chunk_length == 0 && chunk_end == encoded.len()).then_some(true);
        }
        offset = chunk_end;
    }
}

fn decode_native_pixels<D: image::ImageDecoder>(
    decoder: D,
    width: u32,
    height: u32,
    presentation: nacre_protocol::wallpaper::Presentation,
) -> Option<(gdk_pixbuf::Pixbuf, (i32, i32))> {
    let (source_width, source_height) = decoder.dimensions();
    let (scaled_width, scaled_height) = scaled_dimensions(
        source_width,
        source_height,
        width,
        height,
        presentation,
        MAX_SOURCE_PIXELS,
    )?;
    let channels = match decoder.color_type() {
        image::ColorType::Rgb8 => 3usize,
        image::ColorType::Rgba8 => 4,
        _ => return None,
    };
    let bytes = usize::try_from(decoder.total_bytes()).ok()?;
    if bytes == 0
        || bytes > MAX_OUTPUT_RGBA_BYTES
        || bytes
            != (source_width as usize)
                .checked_mul(source_height as usize)?
                .checked_mul(channels)?
    {
        return None;
    }
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(bytes).ok()?;
    pixels.resize(bytes, 0);
    {
        let _span = DecoderSpan::new("wallpaper_native_decode");
        decoder.read_image(&mut pixels).ok()?;
    }
    let pixels = if source_width == scaled_width as u32 && source_height == scaled_height as u32 {
        pixels
    } else {
        let _span = DecoderSpan::new("wallpaper_native_resize");
        use fast_image_resize::{
            FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image,
        };
        let pixel_type = if channels == 4 {
            PixelType::U8x4
        } else {
            PixelType::U8x3
        };
        let source = Image::from_vec_u8(source_width, source_height, pixels, pixel_type).ok()?;
        let mut target = Image::new(scaled_width as u32, scaled_height as u32, pixel_type);
        Resizer::new()
            .resize(
                &source,
                &mut target,
                &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
            )
            .ok()?;
        target.into_vec()
    };
    let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
        &gdk_pixbuf::glib::Bytes::from_owned(pixels),
        gdk_pixbuf::Colorspace::Rgb,
        channels == 4,
        8,
        scaled_width,
        scaled_height,
        scaled_width.checked_mul(channels as i32)?,
    );
    Some((pixbuf, (scaled_width, scaled_height)))
}

fn open_validated(pair: &ActiveWallpaper) -> Result<File, String> {
    let path = Path::new(&pair.path);
    if !path.is_absolute() || path.as_os_str().len() > 4096 {
        return Err("invalid wallpaper path".into());
    }
    // Walk from `/` one component at a time. O_NOFOLLOW prevents a rename or
    // symlink race at each step; the final descriptor is kept through decode.
    let root_fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err("wallpaper root unavailable".into());
    }
    let root = unsafe { OwnedFd::from_raw_fd(root_fd) };
    let mut dir = root;
    let components: Vec<_> = path.components().skip(1).collect();
    if components.is_empty() {
        return Err("invalid wallpaper path".into());
    }
    for component in &components[..components.len() - 1] {
        let std::path::Component::Normal(name) = component else {
            return Err("invalid wallpaper path".into());
        };
        let name = std::ffi::CString::new(name.as_encoded_bytes())
            .map_err(|_| "invalid wallpaper path")?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err("wallpaper directory unavailable".into());
        }
        let next = unsafe { OwnedFd::from_raw_fd(fd) };
        let meta = File::from(
            next.try_clone()
                .map_err(|_| "wallpaper directory unavailable")?,
        )
        .metadata()
        .map_err(|_| "wallpaper directory unavailable")?;
        let uid = unsafe { libc::geteuid() };
        let sticky_tmp = meta.uid() == 0 && meta.mode() & libc::S_ISVTX != 0;
        if !meta.is_dir()
            || (meta.uid() != 0 && meta.uid() != uid)
            || (meta.mode() & 0o022 != 0 && !sticky_tmp)
        {
            return Err("wallpaper directory permissions rejected".into());
        }
        dir = next;
    }
    let std::path::Component::Normal(name) = components.last().unwrap() else {
        return Err("invalid wallpaper path".into());
    };
    let name =
        std::ffi::CString::new(name.as_encoded_bytes()).map_err(|_| "invalid wallpaper path")?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err("wallpaper file unavailable".into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let meta = file
        .metadata()
        .map_err(|_| "wallpaper metadata unavailable")?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o022 != 0
        || meta.len() > 134_217_728
    {
        return Err("wallpaper file permissions rejected".into());
    }
    if !pair.file_identity.matches(&meta) {
        return Err("wallpaper identity changed".into());
    }
    Ok(file)
}

fn allowed_format(name: &str) -> bool {
    matches!(name, "png" | "jpeg" | "webp" | "gif" | "tiff")
}

#[cfg(test)]
mod fast_jpeg_tests {
    use super::*;

    #[test]
    fn fast_jpeg_preserves_crop_fit_fill_dimensions_and_file_identity() {
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-fast-jpeg-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.jpg");
        image::RgbImage::from_fn(4, 2, |x, y| image::Rgb([x as u8 * 60, y as u8 * 100, 30]))
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(&metadata);
        let path = path.to_string_lossy().into_owned();
        let make_pair = |presentation: &str| {
            serde_json::from_value::<ActiveWallpaper>(serde_json::json!({
                "revision":1,"palette_revision":1,"path":path.clone(),"file_identity":identity.clone(),
                "presentation":presentation,
                "palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}
            }))
            .unwrap()
        };
        let pair = make_pair("crop");
        let mut pinned = open_validated(&pair).unwrap();
        assert!(decode_jpeg_fast(&mut pinned, 2, 2, pair.presentation).is_some());
        let crop = decode_pair(make_pair("crop"), 2, 2).unwrap();
        let fit = decode_pair(make_pair("fit"), 2, 2).unwrap();
        let fill = decode_pair(make_pair("fill"), 2, 2).unwrap();
        assert_eq!((crop.len(), fit.len(), fill.len()), (16, 16, 16));
        assert!(fit.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(crop.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
        assert!(fill.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
        assert_ne!(crop, fill);
        std::fs::remove_file(&path).unwrap();
        image::RgbImage::from_pixel(4, 2, image::Rgb([10, 20, 30]))
            .save_with_format(&path, image::ImageFormat::Jpeg)
            .unwrap();
        assert!(decode_pair(make_pair("crop"), 2, 2).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod fast_png_tests {
    use super::*;

    fn pair_for(path: &Path) -> ActiveWallpaper {
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(path).unwrap(),
        );
        serde_json::from_value(serde_json::json!({
            "revision":1,
            "palette_revision":1,
            "path":path.to_string_lossy(),
            "file_identity":identity,
            "presentation":"fit",
            "palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}
        }))
        .unwrap()
    }

    #[test]
    fn fast_png_preserves_rgb_and_rgba_pixels() {
        let root =
            std::env::temp_dir().join(format!("bead-wallpaper-fast-png-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let rgb_path = root.join("rgb.png");
        let rgb = image::RgbImage::from_fn(3, 2, |x, y| {
            image::Rgb([x as u8 * 31, y as u8 * 47, (x + y) as u8 * 13])
        });
        rgb.save_with_format(&rgb_path, image::ImageFormat::Png)
            .unwrap();
        let rgb_pair = pair_for(&rgb_path);
        assert!(
            decode_png_fast(
                &mut open_validated(&rgb_pair).unwrap(),
                3,
                2,
                rgb_pair.presentation
            )
            .is_some()
        );
        let rgb_bytes = decode_pair(rgb_pair, 3, 2).unwrap();
        for (actual, expected) in rgb_bytes.as_chunks::<4>().0.iter().zip(rgb.pixels()) {
            assert_eq!(&actual[..3], &expected.0);
            assert_eq!(actual[3], 255);
        }

        let rgba_path = root.join("rgba.png");
        let rgba = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([x as u8 * 31, y as u8 * 47, 90, (x + y) as u8 * 63])
        });
        rgba.save_with_format(&rgba_path, image::ImageFormat::Png)
            .unwrap();
        let rgba_pair = pair_for(&rgba_path);
        assert!(
            decode_png_fast(
                &mut open_validated(&rgba_pair).unwrap(),
                3,
                2,
                rgba_pair.presentation
            )
            .is_some()
        );
        let rgba_bytes = decode_pair(rgba_pair, 3, 2).unwrap();
        for (actual, expected) in rgba_bytes.as_chunks::<4>().0.iter().zip(rgba.pixels()) {
            assert_eq!(actual, &expected.0);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unsupported_png_uses_streamed_fallback() {
        let root = std::env::temp_dir().join(format!(
            "bead-wallpaper-fast-png-fallback-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sixteen-bit.png");
        let image = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::from_fn(3, 2, |x, y| {
            image::Luma([(x * 4096 + y * 1024) as u16])
        });
        image
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        let pair = pair_for(&path);
        let mut pinned = open_validated(&pair).unwrap();
        assert!(decode_png_fast(&mut pinned, 3, 2, pair.presentation).is_none());
        let pixels = decode_pair(pair, 3, 2).unwrap();
        assert_eq!(pixels.len(), 3 * 2 * 4);
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255)
        );
        let _ = std::fs::remove_dir_all(root);
    }
}

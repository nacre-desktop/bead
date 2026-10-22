// SPDX-License-Identifier: GPL-3.0-only
//! Decoder used only by the short-lived wallpaper worker and unit tests.
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::{
    fs::File,
    io::{BufReader, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::MetadataExt,
    },
    path::Path,
};
const MAX_SOURCE_PIXELS: u64 = 100_000_000;
const MAX_OUTPUT_RGBA_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTPUT_DIMENSION: u32 = 16_384;

pub(crate) fn decode_pair(
    pair: ActiveWallpaper,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    pair.validate().map_err(|_| "invalid wallpaper pair")?;
    if width == 0 || height == 0 {
        return Err("wallpaper unavailable".into());
    }
    let file = open_validated(&pair)?;
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
    let max_dim = MAX_SOURCE_PIXELS;
    let loader = gdk_pixbuf::PixbufLoader::new();
    let source_size = std::rc::Rc::new(std::cell::Cell::new(None));
    let source_size_cb = source_size.clone();
    let requested_size = std::rc::Rc::new(std::cell::Cell::new((1, 1)));
    let requested_size_cb = requested_size.clone();
    let presentation = pair.presentation;
    let target = match pair.presentation {
        nacre_protocol::wallpaper::Presentation::Crop => (width, height),
        nacre_protocol::wallpaper::Presentation::Fit => (width, height),
        nacre_protocol::wallpaper::Presentation::Fill => (width, height),
        nacre_protocol::wallpaper::Presentation::Centre => {
            return Err("centre presentation cannot be bounded by pixbuf loaders".into());
        }
    };
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
            let scale = if matches!(presentation, nacre_protocol::wallpaper::Presentation::Crop) {
                (target.0 as f64 / source_width as f64).max(target.1 as f64 / source_height as f64)
            } else {
                (target.0 as f64 / source_width as f64)
                    .min(target.1 as f64 / source_height as f64)
                    .min(1.0)
            };
            let scaled_width =
                ((source_width as f64 * scale).ceil() as i64).clamp(1, 100_000) as i32;
            let scaled_height =
                ((source_height as f64 * scale).ceil() as i64).clamp(1, 100_000) as i32;
            let scaled_bytes = (scaled_width as usize)
                .checked_mul(scaled_height as usize)
                .and_then(|pixels| pixels.checked_mul(4));
            if scaled_width as u32 > MAX_OUTPUT_DIMENSION
                || scaled_height as u32 > MAX_OUTPUT_DIMENSION
                || scaled_bytes.is_none_or(|bytes| bytes > MAX_OUTPUT_RGBA_BYTES)
                || (scaled_width as u64).saturating_mul(scaled_height as u64) > max_dim
            {
                loader.set_size(1, 1);
            } else {
                requested_size_cb.set((scaled_width, scaled_height));
                loader.set_size(scaled_width, scaled_height);
            }
        }
    });
    let mut input = BufReader::new(&file);
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
    let (max_width, max_height) = requested_size.get();
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
            > max_dim
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
        for x in 0..copy_width as usize {
            let src = y * stride + x * channels;
            let dst = ((y + dy as usize) * width as usize + x + dx as usize) * 4;
            rgba[dst..dst + 3].copy_from_slice(&data[src..src + 3]);
            rgba[dst + 3] = if channels == 4 { data[src + 3] } else { 255 };
        }
    }
    // Check the same still-open inode after decoding, before publishing bytes.
    let after = identity_fd
        .metadata()
        .map_err(|_| "wallpaper metadata unavailable")?;
    if !pair.file_identity.matches(&after) {
        return Err("wallpaper changed during decode".into());
    }
    Ok(rgba)
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

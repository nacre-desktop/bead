// SPDX-License-Identifier: GPL-3.0-only
//! Bounded display-sized wallpaper raster cache. Entries are private raw RGBA with zstd.
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::SystemTime,
};

/// Disk budget (B0171: raised from 144 MiB so the whole picker set stays cached: 17 owner
/// wallpapers at 2880x1800 need about 120-170 MiB and LRU churn made selections miss).
pub(crate) const CACHE_LIMIT: u64 = 512 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"BWRGZ001";
const HEADER: usize = 8 + 4 + 4 + 8 + 8 + 4 + 4 + 4;
type CacheIdentity = (u64, i64, u32, u32, u32);
type CachePath = (PathBuf, CacheIdentity);

fn cache_root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("bead/wallpaper-display")
}

fn identity(pair: &ActiveWallpaper) -> Result<(u64, i64, u32, u32, u32), String> {
    let meta = fs::metadata(&pair.path).map_err(|_| "wallpaper cache metadata unavailable")?;
    if !meta.is_file() || !pair.file_identity.matches(&meta) {
        return Err("wallpaper cache identity changed".into());
    }
    #[cfg(unix)]
    let mtime_ns = meta.mtime_nsec();
    #[cfg(not(unix))]
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.subsec_nanos() as i64);
    Ok((
        meta.len(),
        meta.mtime(),
        mtime_ns as u32,
        meta.ino() as u32,
        meta.dev() as u32,
    ))
}

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

fn entry_path(
    root: &Path,
    pair: &ActiveWallpaper,
    width: u32,
    height: u32,
) -> Result<CachePath, String> {
    let id = identity(pair)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    pair.path.hash(&mut hasher);
    id.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    Ok((root.join(format!("{:016x}.rgba.zst", hasher.finish())), id))
}

fn load_from(root: &Path, pair: &ActiveWallpaper, width: u32, height: u32) -> Option<Vec<u8>> {
    let (path, (size, mtime, mtime_ns, _ino, _dev)) = entry_path(root, pair, width, height).ok()?;
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if width == 0 || height == 0 || expected > 64 * 1024 * 1024 || width > 16_384 || height > 16_384
    {
        return None;
    }
    let mut file = OpenOptions::new().read(true).open(&path).ok()?;
    let mut header = [0u8; HEADER];
    file.read_exact(&mut header).ok()?;
    if &header[..8] != MAGIC
        || u32::from_le_bytes(header[8..12].try_into().ok()?) != width
        || u32::from_le_bytes(header[12..16].try_into().ok()?) != height
        || u64::from_le_bytes(header[16..24].try_into().ok()?) != size
        || i64::from_le_bytes(header[24..32].try_into().ok()?) != mtime
        || u32::from_le_bytes(header[32..36].try_into().ok()?) != mtime_ns
        || u32::from_le_bytes(header[36..40].try_into().ok()?) as usize != expected
    {
        let _ = fs::remove_file(path);
        return None;
    }
    let encoded_len = u32::from_le_bytes(header[40..44].try_into().ok()?) as usize;
    if encoded_len == 0 || encoded_len > expected.saturating_add(1024 * 1024) {
        return None;
    }
    let mut encoded = vec![0; encoded_len];
    file.read_exact(&mut encoded).ok()?;
    let mut trailing = [0];
    if file.read(&mut trailing).ok()? != 0 {
        let _ = fs::remove_file(path);
        return None;
    }
    let _ = file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now()));
    let decoder = zstd::stream::read::Decoder::new(&encoded[..]).ok()?;
    let mut decoded = Vec::with_capacity(expected);
    decoder
        .take(expected as u64 + 1)
        .read_to_end(&mut decoded)
        .ok()?;
    if decoded.len() != expected {
        return None;
    }
    let (current_path, current_identity) = entry_path(root, pair, width, height).ok()?;
    (current_path == path && current_identity == (size, mtime, mtime_ns, _ino, _dev))
        .then_some(decoded)
}

pub(crate) fn load_or_decode(
    pair: &ActiveWallpaper,
    width: u32,
    height: u32,
    decode: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    pair.validate().map_err(|_| "invalid wallpaper pair")?;
    let output_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || output_bytes.is_none_or(|bytes| bytes > 64 * 1024 * 1024)
    {
        return Err("output dimensions exceed wallpaper bound".into());
    }
    if pair.motion.is_some() {
        return decode();
    }
    load_or_decode_async_at(&cache_root(), pair, width, height, decode)
}

/// Return an already cached raster without starting a decode or blocking on a miss.
pub(crate) fn load_cached(pair: &ActiveWallpaper, width: u32, height: u32) -> Option<Vec<u8>> {
    load_cached_at(&cache_root(), pair, width, height)
}

fn load_cached_at(root: &Path, pair: &ActiveWallpaper, width: u32, height: u32) -> Option<Vec<u8>> {
    if pair.motion.is_some() || pair.validate().is_err() {
        return None;
    }
    load_from(root, pair, width, height)
}

/// A decode of one cache entry that other callers (the selection predecode, the synchronous
/// prepare, the picker prefetch) join instead of decoding the same image again.
#[derive(Default)]
struct Flight {
    result: Mutex<Option<Result<Arc<Vec<u8>>, String>>>,
    done: Condvar,
}

fn flights() -> &'static Mutex<HashMap<PathBuf, Arc<Flight>>> {
    static FLIGHTS: std::sync::OnceLock<Mutex<HashMap<PathBuf, Arc<Flight>>>> =
        std::sync::OnceLock::new();
    FLIGHTS.get_or_init(Default::default)
}

/// True when a display-sized entry for `pair` is already on disk (no decompression).
pub(crate) fn is_cached(pair: &ActiveWallpaper, width: u32, height: u32) -> bool {
    entry_path(&cache_root(), pair, width, height).is_ok_and(|(path, _)| path.is_file())
}

fn load_or_decode_async_at(
    root: &Path,
    pair: &ActiveWallpaper,
    width: u32,
    height: u32,
    decode: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if let Some(rgba) = load_from(root, pair, width, height) {
        return Ok(rgba);
    }
    let key = entry_path(root, pair, width, height)?.0;
    let (flight, leader) = {
        let mut map = flights().lock().unwrap_or_else(|p| p.into_inner());
        match map.get(&key) {
            Some(flight) => (flight.clone(), false),
            None => {
                let flight = Arc::new(Flight::default());
                map.insert(key.clone(), flight.clone());
                (flight, true)
            }
        }
    };
    if !leader {
        let mut result = flight.result.lock().unwrap_or_else(|p| p.into_inner());
        while result.is_none() {
            result = flight.done.wait(result).unwrap_or_else(|p| p.into_inner());
        }
        return result
            .as_ref()
            .expect("finished flight")
            .clone()
            .map(|rgba| rgba.to_vec());
    }
    // Finish the flight even if `decode` panics, so followers never wait forever.
    struct Finish(PathBuf, Arc<Flight>);
    impl Drop for Finish {
        fn drop(&mut self) {
            flights()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&self.0);
            let mut result = self.1.result.lock().unwrap_or_else(|p| p.into_inner());
            result.get_or_insert_with(|| Err("wallpaper decode abandoned".into()));
            self.1.done.notify_all();
        }
    }
    let finish = Finish(key, flight.clone());
    let decoded = decode().map(Arc::new);
    *flight.result.lock().unwrap_or_else(|p| p.into_inner()) = Some(decoded.clone());
    drop(finish);
    let rgba = decoded?;
    let root = root.to_path_buf();
    let pair = pair.clone();
    let write_rgba = rgba.clone();
    let _ = std::thread::Builder::new()
        .name("wallpaper-cache-write".into())
        .spawn(move || {
            // Cache generation must never compete with a switch or the interactive shell.
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 10);
            }
            store_at(&root, &pair, width, height, &write_rgba, CACHE_LIMIT);
        });
    Ok(rgba.to_vec())
}

#[cfg(test)]
fn load_or_decode_at(
    root: &Path,
    pair: &ActiveWallpaper,
    width: u32,
    height: u32,
    limit: u64,
    decode: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if let Some(rgba) = load_from(root, pair, width, height) {
        return Ok(rgba);
    }
    let rgba = decode()?;
    store_at(root, pair, width, height, &rgba, limit);
    Ok(rgba)
}

fn store_at(root: &Path, pair: &ActiveWallpaper, width: u32, height: u32, rgba: &[u8], limit: u64) {
    let Some(expected) = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return;
    };
    if expected != rgba.len() || expected as u64 > limit {
        return;
    }
    let Ok((path, (size, mtime, mtime_ns, _, _))) = entry_path(root, pair, width, height) else {
        return;
    };
    if fs::create_dir_all(root).is_err() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(root, fs::Permissions::from_mode(0o700));
    }
    let Ok(encoded) = zstd::stream::encode_all(rgba, 1) else {
        return;
    };
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(MAGIC)?;
        f.write_all(&width.to_le_bytes())?;
        f.write_all(&height.to_le_bytes())?;
        f.write_all(&size.to_le_bytes())?;
        f.write_all(&mtime.to_le_bytes())?;
        f.write_all(&mtime_ns.to_le_bytes())?;
        f.write_all(&(expected as u32).to_le_bytes())?;
        f.write_all(&(encoded.len() as u32).to_le_bytes())?;
        f.write_all(&encoded)?;
        f.sync_all()?;
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
        return;
    }
    let _ = evict(root, limit, &path);
}

fn evict(root: &Path, limit: u64, keep: &Path) -> std::io::Result<()> {
    let mut files = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| {
                (
                    entry.path(),
                    meta.len(),
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                )
            })
        })
        .collect::<Vec<_>>();
    let mut total = files.iter().map(|f| f.1).sum::<u64>();
    files.sort_by_key(|f| f.2);
    for (path, size, _) in files {
        if total <= limit {
            break;
        }
        if path == keep {
            continue;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nacre_protocol::wallpaper::WallpaperFileIdentity;

    fn fixture() -> (PathBuf, ActiveWallpaper) {
        let root = std::env::temp_dir().join(format!(
            "wallpaper-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.bin");
        fs::write(&source, [1u8, 2, 3]).unwrap();
        let identity = WallpaperFileIdentity::from_metadata(&fs::metadata(&source).unwrap());
        let pair: ActiveWallpaper = serde_json::from_value(serde_json::json!({"revision":1,"palette_revision":1,"path":source,"file_identity":identity,"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#101820","onSurface":"#f0f4f8","primary":"#22aa88"}}})).unwrap();
        (root, pair)
    }
    #[test]
    fn invalidates_changed_source_and_dimensions() {
        let (root, pair) = fixture();
        let cache = root.join("cache");
        let pixels = vec![5; 16];
        store_at(&cache, &pair, 2, 2, &pixels, 1024);
        assert_eq!(load_from(&cache, &pair, 2, 2), Some(pixels));
        assert_eq!(load_cached_at(&cache, &pair, 2, 2), Some(vec![5; 16]));
        assert!(load_cached_at(&cache, &pair, 1, 4).is_none());
        assert!(load_from(&cache, &pair, 1, 4).is_none());
        fs::write(&pair.path, [1u8, 2, 3, 4]).unwrap();
        assert!(load_cached_at(&cache, &pair, 2, 2).is_none());
        assert!(load_from(&cache, &pair, 2, 2).is_none());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn enforces_bound_and_lru() {
        let (root, pair) = fixture();
        let cache = root.join("cache");
        store_at(&cache, &pair, 2, 2, &[7; 16], 110);
        std::thread::sleep(std::time::Duration::from_millis(5));
        store_at(&cache, &pair, 3, 2, &[8; 24], 110);
        let total = fs::read_dir(&cache)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.metadata().unwrap().len())
            .sum::<u64>();
        assert!(total <= 110);
        assert!(load_from(&cache, &pair, 3, 2).is_some());
        assert!(load_from(&cache, &pair, 2, 2).is_none());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn concurrent_requests_for_one_entry_decode_it_once() {
        let (root, pair) = fixture();
        let cache = root.join("cache");
        let decodes = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let handles = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        load_or_decode_async_at(&cache, &pair, 2, 2, || {
                            decodes.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            Ok(vec![3; 16])
                        })
                    })
                })
                .collect::<Vec<_>>();
            for handle in handles {
                assert_eq!(handle.join().unwrap(), Ok(vec![3; 16]));
            }
        });
        assert_eq!(decodes.load(Ordering::SeqCst), 1);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn corrupt_entry_is_a_miss_and_decode_can_replace_it() {
        let (root, pair) = fixture();
        let cache = root.join("cache");
        let (path, _) = entry_path(&cache, &pair, 2, 2).unwrap();
        fs::create_dir_all(&cache).unwrap();
        fs::write(&path, b"broken").unwrap();
        let mut decoded = false;
        let pixels = load_or_decode_at(&cache, &pair, 2, 2, 1024, || {
            decoded = true;
            Ok(vec![9; 16])
        })
        .unwrap();
        assert!(decoded);
        assert_eq!(pixels, vec![9; 16]);
        assert_eq!(load_from(&cache, &pair, 2, 2), Some(vec![9; 16]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires the owner's private wallpaper set"]
    fn real_set_cache_hit_and_miss_microbenchmark() {
        let Some(dir) = std::env::var_os("B0146_REAL_WALLPAPERS").map(PathBuf::from) else {
            return;
        };
        let (root, _) = fixture();
        let cache = root.join("cache");
        let mut misses = Vec::new();
        let mut hits = Vec::new();
        fn collect_images(dir: &Path, files: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).unwrap().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    collect_images(&path, files);
                } else if matches!(
                    path.extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .as_str(),
                    "jpg" | "jpeg" | "png"
                ) {
                    files.push(path);
                }
            }
        }
        let mut files = Vec::new();
        collect_images(&dir, &mut files);
        assert!(!files.is_empty(), "no real wallpaper files found");
        for path in &files {
            let identity = WallpaperFileIdentity::from_metadata(&fs::metadata(path).unwrap());
            let pair: ActiveWallpaper = serde_json::from_value(serde_json::json!({"revision":1,"palette_revision":1,"path":path,"file_identity":identity,"presentation":"crop","palette":{"mode":"dark","colours":{"background":"#101820","onSurface":"#f0f4f8","primary":"#22aa88"}}})).unwrap();
            let start = std::time::Instant::now();
            let _ = load_or_decode_async_at(&cache, &pair, 2880, 1800, || {
                crate::wallpaper_decode::decode_pair(pair.clone(), 2880, 1800)
            })
            .unwrap();
            misses.push(start.elapsed().as_secs_f64() * 1000.0);
            let mut warmed = false;
            for _ in 0..500 {
                if load_from(&cache, &pair, 2880, 1800).is_some() {
                    warmed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(warmed, "display cache writer did not finish");
            // Wayland SHM slots are reused; allocate the staging slot before timing.
            let mut shm_bgra = vec![0u8; 2880 * 1800 * 4];
            for _ in 0..5 {
                let start = std::time::Instant::now();
                let rgba = load_from(&cache, &pair, 2880, 1800).unwrap();
                let (dst_pixels, dst_tail) = shm_bgra.as_chunks_mut::<4>();
                let (src_pixels, src_tail) = rgba.as_chunks::<4>();
                debug_assert!(dst_tail.is_empty() && src_tail.is_empty());
                for (dst, src) in dst_pixels.iter_mut().zip(src_pixels) {
                    dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
                }
                std::hint::black_box(&shm_bgra);
                hits.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        fn percentile(v: &mut [f64], q: f64) -> f64 {
            v.sort_by(f64::total_cmp);
            v[((v.len() as f64 * q).ceil() as usize)
                .saturating_sub(1)
                .min(v.len() - 1)]
        }
        eprintln!(
            "B0146_WALLPAPER_CACHE files={} hit_load_bgra_p50_ms={:.2} hit_load_bgra_p95_ms={:.2} miss_decode_p50_ms={:.2} miss_decode_p95_ms={:.2}",
            files.len(),
            percentile(&mut hits, 0.50),
            percentile(&mut hits, 0.95),
            percentile(&mut misses, 0.50),
            percentile(&mut misses, 0.95)
        );
        let _ = fs::remove_dir_all(root);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Give clean file-backed pages (program text, read-only data, libraries) back to the page cache.
//!
//! The loader and the kernel's fault-around and large-folio mapping leave nearly all of a
//! large executable resident, although a closed-idle shell touches a small part of it. The
//! pages stay in the page cache; only this process's mapping of them is dropped, and they
//! are mapped again on the next touch. Only private mappings with no anonymous or dirty
//! pages qualify, so relocated data (RELRO, `.data`) is never discarded.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn release() -> usize {
    let Ok(smaps) = std::fs::read_to_string("/proc/self/smaps") else {
        return 0;
    };
    let mut released = 0;
    // (start, end) of the mapping being read, while it still qualifies.
    let mut current: Option<(usize, usize)> = None;
    let mut finish = |range: &mut Option<(usize, usize)>| {
        if let Some((start, end)) = range.take() {
            // SAFETY: the range is a clean private file mapping of this process; dropping its
            // pages only means they are faulted in again from the page cache on the next touch.
            if unsafe {
                libc::madvise(start as *mut libc::c_void, end - start, libc::MADV_DONTNEED)
            } == 0
            {
                released += end - start;
            }
        }
    };
    for line in smaps.lines() {
        let mut parts = line.split_ascii_whitespace();
        let first = parts.next().unwrap_or("");
        if let Some((lo, hi)) = first.split_once('-')
            && let (Ok(start), Ok(end)) =
                (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16))
        {
            finish(&mut current);
            let perms = parts.next().unwrap_or("");
            let path = parts.nth(3).unwrap_or("");
            // r-xp text and r--p read-only data of files; not shared, writable or special mappings.
            if matches!(perms, "r-xp" | "r--p")
                && path.starts_with('/')
                && !path.contains("(deleted)")
            {
                current = Some((start, end));
            }
        } else if current.is_some() {
            let dirty = matches!(
                first,
                "Anonymous:" | "Private_Dirty:" | "Shared_Dirty:" | "Swap:"
            ) && parts.next().is_some_and(|v| v != "0");
            if dirty {
                current = None;
            }
        }
    }
    finish(&mut current);
    released
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub(crate) fn release() -> usize {
    0
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(7);
    static TABLE: [fn() -> u32; 2] = [|| 1, || 2];

    #[test]
    fn release_keeps_writable_and_relocated_data() {
        let heap = vec![9_u8; 64 * 1024];
        COUNTER.store(11, Ordering::Relaxed);
        let bytes = super::release();
        assert!(bytes > 0, "a test binary has clean file mappings");
        // Code, relocated pointer tables (RELRO), statics and the heap all still work.
        assert_eq!(TABLE.iter().map(|f| f()).sum::<u32>(), 3);
        assert_eq!(COUNTER.load(Ordering::Relaxed), 11);
        assert!(heap.iter().all(|b| *b == 9));
        // A second call is harmless.
        super::release();
    }
}

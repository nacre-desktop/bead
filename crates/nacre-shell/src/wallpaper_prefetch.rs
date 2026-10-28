// SPDX-License-Identifier: GPL-3.0-only
//! B0171: while the picker is open, decode every catalogue image to the output's display size
//! once, so the first selection of any wallpaper is a cache hit instead of a 400 ms+ decode.
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Default)]
pub(crate) struct Prefetch {
    epoch: Arc<AtomicU64>,
    /// One decode at a time, whichever prefetch generation asks for it.
    gate: Arc<Mutex<()>>,
}

impl Prefetch {
    /// Replace any earlier plan; `pairs` are decoded in order, skipping cached entries.
    pub(crate) fn start(&self, pairs: Vec<ActiveWallpaper>, width: u32, height: u32) {
        let mine = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        if pairs.is_empty() {
            return;
        }
        let (epoch, gate) = (self.epoch.clone(), self.gate.clone());
        let _ = std::thread::Builder::new()
            .name("wallpaper-prefetch".into())
            .spawn(move || {
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as u32, 10);
                }
                // Give the newly populated picker scene a frame before background decoding.
                std::thread::sleep(std::time::Duration::from_millis(100));
                for pair in pairs {
                    if epoch.load(Ordering::Acquire) != mine {
                        return;
                    }
                    if crate::wallpaper_cache::is_cached(&pair, width, height) {
                        continue;
                    }
                    let _one = gate.lock().unwrap_or_else(|p| p.into_inner());
                    if epoch.load(Ordering::Acquire) != mine {
                        return;
                    }
                    let _ = crate::wallpaper::decode_pair(pair, width, height);
                }
            });
    }

    pub(crate) fn cancel(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }
}

/// Catalogue ids nearest the active wallpaper first, so the likeliest next picks are ready first.
pub(crate) fn nearest_first(ids: &[String], active: Option<usize>) -> Vec<String> {
    let centre = active.unwrap_or(0) as isize;
    let mut order: Vec<(isize, usize)> = (0..ids.len())
        .map(|index| ((index as isize - centre).abs(), index))
        .collect();
    order.sort();
    order
        .into_iter()
        .map(|(_, index)| ids[index].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_order_starts_beside_the_active_wallpaper() {
        let ids: Vec<String> = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
        assert_eq!(nearest_first(&ids, Some(2)), ["c", "b", "d", "a", "e"]);
        assert_eq!(nearest_first(&ids, None), ["a", "b", "c", "d", "e"]);
    }
}

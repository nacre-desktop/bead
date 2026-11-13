// SPDX-License-Identifier: MIT
//! Dormant sources own no surfaces. Any wakeup invalidates their cached inputs.
use crate::SceneSource;
use std::collections::VecDeque;

pub(super) struct Entry {
    pub epoch: u64,
    pub source: Box<dyn SceneSource>,
    overlay: bool,
    name: String,
    bytes: usize,
}
#[derive(Default)]
pub(super) struct Cache(VecDeque<Entry>);
impl Cache {
    fn stats(&self) -> (usize, usize) {
        (self.0.len(), self.0.iter().map(|entry| entry.bytes).sum())
    }
    fn trace(&self) {
        let (count, bytes) = self.stats();
        bead_core::vprintln!("WARM_SOURCE_CACHE count={count} admission_bytes={bytes}");
    }
    pub fn park(
        &mut self,
        overlay: bool,
        name: String,
        epoch: u64,
        source: Box<dyn SceneSource>,
        budget: usize,
    ) {
        if !source.dormant_ready() {
            return;
        }
        let Some(bytes) = source
            .warm_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_add(name.capacity() + std::mem::size_of::<Entry>() + 64)
            })
            .filter(|bytes| *bytes <= budget)
        else {
            return;
        };
        self.0
            .retain(|entry| entry.overlay != overlay || entry.name != name);
        while self.0.iter().map(|entry| entry.bytes).sum::<usize>() > budget - bytes {
            self.0.pop_front();
        }
        self.0.push_back(Entry {
            overlay,
            name,
            epoch,
            source,
            bytes,
        });
        self.trace();
    }
    pub fn take(&mut self, overlay: bool, name: &str) -> Option<Entry> {
        let index = self
            .0
            .iter()
            .position(|entry| entry.overlay == overlay && entry.name == name)?;
        let entry = self.0.remove(index);
        self.trace();
        entry
    }
    /// Dropping sends Stop even if a Ready packet is awaiting acknowledgement.
    pub fn invalidate(&mut self, overlay: bool, epoch: u64) -> bool {
        let before = self.0.len();
        self.0
            .retain(|entry| entry.overlay != overlay || entry.epoch != epoch);
        let invalidated = self.0.len() != before;
        if invalidated {
            self.trace();
        }
        invalidated
    }
}

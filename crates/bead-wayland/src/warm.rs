// SPDX-License-Identifier: MIT
//! Dormant sources own no surfaces. Payload-free model hints wait for resume.
use crate::{SceneSource, UiEvent};
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
    /// Consume callbacks for dormant epochs. Empty model hints carry no state:
    /// mandatory resume polls their producers. Every payload/protocol event still
    /// invalidates the source, so no data or acknowledgement can be lost.
    pub fn handle_event(&mut self, overlay: bool, epoch: u64, event: &UiEvent) -> bool {
        if !self
            .0
            .iter()
            .any(|entry| entry.overlay == overlay && entry.epoch == epoch)
        {
            return false;
        }
        if !matches!(event, UiEvent::Model(values) if values.is_empty()) {
            self.invalidate(overlay, epoch);
        }
        true
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

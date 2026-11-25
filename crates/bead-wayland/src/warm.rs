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
/// Dormant entries, oldest first, and (B0392) one source retired mid-close with its budget.
#[derive(Default)]
pub(super) struct Cache(VecDeque<Entry>, Option<(Entry, usize)>);
/// The admission charge of a dormant source named with `name` bytes, if it fits `budget`.
pub(super) fn charge(source: &dyn SceneSource, name: usize, budget: usize) -> Option<usize> {
    if !source.dormant_ready() {
        return None;
    }
    source
        .warm_retained_bytes()
        .and_then(|bytes| bytes.checked_add(name + std::mem::size_of::<Entry>() + 64))
        .filter(|bytes| *bytes <= budget)
}
impl Cache {
    pub fn stats(&self) -> (usize, usize) {
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
        let Some(bytes) = charge(source.as_ref(), name.capacity(), budget) else {
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
    /// B0392: a source retired before its close transaction was acknowledged waits here, replacing
    /// an older one; `handle_event` acknowledges its own packets and parks it once it is dormant.
    pub fn park_closing(
        &mut self,
        overlay: bool,
        name: String,
        epoch: u64,
        source: Box<dyn SceneSource>,
        budget: usize,
    ) {
        self.1 = (source.alive() && source.warm_retained_bytes().is_some()).then(|| {
            let entry = Entry {
                overlay,
                name,
                epoch,
                source,
                bytes: 0,
            };
            (entry, budget)
        });
    }
    /// B0405: a closed source parks. One that never heard its close (a prepared panel never
    /// shown) hears it once and waits in the closing slot; true when it does, so the caller
    /// arms `drop_closing`.
    pub fn park_closed(
        &mut self,
        overlay: bool,
        name: String,
        epoch: u64,
        mut source: Box<dyn SceneSource>,
        budget: usize,
        close_heard: bool,
    ) -> bool {
        if close_heard {
            self.park(overlay, name, epoch, source, budget);
            return false;
        }
        if source.stage(UiEvent::PanelVisibility(false)).is_err() {
            return false;
        }
        self.park_closing(overlay, name, epoch, source, budget);
        true
    }
    /// A closing source not acknowledged in time retires as a cold one does.
    pub fn drop_closing(&mut self, overlay: bool, epoch: u64) {
        if self
            .1
            .as_ref()
            .is_some_and(|(entry, _)| entry.overlay == overlay && entry.epoch == epoch)
        {
            self.1 = None;
        }
    }
    pub fn take(&mut self, overlay: bool, name: &str) -> Option<Entry> {
        // A requested view still closing goes cold: one source per view.
        if self
            .1
            .as_ref()
            .is_some_and(|(entry, _)| entry.overlay == overlay && entry.name == name)
        {
            self.1 = None;
        }
        let index = self
            .0
            .iter()
            .position(|entry| entry.overlay == overlay && entry.name == name)?;
        let entry = self.0.remove(index);
        self.trace();
        // A worker that died while parked is a miss (B0345).
        entry.filter(|entry| entry.source.alive())
    }
    /// Consume callbacks for dormant epochs. Empty model hints carry no state:
    /// mandatory resume polls their producers. Every payload/protocol event still
    /// retires the source, so stale state or a waiting acknowledgement cannot survive.
    /// A closing source's own clean packets are acknowledged as a host without a surface would.
    pub fn handle_event(&mut self, overlay: bool, epoch: u64, event: &UiEvent) -> bool {
        if let Some((mut entry, budget)) = self
            .1
            .take_if(|(entry, _)| entry.overlay == overlay && entry.epoch == epoch)
        {
            let kept = match event {
                UiEvent::Model(values) => values.is_empty(),
                UiEvent::Worker(packet) if !packet.has_error() => {
                    match entry.source.stage(UiEvent::Worker(packet.clone())) {
                        // Ready sends its Ack; Accepted commits the candidate.
                        Ok(Some(_)) => {
                            entry.source.commit();
                            true
                        }
                        Ok(None) => true,
                        Err(_) => false,
                    }
                }
                _ => false,
            };
            if kept && entry.source.dormant_ready() {
                self.park(overlay, entry.name, epoch, entry.source, budget);
            } else if kept {
                self.1 = Some((entry, budget));
            }
            return true;
        }
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

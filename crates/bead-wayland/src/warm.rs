// SPDX-License-Identifier: MIT
//! Dormant sources own no surfaces. Payload-free model hints wait for resume.
use crate::{SceneSource, UiEvent};
use std::collections::{BTreeMap, VecDeque};

pub(super) struct Entry {
    pub epoch: u64,
    pub source: Box<dyn SceneSource>,
    /// B0412: the view's last open content extent, so its reopened surface starts at that size.
    pub extent: Option<(u32, u32)>,
    overlay: bool,
    name: String,
    bytes: usize,
}
/// Dormant entries, oldest first, (B0392) one source retired mid-close with its budget, and
/// (B0416) the newest palette commit's generation.
#[derive(Default)]
pub(super) struct Cache(VecDeque<Entry>, Option<(Entry, usize)>, u64);
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
/// Whether a dormant source stays as it was: an empty model hint, or its own clean packet, which
/// it stages and commits (Ready sends its Ack; Accepted commits the candidate).
fn stays_dormant(source: &mut dyn SceneSource, event: &UiEvent) -> bool {
    match event {
        UiEvent::Model(values) => values.is_empty(),
        UiEvent::Worker(packet) if !packet.has_error() => {
            match source.stage(UiEvent::Worker(packet.clone())) {
                Ok(Some(_)) => {
                    source.commit();
                    true
                }
                Ok(None) => true,
                Err(_) => false,
            }
        }
        _ => false,
    }
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
            bead_core::vprintln!(
                "WARM_PARK_REFUSED name={name} dormant={} retained={:?} budget={budget}",
                source.dormant_ready(),
                source.warm_retained_bytes()
            );
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
            extent: None,
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
        // B0411: verbose-only WARM_CLOSING_* lines name why a closing source parks late or not.
        let admitted = source.alive() && source.warm_retained_bytes().is_some();
        if !admitted {
            bead_core::vprintln!(
                "WARM_CLOSING_REFUSED epoch={epoch} alive={} retained={:?}",
                source.alive(),
                source.warm_retained_bytes()
            );
        }
        self.1 = admitted.then(|| {
            let entry = Entry {
                overlay,
                name,
                epoch,
                source,
                extent: None,
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
        if let Err(error) = source.stage(UiEvent::PanelVisibility(false)) {
            bead_core::vprintln!("WARM_CLOSING_REFUSED epoch={epoch} stage_error={error}");
            return false;
        }
        self.park_closing(overlay, name, epoch, source, budget);
        true
    }
    /// B0412: the parked or closing source `epoch` remembers its view's last open content extent.
    pub fn set_extent(&mut self, overlay: bool, epoch: u64, extent: Option<(u32, u32)>) {
        let closing = self.1.as_mut().map(|(entry, _)| entry);
        for entry in self.0.iter_mut().chain(closing) {
            if entry.overlay == overlay && entry.epoch == epoch {
                entry.extent = extent;
            }
        }
    }
    /// B0416: a trailing debounce. Each palette commit arms a one-shot 1 s timer with its
    /// generation, and `dormant_palette` acts on the newest only: commits that never pause for
    /// 1 s (a video wallpaper) stage nothing.
    pub fn debounce_palette<T: 'static>(
        &mut self,
        handle: &calloop::LoopHandle<'static, T>,
        fire: fn(&mut T, u64),
    ) {
        self.2 += 1;
        let generation = self.2;
        let timer = calloop::timer::Timer::from_duration(std::time::Duration::from_secs(1));
        let _ = handle.insert_source(timer, move |_, _, state| {
            fire(state, generation);
            calloop::timer::TimeoutAction::Drop
        });
    }
    /// B0416: unless a newer commit armed since, each parked source takes `palette` in the
    /// background; `handle_event` acknowledges its packets. One that cannot stage it retires.
    pub fn dormant_palette(&mut self, generation: u64, palette: &BTreeMap<String, [u8; 4]>) {
        if generation == self.2 {
            self.0
                .retain_mut(|entry| entry.source.dormant_palette(palette.clone()).is_ok());
        }
    }
    /// A closing source not acknowledged in time retires as a cold one does.
    pub fn drop_closing(&mut self, overlay: bool, epoch: u64) {
        if self
            .1
            .as_ref()
            .is_some_and(|(entry, _)| entry.overlay == overlay && entry.epoch == epoch)
        {
            bead_core::vprintln!("WARM_CLOSING_EXPIRED epoch={epoch}");
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
            bead_core::vprintln!("WARM_CLOSING_RETIRED event=take name={name}");
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
    /// A source's own clean packets (a closing one's, or B0416 a parked one's background palette)
    /// are acknowledged as a host without a surface would.
    pub fn handle_event(&mut self, overlay: bool, epoch: u64, event: &UiEvent) -> bool {
        if let Some((mut entry, budget)) = self
            .1
            .take_if(|(entry, _)| entry.overlay == overlay && entry.epoch == epoch)
        {
            // B0414: the closing view's own request to be closed carries no state.
            let kept = matches!(event, UiEvent::CompanionVisibility(false))
                || stays_dormant(entry.source.as_mut(), event);
            if kept && entry.source.dormant_ready() {
                self.park(overlay, entry.name, epoch, entry.source, budget);
                self.set_extent(overlay, epoch, entry.extent);
            } else if kept {
                bead_core::vprintln!(
                    "WARM_CLOSING_WAIT epoch={epoch} event={}",
                    bead_core::profile::variant_name(event)
                );
                self.1 = Some((entry, budget));
            } else {
                // A Worker without packet_error failed to stage.
                bead_core::vprintln!(
                    "WARM_CLOSING_RETIRED epoch={epoch} event={} packet_error={:?}",
                    bead_core::profile::variant_name(event),
                    match event {
                        UiEvent::Worker(packet) => packet.error(),
                        _ => None,
                    }
                );
            }
            return true;
        }
        let Some(entry) = self
            .0
            .iter_mut()
            .find(|entry| entry.overlay == overlay && entry.epoch == epoch)
        else {
            return false;
        };
        if !stays_dormant(entry.source.as_mut(), event) {
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

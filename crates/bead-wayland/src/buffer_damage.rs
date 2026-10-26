// SPDX-License-Identifier: MIT OR Apache-2.0
//! Semantic surface damage plus independent repair history for reusable SHM buffers.
use super::{Buffer, SlotPool};
use bead_render::{Damage, damage::FrameDamage, tree::Frame};
use std::error::Error;
const EMPTY: Damage = Damage {
    x: 0,
    y: 0,
    w: 0,
    h: 0,
};
pub(super) struct BufferDamage {
    history: FrameDamage,
    pub pending: Vec<Damage>,
    pub surface: Damage,
    last_committed_slot: Option<usize>,
    committed_update: Option<u64>,
    frame_presented: bool,
    identical_revision: Option<u64>,
}
impl Default for BufferDamage {
    fn default() -> Self {
        Self {
            history: Default::default(),
            pending: Vec::new(),
            surface: EMPTY,
            last_committed_slot: None,
            committed_update: None,
            frame_presented: false,
            identical_revision: None,
        }
    }
}
impl BufferDamage {
    pub fn reset(&mut self, width: u32, height: u32, buffers: usize) {
        self.history = Default::default();
        self.last_committed_slot = None;
        self.committed_update = None;
        self.frame_presented = false;
        self.identical_revision = None;
        self.surface = Damage::full(width, height);
        self.pending = vec![self.surface; buffers];
    }
    pub fn update(
        &mut self,
        frame: Option<&Frame>,
        offset: (f32, f32),
        opacity: f32,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let semantic = match frame {
            Some(frame) => self.history.update(frame, offset, opacity, width, height)?,
            None => vec![Damage::full(width, height)],
        };
        for damage in semantic {
            self.surface = self.surface.union(damage);
            for pending in &mut self.pending {
                *pending = pending.union(damage);
            }
        }
        Ok(())
    }
    /// Rebase a stale free slot only when copying the committed source will save
    /// more than a fifth of the frame's area in repairs. The caller repaints the rest.
    fn adoption_source(&self, target: usize, frame_pixels: u64) -> Option<(usize, Damage)> {
        let source = self.last_committed_slot?;
        if source == target {
            return None;
        }
        let source_pending = self.pending[source];
        let target_pending = self.pending[target];
        let area = |damage: Damage| u64::from(damage.w) * u64::from(damage.h);
        let saved = area(target_pending).saturating_sub(area(source_pending));
        (saved > frame_pixels / 5).then_some((source, source_pending))
    }
    /// The copy completed; the target now owes the source's smaller repair.
    fn adopted(&mut self, target: usize, repairs: Damage) {
        self.pending[target] = repairs;
    }
    /// Reuse the last committed pixels for a stale free target, then return the
    /// exact pending repair rectangle the caller must draw before committing.
    pub(super) fn repair(
        &mut self,
        pool: &mut SlotPool,
        buffers: &[Buffer],
        target: usize,
        width: u32,
        height: u32,
    ) -> Result<Damage, Box<dyn Error>> {
        let adoption = self.adoption_source(target, u64::from(width) * u64::from(height));
        if let Some((from, repairs)) = adoption {
            let (source, len) = {
                let committed = buffers[from].slot();
                let data = pool.raw_data_mut(&committed);
                (data.as_ptr(), data.len())
            };
            let target_canvas = buffers[target]
                .canvas(pool)
                .ok_or("compositor owns buffer")?;
            // SAFETY: adoption_source rejects target == from. The committed
            // source and free target are stable, non-overlapping slots in this pool.
            super::rendering::copy_outside_repairs(
                unsafe { std::slice::from_raw_parts(source, len) },
                target_canvas,
                width,
                height,
                std::slice::from_ref(&repairs),
                // Scale/viewport resets can leave rounded-size buffers with old
                // pixels, so never assume the target is zero-filled. These
                // surfaces already paint the full SHM frame on first use.
                false,
            );
            // The copied slot now needs only the committed source's pending repair.
            // Leave semantic surface damage untouched for the eventual commit.
            self.adopted(target, repairs);
        }
        Ok(self.pending[target])
    }
    pub fn committed(&mut self, slot: usize, update: u64) {
        self.pending[slot] = EMPTY;
        self.surface = EMPTY;
        self.last_committed_slot = Some(slot);
        self.committed_update = Some(update);
        self.frame_presented = false;
        self.identical_revision = None;
    }
    /// A pixel-identical generation inherits only a confirmed presentation of
    /// the latest committed frame. Otherwise wait for that frame's feedback.
    pub fn identical_generation(&mut self, revision: u64) -> Option<u64> {
        if self.frame_presented {
            Some(revision)
        } else {
            self.identical_revision = Some(revision);
            None
        }
    }
    pub fn presented(&mut self, update: u64) -> Option<u64> {
        if self.committed_update == Some(update) {
            self.frame_presented = true;
            self.identical_revision.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adoption_uses_only_the_last_committed_slot_and_keeps_surface_damage() {
        let mut damage = BufferDamage::default();
        damage.reset(100, 100, 3);
        let full = Damage::full(100, 100);
        assert_eq!(damage.adoption_source(1, 10_000), None);

        damage.committed(0, 7);
        let repair = Damage {
            x: 40,
            y: 30,
            w: 12,
            h: 9,
        };
        let surface = Damage {
            x: 35,
            y: 28,
            w: 24,
            h: 18,
        };
        damage.pending[0] = repair;
        damage.pending[1] = full;
        damage.pending[2] = full;
        damage.surface = surface;

        assert_eq!(
            damage.adoption_source(0, 10_000),
            None,
            "a slot cannot adopt itself"
        );
        assert_eq!(damage.adoption_source(1, 10_000), Some((0, repair)));
        assert_eq!(damage.adoption_source(2, 10_000), Some((0, repair)));
        damage.pending[1] = repair;
        assert_eq!(
            damage.adoption_source(1, 10_000),
            None,
            "equal repair areas do not trigger a copy"
        );
        damage.pending[1] = full;

        damage.adopted(1, repair);
        assert_eq!(damage.pending[1], repair);
        assert_eq!(damage.surface, surface, "adoption is buffer-local");
        assert_eq!(
            damage.last_committed_slot,
            Some(0),
            "an uncommitted target never replaces the source"
        );
    }

    #[test]
    fn adoption_requires_more_than_a_fifth_of_frame_repair_savings() {
        let mut damage = BufferDamage::default();
        damage.reset(100, 100, 2);
        damage.committed(0, 1);
        damage.pending[1] = Damage::full(100, 100);

        damage.pending[0] = Damage {
            x: 0,
            y: 0,
            w: 99,
            h: 100,
        };
        assert_eq!(damage.adoption_source(1, 10_000), None, "tiny savings");

        damage.pending[0].w = 80;
        assert_eq!(
            damage.adoption_source(1, 10_000),
            None,
            "exactly 20% is not enough"
        );

        damage.pending[0].w = 79;
        assert_eq!(
            damage.adoption_source(1, 10_000),
            Some((0, damage.pending[0]))
        );
    }

    #[test]
    fn observed_expanded_window_repair_exceeds_adoption_threshold() {
        let mut damage = BufferDamage::default();
        damage.reset(1389, 1662, 2);
        damage.committed(0, 1);
        let source_repair = Damage {
            x: 420,
            y: 262,
            w: 948,
            h: 1379,
        };
        let target_repair = Damage {
            x: 57,
            y: 262,
            w: 1311,
            h: 1379,
        };
        damage.pending[0] = source_repair;
        damage.pending[1] = target_repair;

        assert_eq!(
            u64::from(target_repair.w) * u64::from(target_repair.h)
                - u64::from(source_repair.w) * u64::from(source_repair.h),
            500_577
        );
        assert_eq!(
            damage.adoption_source(1, 2_308_518),
            Some((0, source_repair)),
            "the measured 21.7% reduction avoids the oversized SHM repair"
        );
    }

    #[test]
    fn reset_clears_commit_source_and_failed_target_keeps_its_repair() {
        let mut damage = BufferDamage::default();
        damage.reset(100, 100, 3);
        assert_eq!(damage.last_committed_slot, None);

        damage.committed(0, 7);
        assert_eq!(damage.last_committed_slot, Some(0));
        let repair = Damage {
            x: 8,
            y: 9,
            w: 5,
            h: 6,
        };
        damage.pending[0] = repair;
        let target_repair = damage.pending[1];
        assert_eq!(damage.adoption_source(1, 10_000), Some((0, repair)));
        assert_eq!(
            damage.pending[1], target_repair,
            "selection does not mutate repair"
        );
        // A renderer error after a successful copy retains the copied repair
        // but cannot advance the last committed source.
        damage.adopted(1, repair);
        assert_eq!(damage.pending[1], repair);
        assert_eq!(damage.last_committed_slot, Some(0));
        assert_eq!(damage.adoption_source(2, 10_000), Some((0, repair)));
        damage.reset(200, 120, 3);

        assert_eq!(damage.last_committed_slot, None);
        assert_eq!(damage.pending, vec![Damage::full(200, 120); 3]);
        assert_eq!(damage.adoption_source(1, 24_000), None);
    }

    #[test]
    fn identical_generation_waits_for_its_committed_pixels() {
        let mut damage = BufferDamage::default();
        damage.reset(100, 100, 3);
        damage.committed(0, 7);
        assert_eq!(damage.identical_generation(2), None);
        assert_eq!(
            damage.presented(6),
            None,
            "stale feedback cannot acknowledge new pixels"
        );
        assert_eq!(damage.presented(7), Some(2));
        assert_eq!(
            damage.presented(7),
            None,
            "deferred acknowledgement happens once"
        );
        assert_eq!(damage.identical_generation(3), Some(3));
        damage.committed(1, 9);
        assert_eq!(damage.identical_generation(4), None);
        assert_eq!(damage.presented(7), None);
        damage.reset(200, 100, 3);
        assert_eq!(damage.presented(9), None, "old geometry feedback is stale");
        assert_eq!(damage.identical_generation(5), None);
    }
}

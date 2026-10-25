// SPDX-License-Identifier: MIT OR Apache-2.0
//! Semantic surface damage plus independent repair history for reusable SHM buffers.
use bead_render::{Damage, damage::FrameDamage, tree::Frame};
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
            committed_update: None,
            frame_presented: false,
            identical_revision: None,
        }
    }
}
impl BufferDamage {
    pub fn reset(&mut self, width: u32, height: u32, buffers: usize) {
        self.history = Default::default();
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
    pub fn committed(&mut self, slot: usize, update: u64) {
        self.pending[slot] = EMPTY;
        self.surface = EMPTY;
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

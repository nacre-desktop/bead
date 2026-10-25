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
}
impl Default for BufferDamage {
    fn default() -> Self {
        Self {
            history: Default::default(),
            pending: Vec::new(),
            surface: EMPTY,
        }
    }
}
impl BufferDamage {
    pub fn reset(&mut self, width: u32, height: u32, buffers: usize) {
        self.history = Default::default();
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
    pub fn committed(&mut self, slot: usize) {
        self.pending[slot] = EMPTY;
        self.surface = EMPTY;
    }
}

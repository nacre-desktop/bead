// SPDX-License-Identifier: MIT OR Apache-2.0
//! Per-surface CPU-first / warm GPU / idle CPU lifecycle.
#[derive(Default)]
pub(crate) struct Policy {
    first: bool,
    warmed: bool,
    gpu: bool,
    last_active: f64,
}
impl Policy {
    pub fn needs_warmup(&self) -> bool {
        self.first && !self.warmed
    }
    pub fn frame(&mut self, now: f64, animated: bool) -> bool {
        if !self.first {
            self.first = true;
            return false;
        }
        if animated {
            self.last_active = now;
        }
        if !self.gpu && (!self.warmed || animated) {
            self.warmed = true;
            self.gpu = true;
            self.last_active = now;
            return true;
        }
        false
    }
    pub fn remaining(&self, now: f64) -> f64 {
        (2. - (now - self.last_active)).max(0.001)
    }
    pub fn expire(&mut self, now: f64) -> bool {
        if self.gpu && now - self.last_active >= 2. {
            self.gpu = false;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_frame_idle_release_and_animated_reopen() {
        let mut p = Policy::default();
        assert!(!p.frame(0., false));
        assert!(p.needs_warmup());
        assert!(p.frame(0.01, false));
        assert!(!p.expire(1.));
        assert!(p.expire(2.02));
        assert!(!p.frame(3., false));
        assert!(p.frame(4., true));
        assert!(!p.frame(5., true));
        assert!(!p.expire(6.));
        assert!(p.expire(7.));
    }
}

/// Each surface owns one live frame request. Parked callbacks become stale when
/// content reopens or its renderer changes; late replies must not grant a frame.
#[derive(Default)]
pub(crate) struct Callbacks {
    pub pending: bool,
    epoch: u64,
}
impl Callbacks {
    pub fn request(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = true;
        self.epoch
    }
    pub fn invalidate(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = false;
    }
    pub fn accept(&mut self, epoch: u64) -> bool {
        if self.pending && epoch == self.epoch {
            self.pending = false;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod callback_tests {
    use super::*;
    #[test]
    fn reopening_discards_parked_callback_and_never_grants_a_duplicate_frame() {
        let mut c = Callbacks::default();
        let parked = c.request();
        c.invalidate();
        let current = c.request();
        assert!(!c.accept(parked));
        assert!(c.pending);
        assert!(c.accept(current));
        assert!(!c.accept(current));
    }
}

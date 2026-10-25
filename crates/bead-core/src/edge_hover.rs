// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared lip/body/focus exit grace, independent of surface or view ownership.
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct EdgeHover {
    deadline: Option<Instant>,
}
impl EdgeHover {
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Returns one close decision after leaving both regions without input focus.
    pub fn update(
        &mut self,
        enabled: bool,
        regions: (bool, bool),
        focused: bool,
        now: Instant,
        delay: Duration,
    ) -> bool {
        if !enabled || regions.0 || regions.1 || focused {
            self.deadline = None;
            return false;
        }
        let deadline = *self.deadline.get_or_insert(now + delay);
        if now < deadline {
            return false;
        }
        self.deadline = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lip_body_and_focus_share_one_cancellable_exit_deadline() {
        let mut hover = EdgeHover::default();
        let start = Instant::now();
        let delay = Duration::from_millis(130);
        for (lip, body, focus) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert!(!hover.update(
                true,
                (lip, body),
                focus,
                start + Duration::from_secs(5),
                delay
            ));
            assert!(hover.deadline().is_none());
        }
        assert!(!hover.update(true, (false, false), false, start, delay));
        assert_eq!(hover.deadline(), Some(start + delay));
        assert!(!hover.update(true, (false, true), false, start + delay / 2, delay));
        assert!(hover.deadline().is_none());
        let exit = start + Duration::from_secs(6);
        assert!(!hover.update(true, (false, false), false, exit, delay));
        assert!(!hover.update(true, (false, false), false, exit + delay / 2, delay));
        assert!(hover.update(true, (false, false), false, exit + delay, delay));
    }
    #[test]
    fn explicit_open_or_closed_policy_cancels_grace_and_reopen_starts_fresh() {
        let mut hover = EdgeHover::default();
        let now = Instant::now();
        let delay = Duration::from_millis(130);
        assert!(!hover.update(true, (false, false), false, now, delay));
        assert!(!hover.update(false, (false, false), false, now + delay, delay));
        assert!(hover.deadline().is_none());
        assert!(!hover.update(true, (false, false), false, now + delay * 2, delay));
        assert_eq!(hover.deadline(), Some(now + delay * 3));
    }
}

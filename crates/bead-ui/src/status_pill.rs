// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deterministic routing for an icon group, hover previews and a shared panel.
//! Appearance, view names and timing policy are supplied by the shell.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timings {
    pub peek_dwell: Duration,
    pub peek_exit: Duration,
    pub panel_crossing: Duration,
    pub panel_exit: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusPill {
    timings: Timings,
    hovered: Option<String>,
    peek: Option<String>,
    panel: Option<String>,
    peek_at: Option<Instant>,
    peek_close_at: Option<Instant>,
    close_at: Option<Instant>,
    panel_entered: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Change {
    pub peek: Option<String>,
    pub panel: Option<String>,
}

impl StatusPill {
    pub fn new(timings: Timings) -> Self {
        Self {
            timings,
            hovered: None,
            peek: None,
            panel: None,
            peek_at: None,
            peek_close_at: None,
            close_at: None,
            panel_entered: false,
        }
    }
    pub fn hovered(&self) -> Option<&str> {
        self.hovered.as_deref()
    }
    pub fn peek(&self) -> Option<&str> {
        self.peek.as_deref()
    }
    pub fn panel(&self) -> Option<&str> {
        self.panel.as_deref()
    }
    fn change(&self) -> Change {
        Change {
            peek: self.peek.clone(),
            panel: self.panel.clone(),
        }
    }

    /// Entering another icon restarts dwell; motion within one icon does not.
    /// Returning from content to the icon group restores the crossing grace.
    pub fn enter_icon(&mut self, item: impl Into<String>, now: Instant) -> Change {
        let item = item.into();
        let changed = self.hovered.as_deref() != Some(item.as_str());
        self.hovered = Some(item.clone());
        self.close_at = None;
        self.peek_close_at = None;
        self.panel_entered = false;
        if self.panel.is_some() {
            self.peek = None;
            self.peek_at = None;
        } else if changed && self.peek.as_deref() != Some(&item) {
            self.peek = None;
            self.peek_at = Some(now + self.timings.peek_dwell);
        }
        self.change()
    }

    /// Repeated activation of the same icon keeps its panel open.
    pub fn click_icon(&mut self, item: impl Into<String>) -> Change {
        let item = item.into();
        self.hovered = Some(item.clone());
        self.peek = None;
        self.peek_at = None;
        self.peek_close_at = None;
        self.close_at = None;
        self.panel_entered = false;
        self.panel = Some(item);
        self.change()
    }

    /// Arm expiry once, preserving the deadline during continued outside motion.
    pub fn leave_region(&mut self, now: Instant) -> Change {
        self.hovered = None;
        self.peek_at = None;
        if self.panel.is_some() {
            self.close_at.get_or_insert(
                now + if self.panel_entered {
                    self.timings.panel_exit
                } else {
                    self.timings.panel_crossing
                },
            );
        } else if self.peek.is_some() {
            self.peek_close_at
                .get_or_insert(now + self.timings.peek_exit);
        }
        self.change()
    }

    /// The pointer entered the visible preview or clicked panel content.
    pub fn enter_region(&mut self) -> Change {
        self.hovered = None;
        self.close_at = None;
        self.peek_close_at = None;
        self.panel_entered = self.panel.is_some();
        self.change()
    }

    /// Escape and externally dismissed panels cancel every pending transition.
    pub fn escape(&mut self) -> Change {
        self.hovered = None;
        self.peek = None;
        self.panel = None;
        self.panel_entered = false;
        self.peek_at = None;
        self.peek_close_at = None;
        self.close_at = None;
        self.change()
    }

    pub fn advance(&mut self, now: Instant) -> Change {
        if self.peek_at.is_some_and(|at| at <= now) {
            self.peek_at = None;
            if self.panel.is_none() {
                self.peek = self.hovered.clone();
            }
        }
        if self.close_at.is_some_and(|at| at <= now) {
            self.close_at = None;
            self.panel = None;
            self.panel_entered = false;
        }
        if self.peek_close_at.is_some_and(|at| at <= now) {
            self.peek_close_at = None;
            self.peek = None;
        }
        self.change()
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        [self.peek_at, self.close_at, self.peek_close_at]
            .into_iter()
            .flatten()
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DWELL: Duration = Duration::from_millis(350);
    const PEEK_EXIT: Duration = Duration::from_millis(120);
    const CROSSING: Duration = Duration::from_millis(600);
    const PANEL_EXIT: Duration = Duration::from_millis(130);
    const MS: Duration = Duration::from_millis(1);
    fn state() -> StatusPill {
        StatusPill::new(Timings {
            peek_dwell: DWELL,
            peek_exit: PEEK_EXIT,
            panel_crossing: CROSSING,
            panel_exit: PANEL_EXIT,
        })
    }
    fn preview(s: &mut StatusPill, now: Instant) -> Instant {
        s.enter_icon("a", now);
        s.advance(now + DWELL);
        assert_eq!(s.peek(), Some("a"));
        now + DWELL
    }
    #[test]
    fn dwell_opens_only_at_deadline() {
        let now = Instant::now();
        let mut s = state();
        assert_eq!(s.enter_icon("a", now), Change::default());
        assert_eq!(s.next_deadline(), Some(now + DWELL));
        assert_eq!(s.advance(now + DWELL - MS).peek, None);
        assert_eq!(s.advance(now + DWELL).peek.as_deref(), Some("a"));
        assert_eq!(s.next_deadline(), None);
    }
    #[test]
    fn same_icon_motion_preserves_original_dwell() {
        let now = Instant::now();
        let mut s = state();
        s.enter_icon("a", now);
        s.enter_icon("a", now + DWELL / 2);
        assert_eq!(s.next_deadline(), Some(now + DWELL));
        assert_eq!(s.advance(now + DWELL).peek.as_deref(), Some("a"));
    }
    #[test]
    fn retargeting_requires_full_new_dwell() {
        let now = Instant::now();
        let mut s = state();
        s.enter_icon("a", now);
        s.enter_icon("b", now + DWELL - MS);
        assert_eq!(s.advance(now + DWELL).peek, None);
        assert_eq!(s.advance(now + DWELL * 2 - MS).peek.as_deref(), Some("b"));
    }
    #[test]
    fn leave_before_dwell_cancels_pending_preview() {
        let now = Instant::now();
        let mut s = state();
        s.enter_icon("a", now);
        s.leave_region(now + MS);
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + DWELL).peek, None);
    }
    #[test]
    fn visible_preview_expires_after_leave_grace() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        s.leave_region(now);
        assert_eq!(s.advance(now + PEEK_EXIT - MS).peek.as_deref(), Some("a"));
        assert_eq!(s.advance(now + PEEK_EXIT).peek, None);
    }
    #[test]
    fn entering_preview_cancels_expiry() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        s.leave_region(now);
        s.enter_region();
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + PEEK_EXIT).peek.as_deref(), Some("a"));
        s.leave_region(now + PEEK_EXIT);
        assert_eq!(s.advance(now + PEEK_EXIT * 2).peek, None);
    }
    #[test]
    fn returning_to_same_preview_icon_needs_no_new_dwell() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        s.leave_region(now);
        assert_eq!(s.enter_icon("a", now + MS).peek.as_deref(), Some("a"));
        assert_eq!(s.next_deadline(), None);
    }
    #[test]
    fn retargeting_visible_preview_hides_old_content() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        assert_eq!(s.enter_icon("b", now).peek, None);
        assert_eq!(s.advance(now + DWELL).peek.as_deref(), Some("b"));
    }
    #[test]
    fn click_during_dwell_cancels_preview() {
        let now = Instant::now();
        let mut s = state();
        s.enter_icon("a", now);
        let change = s.click_icon("a");
        assert_eq!(change.peek, None);
        assert_eq!(change.panel.as_deref(), Some("a"));
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + DWELL).peek, None);
    }
    #[test]
    fn click_visible_preview_cancels_its_expiry_and_same_click_keeps_panel() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        s.leave_region(now);
        s.click_icon("a");
        assert_eq!(s.peek(), None);
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.click_icon("a").panel.as_deref(), Some("a"));
        assert_eq!(s.advance(now + PEEK_EXIT).panel.as_deref(), Some("a"));
    }
    #[test]
    fn open_panel_preserves_peer_hover_but_suppresses_peeks_then_switches() {
        let now = Instant::now();
        let mut s = state();
        s.click_icon("a");
        s.enter_icon("b", now);
        assert_eq!(s.hovered(), Some("b"));
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + DWELL).peek, None);
        assert_eq!(s.panel(), Some("a"));
        assert_eq!(s.click_icon("b").panel.as_deref(), Some("b"));
    }
    #[test]
    fn first_crossing_allows_longer_grace() {
        let now = Instant::now();
        let mut s = state();
        s.click_icon("a");
        s.leave_region(now);
        assert_eq!(s.advance(now + CROSSING - MS).panel.as_deref(), Some("a"));
        assert_eq!(s.advance(now + CROSSING).panel, None);
    }
    #[test]
    fn entering_panel_cancels_crossing_then_uses_short_exit() {
        let now = Instant::now();
        let mut s = state();
        s.click_icon("a");
        s.leave_region(now);
        s.enter_region();
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + CROSSING).panel.as_deref(), Some("a"));
        s.leave_region(now + CROSSING);
        assert_eq!(
            s.advance(now + CROSSING + PANEL_EXIT - MS).panel.as_deref(),
            Some("a")
        );
        assert_eq!(s.advance(now + CROSSING + PANEL_EXIT).panel, None);
    }
    #[test]
    fn header_return_and_switch_restore_longer_crossing() {
        let now = Instant::now();
        for switch in [false, true] {
            let mut s = state();
            s.click_icon("a");
            s.enter_region();
            s.leave_region(now);
            s.enter_icon("b", now + MS);
            if switch {
                s.click_icon("b");
            }
            s.leave_region(now + MS * 2);
            assert_eq!(s.next_deadline(), Some(now + MS * 2 + CROSSING));
        }
    }
    #[test]
    fn outside_motion_never_restarts_any_expiry() {
        let now = Instant::now();
        for entered in [false, true] {
            let mut s = state();
            s.click_icon("a");
            if entered {
                s.enter_region();
            }
            s.leave_region(now);
            let deadline = s.next_deadline().unwrap();
            s.leave_region(deadline - MS);
            assert_eq!(s.next_deadline(), Some(deadline));
            assert_eq!(s.advance(deadline).panel, None);
        }
        let mut s = state();
        let now = preview(&mut s, now);
        s.leave_region(now);
        s.leave_region(now + PEEK_EXIT - MS);
        assert_eq!(s.advance(now + PEEK_EXIT).peek, None);
    }
    #[test]
    fn escape_cancels_all_phases_and_reopen_starts_fresh() {
        let now = Instant::now();
        for phase in 0..5 {
            let mut s = state();
            s.enter_icon("a", now);
            if phase > 0 {
                s.advance(now + DWELL);
            }
            if phase > 1 {
                s.leave_region(now + DWELL);
            }
            if phase > 2 {
                s.click_icon("a");
            }
            if phase > 3 {
                s.enter_region();
                s.leave_region(now + DWELL);
            }
            assert_eq!(s.escape(), Change::default());
            assert_eq!(s.hovered(), None);
            assert_eq!(s.next_deadline(), None);
            assert_eq!(s.advance(now + DWELL + CROSSING), Change::default());
            s.enter_icon("b", now + DWELL + CROSSING);
            assert_eq!(s.next_deadline(), Some(now + DWELL * 2 + CROSSING));
        }
    }
    #[test]
    fn idle_and_zero_duration_policy_are_well_defined() {
        let now = Instant::now();
        let mut s = StatusPill::new(Timings {
            peek_dwell: Duration::ZERO,
            peek_exit: Duration::ZERO,
            panel_crossing: Duration::ZERO,
            panel_exit: Duration::ZERO,
        });
        assert_eq!(s.advance(now), Change::default());
        s.enter_icon("custom-view", now);
        assert_eq!(s.advance(now).peek.as_deref(), Some("custom-view"));
        s.leave_region(now);
        assert_eq!(s.advance(now), Change::default());
    }
}

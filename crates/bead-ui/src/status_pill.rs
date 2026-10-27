// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deterministic routing for an icon group, hover previews and a shared panel.
//! Appearance, view names and timing policy are supplied by the shell.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timings {
    pub peek_dwell: Duration,
    pub peek_exit: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusPill {
    timings: Timings,
    hovered: Option<String>,
    peek: Option<String>,
    panel: Option<String>,
    /// Prevent the explicit close from immediately turning into a hover preview.
    suppressed_hover: Option<(String, Instant)>,
    peek_at: Option<Instant>,
    peek_close_at: Option<Instant>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Change {
    pub peek: Option<String>,
    pub panel: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PointerRect {
    fn contains(self, point: (f32, f32), tolerance: f32) -> bool {
        point.0 >= self.x - tolerance
            && point.0 <= self.x + self.width + tolerance
            && point.1 >= self.y - tolerance
            && point.1 <= self.y + self.height + tolerance
    }
}

/// The visible dropdown, its vertical continuation through the bar, and its owner slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeepOpenZone {
    pub dropdown: PointerRect,
    pub bar_band: PointerRect,
    pub owner_slot: PointerRect,
    pub tolerance: f32,
}

impl KeepOpenZone {
    pub fn contains(self, point: (f32, f32)) -> bool {
        self.dropdown.contains(point, self.tolerance)
            || self.bar_band.contains(point, self.tolerance)
            || self.owner_slot.contains(point, self.tolerance)
    }
}

impl StatusPill {
    pub fn new(timings: Timings) -> Self {
        Self {
            timings,
            hovered: None,
            peek: None,
            panel: None,
            suppressed_hover: None,
            peek_at: None,
            peek_close_at: None,
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
    /// Pointer dwell opens previews; click-open panels suppress peer previews.
    pub fn enter_icon(&mut self, item: impl Into<String>, now: Instant) -> Change {
        let item = item.into();
        let changed = self.hovered.as_deref() != Some(item.as_str());
        self.hovered = Some(item.clone());
        if self
            .suppressed_hover
            .as_ref()
            .is_some_and(|(owner, until)| owner == &item && now <= *until)
        {
            self.peek = None;
            self.peek_at = None;
            self.peek_close_at = None;
            return self.change();
        }
        self.suppressed_hover = None;
        self.peek_close_at = None;
        if self.panel.is_some() {
            self.peek = None;
            self.peek_at = None;
        } else if changed && self.peek.as_deref() != Some(&item) {
            self.peek = None;
            self.peek_at = Some(now + self.timings.peek_dwell);
        }
        self.change()
    }

    /// Repeated activation closes the owner; another icon switches the open panel.
    pub fn click_icon(&mut self, item: impl Into<String>) -> Change {
        let item = item.into();
        if self.panel.as_deref() == Some(item.as_str()) {
            return self.escape();
        }
        self.suppressed_hover = None;
        self.hovered = Some(item.clone());
        self.peek = None;
        self.peek_at = None;
        self.peek_close_at = None;
        self.panel = Some(item);
        self.change()
    }

    /// Hover previews expire after their exit delay; clicked panels wait for an explicit dismissal.
    pub fn leave_region(&mut self, now: Instant) -> Change {
        self.hovered = None;
        self.peek_at = None;
        if self.peek.is_some() {
            self.peek_close_at
                .get_or_insert(now + self.timings.peek_exit);
        }
        self.change()
    }

    /// The pointer entered a visible preview or popup body.
    pub fn enter_region(&mut self) -> Change {
        self.hovered = None;
        self.peek_close_at = None;
        self.change()
    }

    /// A bar press outside the open dropdown's column dismisses it.
    pub fn outside_click(&mut self, point: (f32, f32), zone: KeepOpenZone) -> Change {
        if self.panel.is_some() && !zone.contains(point) {
            self.escape()
        } else {
            self.change()
        }
    }

    /// Escape and externally dismissed panels cancel every pending transition.
    pub fn escape(&mut self) -> Change {
        self.hovered = None;
        self.peek = None;
        if let Some(owner) = self.panel.take() {
            self.suppressed_hover = Some((owner, Instant::now() + self.timings.peek_dwell));
        }
        self.peek_at = None;
        self.peek_close_at = None;
        self.change()
    }

    pub fn advance(&mut self, now: Instant) -> Change {
        if self.peek_at.is_some_and(|at| at <= now) {
            self.peek_at = None;
            if self.panel.is_none() {
                self.peek = self.hovered.clone();
            }
        }
        if self.peek_close_at.is_some_and(|at| at <= now) {
            self.peek_close_at = None;
            self.peek = None;
        }
        self.change()
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        [self.peek_at, self.peek_close_at]
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
    const MS: Duration = Duration::from_millis(1);
    const WAIT: Duration = Duration::from_secs(2);
    fn state() -> StatusPill {
        StatusPill::new(Timings {
            peek_dwell: DWELL,
            peek_exit: PEEK_EXIT,
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
    fn click_visible_preview_cancels_expiry_and_second_owner_click_closes() {
        let mut s = state();
        let now = preview(&mut s, Instant::now());
        s.leave_region(now);
        s.click_icon("a");
        assert_eq!(s.peek(), None);
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.click_icon("a").panel, None);
        assert_eq!(s.advance(now + PEEK_EXIT).panel, None);
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
    fn click_open_panel_stays_open_after_pointer_leaves_bar_and_panel() {
        let now = Instant::now();
        let mut s = state();
        s.click_icon("a");
        s.leave_region(now);
        assert_eq!(s.advance(now + WAIT).panel.as_deref(), Some("a"));
    }
    #[test]
    fn click_open_panel_stays_open_after_pointer_exits_panel() {
        let now = Instant::now();
        let mut s = state();
        s.click_icon("a");
        s.leave_region(now);
        s.enter_region();
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.advance(now + WAIT).panel.as_deref(), Some("a"));
        s.leave_region(now + WAIT);
        assert_eq!(s.advance(now + WAIT * 2).panel.as_deref(), Some("a"));
    }
    #[test]
    fn hover_across_peer_icons_never_retargets_or_closes_a_click_open_panel() {
        let now = Instant::now();
        for switch in [false, true] {
            let mut s = state();
            s.click_icon("a");
            s.enter_region();
            s.leave_region(now);
            s.enter_icon("b", now + MS);
            if switch {
                assert_eq!(s.click_icon("b").panel.as_deref(), Some("b"));
            } else {
                assert_eq!(s.panel(), Some("a"));
            }
            s.leave_region(now + MS * 2);
            assert_eq!(s.next_deadline(), None);
            assert_eq!(
                s.advance(now + WAIT).panel.as_deref(),
                Some(if switch { "b" } else { "a" })
            );
        }
    }
    #[test]
    fn click_open_panel_ignores_pointer_exit_until_dismissed_or_toggled() {
        let now = Instant::now();
        for entered in [false, true] {
            let mut s = state();
            s.click_icon("a");
            if entered {
                s.enter_region();
            }
            s.leave_region(now);
            assert_eq!(s.next_deadline(), None);
            assert_eq!(s.advance(now + WAIT).panel.as_deref(), Some("a"));
        }
        let mut s = state();
        let now = preview(&mut s, now);
        s.leave_region(now);
        s.leave_region(now + PEEK_EXIT - MS);
        assert_eq!(s.advance(now + PEEK_EXIT).peek, None);
    }

    #[test]
    fn keep_open_zone_covers_dropdown_column_gaps_and_owner_slot() {
        let zone = KeepOpenZone {
            dropdown: PointerRect {
                x: 800.,
                y: 50.,
                width: 240.,
                height: 220.,
            },
            bar_band: PointerRect {
                x: 800.,
                y: 0.,
                width: 240.,
                height: 50.,
            },
            owner_slot: PointerRect {
                x: 1030.,
                y: 0.,
                width: 48.,
                height: 50.,
            },
            tolerance: 4.,
        };
        assert!(zone.contains((920., 200.)));
        assert!(zone.contains((920., 26.))); // bar column above dropdown
        assert!(zone.contains((1050., 26.))); // owning slot beside its column
        assert!(zone.contains((1081., 26.))); // tolerance at a slot edge
        assert!(!zone.contains((1083., 26.)));
        assert!(!zone.contains((700., 26.)));
        assert!(!zone.contains((920., 400.)));
    }

    #[test]
    fn bar_click_inside_zone_keeps_panel_and_outside_zone_dismisses() {
        let zone = KeepOpenZone {
            dropdown: PointerRect {
                x: 800.,
                y: 50.,
                width: 240.,
                height: 220.,
            },
            bar_band: PointerRect {
                x: 800.,
                y: 0.,
                width: 240.,
                height: 50.,
            },
            owner_slot: PointerRect {
                x: 1030.,
                y: 0.,
                width: 48.,
                height: 50.,
            },
            tolerance: 4.,
        };
        let mut pill = state();
        pill.click_icon("battery");
        assert_eq!(
            pill.outside_click((920., 26.), zone).panel.as_deref(),
            Some("battery")
        );
        assert_eq!(pill.outside_click((700., 26.), zone).panel, None);
    }

    #[test]
    fn explicit_close_suppresses_hover_reopen_until_the_pointer_leaves_owner() {
        let now = Instant::now();
        let mut pill = state();
        pill.click_icon("battery");
        assert_eq!(pill.click_icon("battery").panel, None);
        assert_eq!(pill.enter_icon("battery", now).peek, None);
        assert_eq!(pill.advance(now + DWELL).peek, None);
        pill.enter_icon("bluetooth", now + DWELL);
        assert_eq!(pill.enter_icon("battery", now + DWELL).peek, None);
        assert_eq!(pill.next_deadline(), Some(now + DWELL * 2));
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
            assert_eq!(s.advance(now + DWELL + WAIT), Change::default());
            s.enter_icon("b", now + DWELL + WAIT);
            assert_eq!(s.next_deadline(), Some(now + DWELL * 2 + WAIT));
        }
    }
    #[test]
    fn idle_and_zero_duration_policy_are_well_defined() {
        let now = Instant::now();
        let mut s = StatusPill::new(Timings {
            peek_dwell: Duration::ZERO,
            peek_exit: Duration::ZERO,
        });
        assert_eq!(s.advance(now), Change::default());
        s.enter_icon("custom-view", now);
        assert_eq!(s.advance(now).peek.as_deref(), Some("custom-view"));
        s.leave_region(now);
        assert_eq!(s.advance(now), Change::default());
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//! B0036: the owner's panel enable flags and frame sheen as the shell loop's live
//! switches. Nacre keys stay in `prefs`; this maps decoded values onto the generic
//! `ShellControl` answers (bar present, panel allowed, decoration variant).
use crate::prefs::Prefs;
use bead::wayland::ShellControl;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
};

pub struct Gate {
    bar: AtomicBool,
    dashboard: AtomicBool,
    controls: AtomicBool,
    sheen: AtomicBool,
    variant: AtomicU64,
    watchers: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}
impl Gate {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            bar: AtomicBool::new(true),
            dashboard: AtomicBool::new(true),
            controls: AtomicBool::new(true),
            sheen: AtomicBool::new(true),
            variant: AtomicU64::new(0),
            watchers: Mutex::new(Vec::new()),
        })
    }
    /// Take the decoded values; watchers run only when an answer changed.
    pub fn apply(&self, p: &Prefs) {
        let mut changed = false;
        for (cell, value) in [
            (&self.bar, p.top_bar_enabled),
            (&self.dashboard, p.dashboard_enabled),
            (&self.controls, p.control_center_enabled),
        ] {
            changed |= cell.swap(value, SeqCst) != value;
        }
        if self.sheen.swap(p.frame_sheen, SeqCst) != p.frame_sheen {
            self.variant.fetch_add(1, SeqCst);
            // Open panels carry their own frame art: rebuild them too.
            bead::ui::text_style::refresh();
            changed = true;
        }
        if changed {
            for watcher in self
                .watchers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
            {
                watcher();
            }
        }
    }
}
impl ShellControl for Gate {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) {
        self.watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(changed);
    }
    fn bar_enabled(&self) -> bool {
        self.bar.load(SeqCst)
    }
    /// Dashboard pages follow `dashboardEnabled`; the audio, network, Bluetooth and
    /// battery panels (and their hover peeks) follow `controlCenterEnabled`.
    fn panel_allowed(&self, view: &str) -> bool {
        let view = view.strip_prefix("peek-").unwrap_or(view);
        if view.starts_with("dashboard-") {
            self.dashboard.load(SeqCst)
        } else if matches!(
            view,
            "sound" | "audio" | "network" | "bluetooth" | "battery"
        ) {
            self.controls.load(SeqCst)
        } else {
            true
        }
    }
    fn decoration_variant(&self) -> u64 {
        self.variant.load(SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, atomic::AtomicU32};

    #[test]
    fn flags_gate_their_panels_and_wake_watchers_only_on_change() {
        let gate = Gate::new();
        let woke = Arc::new(AtomicU32::new(0));
        let counter = woke.clone();
        gate.watch(Box::new(move || {
            counter.fetch_add(1, SeqCst);
        }));
        assert!(gate.bar_enabled() && gate.panel_allowed("dashboard-today"));
        let off = Prefs::decode(
            &json!({"dashboardEnabled": false, "controlCenterEnabled": false, "topBarEnabled": false}),
            &json!(null),
        );
        gate.apply(&off);
        gate.apply(&off);
        assert_eq!(woke.load(SeqCst), 1);
        assert!(!gate.bar_enabled());
        for view in [
            "dashboard-today",
            "dashboard-weather",
            "sound",
            "peek-network",
            "battery",
        ] {
            assert!(!gate.panel_allowed(view), "{view}");
        }
        for view in ["launcher", "calendar", "session", "tray"] {
            assert!(gate.panel_allowed(view), "{view}");
        }
        // Wrong types and missing keys mean enabled, like QML's `!== false`.
        gate.apply(&Prefs::decode(
            &json!({"dashboardEnabled": 0, "topBarEnabled": "no"}),
            &json!(null),
        ));
        assert!(gate.bar_enabled() && gate.panel_allowed("dashboard-today"));
        assert_eq!(gate.decoration_variant(), 0);
        gate.apply(&Prefs::decode(&json!({"frameSheen": false}), &json!(null)));
        assert_eq!(gate.decoration_variant(), 1);
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//! B0036: the owner's panel enable flags and frame sheen as the shell loop's live
//! switches. Nacre keys stay in `prefs`; this maps decoded values onto the generic
//! `ShellControl` answers (bar present, panel allowed, decoration variant).
use crate::{
    frame_config::FrameConfig,
    placement::{Frame, FrameSettings},
    prefs::Prefs,
};
use bead::wayland::{EdgeChrome, ShellControl, intent::Lip};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
};

/// The frame-related answers (B0047): decoded settings plus the lip configuration.
#[derive(Default)]
struct FrameState {
    settings: Option<FrameSettings>,
    ai: bool,
    delay: u32,
    click: bool,
    config: FrameConfig,
    /// Panel views that exist; a lip whose view is missing is not shown.
    views: std::collections::BTreeSet<String>,
}

pub struct Gate {
    bar: AtomicBool,
    dashboard: AtomicBool,
    controls: AtomicBool,
    sheen: AtomicBool,
    ai: AtomicBool,
    pearls: AtomicBool,
    pearl_drag: AtomicBool,
    pearl_views: Mutex<crate::pearls::Policy>,
    variant: AtomicU64,
    frame: Mutex<FrameState>,
    watchers: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}
impl Gate {
    /// Run the watchers (something the tiles depend on changed outside the settings).
    pub fn notify(&self) {
        for watcher in self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            watcher();
        }
    }
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            bar: AtomicBool::new(true),
            dashboard: AtomicBool::new(true),
            controls: AtomicBool::new(true),
            sheen: AtomicBool::new(true),
            ai: AtomicBool::new(true),
            pearls: AtomicBool::new(true),
            pearl_drag: AtomicBool::new(true),
            pearl_views: Mutex::new(Default::default()),
            variant: AtomicU64::new(0),
            frame: Mutex::new(FrameState {
                ai: true,
                delay: 350,
                ..Default::default()
            }),
            watchers: Mutex::new(Vec::new()),
        })
    }
    /// The lips the frame declares and the panel views that exist (call once at start).
    pub fn configure_frame(&self, config: FrameConfig, views: impl IntoIterator<Item = String>) {
        let mut frame = self.frame.lock().unwrap_or_else(|p| p.into_inner());
        frame.config = config;
        frame.views = views.into_iter().collect();
    }
    fn frame_lock(&self) -> std::sync::MutexGuard<'_, FrameState> {
        self.frame.lock().unwrap_or_else(|p| p.into_inner())
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
        changed |= self.ai.swap(p.ai_sidebar_enabled, SeqCst) != p.ai_sidebar_enabled;
        self.pearls.store(p.detachable_panels, SeqCst);
        self.pearl_drag.store(p.drag_panels, SeqCst);
        *self.pearl_views.lock().unwrap_or_else(|p| p.into_inner()) = crate::pearls::Policy {
            enabled: p.detachable_panels,
            drag: p.drag_panels,
            views: p.pearl_views.clone(),
        };
        let settings = p.frame_settings();
        let frame_changed = {
            let mut frame = self.frame_lock();
            let moved = frame.settings != Some(settings);
            frame.settings = Some(settings);
            frame.ai = p.ai_sidebar_enabled;
            let lips = (frame.delay, frame.click) != (p.lip_hover_delay, p.click_edge_menus);
            frame.delay = p.lip_hover_delay;
            frame.click = p.click_edge_menus;
            changed |= lips;
            moved
        };
        if self.sheen.swap(p.frame_sheen, SeqCst) != p.frame_sheen || frame_changed {
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
        } else if view == "assistant" {
            // The on-demand app owns the sidebar; its lip follows aiSidebarEnabled.
            self.ai.load(SeqCst)
        } else {
            true
        }
    }
    fn decoration_variant(&self) -> u64 {
        self.variant.load(SeqCst) + crate::placement::external_rev()
    }
    fn palette_changed(&self, palette: &std::collections::BTreeMap<String, [u8; 4]>) {
        crate::app::set_live_palette(palette);
        if crate::placement::set_live_palette(palette) {
            self.variant.fetch_add(1, SeqCst);
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
    /// The header (bar plus top frame edge) is the bar surface's thickness and zone.
    fn bar_thickness(&self) -> Option<u32> {
        let settings = self.frame_lock().settings?;
        Some(Frame::from_settings(&settings).top as u32)
    }
    fn external_panel_open(&self, view: &str, _output: &str) -> Option<bool> {
        (view == "assistant" && crate::app::configured())
            .then(|| crate::placement::external_open("chat"))
    }
    fn external_panel_request(&self, view: &str, _output: &str, toggle: bool) {
        if view == "assistant" {
            crate::app::panel_request("chat", if toggle { "toggle" } else { "hover-open" });
        }
    }
    fn external_panel_hover(&self, view: &str, _output: &str, inside: bool) {
        if view == "assistant" {
            crate::app::panel_request("chat", if inside { "hover-enter" } else { "hover-leave" });
        }
    }
    fn edge_chrome(&self) -> EdgeChrome {
        let frame = self.frame_lock();
        let settings = frame.settings.unwrap_or(FrameSettings {
            width: 10.,
            rounding: 25.,
            left: true,
            right: true,
            bottom: true,
            top: true,
            bar: self.bar.load(SeqCst),
        });
        let geometry = Frame::from_settings(&settings);
        // With the bar its surface holds the header's zone; without it the top edge needs
        // its own strip.
        let top = if settings.bar { 0. } else { geometry.top };
        let lips = frame
            .config
            .lips
            .iter()
            .filter(|l| {
                (frame.views.contains(&l.opens)
                    || (l.opens == "assistant" && crate::app::configured()))
                    && self.panel_allowed(&l.opens)
            })
            .filter_map(|l| {
                let thickness = match l.edge {
                    bead::core::Edge::Top => geometry.top,
                    bead::core::Edge::Bottom => geometry.bottom,
                    bead::core::Edge::Left => geometry.left,
                    bead::core::Edge::Right => geometry.right,
                };
                (thickness > 0.).then(|| Lip {
                    id: l.id.clone(),
                    edge: l.edge,
                    length: l.length,
                    thickness,
                    opens: l.opens.clone(),
                })
            })
            .collect();
        EdgeChrome {
            reserved: [
                top as u32,
                geometry.right as u32,
                geometry.bottom as u32,
                geometry.left as u32,
            ],
            lips,
            hover_delay_ms: frame.delay,
            click_to_open: frame.click,
        }
    }
    fn pearls_enabled(&self) -> bool {
        self.pearls.load(SeqCst)
    }
    fn pearl_drag(&self) -> bool {
        self.pearl_drag.load(SeqCst)
    }
    fn pearl_allowed(&self, view: &str) -> bool {
        self.pearl_views
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .allows(view)
    }
    fn edge_hover_allowed(&self, open_panel: Option<&str>) -> bool {
        open_panel.is_none_or(|open| {
            !self
                .frame_lock()
                .config
                .blocked_by
                .iter()
                .any(|b| b == open)
        })
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
        // Count this gate's settings rebuilds independently of external app
        // visibility revisions changed by other tests in the same process.
        assert_eq!(gate.variant.load(SeqCst), 2);
        gate.apply(&Prefs::decode(&json!({"frameSheen": false}), &json!(null)));
        assert_eq!(gate.variant.load(SeqCst), 3);
    }

    fn lips() -> FrameConfig {
        crate::frame_config::parse(
            br#"return {stores={["frame.lips"]={
                {id="dashboard",edge="top",length=208,opens="dashboard-today"},
                {id="assistant",edge="left",length=144,opens="assistant"},
                {id="controls",edge="right",length=144,opens="sound"}},
                ["frame.lips.blocked_by"]={{id="launcher"}}}}"#,
            std::path::Path::new("t.luau"),
        )
        .unwrap()
    }
    fn gate_with(desktop: serde_json::Value) -> std::sync::Arc<Gate> {
        let gate = Gate::new();
        gate.configure_frame(
            lips(),
            ["dashboard-today", "sound", "launcher"].map(String::from),
        );
        gate.apply(&Prefs::decode(&desktop, &json!(null)));
        gate
    }
    fn ids(chrome: &EdgeChrome) -> Vec<(&str, f32)> {
        chrome
            .lips
            .iter()
            .map(|l| (l.id.as_str(), l.thickness))
            .collect()
    }

    #[test]
    fn default_frame_reserves_three_edges_and_the_bar_holds_the_top() {
        let gate = gate_with(json!({}));
        let chrome = gate.edge_chrome();
        // top, right, bottom, left; the bar's own zone is the 50 px header.
        assert_eq!(chrome.reserved, [0, 10, 10, 10]);
        assert_eq!(gate.bar_thickness(), Some(50));
        // The assistant lip is hidden: no such view exists yet.
        assert_eq!(ids(&chrome), [("dashboard", 50.), ("controls", 10.)]);
        assert_eq!((chrome.hover_delay_ms, chrome.click_to_open), (350, false));
        assert!(chrome.lips.iter().all(|l| l.length > 0.));
    }

    #[test]
    fn frame_settings_resize_reserve_and_hide_lips_live() {
        let gate = gate_with(json!({
            "frameWidth": 14, "frameLeftEnabled": false, "frameRightEnabled": false,
            "frameBottomEnabled": false, "lipHoverDelay": 120, "clickEdgeMenus": true,
        }));
        let chrome = gate.edge_chrome();
        assert_eq!(chrome.reserved, [0, 0, 0, 0]);
        assert_eq!(gate.bar_thickness(), Some(54), "40 bar + 14 top edge");
        // The right frame edge is off, so the lip that sat on it has no room.
        assert_eq!(ids(&chrome), [("dashboard", 54.)]);
        assert_eq!((chrome.hover_delay_ms, chrome.click_to_open), (120, true));
        // Bar and top edge both off: nothing reserved at the top and no top lip.
        let bare = gate_with(json!({"topBarEnabled": false, "frameTopEnabled": false}));
        assert_eq!(bare.edge_chrome().reserved, [0, 10, 10, 10]);
        assert!(bare.edge_chrome().lips.iter().all(|l| l.id != "dashboard"));
        // Bar off but the top edge on: the edge needs its own strip and hosts the lip.
        let strip = gate_with(json!({"topBarEnabled": false}));
        assert_eq!(strip.edge_chrome().reserved, [10, 10, 10, 10]);
        assert_eq!(
            ids(&strip.edge_chrome()),
            [("dashboard", 10.), ("controls", 10.)]
        );
    }

    #[test]
    fn lips_follow_their_panels_enable_flags_and_blockers() {
        let gate = gate_with(json!({"dashboardEnabled": false, "controlCenterEnabled": false}));
        assert!(gate.edge_chrome().lips.is_empty());
        let gate = gate_with(json!({}));
        assert!(gate.edge_hover_allowed(None));
        assert!(gate.edge_hover_allowed(Some("dashboard-today")));
        assert!(!gate.edge_hover_allowed(Some("launcher")));
        // Changing only the delay wakes watchers without rebuilding frame tiles.
        let woke = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = woke.clone();
        gate.watch(Box::new(move || {
            counter.fetch_add(1, SeqCst);
        }));
        let variant = gate.decoration_variant();
        gate.apply(&Prefs::decode(&json!({"lipHoverDelay": 500}), &json!(null)));
        assert_eq!(woke.load(SeqCst), 1);
        assert_eq!(gate.decoration_variant(), variant);
        assert_eq!(gate.edge_chrome().hover_delay_ms, 500);
    }
}

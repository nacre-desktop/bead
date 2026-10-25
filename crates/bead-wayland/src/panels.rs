// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static view configuration; only the selected factory creates a worker.
use crate::PanelSpec;
use std::collections::BTreeMap;

pub(crate) fn anchored_position(
    scene: &crate::Scene,
    anchor: crate::PanelAnchor,
    output: (i32, i32),
) -> (i32, i32) {
    use bead_core::Edge;
    let [bx, by, w, h] = crate::rendering::content_window_rect(scene).unwrap_or([
        0,
        0,
        scene.width as i32,
        scene.bounds.h as i32,
    ]);
    let r = anchor.rect;
    let (x, y) = match anchor.edge {
        Edge::Top => (r.x + (r.w - w as f32) / 2., r.y + r.h),
        Edge::Bottom => (r.x + (r.w - w as f32) / 2., r.y - h as f32),
        Edge::Left => (r.x + r.w, r.y + (r.h - h as f32) / 2.),
        Edge::Right => (r.x - w as f32, r.y + (r.h - h as f32) / 2.),
    };
    (
        x.clamp(0., (output.0 - w).max(0) as f32).round() as i32 - bx,
        y.clamp(0., (output.1 - h).max(0) as f32).round() as i32 - by,
    )
}

pub(crate) fn validate_center(center: Option<f32>) -> Result<(), String> {
    if center.is_some_and(|n| !n.is_finite() || n.abs() > 1_000_000.) {
        return Err("invalid panel center".into());
    }
    Ok(())
}
/// Speculative work must never destroy a mapped view, including its close
/// animation/presentation. An unused, unmapped worker may be replaced.
pub(crate) fn preparation_allowed(
    target_open: bool,
    mapped: bool,
    worker: bool,
    changed: bool,
) -> bool {
    !target_open && !mapped && (!worker || changed)
}

/// A requested center runs along the surface's own edge. Keep legacy placement
/// unchanged when no center is supplied; clamp a popout inside its output.
pub(crate) fn placement(
    edge: bead_core::Edge,
    center: Option<f32>,
    size: (u32, u32),
    available: Option<(i32, i32)>,
) -> (smithay_client_toolkit::shell::wlr_layer::Anchor, [i32; 4]) {
    use bead_core::Edge;
    use smithay_client_toolkit::shell::wlr_layer::Anchor;
    let anchor = match edge {
        Edge::Left => Anchor::LEFT | Anchor::TOP,
        Edge::Right => Anchor::RIGHT | Anchor::TOP,
        Edge::Top => Anchor::TOP,
        Edge::Bottom => Anchor::BOTTOM,
    };
    let Some(center) = center else {
        return (anchor, [70, 0, 0, 0]);
    };
    let horizontal = matches!(edge, Edge::Top | Edge::Bottom);
    let extent = if horizontal { size.0 } else { size.1 } as f32;
    let span = available.map(|(w, h)| if horizontal { w } else { h });
    let limit = span.map_or(1_000_000., |span| (span as f32 - extent).max(0.));
    let start = (center - extent / 2.).clamp(0., limit).round() as i32;
    match edge {
        Edge::Top => (Anchor::TOP | Anchor::LEFT, [70, 0, 0, start]),
        Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT, [0, 0, 70, start]),
        Edge::Left => (anchor, [start, 0, 0, 70]),
        Edge::Right => (anchor, [start, 70, 0, 0]),
    }
}

/// Keep a return surface at its final clipped extent for the whole motion.
/// Transparent padding may cross an output edge; it must not clamp the body
/// short of its dock and force a resize/jump on the terminal frame.
pub(crate) fn dock_geometry(
    body_offset: (i32, i32),
    extent: (u32, u32),
    output: (i32, i32),
    current: (f64, f64),
    target: (f64, f64),
) -> ((u32, u32), (i32, i32)) {
    let axis = |offset: i32, extent: u32, span: i32, current: f64, target: f64| {
        let target = target.round() as i32 - offset;
        let extent = (span - target).clamp(1, extent.max(1) as i32);
        let origin =
            (current.round() as i32 - offset).clamp(target.min(0), (span - extent).max(target));
        (extent as u32, origin)
    };
    let (width, x) = axis(body_offset.0, extent.0, output.0, current.0, target.0);
    let (height, y) = axis(body_offset.1, extent.1, output.1, current.1, target.1);
    ((width, height), (x, y))
}

#[derive(Clone)]
pub struct Panels {
    default: String,
    views: BTreeMap<String, PanelSpec>,
}
impl Panels {
    pub fn new(default: String, views: BTreeMap<String, PanelSpec>) -> Result<Self, String> {
        if views.is_empty() || views.len() > 32 || !views.contains_key(&default) {
            return Err("panel catalog needs a configured default and at most 32 views".into());
        }
        if views.keys().any(|name| {
            name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.".contains(&c))
        }) {
            return Err("invalid panel view name".into());
        }
        Ok(Self { default, views })
    }
    pub fn default_name(&self) -> &str {
        &self.default
    }
    pub(crate) fn resolve(&self, name: Option<&str>) -> Result<(&str, &PanelSpec), String> {
        let name = name.unwrap_or(&self.default);
        self.views
            .get_key_value(name)
            .map(|(name, spec)| (name.as_str(), spec))
            .ok_or_else(|| "panel view is not configured".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn padded_return_reaches_the_dock_without_a_clamped_plateau_or_final_resize() {
        // Bottom/right docks clip the trailing shadow. Top/left docks allow
        // negative margins for the leading shadow, with no body-size change.
        for (from, target) in [
            ((410., 560.), (410., 600.)),
            ((410., 50.), (410., 10.)),
            ((750., 300.), (810., 300.)),
            ((50., 300.), (10., 300.)),
        ] {
            let mut previous = None;
            let final_geometry = dock_geometry((32, 32), (1164, 654), (1920, 1200), target, target);
            for step in 0..=10 {
                let t = step as f64 / 10.;
                let current = (
                    from.0 + (target.0 - from.0) * t,
                    from.1 + (target.1 - from.1) * t,
                );
                let (extent, origin) =
                    dock_geometry((32, 32), (1164, 654), (1920, 1200), current, target);
                assert_eq!(
                    extent, final_geometry.0,
                    "extent must stay fixed throughout return"
                );
                assert_eq!(
                    origin,
                    (current.0.round() as i32 - 32, current.1.round() as i32 - 32)
                );
                if let Some(previous) = previous {
                    assert_ne!(
                        origin, previous,
                        "return cannot stall before the terminal pose"
                    );
                }
                previous = Some(origin);
            }
            assert_eq!(
                final_geometry.1,
                (target.0 as i32 - 32, target.1 as i32 - 32)
            );
        }
    }

    #[test]
    fn hover_preserves_a_closing_surface_but_can_replace_unused_preparation() {
        assert!(
            !preparation_allowed(false, true, true, true),
            "Closing sound must present before calendar hover can retire it"
        );
        assert!(!preparation_allowed(true, true, true, true));
        assert!(!preparation_allowed(false, false, true, false));
        assert!(preparation_allowed(false, false, true, true));
        assert!(preparation_allowed(false, false, false, true));
    }
    #[test]
    fn popout_centers_follow_each_edge_and_stay_inside_the_output() {
        use bead_core::Edge;
        use smithay_client_toolkit::shell::wlr_layer::Anchor;
        for (edge, anchor, margins) in [
            (Edge::Top, Anchor::TOP | Anchor::LEFT, [70, 0, 0, 800]),
            (Edge::Bottom, Anchor::BOTTOM | Anchor::LEFT, [0, 0, 70, 800]),
            (Edge::Left, Anchor::LEFT | Anchor::TOP, [780, 0, 0, 70]),
            (Edge::Right, Anchor::RIGHT | Anchor::TOP, [780, 70, 0, 0]),
        ] {
            assert_eq!(
                placement(edge, Some(960.), (320, 240), Some((1920, 1020))),
                (anchor, margins)
            );
        }
        assert_eq!(
            placement(Edge::Top, Some(10.), (320, 240), Some((1920, 1020))).1[3],
            0
        );
        assert_eq!(
            placement(Edge::Top, Some(1910.), (320, 240), Some((1920, 1020))).1[3],
            1600
        );
        assert_eq!(
            placement(Edge::Top, Some(960.), (320, 240), Some((200, 1020))).1[3],
            0
        );
        assert_eq!(
            placement(Edge::Top, None, (320, 240), None),
            (Anchor::TOP, [70, 0, 0, 0])
        );
        for n in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.] {
            assert!(validate_center(Some(n)).is_err());
        }
        assert!(validate_center(Some(-20.)).is_ok());
    }
    fn spec(calls: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> PanelSpec {
        PanelSpec {
            position: None,
            engages_decorations: false,
            overlay: false,
            retain_when_closed: false,
            passive: false,
            keyboard_focus: true,
            initial_open: false,
            factory: std::sync::Arc::new(move |_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("factory should stay cold in catalog checks".into())
            }),
            fallback: bead_render::Scene {
                opacity: 1.,
                motion: None,
                resource_revision: 0,
                asset_data: Default::default(),
                clips: Default::default(),
                layers: Vec::new(),
                prepared: Default::default(),
                font_files: Vec::new(),
                width: 1.,
                slide: None,
                primitives: Vec::new(),
                offset: (0., 0.),
                bounds: bead_render::Rect::new(0., 0., 1., 1.),
            },
            renderer: bead_render::RendererKind::Cpu,
        }
    }
    #[test]
    fn overlay_fullscreen_coverage_is_per_view_and_keeps_factories_cold() {
        use smithay_client_toolkit::shell::wlr_layer::Layer;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let catalog = crate::overlay::Overlays::new(BTreeMap::from([
            ("cards".into(), spec(calls.clone())),
            ("level".into(), spec(calls.clone())),
        ]))
        .unwrap();
        assert_eq!(catalog.layer("cards"), Layer::Overlay);
        let catalog = catalog.with_fullscreen_covered(&["cards".into()]).unwrap();
        assert_eq!(catalog.layer("cards"), Layer::Top);
        assert_eq!(catalog.layer("level"), Layer::Overlay);
        assert!(
            catalog
                .with_fullscreen_covered(&["missing".into()])
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn named_lookup_keeps_factories_cold_and_rejects_unknown_views() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let panels = Panels::new(
            "launcher".into(),
            BTreeMap::from([
                ("launcher".into(), spec(calls.clone())),
                ("calendar".into(), spec(calls.clone())),
            ]),
        )
        .unwrap();
        assert_eq!(panels.resolve(None).unwrap().0, "launcher");
        assert_eq!(panels.resolve(Some("calendar")).unwrap().0, "calendar");
        assert!(panels.resolve(Some("unconfigured")).is_err());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "configuration must not start workers"
        );
        assert_eq!(panels.resolve(None).unwrap().0, "launcher");
    }
    #[test]
    fn configuration_bounds_and_default_are_checked_before_use() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for name in ["", "../calendar", "Calendar", "space name"] {
            assert!(
                Panels::new(
                    name.into(),
                    BTreeMap::from([(name.into(), spec(calls.clone()))])
                )
                .is_err()
            );
        }
        assert!(Panels::new("missing".into(), BTreeMap::new()).is_err());
        assert!(
            Panels::new(
                "missing".into(),
                BTreeMap::from([("calendar".into(), spec(calls.clone()))])
            )
            .is_err()
        );
        let too_many = (0..33)
            .map(|i| (format!("view-{i}"), spec(calls.clone())))
            .collect();
        assert!(Panels::new("view-0".into(), too_many).is_err());
    }
}

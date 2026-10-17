// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static view configuration; only the selected factory creates a worker.
use crate::PanelSpec;
use std::collections::BTreeMap;

pub(crate) fn validate_center(center: Option<f32>) -> Result<(), String> {
    if center.is_some_and(|n| !n.is_finite() || n.abs() > 1_000_000.) {
        return Err("invalid panel center".into());
    }
    Ok(())
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
            passive: false,
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

// SPDX-License-Identifier: GPL-3.0-only
//! B0047: artwork for the frame's edge lips, ported from `NacreEdgeTab.qml`. The framework
//! blits the SVG; this is Nacre's look (accent cue that fills while the pointer dwells, a
//! chevron that points into or out of the panel).
use bead::wayland::{EdgeArt, LipState, intent::Lip};
use bead::{core::Edge, render::Palette};
use std::sync::Arc;

fn rgb(c: [u8; 4]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}
/// The tab drawn inside a left or right lip of `width` x `height` (frame width x lip
/// length). Other edges draw nothing, as in the reference (the top lip is invisible).
pub fn edge_tab(
    lip: &Lip,
    (width, height): (f32, f32),
    state: LipState,
    accent: [u8; 4],
    secondary: [u8; 4],
) -> Option<String> {
    let left = match lip.edge {
        Edge::Left => true,
        Edge::Right => false,
        _ => return None,
    };
    let amount = if state.active { 1. } else { state.progress.clamp(0., 1.) };
    let mut body = String::new();
    // The cue: a 1 px line on the outer edge, 36 px tall, filling top-down as the dwell
    // advances; gone once full or while the panel is open.
    if !state.active && amount < 1. && amount > 0. {
        body.push_str(&format!(
            r#"<rect x="{}" y="{}" width="1" height="{}" fill="url(#cue)"/>"#,
            if left { 0. } else { width - 1. },
            (height - 36.) / 2.,
            36. * amount
        ));
    }
    let arrow_w = (width - 2.).min(8.);
    if arrow_w >= 4. {
        // Left: points right while closed, left while open. Right: the opposite.
        let right = if left { !state.active } else { state.active };
        let (x, y) = ((width - arrow_w) / 2., (height - 18.) / 2.);
        let (a, b) = if right { (1., arrow_w - 1.) } else { (arrow_w - 1., 1.) };
        body.push_str(&format!(
            r#"<path d="M{} {} L{} {} L{} {}" fill="none" stroke="{}" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" opacity="{}"/>"#,
            x + a,
            y + 1.,
            x + b,
            y + 9.,
            x + a,
            y + 17.,
            rgb(accent),
            0.75 + 0.25 * amount
        ));
    }
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs><linearGradient id="cue" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs>{body}</svg>"#,
        rgb(accent),
        rgb(secondary)
    ))
}
pub fn art(colors: &Palette) -> Result<EdgeArt, Box<dyn std::error::Error>> {
    let (accent, secondary) = (colors.get("primary")?, colors.get("secondary")?);
    Ok(Arc::new(move |lip, size, state| {
        edge_tab(lip, size, state, accent, secondary).map(|svg| Arc::from(svg.into_bytes()))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lip(edge: Edge) -> Lip {
        Lip {
            id: "l".into(),
            edge,
            length: 144.,
            thickness: 10.,
            opens: "p".into(),
        }
    }
    const A: [u8; 4] = [10, 20, 30, 255];
    const S: [u8; 4] = [40, 50, 60, 255];
    fn idle() -> LipState {
        LipState {
            progress: 0.,
            active: false,
        }
    }
    #[test]
    fn only_side_lips_have_a_tab() {
        assert!(edge_tab(&lip(Edge::Top), (208., 50.), idle(), A, S).is_none());
        assert!(edge_tab(&lip(Edge::Bottom), (208., 10.), idle(), A, S).is_none());
        assert!(edge_tab(&lip(Edge::Left), (10., 144.), idle(), A, S).is_some());
        assert!(edge_tab(&lip(Edge::Right), (10., 144.), idle(), A, S).is_some());
    }
    #[test]
    fn the_chevron_points_into_the_panel_side_and_flips_when_open() {
        // Left, closed: points right (x grows to 8). Open: points left.
        let closed = edge_tab(&lip(Edge::Left), (10., 144.), idle(), A, S).unwrap();
        assert!(closed.contains(r#"d="M2 64 L8 72 L2 80""#), "{closed}");
        let open = LipState {
            progress: 0.,
            active: true,
        };
        let opened = edge_tab(&lip(Edge::Left), (10., 144.), open, A, S).unwrap();
        assert!(opened.contains(r#"d="M8 64 L2 72 L8 80""#), "{opened}");
        // Right is the mirror image.
        let right = edge_tab(&lip(Edge::Right), (10., 144.), idle(), A, S).unwrap();
        assert!(right.contains(r#"d="M8 64 L2 72 L8 80""#), "{right}");
    }
    #[test]
    fn the_cue_fills_with_the_dwell_and_is_gone_when_full_or_open() {
        let at = |progress, active| {
            edge_tab(
                &lip(Edge::Left),
                (10., 144.),
                LipState { progress, active },
                A,
                S,
            )
            .unwrap()
        };
        assert!(!at(0., false).contains("<rect"), "no cue before the dwell");
        let half = at(0.5, false);
        assert!(half.contains(r#"x="0" y="54" width="1" height="18""#), "{half}");
        assert!(at(0.5, false).contains(r#"opacity="0.875""#));
        assert!(!at(1., false).contains("<rect"), "full: the cue fades out");
        assert!(!at(0.5, true).contains("<rect"), "open: no cue");
        let right = edge_tab(
            &lip(Edge::Right),
            (10., 144.),
            LipState {
                progress: 0.5,
                active: false,
            },
            A,
            S,
        )
        .unwrap();
        assert!(right.contains(r#"x="9" y="54""#), "{right}");
    }
    #[test]
    fn a_thin_frame_drops_the_chevron_like_the_reference() {
        // width < 4 after the 2 px margin: QML hides the arrow.
        let thin = edge_tab(&lip(Edge::Left), (5., 144.), idle(), A, S).unwrap();
        assert!(!thin.contains("<path"));
        assert!(edge_tab(&lip(Edge::Left), (6., 144.), idle(), A, S).unwrap().contains("<path"));
    }
}

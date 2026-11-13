// SPDX-License-Identifier: MIT
//! GraphCanvas geometry and its CPU painter (spec 0232, phase 2a). One `Primitive::Graph`
//! references shared, versioned geometry; frames compare `revision`, camera, style and
//! interaction state instead of every edge. Edges are anti-aliased hairlines and nodes
//! are coverage discs written straight into the premultiplied RGBA strip, with viewport
//! culling and edge decimation below the theme's `lod_zoom`.
use crate::{Damage, Rect};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// World-space graph geometry, replaced as a whole whenever anything in it changes.
#[derive(Clone, Debug, Default)]
pub struct GraphGeometry {
    pub revision: u64,
    pub positions: Arc<[[f32; 2]]>,
    pub radii: Arc<[f32]>,
    pub edges: Arc<[[u32; 2]]>,
    /// Per-node highlight mask; empty means no highlight set, so nothing is dimmed.
    pub highlight: Arc<[bool]>,
}
/// Alpha of nodes and edges outside a non-empty highlight set.
pub const DIM_ALPHA: u8 = 70;
/// `screen = rect.center + (world - center) * zoom`, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub center: [f32; 2],
    pub zoom: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [0., 0.],
            zoom: 1.,
        }
    }
}
impl Camera {
    pub fn to_screen(self, rect: Rect, p: [f32; 2]) -> [f32; 2] {
        [
            rect.x + rect.w * 0.5 + (p[0] - self.center[0]) * self.zoom,
            rect.y + rect.h * 0.5 + (p[1] - self.center[1]) * self.zoom,
        ]
    }
    pub fn to_world(self, rect: Rect, s: [f32; 2]) -> [f32; 2] {
        [
            self.center[0] + (s[0] - rect.x - rect.w * 0.5) / self.zoom,
            self.center[1] + (s[1] - rect.y - rect.h * 0.5) / self.zoom,
        ]
    }
}
/// Colours and thresholds resolved from theme tokens by the view layer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphStyle {
    pub node: [u8; 4],
    pub edge: [u8; 4],
    pub hover: [u8; 4],
    pub selected: [u8; 4],
    /// Ring of the keyboard-focused node.
    pub focus: [u8; 4],
    pub label: [u8; 4],
    pub label_size: f32,
    /// From this zoom on, visible nodes get labels by weight until `max_labels`.
    pub label_zoom: f32,
    pub max_labels: u16,
    /// World radius of a weight-1 node.
    pub node_radius: f32,
    /// Below this zoom only every fourth unfocused edge is drawn, at `far_edge_alpha`.
    pub lod_zoom: f32,
    pub far_edge_alpha: u8,
}
impl Default for GraphStyle {
    fn default() -> Self {
        Self {
            node: [95, 150, 210, 255],
            edge: [105, 115, 130, 150],
            hover: [255, 190, 40, 255],
            selected: [255, 255, 255, 255],
            focus: [120, 190, 255, 255],
            label: [220, 220, 220, 255],
            label_size: 13.,
            label_zoom: 1.2,
            max_labels: 32,
            node_radius: 3.,
            lod_zoom: 0.8,
            far_edge_alpha: 85,
        }
    }
}
pub(crate) fn hash_graph(
    geometry: &GraphGeometry,
    camera: &Camera,
    style: &GraphStyle,
    hover: Option<u32>,
    selected: Option<u32>,
    focus: Option<u32>,
    hash: &mut impl Hasher,
) {
    // Independent canvases can have equal revision counters. Include the resource
    // identity, which stays stable across scene clones without walking its edges.
    std::ptr::hash(geometry, hash);
    geometry.revision.hash(hash);
    for v in [
        camera.center[0],
        camera.center[1],
        camera.zoom,
        style.lod_zoom,
        style.label_size,
        style.label_zoom,
        style.node_radius,
    ] {
        v.to_bits().hash(hash);
    }
    (
        style.node,
        style.edge,
        style.hover,
        style.selected,
        style.focus,
        style.label,
        style.max_labels,
        style.far_edge_alpha,
        hover,
        selected,
        focus,
    )
        .hash(hash);
}

/// Local damage for an interaction-only update. Camera/style/geometry changes use
/// the ordinary viewport damage; only changed focus nodes and affected edges are visited.
pub(crate) fn interaction_damage(
    old: &crate::Primitive,
    new: &crate::Primitive,
    scale: f32,
) -> Option<Vec<Rect>> {
    let crate::Primitive::Graph {
        rect,
        geometry,
        camera,
        style,
        hover,
        selected,
        focus,
    } = new
    else {
        return None;
    };
    let crate::Primitive::Graph {
        rect: old_rect,
        geometry: old_geometry,
        camera: old_camera,
        style: old_style,
        hover: old_hover,
        selected: old_selected,
        focus: old_focus,
    } = old
    else {
        return None;
    };
    if rect != old_rect
        || !std::sync::Arc::ptr_eq(geometry, old_geometry)
        || camera != old_camera
        || style != old_style
    {
        return None;
    }
    let mut changed = Vec::new();
    for (a, b) in [
        (hover, old_hover),
        (selected, old_selected),
        (focus, old_focus),
    ] {
        if a != b {
            for id in [*a, *b].into_iter().flatten() {
                if !changed.contains(&id) {
                    changed.push(id);
                }
            }
        }
    }
    let mut regions = Vec::new();
    for &id in &changed {
        let i = id as usize;
        let Some(&p) = geometry.positions.get(i) else {
            continue;
        };
        let p = camera.to_screen(*rect, p);
        let radius = (geometry.radii.get(i).copied().unwrap_or(3.) * camera.zoom * scale)
            .clamp(1., 64.)
            / scale
            + 2.
            + 1. / scale;
        regions.push(Rect::new(
            p[0] - radius,
            p[1] - radius,
            radius * 2.,
            radius * 2.,
        ));
    }
    // Below the LOD zoom a focused node's edges switch between decimated and drawn.
    if camera.zoom < style.lod_zoom {
        for [a, b] in geometry.edges.iter() {
            if !changed.contains(a) && !changed.contains(b) {
                continue;
            }
            let (Some(&a), Some(&b)) = (
                geometry.positions.get(*a as usize),
                geometry.positions.get(*b as usize),
            ) else {
                continue;
            };
            let (a, b) = (camera.to_screen(*rect, a), camera.to_screen(*rect, b));
            let pad = 2. / scale;
            regions.push(Rect::new(
                a[0].min(b[0]) - pad,
                a[1].min(b[1]) - pad,
                (a[0] - b[0]).abs() + 2. * pad,
                (a[1] - b[1]).abs() + 2. * pad,
            ));
        }
    }
    Some(regions)
}

/// A damage strip of premultiplied RGBA pixels: rows `strip_y..` of a `width`-wide target.
pub struct Strip<'a> {
    pub bytes: &'a mut [u8],
    pub width: u32,
    pub strip_y: u32,
    /// Pixels this paint may touch, inside the strip.
    pub clip: Damage,
    pub scale: f32,
    pub offset: (f32, f32),
}
struct Canvas<'a> {
    strip: Strip<'a>,
    // Inclusive pixel bounds.
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}
impl Canvas<'_> {
    #[inline]
    fn blend(&mut self, x: i32, y: i32, color: [u8; 4], coverage: f32) {
        if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
            return;
        }
        let a = f32::from(color[3]) / 255. * coverage;
        if a <= 0. {
            return;
        }
        let i =
            ((y as u32 - self.strip.strip_y) as usize * self.strip.width as usize + x as usize) * 4;
        let px = &mut self.strip.bytes[i..i + 4];
        let inv = 1. - a;
        for c in 0..3 {
            px[c] = (f32::from(color[c]) * a + f32::from(px[c]) * inv + 0.5) as u8;
        }
        px[3] = (255. * a + f32::from(px[3]) * inv + 0.5) as u8;
    }
    /// Wu's anti-aliased hairline, after Liang–Barsky clipping to the canvas.
    fn line(&mut self, mut p0: [f32; 2], mut p1: [f32; 2], color: [u8; 4]) {
        let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
        let (mut t0, mut t1) = (0f32, 1f32);
        let bounds = [
            (-dx, p0[0] - self.x0 as f32),
            (dx, self.x1 as f32 + 1. - p0[0]),
            (-dy, p0[1] - self.y0 as f32),
            (dy, self.y1 as f32 + 1. - p0[1]),
        ];
        for (p, q) in bounds {
            if p == 0. {
                if q < 0. {
                    return;
                }
            } else {
                let t = q / p;
                if p < 0. {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
        if t0 > t1 {
            return;
        }
        (p0, p1) = (
            [p0[0] + dx * t0, p0[1] + dy * t0],
            [p0[0] + dx * t1, p0[1] + dy * t1],
        );
        let steep = (p1[1] - p0[1]).abs() > (p1[0] - p0[0]).abs();
        let swap = |p: [f32; 2]| if steep { [p[1], p[0]] } else { p };
        let (mut a, mut b) = (swap(p0), swap(p1));
        if a[0] > b[0] {
            std::mem::swap(&mut a, &mut b);
        }
        let gradient = if b[0] == a[0] {
            1.
        } else {
            (b[1] - a[1]) / (b[0] - a[0])
        };
        let mut plot = |x: f32, y: f32, c: f32| {
            let (px, py) = if steep { (y, x) } else { (x, y) };
            self.blend(px as i32, py as i32, color, c);
        };
        let mut y = a[1] + gradient * (a[0].round() - a[0]);
        let (x_start, x_end) = (a[0].round() as i32, b[0].round() as i32);
        for x in x_start..=x_end {
            let fy = y.floor();
            let frac = y - fy;
            plot(x as f32, fy, 1. - frac);
            plot(x as f32, fy + 1., frac);
            y += gradient;
        }
    }
    fn disc(&mut self, c: [f32; 2], r: f32, color: [u8; 4]) {
        let x0 = ((c[0] - r - 1.).floor() as i32).max(self.x0);
        let x1 = ((c[0] + r + 1.).ceil() as i32).min(self.x1);
        let y0 = ((c[1] - r - 1.).floor() as i32).max(self.y0);
        let y1 = ((c[1] + r + 1.).ceil() as i32).min(self.y1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - c[0], y as f32 + 0.5 - c[1]);
                let coverage = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
                if coverage > 0. {
                    self.blend(x, y, color, coverage);
                }
            }
        }
    }
}
/// Paint `geometry` through `camera` into `rect`, clipped to the strip's clip and the rect.
#[allow(clippy::too_many_arguments)] // One painter for every interaction state.
pub fn paint(
    strip: Strip<'_>,
    rect: Rect,
    geometry: &GraphGeometry,
    camera: Camera,
    style: &GraphStyle,
    hover: Option<u32>,
    selected: Option<u32>,
    focus: Option<u32>,
) {
    let scale = strip.scale;
    let (ox, oy) = strip.offset;
    let left = ((rect.x + ox) * scale).floor() as i32;
    let top = ((rect.y + oy) * scale).floor() as i32;
    let right = ((rect.x + ox + rect.w) * scale).ceil() as i32 - 1;
    let bottom = ((rect.y + oy + rect.h) * scale).ceil() as i32 - 1;
    let clip = strip.clip;
    let mut canvas = Canvas {
        x0: left.max(clip.x as i32),
        y0: top.max(clip.y as i32),
        x1: right.min((clip.x + clip.w) as i32 - 1),
        y1: bottom.min((clip.y + clip.h) as i32 - 1),
        strip,
    };
    if canvas.x1 < canvas.x0 || canvas.y1 < canvas.y0 || !camera.zoom.is_finite() {
        return;
    }
    let a = camera.zoom * scale;
    let bx = (rect.x + ox + rect.w * 0.5) * scale - camera.center[0] * a;
    let by = (rect.y + oy + rect.h * 0.5) * scale - camera.center[1] * a;
    let device = |p: [f32; 2]| [p[0] * a + bx, p[1] * a + by];
    let n = geometry.positions.len();
    let far = camera.zoom < style.lod_zoom;
    let dimmed = !geometry.highlight.is_empty();
    let focused = |i: usize| {
        [hover, selected, focus].contains(&Some(i as u32))
            || geometry.highlight.get(i).copied().unwrap_or(false)
    };
    let dim = |mut color: [u8; 4], focus: bool| {
        if dimmed && !focus {
            color[3] = (u16::from(color[3]) * u16::from(DIM_ALPHA) / 255) as u8;
        }
        color
    };
    let (fx0, fy0, fx1, fy1) = (
        canvas.x0 as f32 - 1.,
        canvas.y0 as f32 - 1.,
        canvas.x1 as f32 + 2.,
        canvas.y1 as f32 + 2.,
    );
    for (i, [s, t]) in geometry.edges.iter().enumerate() {
        let (s, t) = (*s as usize, *t as usize);
        if s >= n || t >= n {
            continue;
        }
        let focus = focused(s) || focused(t);
        if far && !focus && i % 4 != 0 {
            continue;
        }
        let (p, q) = (device(geometry.positions[s]), device(geometry.positions[t]));
        if p[0].max(q[0]) < fx0
            || p[0].min(q[0]) > fx1
            || p[1].max(q[1]) < fy0
            || p[1].min(q[1]) > fy1
        {
            continue;
        }
        let mut color = style.edge;
        if far {
            color[3] = style.far_edge_alpha;
        }
        let color = dim(color, focus);
        if color[3] > 0 {
            canvas.line(p, q, color);
        }
    }
    for i in 0..n {
        let p = device(geometry.positions[i]);
        let r = (geometry.radii.get(i).copied().unwrap_or(3.) * a).clamp(1., 64.);
        let ring = r + 2. * scale;
        if p[0] + ring < fx0 || p[0] - ring > fx1 || p[1] + ring < fy0 || p[1] - ring > fy1 {
            continue;
        }
        if Some(i as u32) == hover {
            canvas.disc(p, ring, style.hover);
        } else if Some(i as u32) == selected {
            canvas.disc(p, ring, style.selected);
        } else if Some(i as u32) == focus {
            canvas.disc(p, ring, style.focus);
        }
        canvas.disc(p, r, dim(style.node, focused(i)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelFormat, Primitive, ShmTarget};
    use std::sync::Arc;

    fn seeded(n: usize, m: usize) -> GraphGeometry {
        let mut state = 0xb0232_u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 32) as u32) as f32 / u32::MAX as f32
        };
        GraphGeometry {
            revision: 1,
            positions: (0..n)
                .map(|_| [next() * 1480. - 740., next() * 1000. - 500.])
                .collect::<Vec<_>>()
                .into(),
            radii: vec![3.; n].into(),
            edges: (0..m)
                .map(|_| {
                    [
                        (next() * n as f32) as u32 % n as u32,
                        (next() * n as f32) as u32 % n as u32,
                    ]
                })
                .collect::<Vec<_>>()
                .into(),
            ..Default::default()
        }
    }
    fn strip(w: u32, h: u32) -> Vec<u8> {
        vec![0; (w * h * 4) as usize]
    }
    fn paint_into(
        bytes: &mut [u8],
        w: u32,
        h: u32,
        geometry: &GraphGeometry,
        camera: Camera,
        hover: Option<u32>,
    ) {
        paint(
            Strip {
                bytes,
                width: w,
                strip_y: 0,
                clip: Damage::full(w, h),
                scale: 1.,
                offset: (0., 0.),
            },
            Rect::new(0., 0., w as f32, h as f32),
            geometry,
            camera,
            &GraphStyle::default(),
            hover,
            None,
            None,
        );
    }
    fn alpha_sum(bytes: &[u8]) -> u64 {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u64::from(p[3]))
            .sum()
    }

    #[test]
    fn paints_inside_the_rect_and_marks_hover() {
        let geometry = GraphGeometry {
            revision: 1,
            positions: Arc::from([[-10., 0.], [10., 0.], [500., 500.]]),
            radii: Arc::from([3.; 3]),
            edges: Arc::from([[0, 1], [1, 2]]),
            ..Default::default()
        };
        let (w, h) = (64, 64);
        let mut plain = strip(w, h);
        paint_into(&mut plain, w, h, &geometry, Camera::default(), None);
        assert!(alpha_sum(&plain) > 0);
        // Nothing lands outside the rect, even with an edge leaving it.
        let mut bytes = strip(w, h);
        paint(
            Strip {
                bytes: &mut bytes,
                width: w,
                strip_y: 0,
                clip: Damage::full(w, h),
                scale: 1.,
                offset: (0., 0.),
            },
            Rect::new(8., 8., 16., 16.),
            &geometry,
            Camera::default(),
            &GraphStyle::default(),
            None,
            None,
            None,
        );
        for y in 0..h {
            for x in 0..w {
                let a = bytes[((y * w + x) * 4 + 3) as usize];
                assert!(
                    (8..24).contains(&x) && (8..24).contains(&y) || a == 0,
                    "pixel {x},{y} painted outside the rect"
                );
            }
        }
        let mut hovered = strip(w, h);
        paint_into(&mut hovered, w, h, &geometry, Camera::default(), Some(0));
        assert!(
            alpha_sum(&hovered) > alpha_sum(&plain),
            "hover ring adds coverage"
        );
    }
    #[test]
    fn highlight_keeps_its_nodes_and_edges_and_dims_the_rest() {
        // Node 0 is lit; edge 0-1 touches it, edge 2-3 does not.
        let positions: Arc<[[f32; 2]]> =
            Arc::from([[-20., -10.], [20., -10.], [-20., 10.], [20., 10.]]);
        let plain = GraphGeometry {
            revision: 1,
            positions,
            radii: Arc::from([3.; 4]),
            edges: Arc::from([[0, 1], [2, 3]]),
            ..Default::default()
        };
        let lit = GraphGeometry {
            highlight: Arc::from([true, false, false, false]),
            ..plain.clone()
        };
        let (w, h) = (64, 64);
        let (mut a, mut b) = (strip(w, h), strip(w, h));
        paint_into(&mut a, w, h, &plain, Camera::default(), None);
        paint_into(&mut b, w, h, &lit, Camera::default(), None);
        let alpha = |p: &[u8], x: u32, y: u32| p[((y * w + x) * 4 + 3) as usize];
        // Node centres sit at (12, 22), (52, 22), (12, 42), (52, 42); edge midpoints at x 32.
        assert_eq!(alpha(&b, 12, 22), alpha(&a, 12, 22), "lit node");
        assert_eq!(alpha(&b, 32, 22), alpha(&a, 32, 22), "edge of the lit node");
        assert!(
            alpha(&a, 52, 22) > 0 && alpha(&b, 52, 22) < alpha(&a, 52, 22),
            "neighbour dims"
        );
        assert!(alpha(&b, 32, 42) < alpha(&a, 32, 42), "unrelated edge dims");
        assert!(alpha(&b, 12, 42) < alpha(&a, 12, 42), "unrelated node dims");
    }
    #[test]
    fn far_zoom_decimates_edges() {
        let geometry = seeded(400, 2000);
        let (w, h) = (256, 256);
        let near = Camera {
            center: [0., 0.],
            zoom: 1.,
        };
        let far = Camera {
            center: [0., 0.],
            zoom: 0.5,
        };
        let mut a = strip(w, h);
        paint_into(&mut a, w, h, &geometry, near, None);
        let mut b = strip(w, h);
        paint_into(&mut b, w, h, &geometry, far, None);
        assert!(alpha_sum(&b) < alpha_sum(&a));
    }

    #[test]
    fn interaction_damage_matches_full_repaint() {
        use crate::tree::{Frame, Item, paint};
        let geometry = Arc::new(GraphGeometry {
            positions: Arc::from([[-30., -10.], [20., 10.], [50., -20.]]),
            radii: Arc::from([3.; 3]),
            edges: Arc::from([[0, 1], [1, 2]]),
            ..Default::default()
        });
        for scale in [1., 1.5] {
            for zoom in [0.5, 1.] {
                let (w, h) = (240, 160);
                let frame = |hover, selected| Frame {
                    native_resource_budget: None,
                    _assets: vec![],
                    scale,
                    key: 0,
                    layer_count: 0,
                    layers: vec![],
                    _accounting: None,
                    items: vec![Item {
                        image: None,
                        id: "graph".into(),
                        clip: None,
                        raster: None,
                        primitive: Primitive::Graph {
                            rect: Rect::new(0., 0., 160., 100.),
                            geometry: geometry.clone(),
                            camera: Camera {
                                center: [0., 0.],
                                zoom,
                            },
                            style: GraphStyle::default(),
                            hover,
                            selected,
                            focus: selected,
                        },
                    }],
                };
                let old = frame(Some(0), None);
                let new = frame(Some(1), Some(2));
                let mut tracker = crate::damage::FrameDamage::default();
                tracker.update(&old, (0., 0.), 1., w, h).unwrap();
                let dirty = tracker.update(&new, (0., 0.), 1., w, h).unwrap();
                assert!(dirty.iter().map(|r| r.w * r.h).sum::<u32>() < w * h / 2);
                let mut partial = vec![0; (w * h * 4) as usize];
                let mut full = partial.clone();
                let draw = |frame: &Frame, bytes: &mut [u8], damage| {
                    paint(
                        frame,
                        (0., 0.),
                        ShmTarget {
                            bytes,
                            width: w,
                            height: h,
                            scale,
                            format: PixelFormat::Rgba,
                        },
                        damage,
                    )
                    .unwrap();
                };
                draw(&old, &mut partial, Damage::full(w, h));
                for region in dirty {
                    draw(&new, &mut partial, region);
                }
                draw(&new, &mut full, Damage::full(w, h));
                assert_eq!(partial, full, "scale={scale} zoom={zoom}");
            }
        }
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Backend-neutral drawing commands and a CPU renderer over borrowed SHM memory.
use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    error::Error,
    path::Path,
    time::Instant,
};
use tiny_skia::{
    FillRule, FilterQuality, Mask, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke,
    Transform,
};
pub mod damage;
#[cfg(feature = "native-gl")]
pub mod gl;
#[cfg(feature = "wgpu")]
pub mod gpu;
pub mod pointer_paint;
pub mod retained;
pub mod tree;
fn normal_weight() -> u16 {
    400
}
pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    /// Rectangular bounds and clip, optionally restricted by a normalized even-odd polygon.
    pub fn hit_test(
        self,
        x: f32,
        y: f32,
        clip: Option<Self>,
        polygon: Option<&[[f32; 2]]>,
    ) -> bool {
        let inside_rect = |r: Rect| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;
        if !inside_rect(self) || clip.is_some_and(|clip| !inside_rect(clip)) {
            return false;
        }
        let Some(points) = polygon else {
            return true;
        };
        if points.len() < 3 {
            return false;
        }
        let (px, py) = ((x - self.x) / self.w, (y - self.y) / self.h);
        let mut inside = false;
        let mut previous = points.len() - 1;
        for current in 0..points.len() {
            let [xi, yi] = points[current];
            let [xj, yj] = points[previous];
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            previous = current;
        }
        inside
    }
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            (self.x + self.w).max(other.x + other.w) - x,
            (self.y + self.h).max(other.y + other.h) - y,
        )
    }
    pub fn intersect(self, o: Self) -> Self {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        Self::new(
            x,
            y,
            ((self.x + self.w).min(o.x + o.w) - x).max(0.),
            ((self.y + self.h).min(o.y + o.h) - y).max(0.),
        )
    }
    pub fn translated(self, x: f32, y: f32) -> Self {
        Self::new(self.x + x, self.y + y, self.w, self.h)
    }
    pub fn pixels(self, scale: f32, width: u32, height: u32) -> Damage {
        let x = (self.x * scale).floor().max(0.).min(width as f32) as u32;
        let y = (self.y * scale).floor().max(0.).min(height as f32) as u32;
        let right = ((self.x + self.w) * scale).ceil().max(0.).min(width as f32) as u32;
        let bottom = ((self.y + self.h) * scale)
            .ceil()
            .max(0.)
            .min(height as f32) as u32;
        Damage {
            x,
            y,
            w: right.saturating_sub(x),
            h: bottom.saturating_sub(y),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
impl Damage {
    pub fn full(w: u32, h: u32) -> Self {
        Self { x: 0, y: 0, w, h }
    }
    pub fn union(self, b: Self) -> Self {
        if self.w == 0 || self.h == 0 {
            return b;
        }
        if b.w == 0 || b.h == 0 {
            return self;
        }
        let x = self.x.min(b.x);
        let y = self.y.min(b.y);
        Self {
            x,
            y,
            w: (self.x + self.w).max(b.x + b.w) - x,
            h: (self.y + self.h).max(b.y + b.h) - y,
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    /// A stroked elliptical arc. Angles are degrees clockwise from the rightmost point.
    /// `rect` is the outer paint bound; the path is inset by half the stroke width.
    Arc {
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        stroke_width: f32,
        color: [u8; 4],
        round_caps: bool,
    },
    /// A bounded normalized polyline, clipped to its viewport.
    Polyline {
        rect: Rect,
        points: Vec<[f32; 2]>,
        closed: bool,
        stroke_width: f32,
        color: [u8; 4],
        round_caps: bool,
        fill: Option<[u8; 4]>,
    },
    Rounded {
        rect: Rect,
        radius: f32,
        fill: [u8; 4],
        border: Option<([u8; 4], f32)>,
    },
    Text {
        #[serde(default = "normal_weight")]
        weight: u16,
        #[serde(default)]
        align: Option<TextAlign>,
        rect: Rect,
        text: String,
        family: String,
        size: f32,
        color: [u8; 4],
    },
    Image {
        #[serde(default)]
        blur: f32,
        #[serde(default)]
        fit: ImageFit,
        #[serde(default)]
        clip_polygon: Option<Vec<[f32; 2]>>,
        rect: Rect,
        path: std::path::PathBuf,
    },
    Shadow {
        rect: Rect,
        radius: f32,
        blur: f32,
    },
}
impl Primitive {
    fn translated(&self, x: f32, y: f32) -> Self {
        let mut primitive = self.clone();
        let (Self::Arc { rect, .. }
        | Self::Polyline { rect, .. }
        | Self::Rounded { rect, .. }
        | Self::Text { rect, .. }
        | Self::Image { rect, .. }
        | Self::Shadow { rect, .. }) = &mut primitive;
        *rect = rect.translated(x, y);
        primitive
    }
}
pub(crate) fn polyline_points(
    rect: Rect,
    stroke: f32,
    points: &[[f32; 2]],
) -> Option<Vec<(f32, f32)>> {
    if ![rect.x, rect.y, rect.w, rect.h, stroke]
        .iter()
        .all(|v| v.is_finite())
        || rect.w <= 0.
        || rect.h <= 0.
        || stroke < 0.
        || stroke > rect.w.min(rect.h)
        || points.len() > 256
        || points
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        || points.len() < 2
    {
        return None;
    }
    let inset = stroke * 0.5;
    let (w, h) = (rect.w - stroke, rect.h - stroke);
    if w <= 0. || h <= 0. {
        return None;
    }
    Some(
        points
            .iter()
            .map(|p| (rect.x + inset + p[0] * w, rect.y + inset + p[1] * h))
            .collect(),
    )
}
/// Build a directly painted arc path. Segment density is fixed in angular space, so
/// dynamic progress changes never create retained raster entries or asset data.
pub(crate) fn arc_points(
    rect: Rect,
    stroke_width: f32,
    start: f32,
    sweep: f32,
) -> Option<Vec<(f32, f32)>> {
    if ![rect.x, rect.y, rect.w, rect.h, stroke_width, start, sweep]
        .iter()
        .all(|v| v.is_finite())
        || rect.w <= 0.
        || rect.h <= 0.
        || stroke_width <= 0.
        || !(-360. ..=360.).contains(&start)
        || !(-360. ..=360.).contains(&sweep)
        || sweep == 0.
    {
        return None;
    }
    let rx = rect.w * 0.5 - stroke_width * 0.5;
    let ry = rect.h * 0.5 - stroke_width * 0.5;
    if rx <= 0. || ry <= 0. {
        return None;
    }
    let count = (sweep.abs() / 3.).ceil().max(1.) as usize;
    let points = (0..=count)
        .map(|i| {
            let a = (start + sweep * i as f32 / count as f32).to_radians();
            (
                rect.x + rect.w * 0.5 + rx * a.cos(),
                rect.y + rect.h * 0.5 + ry * a.sin(),
            )
        })
        .collect();
    Some(points)
}
#[cfg(test)]
mod arc_tests {
    use super::{Rect, arc_points};

    #[test]
    fn empty_half_full_and_negative_sweeps_follow_clockwise_degrees() {
        let rect = Rect::new(0., 0., 100., 100.);
        assert!(arc_points(rect, 8., 0., 0.).is_none());
        let half = arc_points(rect, 8., 0., 180.).unwrap();
        assert!((half.first().unwrap().0 - 96.).abs() < 0.01);
        assert!((half.last().unwrap().0 - 4.).abs() < 0.01);
        assert!(half[half.len() / 2].1 > 49.);
        let negative = arc_points(rect, 8., 0., -90.).unwrap();
        assert!(negative[negative.len() / 2].1 < 50.);
        let full = arc_points(rect, 8., -360., 360.).unwrap();
        let (first, last) = (full.first().unwrap(), full.last().unwrap());
        assert!((first.0 - last.0).abs() < 0.01 && (first.1 - last.1).abs() < 0.01);
        assert_eq!(full.len(), 121);
    }

    #[test]
    fn invalid_ranges_and_strokes_are_empty() {
        let rect = Rect::new(0., 0., 20., 20.);
        assert!(arc_points(rect, 20., 0., 90.).is_none());
        assert!(arc_points(rect, 2., 361., 90.).is_none());
        assert!(arc_points(rect, f32::NAN, 0., 90.).is_none());
    }
}
#[derive(Clone, Copy, Debug, Default, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    #[default]
    Fill,
    Cover,
}
#[derive(Clone, Copy, Debug, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}
/// Conservative retained primitive heap and metadata bytes, without raster payloads.
pub fn primitive_bytes(p: &Primitive) -> usize {
    std::mem::size_of::<Primitive>()
        + match p {
            Primitive::Text { text, family, .. } => text.capacity() + family.capacity(),
            Primitive::Polyline { points, .. } => {
                points.capacity() * std::mem::size_of::<[f32; 2]>()
            }
            Primitive::Image {
                path, clip_polygon, ..
            } => {
                path.as_os_str().len()
                    + clip_polygon.as_ref().map_or(0, |points| {
                        points.capacity() * std::mem::size_of::<[f32; 2]>()
                    })
            }
            _ => 0,
        }
}
/// Positions are logical pixels. Resources stay independent of surface buffers.
#[derive(Clone, Copy)]
pub struct Slide {
    pub closed_y: f32,
    pub open_y: f32,
    pub period_seconds: f32,
}
/// Content-sized artwork placement and its vector payload.
pub type RevealFrame = (Rect, std::sync::Arc<[u8]>);

/// Application-supplied vector artwork for a revealing body. The callback receives
/// the revealed body and returns its content-sized artwork rectangle and SVG bytes.
#[derive(Clone)]
pub struct RevealArt {
    pub body: Rect,
    pub index: usize,
    /// `draw(revealed, crop)`: artwork for the revealed body; with `crop` (scene coordinates) exactly that
    /// rectangle is drawn (the pre-rasterised edge template), else the closure picks a trimmed crop.
    pub draw: std::sync::Arc<dyn Fn(Rect, Option<Rect>) -> RevealFrame + Send + Sync>,
}
#[derive(Clone)]
pub struct Motion {
    pub id: String,
    pub open: bool,
    pub duration: f64,
    pub spring: bool,
    /// Explicit easing overrides the legacy spring/ease-out selection.
    pub curve: Option<bead_core::animation::Curve>,
    pub edge: bead_core::Edge,
    /// Slide extent; with `reveal`, only content slides behind the growing body's clip.
    pub slide_distance: Option<f32>,
    /// The scene fades in with the slide (default); false keeps it opaque while it moves (the panel grows out of its edge).
    pub fade: bool,
    /// The panel stays at its final position while its body grows out of `edge`: the scene is
    /// clipped to the revealed part (`progress` of the body extent) instead of sliding.
    pub reveal: bool,
    /// Eased open progress, written by the presenter every frame (1 = fully open).
    pub progress: f32,
    /// The primitives that make the panel body (its shadow and fill) with their open-state shapes, found on the
    /// first revealing frame: they shrink with `progress` so the growing edge keeps its rounded corners and shadow.
    pub parts: Vec<(usize, Primitive)>,
    pub art: Option<RevealArt>,
}
impl Motion {
    /// Scene opacity at a progress: the progress itself when fading, else opaque once any of it is shown.
    pub fn opacity(&self, progress: f32) -> f32 {
        if self.fade || progress <= 0.001 {
            progress
        } else {
            1.
        }
    }
}
/// Retained logical ownership; own primitives precede the ordered child layers.
#[derive(Clone, Debug)]
pub struct SceneLayer {
    pub id: String,
    pub parent: Option<String>,
    pub own: std::ops::Range<usize>,
    pub subtree_end: usize,
    pub bounds: Rect,
}
#[derive(Clone)]
pub struct Scene {
    pub opacity: f32,
    pub motion: Option<Motion>,
    pub resource_revision: u64,
    pub asset_data: std::collections::BTreeMap<std::path::PathBuf, std::sync::Arc<[u8]>>,
    pub clips: std::collections::BTreeMap<usize, Rect>,
    pub layers: Vec<SceneLayer>,
    pub prepared: std::collections::BTreeMap<u32, std::sync::Arc<tree::Frame>>,
    pub font_files: Vec<std::path::PathBuf>,
    pub width: f32,
    pub slide: Option<Slide>,
    pub primitives: Vec<Primitive>,
    pub offset: (f32, f32),
    pub bounds: Rect,
}
impl Scene {
    /// Remove resources while preserving primitive-indexed clips, layers and reveal metadata.
    pub fn retain_primitives(&mut self, mut keep: impl FnMut(&Primitive) -> bool) {
        let Some(first_removed) = self
            .primitives
            .iter()
            .position(|primitive| !keep(primitive))
        else {
            return;
        };
        let mut positions = Vec::with_capacity(self.primitives.len() + 1);
        positions.extend(0..=first_removed);
        positions.push(first_removed);
        for primitive in self.primitives.iter().skip(first_removed + 1) {
            positions.push(positions.last().copied().unwrap() + usize::from(keep(primitive)));
        }
        let retained = |index: usize| {
            positions
                .get(index)
                .zip(positions.get(index + 1))
                .is_some_and(|(before, after)| after > before)
        };
        let remap = |index: usize| positions.get(index).copied().unwrap_or(index);
        let mut index = 0;
        self.primitives.retain(|_| {
            let keep = retained(index);
            index += 1;
            keep
        });
        self.clips = std::mem::take(&mut self.clips)
            .into_iter()
            .filter(|(index, _)| retained(*index))
            .map(|(index, clip)| (remap(index), clip))
            .collect();
        for layer in &mut self.layers {
            layer.own = remap(layer.own.start)..remap(layer.own.end);
            layer.subtree_end = remap(layer.subtree_end);
        }
        if let Some(motion) = &mut self.motion {
            motion.parts.retain_mut(|(index, _)| {
                if !retained(*index) {
                    return false;
                }
                *index = remap(*index);
                true
            });
            if let Some(art) = &mut motion.art {
                if retained(art.index) {
                    art.index = remap(art.index);
                } else {
                    motion.art = None;
                }
            }
        }
        self.prepared.clear();
    }

    /// Content can slide behind a stationary reveal contour without moving the edge join.
    fn content_offset(&self, index: usize) -> (f32, f32) {
        self.motion
            .as_ref()
            .filter(|m| m.reveal && !m.parts.iter().any(|(i, _)| *i == index))
            .and_then(|m| {
                m.slide_distance
                    .map(|distance| m.edge.closed_offset(distance, distance, m.progress))
            })
            .unwrap_or((0., 0.))
    }
    /// The clip for a primitive: its own (scroll viewport) intersected with the reveal window. The body parts
    /// (shadow and fill) are not clipped: `reveal` reshapes them instead.
    pub fn clip_at(&self, index: usize) -> Option<Rect> {
        let (dx, dy) = self.content_offset(index);
        let own = self.clips.get(&index).map(|clip| clip.translated(dx, dy));
        let window = match &self.motion {
            Some(m) if m.parts.iter().any(|(i, _)| *i == index) => None,
            _ => self.reveal_window(),
        };
        match (own, window) {
            (Some(a), Some(b)) => Some(a.intersect(b)),
            (a, b) => a.or(b),
        }
    }
    /// The open-state body of a revealing panel: the scene bounds less the 32 px shadow margin the presenter
    /// offsets by; else the first shadow (30 px outside its body) or the largest fill.
    fn reveal_body(&self) -> Option<Rect> {
        let from_parts = self.motion.as_ref().and_then(|m| {
            m.parts.iter().find_map(|(_, p)| match p {
                Primitive::Rounded { rect, .. } => Some(*rect),
                _ => None,
            })
        });
        self.motion
            .as_ref()
            .and_then(|m| m.art.as_ref().map(|a| a.body))
            .or(from_parts)
            .or_else(|| {
                // Panel scenes carry the presenter's 32 px shadow margin around the body (see `offset`).
                (self.bounds.w > 64. && self.bounds.h > 64.).then(|| {
                    Rect::new(
                        self.bounds.x + 32.,
                        self.bounds.y + 32.,
                        self.bounds.w - 64.,
                        self.bounds.h - 64.,
                    )
                })
            })
            .or_else(|| {
                self.primitives.iter().find_map(|p| match p {
                    Primitive::Shadow { rect, .. } if rect.w > 60. && rect.h > 60. => Some(
                        Rect::new(rect.x + 30., rect.y + 30., rect.w - 60., rect.h - 60.),
                    ),
                    _ => None,
                })
            })
            .or_else(|| {
                self.primitives
                    .iter()
                    .filter_map(|p| match p {
                        Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                            Some(*rect)
                        }
                        _ => None,
                    })
                    .max_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            })
    }
    /// B0067: set the eased progress of a revealing panel and reshape its body: the fill and its shadow cover only
    /// the revealed part of the body (rounded on the growing edge, shadow following), while the content stays put
    /// under `reveal_window`. Idempotent: the open-state shapes are kept in `Motion::parts`.
    pub fn reveal(&mut self, progress: f32) {
        let Some(edge) = self.motion.as_ref().filter(|m| m.reveal).map(|m| m.edge) else {
            return;
        };
        if self.motion.as_ref().is_some_and(|m| m.parts.is_empty())
            && let Some(body) = self.reveal_body()
        {
            let near = |a: Rect, b: Rect| {
                (a.x - b.x).abs() < 0.01
                    && (a.y - b.y).abs() < 0.01
                    && (a.w - b.w).abs() < 0.01
                    && (a.h - b.h).abs() < 0.01
            };
            let grown = Rect::new(body.x - 30., body.y - 30., body.w + 60., body.h + 60.);
            let mut shadowed = false;
            let parts: Vec<(usize, Primitive)> = self
                .primitives
                .iter()
                .enumerate()
                .filter(|(_, p)| match p {
                    Primitive::Rounded { rect, .. } => near(*rect, body),
                    // Only the panel's own (first) shadow follows the body.
                    Primitive::Shadow { rect, .. } if !shadowed && near(*rect, grown) => {
                        shadowed = true;
                        true
                    }
                    _ => false,
                })
                .map(|(i, p)| (i, p.clone()))
                .collect();
            if let Some(m) = &mut self.motion {
                m.parts = parts;
            }
        }
        let Some(body) = self.reveal_body() else {
            return;
        };
        let now = Self::revealed(body, edge, progress);
        if let Some(art) = self.motion.as_ref().and_then(|m| m.art.clone()) {
            if progress >= 1. {
                let motion = self.motion.as_mut().unwrap();
                for (index, original) in &motion.parts {
                    if let Some(slot) = self.primitives.get_mut(*index) {
                        *slot = original.clone();
                    }
                }
                motion.progress = 1.;
                self.asset_data
                    .retain(|p, _| !p.to_string_lossy().starts_with("@reveal-art-"));
                return;
            }
            if self.motion.as_ref().is_some_and(|m| m.progress == progress) {
                return;
            }
            use std::hash::{Hash, Hasher};
            let draw_profile = bead_core::profile::Span::new("reveal_art_draw");
            let (rect, bytes) = (art.draw)(now, None);
            drop(draw_profile);
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hash);
            let path = std::path::PathBuf::from(format!("@reveal-art-{:x}.svg", hash.finish()));
            self.asset_data
                .retain(|p, _| !p.to_string_lossy().starts_with("@reveal-art-"));
            self.asset_data.insert(path.clone(), bytes);
            if let Some(Primitive::Image {
                path: target,
                rect: target_rect,
                ..
            }) = self.primitives.get_mut(art.index)
            {
                *target_rect = rect;
                *target = path;
            }
            if let Some(m) = &mut self.motion {
                m.progress = progress;
            }
            return;
        }
        let Some(m) = &mut self.motion else { return };
        m.progress = progress;
        for (index, original) in &m.parts {
            let shaped = match original {
                Primitive::Rounded {
                    radius,
                    fill,
                    border,
                    ..
                } => Primitive::Rounded {
                    rect: now,
                    radius: *radius,
                    fill: *fill,
                    border: *border,
                },
                Primitive::Shadow { radius, blur, .. } => Primitive::Shadow {
                    rect: Rect::new(now.x - 30., now.y - 30., now.w + 60., now.h + 60.),
                    radius: *radius,
                    blur: *blur,
                },
                other => other.clone(),
            };
            if let Some(slot) = self.primitives.get_mut(*index) {
                *slot = shaped;
            }
        }
    }
    /// The part of `body` that is open at `progress`, growing out of `edge`.
    pub(crate) fn revealed(body: Rect, edge: bead_core::Edge, progress: f32) -> Rect {
        let p = progress.clamp(0., 1.);
        let (ix, iy) = edge.inward();
        if iy > 0. {
            Rect::new(body.x, body.y, body.w, body.h * p)
        } else if iy < 0. {
            Rect::new(body.x, body.y + body.h * (1. - p), body.w, body.h * p)
        } else if ix > 0. {
            Rect::new(body.x, body.y, body.w * p, body.h)
        } else {
            Rect::new(body.x + body.w * (1. - p), body.y, body.w * p, body.h)
        }
    }
    /// While a revealing panel is partly open: the visible window over its body (generous on
    /// the other sides so the shadow margin stays).
    pub fn reveal_window(&self) -> Option<Rect> {
        let m = self
            .motion
            .as_ref()
            .filter(|m| m.reveal && m.progress < 1.)?;
        let body = self.reveal_body()?;
        let p = m.progress.clamp(0., 1.);
        const M: f32 = 4096.;
        let (ix, iy) = m.edge.inward();
        Some(if iy > 0. {
            Rect::new(body.x - M, body.y - M, body.w + 2. * M, M + body.h * p)
        } else if iy < 0. {
            let top = body.y + body.h * (1. - p);
            Rect::new(body.x - M, top, body.w + 2. * M, body.y + body.h + M - top)
        } else if ix > 0. {
            Rect::new(body.x - M, body.y - M, M + body.w * p, body.h + 2. * M)
        } else {
            let left = body.x + body.w * (1. - p);
            Rect::new(
                left,
                body.y - M,
                body.x + body.w + M - left,
                body.h + 2. * M,
            )
        })
    }
    pub fn bounds(&self) -> Rect {
        self.bounds.translated(self.offset.0, self.offset.1)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba,
    Bgra,
}
/// CPU target is borrowed compositor memory, never a per-frame intermediate image.
/// A future GPU backend can add its own target while retaining the same Scene.
pub struct ShmTarget<'a> {
    pub bytes: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub format: PixelFormat,
}
pub enum RenderTarget<'a> {
    Shm(ShmTarget<'a>),
    #[cfg(feature = "wgpu")]
    Gpu(gpu::GpuTarget<'a>),
    #[cfg(feature = "native-gl")]
    Gl(gl::GlTarget<'a>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RendererKind {
    #[default]
    Cpu,
    Femtovg,
    FemtovgGl,
    Vello,
}
pub fn renderer(kind: RendererKind) -> Result<Box<dyn Renderer>> {
    match kind {
        RendererKind::Cpu => Ok(Box::<tree::TreeCpuRenderer>::default()),
        #[cfg(feature = "native-gl")]
        RendererKind::FemtovgGl => Ok(Box::<gl::GlRenderer>::default()),
        #[cfg(feature = "wgpu")]
        RendererKind::Femtovg | RendererKind::Vello => Ok(Box::new(gpu::GpuRenderer::new(kind))),
        #[allow(unreachable_patterns)]
        _ => Err("GPU support disabled; use the tiny-skia fallback".into()),
    }
}
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
pub enum ResourceTarget<'a> {
    #[cfg(feature = "wgpu")]
    Wgpu {
        device: &'a gpu::wgpu::Device,
        queue: &'a gpu::wgpu::Queue,
    },
    #[cfg(feature = "native-gl")]
    Gl {
        display: &'a glutin::display::Display,
    },
}
pub trait Renderer {
    /// Arm one transient-resource timer after rendering starts using scratch.
    fn take_transient_timer(&mut self) -> bool {
        false
    }
    /// Release quiet scratch, or return the delay until the next idle check.
    fn trim_transient_idle(&mut self) -> Option<std::time::Duration> {
        None
    }

    /// Per-surface presentation hint, not global activity on another surface.
    fn presentation_motion(&mut self, _active: bool) {}
    fn prepared_frame(&self) -> Option<std::sync::Arc<tree::Frame>> {
        None
    }
    fn resource_counts(&self) -> (usize, usize) {
        (0, 0)
    }
    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    fn stage_resources(&mut self, _: ResourceTarget<'_>) -> Result<()> {
        Ok(())
    }

    fn wants_gpu(&self) -> bool {
        false
    }
    fn draw(&mut self, scene: &Scene, target: RenderTarget<'_>, damage: Damage) -> Result<()> {
        match target {
            RenderTarget::Shm(t) => self.render(scene, t, damage),
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            _ => Err("GPU target unsupported by CPU renderer".into()),
        }
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()>;
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, damage: Damage) -> Result<()>;
}

pub struct CpuRenderer {
    fonts: Option<SharedFontSystem>,
    glyphs: SwashCache,
    textures: bead_core::cache::Lru<String, Pixmap>,
    glyph_budget: bead_core::cache::Budget,
    glyph_charge: Option<bead_core::cache::Charge>,
    glyph_uses: HashMap<cosmic_text::CacheKey, u64>,
    glyph_clock: u64,
    mask: Option<Mask>,
    old_mask: Option<Damage>,
    mask_charge: Option<bead_core::cache::Charge>,
    used_fonts: BTreeSet<String>,
    color_glyphs: usize,
}
impl Default for CpuRenderer {
    fn default() -> Self {
        Self {
            fonts: None,
            glyphs: SwashCache::new(),
            textures: bead_core::cache::Lru::new("legacy-cpu-textures", 64 * bead_core::cache::MIB),
            glyph_budget: bead_core::cache::Budget::new("swash-images", 8 * bead_core::cache::MIB),
            glyph_charge: None,
            glyph_uses: HashMap::new(),
            glyph_clock: 0,
            mask: None,
            old_mask: None,
            mask_charge: None,
            used_fonts: BTreeSet::new(),
            color_glyphs: 0,
        }
    }
}
impl CpuRenderer {
    fn cap_glyphs(&mut self) -> Result<()> {
        self.glyph_charge = None;
        // This renderer never requests outlines. Avoid retaining dependency-only data.
        self.glyphs.outline_command_cache.clear();
        loop {
            let bytes = self
                .glyphs
                .image_cache
                .values()
                .map(|v| v.as_ref().map_or(0, |i| i.data.capacity()))
                .sum::<usize>()
                + self.glyphs.image_cache.capacity() * 192
                + self.glyph_uses.capacity() * 64;
            if let Ok(charge) = self.glyph_budget.reserve(bytes) {
                self.glyph_charge = Some(charge);
                break;
            }
            let Some(key) = self
                .glyph_uses
                .iter()
                .min_by_key(|(_, age)| **age)
                .map(|(key, _)| *key)
            else {
                self.glyphs = SwashCache::new();
                self.glyph_uses = HashMap::new();
                break;
            };
            self.glyph_uses.remove(&key);
            self.glyphs.image_cache.remove(&key);
            if self.glyph_uses.is_empty() {
                self.glyph_uses.shrink_to_fit();
            }
        }
        Ok(())
    }
}
type SharedFontSystem = std::sync::Arc<std::sync::Mutex<FontSystem>>;
#[derive(PartialEq, Eq)]
struct FontStamp {
    path: std::path::PathBuf,
    length: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64),
}
struct FontPoolIndex {
    stamp: Vec<FontStamp>,
    locale: String,
    fonts: std::sync::Weak<std::sync::Mutex<FontSystem>>,
    _charge: bead_core::cache::Charge,
}
const FONT_POOL_MAX_ENTRIES: usize = 8;
const FONT_POOL_MAX_BYTES: usize = 512 * 1024;
static FONT_POOL_INDEX: std::sync::OnceLock<std::sync::Mutex<VecDeque<FontPoolIndex>>> =
    std::sync::OnceLock::new();
fn font_pool_index() -> &'static std::sync::Mutex<VecDeque<FontPoolIndex>> {
    FONT_POOL_INDEX.get_or_init(|| std::sync::Mutex::new(VecDeque::new()))
}
fn scene_fonts(scene: &Scene) -> Result<SharedFontSystem> {
    let fresh =
        || load_scene_fonts(scene).map(|fonts| std::sync::Arc::new(std::sync::Mutex::new(fonts)));
    // Do not cache an implicit system font database whose file set is unknown.
    if scene.font_files.is_empty() {
        return fresh();
    }
    let locale = scene_locale();
    let mut stamp = Vec::with_capacity(scene.font_files.len());
    for path in &scene.font_files {
        let metadata = std::fs::metadata(path)?;
        let Ok(modified) = metadata.modified() else {
            return fresh();
        };
        stamp.push(FontStamp {
            path: path.clone(),
            length: metadata.len(),
            modified,
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino())
            },
        });
    }
    let mut index = font_pool_index()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    index.retain(|cached| cached.fonts.strong_count() != 0);
    if let Some(position) = index
        .iter()
        .position(|cached| cached.stamp == stamp && cached.locale == locale)
    {
        let cached = index.remove(position).expect("position came from index");
        if let Some(fonts) = cached.fonts.upgrade() {
            index.push_back(cached);
            return Ok(fonts);
        }
    }
    // Keep construction under the short index lock so simultaneous output workers
    // cannot load the same font set twice.
    let fonts = fresh()?;
    let bytes = std::mem::size_of::<FontPoolIndex>()
        + stamp.capacity() * std::mem::size_of::<FontStamp>()
        + stamp.iter().map(|item| item.path.capacity()).sum::<usize>()
        + locale.capacity()
        + std::mem::size_of::<std::sync::Mutex<FontSystem>>()
        + 2 * std::mem::size_of::<usize>();
    let budget = bead_core::cache::Budget::new("weak-font-pool-index", FONT_POOL_MAX_BYTES);
    if let Ok(charge) = budget.reserve(bytes) {
        while index.len() >= FONT_POOL_MAX_ENTRIES
            || index.iter().map(|item| item._charge.bytes()).sum::<usize>() + bytes
                > FONT_POOL_MAX_BYTES
        {
            if index.pop_front().is_none() {
                break;
            }
        }
        if bytes <= FONT_POOL_MAX_BYTES {
            index.push_back(FontPoolIndex {
                stamp,
                locale,
                fonts: std::sync::Arc::downgrade(&fonts),
                _charge: charge,
            });
        }
    }
    // Only the renderer/caller owns font data; the bounded index cannot keep it
    // alive after the existing idle/close teardown drops the renderer's pool.
    Ok(fonts)
}
fn load_scene_fonts(scene: &Scene) -> Result<FontSystem> {
    let start = Instant::now();
    if scene.font_files.is_empty() {
        return Ok(FontSystem::new());
    }
    let mut db = fontdb::Database::new();
    for path in &scene.font_files {
        db.load_font_file(path)?;
    }
    db.set_sans_serif_family("Noto Sans");
    db.set_monospace_family("Noto Sans Mono");
    db.set_serif_family("DejaVu Serif");
    bead_core::vprintln!(
        "PROFILE stage=font_load ms={:.3} files={} faces={}",
        start.elapsed().as_secs_f64() * 1000.,
        scene.font_files.len(),
        db.len()
    );
    Ok(FontSystem::new_with_locale_and_db(scene_locale(), db))
}
fn scene_locale() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "en-US".into())
}
fn key(p: &Primitive, scale: f32) -> String {
    match p {
        Primitive::Text {
            rect,
            text,
            family,
            size,
            color,
            weight,
            align,
        } => format!(
            "t:{scale}:{}:{}:{family}:{weight}:{size}:{color:?}:{align:?}:{text}",
            rect.w, rect.h
        ),
        Primitive::Image {
            rect,
            path,
            blur,
            clip_polygon,
            fit,
        } => {
            format!(
                "i:{blur}:{clip_polygon:?}:{fit:?}:{scale}:{}:{}:{}",
                rect.w,
                rect.h,
                path.display()
            )
        }
        Primitive::Shadow { rect, radius, blur } => {
            format!("s:{scale}:{}:{}:{radius}:{blur}", rect.w, rect.h)
        }
        _ => String::new(),
    }
}
fn dimensions(rect: Rect, scale: f32) -> (u32, u32) {
    (
        (rect.w * scale).ceil().max(1.) as u32,
        (rect.h * scale).ceil().max(1.) as u32,
    )
}
fn apply_polygon_clip(pix: &mut Pixmap, points: &[[f32; 2]]) -> Result<()> {
    let mut path = tiny_skia::PathBuilder::new();
    for (index, [x, y]) in points.iter().enumerate() {
        let point = (x * pix.width() as f32, y * pix.height() as f32);
        if index == 0 {
            path.move_to(point.0, point.1);
        } else {
            path.line_to(point.0, point.1);
        }
    }
    path.close();
    let path = path.finish().ok_or("invalid image clip polygon")?;
    let mut mask =
        tiny_skia::Mask::new(pix.width(), pix.height()).ok_or("image clip allocation")?;
    mask.fill_path(
        &path,
        tiny_skia::FillRule::EvenOdd,
        true,
        tiny_skia::Transform::identity(),
    );
    pix.apply_mask(&mask);
    Ok(())
}
fn rounded(rect: Rect, r: f32) -> Option<tiny_skia::Path> {
    // SVG path parsing is supplied by tiny-skia; no custom rasterizer or shaper.
    let r = r.min(rect.w / 2.).min(rect.h / 2.);
    let x = rect.x;
    let y = rect.y;
    let w = rect.w;
    let h = rect.h;
    let mut p = PathBuilder::new();
    let k = 0.5522848;
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k * r, y, x + w, y + r - k * r, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(
        x + w,
        y + h - r + k * r,
        x + w - r + k * r,
        y + h,
        x + w - r,
        y + h,
    );
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k * r, y + h, x, y + h - r + k * r, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k * r, x + r - k * r, y, x + r, y);
    p.close();
    p.finish()
}
impl Renderer for CpuRenderer {
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if self.fonts.is_none()
            && scene
                .primitives
                .iter()
                .any(|p| matches!(p, Primitive::Text { .. }))
        {
            self.fonts = Some(scene_fonts(scene)?);
        }
        let before = self.textures.len();
        let mut stages = [0f64; 3];
        for p in &scene.primitives {
            if matches!(
                p,
                Primitive::Rounded { .. } | Primitive::Arc { .. } | Primitive::Polyline { .. }
            ) {
                continue;
            }
            let k = key(p, scale);
            if self.textures.contains_key(&k) {
                continue;
            }
            let resource_start = Instant::now();
            let pixmap = match p {
                Primitive::Text {
                    rect,
                    text,
                    family,
                    size,
                    color,
                    weight,
                    align,
                } => {
                    let (w, h) = dimensions(*rect, scale);
                    let mut pix = Pixmap::new(w, h).ok_or("text allocation")?;
                    let shared = self.fonts.as_ref().ok_or("font system unavailable")?;
                    let mut guard = shared.lock().map_err(|_| "font system poisoned")?;
                    let fonts: &mut FontSystem = &mut guard;
                    let mut buf =
                        Buffer::new(fonts, Metrics::new(size * scale, size * scale * 1.4));
                    buf.set_size(Some(w as f32), Some(h as f32));
                    buf.set_text(
                        text,
                        &Attrs::new()
                            .family(Family::Name(family))
                            .weight(cosmic_text::Weight(*weight)),
                        Shaping::Advanced,
                        align.map(|a| match a {
                            TextAlign::Left => cosmic_text::Align::Left,
                            TextAlign::Center => cosmic_text::Align::Center,
                            TextAlign::Right => cosmic_text::Align::Right,
                        }),
                    );
                    let profile = bead_core::profile::Span::new("shape");
                    buf.shape_until_scroll(fonts, false);
                    drop(profile);
                    let _raster_profile = bead_core::profile::Span::new("raster");
                    for run in buf.layout_runs() {
                        for glyph in run.glyphs {
                            if let Some(face) = fonts.db().face(glyph.font_id) {
                                for (family, _) in &face.families {
                                    self.used_fonts.insert(family.clone());
                                }
                            }
                            let physical = glyph.physical((0., run.line_y), 1.);
                            self.glyph_clock += 1;
                            self.glyph_uses.insert(physical.cache_key, self.glyph_clock);
                            if self
                                .glyphs
                                .get_image(fonts, physical.cache_key)
                                .as_ref()
                                .is_some_and(|image| {
                                    image.content == cosmic_text::SwashContent::Color
                                })
                            {
                                self.color_glyphs += 1;
                            }
                        }
                    }
                    buf.draw(
                        fonts,
                        &mut self.glyphs,
                        TextColor::rgba(color[0], color[1], color[2], color[3]),
                        |x, y, cw, ch, c| {
                            for dy in 0..ch {
                                for dx in 0..cw {
                                    let px = x + dx as i32;
                                    let py = y + dy as i32;
                                    if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                                        // cosmic-text supplies glyph coverage but ignores the base colour alpha.
                                        let a =
                                            (u16::from(c.a()) * u16::from(color[3]) + 127) / 255;
                                        let i = (py as usize * w as usize + px as usize) * 4;
                                        // Convert coverage/color to premultiplied RGBA and blend the cached glyph.
                                        let dst = &mut pix.data_mut()[i..i + 4];
                                        let inv = 255 - a;
                                        for (d, v) in dst[..3].iter_mut().zip([c.r(), c.g(), c.b()])
                                        {
                                            *d = ((u16::from(v) * a + u16::from(*d) * inv + 127)
                                                / 255)
                                                as u8;
                                        }
                                        dst[3] = (a + (u16::from(dst[3]) * inv + 127) / 255) as u8;
                                    }
                                }
                            }
                        },
                    );
                    pix
                }
                Primitive::Image {
                    rect,
                    path,
                    blur,
                    clip_polygon,
                    fit,
                } => {
                    let pix = load_image(
                        path,
                        *rect,
                        scale,
                        scene.asset_data.get(path).map(|data| data.as_ref()),
                        *fit,
                    )?;
                    let mut pix = if *blur > 0. {
                        let size = tiny_skia::IntSize::from_wh(pix.width(), pix.height())
                            .ok_or("invalid blur dimensions")?;
                        let image = image::RgbaImage::from_raw(
                            pix.width(),
                            pix.height(),
                            pix.data().to_vec(),
                        )
                        .ok_or("invalid blur image")?;
                        let blurred = image::imageops::blur(&image, blur * scale);
                        Pixmap::from_vec(blurred.into_raw(), size)
                            .ok_or("invalid blurred pixels")?
                    } else {
                        pix
                    };
                    if let Some(points) = clip_polygon {
                        apply_polygon_clip(&mut pix, points)?;
                    }
                    pix
                }
                Primitive::Shadow { rect, radius, blur } => {
                    shadow_pixmap(*rect, *radius, *blur, scale)?
                }
                _ => unreachable!(),
            };
            let stage = match p {
                Primitive::Text { .. } => 0,
                Primitive::Image { .. } => 1,
                _ => 2,
            };
            stages[stage] += resource_start.elapsed().as_secs_f64() * 1000.;
            let bytes = pixmap.data().len() + k.capacity() + 128;
            self.textures.insert(k, pixmap, bytes)?;
            self.cap_glyphs()?;
        }
        if scene
            .primitives
            .iter()
            .filter(|p| {
                !matches!(
                    p,
                    Primitive::Rounded { .. } | Primitive::Arc { .. } | Primitive::Polyline { .. }
                )
            })
            .any(|p| !self.textures.contains_key(&key(p, scale)))
        {
            return Err("active CPU textures exceed cache budget".into());
        }
        if self.textures.len() != before {
            bead_core::vprintln!(
                "PROFILE stage=resources text_ms={:.3} images_ms={:.3} shadow_ms={:.3}",
                stages[0],
                stages[1],
                stages[2]
            );
            bead_core::vprintln!(
                "RESOURCES scale={scale} textures={} color_glyphs={} fonts={:?}",
                self.textures.len(),
                self.color_glyphs,
                self.used_fonts
            );
        }
        Ok(())
    }
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, d: Damage) -> Result<()> {
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        if d.w == 0 || d.h == 0 {
            return Ok(());
        }
        if d.x + d.w > width || d.y + d.h > height {
            return Err("damage outside target".into());
        }
        // Clear only the region being reconstructed. Thus fallback BGRA never
        // needs conversion back to RGBA: every changed destination is overwritten.
        for y in d.y..d.y + d.h {
            let i = (y as usize * width as usize + d.x as usize) * 4;
            bytes[i..i + d.w as usize * 4].fill(0);
        }
        if self
            .mask
            .as_ref()
            .is_none_or(|m| m.width() != width || m.height() != height)
        {
            self.mask = None;
            self.mask_charge = None;
            self.mask_charge = Some(
                bead_core::cache::Budget::new("legacy-cpu-mask", 64 * bead_core::cache::MIB)
                    .reserve(width as usize * height as usize + 64)?,
            );
            self.mask = Some(Mask::new(width, height).ok_or("clip allocation")?);
            self.old_mask = None;
        }
        let mask = self.mask.as_mut().unwrap();
        let mut pix = PixmapMut::from_bytes(bytes, width, height).ok_or("invalid SHM target")?;
        let transform = Transform::from_scale(scale, scale).post_translate(
            (scene.offset.0 * scale).round(),
            (scene.offset.1 * scale).round(),
        );
        let logical_damage = Rect::new(
            d.x as f32 / scale - scene.offset.0,
            d.y as f32 / scale - scene.offset.1,
            d.w as f32 / scale,
            d.h as f32 / scale,
        );
        for (index, p) in scene.primitives.iter().enumerate() {
            let (dx, dy) = scene.content_offset(index);
            let shifted;
            let p = if dx != 0. || dy != 0. {
                shifted = p.translated(dx, dy);
                &shifted
            } else {
                p
            };
            let clipped = scene.clip_at(index).map(|clip| {
                clip.translated(scene.offset.0, scene.offset.1)
                    .pixels(scale, width, height)
            });
            let region = if let Some(c) = clipped {
                let x = d.x.max(c.x);
                let y = d.y.max(c.y);
                Damage {
                    x,
                    y,
                    w: (d.x + d.w).min(c.x + c.w).saturating_sub(x),
                    h: (d.y + d.h).min(c.y + c.h).saturating_sub(y),
                }
            } else {
                d
            };
            if region.w == 0 || region.h == 0 {
                continue;
            }
            if self.old_mask != Some(region) {
                if let Some(old) = self.old_mask {
                    for y in old.y..old.y + old.h {
                        let i = (y * width + old.x) as usize;
                        mask.data_mut()[i..i + old.w as usize].fill(0);
                    }
                }
                for y in region.y..region.y + region.h {
                    let i = (y * width + region.x) as usize;
                    mask.data_mut()[i..i + region.w as usize].fill(255);
                }
                self.old_mask = Some(region);
            }
            let bounds = match p {
                Primitive::Arc { rect, .. } | Primitive::Polyline { rect, .. } => *rect,
                Primitive::Rounded { rect, border, .. } => {
                    let pad = border.map_or(1., |(_, w)| w + 1.);
                    Rect::new(
                        rect.x - pad,
                        rect.y - pad,
                        rect.w + pad * 2.,
                        rect.h + pad * 2.,
                    )
                }
                Primitive::Text { rect, .. }
                | Primitive::Image { rect, .. }
                | Primitive::Shadow { rect, .. } => *rect,
            };
            if bounds.x + bounds.w < logical_damage.x
                || bounds.y + bounds.h < logical_damage.y
                || bounds.x > logical_damage.x + logical_damage.w
                || bounds.y > logical_damage.y + logical_damage.h
            {
                continue;
            }
            match p {
                Primitive::Arc {
                    rect,
                    start_angle,
                    sweep_angle,
                    stroke_width,
                    color,
                    round_caps,
                } => {
                    if color[3] == 0 {
                        continue;
                    }
                    if let Some(points) =
                        arc_points(*rect, *stroke_width, *start_angle, *sweep_angle)
                    {
                        let mut path = PathBuilder::new();
                        path.move_to(points[0].0, points[0].1);
                        for &(x, y) in &points[1..] {
                            path.line_to(x, y);
                        }
                        if sweep_angle.abs() == 360. {
                            path.close();
                        }
                        if let Some(path) = path.finish() {
                            let mut paint = Paint::default();
                            paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
                            pix.stroke_path(
                                &path,
                                &paint,
                                &Stroke {
                                    width: *stroke_width,
                                    line_cap: if *round_caps {
                                        tiny_skia::LineCap::Round
                                    } else {
                                        tiny_skia::LineCap::Butt
                                    },
                                    ..Stroke::default()
                                },
                                transform,
                                Some(mask),
                            );
                        }
                    }
                }
                Primitive::Polyline {
                    rect,
                    points,
                    closed,
                    stroke_width,
                    color,
                    round_caps,
                    fill,
                } => {
                    if let Some(points) = polyline_points(*rect, *stroke_width, points) {
                        let mut path = PathBuilder::new();
                        path.move_to(points[0].0, points[0].1);
                        for &(x, y) in &points[1..] {
                            path.line_to(x, y);
                        }
                        if *closed {
                            path.close();
                        }
                        if let Some(path) = path.finish() {
                            if let Some(c) = fill.filter(|c| *closed && c[3] != 0) {
                                let mut paint = Paint::default();
                                paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
                                pix.fill_path(
                                    &path,
                                    &paint,
                                    FillRule::Winding,
                                    transform,
                                    Some(mask),
                                );
                            }
                            if *stroke_width > 0. && color[3] != 0 {
                                let mut paint = Paint::default();
                                paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
                                pix.stroke_path(
                                    &path,
                                    &paint,
                                    &Stroke {
                                        width: *stroke_width,
                                        line_cap: if *round_caps {
                                            tiny_skia::LineCap::Round
                                        } else {
                                            tiny_skia::LineCap::Butt
                                        },
                                        line_join: tiny_skia::LineJoin::Round,
                                        ..Stroke::default()
                                    },
                                    transform,
                                    Some(mask),
                                );
                            }
                        }
                    }
                }
                Primitive::Rounded {
                    rect,
                    radius,
                    fill,
                    border,
                } => {
                    if let Some(path) = rounded(*rect, *radius) {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                        pix.fill_path(&path, &paint, FillRule::Winding, transform, Some(mask));
                        if let Some((c, w)) = border {
                            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
                            pix.stroke_path(
                                &path,
                                &paint,
                                &Stroke {
                                    width: *w,
                                    ..Stroke::default()
                                },
                                transform,
                                Some(mask),
                            );
                        }
                    }
                }
                _ => {
                    let rect = match p {
                        Primitive::Text { rect, .. }
                        | Primitive::Image { rect, .. }
                        | Primitive::Shadow { rect, .. } => *rect,
                        _ => unreachable!(),
                    };
                    let tex = self
                        .textures
                        .get(&key(p, scale))
                        .ok_or("resource not prepared")?;
                    pix.draw_pixmap(
                        ((rect.x + scene.offset.0) * scale).round() as i32,
                        ((rect.y + scene.offset.1) * scale).round() as i32,
                        tex.as_ref(),
                        &PixmapPaint::default(),
                        Transform::identity(),
                        Some(mask),
                    );
                }
            }
        }
        if format == PixelFormat::Bgra {
            swizzle(bytes, width, d);
        }
        Ok(())
    }
}
/// Only touched pixels are converted for mandatory ARGB8888 compositors.
pub fn swizzle(bytes: &mut [u8], width: u32, d: Damage) {
    for y in d.y..d.y + d.h {
        let i = (y * width + d.x) as usize * 4;
        for px in bytes[i..i + d.w as usize * 4]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
        {
            px.swap(0, 2);
        }
    }
}
fn shadow_svg(w: f32, h: f32, radius: f32, blur: f32) -> String {
    let pad = blur * 3.;
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}'><defs><filter id='s' x='-50%' y='-50%' width='200%' height='200%'><feGaussianBlur stdDeviation='{blur}'/></filter></defs><rect x='{pad}' y='{pad}' width='{}' height='{}' rx='{radius}' fill='black' fill-opacity='.32' filter='url(#s)'/></svg>",
        w - 2. * pad,
        h - 2. * pad
    )
}
fn shadow_pixmap(rect: Rect, radius: f32, blur: f32, scale: f32) -> Result<Pixmap> {
    let (w, h) = dimensions(rect, scale);
    // Preserve the same Gaussian corners/edges, stretching only straight sections.
    // Bake a small template with enough room for radius + 3 sigma on either side.
    let cut = ((radius + 6. * blur) * scale).ceil() as u32;
    let side = cut * 2 + 2;
    if cut == 0 || w < side || h < side {
        return svg_pixmap(shadow_svg(rect.w, rect.h, radius, blur).as_bytes(), w, h);
    }
    // The Gaussian template depends on radius, blur and scale only. A revealing panel re-shapes its
    // shadow every frame, so rendering the template again each time cost tens of milliseconds.
    type Templates = Vec<((u32, u32, u32), std::sync::Arc<Pixmap>)>;
    static TEMPLATES: std::sync::Mutex<Templates> = std::sync::Mutex::new(Vec::new());
    let template_key = (radius.to_bits(), blur.to_bits(), scale.to_bits());
    let cached = TEMPLATES.lock().ok().and_then(|templates| {
        templates
            .iter()
            .find(|(key, _)| *key == template_key)
            .map(|(_, pixmap)| pixmap.clone())
    });
    let template = if let Some(template) = cached {
        template
    } else {
        let logical_side = side as f32 / scale;
        let template = std::sync::Arc::new(svg_pixmap(
            shadow_svg(logical_side, logical_side, radius, blur).as_bytes(),
            side,
            side,
        )?);
        if let Ok(mut templates) = TEMPLATES.lock() {
            if templates.len() >= 6 {
                templates.remove(0);
            }
            templates.push((template_key, template.clone()));
        }
        template
    };
    let mut result = Pixmap::new(w, h).ok_or("shadow allocation")?;
    let src = [0, cut, side - cut, side];
    let xs = [0, cut, w - cut, w];
    let ys = [0, cut, h - cut, h];
    for row in 0..3 {
        for col in 0..3 {
            let tile = template
                .clone_rect(
                    tiny_skia::IntRect::from_xywh(
                        src[col] as i32,
                        src[row] as i32,
                        src[col + 1] - src[col],
                        src[row + 1] - src[row],
                    )
                    .ok_or("shadow tile")?,
                )
                .ok_or("shadow crop")?;
            // Separable nine-patch: corners stay at native resolution, edge tangents stretch.
            let tr = Transform::from_scale(
                (xs[col + 1] - xs[col]) as f32 / tile.width() as f32,
                (ys[row + 1] - ys[row]) as f32 / tile.height() as f32,
            )
            .post_translate(xs[col] as f32, ys[row] as f32);
            result.draw_pixmap(0, 0, tile.as_ref(), &PixmapPaint::default(), tr, None);
        }
    }
    Ok(result)
}
fn svg_pixmap(data: &[u8], w: u32, h: u32) -> Result<Pixmap> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default())?;
    let mut pix = Pixmap::new(w, h).ok_or("SVG allocation")?;
    let s = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(w as f32 / s.width(), h as f32 / s.height()),
        &mut pix.as_mut(),
    );
    Ok(pix)
}
/// Rasterize a static vector directly into borrowed compositor memory. This
/// deliberately retains no decoded tree, texture, font pool or second bitmap.
pub fn svg_into(data: &[u8], target: ShmTarget<'_>) -> Result<()> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default())?;
    target.bytes.fill(0);
    let mut pix = PixmapMut::from_bytes(target.bytes, target.width, target.height)
        .ok_or("invalid SVG target")?;
    let size = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(
            target.width as f32 / size.width(),
            target.height as f32 / size.height(),
        ),
        &mut pix,
    );
    if matches!(target.format, PixelFormat::Bgra) {
        for pixel in target.bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }
    Ok(())
}
#[cfg(test)]
mod borrowed_svg_tests {
    use super::*;
    #[test]
    fn borrowed_svg_respects_pixel_format_and_target_boundaries() {
        let svg=br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="rgb(40,80,160)"/></svg>"#;
        for (format, expected) in [
            (PixelFormat::Rgba, [40, 80, 160, 255]),
            (PixelFormat::Bgra, [160, 80, 40, 255]),
        ] {
            let mut guarded = [231u8; 96];
            svg_into(
                svg,
                ShmTarget {
                    bytes: &mut guarded[16..80],
                    width: 4,
                    height: 4,
                    scale: 1.,
                    format,
                },
            )
            .unwrap();
            assert!(
                guarded[..16]
                    .iter()
                    .chain(&guarded[80..])
                    .all(|&b| b == 231)
            );
            assert!(
                guarded[16..80]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == expected)
            );
        }
    }
}
/// Validate an immutable asset snapshot with the same decoder used for drawing.
pub fn validate_asset(path: &Path, bytes: &[u8]) -> Result<()> {
    load_image(
        path,
        Rect::new(0., 0., 4., 4.),
        1.,
        Some(bytes),
        ImageFit::Fill,
    )
    .map(|_| ())
}
fn load_image(
    path: &Path,
    rect: Rect,
    scale: f32,
    snapshot: Option<&[u8]>,
    fit: ImageFit,
) -> Result<Pixmap> {
    let file;
    let bytes = if let Some(snapshot) = snapshot {
        snapshot
    } else {
        file = std::fs::read(path)?;
        &file
    };
    let (w, h) = dimensions(rect, scale);
    if path.extension().is_some_and(|e| e == "svg") {
        return svg_pixmap(bytes, w, h);
    }
    let mut raw = {
        let decoded = image::load_from_memory(bytes)?;
        match fit {
            ImageFit::Fill => {
                // Large raster icons (256-512 px) shrunk to ~50 px cost tens of milliseconds with
                // Lanczos3; a two-lobe cubic is visually equivalent at that reduction.
                let filter = if decoded.width() >= 2 * w || decoded.height() >= 2 * h {
                    image::imageops::FilterType::CatmullRom
                } else {
                    image::imageops::FilterType::Lanczos3
                };
                decoded.resize_exact(w, h, filter).into_rgba8()
            }
            ImageFit::Cover => decoded.into_rgba8(),
        }
    };
    for pixel in raw.as_mut().as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for channel in pixel.iter_mut().take(3) {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let source_size =
        tiny_skia::IntSize::from_wh(raw.width(), raw.height()).ok_or("image allocation")?;
    let source = Pixmap::from_vec(raw.into_raw(), source_size).ok_or("image allocation")?;
    if matches!(fit, ImageFit::Fill) {
        return Ok(source);
    }
    let cover_scale = (w as f32 / source.width() as f32).max(h as f32 / source.height() as f32);
    let offset_x = (w as f32 - source.width() as f32 * cover_scale) * 0.5;
    let offset_y = (h as f32 - source.height() as f32 * cover_scale) * 0.5;
    let mut target = Pixmap::new(w, h).ok_or("image allocation")?;
    target.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        },
        Transform::from_scale(cover_scale, cover_scale).post_translate(offset_x, offset_y),
        None,
    );
    Ok(target)
}
/// Reads Orient format 1 only. Unknown metadata is intentionally ignored.
#[derive(Clone)]
pub struct Palette(pub std::sync::Arc<HashMap<String, [u8; 4]>>);
impl Palette {
    pub fn read(path: &Path) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        if v["formatVersion"] != 1 {
            return Err("unsupported Orient palette formatVersion".into());
        }
        let mut colors = HashMap::new();
        for (role, value) in v["colours"].as_object().ok_or("missing colours")? {
            let s = value.as_str().ok_or("color must be a string")?;
            if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("invalid color role {role}").into());
            }
            let n = u32::from_str_radix(s, 16)?;
            colors.insert(
                role.clone(),
                [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255],
            );
        }
        // A weak slot shares the immutable theme across independently reloaded
        // output sessions without retaining its colours after their teardown.
        static SHARED: std::sync::Mutex<std::sync::Weak<HashMap<String, [u8; 4]>>> =
            std::sync::Mutex::new(std::sync::Weak::new());
        let mut shared = SHARED.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(current) = shared.upgrade()
            && *current == colors
        {
            return Ok(Self(current));
        }
        let colors = std::sync::Arc::new(colors);
        *shared = std::sync::Arc::downgrade(&colors);
        Ok(Self(colors))
    }
    pub fn get(&self, name: &str) -> Result<[u8; 4]> {
        self.0
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing palette role {name}").into())
    }
}
/// Measure a UTF-8 boundary with the same shaper/font selection as painted text.
pub fn text_boundary_x(
    scene: &Scene,
    text: &str,
    boundary: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<f32> {
    let shared = scene_fonts(scene)?;
    let mut guard = shared.lock().map_err(|_| "font system poisoned")?;
    let fonts: &mut FontSystem = &mut guard;
    boundary_x(fonts, text, boundary, family, size, width)
}
/// Wrapped layout of `text` exactly as the CPU renderer draws it at scale 1: the same
/// shaper and font selection, `width` as the wrap width, line pitch `size * 1.4`.
/// Hosts size text boxes from this; the caret helpers below serve multi-line inputs.
fn wrapped<R>(
    scene: &Scene,
    text: &str,
    family: &str,
    size: f32,
    width: f32,
    use_buffer: impl FnOnce(&mut FontSystem, &mut Buffer) -> R,
) -> Result<R> {
    let shared = scene_fonts(scene)?;
    let mut guard = shared.lock().map_err(|_| "font system poisoned")?;
    let fonts: &mut FontSystem = &mut guard;
    let mut buffer = Buffer::new(fonts, Metrics::new(size, size * 1.4));
    buffer.set_size(Some(width.max(1.)), None);
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    Ok(use_buffer(fonts, &mut buffer))
}
/// Unwrapped advance of the widest line, using the same shaper as painting.
pub fn text_width(scene: &Scene, text: &str, family: &str, size: f32) -> Result<f32> {
    wrapped(scene, text, family, size, 20_000., |_, buffer| {
        buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0_f32, f32::max)
    })
}
/// Number of drawn lines (at least one), counting wraps and explicit newlines.
pub fn text_lines(scene: &Scene, text: &str, family: &str, size: f32, width: f32) -> Result<usize> {
    wrapped(scene, text, family, size, width, |_, buffer| {
        buffer.layout_runs().count().max(1)
    })
}
fn cursor_at(text: &str, boundary: usize) -> cosmic_text::Cursor {
    let before = &text[..boundary.min(text.len())];
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    cosmic_text::Cursor::new(before.matches('\n').count(), before.len() - start)
}
fn boundary_at(text: &str, cursor: cosmic_text::Cursor) -> usize {
    let mut offset = 0;
    for (index, line) in text.split('\n').enumerate() {
        if index == cursor.line {
            let mut end = cursor.index.min(line.len());
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            return offset + end;
        }
        offset += line.len() + 1;
    }
    text.len()
}
/// Top-left of the caret for a UTF-8 boundary, relative to the text box.
pub fn text_caret(
    scene: &Scene,
    text: &str,
    boundary: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<(f32, f32)> {
    wrapped(scene, text, family, size, width, |_, buffer| {
        buffer
            .cursor_position(&cursor_at(text, boundary))
            .unwrap_or_else(|| {
                // Past the shaped text (an empty trailing line): start of the last line.
                let lines = buffer.layout_runs().count().max(1);
                (0., (lines - 1) as f32 * size * 1.4)
            })
    })
}
/// The boundary one drawn line above or below, keeping the caret's x position.
pub fn text_vertical(
    scene: &Scene,
    text: &str,
    boundary: usize,
    up: bool,
    family: &str,
    size: f32,
    width: f32,
) -> Result<usize> {
    wrapped(scene, text, family, size, width, |_, buffer| {
        let pitch = size * 1.4;
        let (x, y) = buffer
            .cursor_position(&cursor_at(text, boundary))
            .unwrap_or((0., 0.));
        let lines = buffer.layout_runs().count().max(1);
        if up && y < pitch / 2. {
            return 0;
        }
        if !up && y + pitch * 1.5 > lines as f32 * pitch {
            return text.len();
        }
        let target = if up { y - pitch / 2. } else { y + pitch * 1.5 };
        buffer
            .hit(x, target)
            .map_or(boundary, |cursor| boundary_at(text, cursor))
    })
}
/// One rectangle per drawn line covered by `[from, to)`, relative to the text box.
pub fn text_selection(
    scene: &Scene,
    text: &str,
    from: usize,
    to: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<Vec<Rect>> {
    let (from, to) = (from.min(to), from.max(to));
    wrapped(scene, text, family, size, width, |_, buffer| {
        let (start, end) = (cursor_at(text, from), cursor_at(text, to));
        buffer
            .layout_runs()
            .flat_map(|run| {
                let (top, height) = (run.line_top, run.line_height);
                run.highlight(start, end)
                    .map(move |(x, w)| Rect::new(x, top, w.max(1.), height))
                    .collect::<Vec<_>>()
            })
            .collect()
    })
}
fn boundary_x(
    fonts: &mut FontSystem,
    text: &str,
    boundary: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<f32> {
    let mut buffer = Buffer::new(fonts, Metrics::new(size, size * 1.4));
    buffer.set_size(Some(width), Some(size * 2.));
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    let Some(run) = buffer.layout_runs().next() else {
        return Ok(0.);
    };
    for glyph in run.glyphs {
        if boundary <= glyph.start {
            return Ok(glyph.x);
        }
        if boundary < glyph.end {
            return Ok(glyph.x + glyph.w);
        }
    }
    Ok(run.line_w)
}

#[cfg(test)]
mod tests {
    #[test]
    fn resource_pruning_remaps_retained_layers_clips_and_reveal_parts() {
        let mut scene = font_pool_scene();
        scene.primitives = (0..3)
            .map(|index| Primitive::Rounded {
                rect: Rect::new(index as f32 * 20., 0., 20., 20.),
                radius: 2.,
                fill: [30, 40, 50, 255],
                border: None,
            })
            .collect();
        let clip = Rect::new(40., 0., 20., 20.);
        scene.clips = [(1, Rect::new(20., 0., 20., 20.)), (2, clip)].into();
        scene.layers = vec![
            SceneLayer {
                id: "root".into(),
                parent: None,
                own: 0..1,
                subtree_end: 3,
                bounds: Rect::new(0., 0., 60., 20.),
            },
            SceneLayer {
                id: "child".into(),
                parent: Some("root".into()),
                own: 1..3,
                subtree_end: 3,
                bounds: Rect::new(20., 0., 40., 20.),
            },
        ];
        scene.motion = Some(Motion {
            id: "reveal".into(),
            open: true,
            duration: 0.3,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            fade: false,
            reveal: true,
            progress: 1.,
            parts: vec![
                (0, scene.primitives[0].clone()),
                (2, scene.primitives[2].clone()),
            ],
            art: None,
        });
        scene.retain_primitives(
            |primitive| !matches!(primitive, Primitive::Rounded { rect, .. } if rect.x == 20.),
        );
        assert_eq!(scene.primitives.len(), 2);
        assert_eq!(scene.layers[0].own, 0..1);
        assert_eq!(scene.layers[0].subtree_end, 2);
        assert_eq!(scene.layers[1].own, 1..2);
        assert_eq!(scene.layers[1].subtree_end, 2);
        assert_eq!(scene.clips.len(), 1);
        assert_eq!(scene.clip_at(1), Some(clip));
        assert_eq!(
            scene
                .motion
                .as_ref()
                .unwrap()
                .parts
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        // The formerly stale child range ended beyond the pruned primitive list.
        renderer(RendererKind::Cpu)
            .unwrap()
            .prepare(&scene, 1.)
            .unwrap();
    }

    use super::*;
    #[test]
    fn fill_image_premultiplication_preserves_exact_lanczos_pixels() {
        use image::ImageEncoder;

        let source = image::RgbaImage::from_raw(
            3,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 20, 40, 60, 32, 90, 80, 70, 192, 200,
                180, 160, 255,
            ],
        )
        .unwrap();
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                source.as_raw(),
                source.width(),
                source.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();

        // Keep the former conversion path as the pixel-for-pixel reference.
        let decoded = image::load_from_memory(&bytes).unwrap();
        let reference = decoded
            .resize_exact(5, 3, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let mut expected = Pixmap::new(5, 3).unwrap();
        for (dst, src) in expected
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(reference.as_raw().as_chunks::<4>().0)
        {
            let alpha = u16::from(src[3]);
            for channel in 0..3 {
                dst[channel] = ((u16::from(src[channel]) * alpha + 127) / 255) as u8;
            }
            dst[3] = src[3];
        }

        let actual = load_image(
            Path::new("wallpaper-preview.png"),
            Rect::new(0., 0., 5., 3.),
            1.,
            Some(&bytes),
            ImageFit::Fill,
        )
        .unwrap();
        assert_eq!((actual.width(), actual.height()), (5, 3));
        assert_eq!(actual.data(), expected.data());
    }
    #[test]
    fn cover_image_centers_crop_and_keeps_premultiplied_alpha() {
        use image::ImageEncoder;

        let source = image::RgbaImage::from_raw(
            4,
            2,
            vec![
                255, 0, 255, 255, 255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 0, 255, 255, 255, 0, 255, 0,
                0, 255, 255, 255, 255, 255, 64, 0, 255, 255, 255,
            ],
        )
        .unwrap();
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                source.as_raw(),
                source.width(),
                source.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();

        let actual = load_image(
            Path::new("wallpaper-preview.png"),
            Rect::new(0., 0., 2., 2.),
            1.,
            Some(&bytes),
            ImageFit::Cover,
        )
        .unwrap();
        let premultiplied = |pixel: [u8; 4]| {
            let alpha = u16::from(pixel[3]);
            [
                ((u16::from(pixel[0]) * alpha + 127) / 255) as u8,
                ((u16::from(pixel[1]) * alpha + 127) / 255) as u8,
                ((u16::from(pixel[2]) * alpha + 127) / 255) as u8,
                pixel[3],
            ]
        };
        let expected = [
            premultiplied([255, 0, 0, 0]),
            premultiplied([0, 255, 0, 128]),
            premultiplied([0, 0, 255, 255]),
            premultiplied([255, 255, 255, 64]),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert_eq!((actual.width(), actual.height()), (2, 2));
        assert_eq!(actual.data(), expected);
    }
    #[test]
    fn polygon_image_mask_clears_pixels_outside_hexagon() {
        let mut image = Pixmap::new(20, 20).unwrap();
        image.fill(tiny_skia::Color::WHITE);
        apply_polygon_clip(
            &mut image,
            &[
                [0.25, 0.],
                [0.75, 0.],
                [1., 0.5],
                [0.75, 1.],
                [0.25, 1.],
                [0., 0.5],
            ],
        )
        .unwrap();
        let pixel = |x: usize, y: usize| image.pixels()[y * 20 + x].alpha();
        assert_eq!(pixel(10, 10), 255);
        assert_eq!(pixel(0, 0), 0);
        assert_eq!(pixel(19, 0), 0);
    }
    #[test]
    fn wrapped_text_lines_caret_and_vertical_moves() {
        let scene = font_pool_scene();
        let family = "IBM Plex Sans";
        assert_eq!(text_lines(&scene, "", family, 16., 200.).unwrap(), 1);
        assert_eq!(
            text_lines(&scene, "a\nb\n\nc", family, 16., 200.).unwrap(),
            4
        );
        let long = "word ".repeat(40);
        let lines = text_lines(&scene, &long, family, 16., 120.).unwrap();
        assert!(lines > 8, "{lines}");
        // The caret of a boundary after a break sits lower and nearer the left edge.
        let (x0, y0) = text_caret(&scene, "ab\ncd", 1, family, 16., 200.).unwrap();
        let (x1, y1) = text_caret(&scene, "ab\ncd", 4, family, 16., 200.).unwrap();
        assert!(x0 > 0. && x1 > 0. && y1 > y0);
        // Down from the first line lands on the second, up returns to the first.
        let down = text_vertical(&scene, "ab\ncd", 1, false, family, 16., 200.).unwrap();
        assert!((3..=5).contains(&down));
        let up = text_vertical(&scene, "ab\ncd", down, true, family, 16., 200.).unwrap();
        assert!(up <= 2);
        assert_eq!(
            text_vertical(&scene, "ab\ncd", 1, true, family, 16., 200.).unwrap(),
            0
        );
        assert_eq!(
            text_vertical(&scene, "ab\ncd", 4, false, family, 16., 200.).unwrap(),
            5
        );
        // A selection across two lines yields one rectangle per line.
        let rects = text_selection(&scene, "ab\ncd", 1, 4, family, 16., 200.).unwrap();
        assert_eq!(rects.len(), 2);
        assert!(rects[1].y > rects[0].y);
    }
    /// B0067: a revealing panel's fill and shadow shrink with the progress (rounded on the growing edge, shadow following)
    /// while its content stays put under the window, and full progress restores the open-state shapes exactly.
    #[test]
    fn reveal_reshapes_the_body_and_its_shadow_and_restores_them() {
        let body = Rect::new(32., 32., 300., 200.);
        let mut scene = font_pool_scene();
        scene.primitives = vec![
            Primitive::Shadow {
                rect: Rect::new(2., 2., 360., 260.),
                radius: 18.,
                blur: 10.,
            },
            Primitive::Rounded {
                rect: body,
                radius: 18.,
                fill: [1, 2, 3, 255],
                border: None,
            },
            Primitive::Text {
                rect: Rect::new(40., 40., 100., 20.),
                text: "x".into(),
                family: "IBM Plex Sans".into(),
                size: 14.,
                color: [9, 9, 9, 255],
                weight: 400,
                align: None,
            },
        ];
        scene.motion = Some(Motion {
            id: "m".into(),
            open: true,
            duration: 0.36,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            fade: false,
            reveal: true,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });
        scene.reveal(0.5);
        let Primitive::Rounded { rect, radius, .. } = &scene.primitives[1] else {
            panic!()
        };
        assert_eq!((rect.y, rect.h, *radius), (32., 100., 18.));
        let Primitive::Shadow { rect, .. } = &scene.primitives[0] else {
            panic!()
        };
        assert_eq!((rect.y, rect.h), (2., 160.));
        // The shapes are not clipped (they were reshaped); the content is, to the revealed window.
        assert!(scene.clip_at(0).is_none() && scene.clip_at(1).is_none());
        let window = scene.clip_at(2).unwrap();
        assert!((window.y + window.h - 132.).abs() < 0.01, "{window:?}");
        // Applying again from a different progress starts from the open-state shapes, not the previous frame.
        scene.reveal(0.25);
        let Primitive::Rounded { rect, .. } = &scene.primitives[1] else {
            panic!()
        };
        assert_eq!(rect.h, 50.);
        scene.reveal(1.);
        let Primitive::Rounded { rect, .. } = &scene.primitives[1] else {
            panic!()
        };
        assert_eq!(*rect, body);
        assert!(scene.clip_at(2).is_none());
    }
    #[test]
    fn sliding_reveal_keeps_content_rasters_and_clips_at_the_join() -> Result<()> {
        let mut scene = font_pool_scene();
        scene.bounds = Rect::new(0., 0., 164., 264.);
        scene.primitives = vec![
            Primitive::Rounded {
                rect: Rect::new(32., 32., 100., 200.),
                radius: 12.,
                fill: [10, 20, 30, 255],
                border: None,
            },
            Primitive::Text {
                rect: Rect::new(40., 40., 40., 20.),
                text: "Retained".into(),
                family: "IBM Plex Sans".into(),
                size: 13.,
                weight: 400,
                align: None,
                color: [200, 0, 0, 255],
            },
            Primitive::Arc {
                rect: Rect::new(40., 80., 40., 40.),
                start_angle: -90.,
                sweep_angle: 210.,
                stroke_width: 4.,
                color: [0, 200, 0, 255],
                round_caps: true,
            },
        ];
        scene.clips.insert(1, Rect::new(40., 40., 40., 20.));
        scene.clips.insert(2, Rect::new(40., 80., 40., 40.));
        scene.motion = Some(Motion {
            id: "slide-reveal".into(),
            open: true,
            duration: 0.3,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Bottom,
            slide_distance: Some(200.),
            fade: false,
            reveal: true,
            progress: 1.,
            parts: Vec::new(),
            art: None,
        });
        let original = serde_json::to_vec(&scene.primitives)?;
        let mut cache = tree::TreeCache::default();
        cache.prepare(&scene, 1.5)?;
        let prepared = cache.frame.clone().unwrap();
        for progress in [0.25, 0.75, 0.5, 1.] {
            scene.reveal(progress);
            let (dx, dy) = scene.content_offset(1);
            assert_eq!((dx, dy), (0., 200. * (1. - progress)));
            assert_eq!(scene.content_offset(0), (0., 0.), "body stays joined");
            let mut direct = tree::TreeCache::default();
            direct.prepare(&scene, 1.5)?;
            scene.prepared.insert(1.5_f32.to_bits(), prepared.clone());
            cache.prepare(&scene, 1.5)?;
            let from_worker = cache.frame.as_ref().unwrap();
            let direct = direct.frame.as_ref().unwrap();
            assert_eq!(
                serde_json::to_vec(&from_worker.items[1].primitive)?,
                serde_json::to_vec(&direct.items[1].primitive)?
            );
            assert_eq!(from_worker.items[1].clip, direct.items[1].clip);
            assert_eq!(
                serde_json::to_vec(&from_worker.items[2].primitive)?,
                serde_json::to_vec(&direct.items[2].primitive)?
            );
            assert_eq!(from_worker.items[2].clip, direct.items[2].clip);
            assert_eq!(tree::rect(&from_worker.items[2].primitive).y, 80. + dy);
            assert!(
                from_worker.items[2].raster.is_none(),
                "arc stays directly painted"
            );
            assert!(
                std::sync::Arc::ptr_eq(
                    from_worker.items[1].raster.as_ref().unwrap(),
                    prepared.items[1].raster.as_ref().unwrap(),
                ),
                "motion must reuse the worker's glyph raster"
            );
            assert_eq!(tree::rect(&from_worker.items[1].primitive).y, 40. + dy);
            if progress < 1. {
                // A frame already baked at this pose must not apply the content slide twice.
                scene.prepared.insert(1.5_f32.to_bits(), direct.clone());
                cache.prepare(&scene, 1.5)?;
                let rebaked = cache.frame.as_ref().unwrap();
                assert_eq!(
                    tree::rect(&rebaked.items[1].primitive),
                    tree::rect(&direct.items[1].primitive)
                );
                assert_eq!(rebaked.items[1].clip, direct.items[1].clip);
                assert_eq!(
                    tree::rect(&rebaked.items[2].primitive),
                    tree::rect(&direct.items[2].primitive)
                );
                assert_eq!(rebaked.items[2].clip, direct.items[2].clip);
            }
            scene.prepared.clear();
        }
        assert_eq!(serde_json::to_vec(&scene.primitives)?, original);
        Ok(())
    }
    fn font_pool_scene() -> Scene {
        let font = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../bead-test/fonts/IBMPlexSans.ttf");
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![font],
            width: 240.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 240., 40.),
            primitives: vec![Primitive::Text {
                rect: Rect::new(0., 0., 240., 32.),
                text: "office café".into(),
                family: "IBM Plex Sans".into(),
                size: 17.,
                color: [13, 27, 41, 255],
                weight: 400,
                align: None,
            }],
        }
    }
    #[test]
    fn text_opacity_multiplies_glyph_coverage() {
        let mut scene = font_pool_scene();
        let Primitive::Text { color, .. } = &mut scene.primitives[0] else {
            panic!("text")
        };
        color[3] = 102;
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.).unwrap();
        let pixmap = renderer
            .textures
            .get(&key(&scene.primitives[0], 1.))
            .unwrap();
        let maximum = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .max()
            .unwrap();
        assert!((100..=102).contains(&maximum), "alpha {maximum}");
    }
    #[test]
    fn caret_uses_existing_font_pool_with_exact_advances() {
        let scene = font_pool_scene();
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.).unwrap();
        let held = renderer.fonts.as_ref().unwrap().clone();
        assert!(std::sync::Arc::ptr_eq(&held, &scene_fonts(&scene).unwrap()));
        let mut fresh = load_scene_fonts(&scene).unwrap();
        let text = "office café";
        for boundary in text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
        {
            assert_eq!(
                text_boundary_x(&scene, text, boundary, "IBM Plex Sans", 17., 240.).unwrap(),
                boundary_x(&mut fresh, text, boundary, "IBM Plex Sans", 17., 240.).unwrap()
            );
        }
        let index = font_pool_index().lock().unwrap();
        assert!(
            index.iter().map(|item| item._charge.bytes()).sum::<usize>() <= FONT_POOL_MAX_BYTES
        );
        drop(index);
        drop(held);
        drop(renderer);
    }
    #[test]
    fn font_pool_is_shared_between_workers_and_releases_weakly() {
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bead-font-workers-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let guard = Temp(dir);
        let mut scene = font_pool_scene();
        let path = guard.0.join("shared.ttf");
        std::fs::copy(&scene.font_files[0], &path).unwrap();
        scene.font_files = vec![path];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let scene = scene.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    scene_fonts(&scene).unwrap()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let a = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert!(std::sync::Arc::ptr_eq(&a[0], &a[1]));
        let weak = std::sync::Arc::downgrade(&a[0]);
        drop(a);
        assert!(
            weak.upgrade().is_none(),
            "global index must not own font data"
        );
    }
    #[test]
    fn font_pool_index_invalidates_changed_file_metadata() {
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bead-font-index-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let guard = Temp(dir);
        let mut scene = font_pool_scene();
        let path = guard.0.join("fixture.ttf");
        std::fs::copy(&scene.font_files[0], &path).unwrap();
        scene.font_files = vec![path.clone()];
        let first = scene_fonts(&scene).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &scene_fonts(&scene).unwrap()
        ));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        let second = scene_fonts(&scene).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &second));
    }
    #[test]
    fn clipped_primitives_leave_pixels_outside_the_scroll_viewport_intact() {
        let scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: std::collections::BTreeMap::from([(1, Rect::new(0., 0., 2., 2.))]),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 4.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 4., 4.),
            primitives: vec![
                Primitive::Rounded {
                    rect: Rect::new(0., 0., 4., 4.),
                    radius: 0.,
                    fill: [0, 255, 0, 255],
                    border: None,
                },
                Primitive::Rounded {
                    rect: Rect::new(0., 0., 4., 4.),
                    radius: 0.,
                    fill: [255, 0, 0, 255],
                    border: None,
                },
            ],
        };
        let mut renderer = CpuRenderer::default();
        let mut bytes = vec![0; 64];
        renderer.prepare(&scene, 1.).unwrap();
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut bytes,
                    width: 4,
                    height: 4,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                Damage::full(4, 4),
            )
            .unwrap();
        assert_eq!(&bytes[0..4], &[255, 0, 0, 255]);
        assert_eq!(&bytes[12..16], &[0, 255, 0, 255]);
    }
    #[test]
    fn rejects_invalid_palette_versions_and_colors() {
        assert!(Palette::parse(br#"{"formatVersion":2,"colours":{}}"#).is_err());
        assert!(
            Palette::parse(br##"{"formatVersion":1,"colours":{"surface":"#ff0000"}}"##).is_err()
        );
        assert_eq!(
            Palette::parse(br#"{"formatVersion":1,"colours":{"surface":"ff0000"},"future":42}"#)
                .unwrap()
                .get("surface")
                .unwrap(),
            [255, 0, 0, 255]
        );
    }
    #[test]
    fn aged_partial_buffers_match_full_redraw_in_both_formats() {
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            let mut renderer = CpuRenderer::default();
            let mut reference = CpuRenderer::default();
            let mut buffers = [
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
            ];
            let mut pending = [Damage::full(120, 90); 3];
            let mut full = vec![0; 120 * 90 * 4];
            let mut previous = Rect::new(0., 0., 0., 0.);
            for frame in 0..30 {
                let mut scene = Scene {
                    opacity: 1.,
                    motion: None,
                    resource_revision: 0,
                    asset_data: Default::default(),
                    clips: Default::default(),
                    layers: Vec::new(),
                    prepared: Default::default(),
                    font_files: Vec::new(),
                    width: 40.,
                    slide: None,
                    offset: ((frame * 7 % 60) as f32, (frame * 3 % 30) as f32),
                    bounds: Rect::new(0., 0., 40., 30.),
                    primitives: vec![
                        Primitive::Rounded {
                            rect: Rect::new(1., 1., 38., 28.),
                            radius: 7.,
                            fill: [210, 20, 40, 180],
                            border: Some(([20, 190, 230, 255], 1.)),
                        },
                        Primitive::Text {
                            rect: Rect::new(3., 3., 34., 20.),
                            text: "A 😀".into(),
                            family: "sans-serif".into(),
                            weight: 400,
                            align: None,
                            size: 10.,
                            color: [20, 120, 210, 255],
                        },
                        Primitive::Image {
                            blur: 0.,
                            fit: crate::ImageFit::Fill,
                            clip_polygon: None,
                            rect: Rect::new(14., 8., 15., 15.),
                            path: Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../shell/sample.png"),
                        },
                        Primitive::Shadow {
                            rect: Rect::new(0., 0., 40., 30.),
                            radius: 4.,
                            blur: 2.,
                        },
                    ],
                };
                // Include fractional scaling and abrupt movement / erasure.
                if frame % 7 == 0 {
                    scene.offset = (-20., -10.);
                }
                let damage = previous.union(scene.bounds()).pixels(1.5, 120, 90);
                previous = scene.bounds();
                for p in &mut pending {
                    *p = p.union(damage);
                }
                let index = frame as usize % 3;
                renderer.prepare(&scene, 1.5).unwrap();
                reference.prepare(&scene, 1.5).unwrap();
                renderer
                    .render(
                        &scene,
                        ShmTarget {
                            bytes: &mut buffers[index],
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        pending[index],
                    )
                    .unwrap();
                pending[index] = Damage::full(0, 0);
                reference
                    .render(
                        &scene,
                        ShmTarget {
                            bytes: &mut full,
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        Damage::full(120, 90),
                    )
                    .unwrap();
                assert_eq!(buffers[index], full, "frame {frame}, {format:?}");
            }
        }
    }
    #[test]
    fn nine_patch_shadow_preserves_gaussian_appearance() {
        let rect = Rect::new(0., 0., 500., 300.);
        let scale = 1.5;
        let (w, h) = dimensions(rect, scale);
        let exact = svg_pixmap(shadow_svg(rect.w, rect.h, 12., 10.).as_bytes(), w, h).unwrap();
        let fast = shadow_pixmap(rect, 12., 10., scale).unwrap();
        let max = exact
            .data()
            .iter()
            .zip(fast.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 2, "shadow channel difference {max}/255");
    }
}

#[cfg(test)]
mod weight_tests {
    use super::*;
    #[test]
    fn weight_defaults_and_invalidates_text_rasters() {
        let json = r#"{"type":"text","rect":{"x":0,"y":0,"w":120,"h":24},"text":"Title","family":"sans-serif","size":16,"color":[0,0,0,255]}"#;
        let normal: Primitive = serde_json::from_str(json).unwrap();
        let mut medium = normal.clone();
        let Primitive::Text { weight, .. } = &mut medium else {
            panic!("text")
        };
        assert_eq!(*weight, 400);
        *weight = 500;
        assert_ne!(key(&normal, 1.), key(&medium, 1.));
    }
}

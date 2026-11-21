// SPDX-License-Identifier: MIT
//! Typed views and Taffy layout are independent of authoring syntax.
use crate::{Diagnostic, Node, frontend::Resolve};
use bead_core::Value;
use bead_render::{Palette, Primitive, Rect, Scene};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use taffy::prelude::*;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Panel,
    Row,
    Column,
    Grid,
    Scroll,
    Text,
    Button,
    TextInput,
    Image,
    Arc,
    Polyline,
    Group,
    Box,
    /// A graph canvas (spec 0232); its data lives in the session's graph engine.
    Graph,
}
impl Widget {
    pub fn constructors() -> [(&'static str, Self); 14] {
        [
            ("Panel", Self::Panel),
            ("Row", Self::Row),
            ("Column", Self::Column),
            ("Grid", Self::Grid),
            ("Scroll", Self::Scroll),
            ("Text", Self::Text),
            ("Button", Self::Button),
            ("TextInput", Self::TextInput),
            ("Image", Self::Image),
            ("Arc", Self::Arc),
            ("Polyline", Self::Polyline),
            ("Group", Self::Group),
            ("Box", Self::Box),
            ("Graph", Self::Graph),
        ]
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Source {
    pub file: String,
    pub line: usize,
}
impl Source {
    pub fn error(&self, msg: impl ToString) -> Diagnostic {
        Diagnostic::new(Path::new(&self.file), self.line, 1, msg)
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    pub kind: Widget,
    pub props: FxHashMap<String, Json>,
    pub children: Vec<RawNode>,
    pub source: Source,
    pub scope: Option<String>,
}
pub(crate) fn raw_bytes(raw: &RawNode) -> usize {
    std::mem::size_of::<RawNode>()
        + raw.source.file.capacity()
        + raw.scope.as_ref().map_or(0, String::capacity)
        + raw
            .props
            .iter()
            .map(|(k, v)| k.capacity() + 64 + bead_core::cache::json_bytes(v))
            .sum::<usize>()
        + raw.children.capacity() * std::mem::size_of::<RawNode>()
        + raw.children.iter().map(raw_bytes).sum::<usize>()
}
pub const PROPERTIES: &[&str] = &[
    "id",
    "metadata",
    "translate_x",
    "translate_y",
    "width",
    "height",
    "min_width",
    "min_height",
    "max_width",
    "max_height",
    "grow",
    "shrink",
    "gap",
    "gap_x",
    "gap_y",
    "padding",
    "padding_left",
    "padding_right",
    "padding_top",
    "padding_bottom",
    "align",
    "justify",
    "columns",
    "column_start",
    "column_span",
    "min_cell_width",
    "row_height",
    "text",
    "placeholder",
    "selected",
    "visible",
    "on_activate",
    "activate_on_release",
    "focus_on_pointer",
    "focus_ring",
    "preview_key",
    "preview_value",
    "preview_inset",
    "preview_source",
    "preview_expected",
    "preview_tolerance",
    "pointer_arg",
    "edge_hover",
    "scroll",
    "scroll_steps",
    "scrollbar",
    "fill",
    "radial",
    "color",
    "border_color",
    "border_width",
    "radius",
    "shadow",
    "body",
    "input",
    "join_edge",
    "join",
    "font_size",
    "font_weight",
    "font_features",
    "text_shadow_blur",
    "text_shadow_y",
    "text_shadow_color",
    "family",
    "icon",
    "tint",
    "path",
    "aria_label",
    "open",
    "transition",
    "duration",
    "return_duration",
    "key_repeat",
    "palette_tween",
    "return_transition",
    "slide_distance",
    "scale_from",
    "fade",
    "reveal",
    "edge",
    "direction",
    "drag",
    "drag_payload",
    "on_drag",
    "on_drop",
    "blur",
    "opacity",
    "scale",
    "start_angle",
    "sweep_angle",
    "stroke_width",
    "round_caps",
    "image_fit",
    "clip_polygon",
    "bake_size",
    "clip_children",
    "points",
    "closed",
    "text_align",
    "position",
    "left",
    "top",
    "right",
    "bottom",
    "hover_fill",
    "selected_fill",
    "pressed_fill",
    "press_scale",
    "visible_on_hover",
    "reflow",
    "multiline",
    "masked",
    "graph",
    "style",
    "highlight",
    "fit",
    "on_hover",
    "on_select",
];
pub fn allowed_property(name: &str) -> bool {
    static SORTED: std::sync::OnceLock<[&str; PROPERTIES.len()]> = std::sync::OnceLock::new();
    SORTED
        .get_or_init(|| {
            let mut names = [""; PROPERTIES.len()];
            names.copy_from_slice(PROPERTIES);
            names.sort_unstable();
            names
        })
        .binary_search(&name)
        .is_ok()
}

pub struct ViewSpec {
    pub id: String,
    pub metadata: Option<Value>,
    pub translate_x: f32,
    pub translate_y: f32,
    pub kind: Widget,
    pub style: Style,
    pub text: String,
    pub placeholder: String,
    pub selected: bool,
    pub visible: bool,
    pub action: Option<String>,
    pub argument: Option<Value>,
    pub activate_on_release: bool,
    pub focus_on_pointer: bool,
    /// Whether a focused interactive node gets the standard focus outline.
    pub focus_ring: bool,
    pub preview_key: String,
    pub preview_inset: f32,
    pub preview_value: f32,
    pub preview_source: Option<f32>,
    pub preview_expected: Option<f32>,
    pub preview_tolerance: f32,
    pub pointer_arg: Option<Value>,
    pub edge_hover: bool,
    pub input: Option<String>,
    /// TextInput accepts newlines (Shift+Enter) and wraps at its width.
    pub multiline: bool,
    /// TextInput value is drawn as bullets and kept out of accessibility, IME and copy.
    pub masked: bool,
    pub scroll: Option<String>,
    /// A non-Scroll node's `scroll = ui.action(..)`; the action takes the wheel delta.
    pub scroll_action: Option<String>,
    pub scroll_steps: Option<bead_render::ScrollSteps>,
    pub scrollbar: bool,
    pub fill: Option<Json>,
    /// The fill amount is an animation marker: draw the box even while it is transparent,
    /// so the paint patch has a primitive to recolour.
    pub animated_fill: bool,
    /// B0331: gradient layers over `fill`; geometry is checked, stop colours resolve at layout.
    pub radial: Vec<RadialSpec>,
    pub color: Option<Json>,
    pub border_color: Option<Json>,
    pub border_width: f32,
    pub radius: f32,
    pub shadow: Option<ShadowStyle>,
    /// Root `Panel` only: the id of the child that is the panel body (reveal, input region).
    pub body: Option<String>,
    /// Root only, `input = "children"` (B0321): the surface takes input on the root's direct children only.
    pub input_children: bool,
    /// Root `Panel` only (B0322): the held-key repeat schedule.
    pub key_repeat: Option<bead_render::KeyRepeat>,
    /// Root `Panel` only (B0333): `palette_tween = false` takes a wallpaper palette tween's final palette only.
    pub palette_at_rest: bool,
    /// The edge this box is attached to: its two corners there are square at `join` = 1.
    pub join_edge: Option<bead_core::Edge>,
    pub join: f32,
    pub blur: f32,
    /// 0..1 multiplier of the alpha of everything this view and its subtree draw (Qt Quick
    /// `Item.opacity`, B0349: no offscreen group, so overlaps show through; nested values multiply).
    /// The post-layout `materialized::apply_opacity` applies it, so layout emits no factor.
    pub opacity: f32,
    /// `scale` (B0364): this view and its subtree scale about its laid-out centre; nested values
    /// multiply. Post-layout, like `opacity`: `materialized::place_static` and the patch apply it.
    pub scale: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
    pub stroke_width: f32,
    pub round_caps: bool,
    pub image_fit: bead_render::ImageFit,
    pub clip_polygon: Option<Vec<[f32; 2]>>,
    /// `Image` only: bake the raster at this logical size and scale it to the rect.
    pub bake_size: Option<(f32, f32)>,
    pub clip_children: bool,
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
    pub drag: bool,
    pub drag_payload: Option<Value>,
    pub drag_action: Option<String>,
    pub drop_action: Option<String>,
    pub drop_argument: Option<Value>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_features: bead_render::FontFeatures,
    pub text_shadow_blur: f32,
    pub text_shadow_y: f32,
    pub text_shadow_color: Option<Json>,
    pub family: String,
    pub icon: Option<String>,
    pub tint: Option<Json>,
    pub path: Option<PathBuf>,
    pub aria_label: String,
    pub text_align: Option<bead_render::TextAlign>,
    pub hover_fill: Option<Json>,
    pub selected_fill: Option<Json>,
    /// Interaction poses (spec 0280), sampled per frame on the laid-out scene: the box fill and
    /// the subtree scale while this view or a descendant is pressed.
    pub pressed_fill: Option<Json>,
    pub press_scale: f32,
    /// The local id of an ancestor: this subtree fades in while the pointer or keyboard focus is in it.
    pub visible_on_hover: Option<String>,
    /// Seconds over which direct children slide to new positions after a relayout (0 = jump).
    pub reflow: f32,
    pub motion: Option<bead_render::Motion>,
    pub graph: Option<std::sync::Arc<crate::graph::GraphData>>,
    pub children: Vec<ViewSpec>,
    pub source: Source,
}
/// A node's authored properties with its bindings resolved on top.
struct Props<'a> {
    raw: &'a FxHashMap<String, Json>,
    resolved: FxHashMap<String, Json>,
}
impl Props<'_> {
    fn get(&self, name: &str) -> Option<&Json> {
        self.resolved.get(name).or_else(|| self.raw.get(name))
    }
    fn contains_key(&self, name: &str) -> bool {
        self.raw.contains_key(name)
    }
}
/// Per-build host presentation inputs, read once instead of once per node.
struct Build {
    default_family: String,
    text_style: crate::text_style::TextStyle,
    /// One clock per build: every `ui.animated` marker samples the same instant.
    now_ns: u64,
    /// Track points of the view's markers so far, capped at `TRACK_POINTS`.
    points: std::cell::Cell<usize>,
}
impl Build {
    /// A marker's value at this build's clock; its track points count toward the view's cap.
    fn sample(&self, marker: &Json, source: &Source) -> Result<f64, Diagnostic> {
        let marker =
            Marker::parse(marker).ok_or_else(|| source.error("invalid animation marker"))?;
        let points = self.points.get() + marker.points();
        if points > TRACK_POINTS {
            return Err(source.error(format!(
                "a view's animation tracks carry more than {TRACK_POINTS} points"
            )));
        }
        self.points.set(points);
        Ok(marker.sample(self.now_ns))
    }
}
/// Most keyframe points all the tracks of one view may carry (B0364).
pub(crate) const TRACK_POINTS: usize = 32768;
#[cfg(test)]
thread_local! {
    /// A test's fixed marker clock for both the layout and the patch.
    pub(crate) static PINNED_CLOCK: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}
/// `clock`, unless a test pinned the marker clock.
pub(crate) fn marker_clock(clock: u64) -> u64 {
    #[cfg(test)]
    let clock = PINNED_CLOCK.get().unwrap_or(clock);
    clock
}
/// The `ui.animated` marker inside a property value, if it is one.
pub(crate) fn marker(value: &Json) -> Option<&Json> {
    value.get("__animate")
}
/// A `ui.animated` marker parsed once (B0364): the driver's motion and, for a track, `keys`
/// (strictly increasing driver values) with one of `values` each, 2..=128 of them.
pub(crate) struct Marker {
    from: f64,
    to: f64,
    start: u64,
    duration: f64,
    curve: bead_core::animation::Curve,
    looping: bool,
    track: Option<Vec<[f64; 2]>>,
}
impl Marker {
    /// `None` when the marker or its track is malformed.
    pub(crate) fn parse(marker: &Json) -> Option<Self> {
        let number = |key: &str| marker.get(key).and_then(Json::as_f64);
        let list = |key: &str| marker.get(key)?.as_array();
        let track = if marker.get("keys").is_none() {
            None
        } else {
            let (keys, values) = (list("keys")?, list("values")?);
            let track = keys
                .iter()
                .zip(values)
                .map(|(k, v)| Some([k.as_f64()?, v.as_f64()?]))
                .collect::<Option<Vec<_>>>()?;
            // JSON numbers are always finite (non-finite ones serialise as null).
            if keys.len() != values.len()
                || !(2..=128).contains(&keys.len())
                || track.windows(2).any(|w| w[0][0] >= w[1][0])
            {
                return None;
            }
            Some(track)
        };
        Some(Self {
            from: number("from")?,
            to: number("to")?,
            start: marker.get("start").and_then(Json::as_u64)?,
            duration: number("duration")?,
            curve: bead_core::animation::Curve::parse(marker.get("curve")?.as_str()?)?,
            looping: marker.get("loop").and_then(Json::as_bool) == Some(true),
            track,
        })
    }
    /// The value at `now_ns`: the driver's eased sample, mapped piecewise-linearly through the
    /// track and clamped to its first and last values. Layout and patch share this one sampler.
    pub(crate) fn sample(&self, now_ns: u64) -> f64 {
        let mut t = now_ns.saturating_sub(self.start) as f64 / (self.duration.max(1e-9) * 1e9);
        if self.looping {
            t = t.rem_euclid(2.);
            t = if t > 1. { 2. - t } else { t };
        }
        let d =
            self.from + (self.to - self.from) * f64::from(self.curve.ease(t.clamp(0., 1.) as f32));
        let Some(track) = &self.track else {
            return d;
        };
        match track.partition_point(|[key, _]| *key <= d) {
            0 => track[0][1],
            i if i == track.len() => track[i - 1][1],
            i => {
                let ([k0, v0], [k1, v1]) = (track[i - 1], track[i]);
                v0 + (v1 - v0) * (d - k0) / (k1 - k0)
            }
        }
    }
    /// Whether the driver's motion still runs at `now_ns`.
    pub(crate) fn active(&self, now_ns: u64) -> bool {
        self.looping || now_ns < self.start.saturating_add((self.duration * 1e9) as u64)
    }
    /// The track's keyframe count (0 for a plain marker).
    pub(crate) fn points(&self) -> usize {
        self.track.as_ref().map_or(0, Vec::len)
    }
}
fn number(p: &Props<'_>, name: &str, default: f32, source: &Source) -> Result<f32, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= 20000.)
            .map(|n| n as f32)
            .ok_or_else(|| source.error(format!("{name} must be a finite number within 20000"))),
    }
}
/// `"tnum ss01, liga=0"`: 4 printable ASCII chars per tag, optional `=0..65535` (bare means 1), at most 16.
pub(crate) fn font_features(list: &str) -> Result<bead_render::FontFeatures, &'static str> {
    let mut features = Vec::new();
    for item in list.split([' ', ',']).filter(|item| !item.is_empty()) {
        let (tag, setting) = item.split_once('=').unwrap_or((item, "1"));
        let tag: [u8; 4] = tag
            .as_bytes()
            .try_into()
            .ok()
            .filter(|tag: &[u8; 4]| tag.iter().all(|b| (0x20..0x7f).contains(b)))
            .ok_or("tags are 4 printable ASCII characters")?;
        features.push((tag, setting.parse().map_err(|_| "values are 0..65535")?));
    }
    if features.len() > 16 {
        return Err("takes at most 16 tags");
    }
    Ok(features)
}
fn polygon(value: Option<&Json>, source: &Source) -> Result<Option<Vec<[f32; 2]>>, Diagnostic> {
    let Some(value) = value else { return Ok(None) };
    let points = value
        .as_array()
        .ok_or_else(|| source.error("clip_polygon must be an array"))?;
    if !(3..=32).contains(&points.len()) {
        return Err(source.error("clip_polygon needs 3..32 points"));
    }
    let mut result = Vec::with_capacity(points.len());
    for point in points {
        let pair = point
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| source.error("clip_polygon point needs x,y"))?;
        let x = pair[0]
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or_else(|| source.error("clip_polygon coordinates must be 0..1"))?
            as f32;
        let y = pair[1]
            .as_f64()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .ok_or_else(|| source.error("clip_polygon coordinates must be 0..1"))?
            as f32;
        result.push([x, y]);
    }
    Ok(Some(result))
}
fn bake_size(value: Option<&Json>, source: &Source) -> Result<Option<(f32, f32)>, Diagnostic> {
    let Some(value) = value else { return Ok(None) };
    let side = |key| {
        value
            .get(key)
            .and_then(Json::as_f64)
            .filter(|v| (1.0..=4096.0).contains(v))
            .ok_or_else(|| source.error("bake_size needs w and h within 1..4096"))
    };
    Ok(Some((side("w")? as f32, side("h")? as f32)))
}
fn text<'a>(
    p: &'a Props<'_>,
    name: &str,
    default: &'a str,
    source: &Source,
) -> Result<&'a str, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_str()
            .filter(|s| s.len() <= 65536)
            .ok_or_else(|| source.error(format!("{name} must be text"))),
    }
}
fn boolean(p: &Props<'_>, name: &str, default: bool, source: &Source) -> Result<bool, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| source.error(format!("{name} must be boolean"))),
    }
}
/// The `key_repeat` prop (B0322): `{delay, interval, min_interval, ramp_ms}` in whole ms, all optional.
fn key_repeat(
    p: &Props<'_>,
    source: &Source,
) -> Result<Option<bead_render::KeyRepeat>, Diagnostic> {
    let table = match p.get("key_repeat") {
        None => return Ok(None),
        Some(Json::Object(table)) => table,
        // An empty Luau table may arrive as an empty array: all defaults.
        Some(Json::Array(items)) if items.is_empty() => return Ok(Some(Default::default())),
        Some(_) => {
            return Err(
                source.error("key_repeat must be a table {delay, interval, min_interval, ramp_ms}")
            );
        }
    };
    let mut out = bead_render::KeyRepeat::default();
    for (key, value) in table {
        let (lo, hi) = match key.as_str() {
            "delay" | "ramp_ms" => (0, if key == "delay" { 2000 } else { 5000 }),
            "interval" | "min_interval" => (16, 1000),
            _ => return Err(source.error(format!("unknown key_repeat field {key}"))),
        };
        let ms = value
            .as_f64()
            .filter(|n| n.fract() == 0. && (f64::from(lo)..=f64::from(hi)).contains(n))
            .ok_or_else(|| {
                source.error(format!(
                    "key_repeat.{key} must be a whole number {lo}..{hi}"
                ))
            })? as u32;
        match key.as_str() {
            "delay" => out.delay = Some(ms),
            "interval" => out.interval = Some(ms),
            "min_interval" => out.min_interval = Some(ms),
            _ => out.ramp_ms = ms,
        }
    }
    if out
        .interval
        .zip(out.min_interval)
        .is_some_and(|(i, m)| m > i)
    {
        return Err(source.error("key_repeat.min_interval must not exceed interval"));
    }
    Ok(Some(out))
}
/// The `scroll_steps` prop (B0329): `{min_interval_ms, threshold}`, both optional.
fn scroll_steps(
    p: &Props<'_>,
    has_action: bool,
    source: &Source,
) -> Result<Option<bead_render::ScrollSteps>, Diagnostic> {
    let table = match p.get("scroll_steps") {
        None => return Ok(None),
        Some(_) if !has_action => {
            return Err(source.error("scroll_steps requires scroll = ui.action(..), not a binding"));
        }
        Some(Json::Object(table)) => table,
        // An empty Luau table may arrive as an empty array: all defaults.
        Some(Json::Array(items)) if items.is_empty() => return Ok(Some(Default::default())),
        Some(_) => {
            return Err(source.error("scroll_steps must be a table {min_interval_ms, threshold}"));
        }
    };
    let mut out = bead_render::ScrollSteps::default();
    for (key, value) in table {
        let (lo, hi) = match key.as_str() {
            "min_interval_ms" => (0., 2000.),
            "threshold" => (1., 1000.),
            _ => return Err(source.error(format!("unknown scroll_steps field {key}"))),
        };
        let n = value
            .as_f64()
            .filter(|n| (lo..=hi).contains(n))
            .ok_or_else(|| source.error(format!("scroll_steps.{key} must be {lo}..{hi}")))?;
        if key == "threshold" {
            out.threshold = n as f32;
        } else {
            out.min_interval_ms = n as u32;
        }
    }
    Ok(Some(out))
}
/// The `shadow` prop (B0320): `true` is the surface shadow (blur 10, spread 0, no offset, full strength).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowStyle {
    pub blur: f32,
    pub spread: f32,
    pub dy: f32,
    pub alpha: f32,
}
pub struct RadialSpec {
    geometry: [f32; 4],
    stops: Vec<(f32, Json)>,
}
/// The `radial` prop (B0331): up to 6 layers `{cx, cy, rx, ry, stops = {{offset, color}, ...}}`.
fn radial_layers(p: &Props<'_>, source: &Source) -> Result<Vec<RadialSpec>, Diagnostic> {
    let layers = match p.get("radial") {
        None => return Ok(Vec::new()),
        Some(Json::Array(layers)) if layers.len() <= 6 => layers,
        Some(_) => return Err(source.error("radial must be a list of 1..6 layer tables")),
    };
    let mut out = Vec::with_capacity(layers.len());
    for (n, layer) in layers.iter().enumerate() {
        let n = n + 1;
        let Json::Object(table) = layer else {
            return Err(source.error(format!(
                "radial[{n}] must be a table {{cx, cy, rx, ry, stops}}"
            )));
        };
        if let Some(key) = table
            .keys()
            .find(|k| !matches!(k.as_str(), "cx" | "cy" | "rx" | "ry" | "stops"))
        {
            return Err(source.error(format!("unknown radial[{n}] field {key}")));
        }
        let mut geometry = [0.; 4];
        for (slot, (key, range)) in geometry.iter_mut().zip([
            ("cx", -1.0..=2.),
            ("cy", -1.0..=2.),
            ("rx", 0.0..=4.),
            ("ry", 0.0..=4.),
        ]) {
            let radius = key.starts_with('r');
            *slot = table
                .get(key)
                .and_then(Json::as_f64)
                .filter(|v| range.contains(v) && !(radius && *v == 0.))
                .ok_or_else(|| {
                    source.error(format!(
                        "radial[{n}].{key} must be a number {}",
                        if radius { "above 0 up to 4" } else { "-1..2" }
                    ))
                })? as f32;
        }
        let bad = || {
            source.error(format!("radial[{n}].stops must be 2..8 entries {{offset 0..1, color}} with increasing offsets"))
        };
        let Some(Json::Array(list)) = table.get("stops") else {
            return Err(bad());
        };
        if !(2..=8).contains(&list.len()) {
            return Err(bad());
        }
        let mut stops = Vec::with_capacity(list.len());
        let mut previous = -1.;
        for stop in list {
            let [at, color] = stop.as_array().map(Vec::as_slice).unwrap_or_default() else {
                return Err(bad());
            };
            let at = at
                .as_f64()
                .filter(|at| (0. ..=1.).contains(at) && *at > previous)
                .ok_or_else(bad)?;
            previous = at;
            stops.push((at as f32, color.clone()));
        }
        out.push(RadialSpec { geometry, stops });
    }
    Ok(out)
}
fn shadow_style(
    p: &Props<'_>,
    default: bool,
    source: &Source,
) -> Result<Option<ShadowStyle>, Diagnostic> {
    let mut style = ShadowStyle {
        blur: bead_render::SURFACE_SHADOW.1,
        spread: 0.,
        dy: 0.,
        alpha: 1.,
    };
    let table = match p.get("shadow") {
        None => return Ok(default.then_some(style)),
        Some(Json::Bool(on)) => return Ok(on.then_some(style)),
        Some(Json::Object(table)) => table,
        // An empty Luau table may arrive as an empty array: all defaults.
        Some(Json::Array(items)) if items.is_empty() => return Ok(Some(style)),
        Some(_) => {
            return Err(source.error("shadow must be boolean or a table {blur, spread, dy, alpha}"));
        }
    };
    for (key, value) in table {
        let (field, range) = match key.as_str() {
            "blur" => (&mut style.blur, 0.0..=64.),
            "spread" => (&mut style.spread, -32.0..=64.),
            "dy" => (&mut style.dy, -64.0..=64.),
            "alpha" => (&mut style.alpha, 0.0..=1.),
            _ => return Err(source.error(format!("unknown shadow field {key}"))),
        };
        *field = value
            .as_f64()
            .map(|n| n as f32)
            .filter(|n| range.contains(n))
            .ok_or_else(|| {
                source.error(format!(
                    "shadow.{key} must be a number {}..{}",
                    range.start(),
                    range.end()
                ))
            })?;
    }
    if style.blur * 3. + style.spread < 0. {
        return Err(source.error("shadow.spread must not be below -3 * blur"));
    }
    Ok(Some(style))
}
fn dimension<T: FromLength + FromPercent + TaffyAuto>(
    p: &Props<'_>,
    name: &str,
    default: T,
    source: &Source,
) -> Result<T, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(Json::String(s)) if s == "auto" => Ok(auto()),
        Some(Json::String(s)) if s == "fill" => Ok(percent(1.0)),
        Some(Json::String(s)) if s.ends_with('%') => s[..s.len() - 1]
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0. && *n <= 100.)
            .map(|n| percent(n / 100.))
            .ok_or_else(|| source.error(format!("invalid percentage {name}"))),
        Some(v) => v
            .as_f64()
            .filter(|n| {
                n.is_finite()
                    && n.abs() <= 20000.
                    && (*n >= 0. || matches!(name, "left" | "right" | "top" | "bottom"))
            })
            .map(|n| length(n as f32))
            .ok_or_else(|| source.error(format!("{name} needs pixels, percentage, fill or auto"))),
    }
}
/// Colour properties whose expressions may carry a bound amount (`ui.alpha(role, ui.bind(..))`).
const COLOR_PROPERTIES: [&str; 6] = [
    "fill",
    "color",
    "border_color",
    "hover_fill",
    "selected_fill",
    "pressed_fill",
];
/// A copy of an expression with every nested binding replaced by its value; `None` when it has none.
fn resolve_nested(
    value: &Json,
    resolve: &Resolve<'_>,
    source: &Source,
    host: &Build,
) -> Result<Option<Json>, Diagnostic> {
    let Json::Object(object) = value else {
        return Ok(None);
    };
    let mut out = object.clone();
    let mut changed = false;
    for (key, inner) in object {
        if let Some(path) = binding(Some(inner)) {
            out.insert(
                key.clone(),
                serde_json::to_value(resolve(path)?).map_err(|e| source.error(e))?,
            );
            changed = true;
        } else if let Some(marker) = marker(inner) {
            out.insert(key.clone(), Json::from(host.sample(marker, source)?));
            changed = true;
        } else if let Some(next) = resolve_nested(inner, resolve, source, host)? {
            out.insert(key.clone(), next);
            changed = true;
        }
    }
    Ok(changed.then_some(Json::Object(out)))
}
fn binding(value: Option<&Json>) -> Option<&str> {
    value.and_then(|v| v.get("__bind")).and_then(Json::as_str)
}
impl ViewSpec {
    pub fn from_raw(raw: &RawNode, resolve: &Resolve<'_>) -> Result<Self, Diagnostic> {
        #[allow(clippy::too_many_arguments)] // Recursive descent shares one fixed build context.
        fn build(
            raw: &RawNode,
            resolve: &Resolve<'_>,
            host: &Build,
            scope: &str,
            position: usize,
            path: &str,
            seen: &mut BTreeSet<String>,
            count: &mut usize,
        ) -> Result<ViewSpec, Diagnostic> {
            *count += 1;
            if *count > 8192 {
                return Err(raw.source.error("view exceeds 8192 nodes"));
            }
            let mut props = Props {
                raw: &raw.props,
                resolved: FxHashMap::default(),
            };
            for (name, value) in &raw.props {
                if !allowed_property(name) {
                    return Err(raw.source.error(format!("unknown property {name}")));
                }
                if let Some(path) = binding(Some(value)) {
                    props.resolved.insert(
                        name.clone(),
                        serde_json::to_value(resolve(path)?).map_err(|e| raw.source.error(e))?,
                    );
                } else if let Some(marker) = marker(value) {
                    props
                        .resolved
                        .insert(name.clone(), Json::from(host.sample(marker, &raw.source)?));
                } else if COLOR_PROPERTIES.contains(&name.as_str())
                    && let Some(resolved) = resolve_nested(value, resolve, &raw.source, host)?
                {
                    props.resolved.insert(name.clone(), resolved);
                }
            }
            let p = &props;
            let source = &raw.source;
            if matches!(
                raw.kind,
                Widget::Button | Widget::TextInput | Widget::Scroll
            ) && !p.contains_key("id")
            {
                return Err(source.error("interactive nodes require stable explicit id"));
            }
            let scope = match raw.scope.as_deref() {
                Some(own) => format!("{scope}{own}"),
                None => scope.to_owned(),
            };
            let id = if p.contains_key("id") {
                format!("{scope}{}", text(p, "id", "", source)?)
            } else {
                format!("{path}._layout{position}")
            };
            if id.len() > 2048 || !seen.insert(id.clone()) {
                return Err(source.error(format!("duplicate or too long view id {id}")));
            }
            let font_size = number(p, "font_size", 16., source)?;
            if !(1. ..=256.).contains(&font_size) {
                return Err(source.error("font_size must be 1..256"));
            }
            let font_weight = number(p, "font_weight", 400., source)?;
            if !(1. ..=1000.).contains(&font_weight) || font_weight.fract() != 0. {
                return Err(source.error("font_weight must be an integer 1..1000"));
            }
            let font_weight = font_weight as u16;
            let font_features = font_features(text(p, "font_features", "", source)?)
                .map_err(|reason| source.error(format!("font_features {reason}")))?;
            let opacity = number(p, "opacity", 1., source)?;
            if !(0. ..=1.).contains(&opacity) {
                return Err(source.error("opacity must be 0..1"));
            }
            let scale = number(p, "scale", 1., source)?;
            if scale <= 0. || scale > 8. {
                return Err(source.error("scale must be above 0 and at most 8"));
            }
            let text_shadow_blur = number(p, "text_shadow_blur", 0., source)?;
            if !(0. ..=32.).contains(&text_shadow_blur) {
                return Err(source.error("text_shadow_blur must be 0..32"));
            }
            let text_shadow_y = number(p, "text_shadow_y", 0., source)?;
            if !(-32. ..=32.).contains(&text_shadow_y) {
                return Err(source.error("text_shadow_y must be -32..32"));
            }
            let start_angle = number(p, "start_angle", -90., source)?;
            let sweep_angle = number(p, "sweep_angle", 360., source)?;
            let stroke_width = number(p, "stroke_width", 4., source)?;
            let points: Vec<[f32; 2]> = if raw.kind == Widget::Polyline {
                let empty = Json::Array(Vec::new());
                let value = p.get("points").unwrap_or(&empty);
                if !value.as_array().is_some_and(|items| items.len() <= 256) {
                    return Err(
                        source.error("Polyline points must be an array of at most 256 pairs")
                    );
                }
                let parsed = Vec::<[f32; 2]>::deserialize(value)
                    .map_err(|_| source.error("Polyline points must be [x,y] pairs"))?;
                if parsed
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
                {
                    return Err(source.error(
                        "Polyline points must contain at most 256 finite normalized pairs",
                    ));
                }
                parsed
            } else {
                Vec::new()
            };
            let closed = boolean(p, "closed", false, source)?;
            if raw.kind == Widget::Polyline && !(0. ..=64.).contains(&stroke_width) {
                return Err(source.error("Polyline stroke_width must be within 0..64"));
            }
            if raw.kind == Widget::Arc
                && (!(-360. ..=360.).contains(&start_angle)
                    || !(-360. ..=360.).contains(&sweep_angle)
                    || !(0. ..=64.).contains(&stroke_width)
                    || stroke_width == 0.)
            {
                return Err(source
                    .error("Arc angles must be within -360..360 and stroke_width within 1..64"));
            }
            let family = host
                .text_style
                .family(text(p, "family", &host.default_family, source)?);
            let font_size = host.text_style.size(font_size, &family);
            let visible = boolean(p, "visible", true, source)?;
            let mut style = Style {
                position: Position::Relative,
                display: if !visible {
                    Display::None
                } else if raw.kind == Widget::Grid {
                    Display::Grid
                } else {
                    Display::Flex
                },
                flex_direction: if raw.kind == Widget::Row {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                min_size: Size {
                    width: length(0.),
                    height: length(0.),
                },
                ..Style::default()
            };
            if let Some(direction) = p.get("direction") {
                style.flex_direction = match direction.as_str() {
                    Some("row") => FlexDirection::Row,
                    Some("row-reverse") => FlexDirection::RowReverse,
                    Some("column") => FlexDirection::Column,
                    Some("column-reverse") => FlexDirection::ColumnReverse,
                    _ => {
                        return Err(
                            source.error("direction needs row/row-reverse/column/column-reverse")
                        );
                    }
                };
            }
            let leaf_height = match raw.kind {
                Widget::Text => font_size * 1.5,
                Widget::Button | Widget::TextInput => 40.,
                Widget::Image => 32.,
                _ => 0.,
            };
            if p.contains_key("position") {
                style.position = match text(p, "position", "relative", source)? {
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    _ => return Err(source.error("position must be relative/absolute")),
                };
            }
            style.inset = taffy::Rect {
                left: dimension(p, "left", auto(), source)?,
                right: dimension(p, "right", auto(), source)?,
                top: dimension(p, "top", auto(), source)?,
                bottom: dimension(p, "bottom", auto(), source)?,
            };
            style.size = Size {
                width: dimension(
                    p,
                    "width",
                    if raw.kind == Widget::Text {
                        percent(1.)
                    } else {
                        auto()
                    },
                    source,
                )?,
                height: dimension(
                    p,
                    "height",
                    if leaf_height > 0. {
                        length(leaf_height)
                    } else {
                        auto()
                    },
                    source,
                )?,
            };
            style.min_size = Size {
                width: dimension(p, "min_width", length(0.), source)?,
                height: dimension(p, "min_height", length(0.), source)?,
            };
            style.max_size = Size {
                width: dimension(p, "max_width", auto(), source)?,
                height: dimension(p, "max_height", auto(), source)?,
            };
            style.flex_grow = number(p, "grow", 0., source)?;
            style.flex_shrink =
                number(p, "shrink", if leaf_height > 0. { 0. } else { 1. }, source)?;
            if style.flex_grow < 0. || style.flex_shrink < 0. {
                return Err(source.error("grow/shrink cannot be negative"));
            }
            let pad = number(
                p,
                "padding",
                if raw.kind == Widget::Button || raw.kind == Widget::TextInput {
                    8.
                } else {
                    0.
                },
                source,
            )?;
            style.padding = taffy::Rect {
                left: length(number(p, "padding_left", pad, source)?),
                right: length(number(p, "padding_right", pad, source)?),
                top: length(number(p, "padding_top", pad, source)?),
                bottom: length(number(p, "padding_bottom", pad, source)?),
            };
            let gap = number(p, "gap", 0., source)?;
            if pad < 0. || gap < 0. {
                return Err(source.error("padding/gap cannot be negative"));
            }
            style.gap = Size {
                width: length(number(p, "gap_x", gap, source)?),
                height: length(number(p, "gap_y", gap, source)?),
            };
            style.align_items = match text(p, "align", "stretch", source)? {
                "start" => AlignItems::START,
                "center" => AlignItems::CENTER,
                "end" => AlignItems::END,
                "stretch" => AlignItems::STRETCH,
                _ => return Err(source.error("align needs start/center/end/stretch")),
            };
            style.justify_content = match text(p, "justify", "start", source)? {
                "start" => AlignContent::START,
                "center" => AlignContent::CENTER,
                "end" => AlignContent::END,
                "space-between" => AlignContent::SPACE_BETWEEN,
                _ => return Err(source.error("unsupported justify")),
            };
            if p.contains_key("column_start") || p.contains_key("column_span") {
                let start = number(p, "column_start", 1., source)?;
                let columns = number(p, "column_span", 1., source)?;
                if !(1. ..=4096.).contains(&start)
                    || start.fract() != 0.
                    || !(1. ..=4096.).contains(&columns)
                    || columns.fract() != 0.
                    || start + columns > 4097.
                {
                    return Err(source
                        .error("column_start/column_span need integer 1..4096 within the grid"));
                }
                style.grid_column = taffy::geometry::Line {
                    start: line(start as i16),
                    end: span(columns as u16),
                };
                style.grid_row = taffy::geometry::Line {
                    start: line(1),
                    end: span(1),
                };
            }
            if raw.kind == Widget::Grid {
                if p.contains_key("min_cell_width") {
                    let minimum = number(p, "min_cell_width", 116., source)?;
                    if minimum <= 0. {
                        return Err(source.error("min_cell_width must be positive"));
                    }
                    style.grid_template_columns = vec![repeat(
                        RepetitionCount::AutoFit,
                        vec![minmax(length(minimum), fr(1.))],
                    )];
                } else {
                    let columns = number(p, "columns", 3., source)?;
                    if !(1. ..=4096.).contains(&columns) || columns.fract() != 0. {
                        return Err(source.error("columns needs integer 1..4096"));
                    }
                    style.grid_template_columns =
                        std::iter::repeat_n(fr(1.), columns as usize).collect();
                }
                let height = number(p, "row_height", 96., source)?;
                if height <= 0. {
                    return Err(source.error("row_height must be positive"));
                }
                style.grid_auto_rows = vec![length(height)];
            }
            let input = if raw.kind == Widget::TextInput {
                Some(
                    binding(raw.props.get("text"))
                        .ok_or_else(|| {
                            source.error("TextInput.text requires a mutable store binding")
                        })?
                        .into(),
                )
            } else {
                None
            };
            let scroll = if raw.kind == Widget::Scroll {
                Some(
                    binding(raw.props.get("scroll"))
                        .ok_or_else(|| {
                            source.error("Scroll.scroll requires a mutable number store binding")
                        })?
                        .into(),
                )
            } else {
                None
            };
            // Any other node may take `scroll = ui.action(..)`; a binding there stays inert.
            let scroll_action = match p.get("scroll").and_then(|v| v.get("name")) {
                Some(name) if raw.kind != Widget::Scroll => Some(
                    name.as_str()
                        .filter(|n| !n.is_empty() && n.len() <= 128)
                        .ok_or_else(|| source.error("scroll action name must be 1 to 128 bytes"))?
                        .to_owned(),
                ),
                _ => None,
            };
            let scroll_steps = scroll_steps(p, scroll_action.is_some(), source)?;
            // A graph's `on_activate` takes a node id from its canvas, not the generic action.
            let action = p
                .get("on_activate")
                .filter(|_| raw.kind != Widget::Graph)
                .map(|v| {
                    let name = v
                        .get("name")
                        .and_then(Json::as_str)
                        .ok_or_else(|| source.error("on_activate requires ui.action"))?;
                    let argument = v
                        .get("argument")
                        .cloned()
                        .map(serde_json::from_value)
                        .transpose()
                        .map_err(|e| source.error(e))?;
                    Ok::<_, Diagnostic>((name.to_owned(), argument))
                })
                .transpose()?;
            let drag_payload = p
                .get("drag_payload")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| source.error(e))?;
            let parse_action =
                |property: &str| -> Result<Option<(String, Option<Value>)>, Diagnostic> {
                    p.get(property)
                        .map(|value| {
                            let name =
                                value.get("name").and_then(Json::as_str).ok_or_else(|| {
                                    source.error(format!("{property} requires ui.action"))
                                })?;
                            if name.is_empty() || name.len() > 128 {
                                return Err(source.error(format!(
                                    "{property} action name must be 1 to 128 bytes"
                                )));
                            }
                            let argument = value
                                .get("argument")
                                .cloned()
                                .map(serde_json::from_value)
                                .transpose()
                                .map_err(|e| source.error(e))?;
                            Ok((name.to_owned(), argument))
                        })
                        .transpose()
                };
            let graph = if raw.kind == Widget::Graph {
                Some(std::sync::Arc::new(crate::graph::GraphData::parse(
                    &|name| p.get(name),
                    &parse_action,
                    source,
                )?))
            } else {
                None
            };
            let on_drag = parse_action("on_drag")?;
            let on_drop = parse_action("on_drop")?;
            for value in [
                drag_payload.as_ref(),
                on_drag.as_ref().and_then(|action| action.1.as_ref()),
                on_drop.as_ref().and_then(|action| action.1.as_ref()),
            ]
            .into_iter()
            .flatten()
            {
                if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > 64 * 1024) {
                    return Err(source.error("drag transfer metadata exceeds 64 KiB"));
                }
            }
            let activate_on_release = boolean(p, "activate_on_release", false, source)?;
            let focus_on_pointer = boolean(p, "focus_on_pointer", true, source)?;
            if activate_on_release && raw.kind != Widget::Button {
                return Err(source.error("activate_on_release is only valid on Button"));
            }
            if p.contains_key("focus_on_pointer") && raw.kind != Widget::Button {
                return Err(source.error("focus_on_pointer is only valid on Button"));
            }
            if p.contains_key("shadow")
                && !matches!(
                    raw.kind,
                    Widget::Panel
                        | Widget::Box
                        | Widget::Row
                        | Widget::Column
                        | Widget::Grid
                        | Widget::Button
                )
            {
                return Err(source.error(
                    "shadow is only valid on a surface: Panel, Box, Row, Column, Grid or Button",
                ));
            }
            let input_children = match p.get("input") {
                None => false,
                Some(v) if v == "children" && path == "view" => true,
                Some(_) => {
                    return Err(source.error("input is only valid on the root, as \"children\""));
                }
            };
            if p.contains_key("key_repeat") && (raw.kind != Widget::Panel || path != "view") {
                return Err(source.error("key_repeat is only valid on the root Panel"));
            }
            if p.contains_key("palette_tween") && (raw.kind != Widget::Panel || path != "view") {
                return Err(source.error("palette_tween is only valid on the root Panel"));
            }
            if p.contains_key("body") && (raw.kind != Widget::Panel || path != "view") {
                return Err(source.error("body is only valid on the root Panel"));
            }
            let press_scale = number(p, "press_scale", 1., source)?;
            let reflow = number(p, "reflow", 0., source)?;
            if !(0.5..=1.5).contains(&press_scale) || !(0. ..=2.).contains(&reflow) {
                return Err(source.error("press_scale needs 0.5..1.5 and reflow 0..2 seconds"));
            }
            let join = number(p, "join", 1., source)?;
            if !(0. ..=1.).contains(&join) {
                return Err(source.error("join needs 0..1"));
            }
            let child_scope = if p.contains_key("id") && raw.kind != Widget::Group {
                format!("{id}.")
            } else {
                scope.clone()
            };
            let mut children = Vec::new();
            for (i, child) in raw.children.iter().enumerate() {
                let mut child = build(child, resolve, host, &child_scope, i, &id, seen, count)?;
                if raw.kind == Widget::Panel && child.style.size.height.is_auto() {
                    child.style.flex_grow = 1.;
                }
                if child.kind == Widget::Group {
                    children.extend(child.children);
                } else {
                    children.push(child);
                }
            }
            let content = text(p, "text", "", source)?.to_owned();
            if raw.kind == Widget::Text && !p.contains_key("text") {
                return Err(source.error("Text requires text"));
            }
            if raw.kind == Widget::Button && !p.contains_key("text") && children.is_empty() {
                return Err(source.error("Button requires text or content children"));
            }
            Ok(ViewSpec {
                id: id.clone(),
                kind: raw.kind,
                style,
                text: content,
                placeholder: text(p, "placeholder", "", source)?.to_owned(),
                selected: boolean(p, "selected", false, source)?,
                visible,
                action: action.as_ref().map(|a| a.0.clone()),
                argument: action.and_then(|a| a.1),
                translate_x: number(p, "translate_x", 0., source)?,
                translate_y: number(p, "translate_y", 0., source)?,
                metadata: p
                    .get("metadata")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| source.error(error))?,
                activate_on_release,
                focus_on_pointer,
                focus_ring: boolean(p, "focus_ring", true, source)?,
                preview_key: text(p, "preview_key", "", source)?.to_owned(),
                preview_value: number(p, "preview_value", 0., source)?,
                preview_inset: number(p, "preview_inset", 0., source)?.max(0.),
                preview_source: p
                    .get("preview_source")
                    .map(|_| number(p, "preview_source", 0., source))
                    .transpose()?,
                preview_expected: p
                    .get("preview_expected")
                    .map(|_| number(p, "preview_expected", 0., source))
                    .transpose()?,
                preview_tolerance: number(p, "preview_tolerance", 0., source)?.max(0.),
                pointer_arg: p
                    .get("pointer_arg")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| source.error(e))?,
                edge_hover: boolean(p, "edge_hover", false, source)?,
                input,
                multiline: boolean(p, "multiline", false, source)?,
                masked: boolean(p, "masked", false, source)?,
                scroll,
                scroll_action,
                scroll_steps,
                scrollbar: boolean(p, "scrollbar", true, source)?,
                fill: p.get("fill").cloned(),
                animated_fill: raw
                    .props
                    .get("fill")
                    .and_then(|v| v.get("alpha"))
                    .and_then(marker)
                    .is_some(),
                radial: radial_layers(p, source)?,
                color: p.get("color").cloned(),
                border_color: p.get("border_color").cloned(),
                drag: boolean(p, "drag", false, source)?,
                drag_payload,
                drag_action: on_drag.as_ref().map(|a| a.0.clone()),
                drop_action: on_drop.as_ref().map(|a| a.0.clone()),
                drop_argument: on_drop.and_then(|a| a.1),
                border_width: number(p, "border_width", 0., source)?,
                blur: number(p, "blur", 0., source)?.clamp(0., 32.),
                opacity,
                scale,
                start_angle,
                sweep_angle,
                stroke_width,
                round_caps: boolean(p, "round_caps", true, source)?,
                image_fit: match text(p, "image_fit", "fill", source)? {
                    "fill" => bead_render::ImageFit::Fill,
                    "cover" => bead_render::ImageFit::Cover,
                    _ => return Err(source.error("image_fit must be fill or cover")),
                },
                clip_polygon: polygon(p.get("clip_polygon"), source)?,
                bake_size: bake_size(p.get("bake_size"), source)?,
                clip_children: boolean(p, "clip_children", false, source)?,
                points,
                closed,
                radius: number(
                    p,
                    "radius",
                    if raw.kind == Widget::Panel { 12. } else { 9. },
                    source,
                )?,
                shadow: shadow_style(p, raw.kind == Widget::Panel, source)?,
                body: p
                    .get("body")
                    .map(|_| text(p, "body", "", source).map(|id| format!("{child_scope}{id}")))
                    .transpose()?,
                input_children,
                key_repeat: key_repeat(p, source)?,
                palette_at_rest: !boolean(p, "palette_tween", true, source)?,
                join_edge: p
                    .get("join_edge")
                    .map(|value| {
                        serde_json::from_value(value.clone())
                            .map_err(|_| source.error("join_edge needs top/bottom/left/right"))
                    })
                    .transpose()?,
                join,
                font_size,
                font_weight,
                font_features,
                text_shadow_blur,
                text_shadow_y,
                text_shadow_color: p.get("text_shadow_color").cloned(),
                family,
                icon: p
                    .get("icon")
                    .map(|_| text(p, "icon", "", source).map(str::to_owned))
                    .transpose()?,
                tint: p.get("tint").cloned(),
                path: p
                    .get("path")
                    .map(|_| text(p, "path", "", source).map(PathBuf::from))
                    .transpose()?,
                aria_label: text(p, "aria_label", "", source)?.to_owned(),
                motion: if p.contains_key("open") {
                    Some(bead_render::Motion {
                        slide_distance: if p.contains_key("slide_distance") {
                            let value = number(p, "slide_distance", 0., source)?;
                            if !(0. ..=4000.).contains(&value) {
                                return Err(
                                    source.error("slide_distance needs 0..4000 logical pixels")
                                );
                            }
                            Some(value)
                        } else {
                            None
                        },
                        scale_from: if p.contains_key("scale_from") {
                            let value = number(p, "scale_from", 1., source)?;
                            if !(0.5..=1.).contains(&value) {
                                return Err(source.error("scale_from needs 0.5..1"));
                            }
                            if boolean(p, "reveal", false, source)? {
                                return Err(source.error("scale_from cannot combine with reveal"));
                            }
                            Some(value)
                        } else {
                            None
                        },
                        id: id.clone(),
                        fade: boolean(p, "fade", true, source)?,
                        reveal: boolean(p, "reveal", false, source)?,
                        progress: 1.,
                        parts: Vec::new(),
                        art: None,
                        body: None,
                        open: boolean(p, "open", true, source)?,
                        // Absent: the panel profile's open duration, set once the layout knows the travel.
                        auto_open: !p.contains_key("duration"),
                        duration: f64::from(number(p, "duration", 0.2, source)?),
                        return_duration: if p.contains_key("return_duration") {
                            let value = number(p, "return_duration", 0., source)?;
                            if !(0. ..=60.).contains(&value) {
                                return Err(source.error("return_duration needs 0..60 seconds"));
                            }
                            Some(f64::from(value))
                        } else {
                            None
                        },
                        return_curve: if p.contains_key("return_transition") {
                            Some(match text(p, "return_transition", "ease", source)? {
                                "ease" => bead_core::animation::Curve::EaseOut,
                                "ease-in-out" => bead_core::animation::Curve::EaseInOut,
                                "spring" => bead_core::animation::Curve::Spring,
                                _ => {
                                    return Err(source
                                        .error("return_transition needs ease/ease-in-out/spring"));
                                }
                            })
                        } else {
                            None
                        },
                        spring: text(p, "transition", "ease", source)? == "spring",
                        curve: (text(p, "transition", "ease", source)? == "ease-in-out")
                            .then_some(bead_core::animation::Curve::EaseInOut),
                        edge: serde_json::from_value(
                            p.get("edge")
                                .cloned()
                                .unwrap_or_else(|| Json::String("top".into())),
                        )
                        .map_err(|_| source.error("edge needs top/bottom/left/right"))?,
                    })
                } else {
                    None
                },
                text_align: p
                    .get("text_align")
                    .map(|v| {
                        serde_json::from_value(v.clone())
                            .map_err(|_| source.error("text_align needs left/center/right"))
                    })
                    .transpose()?,
                hover_fill: p.get("hover_fill").cloned(),
                selected_fill: p.get("selected_fill").cloned(),
                pressed_fill: p.get("pressed_fill").cloned(),
                press_scale,
                visible_on_hover: p
                    .get("visible_on_hover")
                    .map(|_| text(p, "visible_on_hover", "", source).map(str::to_owned))
                    .transpose()?,
                reflow,
                graph,
                children,
                source: raw.source.clone(),
            })
        }
        let host = Build {
            default_family: crate::theme::with_sans_family(str::to_owned),
            text_style: crate::text_style::current(),
            now_ns: marker_clock(bead_core::animation::build_clock()),
            points: Default::default(),
        };
        build(
            raw,
            resolve,
            &host,
            "",
            0,
            "view",
            &mut BTreeSet::new(),
            &mut 0,
        )
    }
}
fn first_label(view: &ViewSpec) -> String {
    view.children
        .iter()
        .find_map(|child| {
            if !child.text.is_empty() {
                Some(child.text.clone())
            } else {
                let label = first_label(child);
                (!label.is_empty()).then_some(label)
            }
        })
        .unwrap_or_default()
}
pub struct LaidOut {
    pub scene: Scene,
    pub nodes: Vec<Node>,
}
pub(super) fn color(
    value: Option<&Json>,
    default: [u8; 4],
    palette: &Palette,
    source: &Source,
) -> Result<[u8; 4], Diagnostic> {
    match value {
        None => Ok(default),
        Some(Json::String(role)) if role == "transparent" => Ok([0, 0, 0, 0]),
        Some(Json::String(role)) => palette.get(role).map_err(|e| source.error(e)),
        Some(Json::Object(object)) if object.contains_key("role") => {
            let role = object
                .get("role")
                .ok_or_else(|| source.error("color role must be text"))?;
            let alpha = object
                .get("alpha")
                .and_then(Json::as_f64)
                .filter(|a| (0. ..=1.).contains(a))
                .ok_or_else(|| source.error("alpha needs 0..1"))?;
            let mut c = match role {
                Json::String(role) => palette.get(role).map_err(|e| source.error(e))?,
                // A tint or another alpha expression: its own alpha is scaled by this one.
                Json::Object(_) => color(Some(role), default, palette, source)?,
                _ => return Err(source.error("color role must be text")),
            };
            c[3] = (f64::from(c[3]) * alpha).round() as u8;
            Ok(c)
        }
        Some(Json::Object(object)) if object.contains_key("base") => {
            let base = object
                .get("base")
                .ok_or_else(|| source.error("base color is required"))?;
            let over = object
                .get("over")
                .ok_or_else(|| source.error("overlay color is required"))?;
            let alpha = object
                .get("alpha")
                .and_then(Json::as_f64)
                .filter(|a| (0. ..=1.).contains(a))
                .ok_or_else(|| source.error("tint amount needs 0..1"))?;
            let a = color(Some(base), default, palette, source)?;
            let b = color(Some(over), default, palette, source)?;
            Ok(std::array::from_fn(|i| {
                (f64::from(a[i]) * (1. - alpha) + f64::from(b[i]) * alpha).round() as u8
            }))
        }
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| source.error("color requires a palette role or RGBA array")),
    }
}
/// The box fill and text colour of a view in its idle or hovered state: the only
/// outputs of a layout that depend on `bead.hovered`.
pub(super) fn surface_colors(
    view: &ViewSpec,
    hovered: bool,
    palette: &Palette,
) -> Result<([u8; 4], [u8; 4]), Diagnostic> {
    let palette_role = |name: &str, default| palette.get(name).unwrap_or(default);
    let body = palette_role("surface", [24, 24, 24, 255]);
    let ink = palette_role("onSurface", [240, 240, 240, 255]);
    // Only a focusable view takes the pointer's hover; a `visible_on_hover` group's id paints nothing.
    let hovered =
        hovered && (view.action.is_some() || view.input.is_some() || view.graph.is_some());
    let selected = view.selected || hovered;
    let fill = color(
        if hovered {
            view.hover_fill.as_ref().or(view.fill.as_ref())
        } else if view.selected {
            view.selected_fill.as_ref().or(view.fill.as_ref())
        } else {
            view.fill.as_ref()
        },
        if view.kind == Widget::Panel {
            body
        } else if selected || view.kind == Widget::TextInput {
            palette_role("surfaceContainer", body)
        } else {
            [0, 0, 0, 0]
        },
        palette,
        &view.source,
    )?;
    let foreground = color(
        view.color.as_ref(),
        if selected {
            palette_role("primary", ink)
        } else {
            ink
        },
        palette,
        &view.source,
    )?;
    Ok((fill, foreground))
}
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
}
#[cfg(feature = "test-counters")]
std::thread_local! { static LAYOUT_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Thread-local actual Taffy passes; independent tests do not race on a global count.
#[cfg(feature = "test-counters")]
pub fn layout_passes() -> usize {
    LAYOUT_PASSES.get()
}
pub fn layout(
    view: &ViewSpec,
    resolve: &Resolve<'_>,
    palette: &Palette,
    revision: u64,
    viewport: (f32, f32),
) -> Result<LaidOut, Diagnostic> {
    #[cfg(feature = "test-counters")]
    LAYOUT_PASSES.set(LAYOUT_PASSES.get().wrapping_add(1));
    let mut tree = TaffyTree::<()>::new();
    let mut ids = BTreeMap::new();
    fn add<'a>(
        view: &'a ViewSpec,
        tree: &mut TaffyTree<()>,
        ids: &mut BTreeMap<&'a str, NodeId>,
    ) -> Result<NodeId, Diagnostic> {
        let children = view
            .children
            .iter()
            .map(|child| add(child, tree, ids))
            .collect::<Result<Vec<_>, _>>()?;
        let id = tree
            .new_with_children(view.style.clone(), &children)
            .map_err(|e| view.source.error(e))?;
        ids.insert(view.id.as_str(), id);
        Ok(id)
    }
    let root = add(view, &mut tree, &mut ids)?;
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(viewport.0),
            height: AvailableSpace::Definite(viewport.1),
        },
    )
    .map_err(|e| view.source.error(e))?;
    let root_size = tree.layout(root).map_err(|e| view.source.error(e))?.size;
    if root_size.width < 1.
        || root_size.height < 1.
        || root_size.width > 4000.
        || root_size.height > 4000.
    {
        return Err(view.source.error("panel size must be 1..4000"));
    }
    let mut output = LaidOut {
        scene: Scene {
            opacity: 1.,
            motion: None,
            resource_revision: revision,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: root_size.width,
            slide: None,
            primitives: vec![],
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., root_size.width + 64., root_size.height + 64.),
            input_islands: None,
            key_repeat: None,
            palette_at_rest: false,
        },
        nodes: vec![],
    };
    output.scene.motion = view.motion.clone();
    output.scene.key_repeat = view.key_repeat;
    output.scene.palette_at_rest = view.palette_at_rest;
    let hover = resolve("bead.hovered")
        .ok()
        .and_then(|v| {
            if let Value::Text(s) = v {
                Some(s)
            } else {
                None
            }
        })
        .unwrap_or_default();
    #[allow(clippy::too_many_arguments)] // Recursive traversal shares one fixed render context.
    fn draw(
        view: &ViewSpec,
        tree: &TaffyTree<()>,
        ids: &BTreeMap<&str, NodeId>,
        parent: (f32, f32),
        parent_layer: Option<&str>,
        clip: Option<Rect>,
        retain_offscreen: bool,
        scroll_store: Option<&str>,
        scroll_limit: Option<f64>,
        resolve: &Resolve<'_>,
        palette: &Palette,
        hover: &str,
        out: &mut LaidOut,
    ) -> Result<(), Diagnostic> {
        if !view.visible {
            return Ok(());
        }
        let l = tree
            .layout(ids[view.id.as_str()])
            .map_err(|e| view.source.error(e))?;
        let r = Rect::new(
            parent.0 + l.location.x + view.translate_x,
            parent.1 + l.location.y + view.translate_y,
            l.size.width,
            l.size.height,
        );
        let own_visible = r.w > 0. && r.h > 0. && !clip.is_some_and(|c| intersect(c, r).is_none());
        // Containers may have overflowing/absolute descendants outside their
        // assigned box. Cull leaves; retain traversal through such containers.
        if !own_visible && view.children.is_empty() && !retain_offscreen {
            return Ok(());
        }
        let layer_index = out.scene.layers.len();
        let own_start = out.scene.primitives.len();
        out.scene.layers.push(bead_render::SceneLayer {
            id: view.id.clone(),
            parent: parent_layer.map(str::to_owned),
            own: own_start..own_start,
            subtree_end: own_start,
            bounds: r,
        });
        let source = &view.source;
        let palette_role = |name: &str, default| palette.get(name).unwrap_or(default);
        let ink = palette_role("onSurface", [240, 240, 240, 255]);
        let muted = palette_role("onSurfaceVariant", ink);
        let accent = palette_role("primary", ink);
        let (fill, foreground) = surface_colors(view, hover == view.id, palette)?;
        // A joined edge: the box (and its shadow, `join_margin` = 30) reaches `join * radius`
        // behind that edge and is clipped at it, so its two corners there are square and flush
        // with the surface it hangs from.
        let joined = view.join_edge.map(|edge| (edge, view.join * view.radius));
        let mut emit = |primitive: Primitive, join_margin: Option<f32>| {
            if !own_visible && !retain_offscreen {
                return;
            }
            let index = out.scene.primitives.len();
            let (primitive, bounds) = match (joined, join_margin) {
                (Some((edge, extent)), Some(margin)) => {
                    let at_edge = joined_clip(r, edge, margin);
                    let bounds = clip.map_or(at_edge, |c| {
                        intersect(c, at_edge).unwrap_or(Rect::new(0., 0., 0., 0.))
                    });
                    (joined_rect(primitive, edge, extent), Some(bounds))
                }
                _ => (primitive, clip),
            };
            out.scene.primitives.push(primitive);
            if let Some(c) = bounds {
                out.scene.clips.insert(index, c);
            }
        };
        if let Some(s) = view.shadow {
            // The raster is the shadow's own extent: 3 sigma of blur plus spread, moved down by dy.
            let margin = s.blur * 3. + s.spread;
            emit(
                Primitive::Shadow {
                    rect: Rect::new(
                        r.x - margin,
                        r.y - margin + s.dy,
                        r.w + 2. * margin,
                        r.h + 2. * margin,
                    ),
                    radius: view.radius + s.spread,
                    blur: s.blur,
                    spread: s.spread,
                    dy: s.dy,
                    alpha: s.alpha,
                },
                Some(margin + s.dy.abs()),
            );
        }
        let border = (view.border_width > 0. || view.selected)
            .then(|| {
                (
                    color(
                        view.border_color.as_ref(),
                        palette_role("outlineVariant", muted),
                        palette,
                        source,
                    ),
                    view.border_width.max(if view.selected { 1. } else { 0. }),
                )
            })
            .map(|(c, w)| c.map(|c| (c, w)))
            .transpose()?;
        let radial = if view.radial.is_empty() {
            None
        } else {
            Some(
                view.radial
                    .iter()
                    .map(|layer| {
                        let [cx, cy, rx, ry] = layer.geometry;
                        let stops = layer
                            .stops
                            .iter()
                            .map(|(at, c)| Ok((*at, color(Some(c), [0; 4], palette, source)?)))
                            .collect::<Result<_, Diagnostic>>()?;
                        Ok(bead_render::RadialLayer {
                            cx,
                            cy,
                            rx,
                            ry,
                            stops,
                        })
                    })
                    .collect::<Result<std::sync::Arc<[_]>, Diagnostic>>()?,
            )
        };
        if view.kind != Widget::Polyline
            && (fill[3] > 0 || border.is_some() || view.animated_fill || radial.is_some())
        {
            emit(
                Primitive::Rounded {
                    rect: r,
                    radius: view.radius,
                    fill,
                    border,
                    radial,
                },
                Some(0.),
            );
        }
        if view.kind == Widget::Text
            || (!view.text.is_empty() && matches!(view.kind, Widget::Button | Widget::TextInput))
            || view.kind == Widget::TextInput
        {
            let padding = l.padding;
            let input_empty = view.kind == Widget::TextInput && view.text.is_empty();
            emit(
                Primitive::Text {
                    rect: Rect::new(
                        r.x + padding.left,
                        r.y + padding.top,
                        (r.w - padding.left - padding.right).max(1.),
                        (r.h - padding.top - padding.bottom).max(1.),
                    ),
                    text: if input_empty {
                        view.placeholder.clone()
                    } else if view.masked && view.input.is_some() {
                        crate::input::mask(&view.text)
                    } else {
                        view.text.clone()
                    },
                    family: view.family.clone(),
                    weight: view.font_weight,
                    features: view.font_features.clone(),
                    align: view.text_align,
                    size: view.font_size,
                    color: if input_empty { muted } else { foreground },
                    shadow: if view.text_shadow_blur > 0. {
                        Some(bead_render::TextShadow {
                            color: color(
                                view.text_shadow_color.as_ref(),
                                [0, 0, 0, 128],
                                palette,
                                source,
                            )?,
                            blur: view.text_shadow_blur,
                            offset_y: view.text_shadow_y,
                        })
                    } else {
                        None
                    },
                },
                None,
            );
        }
        if view.kind == Widget::Image {
            let path = if let Some(icon) = &view.icon {
                PathBuf::from(format!("@icon:{icon}"))
            } else {
                view.path
                    .clone()
                    .ok_or_else(|| source.error("Image requires icon or path"))?
            };
            emit(
                Primitive::Image {
                    blur: view.blur,
                    fit: view.image_fit,
                    clip_polygon: view.clip_polygon.clone(),
                    bake_size: view.bake_size,
                    radius: view.radius,
                    tint: view
                        .tint
                        .as_ref()
                        .map(|tint| color(Some(tint), foreground, palette, source))
                        .transpose()?,
                    opacity: 1.,
                    rect: r,
                    path,
                },
                None,
            );
        }
        if view.kind == Widget::Arc {
            let arc_color = color(view.color.as_ref(), accent, palette, source)?;
            emit(
                Primitive::Arc {
                    rect: r,
                    start_angle: view.start_angle,
                    sweep_angle: view.sweep_angle,
                    stroke_width: view.stroke_width,
                    color: arc_color,
                    round_caps: view.round_caps,
                },
                None,
            );
        }
        if view.kind == Widget::Polyline {
            let line_color = color(view.color.as_ref(), accent, palette, source)?;
            let fill = view
                .fill
                .as_ref()
                .map(|v| color(Some(v), accent, palette, source))
                .transpose()?;
            emit(
                Primitive::Polyline {
                    rect: r,
                    points: view.points.clone(),
                    closed: view.closed,
                    stroke_width: view.stroke_width,
                    color: line_color,
                    round_caps: view.round_caps,
                    fill,
                },
                None,
            );
        }
        if let Some(graph) = &view.graph {
            // Geometry, camera and interaction are filled in by the session's graph engine.
            emit(
                Primitive::Graph {
                    rect: r,
                    geometry: Default::default(),
                    camera: Default::default(),
                    style: graph.style(palette, source)?,
                    hover: None,
                    selected: None,
                    focus: None,
                },
                None,
            );
        }
        out.scene.layers[layer_index].own.end = out.scene.primitives.len();
        let inherited_scroll = view.scroll.as_deref().or(scroll_store);
        let scroll_limit = if view.scroll.is_some() {
            Some(f64::from(l.scroll_height()))
        } else {
            scroll_limit
        };
        if (own_visible || retain_offscreen)
            && (view.action.is_some()
                || view.input.is_some()
                || view.scroll.is_some()
                || view.scroll_action.is_some()
                || view.drag_action.is_some()
                || view.drag_payload.is_some()
                || view.drop_action.is_some()
                || view.metadata.is_some()
                || view.graph.is_some())
        {
            out.nodes.push(Node {
                id: view.id.clone(),
                graph: view.graph.clone(),
                metadata: view.metadata.clone(),
                source: Some((PathBuf::from(&source.file), source.line, 1)),
                draw: Primitive::Rounded {
                    rect: r,
                    radius: 0.,
                    fill: [0, 0, 0, 0],
                    border: None,
                    radial: None,
                },
                bind: BTreeMap::new(),
                visible: None,
                action: view.action.clone(),
                activate_on_release: view.activate_on_release,
                focus_on_pointer: view.focus_on_pointer,
                focus_ring: view.focus_ring,
                preview_key: view.preview_key.clone(),
                preview_value: view.preview_value,
                preview_inset: view.preview_inset,
                preview_source: view.preview_source,
                preview_expected: view.preview_expected,
                preview_tolerance: view.preview_tolerance,
                pointer_arg: view.pointer_arg.clone(),
                drag_payload: view.drag_payload.clone(),
                drag_action: view.drag_action.clone(),
                drop_action: view.drop_action.clone(),
                drop_argument: view.drop_argument.clone(),
                edge_hover: view.edge_hover,
                icon: None,
                input: view.input.clone(),
                scroll: view.scroll_action.clone(),
                scroll_steps: view.scroll_steps,
                action_arg: view.argument.clone(),
                scroll_store: inherited_scroll.map(str::to_owned),
                scrollable: view.scroll.is_some(),
                scrollbar: view.scroll.is_some() && view.scrollbar,
                scroll_limit,
                focusable: view.action.is_some() || view.input.is_some() || view.graph.is_some(),
                multiline: view.multiline && view.input.is_some(),
                masked: view.masked && view.input.is_some(),
                drag: view.drag,
                clip,
                hit_polygon: view.clip_polygon.clone(),
                label: if !view.aria_label.is_empty() {
                    view.aria_label.clone()
                } else if view.masked && view.input.is_some() {
                    view.placeholder.clone()
                } else if !view.text.is_empty() {
                    view.text.clone()
                } else if view.input.is_some() {
                    view.placeholder.clone()
                } else {
                    first_label(view)
                },
            });
        }
        let mut children_pos = (r.x, r.y);
        let mut child_clip = if view.clip_children {
            Some(clip.map_or(r, |clip| {
                intersect(clip, r).unwrap_or(Rect::new(0., 0., 0., 0.))
            }))
        } else {
            clip
        };
        if let Some(store) = &view.scroll {
            let Value::Number(offset) = resolve(store)? else {
                return Err(source.error("scroll must bind a number"));
            };
            children_pos.1 -= offset.clamp(0., scroll_limit.unwrap_or(f64::MAX)) as f32;
            child_clip = Some(if let Some(c) = child_clip {
                c.intersect(r)
            } else {
                r
            });
        }
        for child in &view.children {
            draw(
                child,
                tree,
                ids,
                children_pos,
                Some(&view.id),
                child_clip,
                retain_offscreen || view.scroll.is_some(),
                inherited_scroll,
                scroll_limit,
                resolve,
                palette,
                hover,
                out,
            )?;
        }
        out.scene.layers[layer_index].subtree_end = out.scene.primitives.len();
        Ok(())
    }
    draw(
        view,
        &tree,
        &ids,
        (0., 0.),
        None,
        None,
        false,
        None,
        None,
        resolve,
        palette,
        &hover,
        &mut output,
    )?;
    if let Some(body) = &view.body {
        // The named child is the panel body: the reveal grows it from its edge, its box and
        // shadow are the parts that keep their corners while growing, and input stops at it.
        let layer = output
            .scene
            .layers
            .iter()
            .find(|layer| &layer.id == body)
            .ok_or_else(|| view.source.error("body names no visible child"))?;
        let parts = layer
            .own
            .clone()
            .filter(|i| {
                matches!(
                    output.scene.primitives[*i],
                    Primitive::Rounded { .. } | Primitive::Shadow { .. }
                )
            })
            .map(|i| (i, output.scene.primitives[i].clone()))
            .collect();
        let bounds = layer.bounds;
        if let Some(motion) = &mut output.scene.motion {
            motion.body = Some(bounds);
            motion.parts = parts;
        }
    }
    if view.input_children {
        output.scene.input_islands = Some(input_islands(&output, &view.id));
    }
    output.scene.settle_open_duration();
    Ok(output)
}
/// Islands, not leaves (B0321): the rect of each direct child of the root that, or whose descendant,
/// is interactive or paints something. Empty spacers drop out; the gaps between islands stay open.
fn input_islands(out: &LaidOut, root: &str) -> Vec<Rect> {
    let layers = &out.scene.layers;
    let mut rects = Vec::new();
    for (i, layer) in layers.iter().enumerate() {
        if layer.parent.as_deref() != Some(root) {
            continue;
        }
        let mut subtree = std::collections::BTreeSet::from([layer.id.as_str()]);
        for later in &layers[i + 1..] {
            if later.parent.as_deref().is_some_and(|p| subtree.contains(p)) {
                subtree.insert(later.id.as_str());
            }
        }
        if layer.own.start < layer.subtree_end
            || out.nodes.iter().any(|n| subtree.contains(n.id.as_str()))
        {
            rects.push(layer.bounds);
        }
    }
    rects
}
/// `primitive` grown by `extent` behind `edge`: the part that `joined_clip` cuts off.
fn joined_rect(mut primitive: Primitive, edge: bead_core::Edge, extent: f32) -> Primitive {
    if let Primitive::Rounded { rect, .. } | Primitive::Shadow { rect, .. } = &mut primitive {
        use bead_core::Edge;
        match edge {
            Edge::Top => {
                rect.y -= extent;
                rect.h += extent;
            }
            Edge::Bottom => rect.h += extent,
            Edge::Left => {
                rect.x -= extent;
                rect.w += extent;
            }
            Edge::Right => rect.w += extent,
        }
    }
    primitive
}
/// `r` plus `margin` on every side but the joined edge, which stays flush.
fn joined_clip(r: Rect, edge: bead_core::Edge, margin: f32) -> Rect {
    use bead_core::Edge;
    let mut c = Rect::new(
        r.x - margin,
        r.y - margin,
        r.w + 2. * margin,
        r.h + 2. * margin,
    );
    match edge {
        Edge::Top => {
            c.y = r.y;
            c.h -= margin;
        }
        Edge::Bottom => c.h -= margin,
        Edge::Left => {
            c.x = r.x;
            c.w -= margin;
        }
        Edge::Right => c.w -= margin,
    }
    c
}

#[cfg(test)]
mod scrollbar_property_tests {
    use super::*;

    #[test]
    fn scroll_bar_opt_out_is_boolean_and_defaults_on() {
        let source = Source {
            file: "test.luau".into(),
            line: 1,
        };
        let mut raw = RawNode {
            kind: Widget::Scroll,
            props: serde_json::from_value(serde_json::json!({
                "id":"list", "scroll":{"__bind":"scroll"}
            }))
            .unwrap(),
            children: vec![],
            source: source.clone(),
            scope: None,
        };
        let resolve = |_: &str| Ok(Value::Number(0.));
        assert!(ViewSpec::from_raw(&raw, &resolve).unwrap().scrollbar);
        raw.props.insert("scrollbar".into(), Json::Bool(false));
        assert!(!ViewSpec::from_raw(&raw, &resolve).unwrap().scrollbar);
        raw.props
            .insert("scrollbar".into(), Json::String("no".into()));
        assert!(ViewSpec::from_raw(&raw, &resolve).is_err());
    }
    #[test]
    fn prop_insertion_order_does_not_change_the_scene() {
        let props = [
            ("id", serde_json::json!("box")),
            ("width", serde_json::json!(120.)),
            ("height", serde_json::json!(40.)),
            ("padding", serde_json::json!(6.)),
            ("gap", serde_json::json!(3.)),
            ("radius", serde_json::json!(5.)),
            ("fill", serde_json::json!({"role": "surface", "alpha": 1.})),
        ];
        let scene = |order: &mut dyn Iterator<Item = &(&str, Json)>| {
            let raw = RawNode {
                kind: Widget::Panel,
                props: order.map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
                children: vec![],
                source: Source {
                    file: "test.luau".into(),
                    line: 1,
                },
                scope: None,
            };
            let resolve = |_: &str| Ok(Value::Number(0.));
            let view = ViewSpec::from_raw(&raw, &resolve).unwrap();
            let palette = Palette(std::sync::Arc::new(
                [("surface".to_owned(), [1, 2, 3, 255])].into(),
            ));
            let laid = layout(&view, &resolve, &palette, 1, (400., 300.)).unwrap();
            serde_json::to_string(&laid.scene.primitives).unwrap()
        };
        let forward = scene(&mut props.iter());
        assert!(forward.contains("rounded"));
        assert_eq!(forward, scene(&mut props.iter().rev()));
    }
}

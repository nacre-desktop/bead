// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed views and Taffy layout are independent of authoring syntax.
use crate::{Diagnostic, Node, frontend::Resolve};
use bead_core::Value;
use bead_render::{Palette, Primitive, Rect, Scene};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
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
}
impl Widget {
    pub fn constructors() -> [(&'static str, Self); 13] {
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
    pub props: Map<String, Json>,
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
    "fill",
    "color",
    "border_color",
    "border_width",
    "radius",
    "shadow",
    "font_size",
    "font_weight",
    "text_shadow_blur",
    "text_shadow_y",
    "text_shadow_color",
    "family",
    "icon",
    "path",
    "aria_label",
    "open",
    "transition",
    "duration",
    "return_duration",
    "return_transition",
    "slide_distance",
    "fade",
    "reveal",
    "edge",
    "direction",
    "drag",
    "drag_payload",
    "on_drag",
    "on_drop",
    "blur",
    "start_angle",
    "sweep_angle",
    "stroke_width",
    "round_caps",
    "image_fit",
    "clip_polygon",
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
    "multiline",
    "masked",
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
    pub fill: Option<Json>,
    pub color: Option<Json>,
    pub border_color: Option<Json>,
    pub border_width: f32,
    pub radius: f32,
    pub shadow: bool,
    pub blur: f32,
    pub start_angle: f32,
    pub sweep_angle: f32,
    pub stroke_width: f32,
    pub round_caps: bool,
    pub image_fit: bead_render::ImageFit,
    pub clip_polygon: Option<Vec<[f32; 2]>>,
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
    pub text_shadow_blur: f32,
    pub text_shadow_y: f32,
    pub text_shadow_color: Option<Json>,
    pub family: String,
    pub icon: Option<String>,
    pub path: Option<PathBuf>,
    pub aria_label: String,
    pub text_align: Option<bead_render::TextAlign>,
    pub hover_fill: Option<Json>,
    pub selected_fill: Option<Json>,
    pub motion: Option<bead_render::Motion>,
    pub children: Vec<ViewSpec>,
    pub source: Source,
}
fn number(
    p: &Map<String, Json>,
    name: &str,
    default: f32,
    source: &Source,
) -> Result<f32, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= 20000.)
            .map(|n| n as f32)
            .ok_or_else(|| source.error(format!("{name} must be a finite number within20000"))),
    }
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
fn text(
    p: &Map<String, Json>,
    name: &str,
    default: &str,
    source: &Source,
) -> Result<String, Diagnostic> {
    match p.get(name) {
        None => Ok(default.into()),
        Some(v) => v
            .as_str()
            .filter(|s| s.len() <= 65536)
            .map(str::to_owned)
            .ok_or_else(|| source.error(format!("{name} must be text"))),
    }
}
fn boolean(
    p: &Map<String, Json>,
    name: &str,
    default: bool,
    source: &Source,
) -> Result<bool, Diagnostic> {
    match p.get(name) {
        None => Ok(default),
        Some(v) => v
            .as_bool()
            .ok_or_else(|| source.error(format!("{name} must be boolean"))),
    }
}
fn dimension<T: FromLength + FromPercent + TaffyAuto>(
    p: &Map<String, Json>,
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
const COLOR_PROPERTIES: [&str; 5] = [
    "fill",
    "color",
    "border_color",
    "hover_fill",
    "selected_fill",
];
/// A copy of an expression with every nested binding replaced by its value; `None` when it has none.
fn resolve_nested(
    value: &Json,
    resolve: &Resolve<'_>,
    source: &Source,
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
        } else if let Some(next) = resolve_nested(inner, resolve, source)? {
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
        fn build(
            raw: &RawNode,
            resolve: &Resolve<'_>,
            scope: &str,
            position: usize,
            path: &str,
            seen: &mut BTreeSet<String>,
            count: &mut usize,
        ) -> Result<ViewSpec, Diagnostic> {
            *count += 1;
            if *count > 8192 {
                return Err(raw.source.error("view exceeds8192nodes"));
            }
            let mut props = std::borrow::Cow::Borrowed(&raw.props);
            for (name, value) in &raw.props {
                if !allowed_property(name) {
                    return Err(raw.source.error(format!("unknown property {name}")));
                }
                if let Some(path) = binding(Some(value)) {
                    props.to_mut().insert(
                        name.clone(),
                        serde_json::to_value(resolve(path)?).map_err(|e| raw.source.error(e))?,
                    );
                } else if COLOR_PROPERTIES.contains(&name.as_str())
                    && let Some(resolved) = resolve_nested(value, resolve, &raw.source)?
                {
                    props.to_mut().insert(name.clone(), resolved);
                }
            }
            let p = props.as_ref();
            let source = &raw.source;
            let local = text(p, "id", &format!("_layout{position}"), source)?;
            if matches!(
                raw.kind,
                Widget::Button | Widget::TextInput | Widget::Scroll
            ) && !p.contains_key("id")
            {
                return Err(source.error("interactive nodes require stable explicit id"));
            }
            let scope = format!("{}{}", scope, raw.scope.as_deref().unwrap_or(""));
            let id = if p.contains_key("id") {
                format!("{scope}{local}")
            } else {
                format!("{path}.{local}")
            };
            if id.len() > 2048 || !seen.insert(id.clone()) {
                return Err(source.error(format!("duplicate or too long view id {id}")));
            }
            let font_size = number(p, "font_size", 16., source)?;
            if !(1. ..=256.).contains(&font_size) {
                return Err(source.error("font_size must be1..256"));
            }
            let font_weight = number(p, "font_weight", 400., source)?;
            if !(1. ..=1000.).contains(&font_weight) || font_weight.fract() != 0. {
                return Err(source.error("font_weight must be an integer 1..1000"));
            }
            let font_weight = font_weight as u16;
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
                let value = p.get("points").cloned().unwrap_or(Json::Array(Vec::new()));
                if !value.as_array().is_some_and(|items| items.len() <= 256) {
                    return Err(
                        source.error("Polyline points must be an array of at most 256 pairs")
                    );
                }
                let parsed: Vec<[f32; 2]> = serde_json::from_value(value)
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
            let family = crate::text_style::family(&text(p, "family", "IBM Plex Sans", source)?);
            let font_size = crate::text_style::size(font_size, &family);
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
                style.position = match text(p, "position", "relative", source)?.as_str() {
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
            style.align_items = match text(p, "align", "stretch", source)?.as_str() {
                "start" => AlignItems::START,
                "center" => AlignItems::CENTER,
                "end" => AlignItems::END,
                "stretch" => AlignItems::STRETCH,
                _ => return Err(source.error("align needs start/center/end/stretch")),
            };
            style.justify_content = match text(p, "justify", "start", source)?.as_str() {
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
                        .error("column_start/column_span need integer1..4096 within the grid"));
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
                        return Err(source.error("columns needs integer1..4096"));
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
            let action = p
                .get("on_activate")
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
            if p.contains_key("shadow") && raw.kind != Widget::Panel {
                return Err(source.error("shadow is only valid on Panel"));
            }
            let child_scope = if p.contains_key("id") && raw.kind != Widget::Group {
                format!("{id}.")
            } else {
                scope.clone()
            };
            let mut children = Vec::new();
            for (i, child) in raw.children.iter().enumerate() {
                let mut child = build(child, resolve, &child_scope, i, &id, seen, count)?;
                if raw.kind == Widget::Panel && child.style.size.height.is_auto() {
                    child.style.flex_grow = 1.;
                }
                if child.kind == Widget::Group {
                    children.extend(child.children);
                } else {
                    children.push(child);
                }
            }
            let content = text(p, "text", "", source)?;
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
                placeholder: text(p, "placeholder", "", source)?,
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
                preview_key: text(p, "preview_key", "", source)?,
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
                fill: p.get("fill").cloned(),
                color: p.get("color").cloned(),
                border_color: p.get("border_color").cloned(),
                drag: boolean(p, "drag", false, source)?,
                drag_payload,
                drag_action: on_drag.as_ref().map(|a| a.0.clone()),
                drop_action: on_drop.as_ref().map(|a| a.0.clone()),
                drop_argument: on_drop.and_then(|a| a.1),
                border_width: number(p, "border_width", 0., source)?,
                blur: number(p, "blur", 0., source)?.clamp(0., 32.),
                start_angle,
                sweep_angle,
                stroke_width,
                round_caps: boolean(p, "round_caps", true, source)?,
                image_fit: match text(p, "image_fit", "fill", source)?.as_str() {
                    "fill" => bead_render::ImageFit::Fill,
                    "cover" => bead_render::ImageFit::Cover,
                    _ => return Err(source.error("image_fit must be fill or cover")),
                },
                clip_polygon: polygon(p.get("clip_polygon"), source)?,
                clip_children: boolean(p, "clip_children", false, source)?,
                points,
                closed,
                radius: number(
                    p,
                    "radius",
                    if raw.kind == Widget::Panel { 12. } else { 9. },
                    source,
                )?,
                shadow: boolean(p, "shadow", true, source)?,
                font_size,
                font_weight,
                text_shadow_blur,
                text_shadow_y,
                text_shadow_color: p.get("text_shadow_color").cloned(),
                family,
                icon: p
                    .get("icon")
                    .map(|_| text(p, "icon", "", source))
                    .transpose()?,
                path: p
                    .get("path")
                    .map(|_| text(p, "path", "", source).map(PathBuf::from))
                    .transpose()?,
                aria_label: text(p, "aria_label", "", source)?,
                motion: if p.contains_key("open") {
                    Some(bead_render::Motion {
                        slide_distance: if p.contains_key("slide_distance") {
                            let value = number(p, "slide_distance", 0., source)?;
                            if !(0. ..=4000.).contains(&value) {
                                return Err(
                                    source.error("slide_distance needs0..4000 logical pixels")
                                );
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
                        open: boolean(p, "open", true, source)?,
                        duration: f64::from(number(p, "duration", 0.2, source)?),
                        return_duration: if p.contains_key("return_duration") {
                            let value = number(p, "return_duration", 0., source)?;
                            if !(0. ..=60.).contains(&value) {
                                return Err(source.error("return_duration needs0..60 seconds"));
                            }
                            Some(f64::from(value))
                        } else {
                            None
                        },
                        return_curve: if p.contains_key("return_transition") {
                            Some(
                                match text(p, "return_transition", "ease", source)?.as_str() {
                                    "ease" => bead_core::animation::Curve::EaseOut,
                                    "ease-in-out" => bead_core::animation::Curve::EaseInOut,
                                    "spring" => bead_core::animation::Curve::Spring,
                                    _ => {
                                        return Err(source.error(
                                            "return_transition needs ease/ease-in-out/spring",
                                        ));
                                    }
                                },
                            )
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
                children,
                source: raw.source.clone(),
            })
        }
        build(raw, resolve, "", 0, "view", &mut BTreeSet::new(), &mut 0)
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
                .ok_or_else(|| source.error("alpha needs0..1"))?;
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
                .ok_or_else(|| source.error("tint amount needs0..1"))?;
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
    fn add(
        view: &ViewSpec,
        tree: &mut TaffyTree<()>,
        ids: &mut BTreeMap<String, NodeId>,
    ) -> Result<NodeId, Diagnostic> {
        let children = view
            .children
            .iter()
            .map(|child| add(child, tree, ids))
            .collect::<Result<Vec<_>, _>>()?;
        let id = tree
            .new_with_children(view.style.clone(), &children)
            .map_err(|e| view.source.error(e))?;
        ids.insert(view.id.clone(), id);
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
        return Err(view.source.error("panel size must be1..4000"));
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
        },
        nodes: vec![],
    };
    output.scene.motion = view.motion.clone();
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
        ids: &BTreeMap<String, NodeId>,
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
            .layout(ids[&view.id])
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
        let body = palette_role("surface", [24, 24, 24, 255]);
        let ink = palette_role("onSurface", [240, 240, 240, 255]);
        let muted = palette_role("onSurfaceVariant", ink);
        let accent = palette_role("primary", ink);
        let selected = view.selected || hover == view.id;
        let fill = color(
            if hover == view.id {
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
            source,
        )?;
        let foreground = color(
            view.color.as_ref(),
            if selected { accent } else { ink },
            palette,
            source,
        )?;
        let mut emit = |primitive: Primitive| {
            if !own_visible && !retain_offscreen {
                return;
            }
            let index = out.scene.primitives.len();
            out.scene.primitives.push(primitive);
            if let Some(c) = clip {
                out.scene.clips.insert(index, c);
            }
        };
        if view.kind == Widget::Panel && view.shadow {
            emit(Primitive::Shadow {
                rect: Rect::new(r.x - 30., r.y - 30., r.w + 60., r.h + 60.),
                radius: view.radius,
                blur: 10.,
            });
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
        if view.kind != Widget::Polyline && (fill[3] > 0 || border.is_some()) {
            emit(Primitive::Rounded {
                rect: r,
                radius: view.radius,
                fill,
                border,
            });
        }
        if view.kind == Widget::Text
            || (!view.text.is_empty() && matches!(view.kind, Widget::Button | Widget::TextInput))
            || view.kind == Widget::TextInput
        {
            let padding = l.padding;
            let input_empty = view.kind == Widget::TextInput && view.text.is_empty();
            emit(Primitive::Text {
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
            });
        }
        if view.kind == Widget::Image {
            let path = if let Some(icon) = &view.icon {
                PathBuf::from(format!("@icon:{icon}"))
            } else {
                view.path
                    .clone()
                    .ok_or_else(|| source.error("Image requires icon or path"))?
            };
            emit(Primitive::Image {
                blur: view.blur,
                fit: view.image_fit,
                clip_polygon: view.clip_polygon.clone(),
                rect: r,
                path,
            });
        }
        if view.kind == Widget::Arc {
            let arc_color = color(view.color.as_ref(), accent, palette, source)?;
            emit(Primitive::Arc {
                rect: r,
                start_angle: view.start_angle,
                sweep_angle: view.sweep_angle,
                stroke_width: view.stroke_width,
                color: arc_color,
                round_caps: view.round_caps,
            });
        }
        if view.kind == Widget::Polyline {
            let line_color = color(view.color.as_ref(), accent, palette, source)?;
            let fill = view
                .fill
                .as_ref()
                .map(|v| color(Some(v), accent, palette, source))
                .transpose()?;
            emit(Primitive::Polyline {
                rect: r,
                points: view.points.clone(),
                closed: view.closed,
                stroke_width: view.stroke_width,
                color: line_color,
                round_caps: view.round_caps,
                fill,
            });
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
                || view.drag_action.is_some()
                || view.drag_payload.is_some()
                || view.drop_action.is_some()
                || view.metadata.is_some())
        {
            out.nodes.push(Node {
                id: view.id.clone(),
                metadata: view.metadata.clone(),
                source: Some((PathBuf::from(&source.file), source.line, 1)),
                draw: Primitive::Rounded {
                    rect: r,
                    radius: 0.,
                    fill: [0, 0, 0, 0],
                    border: None,
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
                scroll: None,
                action_arg: view.argument.clone(),
                scroll_store: inherited_scroll.map(str::to_owned),
                scroll_limit,
                focusable: view.action.is_some() || view.input.is_some(),
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
    Ok(output)
}

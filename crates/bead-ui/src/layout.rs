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
    Group,
}
impl Widget {
    pub fn constructors() -> [(&'static str, Self); 10] {
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
            ("Group", Self::Group),
        ]
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Source {
    pub file: String,
    pub line: usize,
}
impl Source {
    pub fn error(&self, msg: impl ToString) -> Diagnostic {
        Diagnostic::new(Path::new(&self.file), self.line, 1, msg)
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    pub kind: Widget,
    pub props: Map<String, Json>,
    pub children: Vec<RawNode>,
    pub source: Source,
    pub scope: Option<String>,
}
pub fn allowed_property(name: &str) -> bool {
    [
        "id",
        "width",
        "height",
        "min_width",
        "min_height",
        "max_width",
        "max_height",
        "grow",
        "shrink",
        "gap",
        "padding",
        "padding_left",
        "padding_right",
        "padding_top",
        "padding_bottom",
        "align",
        "justify",
        "columns",
        "min_cell_width",
        "row_height",
        "text",
        "placeholder",
        "selected",
        "visible",
        "on_activate",
        "scroll",
        "fill",
        "color",
        "border_color",
        "border_width",
        "radius",
        "font_size",
        "family",
        "icon",
        "path",
        "aria_label",
    ]
    .contains(&name)
}
pub struct ViewSpec {
    pub id: String,
    pub kind: Widget,
    pub style: Style,
    pub text: String,
    pub placeholder: String,
    pub selected: bool,
    pub visible: bool,
    pub action: Option<String>,
    pub argument: Option<Value>,
    pub input: Option<String>,
    pub scroll: Option<String>,
    pub fill: Option<Json>,
    pub color: Option<Json>,
    pub border_color: Option<Json>,
    pub border_width: f32,
    pub radius: f32,
    pub font_size: f32,
    pub family: String,
    pub icon: Option<String>,
    pub path: Option<PathBuf>,
    pub aria_label: String,
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
            .filter(|n| n.is_finite() && *n >= 0. && *n <= 20000.)
            .map(|n| length(n as f32))
            .ok_or_else(|| source.error(format!("{name} needs pixels, percentage, fill or auto"))),
    }
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
            let mut props = raw.props.clone();
            for (name, value) in &mut props {
                if !allowed_property(name) {
                    return Err(raw.source.error(format!("unknown property {name}")));
                }
                if let Some(path) = binding(Some(value)) {
                    *value =
                        serde_json::to_value(resolve(path)?).map_err(|e| raw.source.error(e))?;
                }
            }
            let p = &props;
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
            let visible = boolean(p, "visible", true, source)?;
            let mut style = Style {
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
            let leaf_height = match raw.kind {
                Widget::Text => font_size * 1.5,
                Widget::Button | Widget::TextInput => 40.,
                Widget::Image => 32.,
                _ => 0.,
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
                width: length(gap),
                height: length(gap),
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
                    if !(1. ..=64.).contains(&columns) || columns.fract() != 0. {
                        return Err(source.error("columns needs integer1..64"));
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
                id,
                kind: raw.kind,
                style,
                text: content,
                placeholder: text(p, "placeholder", "", source)?,
                selected: boolean(p, "selected", false, source)?,
                visible,
                action: action.as_ref().map(|a| a.0.clone()),
                argument: action.and_then(|a| a.1),
                input,
                scroll,
                fill: p.get("fill").cloned(),
                color: p.get("color").cloned(),
                border_color: p.get("border_color").cloned(),
                border_width: number(p, "border_width", 0., source)?,
                radius: number(
                    p,
                    "radius",
                    if raw.kind == Widget::Panel { 12. } else { 9. },
                    source,
                )?,
                font_size,
                family: text(p, "family", "IBM Plex Sans", source)?,
                icon: p
                    .get("icon")
                    .map(|_| text(p, "icon", "", source))
                    .transpose()?,
                path: p
                    .get("path")
                    .map(|_| text(p, "path", "", source).map(PathBuf::from))
                    .transpose()?,
                aria_label: text(p, "aria_label", "", source)?,
                children,
                source: raw.source.clone(),
            })
        }
        build(raw, resolve, "", 0, "view", &mut BTreeSet::new(), &mut 0)
    }
}
pub struct LaidOut {
    pub scene: Scene,
    pub nodes: Vec<Node>,
}
fn color(
    value: Option<&Json>,
    default: [u8; 4],
    palette: &Palette,
    source: &Source,
) -> Result<[u8; 4], Diagnostic> {
    match value {
        None => Ok(default),
        Some(Json::String(role)) => palette.get(role).map_err(|e| source.error(e)),
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
pub fn layout(
    view: &ViewSpec,
    resolve: &Resolve<'_>,
    palette: &Palette,
    revision: u64,
    viewport: (f32, f32),
) -> Result<LaidOut, Diagnostic> {
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
            resource_revision: revision,
            asset_data: Default::default(),
            clips: Default::default(),
            font_files: vec![],
            width: root_size.width,
            slide: None,
            primitives: vec![],
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., root_size.width + 64., root_size.height + 64.),
        },
        nodes: vec![],
    };
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
        clip: Option<Rect>,
        scroll_store: Option<&str>,
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
            parent.0 + l.location.x,
            parent.1 + l.location.y,
            l.size.width,
            l.size.height,
        );
        if r.w <= 0. || r.h <= 0. || clip.is_some_and(|c| intersect(c, r).is_none()) {
            return Ok(());
        }
        let source = &view.source;
        let palette_role = |name: &str, default| palette.get(name).unwrap_or(default);
        let body = palette_role("surface", [24, 24, 24, 255]);
        let ink = palette_role("onSurface", [240, 240, 240, 255]);
        let muted = palette_role("onSurfaceVariant", ink);
        let accent = palette_role("primary", ink);
        let selected = view.selected || hover == view.id;
        let fill = color(
            view.fill.as_ref(),
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
            let index = out.scene.primitives.len();
            out.scene.primitives.push(primitive);
            if let Some(c) = clip {
                out.scene.clips.insert(index, c);
            }
        };
        if view.kind == Widget::Panel {
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
        if fill[3] > 0 || border.is_some() {
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
                } else {
                    view.text.clone()
                },
                family: view.family.clone(),
                size: view.font_size,
                color: if input_empty { muted } else { foreground },
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
            emit(Primitive::Image { rect: r, path });
        }
        let inherited_scroll = view.scroll.as_deref().or(scroll_store);
        if view.action.is_some() || view.input.is_some() || view.scroll.is_some() {
            out.nodes.push(Node {
                id: view.id.clone(),
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
                icon: None,
                input: view.input.clone(),
                scroll: None,
                action_arg: view.argument.clone(),
                scroll_store: inherited_scroll.map(str::to_owned),
                focusable: view.action.is_some() || view.input.is_some(),
                clip,
            });
        }
        let mut children_pos = (r.x, r.y);
        let mut child_clip = clip;
        if let Some(store) = &view.scroll {
            let Value::Number(offset) = resolve(store)? else {
                return Err(source.error("scroll must bind a number"));
            };
            children_pos.1 -= offset as f32;
            child_clip = Some(if let Some(c) = clip {
                intersect(c, r).unwrap_or(r)
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
                child_clip,
                inherited_scroll,
                resolve,
                palette,
                hover,
                out,
            )?;
        }
        Ok(())
    }
    draw(
        view,
        &tree,
        &ids,
        (0., 0.),
        None,
        None,
        resolve,
        palette,
        &hover,
        &mut output,
    )?;
    Ok(output)
}

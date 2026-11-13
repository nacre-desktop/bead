// SPDX-License-Identifier: MIT
//! GraphCanvas engine (spec 0232, phase 2a). A `Graph` view node carries its data once
//! per change; the session keeps one `Canvas` per node id: the layout worker, the latest
//! positions, the camera and the hover/selection state. Camera, hover and layout
//! snapshots patch the committed scene directly without re-running Luau; data and
//! style changes arrive through ordinary materialization and are reconciled by id.
use crate::{Diagnostic, Node, Session, layout::Source};
use bead_core::{
    Value,
    graph::{Command, Point, Snapshot, Worker},
};
use bead_render::{
    Palette, Primitive, Rect, Scene,
    graph::{Camera, GraphGeometry, GraphStyle},
};
use serde_json::{Map, Value as Json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Host wake callback shared with layout workers; set once the host registers its event sink.
pub(crate) type WakeSlot = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;
type ActionParser<'a> = dyn Fn(&str) -> Result<Option<(String, Option<Value>)>, Diagnostic> + 'a;
const MAX_NODES: usize = 20_000;
const MAX_EDGES: usize = 100_000;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const LABEL_WIDTH: f32 = 200.;
const HIT_CELL: f32 = 64.;

pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub group: String,
    pub weight: f32,
    pub pinned: bool,
}
/// Parsed `Graph` properties; colours stay unresolved until the palette is known.
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<[u32; 2]>,
    pub style: Map<String, Json>,
    pub highlight: Vec<String>,
    pub hidden_groups: Vec<String>,
    pub selected: Option<String>,
    pub label_family: String,
    pub on_hover: Option<String>,
    pub on_select: Option<String>,
    pub on_activate: Option<String>,
    pub on_view_changed: Option<String>,
    pub on_pin: Option<String>,
}
fn id_text(value: &Json) -> Option<String> {
    match value {
        Json::String(s) if !s.is_empty() && s.len() <= 256 => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
impl GraphData {
    pub(crate) fn parse(
        p: &Map<String, Json>,
        action: &ActionParser<'_>,
        source: &Source,
    ) -> Result<Self, Diagnostic> {
        let array = |name: &str| -> Result<&[Json], Diagnostic> {
            match p.get(name) {
                None => Ok(&[][..]),
                Some(Json::Array(items)) => Ok(items),
                Some(Json::Object(items)) if items.is_empty() => Ok(&[][..]),
                Some(_) => Err(source.error(format!("{name} must be an array"))),
            }
        };
        let raw_nodes = array("nodes")?;
        if raw_nodes.len() > MAX_NODES {
            return Err(source.error(format!("GraphCanvas supports at most {MAX_NODES} nodes")));
        }
        let mut nodes = Vec::with_capacity(raw_nodes.len());
        let mut index = HashMap::with_capacity(raw_nodes.len());
        for item in raw_nodes {
            let id = item
                .get("id")
                .and_then(id_text)
                .ok_or_else(|| source.error("GraphCanvas node needs an id"))?;
            if index.insert(id.clone(), nodes.len() as u32).is_some() {
                return Err(source.error(format!("duplicate GraphCanvas node id {id}")));
            }
            let weight = item
                .get("weight")
                .map_or(1., |w| w.as_f64().unwrap_or(f64::NAN));
            if !weight.is_finite() || weight < 0. {
                return Err(source.error(format!("GraphCanvas node {id} weight must be >= 0")));
            }
            let weight = weight as f32;
            if !weight.is_finite() {
                return Err(source.error(format!("GraphCanvas node {id} weight is too large")));
            }
            nodes.push(GraphNode {
                label: item
                    .get("label")
                    .map_or_else(|| id.clone(), |l| l.as_str().unwrap_or("").to_owned()),
                group: item
                    .get("group")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_owned(),
                weight,
                pinned: item.get("pinned").and_then(Json::as_bool).unwrap_or(false),
                id,
            });
        }
        let raw_edges = array("edges")?;
        if raw_edges.len() > MAX_EDGES {
            return Err(source.error(format!("GraphCanvas supports at most {MAX_EDGES} edges")));
        }
        let mut edges = Vec::with_capacity(raw_edges.len());
        for item in raw_edges {
            let end = |key: &str| {
                item.get(key)
                    .and_then(id_text)
                    .and_then(|id| index.get(&id).copied())
                    .ok_or_else(|| source.error(format!("GraphCanvas edge {key} must name a node")))
            };
            edges.push([end("from")?, end("to")?]);
        }
        let ids = |name: &str| -> Result<Vec<String>, Diagnostic> {
            array(name)?
                .iter()
                .map(|v| {
                    id_text(v).ok_or_else(|| source.error(format!("{name} entries must be ids")))
                })
                .collect()
        };
        let style = match p.get("style") {
            None => Map::new(),
            Some(Json::Object(style)) => style.clone(),
            Some(_) => return Err(source.error("style must be a record")),
        };
        let label_family = style
            .get("label_family")
            .and_then(Json::as_str)
            .map_or_else(
                || crate::theme::with_sans_family(str::to_owned),
                str::to_owned,
            );
        let name = |property: &str| Ok::<_, Diagnostic>(action(property)?.map(|(name, _)| name));
        Ok(Self {
            nodes,
            edges,
            style,
            highlight: ids("highlight")?,
            hidden_groups: ids("hidden_groups")?,
            selected: p.get("selected_node").and_then(id_text),
            label_family,
            on_hover: name("on_hover")?,
            on_select: name("on_select")?,
            on_activate: name("on_activate")?,
            on_view_changed: name("on_view_changed")?,
            on_pin: name("on_pin")?,
        })
    }
    /// Resolve theme roles and thresholds; absent slots fall back to palette roles.
    pub(crate) fn style(
        &self,
        palette: &Palette,
        source: &Source,
    ) -> Result<GraphStyle, Diagnostic> {
        let d = GraphStyle::default();
        let slot = |key: &str, role: &str, fallback: [u8; 4], alpha: u8| {
            let base = palette.get(role).map_or(fallback, |mut c| {
                c[3] = alpha;
                c
            });
            crate::layout::color(self.style.get(key), base, palette, source)
        };
        let number = |key: &str, default: f32, range: std::ops::RangeInclusive<f32>| match self
            .style
            .get(key)
        {
            None => Ok(default),
            Some(v) => v
                .as_f64()
                .map(|v| v as f32)
                .filter(|v| range.contains(v))
                .ok_or_else(|| {
                    source.error(format!(
                        "style.{key} must be {}..{}",
                        range.start(),
                        range.end()
                    ))
                }),
        };
        Ok(GraphStyle {
            node: slot("node", "primary", d.node, 255)?,
            edge: slot("edge", "outlineVariant", d.edge, 150)?,
            hover: slot("hover", "secondary", d.hover, 255)?,
            selected: slot("selected", "onSurface", d.selected, 255)?,
            highlight: slot("highlight", "primaryContainer", d.highlight, 255)?,
            label: slot("label", "onSurface", [220, 220, 220, 255], 255)?,
            label_size: number(
                "label_size",
                crate::transcript::Scale::builtin().caption.max(8.),
                6. ..=64.,
            )?,
            label_zoom: number("label_zoom", 1.2, 0. ..=100.)?,
            max_labels: number("max_labels", 32., 0. ..=256.)? as u16,
            node_radius: number("node_radius", 3., 0.5..=64.)?,
            lod_zoom: number("lod_zoom", d.lod_zoom, 0. ..=100.)?,
            far_edge_alpha: number("far_edge_alpha", f32::from(d.far_edge_alpha), 0. ..=255.)?
                as u8,
            dim_alpha: number("dim_alpha", f32::from(d.dim_alpha), 0. ..=255.)? as u8,
        })
    }
}

struct Press {
    node: Option<u32>,
    origin: [f32; 2],
    camera: Camera,
    moved: bool,
}
pub(crate) struct Canvas {
    worker: Worker,
    data: Arc<GraphData>,
    index: HashMap<String, u32>,
    positions: Vec<Point>,
    pinned: Vec<bool>,
    /// Node indices by descending weight: the label priority for unfocused nodes.
    by_weight: Vec<u32>,
    geometry: Arc<GraphGeometry>,
    geometry_dirty: bool,
    hit_grid: HashMap<(i32, i32), Vec<u32>>,
    hit_grid_dirty: bool,
    hit_max_radius: f32,
    command_revision: u64,
    style: GraphStyle,
    rect: Rect,
    camera: Camera,
    fitted: bool,
    /// The one re-fit after the first settled layout, unless the user moved the camera.
    settled_fit: bool,
    user_moved: bool,
    hover: Option<u32>,
    selected: Option<u32>,
    press: Option<Press>,
    last_click: Option<(u32, Instant)>,
}
fn seed_position(i: usize, n: usize) -> Point {
    // A deterministic scatter sized to the node count, so a fresh graph starts spread out.
    let side = (n as f32).sqrt() * 60.;
    let mut h = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x000b_0232;
    let mut next = || {
        h ^= h << 13;
        h ^= h >> 7;
        h ^= h << 17;
        (h >> 40) as f32 / (1u32 << 24) as f32
    };
    [(next() - 0.5) * side, (next() - 0.5) * side]
}
impl Canvas {
    fn new(
        data: &Arc<GraphData>,
        style: GraphStyle,
        rect: Rect,
        wake: &WakeSlot,
    ) -> Result<Self, Diagnostic> {
        let slot = wake.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(wake) = slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                wake();
            }
        });
        let worker = Worker::spawn(Some(wake))
            .map_err(|e| Diagnostic::new(std::path::Path::new("graph"), 1, 1, e))?;
        let mut canvas = Self {
            worker,
            data: Arc::new(GraphData {
                nodes: vec![],
                edges: vec![],
                style: Map::new(),
                highlight: vec![],
                hidden_groups: vec![],
                selected: None,
                label_family: String::new(),
                on_hover: None,
                on_select: None,
                on_activate: None,
                on_view_changed: None,
                on_pin: None,
            }),
            index: HashMap::new(),
            positions: vec![],
            pinned: vec![],
            by_weight: vec![],
            geometry: Arc::default(),
            geometry_dirty: true,
            hit_grid: HashMap::new(),
            hit_grid_dirty: true,
            hit_max_radius: 0.,
            command_revision: 0,
            style,
            rect,
            camera: Camera::default(),
            fitted: false,
            settled_fit: false,
            user_moved: false,
            hover: None,
            selected: None,
            press: None,
            last_click: None,
        };
        canvas.reconcile(data, style);
        Ok(canvas)
    }
    /// Apply new view data: positions follow ids, new nodes start near a neighbour and
    /// settle locally, renames and style changes never reach the worker.
    fn reconcile(&mut self, data: &Arc<GraphData>, style: GraphStyle) {
        let old = &self.data;
        let old_node_radius = self.style.node_radius;
        let same_nodes = old.nodes.len() == data.nodes.len()
            && old
                .nodes
                .iter()
                .zip(&data.nodes)
                .all(|(a, b)| a.id == b.id && a.pinned == b.pinned);
        let topology_changed = !same_nodes || old.edges != data.edges;
        if topology_changed {
            let old_hover = self.hover.map(|i| self.id(i));
            let old_press = self.press.as_ref().and_then(|p| p.node).map(|i| self.id(i));
            let old_click = self.last_click.map(|(i, at)| (self.id(i), at));
            let n = data.nodes.len();
            let mut index = HashMap::with_capacity(n);
            let mut positions = vec![[f32::NAN, f32::NAN]; n];
            let mut hot = vec![false; n];
            for (i, node) in data.nodes.iter().enumerate() {
                index.insert(node.id.clone(), i as u32);
                if let Some(&old_index) = self.index.get(&node.id)
                    && let Some(p) = self.positions.get(old_index as usize)
                {
                    positions[i] = *p;
                    hot[i] = old.nodes[old_index as usize].pinned != node.pinned;
                } else {
                    hot[i] = true;
                }
            }
            for i in 0..n {
                if positions[i][0].is_finite() {
                    continue;
                }
                // Place a new node beside a placed neighbour; an isolated one joins the scatter.
                let neighbour = data.edges.iter().find_map(|[a, b]| {
                    let other = if *a as usize == i {
                        *b
                    } else if *b as usize == i {
                        *a
                    } else {
                        return None;
                    };
                    positions
                        .get(other as usize)
                        .filter(|p| p[0].is_finite())
                        .copied()
                });
                positions[i] = match neighbour {
                    Some(p) => {
                        let angle = (i as f32) * 2.399_963;
                        [p[0] + angle.cos() * 40., p[1] + angle.sin() * 40.]
                    }
                    None => seed_position(i, n),
                };
            }
            let pinned: Vec<bool> = data
                .nodes
                .iter()
                .map(|node| match self.index.get(&node.id).copied() {
                    Some(old_i) if old.nodes[old_i as usize].pinned == node.pinned => {
                        node.pinned || self.pinned.get(old_i as usize).copied().unwrap_or(false)
                    }
                    _ => node.pinned,
                })
                .collect();
            if !self.positions.is_empty() && self.positions.len() == n && !hot.iter().any(|h| *h) {
                // Pins or edges changed: everything may settle again, nothing is reseeded.
                hot.fill(true);
            }
            self.command_revision = self.worker.send(Command::Replace {
                positions: positions.clone(),
                pinned: pinned.clone(),
                edges: data.edges.clone(),
                hot,
            });
            self.index = index;
            self.positions = positions;
            self.pinned = pinned;
            self.geometry_dirty = true;
            self.hit_grid_dirty = true;
            self.hover = old_hover
                .as_ref()
                .and_then(|id| self.index.get(id).copied());
            if let Some(id) = old_press {
                if let Some(index) = self.index.get(&id).copied() {
                    if let Some(press) = self.press.as_mut() {
                        press.node = Some(index);
                    }
                } else {
                    self.press = None;
                }
            }
            self.last_click =
                old_click.and_then(|(id, at)| self.index.get(&id).copied().map(|i| (i, at)));
        }
        let mut by_weight: Vec<u32> = (0..data.nodes.len() as u32).collect();
        by_weight.sort_by(|a, b| {
            data.nodes[*b as usize]
                .weight
                .total_cmp(&data.nodes[*a as usize].weight)
        });
        self.by_weight = by_weight;
        self.selected = data
            .selected
            .as_ref()
            .and_then(|id| self.index.get(id).copied());
        if style != self.style {
            self.style = style;
            self.geometry_dirty = true;
        }
        let radii_changed = style.node_radius != old_node_radius
            || old.nodes.len() != data.nodes.len()
            || old
                .nodes
                .iter()
                .zip(&data.nodes)
                .any(|(a, b)| a.weight != b.weight);
        let attributes_changed = old.highlight != data.highlight
            || old.hidden_groups != data.hidden_groups
            || old.nodes.len() != data.nodes.len()
            || old
                .nodes
                .iter()
                .zip(&data.nodes)
                .any(|(a, b)| a.weight != b.weight || a.group != b.group);
        self.geometry_dirty |= attributes_changed;
        self.hit_grid_dirty |= radii_changed;
        self.data = data.clone();
    }
    /// Consume at most one layout snapshot; true when positions changed.
    fn pull(&mut self) -> bool {
        let Some(snapshot) = self.worker.take_snapshot() else {
            return false;
        };
        self.apply_snapshot(snapshot)
    }
    fn apply_snapshot(&mut self, snapshot: Snapshot) -> bool {
        if snapshot.command_revision < self.command_revision
            || snapshot.positions.len() != self.positions.len()
        {
            return false;
        }
        self.positions.copy_from_slice(&snapshot.positions);
        self.geometry_dirty = true;
        self.hit_grid_dirty = true;
        if snapshot.settled && !self.settled_fit {
            self.settled_fit = true;
            self.fitted = self.user_moved;
        }
        true
    }
    fn fit(&mut self) {
        if self.fitted || self.positions.is_empty() || self.rect.w <= 0. || self.rect.h <= 0. {
            return;
        }
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &self.positions {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        let (w, h) = ((max[0] - min[0]).max(1.), (max[1] - min[1]).max(1.));
        self.camera = Camera {
            center: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            zoom: ((self.rect.w / w).min(self.rect.h / h) * 0.9).clamp(0.05, 8.),
        };
        self.fitted = true;
    }
    fn hidden(&self) -> Vec<bool> {
        if self.data.hidden_groups.is_empty() {
            return vec![];
        }
        self.data
            .nodes
            .iter()
            .map(|node| self.data.hidden_groups.contains(&node.group))
            .collect()
    }
    fn geometry(&mut self) -> Arc<GraphGeometry> {
        if self.geometry_dirty {
            let mut highlighted = vec![];
            if !self.data.highlight.is_empty() {
                highlighted = vec![false; self.data.nodes.len()];
                for id in &self.data.highlight {
                    if let Some(&i) = self.index.get(id) {
                        highlighted[i as usize] = true;
                    }
                }
            }
            self.geometry = Arc::new(GraphGeometry {
                revision: self.geometry.revision + 1,
                positions: self.positions.clone(),
                radii: self
                    .data
                    .nodes
                    .iter()
                    .map(|node| self.style.node_radius * node.weight.clamp(0.25, 16.).sqrt())
                    .collect(),
                edges: self.data.edges.clone(),
                hidden: self.hidden(),
                highlighted,
            });
            self.geometry_dirty = false;
        }
        self.geometry.clone()
    }
    fn visible(&self, i: usize) -> Option<[f32; 2]> {
        if self.geometry.hidden.get(i).copied().unwrap_or(false) {
            return None;
        }
        let s = self.camera.to_screen(self.rect, self.positions[i]);
        let r = self.rect;
        (s[0] >= r.x && s[1] >= r.y && s[0] < r.x + r.w && s[1] < r.y + r.h).then_some(s)
    }
    /// Label primitives in priority order: hovered, selected, highlighted by weight, then
    /// every visible node by weight once zoomed past `label_zoom`; capped by `max_labels`.
    fn labels(&self, clip: Option<Rect>, remaining: usize) -> Vec<(Primitive, Rect)> {
        let cap = usize::from(self.style.max_labels).min(remaining);
        let mut chosen: Vec<u32> = Vec::with_capacity(cap.min(64));
        let push = |i: u32, chosen: &mut Vec<u32>| {
            if chosen.len() < cap && !chosen.contains(&i) && self.visible(i as usize).is_some() {
                chosen.push(i);
            }
        };
        if let Some(i) = self.hover {
            push(i, &mut chosen);
        }
        if let Some(i) = self.selected {
            push(i, &mut chosen);
        }
        if !self.geometry.highlighted.is_empty() {
            for &i in &self.by_weight {
                if self.geometry.highlighted[i as usize] {
                    push(i, &mut chosen);
                }
            }
        }
        if self.camera.zoom >= self.style.label_zoom {
            for &i in &self.by_weight {
                if chosen.len() >= cap {
                    break;
                }
                push(i, &mut chosen);
            }
        }
        let clip = clip.map_or(self.rect, |c| c.intersect(self.rect));
        chosen
            .into_iter()
            .filter_map(|i| {
                let s = self.visible(i as usize)?;
                let radius =
                    self.geometry.radii.get(i as usize).copied().unwrap_or(3.) * self.camera.zoom;
                let size = self.style.label_size;
                Some((
                    Primitive::Text {
                        weight: 400,
                        align: None,
                        rect: Rect::new(
                            s[0] + radius + 4.,
                            s[1] - size * 0.75,
                            LABEL_WIDTH,
                            size * 1.5,
                        ),
                        text: self.data.nodes[i as usize].label.clone(),
                        family: self.data.label_family.clone(),
                        size,
                        color: self.style.label,
                        shadow: None,
                    },
                    clip,
                ))
            })
            .collect()
    }
    /// Nearest node under a logical point, within its radius plus a small slack.
    fn rebuild_hit_grid(&mut self) {
        if !self.hit_grid_dirty {
            return;
        }
        self.hit_grid.clear();
        self.hit_max_radius = 0.;
        for (i, p) in self.positions.iter().enumerate() {
            let radius = self.geometry.radii.get(i).copied().unwrap_or(3.);
            self.hit_max_radius = self.hit_max_radius.max(radius);
            let cell = (
                (p[0] / HIT_CELL).floor() as i32,
                (p[1] / HIT_CELL).floor() as i32,
            );
            self.hit_grid.entry(cell).or_default().push(i as u32);
        }
        self.hit_grid_dirty = false;
    }
    fn hit(&mut self, x: f32, y: f32) -> Option<u32> {
        self.rebuild_hit_grid();
        let w = self.camera.to_world(self.rect, [x, y]);
        let slack = 3. / self.camera.zoom;
        let reach = ((self.hit_max_radius + slack) / HIT_CELL).ceil() as i32;
        let cell = (
            (w[0] / HIT_CELL).floor() as i32,
            (w[1] / HIT_CELL).floor() as i32,
        );
        let mut best: Option<(f32, u32)> = None;
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                let Some(ids) = self.hit_grid.get(&(cell.0 + dx, cell.1 + dy)) else {
                    continue;
                };
                for &id in ids {
                    let i = id as usize;
                    if self.geometry.hidden.get(i).copied().unwrap_or(false) {
                        continue;
                    }
                    let p = self.positions[i];
                    let r = self.geometry.radii.get(i).copied().unwrap_or(3.) + slack;
                    let d2 = (p[0] - w[0]).powi(2) + (p[1] - w[1]).powi(2);
                    if d2 <= r * r && best.is_none_or(|(b, _)| d2 < b) {
                        best = Some((d2, id));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }
    fn id(&self, i: u32) -> String {
        self.data.nodes[i as usize].id.clone()
    }
    fn view_argument(&self) -> Value {
        Value::list(vec![
            Json::from(f64::from(self.camera.zoom)),
            Json::from(f64::from(self.camera.center[0])),
            Json::from(f64::from(self.camera.center[1])),
        ])
    }
    /// Positions and pin flags by id, for host persistence.
    pub fn layout(&self) -> BTreeMap<String, ([f32; 2], bool)> {
        self.data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.id.clone(), (self.positions[i], self.pinned[i])))
            .collect()
    }
    /// Restore saved positions by id; nodes missing from the table keep settling locally.
    pub fn set_layout(&mut self, saved: &BTreeMap<String, ([f32; 2], bool)>) {
        let mut hot = vec![false; self.positions.len()];
        for (i, node) in self.data.nodes.iter().enumerate() {
            match saved.get(&node.id) {
                Some((p, pinned)) if p.iter().all(|v| v.is_finite()) => {
                    self.positions[i] = *p;
                    self.pinned[i] = *pinned;
                }
                _ => hot[i] = true,
            }
        }
        self.command_revision = self.worker.send(Command::Replace {
            positions: self.positions.clone(),
            pinned: self.pinned.clone(),
            edges: self.data.edges.clone(),
            hot,
        });
        self.geometry_dirty = true;
        self.hit_grid_dirty = true;
        self.fitted = false;
        self.settled_fit = false;
        self.user_moved = false;
    }
}

impl Session {
    pub(crate) fn graph_canvas_at(&self, x: f32, y: f32) -> Option<String> {
        self.current_nodes
            .iter()
            .rev()
            .find(|node| node.contains(x, y))
            .filter(|node| node.graph.is_some())
            .map(|node| node.id.clone())
    }
    /// Reconcile every graph node's data into its engine and write the live primitives.
    pub(crate) fn attach_graphs(
        &mut self,
        scene: &mut Scene,
        nodes: &[Node],
        palette: &Palette,
    ) -> Result<(), Diagnostic> {
        let mut seen = Vec::new();
        for node in nodes.iter().filter(|node| node.graph.is_some()) {
            let data = node.graph.as_ref().unwrap();
            let rect = crate::rect(&node.draw);
            let source = node.source.as_ref().map_or_else(
                || Source {
                    file: "ui/main.luau".into(),
                    line: 1,
                },
                |(path, line, _)| Source {
                    file: path.to_string_lossy().into_owned(),
                    line: *line,
                },
            );
            let style = data.style(palette, &source)?;
            match self.graphs.get_mut(&node.id) {
                Some(canvas) => {
                    canvas.reconcile(data, style);
                    canvas.rect = rect;
                    canvas.pull();
                }
                None => {
                    self.graphs.insert(
                        node.id.clone(),
                        Canvas::new(data, style, rect, &self.graph_wake)?,
                    );
                }
            }
            seen.push(node.id.clone());
        }
        self.graphs.retain(|id, _| seen.contains(id));
        self.place_graphs(scene);
        Ok(())
    }
    fn place_graphs(&mut self, scene: &mut Scene) {
        let start = scene.primitives.len();
        let mut labels = Vec::new();
        let text_slots = 1024usize.saturating_sub(
            scene
                .primitives
                .iter()
                .filter(|p| matches!(p, Primitive::Text { .. }))
                .count(),
        );
        for (id, canvas) in &mut self.graphs {
            canvas.fit();
            let geometry = canvas.geometry();
            let Some(index) = scene
                .layers
                .iter()
                .find(|layer| &layer.id == id)
                .and_then(|layer| {
                    layer
                        .own
                        .clone()
                        .find(|i| matches!(scene.primitives.get(*i), Some(Primitive::Graph { .. })))
                })
            else {
                continue;
            };
            let clip = scene.clips.get(&index).copied();
            if let Primitive::Graph {
                geometry: slot,
                camera,
                style,
                hover,
                selected,
                ..
            } = &mut scene.primitives[index]
            {
                *slot = geometry;
                *camera = canvas.camera;
                *style = canvas.style;
                *hover = canvas.hover;
                *selected = canvas.selected;
            }
            labels.extend(canvas.labels(clip, text_slots.saturating_sub(labels.len())));
        }
        for (primitive, clip) in labels {
            scene.clips.insert(scene.primitives.len(), clip);
            scene.primitives.push(primitive);
        }
        self.graph_labels = Some((start, scene.primitives.len() - start));
    }
    /// Remove the labels the last placement appended to a copy of the committed scene.
    pub(crate) fn strip_graph_labels(&self, scene: &mut Scene) {
        if let Some((start, count)) = self.graph_labels
            && scene.primitives.len() == start + count
        {
            scene.primitives.truncate(start);
            scene.clips.retain(|index, _| *index < start);
        }
    }
    /// Patch the committed scene with the newest layout, camera and interaction state.
    /// Returns nothing when `force` is false and no snapshot arrived.
    pub(crate) fn graph_frame(&mut self, force: bool) -> Result<Option<Scene>, Diagnostic> {
        if self.graphs.is_empty() || self.generation.is_none() {
            return Ok(None);
        }
        let mut snapshots = false;
        for canvas in self.graphs.values_mut() {
            snapshots |= canvas.pull();
        }
        if !snapshots && !force {
            return Ok(None);
        }
        let mut scene = self.current.clone();
        self.strip_graph_labels(&mut scene);
        self.place_graphs(&mut scene);
        self.pending = Some(self.patched_pending());
        Ok(Some(scene))
    }
    fn graph_action(&mut self, name: &str, argument: Value) -> Result<Option<Scene>, Diagnostic> {
        self.graph_actions(&[(name.to_owned(), argument)])
    }
    fn graph_actions(&mut self, actions: &[(String, Value)]) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let mut stores = self.stores.clone();
        for (name, argument) in actions {
            stores = generation.script.action(name, &stores, argument.clone())?;
        }
        match self.stage_state(stores)? {
            Some(scene) => Ok(Some(scene)),
            None => self.graph_frame(true),
        }
    }
    /// Hover over or away from a canvas; a scene when the hovered node changed.
    pub(crate) fn graph_hover(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        let over = self.graph_canvas_at(x, y);
        let previous = self
            .graphs
            .iter()
            .find(|(_, c)| c.hover.is_some())
            .map(|(id, _)| id.clone());
        if previous
            .as_ref()
            .and_then(|id| self.graphs.get(id))
            .is_some_and(|c| c.press.is_some())
        {
            return Ok(None);
        }
        let mut actions = Vec::new();
        let mut changed = false;
        if previous != over
            && let Some(id) = previous
        {
            let canvas = self.graphs.get_mut(&id).unwrap();
            canvas.hover = None;
            changed = true;
            if let Some(action) = &canvas.data.on_hover {
                actions.push((action.clone(), Value::Text(String::new())));
            }
        }
        if let Some(id) = over {
            let canvas = self.graphs.get_mut(&id).unwrap();
            let hit = canvas.hit(x, y);
            if hit != canvas.hover {
                canvas.hover = hit;
                changed = true;
                if let Some(action) = &canvas.data.on_hover {
                    actions.push((
                        action.clone(),
                        Value::Text(hit.map_or(String::new(), |i| canvas.id(i))),
                    ));
                }
            }
        }
        if !changed {
            return Ok(None);
        }
        if actions.is_empty() {
            self.graph_frame(true)
        } else {
            self.graph_actions(&actions)
        }
    }
    /// True when a press at this point starts a graph gesture (node drag or pan).
    pub(crate) fn graph_press(&mut self, x: f32, y: f32) -> bool {
        let Some(id) = self.graph_canvas_at(x, y) else {
            return false;
        };
        let canvas = self.graphs.get_mut(&id).unwrap();
        canvas.press = Some(Press {
            node: canvas.hit(x, y),
            origin: [x, y],
            camera: canvas.camera,
            moved: false,
        });
        true
    }
    fn pressed_canvas(&mut self) -> Option<&mut Canvas> {
        self.graphs.values_mut().find(|c| c.press.is_some())
    }
    pub(crate) fn graph_drag(
        &mut self,
        x: f32,
        y: f32,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(canvas) = self.pressed_canvas() else {
            return Ok(None);
        };
        let press = canvas.press.as_mut().unwrap();
        if !press.moved && (x - press.origin[0]).abs() < 2. && (y - press.origin[1]).abs() < 2. {
            return Ok(Some(None));
        }
        press.moved = true;
        match press.node {
            Some(node) => {
                let world = canvas.camera.to_world(canvas.rect, [x, y]);
                canvas.positions[node as usize] = world;
                canvas.pinned[node as usize] = true;
                canvas.geometry_dirty = true;
                canvas.hit_grid_dirty = true;
                canvas.command_revision = canvas.worker.send(Command::Pin {
                    node,
                    position: Some(world),
                    pinned: true,
                });
            }
            None => {
                let zoom = press.camera.zoom;
                canvas.camera.center = [
                    press.camera.center[0] - (x - press.origin[0]) / zoom,
                    press.camera.center[1] - (y - press.origin[1]) / zoom,
                ];
                canvas.user_moved = true;
            }
        }
        self.graph_frame(true).map(Some)
    }
    pub(crate) fn graph_release(&mut self) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(canvas) = self.pressed_canvas() else {
            return Ok(None);
        };
        let press = canvas.press.take().unwrap();
        let now = Instant::now();
        let result = match (press.node, press.moved) {
            (Some(node), true) => {
                let id = canvas.id(node);
                let p = canvas.positions[node as usize];
                match canvas.data.on_pin.clone() {
                    Some(action) => self.graph_action(
                        &action,
                        Value::list(vec![
                            Json::from(id),
                            Json::from(f64::from(p[0])),
                            Json::from(f64::from(p[1])),
                        ]),
                    )?,
                    None => self.graph_frame(true)?,
                }
            }
            (Some(node), false) => {
                let id = canvas.id(node);
                let double = canvas
                    .last_click
                    .take()
                    .is_some_and(|(n, at)| n == node && now - at < DOUBLE_CLICK);
                canvas.last_click = (!double).then_some((node, now));
                canvas.selected = Some(node);
                let action = if double {
                    canvas
                        .data
                        .on_activate
                        .clone()
                        .or(canvas.data.on_select.clone())
                } else {
                    canvas.data.on_select.clone()
                };
                match action {
                    Some(action) => self.graph_action(&action, Value::Text(id))?,
                    None => self.graph_frame(true)?,
                }
            }
            (None, true) => match canvas.data.on_view_changed.clone() {
                Some(action) => {
                    let argument = canvas.view_argument();
                    self.graph_action(&action, argument)?
                }
                None => None,
            },
            (None, false) => None,
        };
        Ok(Some(result))
    }
    /// Wheel zoom around the pointer; a scene when the pointer is over a canvas.
    pub(crate) fn graph_scroll(
        &mut self,
        x: f32,
        y: f32,
        delta: f64,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(id) = self.graph_canvas_at(x, y) else {
            return Ok(None);
        };
        let canvas = self.graphs.get_mut(&id).unwrap();
        let anchor = canvas.camera.to_world(canvas.rect, [x, y]);
        let zoom = (canvas.camera.zoom * (-delta as f32 / 300.).exp()).clamp(0.05, 20.);
        let r = canvas.rect;
        canvas.camera = Camera {
            center: [
                anchor[0] - (x - r.x - r.w * 0.5) / zoom,
                anchor[1] - (y - r.y - r.h * 0.5) / zoom,
            ],
            zoom,
        };
        canvas.user_moved = true;
        let result = match canvas.data.on_view_changed.clone() {
            Some(action) => {
                let argument = canvas.view_argument();
                self.graph_action(&action, argument)?
            }
            None => self.graph_frame(true)?,
        };
        Ok(Some(result))
    }
    /// Positions and pin flags of one canvas by node id, for host persistence.
    pub fn graph_layout(&self, id: &str) -> Option<BTreeMap<String, ([f32; 2], bool)>> {
        self.graphs.get(id).map(Canvas::layout)
    }
    /// Restore a saved layout into one canvas; unknown ids are ignored, missing nodes settle.
    pub fn set_graph_layout(
        &mut self,
        id: &str,
        layout: &BTreeMap<String, ([f32; 2], bool)>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some(canvas) = self.graphs.get_mut(id) else {
            return Ok(None);
        };
        canvas.set_layout(layout);
        self.graph_frame(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Fixture;
    use std::fs;

    const BEHAVIOR: &str = r#"return {
 stores={hovered="",selected="",events=0},
 actions={hover=function(s,id) s.hovered=id end, select=function(s,id) s.selected=id end,
  activate=function(s) s.events+=1 end, view=function(s) s.events+=1 end, pin=function(s) s.events+=1 end},
 computed={}
}"#;
    fn view(label_b: &str) -> String {
        format!(
            r#"local ui=require("bead.ui")
local kit=require("bead.kit")
return ui.Column{{id="root",width=400,height=300,
 kit.GraphCanvas{{id="canvas",width=400,height=300,
  nodes={{{{id="a",label="Alpha",weight=5,pinned=true}},{{id="b",label="{label_b}",group="g"}},{{id="c"}}}},
  edges={{{{from="a",to="b"}},{{from="b",to="c"}}}},selected=ui.bind("selected"),highlight={{"a"}},
  style={{label_zoom=0,node_radius=1}},on_hover=ui.action("hover"),on_select=ui.action("select"),
  on_activate=ui.action("activate"),on_view_changed=ui.action("view"),on_pin=ui.action("pin")}}}}"#
        )
    }
    fn session() -> (Fixture, Session) {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("ui")).unwrap();
        fs::write(f.0.join("ui/behavior.luau"), BEHAVIOR).unwrap();
        fs::write(f.0.join("ui/main.luau"), view("Beta")).unwrap();
        let mut s = f.session();
        s.entry = "ui/main.luau".into();
        let scene = s.reload().unwrap();
        s.commit(scene);
        (f, s)
    }
    fn screen(s: &Session, node: usize) -> (f32, f32) {
        let c = &s.graphs["root.canvas"];
        let p = c.camera.to_screen(c.rect, c.positions[node]);
        (p[0], p[1])
    }
    /// The canvas corner farthest from every node.
    fn empty_point(s: &Session) -> (f32, f32) {
        let nodes: Vec<_> = (0..3).map(|i| screen(s, i)).collect();
        [(1., 1.), (399., 1.), (1., 299.), (399., 299.)]
            .into_iter()
            .max_by(|a, b| {
                let far = |p: &(f32, f32)| {
                    nodes
                        .iter()
                        .map(|n| (n.0 - p.0).powi(2) + (n.1 - p.1).powi(2))
                        .fold(f32::MAX, f32::min)
                };
                far(a).total_cmp(&far(b))
            })
            .unwrap()
    }
    fn events(s: &Session) -> f64 {
        match s.stores.get("events") {
            Some(Value::Number(n)) => *n,
            _ => panic!("events store"),
        }
    }

    #[test]
    fn registering_wakeup_flushes_existing_graph_state() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (_f, mut s) = session();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        s.set_wakeup(Arc::new(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        assert!(wakes.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn empty_luau_tables_are_valid_graph_lists() {
        let source = Source {
            file: "graph".into(),
            line: 1,
        };
        let properties =
            serde_json::json!({"nodes": {}, "edges": {}, "highlight": {}, "hidden_groups": {}});
        let data =
            GraphData::parse(properties.as_object().unwrap(), &|_| Ok(None), &source).unwrap();
        assert!(data.nodes.is_empty() && data.edges.is_empty());
        assert!(data.highlight.is_empty() && data.hidden_groups.is_empty());
        let invalid = serde_json::json!({"nodes": {"not_a_list": 1}});
        assert!(GraphData::parse(invalid.as_object().unwrap(), &|_| Ok(None), &source).is_err());
    }

    #[test]
    fn graph_input_respects_an_overlapping_control() {
        let (_f, mut s) = session();
        let (x, y) = screen(&s, 0);
        assert_eq!(s.graph_canvas_at(x, y).as_deref(), Some("root.canvas"));
        let mut overlay = s
            .current_nodes
            .iter()
            .find(|n| n.graph.is_some())
            .unwrap()
            .clone();
        overlay.id = "overlay".into();
        overlay.graph = None;
        overlay.focusable = true;
        s.current_nodes.push(overlay);
        assert_eq!(s.graph_canvas_at(x, y), None);
        assert!(!s.graph_press(x, y));
        assert!(s.graph_scroll(x, y, -120.).unwrap().is_none());
    }

    #[test]
    fn hit_grid_matches_nearest_node_scan() {
        let (_f, mut s) = session();
        let canvas = s.graphs.get_mut("root.canvas").unwrap();
        for x in 0..40 {
            for y in 0..30 {
                let point = (x as f32 * 10. + 0.5, y as f32 * 10. + 0.5);
                let world = canvas.camera.to_world(canvas.rect, [point.0, point.1]);
                let slack = 3. / canvas.camera.zoom;
                let expected = canvas
                    .positions
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        if canvas.geometry.hidden.get(i).copied().unwrap_or(false) {
                            return None;
                        }
                        let r = canvas.geometry.radii.get(i).copied().unwrap_or(3.) + slack;
                        let d2 = (p[0] - world[0]).powi(2) + (p[1] - world[1]).powi(2);
                        (d2 <= r * r).then_some((d2, i as u32))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, i)| i);
                assert_eq!(canvas.hit(point.0, point.1), expected);
            }
        }
    }

    #[test]
    fn reorder_remaps_interaction_and_keeps_runtime_pins() {
        let (f, mut s) = session();
        let positions = s.graphs["root.canvas"].positions.clone();
        s.graphs.get_mut("root.canvas").unwrap().hover = Some(1);
        s.graphs.get_mut("root.canvas").unwrap().last_click = Some((1, Instant::now()));
        s.graphs.get_mut("root.canvas").unwrap().pinned[1] = true;
        let reordered = r#"local ui=require("bead.ui") local kit=require("bead.kit")
return ui.Column{id="root",width=400,height=300,kit.GraphCanvas{id="canvas",width=400,height=300,
nodes={{id="b",label="Beta",group="g"},{id="c"},{id="a",label="Alpha",weight=5,pinned=true}},
edges={{from="b",to="c"},{from="c",to="a"}},style={node_radius=1}}}"#;
        fs::write(f.0.join("ui/main.luau"), reordered).unwrap();
        let scene = s.reload().unwrap();
        s.commit(scene);
        let c = &s.graphs["root.canvas"];
        assert_eq!(c.hover, Some(0));
        assert_eq!(c.last_click.map(|(i, _)| i), Some(0));
        assert!(c.pinned[0]);
        assert_eq!(c.positions[0], positions[1]);
    }

    #[test]
    fn restore_discards_pre_restore_snapshot() {
        let (_f, mut s) = session();
        let canvas = s.graphs.get_mut("root.canvas").unwrap();
        let old_revision = canvas.command_revision;
        canvas.set_layout(&BTreeMap::from([("a".into(), ([123., 45.], true))]));
        let restored = canvas.positions.clone();
        let stale = Snapshot {
            revision: 99,
            command_revision: old_revision,
            positions: vec![[900., 900.]; restored.len()].into(),
            settled: false,
        };
        assert!(!canvas.apply_snapshot(stale));
        assert_eq!(canvas.positions, restored);
    }

    #[test]
    fn graph_canvas_renders_hovers_selects_pans_zooms_and_pins() {
        let (f, mut s) = session();
        let graphs = s
            .current
            .primitives
            .iter()
            .filter(|p| matches!(p, Primitive::Graph { .. }))
            .count();
        assert_eq!(graphs, 1);
        let Some(Primitive::Graph { geometry, .. }) = s
            .current
            .primitives
            .iter()
            .find(|p| matches!(p, Primitive::Graph { .. }))
        else {
            unreachable!()
        };
        assert_eq!(geometry.positions.len(), 3);
        assert_eq!(geometry.edges, vec![[0, 1], [1, 2]]);
        assert_eq!(geometry.highlighted, vec![true, false, false]);
        let labels: Vec<_> = s
            .current
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(labels.contains(&"Alpha".to_owned()), "labels: {labels:?}");

        // Layout snapshots arrive through the model wakeup without a Luau pass.
        std::thread::sleep(std::time::Duration::from_millis(50));
        let scene = s.update_model(BTreeMap::new()).unwrap().unwrap();
        s.commit(scene);

        let (ax, ay) = screen(&s, 0);
        let scene = s.hover(ax, ay).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.stores.get("hovered"), Some(&Value::Text("a".into())));
        let (ex, ey) = empty_point(&s);
        let scene = s.hover(ex, ey).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.stores.get("hovered"), Some(&Value::Text(String::new())));

        assert!(s.click(ax, ay).unwrap().is_none());
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.stores.get("selected"), Some(&Value::Text("a".into())));
        assert_eq!(s.graphs["root.canvas"].selected, Some(0));
        s.click(ax, ay).unwrap();
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert_eq!(events(&s), 1., "double click activates");

        let (bx, by) = screen(&s, 1);
        s.click(bx, by).unwrap();
        let scene = s.drag(bx + 30., by + 10.).unwrap().unwrap();
        s.commit(scene);
        assert!(s.graphs["root.canvas"].pinned[1]);
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert_eq!(events(&s), 2., "node drag pins");

        let zoom = s.graphs["root.canvas"].camera.zoom;
        let scene = s.scroll(ax, ay, -300.).unwrap().unwrap();
        s.commit(scene);
        assert!(s.graphs["root.canvas"].camera.zoom > zoom);
        assert_eq!(events(&s), 3., "zoom reports the view");

        let center = s.graphs["root.canvas"].camera.center;
        let (ex, ey) = empty_point(&s);
        s.click(ex, ey).unwrap();
        let scene = s.drag(ex + 40., ey + 30.).unwrap().unwrap();
        s.commit(scene);
        assert_ne!(s.graphs["root.canvas"].camera.center, center);
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert_eq!(events(&s), 4., "pan reports the view");

        let layout = s.graph_layout("root.canvas").unwrap();
        assert_eq!(layout.len(), 3);
        assert!(layout["a"].1 && layout["b"].1);
        let positions = s.graphs["root.canvas"].positions.clone();

        // A rename reaches the view, not the worker: positions stay where they were.
        fs::write(f.0.join("ui/main.luau"), view("Renamed")).unwrap();
        let scene = s.reload().unwrap();
        s.commit(scene);
        assert_eq!(s.graphs["root.canvas"].positions, positions);
        assert_eq!(s.graphs["root.canvas"].data.nodes[1].label, "Renamed");

        let mut saved = layout;
        saved.get_mut("c").unwrap().0 = [123., 45.];
        let scene = s.set_graph_layout("root.canvas", &saved).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.graphs["root.canvas"].positions[2], [123., 45.]);
    }
}

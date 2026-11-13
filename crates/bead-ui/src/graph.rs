// SPDX-License-Identifier: MIT
//! GraphCanvas engine (spec 0232). A `Graph` view node carries its handed-over data; the
//! session keeps one `Canvas` per node id: the layout worker, the latest positions, the
//! camera and the hover/selection state. Camera, hover and layout snapshots patch the
//! committed scene directly without re-running Luau; data and style changes arrive through
//! ordinary materialization and are reconciled by id.
use crate::{Diagnostic, Node, Session, graph_data::GraphResource, layout::Source};
use bead_core::{
    Value,
    graph::{Grid, Point, Replace, Snapshot, Worker},
};
use bead_render::{
    Palette, Primitive, Rect, Scene,
    graph::{Camera, GraphGeometry, GraphStyle},
};
use serde_json::{Map, Value as Json};
use std::sync::{Arc, Mutex};

/// Host wake callback shared with layout workers; set once the host registers its event sink.
pub(crate) type WakeSlot = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;
type ActionParser<'a> = dyn Fn(&str) -> Result<Option<(String, Option<Value>)>, Diagnostic> + 'a;
const LABEL_WIDTH: f32 = 200.;
const HIT_CELL: f32 = 64.;

/// Parsed `Graph` properties; colours stay unresolved until the palette is known.
#[derive(Default)]
pub struct GraphData {
    pub topology: Arc<GraphResource>,
    pub style: Map<String, Json>,
    pub on_hover: Option<String>,
    pub on_select: Option<String>,
}
impl GraphData {
    /// `p` reads one authored property with its binding resolved.
    pub(crate) fn parse<'p>(
        p: &dyn Fn(&str) -> Option<&'p Json>,
        action: &ActionParser<'_>,
        source: &Source,
    ) -> Result<Self, Diagnostic> {
        let topology = match p("graph") {
            None => Arc::default(),
            Some(Json::Number(token)) => token
                .as_u64()
                .and_then(crate::graph_data::resolve)
                .ok_or_else(|| source.error("graph data expired; pass a live ui.graph handle"))?,
            Some(_) => return Err(source.error("graph must be a ui.graph(nodes, edges) handle")),
        };
        let style = match p("style") {
            None => Map::new(),
            Some(Json::Object(style)) => style.clone(),
            Some(_) => return Err(source.error("style must be a record")),
        };
        let name = |property: &str| Ok::<_, Diagnostic>(action(property)?.map(|(name, _)| name));
        Ok(Self {
            topology,
            style,
            on_hover: name("on_hover")?,
            on_select: name("on_select")?,
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
            label: slot("label", "onSurface", d.label, 255)?,
            label_size: number(
                "label_size",
                crate::transcript::Scale::builtin().caption.max(8.),
                6. ..=64.,
            )?,
            label_zoom: number("label_zoom", d.label_zoom, 0. ..=100.)?,
            max_labels: number("max_labels", f32::from(d.max_labels), 0. ..=256.)? as u16,
            node_radius: number("node_radius", d.node_radius, 0.5..=64.)?,
            lod_zoom: number("lod_zoom", d.lod_zoom, 0. ..=100.)?,
            far_edge_alpha: number("far_edge_alpha", f32::from(d.far_edge_alpha), 0. ..=255.)?
                as u8,
        })
    }
}

/// A press on the canvas: a click selects `node`, a drag pans from `camera`.
struct Press {
    node: Option<u32>,
    origin: [f32; 2],
    camera: Camera,
    moved: bool,
}
pub(crate) struct Canvas {
    worker: Worker,
    data: Arc<GraphData>,
    positions: Arc<[Point]>,
    geometry: Arc<GraphGeometry>,
    geometry_dirty: bool,
    radii_dirty: bool,
    hit_grid: Grid,
    hit_grid_dirty: bool,
    hit_max_radius: f32,
    /// Snapshots from before this worker command describe another topology.
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
            data: Arc::default(),
            positions: Arc::from([]),
            geometry: Arc::default(),
            geometry_dirty: true,
            radii_dirty: true,
            hit_grid: Grid::default(),
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
        };
        canvas.reconcile(data, style);
        Ok(canvas)
    }
    /// Apply new view data: positions, hover and selection follow ids, new nodes start near
    /// a placed neighbour and settle locally, renames and style changes never reach the worker.
    fn reconcile(&mut self, data: &Arc<GraphData>, style: GraphStyle) {
        let (old, new) = (&self.data.topology, &data.topology);
        let same_ids = || {
            old.nodes.len() == new.nodes.len()
                && old.nodes.iter().zip(&new.nodes).all(|(a, b)| a.id == b.id)
        };
        // Weights may change radii; labels are read per frame.
        self.radii_dirty |= !Arc::ptr_eq(old, new);
        if !Arc::ptr_eq(old, new) && (old.edges != new.edges || !same_ids()) {
            let remap = |i: Option<u32>| i.and_then(|i| new.index(&old.nodes[i as usize].id));
            self.hover = remap(self.hover);
            self.selected = remap(self.selected);
            if let Some(press) = &mut self.press {
                press.node = remap(press.node);
            }
            let n = new.nodes.len();
            let mut positions: Vec<Point> = new
                .nodes
                .iter()
                .map(|node| {
                    old.index(&node.id)
                        .map_or([f32::NAN; 2], |i| self.positions[i as usize])
                })
                .collect();
            let mut hot: Vec<bool> = positions.iter().map(|p| !p[0].is_finite()).collect();
            let fresh = hot.iter().all(|h| *h);
            for i in (0..n).filter(|i| hot[*i]) {
                // Beside a placed neighbour; a fresh graph or an isolated node joins the scatter.
                let neighbour = (!fresh).then(|| {
                    new.edges.iter().find_map(|&[a, b]| {
                        let other = match i as u32 {
                            i if i == a => b,
                            i if i == b => a,
                            _ => return None,
                        };
                        Some(positions[other as usize]).filter(|p| p[0].is_finite())
                    })
                });
                positions[i] = match neighbour.flatten() {
                    Some(p) => {
                        let angle = (i as f32) * 2.399_963;
                        [p[0] + angle.cos() * 40., p[1] + angle.sin() * 40.]
                    }
                    None => seed_position(i, n),
                };
            }
            if self.positions.len() == n && !hot.contains(&true) {
                // Only edges changed: everything may settle again, nothing is reseeded.
                hot.fill(true);
            }
            self.command_revision = self.worker.send(Replace {
                positions: positions.clone(),
                edges: new.edges.clone(),
                hot,
            });
            self.positions = positions.into();
        }
        self.radii_dirty |= style.node_radius != self.style.node_radius;
        self.hit_grid_dirty |= self.radii_dirty;
        self.geometry_dirty |= self.radii_dirty || style != self.style;
        self.style = style;
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
        self.positions = snapshot.positions;
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
        let (min, max) =
            self.positions
                .iter()
                .fold(([f32::MAX; 2], [f32::MIN; 2]), |(mut min, mut max), p| {
                    for axis in 0..2 {
                        min[axis] = min[axis].min(p[axis]);
                        max[axis] = max[axis].max(p[axis]);
                    }
                    (min, max)
                });
        let (w, h) = ((max[0] - min[0]).max(1.), (max[1] - min[1]).max(1.));
        self.camera = Camera {
            center: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            zoom: ((self.rect.w / w).min(self.rect.h / h) * 0.9).clamp(0.05, 8.),
        };
        self.fitted = true;
    }
    fn geometry(&mut self) -> Arc<GraphGeometry> {
        if self.geometry_dirty {
            let radii = if self.radii_dirty {
                let r = self.style.node_radius;
                let nodes = &self.data.topology.nodes;
                nodes
                    .iter()
                    .map(|node| r * node.weight.clamp(0.25, 16.).sqrt())
                    .collect()
            } else {
                self.geometry.radii.clone()
            };
            self.geometry = Arc::new(GraphGeometry {
                revision: self.geometry.revision + 1,
                positions: self.positions.clone(),
                radii,
                edges: self.data.topology.edges.clone(),
            });
            self.geometry_dirty = false;
            self.radii_dirty = false;
        }
        self.geometry.clone()
    }
    fn visible(&self, i: u32) -> Option<[f32; 2]> {
        let s = self.camera.to_screen(self.rect, self.positions[i as usize]);
        let r = self.rect;
        (s[0] >= r.x && s[1] >= r.y && s[0] < r.x + r.w && s[1] < r.y + r.h).then_some(s)
    }
    /// Label primitives in priority order: hovered, selected, then every visible node by
    /// weight once zoomed past `label_zoom`; capped by `max_labels` and the free text slots.
    fn labels(&self, clip: Option<Rect>, remaining: usize) -> Vec<(Primitive, Rect)> {
        let cap = usize::from(self.style.max_labels).min(remaining);
        let zoomed = self.camera.zoom >= self.style.label_zoom;
        let by_weight = self.data.topology.priority.iter().copied();
        let candidates = [self.hover, self.selected]
            .into_iter()
            .flatten()
            .chain(zoomed.then_some(by_weight).into_iter().flatten());
        let mut chosen: Vec<(u32, [f32; 2])> = Vec::new();
        for i in candidates {
            if chosen.len() >= cap {
                break;
            }
            if !chosen.iter().any(|(c, _)| *c == i)
                && let Some(s) = self.visible(i)
            {
                chosen.push((i, s));
            }
        }
        let clip = clip.map_or(self.rect, |c| c.intersect(self.rect));
        let size = self.style.label_size;
        let family = crate::theme::with_sans_family(str::to_owned);
        chosen
            .into_iter()
            .map(|(i, s)| {
                let radius =
                    self.geometry.radii.get(i as usize).copied().unwrap_or(3.) * self.camera.zoom;
                let text = Primitive::Text {
                    weight: 400,
                    align: None,
                    rect: Rect::new(
                        s[0] + radius + 4.,
                        s[1] - size * 0.75,
                        LABEL_WIDTH,
                        size * 1.5,
                    ),
                    text: self.data.topology.nodes[i as usize].label.to_string(),
                    family: family.clone(),
                    size,
                    color: self.style.label,
                    shadow: None,
                };
                (text, clip)
            })
            .collect()
    }
    /// Nearest node under a logical point, within its radius plus a small slack.
    fn hit(&mut self, x: f32, y: f32) -> Option<u32> {
        if self.hit_grid_dirty {
            self.hit_grid.update(&self.positions, HIT_CELL);
            self.hit_max_radius = self.geometry.radii.iter().copied().fold(0., f32::max);
            self.hit_grid_dirty = false;
        }
        let w = self.camera.to_world(self.rect, [x, y]);
        let slack = 3. / self.camera.zoom;
        let reach = ((self.hit_max_radius + slack) / HIT_CELL).ceil() as i32;
        let cell = Grid::cell(w, HIT_CELL);
        let mut best: Option<(f32, u32)> = None;
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for i in self.hit_grid.at([cell[0] + dx, cell[1] + dy]) {
                    let p = self.positions[i];
                    let r = self.geometry.radii.get(i).copied().unwrap_or(3.) + slack;
                    let d2 = (p[0] - w[0]).powi(2) + (p[1] - w[1]).powi(2);
                    if d2 <= r * r && best.is_none_or(|(b, _)| d2 < b) {
                        best = Some((d2, i as u32));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }
    fn id(&self, i: u32) -> String {
        self.data.topology.nodes[i as usize].id.to_string()
    }
}

/// Index of the `Graph` primitive a canvas node emitted into its own layer.
fn graph_primitive(scene: &Scene, id: &str) -> Option<usize> {
    let layer = scene.layers.iter().find(|layer| layer.id == id)?;
    layer
        .own
        .clone()
        .find(|i| matches!(scene.primitives.get(*i), Some(Primitive::Graph { .. })))
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
        _palette: &Palette,
    ) -> Result<(), Diagnostic> {
        // shortcut: `_palette` is unused since layout resolves the style; drop it when session.rs is free.
        // Layout resolved and validated every style, so a rejected update never reaches an engine.
        let updates: Vec<_> = nodes
            .iter()
            .filter_map(|node| {
                let data = node.graph.as_ref()?;
                let style = match graph_primitive(scene, &node.id).map(|i| &scene.primitives[i]) {
                    Some(Primitive::Graph { style, .. }) => *style,
                    _ => GraphStyle::default(),
                };
                Some((node.id.clone(), data, crate::rect(&node.draw), style))
            })
            .collect();
        self.graphs
            .retain(|id, _| updates.iter().any(|(seen, ..)| seen == id));
        for (id, data, rect, style) in updates {
            match self.graphs.get_mut(&id) {
                Some(canvas) => {
                    canvas.reconcile(data, style);
                    canvas.rect = rect;
                    canvas.pull();
                }
                None => {
                    let canvas = Canvas::new(data, style, rect, &self.graph_wake)?;
                    self.graphs.insert(id, canvas);
                }
            }
        }
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
            let Some(index) = graph_primitive(scene, id) else {
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
    /// Run the actions, then stage their state; without a view change only the graphs repaint.
    fn graph_actions(
        &mut self,
        actions: Vec<(String, Value)>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = self.generation.as_ref().filter(|_| !actions.is_empty()) else {
            return self.graph_frame(true);
        };
        let mut stores = self.stores.clone();
        for (name, argument) in actions {
            stores = generation.script.action(&name, &stores, argument)?;
        }
        match self.stage_state(stores)? {
            Some(scene) => Ok(Some(scene)),
            None => self.graph_frame(true),
        }
    }
    /// Hover over or away from a canvas; a scene when a hovered node changed.
    pub(crate) fn graph_hover(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        if self.graphs.values().any(|c| c.press.is_some()) {
            return Ok(None);
        }
        let over = self.graph_canvas_at(x, y);
        let mut actions = Vec::new();
        let mut changed = false;
        for (id, canvas) in &mut self.graphs {
            let hit = if over.as_ref() == Some(id) {
                canvas.hit(x, y)
            } else {
                None
            };
            if hit == canvas.hover {
                continue;
            }
            canvas.hover = hit;
            changed = true;
            if let Some(action) = &canvas.data.on_hover {
                let id = hit.map_or_else(String::new, |i| canvas.id(i));
                actions.push((action.clone(), Value::Text(id)));
            }
        }
        if changed {
            self.graph_actions(actions)
        } else {
            Ok(None)
        }
    }
    /// True when a press at this point starts a graph gesture (click or pan).
    pub(crate) fn graph_press(&mut self, x: f32, y: f32) -> bool {
        let Some(canvas) = self
            .graph_canvas_at(x, y)
            .and_then(|id| self.graphs.get_mut(&id))
        else {
            return false;
        };
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
    /// Pan once the pointer left a small slop around the press.
    pub(crate) fn graph_drag(
        &mut self,
        x: f32,
        y: f32,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(canvas) = self.pressed_canvas() else {
            return Ok(None);
        };
        let press = canvas.press.as_mut().unwrap();
        let (dx, dy) = (x - press.origin[0], y - press.origin[1]);
        if !press.moved && dx.abs() < 2. && dy.abs() < 2. {
            return Ok(Some(None));
        }
        press.moved = true;
        let start = press.camera;
        canvas.camera.center = [
            start.center[0] - dx / start.zoom,
            start.center[1] - dy / start.zoom,
        ];
        canvas.user_moved = true;
        self.graph_frame(true).map(Some)
    }
    /// A click (no pan) on a node selects it and runs `on_select` with its id.
    pub(crate) fn graph_release(&mut self) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(canvas) = self.pressed_canvas() else {
            return Ok(None);
        };
        let press = canvas.press.take().unwrap();
        let Some(node) = press.node.filter(|_| !press.moved) else {
            return Ok(Some(None));
        };
        canvas.selected = Some(node);
        let actions = canvas
            .data
            .on_select
            .iter()
            .map(|action| (action.clone(), Value::Text(canvas.id(node))))
            .collect();
        self.graph_actions(actions).map(Some)
    }
    /// Wheel zoom around the pointer; a scene when the pointer is over a canvas.
    pub(crate) fn graph_scroll(
        &mut self,
        x: f32,
        y: f32,
        delta: f64,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        let Some(canvas) = self
            .graph_canvas_at(x, y)
            .and_then(|id| self.graphs.get_mut(&id))
        else {
            return Ok(None);
        };
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
        self.graph_frame(true).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Fixture;
    use std::{collections::BTreeMap, fs};

    const BEHAVIOR: &str = r#"return {
 stores={hovered="",selected=""},
 actions={hover=function(s,id) s.hovered=id end, select=function(s,id) s.selected=id end},
 computed={}
}"#;
    fn view(label_b: &str) -> String {
        format!(
            r#"local ui=require("bead.ui")
local kit=require("bead.kit")
return ui.Column{{id="root",width=400,height=300,
 kit.GraphCanvas{{id="canvas",width=400,height=300,
  graph=ui.graph({{{{id="a",label="Alpha",weight=5}},{{id="b",label="{label_b}"}},{{id="c"}}}},
   {{{{from="a",to="b"}},{{from="b",to="c"}}}}),
  style={{label_zoom=0,node_radius=1}},on_hover=ui.action("hover"),on_select=ui.action("select")}}}}"#
        )
    }
    fn session() -> (Fixture, Session) {
        session_with(&view("Beta"))
    }
    fn session_with(source: &str) -> (Fixture, Session) {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("ui")).unwrap();
        fs::write(f.0.join("ui/behavior.luau"), BEHAVIOR).unwrap();
        fs::write(f.0.join("ui/main.luau"), source).unwrap();
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
    fn text(s: &Session, store: &str) -> Option<Value> {
        s.stores.get(store).cloned()
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
    fn missing_graph_is_empty_and_a_stale_token_is_rejected() {
        let source = Source {
            file: "graph".into(),
            line: 1,
        };
        let none = serde_json::json!({});
        let data = GraphData::parse(&|name| none.get(name), &|_| Ok(None), &source).unwrap();
        assert!(data.topology.nodes.is_empty() && data.topology.edges.is_empty());
        let stale = serde_json::json!({"graph": 123456789});
        assert!(GraphData::parse(&|name| stale.get(name), &|_| Ok(None), &source).is_err());
    }

    #[test]
    fn invalid_graph_update_keeps_every_canvas_and_layout() {
        let initial = r#"local ui=require("bead.ui") local kit=require("bead.kit")
return ui.Column{id="root",width=400,height=300,
 kit.GraphCanvas{id="a",width=200,height=300,graph=ui.graph({{id="a"}},{})},
 kit.GraphCanvas{id="b",width=200,height=300,graph=ui.graph({{id="b"}},{})}}"#;
        let (f, mut s) = session_with(initial);
        let before = ["root.a", "root.b"].map(|id| {
            let canvas = &s.graphs[id];
            (
                id,
                canvas.data.topology.clone(),
                canvas.positions.to_vec(),
                canvas.command_revision,
            )
        });
        // Canvas a changes validly; canvas b's edge names a missing node, so the whole
        // update is rejected and neither engine hears about it.
        let invalid = r#"local ui=require("bead.ui") local kit=require("bead.kit")
return ui.Column{id="root",width=400,height=300,
 kit.GraphCanvas{id="a",width=200,height=300,graph=ui.graph({{id="a"},{id="new"}},{{from="a",to="new"}})},
 kit.GraphCanvas{id="b",width=200,height=300,graph=ui.graph({{id="b"}},{{from="b",to="missing"}})}}"#;
        fs::write(f.0.join("ui/main.luau"), invalid).unwrap();
        assert!(s.reload().is_err());
        for (id, topology, positions, revision) in before {
            let canvas = &s.graphs[id];
            assert_eq!(canvas.data.topology, topology);
            assert_eq!(canvas.positions.to_vec(), positions);
            assert_eq!(canvas.command_revision, revision);
        }
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
                        let r = canvas.geometry.radii[i] + slack;
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
    fn reorder_remaps_interaction_and_discards_older_snapshots() {
        let (f, mut s) = session();
        let positions = s.graphs["root.canvas"].positions.to_vec();
        let canvas = s.graphs.get_mut("root.canvas").unwrap();
        (canvas.hover, canvas.selected) = (Some(1), Some(2));
        let revision = canvas.command_revision;
        let reordered = r#"local ui=require("bead.ui") local kit=require("bead.kit")
return ui.Column{id="root",width=400,height=300,kit.GraphCanvas{id="canvas",width=400,height=300,
graph=ui.graph({{id="b",label="Beta"},{id="c"},{id="a",label="Alpha",weight=5}},
{{from="b",to="c"},{from="c",to="a"}}),style={node_radius=1}}}"#;
        fs::write(f.0.join("ui/main.luau"), reordered).unwrap();
        let scene = s.reload().unwrap();
        s.commit(scene);
        let c = s.graphs.get_mut("root.canvas").unwrap();
        let id = |i: Option<u32>| i.map(|i| c.data.topology.nodes[i as usize].id.to_string());
        assert_eq!(id(c.hover).as_deref(), Some("b"));
        assert_eq!(id(c.selected).as_deref(), Some("c"));
        assert_eq!(c.positions[0], positions[1]);
        // A snapshot computed for the old order would scramble the new one.
        assert!(c.command_revision > revision);
        let stale = Snapshot {
            command_revision: revision,
            positions: vec![[900., 900.]; 3].into(),
            settled: false,
        };
        let kept = c.positions.to_vec();
        assert!(!c.apply_snapshot(stale));
        assert_eq!(c.positions.to_vec(), kept);
    }

    #[test]
    fn graph_canvas_renders_hovers_selects_pans_and_zooms() {
        let (f, mut s) = session();
        let graphs: Vec<_> = s
            .current
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Graph { geometry, .. } => Some(geometry.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(graphs.len(), 1);
        assert_eq!(graphs[0].positions.len(), 3);
        assert_eq!(&*graphs[0].edges, &[[0, 1], [1, 2]]);
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
        assert_eq!(text(&s, "hovered"), Some(Value::Text("a".into())));
        let (ex, ey) = empty_point(&s);
        let scene = s.hover(ex, ey).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(text(&s, "hovered"), Some(Value::Text(String::new())));

        assert!(s.click(ax, ay).unwrap().is_none());
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert_eq!(text(&s, "selected"), Some(Value::Text("a".into())));
        assert_eq!(s.graphs["root.canvas"].selected, Some(0));

        // A drag pans, even from a node, and selects nothing.
        let center = s.graphs["root.canvas"].camera.center;
        let (bx, by) = screen(&s, 1);
        s.click(bx, by).unwrap();
        let scene = s.drag(bx + 40., by + 30.).unwrap().unwrap();
        s.commit(scene);
        assert_ne!(s.graphs["root.canvas"].camera.center, center);
        assert!(s.release().unwrap().is_none());
        assert_eq!(s.graphs["root.canvas"].selected, Some(0));

        let zoom = s.graphs["root.canvas"].camera.zoom;
        let scene = s.scroll(200., 150., -300.).unwrap().unwrap();
        s.commit(scene);
        assert!(s.graphs["root.canvas"].camera.zoom > zoom);
        let positions = s.graphs["root.canvas"].positions.to_vec();

        // A rename reaches the view, not the worker: positions stay where they were.
        let revision = s.graphs["root.canvas"].command_revision;
        fs::write(f.0.join("ui/main.luau"), view("Renamed")).unwrap();
        let scene = s.reload().unwrap();
        s.commit(scene);
        let c = &s.graphs["root.canvas"];
        assert_eq!(c.positions.to_vec(), positions);
        assert_eq!(c.command_revision, revision);
        assert_eq!(c.data.topology.nodes[1].label.as_ref(), "Renamed");
    }
}

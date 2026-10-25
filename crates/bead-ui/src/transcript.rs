// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reusable scrolling transcript: messages with a label, plain or Markdown text, code
//! blocks and copy actions, laid out as absolutely positioned cells and windowed to
//! what is visible. It knows nothing about any assistant or service: callers map their
//! messages to [`Entry`] values and render the cells with `kit.Transcript`.
//! Sizes, spacing and radii come from the shared size scale.
use crate::markdown::{self, Block};
use bead_render::{Primitive, Rect, Scene};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

const PITCH: f32 = 1.4;
/// A text box never covers more than this many bytes, so a pixmap stays small and a
/// streaming tail re-measures only its last group.
const GROUP: usize = 1500;

/// Spacing, radii and text sizes in logical pixels from `size-scale.json`.
#[derive(Clone, Debug)]
pub struct Scale {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub xxl: f32,
    pub radius_small: f32,
    pub body: f32,
    pub caption: f32,
    pub title: f32,
}
impl Scale {
    pub fn builtin() -> Self {
        let scale: Value = serde_json::from_str(include_str!("../assets/size-scale.json"))
            .expect("size scale is valid JSON");
        let n = |group: &str, name: &str| scale[group][name].as_f64().unwrap_or(0.) as f32;
        let px = |name: &str| n("text", name) * 4. / 3.;
        Self {
            xs: n("spacing", "xs"),
            sm: n("spacing", "sm"),
            md: n("spacing", "md"),
            xxl: n("spacing", "xxl"),
            radius_small: n("radii", "small"),
            body: px("body"),
            caption: px("caption"),
            title: px("title"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Style {
    pub width: f32,
    pub sans: String,
    pub mono: String,
    pub scale: Scale,
    /// Cells farther than this outside the viewport are not materialized.
    pub overscan: f32,
}
impl Style {
    pub fn new(width: f32, sans: &str, mono: &str) -> Self {
        Self {
            width,
            sans: sans.into(),
            mono: mono.into(),
            scale: Scale::builtin(),
            overscan: 300.,
        }
    }
    fn code(&self) -> f32 {
        self.scale.body - self.scale.xs * 0.75
    }
}

/// Chooses font files that cover a scene's text (the host's font policy).
pub type Picker = Arc<dyn Fn(&Scene) -> Vec<PathBuf> + Send + Sync>;

/// Wrapped line counts from the renderer's own shaper.
pub struct Meter {
    picker: Option<Picker>,
    fixed: Option<Vec<PathBuf>>,
    scene: RefCell<Option<Scene>>,
    covered: RefCell<BTreeSet<char>>,
    families: RefCell<[String; 2]>,
    lines: RefCell<HashMap<u64, usize>>,
    widths: RefCell<bead_core::cache::Lru<u64, f32>>,
}
impl Meter {
    pub fn new(picker: Picker) -> Self {
        Self {
            picker: Some(picker),
            fixed: None,
            scene: Default::default(),
            covered: RefCell::new((' '..='~').collect()),
            families: Default::default(),
            lines: Default::default(),
            widths: RefCell::new(bead_core::cache::Lru::new("text-widths", 32 * 1024)),
        }
    }
    /// A fixed font set, for deterministic tests.
    pub fn fixed(fonts: Vec<PathBuf>) -> Self {
        Self {
            picker: None,
            fixed: Some(fonts),
            scene: Default::default(),
            covered: RefCell::new((' '..='~').collect()),
            families: Default::default(),
            lines: Default::default(),
            widths: RefCell::new(bead_core::cache::Lru::new("text-widths", 32 * 1024)),
        }
    }
    fn prepare(&self, text: &str, families: [&str; 2]) {
        let mut covered = self.covered.borrow_mut();
        let mut fresh = false;
        for c in text.chars() {
            if !c.is_control() && covered.len() < 4096 && covered.insert(c) {
                fresh = true;
            }
        }
        let resolved = families.map(crate::text_style::family);
        let built = self.scene.borrow().is_some() && *self.families.borrow() == resolved;
        if built && (self.fixed.is_some() || !fresh) {
            return;
        }
        let mut scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 1.,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 1., 1.),
        };
        scene.font_files = match (&self.fixed, &self.picker) {
            (Some(fonts), _) => fonts.clone(),
            (None, Some(picker)) => {
                for family in families {
                    scene.primitives.push(Primitive::Text {
                        weight: 400,
                        align: None,
                        rect: Rect::new(0., 0., 100., 20.),
                        text: covered.iter().collect(),
                        family: crate::text_style::family(family),
                        size: 16.,
                        color: [0; 4],
                    });
                }
                picker(&scene)
            }
            _ => vec![],
        };
        self.lines.borrow_mut().clear();
        self.widths.borrow_mut().clear();
        *self.families.borrow_mut() = resolved;
        *self.scene.borrow_mut() = Some(scene);
    }
    /// Drawn lines for `text` at `width`. Measured slightly narrower than painted so a
    /// different output scale can never wrap into an unreserved extra line.
    pub fn lines(&self, text: &str, family: &str, size: f32, width: f32) -> usize {
        self.prepare(text, [family, family]);
        let family = if self.fixed.is_some() {
            "IBM Plex Sans".to_owned()
        } else {
            crate::text_style::family(family)
        };
        let size = crate::text_style::size(size, &family);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (text, &family, size.to_bits(), width.to_bits()).hash(&mut hash);
        let key = hash.finish();
        if let Some(lines) = self.lines.borrow().get(&key) {
            return *lines;
        }
        let measured = self.scene.borrow().as_ref().map_or(1, |scene| {
            bead_render::text_lines(scene, text, &family, size, width * 0.98).unwrap_or(1)
        });
        let mut cache = self.lines.borrow_mut();
        if cache.len() >= 1024 {
            cache.clear();
        }
        cache.insert(key, measured);
        measured
    }
    /// Unwrapped label width, using the same selected fonts and text-size offset as painting.
    pub fn width(&self, text: &str, family: &str, size: f32) -> f32 {
        self.prepare(text, [family, family]);
        let family = crate::text_style::family(family);
        let size = crate::text_style::size(size, &family);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (text, &family, size.to_bits()).hash(&mut hash);
        let key = hash.finish();
        if let Some(width) = self.widths.borrow().get(&key) {
            return *width;
        }
        let width = self.scene.borrow().as_ref().map_or(0., |scene| {
            bead_render::text_width(scene, text, &family, size).unwrap_or(0.)
        });
        let mut cache = self.widths.borrow_mut();
        let _ = cache.insert(key, width, 64);
        width
    }
    /// Pixel height of `text`, honouring the host's text-size offset.
    pub fn height(&self, text: &str, family: &str, size: f32, width: f32) -> f32 {
        let drawn = crate::text_style::size(size, &crate::text_style::family(family));
        self.lines(text, family, size, width) as f32 * drawn * PITCH
    }
    pub fn release(&self) {
        *self.scene.borrow_mut() = None;
        self.lines.borrow_mut().clear();
        self.widths.borrow_mut().clear();
    }
}

/// One message to show. Colour and fill values are palette role names.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Stable, digit/letter key; also scopes the cell ids.
    pub key: String,
    pub label: String,
    pub fill: String,
    pub text: String,
    pub markdown: bool,
    /// Buttons under the message: (label, action argument).
    pub actions: Vec<(String, String)>,
    /// For Markdown: (button label, action prefix); one button per closed code block,
    /// with argument `{prefix}:{key}:{index}`.
    pub code_action: Option<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Fill,
    Text,
    Button,
}
#[derive(Clone, Debug)]
struct Cell {
    id: String,
    kind: Kind,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: String,
    size: f32,
    mono: bool,
    color: String,
    fill: String,
    radius: f32,
    action: String,
}
impl Cell {
    fn at(id: String, kind: Kind, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            id,
            kind,
            x,
            y,
            w,
            h,
            text: String::new(),
            size: 0.,
            mono: false,
            color: "onSurface".into(),
            fill: "surfaceContainer".into(),
            radius: 0.,
            action: String::new(),
        }
    }
}
struct Laid {
    cells: Vec<Cell>,
    height: f32,
}

fn groups(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while rest.len() > GROUP {
        let mut cut = GROUP;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let at = rest[..cut]
            .rfind('\n')
            .or_else(|| rest[..cut].rfind(char::is_whitespace))
            .map_or(cut, |i| i + 1);
        out.push(&rest[..at]);
        rest = &rest[at..];
    }
    out.push(rest);
    out
}

struct Flow<'a> {
    meter: &'a Meter,
    style: &'a Style,
    cells: Vec<Cell>,
    id: &'a str,
    y: f32,
}
impl Flow<'_> {
    fn next_id(&self) -> String {
        format!("{}:{}", self.id, self.cells.len())
    }
    /// Stacked text boxes for `text`; returns the height used.
    fn text(&mut self, x: f32, width: f32, text: &str, size: f32, mono: bool, color: &str) -> f32 {
        let family = if mono {
            &self.style.mono
        } else {
            &self.style.sans
        }
        .clone();
        let mut used = 0.;
        for group in groups(text) {
            let group = group.trim_end_matches('\n');
            let h = self.meter.height(group, &family, size, width);
            let mut cell = Cell::at(self.next_id(), Kind::Text, x, self.y + used, width, h);
            cell.text = group.to_owned();
            cell.size = size;
            cell.mono = mono;
            cell.color = color.into();
            self.cells.push(cell);
            used += h;
        }
        used
    }
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, fill: &str, radius: f32) {
        let mut cell = Cell::at(self.next_id(), Kind::Fill, x, y, w, h);
        cell.fill = fill.into();
        cell.radius = radius;
        self.cells.push(cell);
    }
}

fn lay(meter: &Meter, style: &Style, entry: &Entry) -> Laid {
    let s = &style.scale;
    let (pad, body, small) = (s.md, s.body, s.caption - s.xs / 3.);
    let inner = style.width - 2. * pad;
    let mut flow = Flow {
        meter,
        style,
        cells: Vec::new(),
        id: &entry.key,
        y: pad * 2. + s.xs + 1.,
    };
    // The bubble comes first so it paints under everything; its height is patched last.
    flow.fill(0., 0., style.width, 0., &entry.fill, s.md + s.xs / 4.);
    let mut label = Cell::at(
        flow.next_id(),
        Kind::Text,
        pad,
        pad * 0.75,
        inner,
        small * 1.4,
    );
    label.text = entry.label.clone();
    label.size = small;
    label.color = "primary".into();
    flow.cells.push(label);
    if !entry.markdown {
        let used = flow.text(pad, inner, &entry.text, body, false, "onSurface");
        flow.y += used;
    } else {
        let mut first = true;
        for block in markdown::parse(&entry.text) {
            if !first {
                flow.y += s.sm;
            }
            first = false;
            let used = match block {
                Block::Paragraph(text) => flow.text(pad, inner, &text, body, false, "onSurface"),
                Block::Heading(level, text) => {
                    let size = body + [4., 3., 1.][usize::from(level.min(3)) - 1];
                    flow.text(pad, inner, &text, size, false, "onSurface")
                }
                Block::Item {
                    depth,
                    marker,
                    text,
                } => {
                    let indent = pad + f32::from(depth) * (s.md + s.xs);
                    let mut cell = Cell::at(
                        flow.next_id(),
                        Kind::Text,
                        indent,
                        flow.y,
                        s.xxl,
                        body * PITCH,
                    );
                    cell.text = marker;
                    cell.size = body;
                    cell.color = "primary".into();
                    flow.cells.push(cell);
                    flow.text(
                        indent + s.xxl - s.xs,
                        inner - indent + pad - s.xxl + s.xs,
                        &text,
                        body,
                        false,
                        "onSurface",
                    )
                }
                Block::Quote(text) => {
                    let top = flow.y;
                    let used = flow.text(
                        pad + s.md + s.xs,
                        inner - s.md - s.xs + s.xs,
                        &text,
                        body,
                        false,
                        "onSurfaceVariant",
                    );
                    flow.fill(pad, top, s.xs - 1., used, "outline", 1.);
                    used
                }
                Block::Code(text) => {
                    let top = flow.y;
                    let at = flow.cells.len();
                    flow.fill(pad, top, inner, 0., "surfaceContainerHigh", s.radius_small);
                    flow.y += s.sm;
                    let code = style.code();
                    let used = flow.text(
                        pad + s.sm,
                        inner - 2. * s.sm,
                        &text,
                        code,
                        true,
                        "onSurface",
                    );
                    flow.cells[at].h = used + 2. * s.sm;
                    flow.y = top;
                    used + 2. * s.sm
                }
                Block::Rule => {
                    flow.fill(pad, flow.y + s.xs, inner, 1., "outlineVariant", 0.);
                    s.sm + 1.
                }
            };
            flow.y += used;
        }
    }
    let end = flow.y;
    let mut buttons = entry.actions.clone();
    if entry.markdown
        && let Some((label, prefix)) = &entry.code_action
    {
        for index in 0..markdown::code_blocks(&entry.text).len() {
            buttons.push((label.clone(), format!("{prefix}:{}:{index}", entry.key)));
        }
    }
    let button_h = s.xxl + 2.5 * s.xs;
    let mut x = 0.;
    let mut button_y = end + s.xs + 1.;
    if !buttons.is_empty() {
        for (label, action) in buttons {
            let width = (meter.width(&label, &style.sans, body) + s.xxl + s.sm).min(style.width);
            if x > 0. && x + width > style.width {
                x = 0.;
                button_y += button_h + s.xs + 1.;
            }
            let mut cell = Cell::at(flow.next_id(), Kind::Button, x, button_y, width, button_h);
            cell.text = label;
            cell.action = action;
            cell.size = body;
            flow.cells.push(cell);
            x += width + s.xs + 1.;
        }
    }
    let height = button_y + button_h + pad - 1.;
    flow.cells[0].h = height;
    Laid {
        cells: flow.cells,
        height,
    }
}

/// Layout cache and window for one transcript.
pub struct Transcript {
    style: Style,
    meter: Meter,
    laid: HashMap<String, (u64, Rc<Laid>)>,
    order: Vec<(f32, Rc<Laid>)>,
    extent: f32,
}
impl Transcript {
    pub fn new(style: Style, meter: Meter) -> Self {
        Self {
            style,
            meter,
            laid: HashMap::new(),
            order: Vec::new(),
            extent: 0.,
        }
    }
    pub fn style(&self) -> &Style {
        &self.style
    }
    pub fn meter(&self) -> &Meter {
        &self.meter
    }
    /// Lay out `entries`; unchanged messages reuse their cells, so streaming text only
    /// re-measures its last group. Returns the total height.
    pub fn layout(&mut self, entries: &[Entry]) -> f32 {
        let mut seen = BTreeSet::new();
        self.order.clear();
        let mut y = 0.;
        for entry in entries {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                &entry.text,
                &entry.label,
                &entry.fill,
                entry.markdown,
                &entry.actions,
                &entry.code_action,
                self.style.width.to_bits(),
            )
                .hash(&mut hash);
            let hash = hash.finish();
            let laid = match self.laid.get(&entry.key) {
                Some((known, laid)) if *known == hash => laid.clone(),
                _ => {
                    let laid = Rc::new(lay(&self.meter, &self.style, entry));
                    self.laid.insert(entry.key.clone(), (hash, laid.clone()));
                    laid
                }
            };
            seen.insert(entry.key.clone());
            self.order.push((y, laid.clone()));
            y += laid.height + self.style.scale.md;
        }
        self.laid.retain(|key, _| seen.contains(key));
        self.extent = if self.order.is_empty() {
            0.
        } else {
            y - self.style.scale.md
        };
        self.extent
    }
    /// Cells overlapping `[scroll - overscan, scroll + height + overscan]`, as model rows.
    pub fn window(&self, scroll: f32, height: f32) -> Vec<Value> {
        let (low, high) = (
            scroll - self.style.overscan,
            scroll + height + self.style.overscan,
        );
        let mut out = Vec::new();
        for (top, laid) in &self.order {
            if top + laid.height < low || *top > high {
                continue;
            }
            for c in &laid.cells {
                out.push(json!({
                    "id": c.id,
                    "k": match c.kind { Kind::Fill => "fill", Kind::Text => "text", Kind::Button => "button" },
                    "x": c.x, "y": c.y + top, "w": c.w, "h": c.h,
                    "t": c.text, "s": c.size,
                    "f": if c.mono { &self.style.mono } else { &self.style.sans },
                    "c": c.color, "b": c.fill, "r": c.radius, "a": c.action,
                }));
            }
        }
        out
    }
    /// Drop caches and fonts (the panel is closed).
    pub fn release(&mut self) {
        self.laid.clear();
        self.order.clear();
        self.extent = 0.;
        self.meter.release();
    }
}

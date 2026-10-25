// SPDX-License-Identifier: GPL-3.0-only
//! Nacre's drawers/layout.js, e0a81ce7. Keep application policy out of bead-wayland.
use bead::{
    core::Edge,
    render::{Rect, Scene},
    wayland::PanelPositioner,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attachment {
    Attached(Edge),
    /// Luau owns the attached edge; the argument is the cold-scene default.
    SceneEdge(Edge),
    /// Compact Nacre control panel: joins the top frame at the usable top-right.
    ControlCenter,
    Popout,
    /// B0048: detached from every edge, a free-floating body with its own border.
    Pearl,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub rounding: f32,
}
impl Frame {
    /// NacreFrame defaults: 40px header + 10px frame, 25px contour rounding.
    pub const DEFAULT: Frame = Frame {
        left: 10.,
        right: 10.,
        top: 50.,
        bottom: 10.,
        rounding: 25.,
    };
    /// The frame the settings currently describe. Positioners and the static tiles read
    /// it when they run, so a settings change needs no restart (B0047).
    pub fn current() -> Frame {
        *LIVE.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Publish a new live frame; true when it differs from the previous one.
    pub fn set_current(frame: Frame) -> bool {
        let mut live = LIVE.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::replace(&mut *live, frame) != frame
    }
    /// `NacreFrame.qml`: the header is the 40 px bar (when enabled) plus the top frame
    /// edge; each side is the frame width unless its toggle is off.
    pub fn from_settings(s: &FrameSettings) -> Frame {
        let width = s.width.max(0.);
        let edge = |on: bool| if on { width } else { 0. };
        Frame {
            left: edge(s.left),
            right: edge(s.right),
            bottom: edge(s.bottom),
            top: if s.bar { BAR_HEIGHT } else { 0. } + edge(s.top),
            rounding: s.rounding.max(0.),
        }
    }
}
/// The presentation palette the frame art follows (surface, primary, secondary); `None` until
/// the wallpaper palette arrives, when each chrome keeps the colours it was built with.
static LIVE_COLORS: std::sync::Mutex<Option<[[u8; 4]; 3]>> = std::sync::Mutex::new(None);
/// Publish the live presentation palette; true when the frame art colours changed.
pub fn set_live_palette(palette: &std::collections::BTreeMap<String, [u8; 4]>) -> bool {
    let (Some(fill), Some(primary), Some(secondary)) = (
        palette.get("surface"),
        palette.get("primary"),
        palette.get("secondary"),
    ) else {
        return false;
    };
    let next = Some([*fill, *primary, *secondary]);
    let mut live = LIVE_COLORS.lock().unwrap_or_else(|p| p.into_inner());
    std::mem::replace(&mut *live, next) != next
}
/// B0056: a panel another process draws (the AI sidebar app) still takes its place in the frame.
/// The host describes it (edge attachment and body size); the frame tiles then leave it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct External {
    pub attachment: Attachment,
    pub size: (f32, f32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct ExternalState {
    geometry: Option<External>,
    visible: bool,
}
#[derive(Default)]
struct ExternalPanels(Vec<(String, ExternalState)>);
impl ExternalPanels {
    fn set_geometry(&mut self, name: &str, geometry: Option<External>) -> bool {
        let before = self.0.iter().position(|(n, _)| n == name);
        match (before, geometry) {
            (Some(i), Some(p)) if self.0[i].1.geometry == Some(p) => false,
            (Some(i), Some(p)) => {
                self.0[i].1.geometry = Some(p);
                true
            }
            (Some(i), None) => {
                self.0.remove(i);
                true
            }
            (None, Some(p)) => {
                self.0.push((
                    name.to_owned(),
                    ExternalState {
                        geometry: Some(p),
                        visible: true,
                    },
                ));
                true
            }
            (None, None) => false,
        }
    }

    fn set_visible(&mut self, name: &str, visible: bool) -> bool {
        if let Some((_, state)) = self.0.iter_mut().find(|(n, _)| n == name) {
            if state.visible == visible {
                return false;
            }
            state.visible = visible;
        } else {
            self.0.push((
                name.to_owned(),
                ExternalState {
                    geometry: None,
                    visible,
                },
            ));
        }
        true
    }

    fn is_open(&self, name: &str) -> bool {
        self.0.iter().any(|(n, state)| n == name && state.visible)
    }

    fn has_geometry(&self, name: &str) -> bool {
        self.0
            .iter()
            .any(|(n, state)| n == name && state.geometry.is_some())
    }

    fn visible_geometry(&self) -> Vec<External> {
        self.0
            .iter()
            .filter_map(|(_, state)| if state.visible { state.geometry } else { None })
            .collect()
    }
}
static EXTERNAL: std::sync::Mutex<ExternalPanels> =
    std::sync::Mutex::new(ExternalPanels(Vec::new()));
static EXTERNAL_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
type Notify = Box<dyn Fn() + Send + Sync>;
static EXTERNAL_NOTIFY: std::sync::Mutex<Option<Notify>> = std::sync::Mutex::new(None);
/// Describe (Some) or drop (None) an external panel; the frame tiles rebuild.
pub fn set_external(name: &str, panel: Option<External>) {
    let changed = {
        let mut all = EXTERNAL.lock().unwrap_or_else(|p| p.into_inner());
        all.set_geometry(name, panel)
    };
    if !changed {
        return;
    }
    EXTERNAL_REV.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Some(notify) = &*EXTERNAL_NOTIFY.lock().unwrap_or_else(|p| p.into_inner()) {
        notify();
    }
}
/// Update an external panel's live visibility while retaining its last geometry for a warm reopen.
pub fn set_external_visible(name: &str, visible: bool) {
    let changed = {
        let mut all = EXTERNAL.lock().unwrap_or_else(|p| p.into_inner());
        all.set_visible(name, visible)
    };
    if !changed {
        return;
    }
    EXTERNAL_REV.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Some(notify) = &*EXTERNAL_NOTIFY.lock().unwrap_or_else(|p| p.into_inner()) {
        notify();
    }
}
pub fn external_open(name: &str) -> bool {
    EXTERNAL
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_open(name)
}
pub fn has_external_geometry(name: &str) -> bool {
    EXTERNAL
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .has_geometry(name)
}
pub fn external_rev() -> u64 {
    EXTERNAL_REV.load(std::sync::atomic::Ordering::SeqCst)
}
/// The shell registers how its decoration variant is bumped and its watchers run.
pub fn on_external_change(notify: Notify) {
    *EXTERNAL_NOTIFY.lock().unwrap_or_else(|p| p.into_inner()) = Some(notify);
}
fn externals() -> Vec<External> {
    EXTERNAL
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .visible_geometry()
}
static LIVE: std::sync::Mutex<Frame> = std::sync::Mutex::new(Frame::DEFAULT);
/// The Nacre bar's own height inside the header (the rest is the top frame edge).
pub const BAR_HEIGHT: f32 = 40.;
/// Decoded frame settings (`frameWidth`, `frameRounding`, the four side toggles, bar on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSettings {
    pub width: f32,
    pub rounding: f32,
    pub left: bool,
    pub right: bool,
    pub bottom: bool,
    pub top: bool,
    pub bar: bool,
}
impl Default for Frame {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// Opt-in Luau panels own their edge; keep positioning and joined chrome in sync
// with the motion token, including a user moving a panel to the opposite edge.
fn scene_attachment(fallback: Attachment, scene: &Scene) -> Attachment {
    match (fallback, scene.motion.as_ref()) {
        (Attachment::SceneEdge(edge), motion) => {
            Attachment::Attached(motion.map_or(edge, |motion| motion.edge))
        }
        _ => fallback,
    }
}

pub fn attached(edge: Edge, viewport: (f32, f32), size: (f32, f32), offset: f32) -> (f32, f32) {
    let centered_x = ((viewport.0 - size.0) / 2.).max(0.);
    let centered_y = ((viewport.1 - size.1) / 2.).max(0.);
    match edge {
        Edge::Left => (0., centered_y),
        Edge::Right => ((viewport.0 - size.0 - offset).max(0.), centered_y),
        Edge::Bottom => (centered_x, (viewport.1 - size.1).max(0.)),
        Edge::Top => (centered_x, 0.),
    }
}

pub fn popout(
    viewport_x: f32,
    viewport_width: f32,
    center: f32,
    width: f32,
    target_width: f32,
    corner: f32,
) -> (f32, bool) {
    let relative_center = center - viewport_x;
    let safe_corner = corner.min(viewport_width / 2.).max(0.);
    let dock_right = relative_center + target_width / 2. >= viewport_width - safe_corner;
    let right_limit = (viewport_width - width).max(0.);
    let centered = relative_center - width / 2.;
    (
        if dock_right {
            right_limit
        } else {
            right_limit.min(safe_corner.max(centered))
        },
        dock_right,
    )
}

pub fn body(scene: &Scene) -> Rect {
    if let Some(art) = scene.motion.as_ref().and_then(|m| m.art.as_ref()) {
        return art.body;
    }
    scene
        .primitives
        .iter()
        .find_map(|p| match p {
            bead::render::Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                Some(*rect)
            }
            bead::render::Primitive::Image { rect, path, .. }
                if path.to_string_lossy().starts_with("@joined-frame-") =>
            {
                let top = if path.to_string_lossy().starts_with("@joined-frame-32-") {
                    32.
                } else {
                    2.
                };
                Some(Rect::new(
                    rect.x + 32.,
                    rect.y + top,
                    rect.w - 64.,
                    rect.h - 32. - top,
                ))
            }
            _ => None,
        })
        .unwrap_or(Rect::new(
            0.,
            0.,
            scene.width,
            (scene.bounds.h - 64.).max(0.),
        ))
}

static SHEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// `frameSheen` (B0036): the 1 px gradient rim along the frame. Off draws no rim.
pub fn set_sheen(on: bool) {
    SHEEN.store(on, std::sync::atomic::Ordering::SeqCst);
}
fn sheen() -> bool {
    SHEEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// Worker-owned and transient. No module, raster or generated asset survives the
/// existing panel worker's close/idle retirement. Renderer caches remain capped.
#[derive(Clone)]
pub struct Chrome {
    pub center: Option<f32>,
    pub output: (u32, u32),
    attachment: Attachment,
    engaged: bool,
    fill: [u8; 4],
    primary: [u8; 4],
    secondary: [u8; 4],
    ink: Option<crate::pearls::Ink>,
}
impl Chrome {
    pub fn set_engaged(&mut self, engaged: bool) {
        self.engaged = engaged;
    }
    pub fn engaged(&self) -> bool {
        self.engaged
    }
    pub fn new(
        attachment: Attachment,
        colors: &bead::render::Palette,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            center: None,
            output: (1920, 1200),
            attachment,
            engaged: matches!(
                attachment,
                Attachment::Popout
                    | Attachment::ControlCenter
                    | Attachment::Attached(Edge::Left | Edge::Right)
            ),
            fill: colors.get("surface")?,
            primary: colors.get("primary")?,
            secondary: colors.get("secondary")?,
            ink: matches!(attachment, Attachment::Pearl)
                .then(|| crate::pearls::Ink::new(colors))
                .transpose()?,
        })
    }
    fn svg(
        &self,
        frame: Frame,
        output: (f32, f32),
        rectangles: &[Rect],
        crop: Rect,
        engaged: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let [fill, primary, secondary] = LIVE_COLORS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .unwrap_or([self.fill, self.primary, self.secondary]);
        let radius = frame.radius(output) as f64;
        let contour =
            crate::contour::geometry(output.0 as f64, output.1 as f64, rectangles, radius)?;
        let rgb = |c: [u8; 4]| format!("rgb({},{},{})", c[0], c[1], c[2]);
        let highlight = std::array::from_fn::<_, 4, _>(|i| {
            if i == 3 {
                255
            } else {
                (primary[i] as f32 * 0.78 + [255., 249., 241.][i] * 0.22).round() as u8
            }
        });
        let rim = if sheen() {
            format!(
                r#"<path d="{}" fill="none" stroke="url(#rim)" stroke-width="1" opacity="{}" stroke-linecap="butt"/>"#,
                contour.rim,
                if engaged { 0.25 } else { 0.16 }
            )
        } else {
            String::new()
        };
        Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><defs><linearGradient id="rim" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="{}" y2="{}"><stop stop-color="{}"/><stop offset=".38" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs><path d="{}" fill="{}" fill-rule="evenodd"/>{}</svg>"#,
            crop.w,
            crop.h,
            crop.x,
            crop.y,
            crop.w,
            crop.h,
            output.0,
            output.1,
            rgb(highlight),
            rgb(primary),
            rgb(secondary),
            contour.body,
            rgb(fill),
            rim
        ))
    }
    /// The static frame tiles. The factory reads [`Frame::current`] on every call, so a
    /// settings change (and a decoration variant bump) redraws them with the new frame.
    pub fn decorations(self) -> bead::wayland::DecorationFactory {
        Arc::new(move |(w, h), engaged, panel| self.tiles(Frame::current(), (w, h), engaged, panel))
    }
    /// Tiles for an explicit frame: only the strips that carry material are made, each
    /// including the rim's half pixel on both sides (disjoint crops, even coordinates).
    pub fn tiles(
        &self,
        frame: Frame,
        (w, h): (u32, u32),
        engaged: bool,
        panel: Option<bead::wayland::DecorationPanel<'_>>,
    ) -> Result<Vec<bead::wayland::Decoration>, String> {
        let (w, h) = (w as f32, h as f32);
        let radius = frame.radius((w, h));
        let mut material = frame.material((w, h));
        for external in externals() {
            material.push(frame.panel(external.attachment, (w, h), external.size, None));
        }
        if let Some(panel) = panel {
            material.push(body(panel.scene).translated(
                panel.origin.0 as f32 - panel.scene.bounds.x,
                panel.origin.1 as f32 - panel.scene.bounds.y,
            ));
        }
        let Frame {
            left,
            right,
            top,
            bottom,
            ..
        } = frame;
        let side_top = if top > 0. { top + 2. } else { 0. };
        let side_bottom = if bottom > 0. { h - bottom - 2. } else { h };
        let corner = (radius - 1.).max(0.);
        let mut crops = Vec::new();
        if left > 0. {
            crops.push(Rect::new(0., side_top, left + 2., side_bottom - side_top));
        }
        if right > 0. {
            crops.push(Rect::new(
                w - right - 2.,
                side_top,
                right + 2.,
                side_bottom - side_top,
            ));
        }
        if bottom > 0. {
            crops.push(Rect::new(0., h - bottom - 2., w, bottom + 2.));
        }
        if top > 0. {
            crops.push(Rect::new(0., top - 2., w, 4.));
        }
        // A concave corner exists where two frame edges meet; a screen edge has none.
        let tl = left > 0. && top > 0.;
        let tr = right > 0. && top > 0.;
        let bl = left > 0. && bottom > 0.;
        let br = right > 0. && bottom > 0.;
        for (on, x, y) in [
            (tl, left + 2., top + 2.),
            (tr, w - right - 1. - radius, top + 2.),
            (bl, left + 2., h - bottom - 1. - radius),
            (br, w - right - 1. - radius, h - bottom - 1. - radius),
        ] {
            if on {
                crops.push(Rect::new(x, y, corner, corner));
            }
        }
        crops
            .into_iter()
            .filter(|r| r.w > 0. && r.h > 0.)
            .map(|rect| {
                let svg = self
                    .svg(frame, (w, h), &material, rect, engaged)
                    .map_err(|e| e.to_string())?;
                Ok(bead::wayland::Decoration {
                    rect,
                    svg: Arc::from(svg.into_bytes()),
                })
            })
            .collect()
    }
    /// A pearl keeps its shadow; the panel's root background takes the pearl's colour and
    /// corners, and the border is drawn above everything as small pieces (see
    /// `pearls::border_svgs`). Decorating a decorated scene again replaces the pieces.
    fn decorate_pearl(&self, scene: &mut Scene) -> Result<(), Box<dyn std::error::Error>> {
        use bead::render::Primitive;
        use std::hash::{Hash, Hasher};
        let ink = self.ink.ok_or("pearl chrome without ink")?;
        let is_piece = |path: &std::path::Path| path.to_string_lossy().starts_with("@pearl-edge-");
        while matches!(scene.primitives.last(), Some(Primitive::Image { path, .. }) if is_piece(path))
        {
            scene.primitives.pop();
        }
        scene.asset_data.retain(|path, _| !is_piece(path));
        let body = body(scene);
        let root = scene
            .primitives
            .iter_mut()
            .find(|p| matches!(p, Primitive::Rounded { rect, .. } if *rect == body))
            .ok_or("Panel root background missing")?;
        *root = Primitive::Rounded {
            rect: body,
            radius: crate::pearls::RADIUS,
            fill: ink.body,
            border: None,
        };
        for (piece, svg) in crate::pearls::border_svgs(body.w, body.h, &ink, self.engaged) {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            svg.hash(&mut hash);
            let path = std::path::PathBuf::from(format!("@pearl-edge-{:x}.svg", hash.finish()));
            scene
                .asset_data
                .insert(path.clone(), Arc::from(svg.into_bytes()));
            scene.primitives.push(Primitive::Image {
                blur: 0.,
                fit: bead::render::ImageFit::Fill,
                clip_polygon: None,
                rect: Rect::new(body.x + piece.x, body.y + piece.y, piece.w, piece.h),
                path,
            });
        }
        scene.prepared.clear();
        Ok(())
    }
    pub fn decorate(&self, scene: &mut Scene) -> Result<(), Box<dyn std::error::Error>> {
        self.decorate_with(Frame::current(), scene)
    }
    /// As [`Chrome::decorate`] for an explicit frame.
    pub fn decorate_with(
        &self,
        frame: Frame,
        scene: &mut Scene,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use bead::render::Primitive;
        use std::hash::{Hash, Hasher};
        if matches!(self.attachment, Attachment::Pearl) {
            return self.decorate_pearl(scene);
        }
        let attachment = scene_attachment(self.attachment, scene);
        let engaged = self.engaged
            || (matches!(self.attachment, Attachment::SceneEdge(_))
                && matches!(attachment, Attachment::Attached(Edge::Left | Edge::Right)));
        let body = body(scene);
        let output = (self.output.0 as f32, self.output.1 as f32);
        let panel = frame.panel(attachment, output, (body.w, body.h), self.center);
        let top_inset = if matches!(
            attachment,
            Attachment::Popout | Attachment::ControlCenter | Attachment::Attached(Edge::Top)
        ) || (matches!(attachment, Attachment::Attached(_))
            && panel.y <= frame.top)
        {
            2.
        } else {
            32.
        };
        let mut rectangles = frame.material(output);
        rectangles.push(panel);
        let svg = self.svg(
            frame,
            output,
            &rectangles,
            Rect::new(
                panel.x - 32.,
                panel.y - top_inset,
                body.w + 64.,
                body.h + 32. + top_inset,
            ),
            engaged,
        )?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        svg.hash(&mut hash);
        let path =
            std::path::PathBuf::from(format!("@joined-frame-{top_inset}-{:x}.svg", hash.finish()));
        scene
            .asset_data
            .retain(|path, _| !path.to_string_lossy().starts_with("@joined-frame-"));
        scene
            .asset_data
            .insert(path.clone(), Arc::from(svg.into_bytes()));
        let mut replaced = false;
        for primitive in &mut scene.primitives {
            if !replaced && matches!(primitive, Primitive::Shadow { .. }) {
                *primitive = Primitive::Rounded {
                    rect: Rect::new(0., 0., 0., 0.),
                    radius: 0.,
                    fill: [0; 4],
                    border: None,
                };
                continue;
            }
            let is_root = match primitive {
                Primitive::Rounded { rect, .. } => {
                    rect.w == body.w && rect.h == body.h && rect.x == body.x && rect.y == body.y
                }
                Primitive::Image { path, .. } => {
                    path.to_string_lossy().starts_with("@joined-frame-")
                }
                _ => false,
            };
            if !replaced && is_root {
                *primitive = Primitive::Image {
                    blur: 0.,
                    fit: bead::render::ImageFit::Fill,
                    clip_polygon: None,
                    rect: Rect::new(
                        body.x - 32.,
                        body.y - top_inset,
                        body.w + 64.,
                        body.h + 32. + top_inset,
                    ),
                    path: path.clone(),
                };
                replaced = true;
            }
        }
        if !replaced {
            return Err("Panel root background missing".into());
        }
        if scene.motion.as_ref().is_some_and(|m| m.reveal) {
            let index = scene
                .primitives
                .iter()
                .position(|p| matches!(p, Primitive::Image { path: p, .. } if *p == path))
                .ok_or("Missing frame artwork")?;
            let chrome = self.clone();
            let original = scene.asset_data[&path].clone();
            let crop = Rect::new(
                panel.x - 32.,
                panel.y - top_inset,
                body.w + 64.,
                body.h + 32. + top_inset,
            );
            let motion = scene.motion.as_mut().unwrap();
            motion.parts = vec![(index, scene.primitives[index].clone())];
            motion.art = Some(bead::render::RevealArt {
                body,
                index,
                draw: Arc::new(move |revealed| {
                    let mut rectangles = frame.material(output);
                    rectangles.push(Rect::new(
                        panel.x + revealed.x - body.x,
                        panel.y + revealed.y - body.y,
                        revealed.w,
                        revealed.h,
                    ));
                    // Trim transparent rows/columns before SVG rasterization. A 120 px
                    // logical grid keeps crop offsets integral in scale120 physical pixels.
                    let visible = Rect::new(
                        panel.x + revealed.x - body.x - 32.,
                        panel.y + revealed.y - body.y - top_inset,
                        revealed.w + 64.,
                        revealed.h + 32. + top_inset,
                    );
                    let x = crop.x + ((visible.x - crop.x) / 120.).floor() * 120.;
                    let y = crop.y + ((visible.y - crop.y) / 120.).floor() * 120.;
                    let right = (crop.x + ((visible.x + visible.w - crop.x) / 120.).ceil() * 120.)
                        .min(crop.x + crop.w);
                    let bottom = (crop.y + ((visible.y + visible.h - crop.y) / 120.).ceil() * 120.)
                        .min(crop.y + crop.h);
                    let trimmed = Rect::new(x, y, right - x, bottom - y);
                    let local = |r: Rect| {
                        Rect::new(body.x + r.x - panel.x, body.y + r.y - panel.y, r.w, r.h)
                    };
                    match chrome.svg(frame, output, &rectangles, trimmed, engaged) {
                        Ok(svg) => (local(trimmed), Arc::from(svg.into_bytes())),
                        Err(error) => {
                            eprintln!("FRAME_REVEAL_FAILED {error}");
                            (local(crop), original.clone())
                        }
                    }
                }),
            });
        }
        scene.prepared.clear();
        Ok(())
    }
}

impl Frame {
    /// Contour rounding, bounded by the room left between the frame edges
    /// (`NacreChrome`: `min(rounding, width / 2, height / 2)` of the opening).
    pub fn radius(self, output: (f32, f32)) -> f32 {
        self.rounding
            .min(((output.0 - self.left - self.right).max(0.)) / 2.)
            .min(((output.1 - self.top - self.bottom).max(0.)) / 2.)
    }
    /// The frame's own material rectangles (header and the enabled sides).
    pub fn material(self, (w, h): (f32, f32)) -> Vec<Rect> {
        let mut rects = Vec::new();
        if self.top > 0. {
            rects.push(Rect::new(0., 0., w, self.top));
        }
        if self.left > 0. {
            rects.push(Rect::new(0., 0., self.left, h));
        }
        if self.right > 0. {
            rects.push(Rect::new(w - self.right, 0., self.right, h));
        }
        if self.bottom > 0. {
            rects.push(Rect::new(0., h - self.bottom, w, self.bottom));
        }
        rects
    }
    pub fn panel(
        self,
        attachment: Attachment,
        output: (f32, f32),
        size: (f32, f32),
        center: Option<f32>,
    ) -> Rect {
        let viewport = (
            (output.0 - self.left - self.right).max(0.),
            (output.1 - self.top - self.bottom).max(0.),
        );
        let (x, y) = match attachment {
            Attachment::Attached(edge) | Attachment::SceneEdge(edge) => {
                attached(edge, viewport, size, 0.)
            }
            Attachment::ControlCenter => {
                let (x, _) = attached(Edge::Right, viewport, size, 0.);
                (x, 0.)
            }
            Attachment::Pearl => (0., 0.),
            Attachment::Popout => (
                popout(
                    self.left,
                    viewport.0,
                    center.unwrap_or(output.0 / 2.),
                    size.0,
                    size.0,
                    self.rounding,
                )
                .0,
                0.,
            ),
        };
        Rect::new(self.left + x, self.top + y, size.0, size.1)
    }

    /// Panel origin for the live frame (settings change it without a restart).
    pub fn positioner(self, attachment: Attachment) -> PanelPositioner {
        let _ = self;
        Arc::new(move |scene, center, output| {
            let body = body(scene);
            let panel = Frame::current().panel(
                scene_attachment(attachment, scene),
                (output.0 as f32, output.1 as f32),
                (body.w, body.h),
                center,
            );
            // Surface pixels include the scene's shadow/contour inset. Align the
            // visible body, never that padded surface rectangle.
            (
                (panel.x - body.x + scene.bounds.x).round() as i32,
                (panel.y - body.y + scene.bounds.y).round() as i32,
            )
        })
    }
}

#[cfg(test)]
#[path = "placement_pixels_tests.rs"]
mod pixels;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warm_external_visibility_retains_geometry_without_reopening_the_frame() {
        let mut panels = ExternalPanels::default();
        let geometry = External {
            attachment: Attachment::Attached(Edge::Left),
            size: (320., 640.),
        };
        panels.set_geometry("visibility-test", Some(geometry));
        panels.set_visible("visibility-test", false);
        assert!(!panels.is_open("visibility-test"));
        assert!(panels.has_geometry("visibility-test"));
        assert!(!panels.visible_geometry().contains(&geometry));

        // A close can arrive before the first geometry description; that later description
        // must preserve the closed state.
        panels.set_visible("visibility-before-geometry-test", false);
        panels.set_geometry("visibility-before-geometry-test", Some(geometry));
        assert!(!panels.is_open("visibility-before-geometry-test"));
        assert!(panels.has_geometry("visibility-before-geometry-test"));

        panels.set_visible("visibility-test", true);
        assert!(panels.is_open("visibility-test"));
        assert!(panels.visible_geometry().contains(&geometry));
    }
    fn colors() -> bead::render::Palette {
        bead::render::Palette::read(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"),
        )
        .unwrap()
    }
    #[test]
    fn attached_position_and_chrome_follow_the_luau_edge() {
        let colors = colors();
        let mut scene = crate::sound::fallback(colors.get("surface").unwrap());
        scene.motion.as_mut().unwrap().edge = Edge::Bottom;
        let frame = Frame::default();
        let bottom = frame.positioner(Attachment::Attached(Edge::Bottom));
        let token = frame.positioner(Attachment::SceneEdge(Edge::Top));
        assert_eq!(
            bottom(&scene, None, (1920, 1200)),
            token(&scene, None, (1920, 1200))
        );
        let chrome = Chrome::new(Attachment::SceneEdge(Edge::Top), &colors).unwrap();
        chrome.decorate(&mut scene).unwrap();
        assert!(
            scene
                .asset_data
                .keys()
                .any(|p| p.to_string_lossy().starts_with("@joined-frame-32-"))
        );
        scene.motion.as_mut().unwrap().edge = Edge::Top;
        let top_origin = token(&scene, None, (1920, 1200));
        scene.motion.as_mut().unwrap().edge = Edge::Bottom;
        assert_ne!(top_origin, token(&scene, None, (1920, 1200)));
        for edge in [Edge::Left, Edge::Top] {
            let mut token_scene = crate::sound::fallback(colors.get("surface").unwrap());
            token_scene.motion.as_mut().unwrap().edge = edge;
            let mut fixed_scene = token_scene.clone();
            Chrome::new(Attachment::SceneEdge(Edge::Bottom), &colors)
                .unwrap()
                .decorate(&mut token_scene)
                .unwrap();
            Chrome::new(Attachment::Attached(edge), &colors)
                .unwrap()
                .decorate(&mut fixed_scene)
                .unwrap();
            assert_eq!(token_scene.asset_data, fixed_scene.asset_data);
        }
    }
    #[test]
    fn fullscreen_bottom_chrome_only_overlaps_the_frame_header_by_two_pixels() {
        use bead::render::Primitive;

        let colors = colors();
        let mut scene = crate::sound::fallback(colors.get("surface").unwrap());
        let body = Rect::new(0., 0., 1900., 1140.);
        scene.width = body.w;
        scene.bounds = Rect::new(-32., -32., body.w + 64., body.h + 64.);
        if let Some(Primitive::Rounded { rect, .. }) = scene
            .primitives
            .iter_mut()
            .find(|primitive| matches!(primitive, Primitive::Rounded { rect, .. } if rect.w > 0.))
        {
            *rect = body;
        }

        Chrome::new(Attachment::Attached(Edge::Bottom), &colors)
            .unwrap()
            .decorate_with(Frame::DEFAULT, &mut scene)
            .unwrap();

        let artwork = scene
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                Primitive::Image { rect, path, .. }
                    if path.to_string_lossy().starts_with("@joined-frame-") =>
                {
                    Some((*rect, path))
                }
                _ => None,
            });
        let (rect, path) = artwork.expect("joined chrome artwork");
        assert!(path.to_string_lossy().starts_with("@joined-frame-2-"));
        assert_eq!(rect.y, body.y - 2.);
        let panel = Frame::DEFAULT.panel(
            Attachment::Attached(Edge::Bottom),
            (1920., 1200.),
            (body.w, body.h),
            None,
        );
        assert_eq!(panel.y + rect.y, Frame::DEFAULT.top - 2.);
    }
    #[test]
    fn revealing_joined_art_rebakes_the_growing_contour_without_moving_content() {
        let colors = colors();
        let chrome = Chrome::new(Attachment::Attached(Edge::Top), &colors).unwrap();
        let mut scene = crate::sound::fallback(colors.get("surface").unwrap());
        let motion = scene.motion.as_mut().unwrap();
        motion.reveal = true;
        motion.edge = Edge::Top;
        chrome.decorate(&mut scene).unwrap();
        let full_body = body(&scene);
        let index = scene.motion.as_ref().unwrap().art.as_ref().unwrap().index;
        let (full_rect, full_svg) = match &scene.primitives[index] {
            bead::render::Primitive::Image { rect, path, .. } => {
                (*rect, scene.asset_data[path].clone())
            }
            _ => panic!("frame artwork must be an image"),
        };
        let mut cache = bead::render::tree::TreeCache::default();
        cache.prepare(&scene, 1.).unwrap();
        scene
            .prepared
            .insert(1_f32.to_bits(), cache.frame.clone().unwrap());
        let origin = Frame::default().positioner(Attachment::Attached(Edge::Top));
        let before = origin(&scene, None, (1920, 1200));
        for progress in [0.5, 0.25, 0.75] {
            scene.reveal(progress);
            cache.prepare(&scene, 1.).unwrap();
            let frame = cache.frame.as_ref().unwrap();
            let item = &frame.items[index];
            assert!(
                item.clip.is_none(),
                "art must keep its curved edge beyond the content clip"
            );
            let pixels = &item.raster.as_ref().unwrap().pixels;
            let bottom = (2. + full_body.h * progress).round() as u32;
            assert!(
                pixels
                    .pixel((32. + full_body.w / 2.) as u32, bottom - 2)
                    .unwrap()
                    .alpha()
                    > 0
            );
            assert_eq!(
                pixels.pixel(33, bottom - 2).unwrap().alpha(),
                0,
                "growing corner stays rounded"
            );
            assert_eq!(
                pixels
                    .pixel((32. + full_body.w / 2.) as u32, bottom + 2)
                    .unwrap()
                    .alpha(),
                0
            );
            if progress == 0.25 {
                // Reference is the same contour in the former full-height SVG
                // viewport. Cropping must preserve every rasterized pixel at 1.5.
                let mut reference = scene.clone();
                let bead::render::Primitive::Image { rect, path, .. } =
                    &mut reference.primitives[index]
                else {
                    panic!("image")
                };
                *rect = full_rect;
                let prefix = std::str::from_utf8(&full_svg)
                    .unwrap()
                    .split_once('>')
                    .unwrap()
                    .0;
                let dynamic = std::str::from_utf8(&scene.asset_data[path])
                    .unwrap()
                    .split_once('>')
                    .unwrap()
                    .1;
                reference.asset_data.insert(
                    path.clone(),
                    Arc::from(format!("{prefix}>{dynamic}").into_bytes()),
                );
                let mut full_cache = bead::render::tree::TreeCache::default();
                let mut cropped_cache = bead::render::tree::TreeCache::default();
                full_cache.prepare(&reference, 1.5).unwrap();
                cropped_cache.prepare(&scene, 1.5).unwrap();
                let full = &full_cache.frame.as_ref().unwrap().items[index]
                    .raster
                    .as_ref()
                    .unwrap()
                    .pixels;
                let cropped = &cropped_cache.frame.as_ref().unwrap().items[index]
                    .raster
                    .as_ref()
                    .unwrap()
                    .pixels;
                assert!(cropped.height() < full.height());
                assert_eq!(cropped.width(), full.width());
                assert_eq!(cropped.data(), &full.data()[..cropped.data().len()]);
                assert!(
                    full.data()[cropped.data().len()..]
                        .iter()
                        .all(|byte| *byte == 0)
                );
            }
            assert_eq!(body(&scene), full_body);
            assert_eq!(origin(&scene, None, (1920, 1200)), before);
            assert_eq!(
                scene.asset_data.len(),
                2,
                "only open art and the current reveal asset survive"
            );
        }
        scene.reveal(1.);
        assert_eq!(
            scene.asset_data.len(),
            1,
            "settled art drops the transient payload"
        );
        cache.prepare(&scene, 1.).unwrap();
        assert_eq!(
            cache.frame.as_ref().unwrap().key,
            scene.prepared[&1_f32.to_bits()].key
        );
    }
    #[test]
    fn frame_tiles_stay_small_and_panel_redecoration_keeps_one_asset() {
        let colors = colors();
        let chrome = Chrome::new(Attachment::Popout, &colors).unwrap();
        let mut scene = crate::sound::fallback(colors.get("surface").unwrap());
        chrome.decorate(&mut scene).unwrap();
        let first = body(&scene);
        for _ in 0..20 {
            chrome.decorate(&mut scene).unwrap();
        }
        assert_eq!((body(&scene).w, body(&scene).h), (first.w, first.h));
        assert_eq!(
            scene.asset_data.len(),
            1,
            "No historical contour payloads retained"
        );
        let mut cache = bead::render::tree::TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let builds = cache.builds;
        let position = Frame::default().positioner(Attachment::Popout);
        let origin = position(&scene, None, (1920, 1200));
        let factory = Chrome::new(Attachment::Popout, &colors)
            .unwrap()
            .decorations();
        let static_tiles = factory(
            (1920, 1200),
            true,
            Some(bead::wayland::DecorationPanel {
                scene: &scene,
                origin,
            }),
        )
        .unwrap();
        for offset in [1., 4., 8., 16.] {
            let mut moving = scene.clone();
            moving.offset.1 += offset;
            moving.opacity = 0.75;
            cache.prepare(&moving, 1.5).unwrap();
            assert_eq!(
                cache.builds, builds,
                "Transform/opacity must reuse the baked contour"
            );
            assert_eq!(
                position(&moving, None, (1920, 1200)),
                origin,
                "Animation must not move the layer's attachment"
            );
            let tiles = factory(
                (1920, 1200),
                true,
                Some(bead::wayland::DecorationPanel {
                    scene: &moving,
                    origin,
                }),
            )
            .unwrap();
            assert!(
                tiles.iter().zip(&static_tiles).all(|(a, b)| a.svg == b.svg),
                "Transform/opacity must not invalidate static frame geometry"
            );
        }
        let factory = Chrome::new(Attachment::Attached(Edge::Top), &colors)
            .unwrap()
            .decorations();
        let tiles = factory((1920, 1200), false, None).unwrap();
        assert_eq!(tiles.len(), 8);
        let bytes: usize = tiles
            .iter()
            .map(|t| (t.rect.w * 1.5).ceil() as usize * (t.rect.h * 1.5).ceil() as usize * 4)
            .sum();
        assert_eq!(bytes, 542592);
        assert!(
            bytes < 600 * 1024,
            "Never allocate an output-sized frame raster"
        );
        assert!(tiles.iter().all(|t| t.svg.len() < 16384));
        assert!(
            tiles
                .iter()
                .all(|t| String::from_utf8_lossy(&t.svg).contains("opacity=\"0.16\""))
        );
        let active = factory((1920, 1200), true, None).unwrap();
        assert!(
            active
                .iter()
                .all(|t| String::from_utf8_lossy(&t.svg).contains("opacity=\"0.25\""))
        );
    }
    fn settings() -> FrameSettings {
        FrameSettings {
            width: 10.,
            rounding: 25.,
            left: true,
            right: true,
            bottom: true,
            top: true,
            bar: true,
        }
    }
    fn rects(frame: Frame, size: (u32, u32)) -> Vec<(f32, f32, f32, f32)> {
        let chrome = Chrome::new(Attachment::Attached(Edge::Top), &colors()).unwrap();
        chrome
            .tiles(frame, size, false, None)
            .unwrap()
            .iter()
            .map(|t| (t.rect.x, t.rect.y, t.rect.w, t.rect.h))
            .collect()
    }

    #[test]
    fn settings_build_the_nacre_frame() {
        assert_eq!(Frame::from_settings(&settings()), Frame::DEFAULT);
        let wide = FrameSettings {
            width: 14.,
            rounding: 30.,
            left: false,
            bottom: false,
            ..settings()
        };
        let frame = Frame::from_settings(&wide);
        assert_eq!(
            (
                frame.left,
                frame.right,
                frame.bottom,
                frame.top,
                frame.rounding
            ),
            (0., 14., 0., 54., 30.)
        );
        // No bar: the header is only the top edge; no top edge either: nothing.
        let bar_off = FrameSettings {
            bar: false,
            ..settings()
        };
        assert_eq!(Frame::from_settings(&bar_off).top, 10.);
        let bare = FrameSettings {
            bar: false,
            top: false,
            ..settings()
        };
        assert_eq!(Frame::from_settings(&bare).top, 0.);
        // Nonsense widths never go negative.
        let negative = FrameSettings {
            width: -5.,
            rounding: -1.,
            ..settings()
        };
        let frame = Frame::from_settings(&negative);
        assert_eq!((frame.left, frame.top, frame.rounding), (0., 40., 0.));
    }

    #[test]
    fn tiles_follow_the_frame_the_way_the_default_crops_do() {
        // Default crops, exactly as before the frame became configurable.
        assert_eq!(
            rects(Frame::DEFAULT, (1920, 1200)),
            [
                (0., 52., 12., 1136.),
                (1908., 52., 12., 1136.),
                (0., 1188., 1920., 12.),
                (0., 48., 1920., 4.),
                (12., 52., 24., 24.),
                (1884., 52., 24., 24.),
                (12., 1164., 24., 24.),
                (1884., 1164., 24., 24.),
            ]
        );
        // A wider frame moves every crop with it.
        let wide = Frame {
            left: 14.,
            right: 14.,
            bottom: 14.,
            top: 54.,
            rounding: 25.,
        };
        let tiles = rects(wide, (1920, 1200));
        assert_eq!(tiles[0], (0., 56., 16., 1200. - 56. - 16.));
        assert_eq!(tiles[3], (0., 52., 1920., 4.));
        assert_eq!(tiles[4], (16., 56., 24., 24.));
        // A side that is off has neither its strip nor the corners that join it.
        let no_left = Frame {
            left: 0.,
            ..Frame::DEFAULT
        };
        let tiles = rects(no_left, (1920, 1200));
        assert_eq!(tiles.len(), 5, "{tiles:?}");
        assert!(tiles.iter().all(|t| t.0 != 0. || t.2 == 1920.));
        // Only the header: one band, no corners.
        let top_only = Frame {
            left: 0.,
            right: 0.,
            bottom: 0.,
            ..Frame::DEFAULT
        };
        assert_eq!(rects(top_only, (1920, 1200)), [(0., 48., 1920., 4.)]);
        // No frame at all: nothing to draw.
        let none = Frame {
            left: 0.,
            right: 0.,
            bottom: 0.,
            top: 0.,
            rounding: 25.,
        };
        assert!(rects(none, (1920, 1200)).is_empty());
    }

    #[test]
    fn rounding_is_bounded_by_the_opening_and_zero_makes_square_joins() {
        let frame = Frame::DEFAULT;
        assert_eq!(frame.radius((1920., 1200.)), 25.);
        // A 50 px opening can only round by 25; a 30 px one by 15.
        assert_eq!(frame.radius((70., 1200.)), 25.);
        assert_eq!(frame.radius((50., 1200.)), 15.);
        assert_eq!(frame.radius((1920., 90.)), 15.);
        assert_eq!(frame.radius((10., 10.)), 0.);
        let square = Frame {
            rounding: 0.,
            ..Frame::DEFAULT
        };
        assert_eq!(rects(square, (1920, 1200)).len(), 4, "no corner tiles at 0");
    }

    #[test]
    fn frame_tiles_stay_inside_the_memory_budget_for_any_width() {
        let chrome = Chrome::new(Attachment::Attached(Edge::Top), &colors()).unwrap();
        for width in [0., 1., 10., 24., 30.] {
            let frame = Frame::from_settings(&FrameSettings {
                width,
                ..settings()
            });
            let tiles = chrome.tiles(frame, (2560, 1440), true, None).unwrap();
            let bytes: usize = tiles
                .iter()
                .map(|t| (t.rect.w * 1.5).ceil() as usize * (t.rect.h * 1.5).ceil() as usize * 4)
                .sum();
            assert!(bytes < 2 * 1024 * 1024, "{width}: {bytes}");
            assert!(tiles.len() <= 8);
        }
    }

    #[test]
    fn nacre_attachment_and_corner_threshold_match_the_published_rules() {
        assert_eq!(
            attached(Edge::Top, (1900., 1140.), (1100., 590.), 0.),
            (400., 0.)
        );
        assert_eq!(
            attached(Edge::Right, (1900., 1140.), (400., 286.), 120.),
            (1380., 427.)
        );
        assert_eq!(
            attached(Edge::Bottom, (1900., 1140.), (1100., 590.), 0.),
            (400., 550.)
        );
        assert_eq!(
            attached(Edge::Left, (200., 100.), (400., 286.), 0.),
            (0., 0.)
        );
        assert_eq!(popout(10., 1900., 960., 310., 310., 25.), (795., false));
        assert_eq!(popout(10., 1900., 1729., 310., 310., 25.), (1564., false));
        // Already docked at the rounding threshold, before ordinary clamping.
        assert_eq!(popout(10., 1900., 1730., 310., 310., 25.), (1590., true));
        assert_eq!(popout(10., 1900., 0., 310., 310., 25.), (25., false));
    }
    #[test]
    fn visible_body_is_flush_even_with_a_thirty_two_pixel_surface_inset() {
        let scene = crate::sound::fallback([20, 20, 20, 255]);
        let position = Frame::default().positioner(Attachment::Attached(Edge::Right));
        assert_eq!(position(&scene, None, (1920, 1200)), (1478, 445));
        let mut scene = scene;
        scene.primitives = vec![bead::render::Primitive::Rounded {
            rect: Rect::new(0., 0., 310., 222.),
            radius: 16.,
            fill: [0; 4],
            border: None,
        }];
        let position = Frame::default().positioner(Attachment::Popout);
        assert_eq!(position(&scene, Some(1890.), (1920, 1200)), (1568, 18));
        // Body is y=18+32=50, touching the header rather than floating at102.
    }

    #[test]
    fn control_center_body_joins_top_frame_at_right_inset() {
        let frame = Frame::default();
        assert_eq!(
            frame.panel(
                Attachment::ControlCenter,
                (1920., 1200.),
                (400., 286.),
                None,
            ),
            Rect::new(1510., 50., 400., 286.)
        );
        let scene = crate::sound::fallback([20, 20, 20, 255]);
        let position = frame.positioner(Attachment::ControlCenter);
        assert_eq!(position(&scene, None, (1920, 1200)), (1478, 18));
    }
}

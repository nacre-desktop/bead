// SPDX-License-Identifier: GPL-3.0-only
//! B0048: the Nacre look of pearls. The mechanics (detach, dock, drag, remember) are
//! `bead-wayland` and `bead_core::pearl`; this file owns the colours and shapes:
//! the pearl body and its border, the dock film and landing field, the view that
//! draws them during a drag, and which panels may detach.
use bead::{
    core::Edge,
    render::{Palette, Primitive, Rect, Scene},
    wayland::{
        Guide, NativePearlMotion, NativePearlOptions, NativePearlPlacement, PanelSpec,
        PearlOptions, SceneSource, UiEvent,
    },
};
use std::{
    error::Error,
    sync::{Arc, atomic::AtomicU8, atomic::Ordering},
};

/// Name of the guide view in the panel catalogue.
pub const GUIDE_VIEW: &str = "pearl-guide";
/// Corner radius of a pearl body (the border strokes inside it).
pub const RADIUS: f32 = 18.;
const FILM_STRETCH: f64 = 170.;

/// View-owned state only: never providers, results, credentials or image data.
pub fn capture_state(stores: &bead::core::Stores) -> serde_json::Value {
    let mut values = crate::launcher::remember(stores);
    for key in ["dashboard.page", "cc.page", "chat.mode", "chat.page"] {
        if let Some(bead::core::Value::Text(value)) = stores.get(key)
            && value.len() <= 256
        {
            values.insert(key.into(), bead::core::Value::Text(value.clone()));
        }
    }
    serde_json::to_value(values).unwrap_or_default()
}
pub fn restore_state(
    saved: serde_json::Value,
    stores: &bead::core::Stores,
) -> std::collections::BTreeMap<String, bead::core::Value> {
    let Ok(values) =
        serde_json::from_value::<std::collections::BTreeMap<String, bead::core::Value>>(saved)
    else {
        return Default::default();
    };
    // Project through the same whitelist and bounds as capture; saved files are
    // external input and must not inject provider or secret state.
    let mut candidate = stores.clone();
    if candidate.commit(values.clone()).is_err() {
        return Default::default();
    }
    let allowed = capture_state(&candidate);
    serde_json::from_value::<std::collections::BTreeMap<String, bead::core::Value>>(allowed)
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| values.contains_key(key))
        .collect()
}

pub fn options(
    store: Option<std::path::PathBuf>,
    client: Option<crate::nacre_source::Client>,
) -> PearlOptions {
    PearlOptions {
        tuning: Default::default(),
        frame: Arc::new(move |_, _| {
            let frame = crate::placement::Frame::current();
            bead::core::pearl::Insets {
                top: frame.top as f64,
                right: frame.right as f64,
                bottom: frame.bottom as f64,
                left: frame.left as f64,
            }
        }),
        guide_view: Some(GUIDE_VIEW.into()),
        store,
        max: 8,
        test_any_press: std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() == Ok("1"),
        native: Some(native_options(client)),
    }
}

/// Reuse Nacre's owned-window placement tool. Only the latest pending position
/// for each pearl matters; the UI never waits for compositor IPC or a helper.
fn native_options(client: Option<crate::nacre_source::Client>) -> NativePearlOptions {
    #[derive(Default)]
    struct Queue {
        pending: std::collections::BTreeMap<u32, NativePearlPlacement>,
        running: bool,
    }
    let queue = Arc::new(std::sync::Mutex::new(Queue::default()));
    NativePearlOptions {
        watch: client.map(|client| {
            Arc::new(move |send: Box<dyn Fn(NativePearlMotion) + Send + Sync>| {
                use bead::core::source::Source;
                // Notices already requests this topic permanently. Reuse that
                // subscription; a permanent topic deliberately cannot be leased.
                let topic: Box<dyn Source> = match client.topic("compositor") {
                    Ok(topic) => Box::new(topic),
                    Err(_) => Box::new(client.lease_topic("compositor").ok()?),
                };
                let previous = std::sync::Mutex::new(std::collections::BTreeMap::new());
                let subscription = topic
                    .subscribe(Arc::new(move |snapshot| {
                        let data = if snapshot.connected {
                            &*snapshot.data
                        } else {
                            &serde_json::Value::Null
                        };
                        let mut previous = previous.lock().unwrap_or_else(|p| p.into_inner());
                        for motion in native_motions(data, &mut previous, std::process::id()) {
                            send(motion);
                        }
                    }))
                    .ok()?;
                Some(Box::new((topic, subscription)) as Box<dyn Send>)
            }) as bead::wayland::NativePearlWatch
        }),
        title: Arc::new(|id, view| {
            let kind = match view {
                "launcher" => "Launcher",
                "chat" => "AI",
                "control" => "Controls",
                _ => "Dashboard",
            };
            format!("Nacre Pearl {kind} {id}")
        }),
        place: Arc::new(move |placement| {
            let mut state = queue.lock().unwrap_or_else(|p| p.into_inner());
            let mut placement = placement;
            if let Some(previous) = state.pending.get(&placement.id) {
                placement.first |= previous.first;
            }
            state.pending.insert(placement.id, placement);
            if state.running {
                return;
            }
            state.running = true;
            drop(state);
            let queue = queue.clone();
            std::thread::spawn(move || {
                loop {
                    let next = {
                        let mut state = queue.lock().unwrap_or_else(|p| p.into_inner());
                        let next = state.pending.pop_first();
                        if next.is_none() {
                            state.running = false;
                        }
                        next
                    };
                    let Some((_, event)) = next else {
                        break;
                    };
                    let rect = event.rect;
                    let result = std::process::Command::new(crate::prompt::helper_binary())
                        .args([
                            "pearl-window",
                            "place",
                            &event.title,
                            &std::process::id().to_string(),
                            &rect.x.round().to_string(),
                            &rect.y.round().to_string(),
                            &if event.first {
                                rect.w.round().to_string()
                            } else {
                                "0".into()
                            },
                            &if event.first {
                                rect.h.round().to_string()
                            } else {
                                "0".into()
                            },
                        ])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .output();
                    match result {
                        Ok(output) if output.status.success() => {}
                        Ok(_) => eprintln!("PEARL_PLACE_ERROR id={} helper failed", event.id),
                        Err(error) => eprintln!("PEARL_PLACE_ERROR id={} {error}", event.id),
                    }
                }
            });
        }),
    }
}

/// Niri exposes the completed body, not a live drag cursor. Follow QML's
/// moving -> geometry-known transition and show only landing fields in between.
fn native_motions(
    data: &serde_json::Value,
    previous: &mut std::collections::BTreeMap<u32, bool>,
    pid: u32,
) -> Vec<NativePearlMotion> {
    let mut next = std::collections::BTreeMap::new();
    let mut motions = Vec::new();
    if data["available"] != false
        && let Some(windows) = data.get("windows").and_then(serde_json::Value::as_object)
    {
        for window in windows.values() {
            if window["pid"].as_u64() != Some(pid as u64) {
                continue;
            }
            let Some(title) = window["title"].as_str() else {
                continue;
            };
            let Some((kind, id)) = title
                .strip_prefix("Nacre Pearl ")
                .and_then(|s| s.rsplit_once(' '))
            else {
                continue;
            };
            if !["Launcher", "Dashboard", "AI", "Controls", "Media"].contains(&kind) {
                continue;
            }
            let Ok(id) = id.parse::<u32>() else {
                continue;
            };
            let moving = window["moving"] == true;
            let was_moving = previous.get(&id).copied().unwrap_or(false);
            if moving && !was_moving {
                motions.push(NativePearlMotion {
                    id,
                    moving: true,
                    rect: None,
                    output: None,
                });
            } else if was_moving && !moving && window["geometryKnown"] == true {
                let g = &window["geometry"];
                let rect = (|| {
                    Some(bead::core::pearl::Rect::new(
                        g["x"].as_f64()?,
                        g["y"].as_f64()?,
                        g["width"].as_f64()?,
                        g["height"].as_f64()?,
                    ))
                })();
                motions.push(NativePearlMotion {
                    id,
                    moving: false,
                    rect: rect.filter(|_| window["floating"] == true),
                    output: window["outputId"].as_str().map(String::from),
                });
            }
            next.insert(
                id,
                moving || (was_moving && window["geometryKnown"] != true),
            );
        }
    }
    for id in previous.keys().filter(|id| !next.contains_key(id)) {
        motions.push(NativePearlMotion {
            id: *id,
            moving: false,
            rect: None,
            output: None,
        });
    }
    *previous = next;
    motions
}

/// The palette entries the art is drawn from (NacreTokens: body, accent,
/// orientSecondary, orientHighlight, outline).
#[derive(Clone, Copy, Debug)]
pub struct Ink {
    pub body: [u8; 4],
    pub accent: [u8; 4],
    pub secondary: [u8; 4],
    pub highlight: [u8; 4],
    pub outline: [u8; 4],
}
impl Ink {
    pub fn new(colors: &Palette) -> Result<Self, Box<dyn Error>> {
        let accent = colors.get("primary")?;
        Ok(Self {
            body: colors.get("surface")?,
            accent,
            secondary: colors.get("secondary").unwrap_or(accent),
            highlight: std::array::from_fn(|i| {
                if i == 3 {
                    255
                } else {
                    (accent[i] as f32 * 0.78 + [255., 249., 241.][i] * 0.22).round() as u8
                }
            }),
            outline: colors.get("outline")?,
        })
    }
}
fn rgb(c: [u8; 4]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}
/// A stop with its own alpha, as `stop-color` plus `stop-opacity`.
fn stop(offset: f32, c: [u8; 4], alpha: f32) -> String {
    format!(
        r#"<stop offset="{offset}" stop-color="{}" stop-opacity="{}"/>"#,
        rgb(c),
        alpha * f32::from(c[3]) / 255.
    )
}
/// Qt.tint(outline, Qt.alpha(accent, strength)): the accent over the outline.
fn blend(base: [u8; 4], over: [u8; 4], strength: f32) -> [u8; 4] {
    std::array::from_fn(|i| {
        if i == 3 {
            255
        } else {
            (base[i] as f32 * (1. - strength) + over[i] as f32 * strength).round() as u8
        }
    })
}

/// Thickness of the pearl border.
const BORDER: f32 = 2.;

/// The pearl border: a two-pixel ring that is the outline at rest and the accent
/// gradient when the pearl is focused or hovered. It is cut into four corners and
/// four edges (each overlapping its neighbour by a pixel) so that no raster is as
/// large as the body; the fill is the panel's own root background. Rects are
/// relative to the body. A body too small to cut gets the whole ring.
pub fn border_svgs(w: f32, h: f32, ink: &Ink, engaged: bool) -> Vec<(Rect, String)> {
    let strength = if engaged { 1. } else { 0. };
    let a = blend(ink.outline, ink.accent, strength);
    let b = blend(ink.outline, ink.secondary, strength);
    let (inner_w, inner_h) = ((w - 2.).max(1.), (h - 2.).max(1.));
    let ring = |crop: Rect| {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><defs><linearGradient id="b" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="{w}" y2="{h}">{}{}</linearGradient></defs><rect x="1" y="1" width="{inner_w}" height="{inner_h}" rx="{}" fill="none" stroke="url(#b)" stroke-width="{BORDER}"/></svg>"#,
            crop.w,
            crop.h,
            crop.x,
            crop.y,
            crop.w,
            crop.h,
            stop(0., a, 1.),
            stop(1., b, 1.),
            RADIUS - 1.
        )
    };
    let r = RADIUS;
    let pieces = if w < 2. * r + 2. || h < 2. * r + 2. {
        vec![Rect::new(0., 0., w, h)]
    } else {
        vec![
            Rect::new(0., 0., r, r),
            Rect::new(w - r, 0., r, r),
            Rect::new(0., h - r, r, r),
            Rect::new(w - r, h - r, r, r),
            Rect::new(r - 1., 0., w - 2. * r + 2., BORDER),
            Rect::new(r - 1., h - BORDER, w - 2. * r + 2., BORDER),
            Rect::new(0., r - 1., BORDER, h - 2. * r + 2.),
            Rect::new(w - BORDER, r - 1., BORDER, h - 2. * r + 2.),
        ]
    };
    pieces.into_iter().map(|rect| (rect, ring(rect))).collect()
}

/// A point in (along, across) order: x then y for a horizontal edge's film.
fn at(vertical: bool, along: f64, across: f64) -> String {
    if vertical {
        format!("{along:.2} {across:.2}")
    } else {
        format!("{across:.2} {along:.2}")
    }
}
/// The membrane that stretches from where a panel was grabbed to where it is now,
/// in output coordinates. Returns its bounding box and the SVG drawn in it.
pub fn film_svg(
    edge: Edge,
    origin: (f64, f64),
    tip: (f64, f64),
    armed: bool,
    ink: &Ink,
) -> Option<(Rect, String)> {
    let vertical = matches!(edge, Edge::Top | Edge::Bottom);
    let (start_along, start_across) = if vertical {
        (origin.0, origin.1)
    } else {
        (origin.1, origin.0)
    };
    let (end_along, end_across) = if vertical {
        (tip.0, tip.1)
    } else {
        (tip.1, tip.0)
    };
    let distance = (end_across - start_across).abs();
    let stretch = (distance / FILM_STRETCH).min(1.);
    if distance <= 1. || stretch >= 0.995 {
        return None;
    }
    let span = if vertical { 68. } else { 52. };
    let neck = (span * (1. - stretch)).max(3.);
    let delta = end_across - start_across;
    let p = |along, across| at(vertical, along, across);
    let path = format!(
        "M{} C{} {} {} L{} C{} {} {} Z",
        p(start_along - span, start_across),
        p(start_along - span, start_across + delta * 0.7),
        p(end_along - neck, end_across - delta * 0.3),
        p(end_along - neck, end_across),
        p(end_along + neck, end_across),
        p(end_along + neck, end_across - delta * 0.3),
        p(start_along + span, start_across + delta * 0.7),
        p(start_along + span, start_across),
    );
    let (lo_along, hi_along) = (
        (start_along - span).min(end_along - neck),
        (start_along + span).max(end_along + neck),
    );
    let (lo_across, hi_across) = (start_across.min(end_across), start_across.max(end_across));
    let pad = 2.;
    let bounds = if vertical {
        Rect::new(
            (lo_along - pad) as f32,
            (lo_across - pad) as f32,
            (hi_along - lo_along + 2. * pad) as f32,
            (hi_across - lo_across + 2. * pad) as f32,
        )
    } else {
        Rect::new(
            (lo_across - pad) as f32,
            (lo_along - pad) as f32,
            (hi_across - lo_across + 2. * pad) as f32,
            (hi_along - lo_along + 2. * pad) as f32,
        )
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><defs><linearGradient id="f" gradientUnits="userSpaceOnUse" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}">{}{}{}</linearGradient></defs><path d="{path}" fill="url(#f)" stroke="{}" stroke-opacity=".35" stroke-width="1" opacity="{}"/></svg>"#,
        bounds.w,
        bounds.h,
        bounds.x,
        bounds.y,
        bounds.w,
        bounds.h,
        origin.0,
        origin.1,
        tip.0,
        tip.1,
        stop(0., ink.accent, 0.6),
        stop(0.55, ink.secondary, 0.65),
        stop(1., ink.highlight, 0.55),
        rgb(ink.highlight),
        if armed { 0.88 } else { 0.6 },
    );
    Some((bounds, svg))
}

/// The quiet landing field: three stacked, rounded bars fading out at both ends,
/// centred on `at` and turned for a left or right edge. Size and SVG.
pub fn target_svg(edge: Edge, ink: &Ink) -> (Rect, String) {
    let (w, h) = (220., 18.);
    let bars = [(220., 18., 0.08), (200., 8., 0.18), (180., 2., 0.85)];
    let stops = format!(
        "{}{}{}{}{}",
        stop(0., ink.accent, 0.),
        stop(0.2, ink.accent, 1.),
        stop(0.6, ink.secondary, 1.),
        stop(0.85, ink.highlight, 1.),
        stop(1., ink.highlight, 0.)
    );
    let rects: String = bars
        .iter()
        .map(|(bw, bh, alpha)| {
            format!(
                r#"<rect x="{}" y="{}" width="{bw}" height="{bh}" rx="{}" fill="url(#t)" opacity="{alpha}"/>"#,
                (w - bw) / 2.,
                (h - bh) / 2.,
                bh / 2.
            )
        })
        .collect();
    let turn = matches!(edge, Edge::Left | Edge::Right);
    let (sw, sh) = if turn { (h, w) } else { (w, h) };
    let transform = if turn {
        format!(r#" transform="translate({h} 0) rotate(90)""#)
    } else {
        String::new()
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{sw}" height="{sh}" viewBox="0 0 {sw} {sh}"><defs><linearGradient id="t" x1="0" y1="0" x2="1" y2="0">{stops}</linearGradient></defs><g{transform}>{rects}</g></svg>"#
    );
    (Rect::new(0., 0., sw, sh), svg)
}

/// Strip of the output the guide covers: along the edge, deep enough for the
/// longest film. Output-local.
pub fn strip(g: &Guide) -> Rect {
    let [top, right, bottom, left] = g.frame.map(|v| v as f32);
    let (w, h) = (g.output.0 as f32, g.output.1 as f32);
    let reach = FILM_STRETCH as f32 + 48.;
    match g.edge {
        Edge::Top => Rect::new(0., 0., w, top + reach),
        Edge::Bottom => Rect::new(0., h - (bottom + reach), w, bottom + reach),
        Edge::Left => Rect::new(0., 0., left + reach, h),
        Edge::Right => Rect::new(w - (right + reach), 0., right + reach, h),
    }
}

static EDGE: AtomicU8 = AtomicU8::new(0);
fn edge_code(edge: Edge) -> u8 {
    match edge {
        Edge::Top => 0,
        Edge::Bottom => 1,
        Edge::Left => 2,
        Edge::Right => 3,
    }
}
/// Surface origin of the guide's strip, from its size: the strip hugs the edge.
fn guide_origin(scene: &Scene, output: (i32, i32)) -> (i32, i32) {
    let (w, h) = (scene.bounds.w.ceil() as i32, scene.bounds.h.ceil() as i32);
    match EDGE.load(Ordering::Relaxed) {
        1 => (0, output.1 - h),
        3 => (output.0 - w, 0),
        _ => (0, 0),
    }
}

fn blank(w: f32, h: f32) -> Scene {
    Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: w,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., w, h),
        primitives: vec![],
    }
}
fn asset(scene: &mut Scene, name: &str, rect: Rect, svg: String) {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    svg.hash(&mut hash);
    let path = std::path::PathBuf::from(format!("@pearl-{name}-{:x}.svg", hash.finish()));
    scene
        .asset_data
        .insert(path.clone(), Arc::from(svg.into_bytes()));
    scene.primitives.push(Primitive::Image {
        blur: 0.,
        fit: bead::render::ImageFit::Fill,
        clip_polygon: None,
        rect,
        path,
    });
}
/// The guide for one output as a scene: its landing field and, while a panel is
/// within reach of the edge, the film.
pub fn guide_scene(g: &Guide, ink: &Ink) -> Scene {
    let strip = strip(g);
    let mut scene = blank(strip.w, strip.h);
    if let Some((x, y)) = g.landing {
        let (bounds, svg) = target_svg(g.edge, ink);
        let rect = Rect::new(
            x as f32 - bounds.w / 2. - strip.x,
            y as f32 - bounds.h / 2. - strip.y,
            bounds.w,
            bounds.h,
        );
        asset(&mut scene, "target", rect, svg);
    }
    if let Some(film) = &g.film
        && let Some((bounds, svg)) = film_svg(g.edge, film.origin, film.tip, film.armed, ink)
    {
        let rect = Rect::new(bounds.x - strip.x, bounds.y - strip.y, bounds.w, bounds.h);
        asset(&mut scene, "film", rect, svg);
    }
    scene
}

/// A scene source that only turns [`UiEvent::PearlGuide`] into a scene.
struct GuideSource {
    ink: Ink,
    scene: Scene,
    film: bool,
}
impl SceneSource for GuideSource {
    fn watch(&mut self, _: Box<dyn Fn(Result<(), String>) + Send>) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        match event {
            UiEvent::PearlGuide(guide) => {
                EDGE.store(edge_code(guide.edge), Ordering::Relaxed);
                let mut guide = (*guide).clone();
                if !self.film {
                    guide.film = None;
                }
                self.scene = guide_scene(&guide, &self.ink);
                self.scene.resource_revision += 1;
                Ok(Some(self.scene.clone()))
            }
            _ => Ok(None),
        }
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn state_summary(&self) -> String {
        "guide".into()
    }
    fn commit(&mut self) -> bool {
        true
    }
    fn error_scene(&mut self, _: String) -> Scene {
        blank(1., 1.)
    }
}

/// The guide view for the panel catalogue: passive (no input, no focus) and placed
/// by its own strip.
pub fn guide_spec(colors: &Palette) -> Result<PanelSpec, Box<dyn Error>> {
    let ink = Ink::new(colors)?;
    // QML suppresses magneticCue on niri: its compositor move contract has no
    // live cursor. The stationary landing guide remains available.
    let film = match std::env::var("NACRE_COMPOSITOR").as_deref() {
        Ok("niri") => false,
        Ok("hyprland") => true,
        _ => std::env::var_os("NIRI_SOCKET").is_none(),
    };
    Ok(PanelSpec {
        position: Some(Arc::new(|scene, _, output| guide_origin(scene, output))),
        engages_decorations: false,
        passive: true,
        keyboard_focus: false,
        initial_open: false,
        fallback: blank(1., 1.),
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |_| {
            Ok(Box::new(GuideSource {
                ink,
                scene: blank(1., 1.),
                film,
            }) as Box<dyn SceneSource>)
        }),
    })
}

/// Which views may detach, from the `desktop` settings: the master switch, the
/// drag gesture, and a list of view names (a trailing `*` matches a prefix).
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub enabled: bool,
    pub drag: bool,
    pub views: Vec<String>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: true,
            drag: true,
            views: ["launcher", "chat", "dashboard-*"]
                .map(String::from)
                .to_vec(),
        }
    }
}
impl Policy {
    pub fn allows(&self, view: &str) -> bool {
        self.views.iter().any(|v| match v.strip_suffix('*') {
            Some(prefix) => view.starts_with(prefix),
            None => v == view,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_moves_require_owned_identity_and_completed_geometry() {
        let mut previous = Default::default();
        let snapshot = |pid, moving, known| {
            serde_json::json!({"windows":{"one":{
                "pid":pid,"title":"Nacre Pearl Launcher 7","moving":moving,"geometryKnown":known,
                "floating":true,"outputId":"fixture","geometry":{"x":400,"y":50,"width":600,"height":400}
            }}})
        };
        assert!(native_motions(&snapshot(22, false, true), &mut previous, 22).is_empty());
        let begin = native_motions(&snapshot(22, true, false), &mut previous, 22);
        assert_eq!(begin.len(), 1);
        assert!(begin[0].moving && begin[0].id == 7 && begin[0].rect.is_none());
        assert!(native_motions(&snapshot(22, false, false), &mut previous, 22).is_empty());
        let end = native_motions(&snapshot(22, false, true), &mut previous, 22);
        assert_eq!(end.len(), 1);
        assert!(!end[0].moving);
        assert_eq!(
            end[0].rect,
            Some(bead::core::pearl::Rect::new(400., 50., 600., 400.))
        );
        assert!(
            native_motions(&snapshot(23, true, true), &mut previous, 22)
                .iter()
                .all(|m| !m.moving)
        );
        native_motions(&snapshot(22, true, false), &mut previous, 22);
        let mut unavailable = snapshot(22, false, true);
        unavailable["available"] = false.into();
        let cancel = native_motions(&unavailable, &mut previous, 22);
        assert_eq!(cancel.len(), 1);
        assert!(!cancel[0].moving && cancel[0].rect.is_none());
        assert!(previous.is_empty());
    }

    #[test]
    fn pearl_state_restores_bounded_view_fields_without_secrets_or_providers() {
        use bead::core::{Stores, Value};
        let stores = Stores::default()
            .reconcile(&std::collections::BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("hello".into())),
                ("cc.page".into(), Value::Text("network".into())),
                ("network.secret".into(), Value::Text("private".into())),
            ]))
            .unwrap();
        let saved = capture_state(&stores);
        let restored = restore_state(saved, &stores);
        assert_eq!(
            restored.get("launcher.search.text"),
            Some(&Value::Text("hello".into()))
        );
        assert_eq!(
            restored.get("cc.page"),
            Some(&Value::Text("network".into()))
        );
        assert!(!restored.contains_key("network.secret"));
        let invalid = serde_json::json!({"network.secret":"injected", "launcher.search.text":"x".repeat(65537)});
        assert!(restore_state(invalid, &stores).is_empty());
    }

    fn ink() -> Ink {
        Ink::new(
            &Palette::read(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"),
            )
            .unwrap(),
        )
        .unwrap()
    }
    fn guide(edge: Edge) -> Guide {
        Guide {
            edge,
            output: (1920, 1200),
            frame: [50., 10., 10., 10.],
            landing: Some((960., 50.)),
            film: None,
        }
    }

    #[test]
    fn the_border_is_the_outline_at_rest_and_the_accent_when_engaged() {
        let ink = ink();
        let rest = border_svgs(400., 300., &ink, false);
        let hot = border_svgs(400., 300., &ink, true);
        let text =
            |pieces: &[(Rect, String)]| pieces.iter().map(|(_, s)| s.as_str()).collect::<String>();
        assert!(text(&rest).contains(&rgb(ink.outline)) && !text(&rest).contains(&rgb(ink.accent)));
        assert!(text(&hot).contains(&rgb(ink.accent)) && text(&hot).contains(&rgb(ink.secondary)));
        assert!(text(&rest).contains(r#"rx="17""#) && text(&rest).contains(r#"width="398""#));
        // Four corners and four edges: together a few hundred pixels, not a body-sized raster.
        assert_eq!(rest.len(), 8);
        let area: f32 = rest.iter().map(|(r, _)| r.w * r.h).sum();
        assert!(
            area < 4. * 18. * 18. + 2. * 2. * (364. + 264. + 8.),
            "{area}"
        );
        assert!(rest.iter().all(|(_, svg)| svg.len() < 1200));
        // A body too small to cut gets the whole ring.
        assert_eq!(border_svgs(30., 30., &ink, false).len(), 1);
    }

    #[test]
    fn film_stretches_from_the_root_to_the_tip_and_vanishes_when_flush_or_far() {
        let ink = ink();
        assert!(film_svg(Edge::Top, (900., 50.), (900., 50.5), false, &ink).is_none());
        assert!(film_svg(Edge::Top, (900., 50.), (900., 221.), false, &ink).is_none());
        let (bounds, svg) = film_svg(Edge::Top, (900., 50.), (1000., 130.), true, &ink).unwrap();
        // Rooted 68 either side of the grab at the edge, reaching the tip's neck.
        assert!(bounds.x <= 900. - 68. && bounds.y <= 50. && bounds.y + bounds.h >= 130.);
        assert!(svg.contains(r#"opacity="0.88""#) && svg.contains("<path"));
        let (_, unarmed) = film_svg(Edge::Top, (900., 50.), (1000., 130.), false, &ink).unwrap();
        assert!(unarmed.contains(r#"opacity="0.6""#));
        // A left edge draws the same membrane turned: roots are 52 apart along y.
        let (side, _) = film_svg(Edge::Left, (10., 600.), (90., 640.), false, &ink).unwrap();
        assert!(side.y <= 600. - 52. && side.x <= 10. && side.x + side.w >= 90.);
    }

    #[test]
    fn the_landing_field_turns_for_side_edges() {
        let ink = ink();
        let (flat, svg) = target_svg(Edge::Top, &ink);
        assert_eq!((flat.w, flat.h), (220., 18.));
        assert!(!svg.contains("rotate"));
        let (tall, svg) = target_svg(Edge::Right, &ink);
        assert_eq!((tall.w, tall.h), (18., 220.));
        assert!(svg.contains("rotate(90)"));
    }

    #[test]
    fn the_guide_strip_hugs_its_edge_and_the_scene_is_small() {
        let g = guide(Edge::Top);
        assert_eq!(strip(&g), Rect::new(0., 0., 1920., 268.));
        assert_eq!(
            strip(&guide(Edge::Bottom)),
            Rect::new(0., 972., 1920., 228.)
        );
        assert_eq!(strip(&guide(Edge::Left)), Rect::new(0., 0., 228., 1200.));
        assert_eq!(
            strip(&guide(Edge::Right)),
            Rect::new(1692., 0., 228., 1200.)
        );
        let scene = guide_scene(&g, &ink());
        assert_eq!(scene.primitives.len(), 1, "landing only, no film yet");
        let mut moving = g.clone();
        moving.film = Some(bead::wayland::Film {
            origin: (900., 50.),
            tip: (960., 120.),
            armed: false,
        });
        let scene = guide_scene(&moving, &ink());
        assert_eq!(scene.primitives.len(), 2);
        assert!(scene.asset_data.values().all(|svg| svg.len() < 2048));
        // The positioner hugs the output's far edge for bottom and right strips.
        EDGE.store(edge_code(Edge::Bottom), Ordering::Relaxed);
        let bottom = guide_scene(&guide(Edge::Bottom), &ink());
        assert_eq!(guide_origin(&bottom, (1920, 1200)), (0, 972));
        EDGE.store(edge_code(Edge::Right), Ordering::Relaxed);
        let right = guide_scene(&guide(Edge::Right), &ink());
        assert_eq!(guide_origin(&right, (1920, 1200)), (1692, 0));
        EDGE.store(0, Ordering::Relaxed);
    }

    #[test]
    fn policy_matches_names_and_prefixes() {
        let p = Policy::default();
        assert!(p.allows("launcher") && p.allows("chat") && p.allows("dashboard-media"));
        assert!(!p.allows("sound") && !p.allows("peek-network") && !p.allows("calendar"));
        let none = Policy {
            views: vec![],
            ..Policy::default()
        };
        assert!(!none.allows("launcher"));
    }
}

#[cfg(test)]
#[path = "pearls_pixels_tests.rs"]
mod pixels;

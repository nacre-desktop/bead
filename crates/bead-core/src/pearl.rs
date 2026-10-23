// SPDX-License-Identifier: MIT OR Apache-2.0
//! B0048: pearls, panels detached from their edge into free-floating surfaces.
//!
//! Pure geometry, the drag state machine and the layout store. Nothing here knows a
//! view name, a frame size or an output: the application supplies the edge a panel
//! belongs to, the monitors with their frame insets, and the thresholds in
//! [`Tuning`]. Distances are logical pixels, independent of where a panel was grabbed.
use crate::Edge;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn centre(&self) -> Point {
        Point {
            x: self.x + self.w / 2.,
            y: self.y + self.h / 2.,
        }
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }
    fn finite(&self) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
    }
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}
/// What each output reserves along its edges (a bar, a frame); panels dock to the
/// inner side of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub rect: Rect,
    pub frame: Insets,
}
/// Distances and hysteresis. The defaults are the values the Nacre desktop was
/// designed around; an application may override any of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// A panel farther than this from its edge shows no guide at all.
    pub capture: f64,
    /// Dock when released within this distance of the edge...
    pub arm: f64,
    /// ...or within this once armed (hysteresis, so the guide does not flicker).
    pub hold: f64,
    /// Pointer travel before a press becomes a drag.
    pub threshold: f64,
    /// A panel must overlap the monitor along its edge by min(this, a third of itself).
    pub overlap: f64,
    /// Keep a placed panel this far inside its monitor.
    pub margin: f64,
    /// Where a panel is held when placed at a bare point: the grab sits this far
    /// below its top edge.
    pub grip: f64,
    /// Half-extent kept clear of the monitor's corners along the edge.
    pub corner: f64,
    /// How far the guide's origin may slide from the panel's centre along the edge.
    pub slide: f64,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            capture: 170.,
            arm: 72.,
            hold: 104.,
            threshold: 16.,
            overlap: 96.,
            margin: 12.,
            grip: 24.,
            corner: 24.,
            slide: 96.,
        }
    }
}

pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    value.min(high.max(low)).max(low)
}
pub fn horizontal(edge: Edge) -> bool {
    matches!(edge, Edge::Top | Edge::Bottom)
}
/// The displacement of a pointer motion measured away from the edge.
pub fn outward(edge: Edge, dx: f64, dy: f64) -> f64 {
    match edge {
        Edge::Top => dy,
        Edge::Bottom => -dy,
        Edge::Left => dx,
        Edge::Right => -dx,
    }
}
/// Where a pearl lands when opened at a bare point: centred on it, held by its
/// grip, and kept inside the monitor.
pub fn place(point: Point, size: (f64, f64), monitor: &Rect, t: &Tuning) -> Point {
    Point {
        x: clamp(
            point.x - size.0 / 2.,
            monitor.x + t.margin,
            monitor.x + monitor.w - size.0 - t.margin,
        ),
        y: clamp(
            point.y - t.grip,
            monitor.y + t.margin,
            monitor.y + monitor.h - size.1 - t.margin,
        ),
    }
}
fn across(edge: Edge, m: &Monitor) -> f64 {
    match edge {
        Edge::Top => m.rect.y + m.frame.top,
        Edge::Bottom => m.rect.y + m.rect.h - m.frame.bottom,
        Edge::Left => m.rect.x + m.frame.left,
        Edge::Right => m.rect.x + m.rect.w - m.frame.right,
    }
}

/// The pull towards an edge: where the film is rooted and where it reaches.
#[derive(Clone, Debug, PartialEq)]
pub struct Magnet {
    pub screen: String,
    pub edge: Edge,
    pub armed: bool,
    pub distance: f64,
    pub origin: Point,
    pub tip: Point,
}
/// One magnetic target for every kind of lift. `previous` carries the hysteresis;
/// `anchor` is the panel as it was grabbed, so the film keeps its root while the
/// panel moves.
pub fn magnet(
    edge: Edge,
    body: Rect,
    point: Point,
    monitors: &[Monitor],
    previous: Option<&Magnet>,
    anchor: Rect,
    t: &Tuning,
) -> Option<Magnet> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !body.finite()
        || body.w <= 0.
        || body.h <= 0.
    {
        return None;
    }
    let monitor = monitors.iter().find(|m| m.rect.contains(point))?;
    let horiz = horizontal(edge);
    let (low, high) = if horiz {
        (monitor.rect.x, monitor.rect.x + monitor.rect.w)
    } else {
        (monitor.rect.y, monitor.rect.y + monitor.rect.h)
    };
    let panel_low = if horiz { body.x } else { body.y };
    let span = if horiz { body.w } else { body.h };
    if high.min(panel_low + span) - low.max(panel_low) < t.overlap.min(span / 3.) {
        return None;
    }
    let edge_line = across(edge, monitor);
    let leading = match edge {
        Edge::Top => body.y,
        Edge::Bottom => body.y + body.h,
        Edge::Left => body.x,
        Edge::Right => body.x + body.w,
    };
    let distance = (leading - edge_line).abs();
    if distance > t.capture {
        return None;
    }
    let armed_before =
        previous.is_some_and(|p| p.armed && p.screen == monitor.name && p.edge == edge);
    let along = clamp(panel_low + span / 2., low + t.corner, high - t.corner);
    let anchor_along = if horiz {
        anchor.x + anchor.w / 2.
    } else {
        anchor.y + anchor.h / 2.
    };
    let origin_along = clamp(
        anchor_along,
        (low + t.corner).max(along - t.slide),
        (high - t.corner).min(along + t.slide),
    );
    let (origin, tip) = if horiz {
        (
            Point {
                x: origin_along,
                y: edge_line,
            },
            Point {
                x: along,
                y: leading,
            },
        )
    } else {
        (
            Point {
                x: edge_line,
                y: origin_along,
            },
            Point {
                x: leading,
                y: along,
            },
        )
    };
    Some(Magnet {
        screen: monitor.name.clone(),
        edge,
        armed: distance <= if armed_before { t.hold } else { t.arm },
        distance,
        origin,
        tip,
    })
}
/// The stationary landing field, shown even when the compositor hides a moving
/// window's coordinates: a target, not a guessed position.
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    pub screen: String,
    pub edge: Edge,
    pub origin: Point,
}
pub fn landing(edge: Edge, monitor: &Monitor) -> Landing {
    let f = &monitor.frame;
    let m = &monitor.rect;
    let along = if horizontal(edge) {
        m.x + m.w / 2.
    } else {
        m.y + f.top + (m.h - f.top - f.bottom) / 2.
    };
    let line = across(edge, monitor);
    Landing {
        screen: monitor.name.clone(),
        edge,
        origin: if horizontal(edge) {
            Point { x: along, y: line }
        } else {
            Point { x: line, y: along }
        },
    }
}

/// Where a gesture started: a docked panel being pulled out, or a pearl being moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Docked,
    Pearl,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Docked panel released near its own edge (or never pulled): it springs back.
    Return,
    /// Pulled free. `dock` names another output whose edge it was released at: the
    /// pearl is created and docked there at once.
    Detach { rect: Rect, dock: Option<String> },
    /// A pearl released near an edge docks there.
    Dock { screen: String },
    /// A pearl released elsewhere stays, at its new place.
    Moved { rect: Rect },
}
/// One live drag. The application feeds pointer positions in global logical
/// coordinates and draws `magnet` (the film) while it is `Some`.
#[derive(Clone, Debug)]
pub struct Drag {
    pub source: Source,
    pub edge: Edge,
    /// The output a docked panel was pulled from.
    pub screen: String,
    pub start: Point,
    pub point: Point,
    pub body: Rect,
    pub ready: bool,
    pub magnet: Option<Magnet>,
}
impl Drag {
    pub fn begin(
        source: Source,
        edge: Edge,
        screen: &str,
        body: Rect,
        point: Point,
        monitors: &[Monitor],
        t: &Tuning,
    ) -> Self {
        Self {
            source,
            edge,
            screen: screen.into(),
            start: point,
            point,
            body,
            ready: false,
            magnet: magnet(edge, body, point, monitors, None, body, t),
        }
    }
    pub fn delta(&self) -> (f64, f64) {
        (self.point.x - self.start.x, self.point.y - self.start.y)
    }
    /// The panel's rectangle under the pointer.
    pub fn current(&self) -> Rect {
        let (dx, dy) = self.delta();
        self.body.translated(dx, dy)
    }
    pub fn update(&mut self, point: Point, monitors: &[Monitor], t: &Tuning) {
        self.point = point;
        let (dx, dy) = self.delta();
        self.ready = dx.hypot(dy) >= t.threshold;
        self.magnet = magnet(
            self.edge,
            self.current(),
            point,
            monitors,
            self.magnet.as_ref(),
            self.body,
            t,
        );
    }
    pub fn release(&self) -> Outcome {
        let armed = self.magnet.as_ref().filter(|m| m.armed);
        match self.source {
            Source::Docked => {
                if !self.ready || armed.is_some_and(|m| m.screen == self.screen) {
                    return Outcome::Return;
                }
                let r = self.current();
                Outcome::Detach {
                    rect: Rect::new(r.x.round(), r.y.round(), r.w, r.h),
                    dock: armed.map(|m| m.screen.clone()),
                }
            }
            Source::Pearl => match armed {
                Some(m) if self.ready => Outcome::Dock {
                    screen: m.screen.clone(),
                },
                _ => {
                    let r = self.current();
                    Outcome::Moved {
                        rect: Rect::new(r.x.round(), r.y.round(), r.w, r.h),
                    }
                }
            },
        }
    }
}

/// The persisted pearls: the same shape the Nacre desktop keeps, with `kind` naming
/// the view. Unknown top-level fields survive a load and save.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    #[serde(default)]
    pub windows: Vec<Entry>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            windows: Vec::new(),
            extra: Map::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub kind: String,
    /// The output the pearl belongs to.
    #[serde(default)]
    pub origin: String,
    /// View-owned state captured at detach (a tab, a query), restored on open.
    #[serde(default)]
    pub initial: Value,
    pub placement: Option<Placement>,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub exact: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Placement {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
    pub fn exact(r: Rect) -> Self {
        Self {
            exact: true,
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
        }
    }
}
const MAX_PEARLS: usize = 32;
impl Layout {
    /// Missing means empty. A file that cannot be understood is preserved beside
    /// itself and the layout starts empty (never silently overwritten).
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(mut layout) if layout.version == 1 => {
                layout.windows.truncate(MAX_PEARLS);
                layout
                    .windows
                    .retain(|e| !e.kind.is_empty() && e.kind.len() <= 64);
                layout
            }
            _ => {
                let mut side = path.as_os_str().to_owned();
                side.push(".unreadable");
                let _ = std::fs::rename(path, side);
                Self::default()
            }
        }
    }
    /// Atomic replace in a private directory.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::other("layout path has no parent"))?;
        if !parent.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        let mut temp = path.as_os_str().to_owned();
        temp.push(format!(".{}.tmp", std::process::id()));
        let temp = std::path::PathBuf::from(temp);
        let mut text = serde_json::to_string(self)?;
        text.push('\n');
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&temp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn monitor(name: &str, x: f64, y: f64, w: f64, h: f64) -> Monitor {
        Monitor {
            name: name.into(),
            rect: Rect::new(x, y, w, h),
            frame: Insets {
                top: 50.,
                right: 10.,
                bottom: 10.,
                left: 10.,
            },
        }
    }
    fn t() -> Tuning {
        Tuning::default()
    }

    #[test]
    fn clamp_prefers_the_low_bound_when_the_range_is_inverted() {
        assert_eq!(clamp(5., 0., 10.), 5.);
        assert_eq!(clamp(-5., 0., 10.), 0.);
        assert_eq!(clamp(50., 0., 10.), 10.);
        // geometry.js: Math.max(low, Math.min(Math.max(low, high), value))
        assert_eq!(clamp(5., 20., 10.), 20.);
    }

    #[test]
    fn place_centres_on_the_point_and_stays_twelve_pixels_inside() {
        let m = Rect::new(0., 0., 1920., 1200.);
        let p = place(Point { x: 960., y: 300. }, (400., 300.), &m, &t());
        assert_eq!((p.x, p.y), (760., 276.));
        let p = place(Point { x: 0., y: 0. }, (400., 300.), &m, &t());
        assert_eq!((p.x, p.y), (12., 12.));
        let p = place(Point { x: 1919., y: 1199. }, (400., 300.), &m, &t());
        assert_eq!((p.x, p.y), (1508., 888.));
        // A second output is offset, not clamped to the first.
        let m2 = Rect::new(1920., 0., 1280., 800.);
        let p = place(Point { x: 2000., y: 100. }, (400., 300.), &m2, &t());
        assert_eq!((p.x, p.y), (1932., 76.));
    }

    #[test]
    fn outward_is_signed_per_edge() {
        assert_eq!(outward(Edge::Top, 3., 7.), 7.);
        assert_eq!(outward(Edge::Bottom, 3., 7.), -7.);
        assert_eq!(outward(Edge::Left, 3., 7.), 3.);
        assert_eq!(outward(Edge::Right, 3., 7.), -3.);
    }

    #[test]
    fn magnet_measures_from_the_frame_edge_and_arms_with_hysteresis() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let body = Rect::new(400., 50., 1000., 600.);
        // Flush with the frame: distance 0, armed.
        let m = magnet(
            Edge::Top,
            body,
            Point { x: 800., y: 300. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert!(m.armed && m.distance == 0. && m.screen == "A");
        assert_eq!((m.origin.x, m.origin.y), (900., 50.));
        assert_eq!((m.tip.x, m.tip.y), (900., 50.));
        // 90 px away: not armed on its own, but still armed once it was.
        let far = body.translated(0., 90.);
        let p = Point { x: 800., y: 390. };
        let cold = magnet(Edge::Top, far, p, &ms, None, body, &t()).unwrap();
        assert!(!cold.armed && cold.distance == 90.);
        let mut warm_from = cold.clone();
        warm_from.armed = true;
        let warm = magnet(Edge::Top, far, p, &ms, Some(&warm_from), body, &t()).unwrap();
        assert!(warm.armed);
        // Hysteresis does not carry to another edge or another output.
        warm_from.edge = Edge::Left;
        assert!(
            !magnet(Edge::Top, far, p, &ms, Some(&warm_from), body, &t())
                .unwrap()
                .armed
        );
        // Beyond the capture distance there is no guide at all.
        let gone = body.translated(0., 171.);
        assert!(
            magnet(
                Edge::Top,
                gone,
                Point { x: 800., y: 500. },
                &ms,
                None,
                body,
                &t()
            )
            .is_none()
        );
        let edge_of = body.translated(0., 170.);
        assert!(
            magnet(
                Edge::Top,
                edge_of,
                Point { x: 800., y: 500. },
                &ms,
                None,
                body,
                &t()
            )
            .is_some()
        );
    }

    #[test]
    fn magnet_covers_every_edge_with_its_own_frame_inset() {
        let ms = [monitor("A", 0., 0., 1000., 800.)];
        // Bottom: the panel's lower side against y = 800 - 10.
        let body = Rect::new(100., 300., 800., 490.);
        let m = magnet(
            Edge::Bottom,
            body,
            Point { x: 500., y: 400. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert_eq!(m.distance, 0.);
        assert_eq!((m.origin.x, m.origin.y), (500., 790.));
        // Left: against x = 10; Right: against x = 990.
        let body = Rect::new(10., 200., 400., 400.);
        let m = magnet(
            Edge::Left,
            body,
            Point { x: 100., y: 300. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert_eq!(m.distance, 0.);
        assert_eq!((m.origin.x, m.origin.y), (10., 400.));
        let body = Rect::new(590., 200., 400., 400.);
        let m = magnet(
            Edge::Right,
            body,
            Point { x: 700., y: 300. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert_eq!(m.distance, 0.);
        assert_eq!((m.tip.x, m.tip.y), (990., 400.));
    }

    #[test]
    fn magnet_origin_keeps_the_grab_but_slides_at_most_ninety_six() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let anchor = Rect::new(400., 50., 1000., 600.); // centre 900
        // Moved right by 300: the panel's centre is 1200, the root lags 96 behind.
        let body = anchor.translated(300., 40.);
        let m = magnet(
            Edge::Top,
            body,
            Point { x: 1100., y: 400. },
            &ms,
            None,
            anchor,
            &t(),
        )
        .unwrap();
        assert_eq!(m.tip.x, 1200.);
        assert_eq!(m.origin.x, 1104.);
        // Kept clear of the corner by 24.
        let body = Rect::new(-400., 50., 1000., 600.);
        let m = magnet(
            Edge::Top,
            body,
            Point { x: 10., y: 300. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert_eq!(m.tip.x, 100.);
    }

    #[test]
    fn magnet_needs_overlap_with_the_monitor_and_a_pointer_on_it() {
        let ms = [
            monitor("A", 0., 0., 1920., 1200.),
            monitor("B", 1920., 0., 1280., 800.),
        ];
        let body = Rect::new(1900., 50., 1000., 600.);
        // Pointer on B, panel mostly over B: attaches to B, not A.
        let m = magnet(
            Edge::Top,
            body,
            Point { x: 2500., y: 300. },
            &ms,
            None,
            body,
            &t(),
        )
        .unwrap();
        assert_eq!(m.screen, "B");
        // Hardly any overlap with A (20 px of a 1000 px panel, need min(96, 333)).
        assert!(
            magnet(
                Edge::Top,
                body,
                Point { x: 1910., y: 300. },
                &ms,
                None,
                body,
                &t()
            )
            .is_none()
        );
        // Pointer outside every monitor.
        assert!(
            magnet(
                Edge::Top,
                body,
                Point { x: -5., y: 300. },
                &ms,
                None,
                body,
                &t()
            )
            .is_none()
        );
        // Not finite, or empty.
        let nan = Rect::new(f64::NAN, 0., 10., 10.);
        assert!(magnet(Edge::Top, nan, Point { x: 5., y: 5. }, &ms, None, nan, &t()).is_none());
        let empty = Rect::new(0., 0., 0., 10.);
        assert!(
            magnet(
                Edge::Top,
                empty,
                Point { x: 5., y: 5. },
                &ms,
                None,
                empty,
                &t()
            )
            .is_none()
        );
        // A small panel needs only a third of itself: 90 px wide, 30 px over.
        let small = Rect::new(-60., 50., 90., 90.);
        assert!(
            magnet(
                Edge::Top,
                small,
                Point { x: 10., y: 60. },
                &ms,
                None,
                small,
                &t()
            )
            .is_some()
        );
    }

    #[test]
    fn landing_sits_centred_on_the_usable_edge() {
        let m = monitor("A", 100., 0., 1920., 1200.);
        let l = landing(Edge::Top, &m);
        assert_eq!((l.origin.x, l.origin.y), (1060., 50.));
        let l = landing(Edge::Bottom, &m);
        assert_eq!((l.origin.x, l.origin.y), (1060., 1190.));
        // Vertical edges centre between the top and bottom insets: 50 + (1200-60)/2.
        let l = landing(Edge::Left, &m);
        assert_eq!((l.origin.x, l.origin.y), (110., 620.));
        let l = landing(Edge::Right, &m);
        assert_eq!((l.origin.x, l.origin.y), (2010., 620.));
    }

    fn drag(source: Source, ms: &[Monitor], body: Rect, start: Point) -> Drag {
        Drag::begin(source, Edge::Top, "A", body, start, ms, &t())
    }

    #[test]
    fn a_press_below_the_threshold_springs_back() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let body = Rect::new(400., 50., 1000., 600.);
        let mut d = drag(Source::Docked, &ms, body, Point { x: 800., y: 300. });
        d.update(Point { x: 805., y: 310. }, &ms, &t());
        assert!(!d.ready);
        assert_eq!(d.release(), Outcome::Return);
    }

    #[test]
    fn pulling_free_detaches_at_the_rounded_rectangle() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let body = Rect::new(400., 50., 1000., 600.);
        let mut d = drag(Source::Docked, &ms, body, Point { x: 800., y: 300. });
        d.update(Point { x: 900.4, y: 520.6 }, &ms, &t());
        assert!(d.ready);
        assert!(d.magnet.is_none(), "270 px away is out of reach");
        assert_eq!(
            d.release(),
            Outcome::Detach {
                rect: Rect::new(500., 271., 1000., 600.),
                dock: None
            }
        );
    }

    #[test]
    fn released_armed_on_its_own_edge_returns_but_on_another_output_it_docks_there() {
        let ms = [
            monitor("A", 0., 0., 1920., 1200.),
            monitor("B", 1920., 0., 1280., 800.),
        ];
        let body = Rect::new(400., 50., 1000., 600.);
        let mut d = drag(Source::Docked, &ms, body, Point { x: 800., y: 300. });
        d.update(Point { x: 840., y: 330. }, &ms, &t());
        assert!(d.ready && d.magnet.as_ref().unwrap().armed);
        assert_eq!(d.release(), Outcome::Return);
        // Carry it over to output B and flush with B's top inset.
        let mut d = drag(Source::Docked, &ms, body, Point { x: 800., y: 300. });
        d.update(Point { x: 2500., y: 300. }, &ms, &t());
        let m = d.magnet.clone().unwrap();
        assert_eq!(m.screen, "B");
        assert!(m.armed);
        match d.release() {
            Outcome::Detach { dock, .. } => assert_eq!(dock.as_deref(), Some("B")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_moved_pearl_docks_near_an_edge_and_otherwise_stays() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let body = Rect::new(500., 300., 600., 400.);
        let mut d = drag(Source::Pearl, &ms, body, Point { x: 700., y: 320. });
        d.update(Point { x: 710., y: 40. }, &ms, &t());
        // Body moved to y = 20, 30 px above the 50 px inset: armed.
        assert_eq!(d.release(), Outcome::Dock { screen: "A".into() });
        let mut d = drag(Source::Pearl, &ms, body, Point { x: 700., y: 320. });
        d.update(Point { x: 800., y: 420. }, &ms, &t());
        assert_eq!(
            d.release(),
            Outcome::Moved {
                rect: Rect::new(600., 400., 600., 400.)
            }
        );
        // Barely moved: it stays where it is, it does not dock.
        let mut d = drag(
            Source::Pearl,
            &ms,
            Rect::new(500., 60., 600., 400.),
            Point { x: 700., y: 80. },
        );
        d.update(Point { x: 703., y: 82. }, &ms, &t());
        assert!(matches!(d.release(), Outcome::Moved { .. }));
    }

    #[test]
    fn the_film_root_stays_with_the_grab_while_the_panel_follows_the_pointer() {
        let ms = [monitor("A", 0., 0., 1920., 1200.)];
        let body = Rect::new(400., 50., 1000., 600.);
        let mut d = drag(Source::Docked, &ms, body, Point { x: 800., y: 300. });
        d.update(Point { x: 800., y: 360. }, &ms, &t());
        let m = d.magnet.unwrap();
        assert_eq!(m.distance, 60.);
        assert_eq!((m.origin.y, m.tip.y), (50., 110.));
        assert_eq!(m.origin.x, 900.);
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bead-pearl-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn layout_round_trips_privately_and_keeps_unknown_fields() {
        let dir = temp("roundtrip");
        let path = dir.join("state/pearls.json");
        assert_eq!(Layout::load(&path), Layout::default());
        let mut layout = Layout::default();
        layout
            .extra
            .insert("ai".into(), serde_json::json!({"edge": ""}));
        layout.windows.push(Entry {
            kind: "dashboard-system".into(),
            origin: "A".into(),
            initial: serde_json::json!({"tab": 2}),
            placement: Some(Placement::exact(Rect::new(10., 20., 300., 400.))),
        });
        layout.save(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(Layout::load(&path), layout);
        // No temporary file is left behind.
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unreadable_layout_is_preserved_not_overwritten() {
        let dir = temp("corrupt");
        let path = dir.join("pearls.json");
        for (i, bad) in ["{not json", "[]", r#"{"version":2,"windows":[]}"#]
            .into_iter()
            .enumerate()
        {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(Layout::load(&path), Layout::default(), "{bad}");
            let side = dir.join("pearls.json.unreadable");
            assert_eq!(std::fs::read_to_string(&side).unwrap(), bad, "{i}");
            assert!(!path.exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_bounds_the_pearl_count_and_drops_nameless_entries() {
        let dir = temp("bounds");
        let path = dir.join("pearls.json");
        let mut windows = vec![serde_json::json!({"kind": ""})];
        for _ in 0..40 {
            windows.push(serde_json::json!({"kind": "launcher", "origin": "A"}));
        }
        std::fs::write(
            &path,
            serde_json::json!({"version": 1, "windows": windows}).to_string(),
        )
        .unwrap();
        let layout = Layout::load(&path);
        assert_eq!(layout.windows.len(), MAX_PEARLS - 1);
        assert!(layout.windows.iter().all(|e| e.kind == "launcher"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

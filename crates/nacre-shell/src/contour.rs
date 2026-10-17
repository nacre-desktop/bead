// SPDX-License-Identifier: GPL-3.0-only
//! Port of Nacre drawers/contour.js at e0a81ce7, for the joined frame silhouette.
//! Coordinates and quadratic corner cuts intentionally match the QML algorithm.
use bead::render::Rect;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point(f64, f64);
impl Point {
    fn key(self) -> (i64, i64) {
        (
            (self.0 * 1000.).round() as i64,
            (self.1 * 1000.).round() as i64,
        )
    }
}
struct Edge {
    a: Point,
    b: Point,
    direction: usize,
    used: bool,
}
#[derive(Debug, Default)]
pub struct Geometry {
    pub body: String,
    pub rim: String,
}
fn round(n: f64) -> f64 {
    (n * 1000.).round() / 1000.
}
fn area(points: &[Point]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = points[(i + 1) % points.len()];
            p.0 * n.1 - n.0 * p.1
        })
        .sum::<f64>()
        / 2.
}
fn outer(a: Point, b: Point, width: f64, height: f64) -> bool {
    a.0 == b.0 && (a.0 == 0. || a.0 == width) || a.1 == b.1 && (a.1 == 0. || a.1 == height)
}
fn path(points: &[Point], closed: bool, width: f64, height: f64, radius: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let pair = |p: Point| format!("{} {}", round(p.0), round(p.1));
    let corners = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if !closed && (i == 0 || i == points.len() - 1) {
                return (*p, *p);
            }
            let r = if p.0 == 0. || p.0 == width || p.1 == 0. || p.1 == height {
                0.
            } else {
                radius
            };
            let toward = |q: Point| {
                let dx = q.0 - p.0;
                let dy = q.1 - p.1;
                let cut = r.min((dx.abs() + dy.abs()) / 2.);
                Point(
                    p.0 + if dx == 0. { 0. } else { dx.signum() * cut },
                    p.1 + if dy == 0. { 0. } else { dy.signum() * cut },
                )
            };
            (
                toward(points[(i + points.len() - 1) % points.len()]),
                toward(points[(i + 1) % points.len()]),
            )
        })
        .collect::<Vec<_>>();
    let mut result = format!("M{}", pair(corners[0].0));
    for (i, (entry, exit)) in corners.iter().enumerate() {
        if i > 0 {
            result.push_str(&format!(" L{}", pair(*entry)));
        }
        result.push_str(&format!(" Q{} {}", pair(points[i]), pair(*exit)));
    }
    if closed {
        result.push_str(" Z");
    }
    result
}

/// FrameLips currently supplies no rim handles, exactly as in the reference.
/// This uses no handle approximation; handle geometry must be ported if enabled.
pub fn geometry(
    width: f64,
    height: f64,
    rectangles: &[Rect],
    radius: f64,
) -> Result<Geometry, String> {
    if width <= 0. || height <= 0. {
        return Ok(Geometry::default());
    }
    let boxes = rectangles
        .iter()
        .map(|r| {
            (
                round((r.x as f64).clamp(0., width)),
                round((r.y as f64).clamp(0., height)),
                round(((r.x + r.w) as f64).clamp(0., width)),
                round(((r.y + r.h) as f64).clamp(0., height)),
            )
        })
        .filter(|(x, y, r, b)| r > x && b > y)
        .collect::<Vec<_>>();
    if boxes.is_empty() {
        return Ok(Geometry::default());
    }
    let mut xs = vec![0., width];
    let mut ys = vec![0., height];
    for &(x, y, r, b) in &boxes {
        xs.extend([x, r]);
        ys.extend([y, b]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let occupied = ys
        .windows(2)
        .map(|y| {
            xs.windows(2)
                .map(|x| {
                    let px = (x[0] + x[1]) / 2.;
                    let py = (y[0] + y[1]) / 2.;
                    boxes
                        .iter()
                        .any(|&(x, y, r, b)| px > x && px < r && py > y && py < b)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::<Edge>::new();
    let mut outgoing = BTreeMap::<(i64, i64), Vec<usize>>::new();
    let mut add = |a: Point, b: Point, direction| {
        outgoing.entry(a.key()).or_default().push(edges.len());
        edges.push(Edge {
            a,
            b,
            direction,
            used: false,
        });
    };
    for row in 0..occupied.len() {
        for col in 0..occupied[row].len() {
            if !occupied[row][col] {
                continue;
            }
            let tl = Point(xs[col], ys[row]);
            let tr = Point(xs[col + 1], ys[row]);
            let br = Point(xs[col + 1], ys[row + 1]);
            let bl = Point(xs[col], ys[row + 1]);
            if row == 0 || !occupied[row - 1][col] {
                add(tl, tr, 0);
            }
            if !occupied[row].get(col + 1).copied().unwrap_or(false) {
                add(tr, br, 1);
            }
            if row + 1 == occupied.len() || !occupied[row + 1][col] {
                add(br, bl, 2);
            }
            if col == 0 || !occupied[row][col - 1] {
                add(bl, tl, 3);
            }
        }
    }
    let mut loops = Vec::<Vec<Point>>::new();
    for first in 0..edges.len() {
        if edges[first].used {
            continue;
        }
        let mut edge = first;
        let mut points = Vec::new();
        loop {
            edges[edge].used = true;
            points.push(edges[edge].a);
            if edges[edge].b.key() == edges[first].a.key() {
                break;
            }
            let choices = outgoing
                .get(&edges[edge].b.key())
                .ok_or("Unclosed frame contour")?;
            edge = *choices
                .iter()
                .filter(|&&i| !edges[i].used)
                .min_by_key(|&&i| {
                    let turn = (edges[i].direction + 4 - edges[edge].direction) % 4;
                    [1, 0, 3, 2].iter().position(|&t| t == turn).unwrap_or(4)
                })
                .ok_or("Unclosed frame contour")?;
            if points.len() > edges.len() {
                return Err("Unclosed frame contour".into());
            }
        }
        loops.push(
            points
                .iter()
                .enumerate()
                .filter_map(|(i, &p)| {
                    let a = points[(i + points.len() - 1) % points.len()];
                    let b = points[(i + 1) % points.len()];
                    ((p.0 - a.0) * (b.1 - p.1) != (p.1 - a.1) * (b.0 - p.0)).then_some(p)
                })
                .collect(),
        );
    }
    let body = loops
        .iter()
        .map(|p| path(p, true, width, height, radius))
        .collect::<Vec<_>>()
        .join(" ");
    let mut rim = Vec::new();
    for points in &loops {
        let excluded = points
            .iter()
            .enumerate()
            .map(|(i, &p)| outer(p, points[(i + 1) % points.len()], width, height))
            .collect::<Vec<_>>();
        if area(points) < 0. || !excluded.contains(&true) {
            rim.push(path(points, true, width, height, radius));
            continue;
        }
        let start = excluded
            .iter()
            .position(|&e| e)
            .ok_or("Missing frame outer edge")?;
        let mut chain = Vec::new();
        for step in 1..=points.len() {
            let i = (start + step) % points.len();
            if excluded[i] {
                if chain.len() > 1 {
                    rim.push(path(&chain, false, width, height, radius));
                }
                chain.clear();
            } else {
                if chain.is_empty() {
                    chain.push(points[i]);
                }
                chain.push(points[(i + 1) % points.len()]);
            }
        }
        if chain.len() > 1 {
            rim.push(path(&chain, false, width, height, radius));
        }
    }
    Ok(Geometry {
        body,
        rim: rim.join(" "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_paths_and_attachment_match_real_qt_reference() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../shell/tests/fixtures/placement-qt.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let edge = case["edge"].as_str().unwrap();
            let width = case["width"].as_f64().unwrap() as f32;
            let height = case["height"].as_f64().unwrap() as f32;
            let attachment = if edge == "popout" {
                crate::placement::Attachment::Popout
            } else {
                crate::placement::Attachment::Attached(match edge {
                    "right" => bead::core::Edge::Right,
                    "left" => bead::core::Edge::Left,
                    "bottom" => bead::core::Edge::Bottom,
                    _ => bead::core::Edge::Top,
                })
            };
            let panel = crate::placement::Frame::default().panel(
                attachment,
                (1920., 1200.),
                (width, height),
                case["center"].as_f64().map(|n| n as f32),
            );
            assert_eq!(panel.x as f64, case["x"].as_f64().unwrap(), "{name}");
            assert_eq!(panel.y as f64, case["y"].as_f64().unwrap(), "{name}");
            let rects = [
                Rect::new(0., 0., 1920., 50.),
                Rect::new(0., 0., 10., 1200.),
                Rect::new(1910., 0., 10., 1200.),
                Rect::new(0., 1190., 1920., 10.),
                panel,
            ];
            let result = geometry(1920., 1200., &rects, 25.).unwrap();
            assert_eq!(result.body, case["body"].as_str().unwrap(), "body {name}");
            assert_eq!(result.rim, case["rim"].as_str().unwrap(), "rim {name}");
        }
    }
}

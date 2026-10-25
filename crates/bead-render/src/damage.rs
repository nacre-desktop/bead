// SPDX-License-Identifier: MIT OR Apache-2.0
//! Semantic surface damage, independent of back-buffer repaint history.
use crate::{
    Damage, Rect,
    tree::{Frame, hash_primitive, rect},
};
use std::hash::Hasher;
#[derive(Clone)]
struct Record {
    key: u64,
    bounds: Damage,
    geometry: Rect,
    clip: Option<Rect>,
    opacity: u32,
}
impl Record {
    fn same_pixels(&self, other: &Self) -> bool {
        self.key == other.key
            && self.bounds == other.bounds
            && self.geometry == other.geometry
            && self.clip == other.clip
            && self.opacity == other.opacity
    }
}
#[derive(Default)]
pub struct FrameDamage {
    previous: Vec<Record>,
    dimensions: Option<(u32, u32, u32)>,
}
fn intersect(a: Damage, b: Damage) -> Damage {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    Damage {
        x,
        y,
        w: (a.x + a.w).min(b.x + b.w).saturating_sub(x),
        h: (a.y + a.h).min(b.y + b.h).saturating_sub(y),
    }
}
/// The pixels in exactly one of two rectangles when they differ along one axis only (else both rectangles).
fn clip_difference(a: Damage, b: Damage) -> Vec<Damage> {
    let strip = |from: u32, to: u32| (from.min(to), from.max(to));
    if a.x == b.x && a.w == b.w && (a.y == b.y || a.y + a.h == b.y + b.h) {
        let (from, to) = if a.y == b.y {
            strip(a.y + a.h, b.y + b.h)
        } else {
            strip(a.y, b.y)
        };
        return vec![Damage {
            x: a.x,
            y: from,
            w: a.w,
            h: to - from,
        }];
    }
    if a.y == b.y && a.h == b.h && (a.x == b.x || a.x + a.w == b.x + b.w) {
        let (from, to) = if a.x == b.x {
            strip(a.x + a.w, b.x + b.w)
        } else {
            strip(a.x, b.x)
        };
        return vec![Damage {
            x: from,
            y: a.y,
            w: to - from,
            h: a.h,
        }];
    }
    vec![a, b]
}
fn add(regions: &mut Vec<Damage>, mut next: Damage) {
    if next.w == 0 || next.h == 0 {
        return;
    }
    let mut i = 0;
    while i < regions.len() {
        let old = regions[i];
        if next.x <= old.x + old.w
            && old.x <= next.x + next.w
            && next.y <= old.y + old.h
            && old.y <= next.y + next.h
        {
            next = next.union(regions.swap_remove(i));
            i = 0;
        } else {
            i += 1;
        }
    }
    regions.push(next);
}
/// Pair the items that differ between two frames, in order. Identical records are matched by a
/// bounded longest-common-subsequence over the differing middle; each unmatched run between two
/// matches then pairs by position, with the surplus an insertion or a removal.
fn align(old: &[Record], new: &[Record]) -> Vec<(Option<usize>, Option<usize>)> {
    const CELLS: usize = 1 << 16;
    let common = old.len().min(new.len());
    let prefix = (0..common)
        .take_while(|&k| old[k].same_pixels(&new[k]))
        .count();
    let suffix = (0..common - prefix)
        .take_while(|&k| old[old.len() - 1 - k].same_pixels(&new[new.len() - 1 - k]))
        .count();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    let (rows, columns) = (old_end - prefix, new_end - prefix);
    let mut anchors = Vec::new();
    if rows > 0 && columns > 0 && rows * columns <= CELLS {
        let width = columns + 1;
        let mut length = vec![0u16; (rows + 1) * width];
        for i in (0..rows).rev() {
            for j in (0..columns).rev() {
                length[i * width + j] = if old[prefix + i].same_pixels(&new[prefix + j]) {
                    length[(i + 1) * width + j + 1] + 1
                } else {
                    length[(i + 1) * width + j].max(length[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < rows && j < columns {
            if old[prefix + i].same_pixels(&new[prefix + j]) {
                anchors.push((prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if length[(i + 1) * width + j] >= length[i * width + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    anchors.push((old_end, new_end));
    let mut pairs = Vec::new();
    let (mut o, mut n) = (prefix, prefix);
    for (anchor_old, anchor_new) in anchors {
        for k in 0..(anchor_old - o).max(anchor_new - n) {
            pairs.push((
                (o + k < anchor_old).then_some(o + k),
                (n + k < anchor_new).then_some(n + k),
            ));
        }
        (o, n) = (anchor_old + 1, anchor_new + 1);
    }
    pairs
}
impl FrameDamage {
    pub fn update(
        &mut self,
        frame: &Frame,
        offset: (f32, f32),
        opacity: f32,
        width: u32,
        height: u32,
    ) -> crate::Result<Vec<Damage>> {
        let mut current = Vec::with_capacity(frame.items.len());
        let mut dirty = Vec::new();
        let changed_size = self.dimensions != Some((width, height, frame.scale.to_bits()));
        for item in &frame.items {
            let source = if let Some(raster) = &item.raster {
                let original = rect(&item.primitive);
                Rect::new(
                    original.x + raster.origin.0 as f32 / frame.scale,
                    original.y + raster.origin.1 as f32 / frame.scale,
                    raster.pixels.width() as f32 / frame.scale,
                    raster.pixels.height() as f32 / frame.scale,
                )
            } else {
                rect(&item.primitive)
            };
            let geometry = source.translated(offset.0, offset.1);
            let pad = match item.primitive {
                crate::Primitive::Rounded { border, .. } => border.map_or(1., |(_, w)| w / 2. + 1.),
                crate::Primitive::Arc { .. } => 1.,
                _ => 0.,
            };
            let mut bounds = Rect::new(
                geometry.x - pad,
                geometry.y - pad,
                geometry.w + pad * 2.,
                geometry.h + pad * 2.,
            )
            .pixels(frame.scale, width, height);
            let clip = item.clip.map(|c| c.translated(offset.0, offset.1));
            if let Some(c) = clip {
                bounds = intersect(bounds, c.pixels(frame.scale, width, height));
            }
            let key = if let Some(raster) = &item.raster {
                raster.key
            } else {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                hash_primitive(&item.primitive, false, &mut h);
                h.finish()
            };
            let next = Record {
                key,
                bounds,
                geometry,
                clip,
                opacity: opacity.to_bits(),
            };
            current.push(next);
        }
        // Damage is a comparison of ordered pixels, not UI state IDs. Identical items stay clean even
        // when an insert or removal moved their paint slot (grid <-> list keeps the search bar).
        for (old, new) in align(&self.previous, &current) {
            let (old, new) = (
                old.map(|i| &self.previous[i]),
                new.map(|i| (&frame.items[i], &current[i])),
            );
            match (old, new) {
                (None, None) => {}
                (None, Some((_, next))) => add(&mut dirty, next.bounds),
                (Some(old), None) => add(&mut dirty, old.bounds),
                (Some(old), Some((item, next))) => {
                    if old.same_pixels(next) {
                        continue;
                    }
                    if let Some(raster) = &item.raster
                        && raster.base_key == Some(old.key)
                        && old.geometry.x == next.geometry.x
                        && old.geometry.y == next.geometry.y
                        && old.clip == next.clip
                        && old.opacity == next.opacity
                    {
                        let d = raster.changed;
                        let mut changed = Rect::new(
                            next.geometry.x + d.x as f32 / frame.scale,
                            next.geometry.y + d.y as f32 / frame.scale,
                            d.w as f32 / frame.scale,
                            d.h as f32 / frame.scale,
                        )
                        .pixels(frame.scale, width, height);
                        if let Some(c) = next.clip {
                            changed = intersect(changed, c.pixels(frame.scale, width, height));
                        }
                        add(&mut dirty, changed);
                    } else if old.key == next.key
                        && old.geometry == next.geometry
                        && old.opacity == next.opacity
                        && old.clip != next.clip
                    {
                        // Only the clip moved (a reveal window): the pixels between the two clip edges.
                        for part in clip_difference(old.bounds, next.bounds) {
                            add(&mut dirty, part);
                        }
                    } else {
                        add(&mut dirty, old.bounds);
                        add(&mut dirty, next.bounds);
                    }
                }
            }
        }
        self.previous = current;
        self.dimensions = Some((width, height, frame.scale.to_bits()));
        if changed_size {
            return Ok(vec![Damage::full(width, height)]);
        }
        // Greedy minimum extra area; retain at most four compositor rectangles.
        while dirty.len() > 4 {
            let mut best = (u64::MAX, 0, 1);
            for a in 0..dirty.len() {
                for b in a + 1..dirty.len() {
                    let union = dirty[a].union(dirty[b]);
                    let cost = u64::from(union.w) * u64::from(union.h)
                        - u64::from(dirty[a].w) * u64::from(dirty[a].h)
                        - u64::from(dirty[b].w) * u64::from(dirty[b].h);
                    if cost < best.0 {
                        best = (cost, a, b);
                    }
                }
            }
            let b = dirty.swap_remove(best.2);
            let a = dirty.swap_remove(best.1);
            add(&mut dirty, a.union(b));
        }
        Ok(dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_moving_clip_edge_damages_only_the_strip_between_the_edges() {
        let rect = |y, h| Damage { x: 10, y, w: 50, h };
        assert_eq!(
            clip_difference(rect(0, 100), rect(0, 130)),
            vec![rect(100, 30)]
        );
        assert_eq!(
            clip_difference(rect(30, 70), rect(0, 100)),
            vec![rect(0, 30)]
        );
        let moved = Damage {
            x: 12,
            ..rect(0, 100)
        };
        assert_eq!(clip_difference(rect(0, 100), moved).len(), 2);
    }
    #[test]
    fn ordered_pixel_damage_handles_ids_insertion_removal_and_overlap() {
        use crate::{
            PixelFormat, Primitive, ShmTarget,
            tree::{Item, paint},
        };
        let item = |id: &str, rect: Rect, fill| Item {
            id: id.into(),
            primitive: Primitive::Rounded {
                rect,
                radius: 2.,
                fill,
                border: None,
            },
            clip: None,
            raster: None,
        };
        for scale in [1., 1.25, 1.5] {
            for offset in [(0., 0.), (0.3333, -0.5)] {
                let frame = |items| Frame {
                    items,
                    scale,
                    key: 0,
                    layer_count: 0,
                    layers: vec![],
                    _accounting: None,
                };
                let old = frame(vec![
                    item("root", Rect::new(0., 0., 96., 72.), [10, 20, 30, 255]),
                    item("old-row", Rect::new(4., 4., 88., 60.), [0; 4]),
                    item(
                        "old-selection",
                        Rect::new(8., 8., 80., 20.),
                        [50, 60, 70, 180],
                    ),
                    item(
                        "old-label",
                        Rect::new(14., 10., 12., 8.),
                        [180, 40, 30, 128],
                    ),
                ]);
                for case in 0..8 {
                    let mut next = frame(old.items.clone());
                    for (i, node) in next.items.iter_mut().enumerate().skip(1) {
                        node.id = format!("new-{i}");
                    }
                    match case {
                        0 => {} // All pixels identical, only action/state IDs change.
                        1 => {
                            if let Primitive::Rounded { fill, .. } = &mut next.items[3].primitive {
                                *fill = [30, 80, 190, 128];
                            }
                        }
                        2 => {
                            // An old ID survives elsewhere; it must not be consumed.
                            next.items[3].id = old.items[2].id.clone();
                            if let Primitive::Rounded { rect, .. } = &mut next.items[3].primitive {
                                rect.w = 3.;
                            }
                        }
                        3 => {
                            // Stable overlapping IDs reverse their paint order.
                            next = frame(old.items.clone());
                            next.items.swap(2, 3);
                        }
                        4 => {
                            next.items.pop();
                        } // Removed ink must still be erased.
                        5 => {
                            next.items.insert(
                                1,
                                item("inserted", Rect::new(2., 2., 50., 50.), [9, 70, 19, 90]),
                            );
                        }
                        6 => {
                            next.items.remove(0);
                        }
                        7 => {
                            next.items.remove(1);
                            next.items.push(item(
                                "appended",
                                Rect::new(20., 20., 20., 20.),
                                [90, 80, 12, 110],
                            ));
                        }
                        _ => unreachable!(),
                    }
                    let mut tracker = FrameDamage::default();
                    tracker.update(&old, offset, 1., 160, 120).unwrap();
                    let dirty = tracker.update(&next, offset, 1., 160, 120).unwrap();
                    if case == 0 {
                        assert!(dirty.is_empty());
                    }
                    if case == 1 {
                        assert!(
                            dirty.iter().all(|d| d.w < (40. * scale) as u32),
                            "unchanged row backgrounds widened a text-only repaint"
                        );
                    }
                    let damage = dirty.into_iter().fold(Damage::full(0, 0), Damage::union);
                    for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
                        let mut repaired = vec![0; 160 * 120 * 4];
                        let mut expected = repaired.clone();
                        paint(
                            &old,
                            offset,
                            ShmTarget {
                                bytes: &mut repaired,
                                width: 160,
                                height: 120,
                                scale,
                                format,
                            },
                            Damage::full(160, 120),
                        )
                        .unwrap();
                        paint(
                            &next,
                            offset,
                            ShmTarget {
                                bytes: &mut expected,
                                width: 160,
                                height: 120,
                                scale,
                                format,
                            },
                            Damage::full(160, 120),
                        )
                        .unwrap();
                        paint(
                            &next,
                            offset,
                            ShmTarget {
                                bytes: &mut repaired,
                                width: 160,
                                height: 120,
                                scale,
                                format,
                            },
                            damage,
                        )
                        .unwrap();
                        assert!(
                            repaired == expected,
                            "stale pixels: case{case}, scale{scale}, offset{offset:?}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn an_insert_does_not_dirty_identical_items_that_only_changed_paint_slot() {
        let item = |id: &str, rect: Rect| crate::tree::Item {
            id: id.into(),
            primitive: crate::Primitive::Rounded {
                rect,
                radius: 2.,
                fill: [40, 40, 40, 255],
                border: None,
            },
            clip: None,
            raster: None,
        };
        let frame = |items| Frame {
            _accounting: None,
            items,
            scale: 1.,
            key: 0,
            layer_count: 0,
            layers: vec![],
        };
        let bar = item("bar", Rect::new(0., 90., 100., 20.));
        let tail = |width| item("tail", Rect::new(110., 90., width, 20.));
        let mut damage = FrameDamage::default();
        damage
            .update(
                &frame(vec![
                    item("a", Rect::new(0., 0., 50., 50.)),
                    bar.clone(),
                    tail(30.),
                ]),
                (0., 0.),
                1.,
                200,
                200,
            )
            .unwrap();
        let dirty = damage
            .update(
                &frame(vec![
                    item("a", Rect::new(0., 0., 50., 50.)),
                    item("b", Rect::new(60., 0., 30., 30.)),
                    bar,
                    tail(40.),
                ]),
                (0., 0.),
                1.,
                200,
                200,
            )
            .unwrap();
        // The inserted item and the changed last item, but not the shifted bar between them.
        assert!(
            dirty
                .iter()
                .all(|d| d.x >= 60 || d.y + d.h <= 50 || d.x >= 110),
            "the shifted bar stayed clean: {dirty:?}"
        );
        assert!(dirty.iter().all(|d| !(d.x < 100 && d.y >= 90)), "{dirty:?}");
    }
    #[test]
    fn hover_is_local_and_removing_a_node_erases_its_old_bounds() {
        let primitive = crate::Primitive::Rounded {
            rect: Rect::new(20., 20., 10., 10.),
            radius: 2.,
            fill: [40, 40, 40, 255],
            border: None,
        };
        let mut frame = Frame {
            _accounting: None,
            items: vec![crate::tree::Item {
                id: "button".into(),
                primitive,
                clip: None,
                raster: None,
            }],
            scale: 1.5,
            key: 0,
            layer_count: 1,
            layers: vec![],
        };
        let mut damage = FrameDamage::default();
        assert_eq!(
            damage.update(&frame, (0., 0.), 1., 300, 200).unwrap(),
            vec![Damage::full(300, 200)]
        );
        assert!(
            damage
                .update(&frame, (0., 0.), 1., 300, 200)
                .unwrap()
                .is_empty()
        );
        if let crate::Primitive::Rounded { fill, .. } = &mut frame.items[0].primitive {
            *fill = [60, 60, 60, 255];
        }
        let hover = damage.update(&frame, (0., 0.), 1., 300, 200).unwrap();
        assert_eq!(hover.len(), 1);
        assert!(hover[0].w * hover[0].h < 1000);
        frame.items.clear();
        assert_eq!(
            damage.update(&frame, (0., 0.), 1., 300, 200).unwrap(),
            hover
        );
    }

    #[test]
    fn arc_progress_repairs_only_clipped_pixels_at_fractional_scale_and_honors_round_caps() {
        use crate::{
            PixelFormat, Primitive, ShmTarget,
            tree::{Item, paint},
        };
        let clip = Rect::new(40., 25., 10., 12.);
        let arc = |sweep_angle, round_caps| Item {
            id: "progress".into(),
            primitive: Primitive::Arc {
                rect: Rect::new(10., 10., 40., 40.),
                start_angle: 0.,
                sweep_angle,
                stroke_width: 8.,
                color: [255, 20, 10, 255],
                round_caps,
            },
            clip: Some(clip),
            raster: None,
        };
        let frame = |item| Frame {
            items: vec![item],
            scale: 1.5,
            key: 0,
            layer_count: 0,
            layers: vec![],
            _accounting: None,
        };
        let before = frame(arc(20., true));
        let after = frame(arc(90., true));
        let (width, height) = (120, 120);
        let mut tracker = FrameDamage::default();
        tracker
            .update(&before, (0., 0.), 1., width, height)
            .unwrap();
        let dirty = tracker.update(&after, (0., 0.), 1., width, height).unwrap();
        assert!(!dirty.is_empty());
        let clip_px = clip.pixels(1.5, width, height);
        assert!(
            dirty.iter().all(|d| {
                d.x >= clip_px.x
                    && d.y >= clip_px.y
                    && d.x + d.w <= clip_px.x + clip_px.w
                    && d.y + d.h <= clip_px.y + clip_px.h
            }),
            "arc damage escaped clip: {dirty:?}"
        );

        let mut repaired = vec![0; (width * height * 4) as usize];
        paint(
            &before,
            (0., 0.),
            ShmTarget {
                bytes: &mut repaired,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        let mut expected = vec![0; repaired.len()];
        paint(
            &after,
            (0., 0.),
            ShmTarget {
                bytes: &mut expected,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        for d in dirty {
            paint(
                &after,
                (0., 0.),
                ShmTarget {
                    bytes: &mut repaired,
                    width,
                    height,
                    scale: 1.5,
                    format: PixelFormat::Rgba,
                },
                d,
            )
            .unwrap();
        }
        assert!(
            repaired == expected,
            "incremental arc repaint differs first at {:?}",
            repaired
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .map(|i| (
                    i / 4 % width as usize,
                    i / 4 / width as usize,
                    repaired[i],
                    expected[i]
                ))
        );

        let cap_frame = |round_caps| frame(arc(90., round_caps));
        let mut round = vec![0; repaired.len()];
        let mut butt = vec![0; repaired.len()];
        paint(
            &cap_frame(true),
            (0., 0.),
            ShmTarget {
                bytes: &mut round,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        paint(
            &cap_frame(false),
            (0., 0.),
            ShmTarget {
                bytes: &mut butt,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        let (sample_x, sample_y) = (69, 42);
        let sample = ((sample_y * width + sample_x) * 4) as usize;
        assert!(
            round[sample + 3] > 0,
            "round cap should extend along its tangent"
        );
        assert_eq!(
            butt[sample + 3],
            0,
            "butt cap should stop at the path endpoint"
        );
        let mut translucent = arc(90., true);
        if let Primitive::Arc { color, .. } = &mut translucent.primitive {
            color[3] = 128;
        }
        let mut layered = frame(translucent.clone());
        layered.items.push(translucent);
        let mut pixels = vec![0; repaired.len()];
        paint(
            &layered,
            (0., 0.),
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .unwrap();
        assert_eq!(
            pixels.as_chunks::<4>().0.iter().map(|p| p[3]).max(),
            Some(192),
            "each arc must clear temporary pixels before source-over compositing"
        );
    }
}

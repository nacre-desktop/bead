// SPDX-License-Identifier: MIT OR Apache-2.0
//! Semantic surface damage, independent of back-buffer repaint history.
use crate::{
    Damage, Rect,
    tree::{Frame, rect},
};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
};
#[derive(Clone)]
struct Record {
    key: u64,
    bounds: Damage,
    geometry: Rect,
    clip: Option<Rect>,
    opacity: u32,
}
#[derive(Default)]
pub struct FrameDamage {
    previous: BTreeMap<String, Record>,
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
impl FrameDamage {
    pub fn update(
        &mut self,
        frame: &Frame,
        offset: (f32, f32),
        opacity: f32,
        width: u32,
        height: u32,
    ) -> crate::Result<Vec<Damage>> {
        let mut current = BTreeMap::new();
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
            let pad = if let crate::Primitive::Rounded { border, .. } = item.primitive {
                border.map_or(1., |(_, w)| w / 2. + 1.)
            } else {
                0.
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
                serde_json::to_vec(&item.primitive)?.hash(&mut h);
                h.finish()
            };
            let next = Record {
                key,
                bounds,
                geometry,
                clip,
                opacity: opacity.to_bits(),
            };
            match self.previous.get(&item.id) {
                None => add(&mut dirty, bounds),
                Some(old)
                    if old.key != key
                        || old.geometry != geometry
                        || old.clip != clip
                        || old.opacity != next.opacity =>
                {
                    if let Some(raster) = &item.raster
                        && raster.base_key == Some(old.key)
                        && old.geometry == geometry
                        && old.clip == clip
                        && old.opacity == next.opacity
                    {
                        let d = raster.changed;
                        let mut changed = Rect::new(
                            geometry.x + d.x as f32 / frame.scale,
                            geometry.y + d.y as f32 / frame.scale,
                            d.w as f32 / frame.scale,
                            d.h as f32 / frame.scale,
                        )
                        .pixels(frame.scale, width, height);
                        if let Some(c) = clip {
                            changed = intersect(changed, c.pixels(frame.scale, width, height));
                        }
                        add(&mut dirty, changed);
                    } else {
                        add(&mut dirty, old.bounds);
                        add(&mut dirty, bounds);
                    }
                }
                _ => {}
            }
            current.insert(item.id.clone(), next);
        }
        for (id, old) in &self.previous {
            if !current.contains_key(id) {
                add(&mut dirty, old.bounds);
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
}

// SPDX-License-Identifier: MIT OR Apache-2.0
//! Exact compact storage for large rasters with large uniform areas.
use std::{
    collections::{HashMap, hash_map::Entry},
    mem::size_of,
};

use tiny_skia::{Pixmap, PremultipliedColorU8};

const TILE: u32 = 32;
const GUTTER: u32 = 1;
const MAX_ATLAS_WIDTH: u32 = 4096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CompactRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

#[derive(Clone, Debug)]
// Only the GPU upload paths read the region geometry.
#[cfg_attr(not(any(feature = "native-gl", feature = "wgpu")), allow(dead_code))]
pub(crate) struct CompactRegion {
    /// Source pixels in the original raster.
    pub(crate) source: CompactRect,
    /// Corresponding inner pixels in `CompactRaster::atlas`; gutters surround it.
    pub(crate) atlas: CompactRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompactFill {
    pub(crate) rect: CompactRect,
    /// Premultiplied RGBA, matching tiny-skia's Pixmap storage.
    pub(crate) color: [u8; 4],
    /// A 1×1 exact-color texel in the atlas; its surrounding gutter is replicated.
    pub(crate) atlas: CompactRect,
}

#[derive(Clone, Debug)]
pub(crate) struct CompactRaster {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) atlas: Pixmap,
    pub(crate) regions: Vec<CompactRegion>,
    pub(crate) fills: Vec<CompactFill>,
}

type FillRuns = HashMap<(u32, u32, [u8; 4]), usize>;

#[derive(Default)]
struct Shelf {
    x: u32,
    y: u32,
    row_height: u32,
    width: u32,
}

impl Shelf {
    fn place(&mut self, width: u32, height: u32) -> Option<CompactRect> {
        let padded_width = width.checked_add(2 * GUTTER)?;
        let padded_height = height.checked_add(2 * GUTTER)?;
        if padded_width > MAX_ATLAS_WIDTH {
            return None;
        }
        if self.x + padded_width > MAX_ATLAS_WIDTH {
            self.y = self.y.checked_add(self.row_height)?;
            self.x = 0;
            self.row_height = 0;
        }
        let inner = CompactRect {
            x: self.x + GUTTER,
            y: self.y + GUTTER,
            w: width,
            h: height,
        };
        self.x += padded_width;
        self.row_height = self.row_height.max(padded_height);
        self.width = self.width.max(self.x);
        Some(inner)
    }

    fn size(&self) -> (u32, u32) {
        if self.width == 0 {
            (1, 1)
        } else {
            (self.width, self.y + self.row_height)
        }
    }
}

impl CompactRaster {
    /// Packs only nonuniform 32×32 tiles, preserving all source bytes exactly.
    /// `None` means the atlas plus its metadata would not save memory.
    pub(crate) fn from_pixmap(source: &Pixmap) -> Option<Self> {
        let (width, height) = (source.width(), source.height());
        if width == 0 || height == 0 {
            return None;
        }
        let columns = width.div_ceil(TILE);
        let rows = height.div_ceil(TILE);
        let mut fills = Vec::new();
        let mut literals = Vec::new();
        let mut previous = FillRuns::new();
        for ty in 0..rows {
            let mut current = FillRuns::new();
            let mut run: Option<(CompactRect, [u8; 4])> = None;
            for tx in 0..columns {
                let rect = CompactRect {
                    x: tx * TILE,
                    y: ty * TILE,
                    w: TILE.min(width - tx * TILE),
                    h: TILE.min(height - ty * TILE),
                };
                match uniform_tile(source, rect) {
                    Some(color) => {
                        if let Some((previous_rect, previous_color)) = &mut run
                            && *previous_color == color
                        {
                            previous_rect.w += rect.w;
                            continue;
                        }
                        if let Some((rect, color)) = run.take() {
                            add_fill(rect, color, &mut fills, &previous, &mut current);
                        }
                        run = Some((rect, color));
                    }
                    None => {
                        if let Some((rect, color)) = run.take() {
                            add_fill(rect, color, &mut fills, &previous, &mut current);
                        }
                        literals.push(rect);
                    }
                }
            }
            if let Some((rect, color)) = run {
                add_fill(rect, color, &mut fills, &previous, &mut current);
            }
            previous = current;
        }

        let mut placements = Vec::new();
        placements.try_reserve_exact(literals.len()).ok()?;
        let mut shelf = Shelf::default();
        for rect in literals {
            let atlas = shelf.place(rect.w, rect.h)?;
            placements.push((rect, atlas));
        }
        let mut fill_atlas = HashMap::new();
        for fill in &fills {
            if let Entry::Vacant(entry) = fill_atlas.entry(fill.color) {
                entry.insert(shelf.place(1, 1)?);
            }
        }
        let (atlas_w, atlas_h) = shelf.size();
        let atlas_bytes = (atlas_w as usize)
            .checked_mul(atlas_h as usize)?
            .checked_mul(4)?;
        let region_bytes = placements.len().checked_mul(size_of::<CompactRegion>())?;
        let fill_bytes = fills.capacity().checked_mul(size_of::<CompactFill>())?;
        if atlas_bytes
            .checked_add(region_bytes)?
            .checked_add(fill_bytes)?
            .checked_add(size_of::<Self>())?
            >= source.data().len()
        {
            return None;
        }

        let mut atlas = Pixmap::new(atlas_w, atlas_h)?;
        let mut regions = Vec::with_capacity(placements.len());
        for (rect, destination) in placements {
            copy_tile_with_gutter(source, &mut atlas, rect, destination);
            regions.push(CompactRegion {
                source: rect,
                atlas: destination,
            });
        }
        for (&color, &destination) in &fill_atlas {
            fill_color_with_gutter(&mut atlas, destination, color);
        }
        for fill in &mut fills {
            fill.atlas = fill_atlas[&fill.color];
        }
        let compact = Self {
            width,
            height,
            atlas,
            regions,
            fills,
        };
        (compact.retained_bytes() < source.data().len()).then_some(compact)
    }

    /// Retained bytes, including the atlas gutter pixels and vector capacities.
    pub(crate) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(self.atlas.data().len())
            .saturating_add(
                self.regions
                    .capacity()
                    .saturating_mul(size_of::<CompactRegion>()),
            )
            .saturating_add(
                self.fills
                    .capacity()
                    .saturating_mul(size_of::<CompactFill>()),
            )
    }
}

fn uniform_tile(source: &Pixmap, rect: CompactRect) -> Option<[u8; 4]> {
    let pixels = source.pixels();
    let first = rgba(pixels[rect.y as usize * source.width() as usize + rect.x as usize]);
    (0..rect.h)
        .all(|dy| {
            let start = (rect.y + dy) as usize * source.width() as usize + rect.x as usize;
            pixels[start..start + rect.w as usize]
                .iter()
                .all(|&pixel| rgba(pixel) == first)
        })
        .then_some(first)
}

fn add_fill(
    rect: CompactRect,
    color: [u8; 4],
    fills: &mut Vec<CompactFill>,
    previous: &FillRuns,
    current: &mut FillRuns,
) {
    if color[3] == 0 {
        return;
    }
    let key = (rect.x, rect.w, color);
    let index = if let Some(&index) = previous.get(&key)
        && fills[index].rect.y + fills[index].rect.h == rect.y
    {
        fills[index].rect.h += rect.h;
        index
    } else {
        fills.push(CompactFill {
            rect,
            color,
            atlas: CompactRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        });
        fills.len() - 1
    };
    current.insert(key, index);
}

fn rgba(pixel: PremultipliedColorU8) -> [u8; 4] {
    [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
}

fn copy_tile_with_gutter(
    source: &Pixmap,
    atlas: &mut Pixmap,
    rect: CompactRect,
    destination: CompactRect,
) {
    let padded_x = destination.x - GUTTER;
    let padded_y = destination.y - GUTTER;
    for py in 0..rect.h + 2 * GUTTER {
        let sy = rect.y + py.saturating_sub(GUTTER).min(rect.h - 1);
        for px in 0..rect.w + 2 * GUTTER {
            let sx = rect.x + px.saturating_sub(GUTTER).min(rect.w - 1);
            let source_offset = (sy as usize * source.width() as usize + sx as usize) * 4;
            let atlas_offset =
                ((padded_y + py) as usize * atlas.width() as usize + (padded_x + px) as usize) * 4;
            atlas.data_mut()[atlas_offset..atlas_offset + 4]
                .copy_from_slice(&source.data()[source_offset..source_offset + 4]);
        }
    }
}

fn fill_color_with_gutter(atlas: &mut Pixmap, destination: CompactRect, color: [u8; 4]) {
    let x = destination.x - GUTTER;
    let y = destination.y - GUTTER;
    for dy in 0..3 {
        for dx in 0..3 {
            let offset = ((y + dy) as usize * atlas.width() as usize + (x + dx) as usize) * 4;
            atlas.data_mut()[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(pixmap: &mut Pixmap, x: u32, y: u32, color: [u8; 4]) {
        let offset = ((y * pixmap.width() + x) * 4) as usize;
        pixmap.data_mut()[offset..offset + 4].copy_from_slice(&color);
    }

    fn reconstruct(compact: &CompactRaster) -> Pixmap {
        let mut result = Pixmap::new(compact.width, compact.height).unwrap();
        for fill in &compact.fills {
            for y in fill.rect.y..fill.rect.y + fill.rect.h {
                for x in fill.rect.x..fill.rect.x + fill.rect.w {
                    set(&mut result, x, y, fill.color);
                }
            }
        }
        for region in &compact.regions {
            for y in 0..region.source.h {
                let source_start =
                    (((region.atlas.y + y) * compact.atlas.width() + region.atlas.x) * 4) as usize;
                let destination_start =
                    (((region.source.y + y) * compact.width + region.source.x) * 4) as usize;
                let length = region.source.w as usize * 4;
                result.data_mut()[destination_start..destination_start + length]
                    .copy_from_slice(&compact.atlas.data()[source_start..source_start + length]);
            }
        }
        result
    }

    #[test]
    fn reconstruction_is_byte_exact_across_uniform_transparent_and_edge_tiles() {
        let mut source = Pixmap::new(79, 67).unwrap();
        source.data_mut().fill(0);
        for y in 0..67 {
            for x in 0..79 {
                if (32..64).contains(&x) && y >= 32 {
                    let a = 128;
                    set(&mut source, x, y, [x as u8 / 2, y as u8 / 2, 20, a]);
                } else if x >= 64 {
                    set(&mut source, x, y, [30, 20, 10, 255]);
                }
            }
        }
        let compact = CompactRaster::from_pixmap(&source).unwrap();
        assert_eq!(reconstruct(&compact).data(), source.data());
        assert!(compact.regions.iter().all(|region| {
            region.atlas.w == region.source.w && region.atlas.h == region.source.h
        }));
        let region = &compact.regions[0];
        let source_top_left = ((region.source.y * source.width() + region.source.x) * 4) as usize;
        let atlas_top_left =
            (((region.atlas.y - 1) * compact.atlas.width() + region.atlas.x - 1) * 4) as usize;
        assert_eq!(
            &compact.atlas.data()[atlas_top_left..atlas_top_left + 4],
            &source.data()[source_top_left..source_top_left + 4]
        );
        assert!(
            compact
                .fills
                .iter()
                .any(|fill| fill.color == [30, 20, 10, 255])
        );
        assert!(!compact.fills.iter().any(|fill| fill.color == [0, 0, 0, 0]));
        assert!(compact.fills.iter().all(|fill| {
            fill.atlas.w == 1
                && fill.atlas.h == 1
                && rgba(
                    compact.atlas.pixels()
                        [(fill.atlas.y * compact.atlas.width() + fill.atlas.x) as usize],
                ) == fill.color
        }));
    }

    #[test]
    fn dense_small_and_noisy_inputs_fall_back() {
        let mut tiny = Pixmap::new(2, 2).unwrap();
        tiny.data_mut()
            .copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 1, 2, 3, 255]);
        assert!(CompactRaster::from_pixmap(&tiny).is_none());

        let mut noise = Pixmap::new(128, 128).unwrap();
        for (i, pixel) in noise.pixels_mut().iter_mut().enumerate() {
            *pixel = PremultipliedColorU8::from_rgba(
                (i as u8) & 0x3f,
                ((i >> 2) as u8) & 0x3f,
                ((i >> 4) as u8) & 0x3f,
                255,
            )
            .unwrap();
        }
        assert!(CompactRaster::from_pixmap(&noise).is_none());
    }

    #[test]
    fn large_uniform_raster_merges_to_one_fill_and_accounts_gutters() {
        let mut source = Pixmap::new(512, 512).unwrap();
        source.data_mut().fill(0);
        for pixel in source.pixels_mut() {
            *pixel = PremultipliedColorU8::from_rgba(40, 50, 60, 255).unwrap();
        }
        let compact = CompactRaster::from_pixmap(&source).unwrap();
        assert_eq!(compact.fills.len(), 1);
        assert!(compact.regions.is_empty());
        assert_eq!((compact.atlas.width(), compact.atlas.height()), (3, 3));
        assert!(compact.retained_bytes() < source.data().len());
        assert_eq!(
            compact.retained_bytes(),
            size_of::<CompactRaster>()
                + compact.atlas.data().len()
                + compact.regions.capacity() * size_of::<CompactRegion>()
                + compact.fills.capacity() * size_of::<CompactFill>()
        );
        assert_eq!(reconstruct(&compact).data(), source.data());
    }

    #[test]
    fn repeated_fill_colors_share_one_atlas_patch() {
        let mut source = Pixmap::new(96, 32).unwrap();
        for y in 0..32 {
            for x in 0..96 {
                set(
                    &mut source,
                    x,
                    y,
                    if (32..64).contains(&x) {
                        [20, 40, 10, 255]
                    } else {
                        [40, 20, 10, 255]
                    },
                );
            }
        }
        let compact = CompactRaster::from_pixmap(&source).unwrap();
        assert_eq!(compact.fills.len(), 3);
        assert_eq!(compact.fills[0].atlas, compact.fills[2].atlas);
        assert_ne!(compact.fills[0].atlas, compact.fills[1].atlas);
        assert_eq!(reconstruct(&compact).data(), source.data());
    }
}

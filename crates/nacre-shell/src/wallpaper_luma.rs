// SPDX-License-Identifier: GPL-3.0-only
//! B0193: wallpaper luminance for adaptive widget contrast. The grid is computed once per
//! decoded wallpaper (in the decode path that already scales it, never per frame), published
//! when the wallpaper is committed, and handed to the widget layer as the
//! `bead.desktop.wallpaper.luma` store. Nacre knowledge stays here; Luau picks palette roles.
use bead::core::Value;
use nacre_protocol::wallpaper::ActiveWallpaper;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

pub(crate) const COLS: usize = 16;
pub(crate) const ROWS: usize = 9;
pub(crate) const STORE: &str = "bead.desktop.wallpaper.luma";
/// Sample every STRIDE-th pixel in both directions: ~1 ms for a 5 Mpx wallpaper.
const STRIDE: usize = 4;

/// Mean WCAG relative luminance (0 black .. 1 white) per grid cell, plus the luminance of the
/// palette's `onSurface` and `surface` roles so widgets can pick the better ink.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Luma {
    pub on_surface: f32,
    pub surface: f32,
    pub cells: Vec<f32>,
}

fn linear() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f32 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

fn relative(r: u8, g: u8, b: u8) -> f32 {
    let lut = linear();
    0.2126 * lut[r as usize] + 0.7152 * lut[g as usize] + 0.0722 * lut[b as usize]
}

fn role(colours: &BTreeMap<String, String>, name: &str, fallback: f32) -> f32 {
    colours
        .get(name)
        .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
        .map_or(fallback, |rgb| {
            relative((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        })
}

/// `rgba` is `width * height` RGBA pixels as decoded for the output.
pub(crate) fn measure(pair: &ActiveWallpaper, rgba: &[u8], width: u32, height: u32) -> Luma {
    let colours = &pair.palette.colours;
    Luma {
        on_surface: role(colours, "onSurface", 0.8),
        surface: role(colours, "surface", 0.02),
        cells: grid(rgba, width, height),
    }
}

fn grid(rgba: &[u8], width: u32, height: u32) -> Vec<f32> {
    let (width, height) = (width as usize, height as usize);
    let mut sum = [0f32; COLS * ROWS];
    let mut count = [0u32; COLS * ROWS];
    if rgba.len() >= width * height * 4 {
        for y in (0..height).step_by(STRIDE) {
            let row = (y * ROWS / height) * COLS;
            for x in (0..width).step_by(STRIDE) {
                let at = (y * width + x) * 4;
                let cell = row + x * COLS / width;
                sum[cell] += relative(rgba[at], rgba[at + 1], rgba[at + 2]);
                count[cell] += 1;
            }
        }
    }
    sum.iter()
        .zip(count)
        .map(|(sum, count)| if count == 0 { 0.5 } else { sum / count as f32 })
        .collect()
}

impl Luma {
    /// One record `{cols, rows, on_surface, surface, cells}` (store lists hold objects),
    /// three decimals each.
    fn store_value(&self) -> Value {
        let round = |v: f32| serde_json::json!((f64::from(v) * 1000.).round() / 1000.);
        Value::list(vec![serde_json::json!({
            "cols": COLS,
            "rows": ROWS,
            "on_surface": round(self.on_surface),
            "surface": round(self.surface),
            "cells": self.cells.iter().map(|v| round(*v)).collect::<Vec<_>>(),
        })])
    }
}

type Watcher = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct State {
    committed: BTreeMap<String, Arc<Luma>>,
    watchers: Vec<(u64, Watcher)>,
    next: u64,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}

/// The wallpaper of `output` is now on screen. Watchers run only when the luminance changed.
pub(crate) fn publish(output: &str, luma: Arc<Luma>) {
    let watchers: Vec<Watcher> = {
        let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
        if state.committed.get(output).is_some_and(|old| **old == *luma) {
            return;
        }
        state.committed.insert(output.to_owned(), luma);
        state.watchers.iter().map(|(_, w)| w.clone()).collect()
    };
    for watcher in watchers {
        watcher();
    }
}

pub(crate) fn remove(output: &str) {
    state()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .committed
        .remove(output);
}

/// Store values for one output; empty until its first wallpaper is committed.
pub(crate) fn values(output: Option<&str>) -> BTreeMap<String, Value> {
    let state = state().lock().unwrap_or_else(|p| p.into_inner());
    output
        .and_then(|output| state.committed.get(output))
        .map(|luma| BTreeMap::from([(STORE.to_owned(), luma.store_value())]))
        .unwrap_or_default()
}

/// Call `changed` (from the committing thread) after any output's luminance changed.
pub(crate) struct Watch(u64);
pub(crate) fn watch(changed: Watcher) -> Watch {
    let mut state = state().lock().unwrap_or_else(|p| p.into_inner());
    state.next += 1;
    let id = state.next;
    state.watchers.push((id, changed));
    Watch(id)
}
impl Drop for Watch {
    fn drop(&mut self) {
        state()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .watchers
            .retain(|(id, _)| *id != self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_follows_the_pixels_and_roles_come_from_the_palette() {
        let (w, h) = (64usize, 36usize);
        let mut rgba = vec![0u8; w * h * 4];
        for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
            // left half white, right half black
            let v = if i % w < w / 2 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let cells = grid(&rgba, w as u32, h as u32);
        assert_eq!(cells.len(), COLS * ROWS);
        assert!(cells[0] > 0.99, "{}", cells[0]);
        assert!(cells[COLS - 1] < 0.01);
        let colours = BTreeMap::from([("onSurface".to_owned(), "#ffffff".to_owned())]);
        assert!((role(&colours, "onSurface", 0.) - 1.).abs() < 1e-3);
        assert_eq!(role(&colours, "surface", 0.25), 0.25);
        let Value::List(row) = (Luma { on_surface: 1., surface: 0., cells }).store_value() else {
            panic!("list")
        };
        assert_eq!(row[0]["cells"].as_array().map(Vec::len), Some(COLS * ROWS));
    }
}

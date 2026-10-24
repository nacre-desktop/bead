// SPDX-License-Identifier: MIT OR Apache-2.0
//! Replaceable typed views and bounded Luau behavior over durable Rust state.
pub mod accessibility;
pub mod frontend;
mod generation;
pub mod i18n;
pub mod input;
mod interaction;
pub mod layout;
pub mod markdown;
mod materialized;
pub mod recovery;
pub mod schema;
pub mod script;
mod session;
pub mod status_pill;
pub mod text_style;
mod themed;
pub mod transcript;
mod watcher;
use bead_core::{Stores, Value};
use bead_render::{Palette, Primitive, Rect, Scene};
pub use generation::Generation;
use serde::Deserialize;
pub use session::Session;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
}
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file, self.line, self.column, self.message
        )
    }
}
impl std::error::Error for Diagnostic {}
impl Diagnostic {
    pub fn new(path: &Path, line: usize, column: usize, message: impl ToString) -> Self {
        Self {
            file: path.to_string_lossy().into_owned(),
            line,
            column,
            message: message.to_string(),
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    #[serde(skip)]
    pub source: Option<(PathBuf, usize, usize)>,
    pub draw: Primitive,
    #[serde(default)]
    pub bind: BTreeMap<String, String>,
    pub visible: Option<String>,
    pub action: Option<String>,
    #[serde(default)]
    pub activate_on_release: bool,
    #[serde(default = "default_true")]
    pub focus_on_pointer: bool,
    #[serde(default = "default_true")]
    pub focus_ring: bool,
    /// Transient numeric feedback, independent of an asynchronous action result.
    #[serde(default)]
    pub preview_key: String,
    #[serde(default)]
    pub preview_inset: f32,
    #[serde(default)]
    pub preview_value: f32,
    /// Authoritative value and acknowledgement tolerance for latched range feedback.
    #[serde(default)]
    pub preview_source: Option<f32>,
    #[serde(default)]
    pub preview_expected: Option<f32>,
    #[serde(default)]
    pub preview_tolerance: f32,
    /// Pointer argument evaluated by the view from its continuous local feedback.
    #[serde(default)]
    pub pointer_arg: Option<Value>,
    /// Allow this interactive region to participate in configured edge hover.
    #[serde(default)]
    pub edge_hover: bool,
    pub icon: Option<String>,
    pub input: Option<String>,
    pub scroll: Option<String>,
    #[serde(default)]
    pub action_arg: Option<Value>,
    #[serde(default)]
    pub scroll_store: Option<String>,
    /// Content extent minus viewport, inherited by scrollable descendants.
    #[serde(skip)]
    pub scroll_limit: Option<f64>,
    #[serde(default)]
    pub focusable: bool,
    #[serde(default)]
    pub multiline: bool,
    /// TextInput shows one bullet per character and never exposes its value.
    #[serde(default)]
    pub masked: bool,
    #[serde(default)]
    pub drag: bool,
    #[serde(skip)]
    pub clip: Option<Rect>,
    #[serde(default)]
    pub hit_polygon: Option<Vec<[f32; 2]>>,
    #[serde(default)]
    pub label: String,
}
fn default_true() -> bool {
    true
}
impl Node {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.contains_in(rect(&self.draw), x, y)
    }
    pub fn contains_in(&self, r: Rect, x: f32, y: f32) -> bool {
        let inside_rect = |r: Rect| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;
        if !inside_rect(r) || self.clip.is_some_and(|clip| !inside_rect(clip)) {
            return false;
        }
        let Some(points) = &self.hit_polygon else {
            return true;
        };
        if points.len() < 3 {
            return false;
        }
        let (px, py) = ((x - r.x) / r.w, (y - r.y) / r.h);
        let mut inside = false;
        let mut previous = points.len() - 1;
        for current in 0..points.len() {
            let [xi, yi] = points[current];
            let [xj, yj] = points[previous];
            if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            previous = current;
        }
        inside
    }
    fn diagnostic(&self, message: impl ToString) -> Diagnostic {
        let (path, line, column) =
            self.source
                .clone()
                .unwrap_or((PathBuf::from("ui/main.json"), 1, 1));
        Diagnostic::new(&path, line, column, message)
    }
}
pub fn rect(draw: &Primitive) -> Rect {
    match draw {
        Primitive::Rounded { rect, .. }
        | Primitive::Arc { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => *rect,
    }
}
fn rect_mut(draw: &mut Primitive) -> &mut Rect {
    match draw {
        Primitive::Rounded { rect, .. }
        | Primitive::Arc { rect, .. }
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => rect,
    }
}
fn valid_rect(r: Rect) -> bool {
    [r.x, r.y, r.w, r.h]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 20000.)
        && r.w > 0.
        && r.h > 0.
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod markup_tests;

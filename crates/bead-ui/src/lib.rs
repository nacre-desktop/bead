// SPDX-License-Identifier: MIT OR Apache-2.0
//! Replaceable typed views and bounded Luau behavior over durable Rust state.
pub mod accessibility;
pub mod frontend;
mod generation;
pub mod input;
mod interaction;
pub mod layout;
pub mod schema;
pub mod script;
mod session;
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
    pub icon: Option<String>,
    pub input: Option<String>,
    pub scroll: Option<String>,
    #[serde(default)]
    pub action_arg: Option<Value>,
    #[serde(default)]
    pub scroll_store: Option<String>,
    #[serde(default)]
    pub focusable: bool,
    #[serde(default)]
    pub drag: bool,
    #[serde(skip)]
    pub clip: Option<Rect>,
    #[serde(default)]
    pub label: String,
}
impl Node {
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
        | Primitive::Text { rect, .. }
        | Primitive::Image { rect, .. }
        | Primitive::Shadow { rect, .. } => *rect,
    }
}
fn rect_mut(draw: &mut Primitive) -> &mut Rect {
    match draw {
        Primitive::Rounded { rect, .. }
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

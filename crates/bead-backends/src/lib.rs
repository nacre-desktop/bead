// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor-neutral snapshots/actions. Native transports remain in GPL adapters.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Id(pub String);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub id: Id,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub refresh_mhz: u32,
    pub x: i32,
    pub y: i32,
    pub focused: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Id,
    pub name: String,
    pub index: i64,
    pub output: Option<Id>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    /// A workspace the compositor keeps by name (niri named workspace), not a spare.
    #[serde(default)]
    pub fixed_slot: bool,
    /// Shown on an output other than its home output. Adapters that cannot tell report false.
    #[serde(default)]
    pub visiting: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub id: Id,
    pub title: String,
    pub app_id: String,
    pub workspace: Option<Id>,
    pub output: Option<Id>,
    pub focused: bool,
    pub floating: bool,
    /// None when the backend does not expose fullscreen state.
    pub fullscreen: Option<bool>,
    pub urgent: bool,
}
/// Names match Nacre's public compositor capability vocabulary.
pub const CAPABILITIES: &[&str] = &[
    "stableWorkspaceIndices",
    "globalWorkspaces",
    "perOutputWorkspaces",
    "scrollingLayout",
    "floatingPlacement",
    "pinWindow",
    "windowCapture",
    "nativeWindowScreenshot",
    "shapedBackgroundBlur",
    "cursorPosition",
    "nativePanelDrag",
    "windowHide",
    "windowTrash",
    "scratchpad",
    "windowGroups",
    "windowForceKill",
    "pseudoTiling",
    "layoutSplit",
    "keymaps",
    "nonConsumingShortcuts",
    "bindingIntrospection",
    "fullscreenState",
    "shortcutRelease",
    "monitorConfiguration",
    "monitorMirroring",
    "nightLightController",
];
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub features: BTreeMap<String, bool>,
    #[serde(rename = "desktopOptions")]
    pub desktop_options: Vec<String>,
}
impl Capabilities {
    pub fn empty() -> Self {
        Self {
            features: CAPABILITIES
                .iter()
                .map(|n| ((*n).to_owned(), false))
                .collect(),
            desktop_options: vec![],
        }
    }
    pub fn supports(&self, name: &str) -> bool {
        self.features.get(name) == Some(&true)
    }
}
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub backend: String,
    pub epoch: u64,
    pub connected: bool,
    pub outputs: Vec<Output>,
    pub workspaces: Vec<Workspace>,
    pub windows: Vec<Window>,
    pub capabilities: Capabilities,
}
impl Snapshot {
    pub fn focused_window(&self) -> Option<&Window> {
        self.windows.iter().find(|w| w.focused)
    }
    pub fn focused_workspace(&self) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.focused)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Action {
    FocusWindow {
        window: Id,
    },
    CloseWindow {
        window: Id,
    },
    SwitchWorkspace {
        workspace: Id,
    },
    MoveWindow {
        window: Id,
        workspace: Id,
        follow: bool,
    },
    MoveWindowToOutput {
        window: Id,
        output: Id,
        follow: bool,
    },
    FocusOutput {
        output: Id,
    },
    FocusDirection {
        direction: Direction,
    },
    SetFloating {
        window: Id,
        floating: bool,
    },
    SetFullscreen {
        window: Id,
        fullscreen: bool,
    },
    WindowTrash {
        window: Id,
    },
    Scratchpad {
        window: Option<Id>,
    },
    WindowCapture {
        window: Id,
    },
    MonitorMirroring {
        source: Id,
        target: Id,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Unsupported { operation: String, backend: String },
    Disconnected(String),
    UnknownId(String),
    Protocol(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { operation, backend } => {
                write!(f, "unsupported {operation} on {backend}")
            }
            Self::Disconnected(s) => write!(f, "disconnected: {s}"),
            Self::UnknownId(s) => write!(f, "unknown/stale ID {s}"),
            Self::Protocol(s) => write!(f, "compositor protocol: {s}"),
        }
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Debug)]
pub enum Event {
    Snapshot(Arc<Snapshot>),
    Disconnected(String),
    Reconnecting { attempt: u32 },
    Error(Error),
}
pub trait Backend: Send + Sync {
    fn snapshot(&self) -> Arc<Snapshot>;
    fn action(&self, action: Action) -> Result<(), Error>;
    fn subscribe(&self, callback: Box<dyn Fn(Event) + Send + Sync>) -> Subscription;
}
/// Dropping a subscription unregisters it. Views never own compositor transport.
pub struct Subscription {
    cancel: Option<Box<dyn FnOnce() + Send + Sync>>,
}
impl Subscription {
    pub fn new(cancel: impl FnOnce() + Send + Sync + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}
pub fn stable_id(backend: &str, epoch: u64, kind: &str, native: impl fmt::Display) -> Id {
    Id(format!("{backend}:{epoch}:{kind}:{native}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_indices_are_labels_not_identity() {
        let a = stable_id("niri", 3, "workspace", 12);
        let b = stable_id("niri", 3, "workspace", 13);
        assert_ne!(a, b);
        assert_ne!(a, stable_id("niri", 4, "workspace", 12));
    }
    #[test]
    fn unsupported_is_an_error_not_an_empty_success() {
        assert_eq!(
            Error::Unsupported {
                operation: "windowTrash".into(),
                backend: "niri".into()
            }
            .to_string(),
            "unsupported windowTrash on niri"
        );
        assert!(!Capabilities::empty().supports("windowTrash"));
    }
}

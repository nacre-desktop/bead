// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reusable shell facade. No application identity, palette engine or GPL adapters.
pub use bead_backends as backends;
pub use bead_core as core;
pub use bead_render as render;
pub use bead_services as services;
pub use bead_ui as ui;
pub use bead_wayland as wayland;

pub mod source;

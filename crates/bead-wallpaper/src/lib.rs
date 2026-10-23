// SPDX-License-Identifier: MIT OR Apache-2.0
//! Product-neutral motion-wallpaper primitives: the wire protocol of a separate wallpaper
//! renderer process, the pause/release policy state machine, and a crash-tolerant
//! supervisor for that process. The renderer itself (decoder + Wayland surfaces) is a
//! binary in the product crate; nothing here names a product, a setting or a daemon.
pub mod policy;
pub mod protocol;
pub mod supervisor;

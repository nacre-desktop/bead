// SPDX-License-Identifier: MIT
//! Embedded, reusable Luau kit and its generic default size scale.

/// Framework defaults for an application that does not provide its own scale.
pub const DEFAULT_SCALE: &str = include_str!("../assets/size-scale.json");

/// Source for the standard reusable Luau kit.
pub const fn source() -> &'static str {
    include_str!("../assets/kit.luau")
}

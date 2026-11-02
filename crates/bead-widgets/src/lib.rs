// SPDX-License-Identifier: MIT
//! Generic widget modules and persistence primitives shared by host applications.

mod layout_store;

/// Return an embedded generic Luau widget module by its stable suffix.
pub fn source(name: &str) -> Option<&'static str> {
    match name {
        "measure" => Some(include_str!("luau/measure.luau")),
        "settings-form" => Some(include_str!("luau/settings-form.luau")),
        "forms" => Some(include_str!("luau/forms.luau")),
        "placement" => Some(include_str!("luau/placement.luau")),
        "drag-geometry" => Some(include_str!("luau/drag-geometry.luau")),
        "drag" => Some(include_str!("luau/drag.luau")),
        "kit.virtual-list" => Some(include_str!("luau/kit-virtual-list.luau")),
        _ => None,
    }
}

pub use layout_store::{LayoutStore, atomic_replace};

/// Modules with no dependency on a view SDK can also run in behavior scripts.
pub fn behavior_source(name: &str) -> Option<&'static str> {
    match name {
        "drag-geometry" | "drag" | "placement" | "forms" => source(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::source;

    #[test]
    fn embedded_module_names_are_stable_and_resolvable() {
        for name in [
            "measure",
            "settings-form",
            "forms",
            "placement",
            "drag-geometry",
            "drag",
            "kit.virtual-list",
        ] {
            assert!(source(name).is_some(), "missing embedded module {name}");
        }
        assert!(source("unknown-app-module").is_none());
    }
}

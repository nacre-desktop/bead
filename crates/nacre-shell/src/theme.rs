// SPDX-License-Identifier: GPL-3.0-only
//! Nacre's theme values at the framework boundary.
use serde_json::json;

pub(crate) fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| apply(&crate::prefs::Prefs::default()));
}

pub(crate) fn apply(prefs: &crate::prefs::Prefs) {
    let mut tokens: serde_json::Value =
        serde_json::from_str(include_str!("../../../shell/size-scale.json"))
            .expect("Nacre size scale is valid JSON");
    // Keep authored family names identical to the original product. User font
    // choices still flow through TextStyle aliases; static fallback scenes keep
    // their original faces rather than gaining a second alias layer here.
    tokens["fonts"] = json!({
        "sans": crate::prefs::INTERFACE_FONT,
        "mono": crate::prefs::MONO_FONT,
        "icon": "Material Symbols Rounded",
    });
    tokens["gradients"] = prefs.interface_gradients.into();
    bead::ui::theme::set(tokens).expect("Nacre theme tokens match the framework schema");
    let mut style = prefs.text_style();
    style.fixed_size_prefixes.push("Material Symbols".into());
    let _ = bead::ui::text_style::set(style);
}

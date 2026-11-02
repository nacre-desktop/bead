// SPDX-License-Identifier: MIT
//! Host-supplied text presentation: family aliases and a size offset, applied while
//! views are laid out. Markup keeps naming its families literally; a host that lets
//! the owner pick fonts maps those names to the chosen ones and every later layout
//! uses them. Icon families keep their size. A change reloads every view through the
//! same listeners as a catalog change; the old family's faces are dropped with the
//! scene's font set, so nothing of it stays resident.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    /// Markup family name to the family drawn instead.
    pub aliases: BTreeMap<String, String>,
    /// Families whose sizes are supplied by the host unchanged (usually icon fonts).
    pub fixed_size_families: BTreeSet<String>,
    /// Family prefixes with the same size exception, for a host's related icon family set.
    pub fixed_size_prefixes: Vec<String>,
    /// Added to every non-icon font size, in the markup's size unit.
    pub size_offset: f32,
    /// Solid colours: the owner turned interface gradients off (`tokens.gradients` is false in the kit).
    pub solid: bool,
}
impl TextStyle {
    fn family(&self, name: &str) -> String {
        self.aliases
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    }
    fn size(&self, base: f32, family: &str) -> f32 {
        if self.fixed_size_families.contains(family)
            || self
                .fixed_size_prefixes
                .iter()
                .any(|prefix| family.starts_with(prefix))
        {
            base
        } else {
            (base + self.size_offset).clamp(1., 256.)
        }
    }
}
static STYLE: Mutex<Option<TextStyle>> = Mutex::new(None);

/// Replace the style; views reload only when it changed (returned).
pub fn set(style: TextStyle) -> bool {
    let changed = {
        let mut held = STYLE.lock().unwrap_or_else(|p| p.into_inner());
        let old = held.take().unwrap_or_default();
        let changed = old != style;
        *held = (style != TextStyle::default()).then_some(style);
        changed
    };
    if changed {
        crate::i18n::refresh();
    }
    changed
}
/// Rebuild every view now, for a host presentation input read while building
/// scenes (a decoration colour or toggle) that changed.
pub fn refresh() {
    crate::i18n::refresh();
}
/// Whether gradient fills are drawn (true until the host asks for solid colours).
pub fn gradients() -> bool {
    let held = STYLE.lock().unwrap_or_else(|p| p.into_inner());
    !held.as_ref().is_some_and(|s| s.solid)
}
/// The family to draw for a markup family name.
pub fn family(name: &str) -> String {
    let held = STYLE.lock().unwrap_or_else(|p| p.into_inner());
    held.as_ref()
        .map_or_else(|| name.to_owned(), |s| s.family(name))
}
/// The size to draw for a family, clamped to the markup range.
pub fn size(base: f32, family: &str) -> f32 {
    let held = STYLE.lock().unwrap_or_else(|p| p.into_inner());
    held.as_ref().map_or(base, |s| s.size(base, family))
}
/// Whether the host declared `family` as an icon family whose glyphs are not text labels.
pub fn is_icon_family(family: &str) -> bool {
    let held = STYLE.lock().unwrap_or_else(|p| p.into_inner());
    held.as_ref().is_some_and(|style| {
        style.fixed_size_families.contains(family)
            || style
                .fixed_size_prefixes
                .iter()
                .any(|prefix| family.starts_with(prefix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_offset_map_text_and_icons_keep_their_size() {
        let style = TextStyle {
            aliases: BTreeMap::from([("Source Sans".into(), "DejaVu Sans".into())]),
            fixed_size_families: BTreeSet::from(["Test Icon Family".into()]),
            fixed_size_prefixes: vec!["Symbol ".into()],
            size_offset: 2.,
            ..Default::default()
        };
        assert_eq!(style.family("Source Sans"), "DejaVu Sans");
        assert_eq!(style.family("Other"), "Other");
        assert_eq!(style.size(12., "DejaVu Sans"), 14.);
        assert_eq!(style.size(255., "DejaVu Sans"), 256.);
        assert_eq!(style.size(20., "Test Icon Family"), 20.);
        assert_eq!(style.size(20., "Symbol Rounded"), 20.);
        assert_eq!(TextStyle::default().size(12., "Source Sans"), 12.);
    }
    #[test]
    fn gradients_stay_on_until_the_host_asks_for_solid_colours() {
        let solid = TextStyle {
            aliases: BTreeMap::from([("Unique Solid Family".into(), "Other".into())]),
            solid: true,
            ..Default::default()
        };
        assert!(set(solid));
        assert!(!gradients());
        assert!(set(TextStyle::default()));
        assert!(gradients());
    }
    #[test]
    fn a_change_wakes_listeners_and_a_repeat_does_not() {
        // Unique alias and no offset: concurrent layout tests see no effect.
        let woke = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = woke.clone();
        let _watch = crate::i18n::on_change(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        let style = TextStyle {
            aliases: BTreeMap::from([("Unique Test Family".into(), "Other".into())]),
            size_offset: 0.,
            ..Default::default()
        };
        assert!(set(style.clone()));
        assert!(woke.load(std::sync::atomic::Ordering::SeqCst) >= 1);
        assert_eq!(family("Unique Test Family"), "Other");
        assert!(!set(style));
        assert!(set(TextStyle::default()));
        assert_eq!(family("Unique Test Family"), "Unique Test Family");
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//! Shell strings: the built-in English table and `tr` for Rust-side text.
//! The same table backs the Luau `tr`, so a daemon-less preview reads correctly.
pub fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| bead::ui::i18n::set_builtin(include_str!("../../../shell/i18n/en.json")));
}
pub fn tr(id: &str, args: &[(&str, &str)]) -> String {
    init();
    bead::ui::i18n::tr(id, args)
}
pub fn tr_n(id: &str, count: u64, args: &[(&str, &str)]) -> String {
    init();
    bead::ui::i18n::tr_n(id, count, args)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::Path};

    const SURFACES: [&str; 10] = [
        "bar",
        "panel",
        "dashboard",
        "launcher",
        "osd",
        "notifications",
        "settings",
        "colors",
        "welcome",
        "common",
    ];
    fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "luau" || e == "rs")
                && !path.ends_with("i18n.rs")
            {
                out.push(path);
            }
        }
    }
    /// Every `bead.<surface>.<name>` literal in the shell sources.
    fn used() -> BTreeSet<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        sources(&root.join("../../shell"), &mut files);
        let mut ids = BTreeSet::new();
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            for (at, _) in text.match_indices("\"bead.") {
                let rest = &text[at + 1..];
                let id = &rest[..rest.find('"').unwrap_or(0)];
                let mut parts = id.split('.');
                let (_, surface, name) = (parts.next(), parts.next(), parts.next());
                if surface.is_some_and(|s| SURFACES.contains(&s)) && name.is_some() {
                    ids.insert(id.to_owned());
                }
            }
        }
        ids
    }
    #[test]
    fn the_english_table_and_the_sources_agree() {
        let english: std::collections::BTreeMap<String, bead::ui::i18n::Message> =
            serde_json::from_str(include_str!("../../../shell/i18n/en.json")).unwrap();
        let context: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("../../../shell/i18n/context.json")).unwrap();
        let used = used();
        let known: BTreeSet<_> = english.keys().cloned().collect();
        assert_eq!(
            used.difference(&known).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "ids used by the shell without English text"
        );
        assert_eq!(
            known.difference(&used).collect::<Vec<_>>(),
            Vec::<&String>::new(),
            "English ids no source uses"
        );
        assert_eq!(known, context.keys().cloned().collect::<BTreeSet<_>>());
        assert!(context.values().all(|note| note.len() >= 20));
        for id in &known {
            let parts: Vec<_> = id.split('.').collect();
            assert!(
                parts.len() == 3
                    && parts[0] == "bead"
                    && SURFACES.contains(&parts[1])
                    && parts[2].split('-').all(|w| {
                        !w.is_empty()
                            && w.bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                    }),
                "{id}"
            );
        }
    }
    #[test]
    fn plural_and_placeholder_text_comes_from_the_builtin_table() {
        assert_eq!(
            super::tr_n("bead.dashboard.updates-count", 1, &[]),
            "1 update"
        );
        assert_eq!(
            super::tr_n("bead.dashboard.updates-count", 3, &[]),
            "3 updates"
        );
        assert_eq!(
            super::tr("bead.panel.signal-strength", &[("percent", "40")]),
            "Signal strength 40%"
        );
        assert_eq!(super::tr("bead.nope.missing", &[]), "bead.nope.missing");
    }
}

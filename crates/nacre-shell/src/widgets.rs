// SPDX-License-Identifier: GPL-3.0-only
//! Opaque widget layout transport. The shell's Luau registry owns validation/composition.
use bead::core::{Value, source::Snapshot};
use std::collections::BTreeMap;

pub const TOPIC: &str = "settings.store.widgets";
/// `settings.set {store:"widgets", key:"layout", value:<document>, rev:<widgets.rev>}`.
/// A singleton record list carries the opaque document through the general UI store.
/// Disconnection/errors retain the previous accepted UI, rather than resetting it.
pub fn values(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    if !snapshot.connected
        || snapshot.data["store"] != "widgets"
        || !snapshot.data["error"].is_null()
    {
        return BTreeMap::new();
    }
    let layout = &snapshot.data["data"]["layout"];
    let documents = if layout.is_null() {
        Vec::new()
    } else if layout.is_object() {
        vec![layout.clone()]
    } else {
        eprintln!(
            "WIDGET_LAYOUT_ERROR file=settings.store.widgets line=1 message=layout must be an object"
        );
        return BTreeMap::new();
    };
    BTreeMap::from([
        ("widgets.layout".into(), Value::list(documents)),
        (
            "widgets.rev".into(),
            Value::Number(snapshot.data["rev"].as_u64().unwrap_or(0) as f64),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::core::source::Source;
    use bead_test::source::FakeSource;
    use serde_json::json;
    #[test]
    fn opaque_layout_preserves_unknown_packages_and_revision() {
        let doc = json!({"version":1,"hosts":{"top_bar":{"instances":[{"widget":"future","settings":{"opaque":[1,2]}}]}}});
        let source =
            FakeSource::new(json!({"store":"widgets","rev":7,"data":{"layout":doc},"error":null}))
                .unwrap();
        let mut snapshot = (*source.snapshot()).clone();
        let projected = values(&snapshot);
        assert_eq!(projected["widgets.rev"], Value::Number(7.));
        assert_eq!(projected["widgets.layout"], Value::list(vec![doc]));
        snapshot.connected = false;
        assert!(values(&snapshot).is_empty());
    }
}

/// Standalone shells persist the same document as JSON; atomic file replacement is watched.
pub fn file_values(path: &std::path::Path) -> BTreeMap<String, Value> {
    use std::io::Read;
    let read = || -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err("widget layout exceeds64KiB".into());
        }
        let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !doc.is_object() {
            return Err("widget layout must be an object".into());
        }
        Ok(doc)
    };
    match read() {
        Ok(doc) => BTreeMap::from([("widgets.layout".into(), Value::list(vec![doc]))]),
        Err(error) => {
            if path.exists() {
                eprintln!(
                    "WIDGET_LAYOUT_ERROR file={} line=1 message={error}",
                    path.display()
                );
            }
            BTreeMap::new()
        }
    }
}

pub fn file_path() -> Option<std::path::PathBuf> {
    std::env::var_os("BEAD_WIDGET_LAYOUT")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
                })
                .map(|root| root.join("bead/widgets.json"))
        })
        .filter(|path| path.is_absolute())
}

#[cfg(test)]
mod file_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn persisted_document_round_trips_and_malformed_edits_do_not_replace_it() {
        let path = std::env::temp_dir().join(format!(
            "bead-widget-layout-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let doc = json!({"version":1,"future":{"widget":"unknown","data":[1,2]}});
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(file_values(&path)["widgets.layout"], Value::list(vec![doc]));
        std::fs::write(&path, b"{broken").unwrap();
        assert!(file_values(&path).is_empty());
        std::fs::write(&path, b"[]").unwrap();
        assert!(file_values(&path).is_empty());
        std::fs::remove_file(path).unwrap();
    }
}

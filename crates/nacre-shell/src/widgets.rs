// SPDX-License-Identifier: GPL-3.0-only
//! Opaque widget layout transport. The shell's Luau registry owns validation/composition.
use bead::core::{Value, source::Snapshot};
use std::collections::BTreeMap;

pub const TOPIC: &str = "settings.store.widgets";
/// Compatibility with the original bar's string arguments and instance-owned dropdowns.
pub fn dropdown_view(argument: &Value) -> Option<&str> {
    match argument {
        Value::Text(view) => Some(view),
        Value::List(records) => records.first()?.get("view")?.as_str(),
        _ => None,
    }
}
pub fn dropdown_hover(argument: &Value) -> bool {
    match argument {
        Value::List(records) => records
            .first()
            .is_some_and(|record| record["trigger"] == "hover"),
        // Legacy authored controls retain their original dwell policy.
        _ => true,
    }
}
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

/// Replace a standalone layout without exposing a partially written document to watchers.
pub fn save_file(path: &std::path::Path, document: &serde_json::Value) -> std::io::Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec(document)?;
    if !document.is_object() || bytes.len() > 65536 {
        return Err(std::io::Error::other(
            "widget layout must be an object under64KiB",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("layout needs a parent"))?;
    std::fs::create_dir_all(parent)?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".widgets-{}-{next}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[derive(Clone, Copy)]
pub enum View {
    Editor,
    Menu,
    Canvas,
}

#[derive(Clone)]
pub struct Inputs {
    pub client: Option<crate::nacre_source::Client>,
    pub desktop: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<bead::services::Desktop>>>,
    pub backend: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<crate::backends::Native>>>,
    pub prefs: Option<std::sync::Arc<dyn bead::core::source::Source>>,
}

/// Widget views are lazy and share the catalogue and settings kit with the bar.
pub fn editor(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
    view: View,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    use bead::{
        render::{Rect, RendererKind},
        wayland::PanelSpec,
    };
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.width = 1.;
    fallback.bounds = Rect::new(0., 0., 1., 1.);
    fallback.primitives.clear();
    if matches!(view, View::Canvas) {
        fallback.motion = None;
    }
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: false,
        passive: false,
        keyboard_focus: false,
        initial_open: false,
        fallback,
        renderer: RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                (if matches!(view, View::Canvas) {
                    "widgets/editor/canvas.luau"
                } else {
                    "widgets/editor/main.luau"
                })
                .into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let mut runtime =
                    crate::reload::Runtime::panel_session(session, open).with_trace(trace);
                if let Some(backend) = inputs.backend.get() {
                    runtime = runtime.with_backend(backend.clone())?;
                }
                if let Some(desktop) = inputs.desktop.get() {
                    runtime = runtime.with_shared_desktop(desktop.clone(), false)?;
                }
                runtime = runtime
                    .with_widget_editor(inputs.client.as_ref(), matches!(view, View::Canvas))?;
                if let Some(prefs) = &inputs.prefs {
                    runtime = runtime.with_prefs(prefs.clone())?;
                }
                Ok(runtime)
            })();
            let mut runtime = result.map_err(|error| error.to_string())?;
            let mut restore = BTreeMap::from([
                (
                    "widgets.context".into(),
                    Value::Bool(matches!(view, View::Menu)),
                ),
                ("panel.open".into(), Value::Bool(open)),
            ]);
            if !matches!(view, View::Canvas) {
                restore.insert(
                    "widgets.editing".into(),
                    Value::Bool(matches!(view, View::Editor)),
                );
            }
            runtime.session.restore_on_start(restore);
            Ok(Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
        }),
    })
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

    #[test]
    fn atomic_save_keeps_previous_file_on_invalid_write() {
        let parent = std::env::temp_dir().join(format!("bead-widget-save-{}", std::process::id()));
        let path = parent.join("widgets.json");
        let doc = json!({"version":1,"future":{"opaque":[1,2]}});
        save_file(&path, &doc).unwrap();
        assert!(save_file(&path, &json!([])).is_err());
        assert_eq!(file_values(&path)["widgets.layout"], Value::list(vec![doc]));
        std::fs::remove_dir_all(parent).unwrap();
    }
}

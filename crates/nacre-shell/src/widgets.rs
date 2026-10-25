// SPDX-License-Identifier: GPL-3.0-only
//! Opaque widget layout transport. The shell's Luau registry owns validation/composition.
use bead::core::{Value, source::Snapshot};
use std::collections::BTreeMap;

pub const TOPIC: &str = "settings.store.widgets";

/// The catalogue and the user's canonical layout share the standalone app's warm first frame.
pub fn window_spec(
    root: std::path::PathBuf,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    client: Option<crate::nacre_source::Client>,
    trace: bool,
) -> Result<bead::wayland::WindowSpec, Box<dyn std::error::Error>> {
    use std::sync::Arc;
    let behavior = root.join("widgets/app/behavior.luau");
    let schema = bead::ui::script::Script::load(&std::fs::read(&behavior)?, &behavior)?.schema()?;
    let number = |key: &str| -> Result<u32, Box<dyn std::error::Error>> {
        match schema.get(key) {
            Some(Value::Number(value)) if value.is_finite() && *value > 0. && *value <= 4096. => {
                Ok(*value as u32)
            }
            _ => Err(format!("invalid Widgets app dimension: {key}").into()),
        }
    };
    let size = (number("window.width")?, number("window.height")?);
    let min_size = (number("window.min_width")?, number("window.min_height")?);
    let preferred_min = (
        number("window.preferred_min_width")?,
        number("window.preferred_min_height")?,
    );
    let fraction = match (
        schema.get("window.width_fraction"),
        schema.get("window.height_fraction"),
    ) {
        (Some(Value::Number(width)), Some(Value::Number(height)))
            if width.is_finite()
                && height.is_finite()
                && (0.1..=1.).contains(width)
                && (0.1..=1.).contains(height) =>
        {
            (*width, *height)
        }
        _ => return Err("invalid Widgets app output size fraction".into()),
    };
    let mut blank = crate::sound::fallback(colors.get("surface")?);
    blank.motion = None;
    blank.primitives.clear();
    blank.offset = (0., 0.);
    blank.width = size.0 as f32;
    blank.bounds = bead::render::Rect::new(0., 0., size.0 as f32, size.1 as f32);
    let mut warm = bead::ui::Session::new(
        root.clone(),
        "widgets/app/main.luau".into(),
        palette.clone(),
        colors.clone(),
        blank.clone(),
    );
    warm.set_viewport(size.0, size.1);
    let warm_runtime = crate::reload::Runtime::new(warm)
        .with_trace(trace)
        .with_shared_desktop(bead::services::Desktop::start(), false)?
        .with_widget_app(client.clone().as_ref(), &root)?;
    let mut warm_runtime = warm_runtime;
    let fallback = warm_runtime
        .warm_frame()
        .map_err(|error| format!("Widgets app first-frame compile failed: {error}"))?;
    drop(warm_runtime);
    let cold = blank;
    let app_root = root.clone();
    let app_palette = palette.clone();
    let app_colors = colors.clone();
    let factory: bead::wayland::WindowFactory = Arc::new(move |_: &[serde_json::Value]| {
        let mut session = bead::ui::Session::new(
            app_root.clone(),
            "widgets/app/main.luau".into(),
            app_palette.clone(),
            app_colors.clone(),
            cold.clone(),
        );
        session.set_viewport(size.0, size.1);
        crate::reload::Runtime::new(session)
            .with_trace(trace)
            .with_shared_desktop(bead::services::Desktop::start(), false)
            .and_then(|runtime| runtime.with_widget_app(client.as_ref(), &app_root))
            .map(|runtime| Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
            .map_err(|error| error.to_string())
    });
    Ok(bead::wayland::WindowSpec {
        app_id: "nacre-widgets".into(),
        title: "Widgets".into(),
        size,
        min_size,
        size_for_output: Some(Arc::new(move |output| {
            (
                ((f64::from(output.0) * fraction.0) as u32)
                    .max(preferred_min.0)
                    .min(size.0),
                ((f64::from(output.1) * fraction.1) as u32)
                    .max(preferred_min.1)
                    .min(size.1),
            )
        })),
        fallback,
        factory,
        model: Arc::new(|args| {
            let query = args.first().and_then(serde_json::Value::as_str).unwrap_or_default();
            BTreeMap::from([("widgets.search".into(), Value::Text(query.into()))])
        }),
    })
}
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

#[derive(Clone)]
pub struct Inputs {
    pub client: Option<crate::nacre_source::Client>,
    pub desktop: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<bead::services::Desktop>>>,
    pub backend: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<crate::backends::Native>>>,
    pub prefs: Option<std::sync::Arc<dyn bead::core::source::Source>>,
}

/// The desktop canvas is lazy and shares the layout and catalogue with the bar.
pub fn canvas(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    use bead::{
        render::{Rect, RendererKind},
        wayland::PanelSpec,
    };
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.width = 1.;
    fallback.bounds = Rect::new(0., 0., 1., 1.);
    fallback.primitives.clear();
    fallback.motion = None;
    let cold = fallback.clone();
    Ok(PanelSpec {
        position: Some(
            crate::placement::Frame::default().positioner(crate::placement::Attachment::Popout),
        ),
        engages_decorations: false,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: false,
        initial_open: false,
        fallback,
        renderer: RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                "widgets/editor/canvas.luau".into(),
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
                runtime = runtime.with_widget_editor(inputs.client.as_ref(), true)?;
                if let Some(prefs) = &inputs.prefs {
                    runtime = runtime.with_prefs(prefs.clone())?;
                }
                Ok(runtime)
            })();
            let mut runtime = result.map_err(|error| error.to_string())?;
            let restore = BTreeMap::from([("panel.open".into(), Value::Bool(open))]);
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

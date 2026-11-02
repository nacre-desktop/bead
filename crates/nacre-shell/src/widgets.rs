// SPDX-License-Identifier: GPL-3.0-only
//! Opaque widget layout transport. The shell's Luau registry owns validation/composition.
use bead::core::{Value, source::Snapshot};
use std::collections::BTreeMap;

pub const TOPIC: &str = "settings.store.widgets";

fn window_state_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/state"))
        })
        .map(|state| state.join("nacre/widgets-window.json"))
}

fn restored_window_size(path: &std::path::Path) -> Option<(u32, u32)> {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    if value.get("version")?.as_u64()? != 1 {
        return None;
    }
    let width = u32::try_from(value.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(value.get("height")?.as_u64()?).ok()?;
    ((420..=4096).contains(&width) && (360..=4096).contains(&height)).then_some((width, height))
}

fn save_window_size(path: &std::path::Path, (width, height): (u32, u32)) -> std::io::Result<()> {
    if !(420..=4096).contains(&width) || !(360..=4096).contains(&height) {
        return Err(std::io::Error::other("invalid Widgets window size"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("window size needs a parent"))?;
    std::fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version":1,"width":width,"height":height
    }))
    .map_err(std::io::Error::other)?;
    bead_widgets::atomic_replace(path, &bytes).map_err(widget_store_error)
}

/// Read the active presentation poster for the Widgets app's wallpaper preview fallback.
pub(crate) fn presentation_wallpaper_path() -> Option<String> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/state"))
        })?;
    presentation_wallpaper_path_from(&state)
}

fn presentation_wallpaper_path_from(state: &std::path::Path) -> Option<String> {
    let path = state.join("nacre/presentation.json");
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let poster = record.get("poster")?.as_str()?;
    let poster_path = std::path::Path::new(poster);
    (poster_path.is_absolute() && poster_path.is_file())
        .then(|| format!("wallpaper:current:{poster}"))
}

#[path = "widget_drag.rs"]
pub mod widget_drag;

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
    let default_size = (number("window.width")?, number("window.height")?);
    let state_path = window_state_path();
    let size = state_path
        .as_deref()
        .and_then(restored_window_size)
        .unwrap_or(default_size);
    let min_size = (number("window.min_width")?, number("window.min_height")?);
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
        command_targets: vec!["bead.widgets".into()],
        app_id: "nacre-widgets".into(),
        title: "Nacre Widgets".into(),
        size,
        min_size,
        size_for_output: Some(Arc::new(move |output| {
            (
                ((f64::from(output.0) * fraction.0) as u32)
                    .max(min_size.0)
                    .min(size.0),
                ((f64::from(output.1) * fraction.1) as u32)
                    .max(min_size.1)
                    .min(size.1),
            )
        })),
        on_resize: state_path.map(|path| {
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("nacre-widgets-size-save".into())
                .spawn(move || {
                    while let Ok(mut size) = receiver.recv() {
                        loop {
                            match receiver.recv_timeout(std::time::Duration::from_millis(300)) {
                                Ok(next) => size = next,
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        if let Err(error) = save_window_size(&path, size) {
                            eprintln!("WIDGET_WINDOW_SIZE_SAVE_ERROR {error}");
                        }
                    }
                })
                .expect("Widgets window size writer starts");
            Arc::new(move |size| {
                let _ = sender.send(size);
            }) as Arc<dyn Fn((u32, u32)) + Send + Sync>
        }),
        fallback,
        factory,
        model: Arc::new(|args| open_model(args.first())),
    })
}
/// An open request: a gallery search string, or (B0208 edit mode) the placed instance the user
/// clicked, `{widget, group}`, which opens that widget's page on that copy.
pub(crate) fn open_model(arg: Option<&serde_json::Value>) -> BTreeMap<String, Value> {
    let text = |value: &serde_json::Value| value.as_str().unwrap_or_default().to_owned();
    match arg {
        Some(target @ serde_json::Value::Object(_)) if target["widget"].is_string() => {
            BTreeMap::from([
                ("widgets.search".into(), Value::Text(String::new())),
                (
                    "widgets.selected".into(),
                    Value::Text(text(&target["widget"])),
                ),
                ("widgets.group".into(), Value::Text(text(&target["group"]))),
            ])
        }
        other => BTreeMap::from([(
            "widgets.search".into(),
            Value::Text(other.map(text).unwrap_or_default()),
        )]),
    }
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

/// Preserve Nacre's established layout error text at its storage adapter.
fn widget_store_error(error: std::io::Error) -> std::io::Error {
    let message = error.to_string();
    let compatible = match message.as_str() {
        "document path needs a file name" => "layout needs a file name",
        "document path needs a parent" => "layout needs a parent",
        "invalid revision stamp" => "invalid widget revision stamp",
        "document exceeds 64KiB" => "widget layout exceeds64KiB",
        "document must be an object" => "widget layout must be an object",
        "document must be an object under 64KiB" => "widget layout must be an object under64KiB",
        _ if message.starts_with("layout revision conflict:") => {
            return std::io::Error::new(
                error.kind(),
                message.replacen(
                    "layout revision conflict:",
                    "widget layout revision conflict:",
                    1,
                ),
            );
        }
        _ => return error,
    };
    std::io::Error::new(error.kind(), compatible)
}

pub fn file_values(path: &std::path::Path) -> BTreeMap<String, Value> {
    let store = bead_widgets::LayoutStore::new(path);
    match store.read_with_report(|error| {
        let error = widget_store_error(std::io::Error::new(error.kind(), error.to_string()));
        eprintln!(
            "WIDGET_LAYOUT_REVISION_ERROR file={} message={error}",
            path.display()
        );
    }) {
        Ok((doc, revision)) => BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![doc])),
            ("widgets.rev".into(), Value::Number(revision as f64)),
        ]),
        Err(error) => {
            if path.exists() {
                let error = widget_store_error(error);
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

/// Expected-revision compare-and-swap for Nacre's app-supplied layout path.
pub fn save_file_revisioned(
    path: &std::path::Path,
    document: &serde_json::Value,
    expected: u64,
) -> std::io::Result<u64> {
    bead_widgets::LayoutStore::new(path)
        .write_revisioned(document, expected)
        .map_err(widget_store_error)
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
    canvas_spec(
        palette,
        colors,
        inputs,
        trace,
        "widgets/editor/canvas.luau",
        false,
    )
}

pub fn drag_preview(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    canvas_spec(
        palette,
        colors,
        inputs,
        trace,
        "widgets/drag/ghost.luau",
        true,
    )
}

/// B0210: the static drop zones shown during a widget drag (above windows, input-empty).
pub fn drag_zones(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    canvas_spec(
        palette,
        colors,
        inputs,
        trace,
        "widgets/drag/zones.luau",
        true,
    )
}

fn canvas_spec(
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
    entry: &'static str,
    passive: bool,
) -> Result<bead::wayland::PanelSpec, Box<dyn std::error::Error>> {
    canvas_spec_with_root(
        "shell".into(),
        palette,
        colors,
        inputs,
        trace,
        entry,
        passive,
    )
}

pub(crate) fn canvas_spec_with_root(
    shell_root: std::path::PathBuf,
    palette: std::path::PathBuf,
    colors: bead::render::Palette,
    inputs: Inputs,
    trace: bool,
    entry: &'static str,
    passive: bool,
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
        // The passive preview retains its hidden, prepared scene briefly between
        // gestures; the host canvas remains lazy when it is empty.
        retain_when_closed: passive,
        passive,
        keyboard_focus: false,
        text_focus_only: false,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let session = bead::ui::Session::new(
                shell_root.clone(),
                entry.into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            let result = if passive {
                Ok(crate::reload::Runtime::new(session)
                    .with_trace(trace)
                    .with_widget_drag_preview(entry == "widgets/drag/ghost.luau"))
            } else {
                (|| -> Result<_, Box<dyn std::error::Error>> {
                    let mut runtime =
                        crate::reload::Runtime::panel_session(session, open).with_trace(trace);
                    if let Some(backend) = inputs.backend.get() {
                        runtime = runtime.with_backend(backend.clone())?;
                    }
                    if let Some(desktop) = inputs.desktop.get() {
                        runtime = runtime.with_shared_desktop(desktop.clone(), false)?;
                    }
                    runtime = runtime.with_widget_editor(inputs.client.as_ref(), true)?;
                    if let Some(client) = &inputs.client {
                        runtime = runtime.with_session_menu(client.clone())?;
                    }
                    if let Some(prefs) = &inputs.prefs {
                        runtime = runtime.with_prefs(prefs.clone())?;
                    }
                    Ok(runtime)
                })()
            };
            let mut runtime = result.map_err(|error| error.to_string())?;
            if !passive {
                let restore = BTreeMap::from([("panel.open".into(), Value::Bool(open))]);
                runtime.session.restore_on_start(restore);
            }
            Ok(Box::new(runtime) as Box<dyn bead::wayland::SceneSource>)
        }),
    })
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn widget_store_errors_keep_product_wording() {
        assert_eq!(
            widget_store_error(std::io::Error::other("document exceeds 64KiB")).to_string(),
            "widget layout exceeds64KiB"
        );
        assert_eq!(
            widget_store_error(std::io::Error::other("document must be an object")).to_string(),
            "widget layout must be an object"
        );
        assert_eq!(
            widget_store_error(std::io::Error::other(
                "layout revision conflict: expected 2, found 3"
            ))
            .to_string(),
            "widget layout revision conflict: expected 2, found 3"
        );
    }

    #[test]
    fn widget_window_size_restore_accepts_only_bounded_versioned_dimensions() {
        let path = std::env::temp_dir().join(format!(
            "bead-widget-window-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, br#"{"version":1,"width":1080,"height":760}"#).unwrap();
        assert_eq!(restored_window_size(&path), Some((1080, 760)));
        std::fs::write(&path, br#"{"version":2,"width":1080,"height":760}"#).unwrap();
        assert_eq!(restored_window_size(&path), None);
        std::fs::write(&path, br#"{"version":1,"width":9000,"height":760}"#).unwrap();
        assert_eq!(restored_window_size(&path), None);
        save_window_size(&path, (960, 700)).unwrap();
        assert_eq!(restored_window_size(&path), Some((960, 700)));
        assert!(save_window_size(&path, (400, 300)).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn presentation_poster_fallback_requires_an_existing_absolute_file() {
        let parent =
            std::env::temp_dir().join(format!("bead-widget-poster-{}", std::process::id()));
        let state = parent.join("state");
        let poster = parent.join("wallpaper.jpg");
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(state.join("nacre")).unwrap();
        std::fs::write(&poster, b"fixture poster").unwrap();
        let record = state.join("nacre/presentation.json");
        std::fs::write(
            &record,
            serde_json::to_vec(&serde_json::json!({"poster":poster.to_string_lossy()})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            presentation_wallpaper_path_from(&state),
            Some(format!("wallpaper:current:{}", poster.to_string_lossy()))
        );
        std::fs::write(
            &record,
            br#"{"poster":"https://example.invalid/wallpaper"}"#,
        )
        .unwrap();
        assert_eq!(presentation_wallpaper_path_from(&state), None);
        std::fs::remove_dir_all(parent).unwrap();
    }
}

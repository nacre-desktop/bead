// SPDX-License-Identifier: GPL-3.0-only
//! Opaque widget layout transport. The shell's Luau registry owns validation/composition.
use bead::core::{Value, source::Snapshot};
use std::collections::BTreeMap;

pub const TOPIC: &str = "settings.store.widgets";
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
            let query = args
                .first()
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
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
    let read = match lock_layout(path) {
        Ok(_lock) => read_layout(path).map(|(doc, bytes)| {
            let revision = current_revision_locked(path, &bytes).unwrap_or_else(|error| {
                eprintln!(
                    "WIDGET_LAYOUT_REVISION_ERROR file={} message={error}",
                    path.display()
                );
                stamp_revision(path).unwrap_or(0)
            });
            (doc, revision)
        }),
        Err(lock_error) => {
            eprintln!(
                "WIDGET_LAYOUT_REVISION_ERROR file={} message={lock_error}",
                path.display()
            );
            read_layout(path).map(|(doc, _)| (doc, stamp_revision(path).unwrap_or(0)))
        }
    };
    match read {
        Ok((doc, revision)) => BTreeMap::from([
            ("widgets.layout".into(), Value::list(vec![doc])),
            ("widgets.rev".into(), Value::Number(revision as f64)),
        ]),
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

struct LayoutLock(std::fs::File);
impl Drop for LayoutLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn sibling_path(path: &std::path::Path, suffix: &str) -> std::io::Result<std::path::PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("layout needs a file name"))?;
    Ok(path.with_file_name(format!("{}{}", name.to_string_lossy(), suffix)))
}
fn lock_layout(path: &std::path::Path) -> std::io::Result<LayoutLock> {
    use std::os::fd::AsRawFd;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("layout needs a parent"))?;
    std::fs::create_dir_all(parent)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(sibling_path(path, ".lock")?)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(LayoutLock(file))
}
fn layout_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
fn stamp_revision(path: &std::path::Path) -> std::io::Result<u64> {
    let stamp: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sibling_path(path, ".rev")?)?)?;
    stamp["rev"]
        .as_u64()
        .ok_or_else(|| std::io::Error::other("invalid widget revision stamp"))
}
fn atomic_replace(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("layout needs a parent"))?;
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".widgets-{}-{next}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
fn read_layout(path: &std::path::Path) -> std::io::Result<(serde_json::Value, Vec<u8>)> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(std::io::Error::other("widget layout exceeds64KiB"));
    }
    let doc: serde_json::Value = serde_json::from_slice(&bytes)?;
    if !doc.is_object() {
        return Err(std::io::Error::other("widget layout must be an object"));
    }
    Ok((doc, bytes))
}
fn current_revision_locked(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<u64> {
    let stamp_path = sibling_path(path, ".rev")?;
    let hash = layout_hash(bytes);
    let (revision, next) = match std::fs::read(&stamp_path) {
        Ok(raw) => {
            let stamp: serde_json::Value = serde_json::from_slice(&raw)?;
            let revision = stamp["rev"]
                .as_u64()
                .ok_or_else(|| std::io::Error::other("invalid widget revision stamp"))?;
            if stamp["hash"].as_str() == Some(hash.as_str()) {
                (revision, None)
            } else {
                (revision.saturating_add(1), Some(revision.saturating_add(1)))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (0, Some(0)),
        Err(error) => return Err(error),
    };
    if let Some(revision) = next {
        let stamp = serde_json::to_vec(&serde_json::json!({"hash":hash,"rev":revision}))?;
        atomic_replace(&stamp_path, &stamp)?;
    }
    Ok(revision)
}
fn write_file_locked(
    path: &std::path::Path,
    document: &serde_json::Value,
    expected: Option<u64>,
) -> std::io::Result<u64> {
    let bytes = serde_json::to_vec(document)?;
    if !document.is_object() || bytes.len() > 65536 {
        return Err(std::io::Error::other(
            "widget layout must be an object under64KiB",
        ));
    }
    let old = match read_layout(path) {
        Ok((_, bytes)) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let current = if let Some(bytes) = old.as_ref() {
        current_revision_locked(path, bytes)?
    } else {
        match stamp_revision(path) {
            Ok(revision) => {
                let revision = revision.saturating_add(1);
                let stamp = serde_json::to_vec(&serde_json::json!({"hash":"","rev":revision}))?;
                atomic_replace(&sibling_path(path, ".rev")?, &stamp)?;
                revision
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error),
        }
    };
    if let Some(expected) = expected.filter(|expected| *expected != current) {
        return Err(std::io::Error::other(format!(
            "widget layout revision conflict: expected {expected}, found {current}"
        )));
    }
    let revision = current.saturating_add(1);
    atomic_replace(path, &bytes)?;
    let stamp_path = sibling_path(path, ".rev")?;
    let stamp =
        serde_json::to_vec(&serde_json::json!({"hash":layout_hash(&bytes),"rev":revision}))?;
    if let Err(error) = atomic_replace(&stamp_path, &stamp) {
        if let Some(old) = old {
            let _ = atomic_replace(path, &old);
        } else {
            let _ = std::fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(revision)
}
/// Expected-revision compare-and-swap for standalone layouts; JSON remains the raw document.
pub fn save_file_revisioned(
    path: &std::path::Path,
    document: &serde_json::Value,
    expected: u64,
) -> std::io::Result<u64> {
    let _lock = lock_layout(path)?;
    write_file_locked(path, document, Some(expected))
}
/// Compatibility writer for tests and legacy callers that intentionally replace unconditionally.
#[cfg(test)]
pub fn save_file(path: &std::path::Path, document: &serde_json::Value) -> std::io::Result<()> {
    let _lock = lock_layout(path)?;
    write_file_locked(path, document, None).map(|_| ())
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
        initial_open: false,
        fallback,
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
                    .with_widget_drag_preview())
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

    #[test]
    fn standalone_revision_stamp_serializes_writers_and_detects_raw_edits() {
        let parent =
            std::env::temp_dir().join(format!("bead-widget-revisions-{}", std::process::id()));
        let path = parent.join("widgets.json");
        let _ = std::fs::remove_dir_all(&parent);
        let first = json!({"version":1,"layout":"first","opaque":{"x":1}});
        let second = json!({"version":1,"layout":"second","opaque":{"x":1}});
        let alternate = json!({"version":1,"layout":"alternate","opaque":{"x":1}});
        let third = json!({"version":1,"layout":"third","opaque":{"x":1}});
        assert_eq!(save_file_revisioned(&path, &first, 0).unwrap(), 1);
        let reader_a = file_values(&path);
        let reader_b = file_values(&path);
        assert_eq!(reader_a["widgets.rev"], Value::Number(1.));
        assert_eq!(reader_b["widgets.rev"], reader_a["widgets.rev"]);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            serde_json::to_vec(&first).unwrap(),
            "revision metadata remains outside the canonical JSON document"
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let writer = |document: serde_json::Value, barrier: std::sync::Arc<std::sync::Barrier>| {
            let path = path.clone();
            std::thread::spawn(move || {
                barrier.wait();
                save_file_revisioned(&path, &document, 1)
            })
        };
        let writer_a = writer(second.clone(), barrier.clone());
        let writer_b = writer(alternate.clone(), barrier.clone());
        barrier.wait();
        let result_a = writer_a.join().unwrap();
        let result_b = writer_b.join().unwrap();
        assert_ne!(
            result_a.is_ok(),
            result_b.is_ok(),
            "the lock and revision check admit only one concurrent writer"
        );
        assert!(
            save_file_revisioned(&path, &third, 1)
                .unwrap_err()
                .to_string()
                .contains("revision conflict"),
            "a second reader cannot overwrite a newer save using its stale revision"
        );
        assert_eq!(file_values(&path)["widgets.rev"], Value::Number(2.));
        let winner = file_values(&path)["widgets.layout"].clone();
        assert!(winner == Value::list(vec![second]) || winner == Value::list(vec![alternate]));

        std::fs::write(&path, serde_json::to_vec(&third).unwrap()).unwrap();
        let reopened = file_values(&path);
        assert_eq!(
            reopened["widgets.rev"],
            Value::Number(3.),
            "an external raw JSON edit advances the stamp"
        );
        assert_eq!(reopened["widgets.layout"], Value::list(vec![third]));
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn unreadable_revision_sidecar_does_not_hide_a_valid_layout_or_allow_write() {
        let parent =
            std::env::temp_dir().join(format!("bead-widget-bad-stamp-{}", std::process::id()));
        let path = parent.join("widgets.json");
        let doc = json!({"version":1,"widget":"known"});
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(parent.join("widgets.json.rev")).unwrap();
        std::fs::write(&path, serde_json::to_vec(&doc).unwrap()).unwrap();
        let values = file_values(&path);
        assert_eq!(
            values["widgets.layout"],
            Value::list(vec![doc.clone()]),
            "stamp errors do not make a valid readable document disappear"
        );
        assert!(
            save_file_revisioned(&path, &json!({"version":1}), 0).is_err(),
            "a writer fails closed when it cannot establish the shared revision"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            doc
        );
        std::fs::remove_dir_all(parent).unwrap();
    }
}

// SPDX-License-Identifier: MIT
use bead::{
    host::{FolderServicesRuntime, FolderView},
    render::{RendererKind, Scene},
    wayland::{Options, PanelSpec, Panels, PrimaryFactory, UiEvent, WindowSpec},
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    title: String,
    #[serde(default = "default_view")]
    view: PathBuf,
    #[serde(default = "default_palette")]
    palette: PathBuf,
    window: Option<Window>,
    layer: Option<Layer>,
    services: Option<Services>,
    commands: Option<BTreeMap<String, DeclaredCommand>>,
    /// B0250: popup panels a `[layer]` bar opens with `panel.open|toggle|close`.
    panels: Option<BTreeMap<String, Panel>>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Panel {
    view: PathBuf,
    size: [u32; 2],
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Services {
    #[serde(default)]
    desktop: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredCommand {
    argv: Vec<String>,
    interval: Option<u64>,
    #[serde(default = "five")]
    timeout: u64,
}
fn five() -> u64 {
    5
}
fn default_view() -> PathBuf {
    "view/main.luau".into()
}
fn default_palette() -> PathBuf {
    "palette.json".into()
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    size: [u32; 2],
    min_size: Option<[u32; 2]>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    edge: Edge,
    size: u32,
    #[serde(default = "yes")]
    exclusive: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}
impl From<Edge> for bead::core::Edge {
    fn from(e: Edge) -> Self {
        match e {
            Edge::Top => Self::Top,
            Edge::Bottom => Self::Bottom,
            Edge::Left => Self::Left,
            Edge::Right => Self::Right,
        }
    }
}
#[derive(Debug)]
struct App {
    root: PathBuf,
    manifest: Manifest,
}

/// `panels`: `None` when the app has no panels, else the bar edge for the bar view and
/// `Some(None)` for a panel view (see [`FolderView::with_panel_actions`]).
fn load_source(
    root: PathBuf,
    view: PathBuf,
    palette: PathBuf,
    title: String,
    size: (u32, u32),
    runtime: Option<Arc<FolderServicesRuntime>>,
    panels: Option<Option<bead::core::Edge>>,
) -> Result<Box<dyn bead::wayland::SceneSource>, String> {
    let loaded = if let Some(runtime) = runtime {
        FolderView::load_with_runtime(root, view, palette, &title, size, runtime)
    } else {
        FolderView::load(root, view, palette, &title, size)
    };
    loaded
        .map(|view| match panels {
            Some(bar_edge) => view.with_panel_actions(bar_edge),
            None => view,
        })
        .map(|view| Box::new(view) as Box<dyn bead::wayland::SceneSource>)
        .map_err(|error| error.to_string())
}
/// A popup sits flush against the bar's inner edge, centred on the invocation point along
/// the bar and clamped inside the output.
fn panel_position(
    edge: Edge,
    bar: i32,
    size: (f32, f32),
    center: Option<f32>,
    output: (i32, i32),
) -> (i32, i32) {
    let (w, h) = (size.0.ceil() as i32, size.1.ceil() as i32);
    let along = |span: i32, extent: i32| {
        center
            .map_or(0, |c| (c - extent as f32 / 2.).round() as i32)
            .clamp(0, (span - extent).max(0))
    };
    match edge {
        Edge::Top => (along(output.0, w), bar),
        Edge::Bottom => (along(output.0, w), output.1 - bar - h),
        Edge::Left => (bar, along(output.1, h)),
        Edge::Right => (output.0 - bar - w, along(output.1, h)),
    }
}
impl App {
    fn load(folder: &Path) -> Result<Self, Box<dyn Error>> {
        let root = folder
            .canonicalize()
            .map_err(|e| format!("{}: {e}", folder.display()))?;
        let path = root.join("bead.toml");
        let source =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest: Manifest =
            toml::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))?;
        if manifest.id.trim().is_empty() || manifest.title.trim().is_empty() {
            return Err(format!("{}: id and title must not be empty", path.display()).into());
        }
        if manifest.window.is_some() == manifest.layer.is_some() {
            return Err(format!(
                "{}: specify exactly one of [window] or [layer]",
                path.display()
            )
            .into());
        }
        for p in [&manifest.view, &manifest.palette] {
            validate_path(&root, p)?;
        }
        if let Some(w) = &manifest.window
            && (w.size.contains(&0) || w.min_size.is_some_and(|s| s.contains(&0)))
        {
            return Err(format!("{}: window sizes must be positive", path.display()).into());
        }
        if manifest.layer.as_ref().is_some_and(|l| l.size == 0) {
            return Err(format!("{}: layer.size must be positive", path.display()).into());
        }
        if let Some(panels) = &manifest.panels {
            if manifest.layer.is_none() {
                return Err(format!("{}: panels require a [layer] surface", path.display()).into());
            }
            for (name, panel) in panels {
                if panel.size.contains(&0) {
                    return Err(format!(
                        "{}: panel '{name}' size must be positive",
                        path.display()
                    )
                    .into());
                }
                validate_path(&root, &panel.view)?;
            }
        }
        if let Some(services) = &manifest.services {
            for name in &services.desktop {
                if !bead::host::DESKTOP_SERVICES.contains(&name.as_str()) {
                    return Err(
                        format!("{}: unknown desktop service '{name}'", path.display()).into(),
                    );
                }
            }
        }
        if let Some(commands) = &manifest.commands {
            for (name, command) in commands {
                if name.is_empty()
                    || name.len() > 32
                    || !name.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
                    })
                {
                    return Err(format!("{}: invalid command name '{name}'", path.display()).into());
                }
                if command.argv.is_empty() || command.argv.iter().any(|a| a.contains('\0')) {
                    return Err(format!(
                        "{}: command '{name}' argv must be non-empty and contain no NUL",
                        path.display()
                    )
                    .into());
                }
                if command.timeout == 0 || command.timeout > 60 {
                    return Err(format!(
                        "{}: command '{name}' timeout must be 1..=60 seconds",
                        path.display()
                    )
                    .into());
                }
                if command.interval == Some(0) {
                    return Err(format!(
                        "{}: command '{name}' interval must be at least 1 second",
                        path.display()
                    )
                    .into());
                }
            }
        }
        let app = Self { root, manifest };
        // Name and count rules live in the framework catalog.
        app.panel_catalog(None, RendererKind::Cpu)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(app)
    }
    /// The bar edge for the bar view, `Some(None)` for a panel view, `None` without panels.
    fn panel_actions(&self, bar: bool) -> Option<Option<bead::core::Edge>> {
        self.manifest
            .panels
            .as_ref()
            .and(self.manifest.layer.as_ref())
            .map(|layer| bar.then(|| layer.edge.into()))
    }
    /// The named popup catalog and its default spec (spec 0250), if the manifest declares panels.
    fn panel_catalog(
        &self,
        runtime: Option<Arc<FolderServicesRuntime>>,
        renderer: RendererKind,
    ) -> Result<Option<(Panels, PanelSpec)>, String> {
        let (Some(panels), Some(layer)) = (&self.manifest.panels, &self.manifest.layer) else {
            return Ok(None);
        };
        let (edge, thickness) = (layer.edge, layer.size as i32);
        let views: BTreeMap<String, PanelSpec> = panels
            .iter()
            .map(|(name, panel)| {
                let size = (panel.size[0], panel.size[1]);
                let root = self.root.clone();
                let view = panel.view.clone();
                let palette = self.manifest.palette.clone();
                let title = self.manifest.title.clone();
                let runtime = runtime.clone();
                let spec = PanelSpec {
                    engages_decorations: false,
                    overlay: false,
                    retain_when_closed: true,
                    passive: false,
                    keyboard_focus: true,
                    text_focus_only: false,
                    initial_open: false,
                    factory: Arc::new(move |_| {
                        load_source(
                            root.clone(),
                            view.clone(),
                            palette.clone(),
                            title.clone(),
                            size,
                            runtime.clone(),
                            Some(None),
                        )
                    }),
                    fallback: Scene::empty(size.0 as f32, size.1 as f32),
                    fallback_factory: None,
                    renderer,
                    position: Some(Arc::new(
                        move |scene: &Scene, center: Option<f32>, output: (i32, i32)| {
                            panel_position(
                                edge,
                                thickness,
                                (scene.bounds.w, scene.bounds.h),
                                center,
                                output,
                            )
                        },
                    )),
                };
                (name.clone(), spec)
            })
            .collect();
        // An empty table fails inside `Panels::new`.
        let first = views.keys().next().cloned().unwrap_or_default();
        let default = views.get(&first).cloned();
        Panels::new(first, views).map(|panels| default.map(|default| (panels, default)))
    }
    fn folder_services(&self) -> bead::host::FolderServices {
        bead::host::FolderServices {
            desktop: self
                .manifest
                .services
                .as_ref()
                .map(|s| s.desktop.iter().cloned().collect())
                .unwrap_or_default(),
            commands: self
                .manifest
                .commands
                .as_ref()
                .into_iter()
                .flat_map(|m| m.iter())
                .map(|(name, cmd)| {
                    (
                        name.clone(),
                        bead::host::CommandSpec {
                            argv: cmd.argv.clone(),
                            interval: cmd.interval.map(std::time::Duration::from_secs),
                            timeout: std::time::Duration::from_secs(cmd.timeout),
                        },
                    )
                })
                .collect(),
        }
    }
    fn view(&self, view: &Path, size: (u32, u32), bar: bool) -> Result<FolderView, Box<dyn Error>> {
        FolderView::load_with_services(
            self.root.clone(),
            view.to_path_buf(),
            self.manifest.palette.clone(),
            &self.manifest.title,
            size,
            self.folder_services(),
        )
        .map(|loaded| match self.panel_actions(bar) {
            Some(bar_edge) => loaded.with_panel_actions(bar_edge),
            None => loaded,
        })
        .map_err(|e| format!("{} / {}: {e}", self.root.display(), view.display()).into())
    }
    /// The bar's source. As for windows and panels, its script and CPU image preparation run
    /// on a worker actor: a bare `FolderView` cannot prepare images for the CPU renderer.
    fn primary_factory(&self, runtime: Option<Arc<FolderServicesRuntime>>) -> PrimaryFactory {
        let size = self.surface_size();
        let panel_actions = self.panel_actions(true);
        let root = self.root.clone();
        let view = self.manifest.view.clone();
        let palette = self.manifest.palette.clone();
        let title = self.manifest.title.clone();
        Arc::new(move |_: String| {
            let (root, view, palette) = (root.clone(), view.clone(), palette.clone());
            let (title, runtime) = (title.clone(), runtime.clone());
            Ok(Box::new(bead::wayland::worker::Source::new(
                Box::new(move || {
                    load_source(root, view, palette, title, size, runtime, panel_actions)
                }),
                Scene::empty(size.0 as f32, size.1 as f32),
            )) as Box<dyn bead::wayland::SceneSource>)
        })
    }
    fn surface_size(&self) -> (u32, u32) {
        if let Some(w) = &self.manifest.window {
            (w.size[0], w.size[1])
        } else {
            let layer = self.manifest.layer.as_ref().expect("validated surface");
            match layer.edge {
                Edge::Top | Edge::Bottom => (1280, layer.size),
                Edge::Left | Edge::Right => (layer.size, 720),
            }
        }
    }
    fn check(&self) -> Result<(), Box<dyn Error>> {
        let size = self.surface_size();
        let mut view = self.view(&self.manifest.view, size, true)?;
        view.check_render(UiEvent::Reload)
            .map_err(|e| format!("{}: {e}", self.manifest.view.display()))?;
        for panel in self.manifest.panels.iter().flat_map(|p| p.values()) {
            let mut view = self.view(&panel.view, (panel.size[0], panel.size[1]), false)?;
            view.check_render(UiEvent::Reload)
                .map_err(|e| format!("{}: {e}", panel.view.display()))?;
        }
        println!("OK {}", self.manifest.title);
        Ok(())
    }
    fn run(self, seconds: Option<f64>) -> Result<(), Box<dyn Error>> {
        let surface_size = self.surface_size();
        let runtime = FolderServicesRuntime::start(self.folder_services());
        let fallback = Scene::empty(surface_size.0 as f32, surface_size.1 as f32);
        let mut options = Options::default();
        options.namespace = self.manifest.id.clone();
        options.seconds = seconds;
        if let Some(w) = &self.manifest.window {
            let app_id = self.manifest.id.clone();
            let title = self.manifest.title.clone();
            let root = self.root.clone();
            let view = self.manifest.view.clone();
            let palette = self.manifest.palette.clone();
            let size = (w.size[0], w.size[1]);
            let runtime = runtime.clone();
            let min = w.min_size.map(|s| (s[0], s[1])).unwrap_or((320, 240));
            let mut spec = WindowSpec::new(
                app_id,
                title.clone(),
                size,
                fallback.clone(),
                Arc::new(move |_| {
                    load_source(
                        root.clone(),
                        view.clone(),
                        palette.clone(),
                        title.clone(),
                        size,
                        runtime.clone(),
                        None,
                    )
                }),
            );
            spec.min_size = min;
            options.window = Some(spec);
            options.app = Some(vec![]);
            options.interactive = true;
            options.keyboard_focus = true;
        } else if let Some(l) = &self.manifest.layer {
            if let Some((panels, default)) =
                self.panel_catalog(runtime.clone(), options.renderer)?
            {
                options.panels = Some(panels);
                options.panel_spec = Some(default);
            }
            options.primary_factory = Some(self.primary_factory(runtime.clone()));
            options.bar = true;
            options.primary_surface_viewport = true;
            options.edge = l.edge.into();
            options.bar_size = Some(l.size);
            options.bar_exclusive = l.exclusive;
            options.content_size = false;
            options.interactive = true;
        }
        bead::wayland::run(options, fallback)?;
        Ok(())
    }
}
fn validate_path(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!(
            "{}: manifest path must be relative to the app folder",
            path.display()
        )
        .into());
    }
    let full = root.join(path);
    let resolved = full
        .canonicalize()
        .map_err(|e| format!("{}: {e}", full.display()))?;
    if !resolved.starts_with(root) {
        return Err(format!("{}: path escapes the app folder", path.display()).into());
    }
    Ok(())
}
fn usage() -> &'static str {
    "Usage:\n  bead run <folder> [--seconds N]\n  bead check <folder>\n\nRuns or validates a Luau application folder."
}
fn main() {
    // Display, not the Debug form `main() -> Result` prints, so multi-line errors stay readable.
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("{}", usage());
        std::process::exit(2)
    };
    if cmd == "--help" || cmd == "-h" {
        println!("{}", usage());
        return Ok(());
    }
    if cmd != "run" && cmd != "check" {
        eprintln!("unknown command {cmd}\n{}", usage());
        std::process::exit(2)
    }
    let folder = args
        .next()
        .ok_or_else(|| format!("missing folder\n{}", usage()))?;
    let mut seconds = None;
    while let Some(a) = args.next() {
        if cmd == "run" && a == "--seconds" {
            let n = args
                .next()
                .ok_or("missing --seconds value")?
                .parse::<f64>()?;
            if !n.is_finite() || n <= 0. {
                return Err("--seconds must be finite and positive".into());
            }
            seconds = Some(n)
        } else {
            return Err(format!("unexpected argument {a}").into());
        }
    }
    let app = App::load(Path::new(&folder))?;
    if cmd == "check" {
        app.check()
    } else {
        app.run(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    fn folder(body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "bead-cli-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(p.join("view")).unwrap();
        std::fs::write(p.join("bead.toml"), body).unwrap();
        std::fs::write(p.join("view/main.luau"), "return {}\n").unwrap();
        std::fs::write(
            p.join("palette.json"),
            r#"{"formatVersion":1,"colours":{}}"#,
        )
        .unwrap();
        p
    }
    fn base() -> &'static str {
        "id='x.y'\ntitle='Test'\n"
    }
    #[test]
    fn accepts_window_manifest() {
        let p = folder(&format!("{}\n[window]\nsize=[440,300]\n", base()));
        assert!(App::load(&p).is_ok());
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn accepts_layer_manifest() {
        let p = folder(&format!("{}\n[layer]\nedge='top'\nsize=36\n", base()));
        assert_eq!(App::load(&p).unwrap().surface_size(), (1280, 36));
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn vertical_layer_check_uses_real_thickness() {
        for edge in ["left", "right"] {
            let p = folder(&format!("{}\n[layer]\nedge='{edge}'\nsize=36\n", base()));
            assert_eq!(App::load(&p).unwrap().surface_size(), (36, 720));
            let _ = std::fs::remove_dir_all(p);
        }
    }
    #[test]
    fn rejects_both_and_neither_surface_tables() {
        for suffix in [
            "",
            "\n[window]\nsize=[10,10]\n[layer]\nedge='top'\nsize=2\n",
        ] {
            let p = folder(&format!("{}{}", base(), suffix));
            assert!(App::load(&p).is_err());
            let _ = std::fs::remove_dir_all(p);
        }
    }
    #[test]
    fn rejects_unknown_manifest_keys() {
        let p = folder(&format!("{}\nextra=true\n[window]\nsize=[10,10]\n", base()));
        assert!(App::load(&p).is_err());
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn accepts_service_subset_and_declared_command() {
        let p = folder(&format!(
            "{}\n[window]\nsize=[10,10]\n[services]\ndesktop=['clock','tray','network_devices']\n[commands.weather]\nargv=['echo','hi']\ninterval=60\n",
            base()
        ));
        let app = App::load(&p).unwrap();
        assert_eq!(
            app.folder_services().desktop,
            BTreeSet::from(["clock".into(), "tray".into(), "network_devices".into()])
        );
        assert_eq!(
            app.folder_services().commands["weather"].argv,
            vec!["echo", "hi"]
        );
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn rejects_bad_service_and_command_declarations() {
        for suffix in [
            "\n[services]\ndesktop=['clock','battery-plus']\n",
            "\n[commands.BadName]\nargv=['echo']\n",
            "\n[commands.empty]\nargv=[]\n",
            "\n[commands.late]\nargv=['echo']\ntimeout=61\n",
            "\n[commands.zero]\nargv=['echo']\ninterval=0\n",
        ] {
            let p = folder(&format!("{}\n[window]\nsize=[10,10]\n{}", base(), suffix));
            assert!(
                App::load(&p).is_err(),
                "accepted invalid manifest suffix: {suffix}"
            );
            let _ = std::fs::remove_dir_all(p);
        }
    }
    /// B0264: an icon slot resolved from the icon directories (tinted) and a kit badge pass
    /// `bead check` in a folder app; an unknown icon names itself in the error.
    #[test]
    fn check_accepts_tinted_icon_slots_and_kit_badges() {
        let p = folder(&format!("{}\n[layer]\nedge='top'\nsize=36\n", base()));
        let apps = p.join("data/icons/hicolor/scalable/apps");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join("bell.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#000"/></svg>"##,
        )
        .unwrap();
        // SAFETY: only the icon lookup reads XDG_DATA_DIRS in this process.
        unsafe { std::env::set_var("XDG_DATA_DIRS", p.join("data")) };
        std::fs::write(
            p.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"202020","onSurface":"eeeeee","primary":"90c0f0","onPrimary":"102030","secondary":"a0d0b0"}}"#,
        )
        .unwrap();
        std::fs::write(
            p.join("view/main.luau"),
            "local ui=require('bead.ui'); local kit=require('bead.kit')\nreturn ui.Box{id='slot',width=20,height=20,ui.Image{id='icon',icon='bell',width=16,height=16,tint='secondary'},kit.Badge{id='count',text='9',tone='primary'}}\n",
        )
        .unwrap();
        std::fs::write(
            p.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}\n",
        )
        .unwrap();
        App::load(&p).unwrap().check().unwrap();
        std::fs::write(
            p.join("view/main.luau"),
            "local ui=require('bead.ui'); return ui.Image{id='icon',icon='no-such-icon',width=16,height=16}\n",
        )
        .unwrap();
        let error = App::load(&p).unwrap().check().unwrap_err().to_string();
        assert!(error.contains("icon \"no-such-icon\""), "{error}");
        let _ = std::fs::remove_dir_all(p);
    }
    /// B0268: a bar with an image renders under the CPU renderer. A bare `FolderView`
    /// refused the renderer's CPU preparation, so `bead run` showed "Reload failed".
    #[test]
    fn layer_image_renders_through_the_cpu_preparation_path() {
        let p = folder(&format!("{}\n[layer]\nedge='top'\nsize=36\n", base()));
        std::fs::write(
            p.join("logo.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##,
        )
        .unwrap();
        std::fs::write(
            p.join("view/main.luau"),
            "local ui=require('bead.ui'); return ui.Image{id='logo',path='logo.svg',width='fill',height='fill'}\n",
        )
        .unwrap();
        std::fs::write(
            p.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}\n",
        )
        .unwrap();
        let app = App::load(&p).unwrap();
        let mut source = app.primary_factory(None)("test".into()).unwrap();
        let (sender, events) = std::sync::mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        let mut renderer = bead::render::renderer(RendererKind::Cpu).unwrap();
        // The host loop: staged scenes and finished preparation both arrive as events.
        let mut scene = source.stage(UiEvent::Reload).unwrap();
        let scene = loop {
            if let Some(scene) = &scene {
                match renderer.prepare_on_ui(scene, 1.) {
                    Ok(()) => break scene.clone(),
                    Err(error) => {
                        let pending = error
                            .downcast_ref::<bead::render::tree::CpuResourcesPending>()
                            .unwrap_or_else(|| panic!("unexpected render error: {error}"));
                        source.prepare_cpu(pending.request.clone()).unwrap();
                    }
                }
            }
            let event = events
                .recv_timeout(std::time::Duration::from_secs(10))
                .expect("source event");
            scene = source.stage(event).unwrap().or(scene);
        };
        let (w, h) = app.surface_size();
        let mut pixels = vec![0; (w * h * 4) as usize];
        renderer
            .render(
                &scene,
                bead::render::ShmTarget {
                    bytes: &mut pixels,
                    width: w,
                    height: h,
                    scale: 1.,
                    format: bead::render::PixelFormat::Rgba,
                },
                bead::render::Damage::full(w, h),
            )
            .unwrap();
        let at = ((8 * w + 8) * 4) as usize;
        assert_eq!(pixels[at..at + 3], [0x12, 0x34, 0x56], "image pixels");
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn check_does_not_start_declared_services_or_commands() {
        let p = folder(&format!(
            "{}\n[window]\nsize=[10,10]\n[services]\ndesktop=['clock']\n[commands.probe]\nargv=['touch','{}']\ninterval=1\n",
            base(),
            std::env::temp_dir()
                .join(format!("bead-check-command-{}", std::process::id()))
                .display()
        ));
        let marker =
            std::env::temp_dir().join(format!("bead-check-command-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        std::fs::write(
            p.join("view/main.luau"),
            "local ui=require('bead.ui'); return ui.Text{text='check'}\n",
        )
        .unwrap();
        std::fs::write(
            p.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}\n",
        )
        .unwrap();
        App::load(&p).unwrap().check().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            !marker.exists(),
            "bead check must not run declared commands"
        );
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn panels_need_a_bar_valid_names_sizes_and_contained_views() {
        let panel = "[panels.calendar]\nview='view/calendar.luau'\nsize=[320,240]\n";
        let p = folder(&format!(
            "{}\n[layer]\nedge='top'\nsize=36\n{panel}",
            base()
        ));
        std::fs::write(p.join("view/calendar.luau"), "return {}\n").unwrap();
        let app = App::load(&p).unwrap();
        assert_eq!(app.panel_actions(true), Some(Some(bead::core::Edge::Top)));
        assert_eq!(app.panel_actions(false), Some(None));
        let (panels, _) = app.panel_catalog(None, RendererKind::Cpu).unwrap().unwrap();
        assert_eq!(panels.default_name(), "calendar");
        for (suffix, message) in [
            (format!("[window]\nsize=[10,10]\n{panel}"), "require a [layer]"),
            (
                "[layer]\nedge='top'\nsize=36\n[panels.Bad]\nview='view/calendar.luau'\nsize=[1,1]\n"
                    .to_owned(),
                "invalid panel view name",
            ),
            (
                "[layer]\nedge='top'\nsize=36\n[panels.z]\nview='view/calendar.luau'\nsize=[0,1]\n"
                    .to_owned(),
                "size must be positive",
            ),
            (
                "[layer]\nedge='top'\nsize=36\n[panels.z]\nview='/etc/passwd'\nsize=[1,1]\n"
                    .to_owned(),
                "relative to the app folder",
            ),
            (
                "[layer]\nedge='top'\nsize=36\n[panels.z]\nview='view/calendar.luau'\nsize=[1,1]\nextra=1\n"
                    .to_owned(),
                "unknown field",
            ),
        ] {
            std::fs::write(p.join("bead.toml"), format!("{}\n{suffix}", base())).unwrap();
            let error = App::load(&p).unwrap_err().to_string();
            assert!(error.contains(message), "{suffix}: {error}");
        }
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn check_validates_every_panel_view_and_accepts_panel_actions() {
        let p = folder(&format!(
            "{}\n[layer]\nedge='top'\nsize=36\n[panels.calendar]\nview='view/calendar.luau'\nsize=[200,100]\n",
            base()
        ));
        std::fs::write(
            p.join("view/behavior.luau"),
            "return {stores={},actions={},computed={}}\n",
        )
        .unwrap();
        std::fs::write(
            p.join("view/main.luau"),
            "local ui=require('bead.ui'); return ui.Button{id='clock',text='12:34',width=60,height=24,on_activate=ui.action('panel.toggle','calendar')}\n",
        )
        .unwrap();
        std::fs::write(
            p.join("view/calendar.luau"),
            "local ui=require('bead.ui'); return ui.Button{id='close',text='Close',width=60,height=24,on_activate=ui.action('panel.close')}\n",
        )
        .unwrap();
        App::load(&p).unwrap().check().unwrap();
        std::fs::write(p.join("view/calendar.luau"), "return nil +\n").unwrap();
        let error = App::load(&p).unwrap().check().unwrap_err().to_string();
        assert!(error.contains("calendar.luau"), "{error}");
        let _ = std::fs::remove_dir_all(p);
    }
    #[test]
    fn popups_sit_against_the_bar_centred_and_clamped() {
        let size = (300., 200.);
        let out = (1280, 720);
        assert_eq!(
            panel_position(Edge::Top, 36, size, Some(640.), out),
            (490, 36)
        );
        assert_eq!(panel_position(Edge::Top, 36, size, Some(10.), out), (0, 36));
        assert_eq!(
            panel_position(Edge::Top, 36, size, Some(1275.), out),
            (980, 36)
        );
        assert_eq!(panel_position(Edge::Top, 36, size, None, out), (0, 36));
        assert_eq!(
            panel_position(Edge::Bottom, 36, size, Some(640.), out),
            (490, 484)
        );
        assert_eq!(
            panel_position(Edge::Left, 36, size, Some(360.), out),
            (36, 260)
        );
        assert_eq!(
            panel_position(Edge::Right, 36, size, Some(360.), out),
            (944, 260)
        );
    }
    #[test]
    fn rejects_paths_escaping_folder() {
        let p = folder(base());
        let escaped_name = format!("{}.outside.luau", p.file_name().unwrap().to_string_lossy());
        let outside = p.parent().unwrap().join(&escaped_name);
        std::fs::write(&outside, "return {}\n").unwrap();
        std::fs::write(
            p.join("bead.toml"),
            format!(
                "{}\nview='../{}'\n[window]\nsize=[10,10]\n",
                base(),
                escaped_name
            ),
        )
        .unwrap();
        let e = match App::load(&p) {
            Ok(_) => panic!("escaping path accepted"),
            Err(e) => e.to_string(),
        };
        assert!(e.contains("path escapes the app folder"));
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(p);
    }
}

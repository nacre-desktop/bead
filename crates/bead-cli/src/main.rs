// SPDX-License-Identifier: MIT
use bead::{
    host::{FolderServicesRuntime, FolderView, empty_scene},
    wayland::{Options, PrimaryFactory, UiEvent, WindowSpec},
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
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
struct App {
    root: PathBuf,
    manifest: Manifest,
}

fn load_source(
    root: PathBuf,
    view: PathBuf,
    palette: PathBuf,
    title: String,
    size: (u32, u32),
    runtime: Option<Arc<FolderServicesRuntime>>,
) -> Result<Box<dyn bead::wayland::SceneSource>, String> {
    let loaded = if let Some(runtime) = runtime {
        FolderView::load_with_runtime(root, view, palette, &title, size, runtime)
    } else {
        FolderView::load(root, view, palette, &title, size)
    };
    loaded
        .map(|view| Box::new(view) as Box<dyn bead::wayland::SceneSource>)
        .map_err(|error| error.to_string())
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
        let known = [
            "clock",
            "audio",
            "battery",
            "network",
            "bluetooth",
            "tray",
            "session",
            "media",
            "power",
            "bluetooth_devices",
            "network_devices",
        ];
        if let Some(services) = &manifest.services {
            for name in &services.desktop {
                if !known.contains(&name.as_str()) {
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
        Ok(Self { root, manifest })
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
    fn view(&self, size: (u32, u32)) -> Result<FolderView, Box<dyn Error>> {
        FolderView::load_with_services(
            self.root.clone(),
            self.manifest.view.clone(),
            self.manifest.palette.clone(),
            &self.manifest.title,
            size,
            self.folder_services(),
        )
        .map_err(|e| {
            format!(
                "{} / {}: {e}",
                self.root.display(),
                self.manifest.view.display()
            )
            .into()
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
        let mut view = self.view(size)?;
        view.check_render(UiEvent::Reload)
            .map_err(|e| format!("{}: {e}", self.manifest.view.display()))?;
        println!("OK {}", self.manifest.title);
        Ok(())
    }
    fn run(self, seconds: Option<f64>) -> Result<(), Box<dyn Error>> {
        let surface_size = self.surface_size();
        let runtime = FolderServicesRuntime::start(self.folder_services());
        let fallback = empty_scene(surface_size);
        let mut options = Options {
            namespace: self.manifest.id.clone(),
            seconds,
            started: Instant::now(),
            ..Default::default()
        };
        if let Some(w) = self.manifest.window {
            let app_id = self.manifest.id.clone();
            let title = self.manifest.title.clone();
            let root = self.root.clone();
            let view = self.manifest.view.clone();
            let palette = self.manifest.palette.clone();
            let size = (w.size[0], w.size[1]);
            let runtime = runtime.clone();
            let min = w.min_size.map(|s| (s[0], s[1])).unwrap_or((320, 240));
            options.window = Some(WindowSpec {
                app_id,
                title: title.clone(),
                command_targets: vec![],
                size,
                min_size: min,
                size_for_output: None,
                on_resize: None,
                fallback: fallback.clone(),
                factory: Arc::new(move |_| {
                    load_source(
                        root.clone(),
                        view.clone(),
                        palette.clone(),
                        title.clone(),
                        size,
                        runtime.clone(),
                    )
                }),
                model: Arc::new(|_| Default::default()),
            });
            options.app = Some(vec![]);
            options.interactive = true;
            options.keyboard_focus = true;
        } else if let Some(l) = self.manifest.layer {
            let factory: PrimaryFactory = Arc::new({
                let root = self.root.clone();
                let view = self.manifest.view.clone();
                let palette = self.manifest.palette.clone();
                let title = self.manifest.title.clone();
                let runtime = runtime.clone();
                move |_| {
                    load_source(
                        root.clone(),
                        view.clone(),
                        palette.clone(),
                        title.clone(),
                        surface_size,
                        runtime.clone(),
                    )
                }
            });
            options.primary_factory = Some(factory);
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
fn main() -> Result<(), Box<dyn Error>> {
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

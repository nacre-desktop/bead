// SPDX-License-Identifier: MIT
use bead::{
    host::{FolderView, empty_scene},
    wayland::{Options, PrimaryFactory, UiEvent, WindowSpec},
};
use serde::Deserialize;
use std::{
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
        Ok(Self { root, manifest })
    }
    fn view(&self, size: (u32, u32)) -> Result<FolderView, Box<dyn Error>> {
        FolderView::load(
            self.root.clone(),
            self.manifest.view.clone(),
            self.manifest.palette.clone(),
            &self.manifest.title,
            size,
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
                    FolderView::load(root.clone(), view.clone(), palette.clone(), &title, size)
                        .map(|v| Box::new(v) as Box<dyn bead::wayland::SceneSource>)
                        .map_err(|e| e.to_string())
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
                move |_| {
                    FolderView::load(
                        root.clone(),
                        view.clone(),
                        palette.clone(),
                        &title,
                        surface_size,
                    )
                    .map(|v| Box::new(v) as Box<dyn bead::wayland::SceneSource>)
                    .map_err(|e| e.to_string())
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
    use std::sync::atomic::{AtomicU64, Ordering};
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

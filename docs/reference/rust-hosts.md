# Rust hosts

Use Rust when you need a custom Wayland host, want to embed a Luau view in an existing application, or need to control surfaces and event mapping directly. A folder app using the `bead` CLI does not require Rust.

`bead::host::FolderView` loads a folder view, owns its UI `Session`, maps host events, and can CPU-render a check scene. The CLI combines it with `FolderServicesRuntime` and surface creation. This API is a reusable host seam; it does not encode app identity or installation policy. This is what `bead check examples/hello-bar` does:

```rust
use bead::{host::FolderView, wayland::UiEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::fs::canonicalize("examples/hello-bar")?;
    let size = (1280, 36);
    let mut view = FolderView::load(root, "view/main.luau".into(), "palette.json".into(), "Hello bar", size)?;
    let scene = view.check_render(UiEvent::Reload)?;
    println!("rendered {}x{}", scene.bounds.w, scene.bounds.h);
    Ok(())
}
```

To put the same bar on screen, give `bead::wayland::run` a bar factory. `Options` is `#[non_exhaustive]`, like `WindowSpec` and the service `Config` and snapshot types (spec 0262): start from `Options::default()` and assign fields, since a struct literal does not compile outside Bead:

```rust
use bead::{
    host::FolderView,
    render::Scene,
    wayland::{Options, SceneSource, worker::Source},
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::fs::canonicalize("examples/hello-bar")?;
    let size = (1280, 36);
    let mut options = Options::default();
    options.namespace = "org.example.HelloBar".into();
    options.bar = true;
    options.edge = bead::core::Edge::Top;
    options.bar_size = Some(size.1);
    options.primary_surface_viewport = true;
    options.interactive = true;
    options.seconds = Some(5.0);
    options.primary_factory = Some(Arc::new(move |_output: String| {
        let root = root.clone();
        let load = move || {
            FolderView::load(root, "view/main.luau".into(), "palette.json".into(), "Hello bar", size)
                .map(|view| Box::new(view) as Box<dyn SceneSource>)
                .map_err(|e| e.to_string())
        };
        let empty = Scene::empty(size.0 as f32, size.1 as f32);
        Ok(Box::new(Source::new(Box::new(load), empty)) as Box<dyn SceneSource>)
    }));
    bead::wayland::run(options, Scene::empty(size.0 as f32, size.1 as f32))
}
```

The factory runs once per output; `Source` moves the view's Luau and CPU image preparation onto a worker thread. A window is `options.window = Some(WindowSpec::new(app_id, title, size, fallback, factory))` with the optional fields (`min_size`, `size_for_output`, `on_resize`, `model`, `command_targets`) assigned afterwards, as [`examples/minimal-shell`](../../examples/minimal-shell/src/main.rs) does.

`FolderView::load_with_services` takes a `FolderServices` (desktop service names from `bead::host::DESKTOP_SERVICES`, plus named `CommandSpec`s) and declares their stores and actions; `FolderServicesRuntime::start` starts them once and `FolderView::load_with_runtime` attaches each view to that runtime. `with_panel_actions` enables the `panel.*` actions in `PANEL_ACTIONS`. `FolderView` implements `bead::wayland::SceneSource`; `bead::wayland::run` puts it on screen, as `crates/bead-cli/src/main.rs` shows. Windows and panels run their source on a worker thread for you; wrap a bar's source in `bead::wayland::worker::Source::new(factory, fallback)` from `Options::primary_factory` the same way, because the CPU renderer prepares a scene with images on that worker. A custom `SceneSource` may return `Some(revision)` from `state_revision`, changed whenever its accessibility tree, state summary or saved state can differ, so the worker reuses its last copies; the default `None` reads them for every worker packet. A panel closed but kept alive for a fast reopen receives `UiEvent::PanelVisibility(false)` in `stage`, and `PanelVisibility(true)` when it opens again: `FolderView` stops its service wakeups on `false` and resubscribes with a fresh snapshot on `true`, and a custom source holding its own subscriptions or leases should do the same.

`bead::ui::UiPackage::load(root, entry, palette)` reads a Luau UI root once (`root`, the `entry` view and the `palette` path, relative to `root`); `FolderView::from_package` / `from_package_with_runtime` build views from that copy without touching the filesystem, so a screen lock can add views after the installed files change ([spec 0285](../specs/0285-ui-package.md); `watch` is a no-op). A root whose `bundle.json` says `"plugins": true`, like `examples/hello-bar`'s, is refused with `plugins are not part of a UI package`, so the example below uses `examples/graph-demo`.

```rust
use bead::{host::FolderView, ui::UiPackage, wayland::UiEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::fs::canonicalize("examples/graph-demo")?;
    let package = UiPackage::load(&root, "view/main.luau".as_ref(), "palette.json".as_ref())?;
    // Later views (one per output, say) come from memory: the files may change or vanish.
    let mut view = FolderView::from_package(package.clone(), "Graph demo", (960, 640))?;
    let scene = view.check_render(UiEvent::Reload)?;
    println!("rendered {}x{} from the package", scene.bounds.w, scene.bounds.h);
    Ok(())
}
```

Start with [`examples/minimal-shell`](../../examples/minimal-shell/README.md) for a standalone Rust host using Bead alone. See [spec 0234](../specs/0234-general-host.md) for the folder host contract and [spec 0235](../specs/0235-folder-app-services.md) for services.

`bead::render::prewarm_shadows(&[(Rect, radius)], scale)` rasters the shadows of these surface boxes (the `shadow=true` box in logical px; its position is ignored) at that output scale and pins them, so the first frame of a shadowed surface (an OSD, a toast) needs no Gaussian. Each call replaces the previous set; pass every shape at once. It replaces the removed `prewarm_osd_shadows` (spec 0280).

## Edge lips and the panel tab

`ShellControl::edge_chrome` describes lips (small strips on a screen edge that open a named panel on hover dwell or click); `Options::edge_art` draws them as a static SVG per `LipState`. While the panel a lip opens is resident, docked on the lip's edge and local to Bead, the lip becomes that panel's tab: Bead draws it from the same callback at the same size and poses it against the panel body's inner edge in the same commit as the panel's pose (open, close, retarget and body width changes alike). The tab is a subsurface, so it may extend past the panel surface (a lip thicker than the 32 px shadow margin is fine). `LipState::attach` is 0 for the free lip, 1 when docked, and the panel's eased open progress in between; `LipState::hovered` reports the pointer on the lip or tab. Hovering the tab holds a hover-opened panel open (the panel profile's 150 ms `leave_ms` after leaving both, for an edge dock and a bar popup alike), clicking it closes the panel, and a press or key on the panel pins it. Construct `LipState` with `..Default::default()` for the rest state. While a tab exists the panel's motion runs on the client frame path (a subsurface cannot ride a compositor-driven atlas). See [spec 0279](../specs/0279-lip-tab.md).

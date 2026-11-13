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

`FolderView::load_with_services` takes a `FolderServices` (desktop service names from `bead::host::DESKTOP_SERVICES`, plus named `CommandSpec`s) and declares their stores and actions; `FolderServicesRuntime::start` starts them once and `FolderView::load_with_runtime` attaches each view to that runtime. `with_panel_actions` enables the `panel.*` actions in `PANEL_ACTIONS`. `FolderView` implements `bead::wayland::SceneSource`; `bead::wayland::run` puts it on screen, as `crates/bead-cli/src/main.rs` shows. Windows and panels run their source on a worker thread for you; wrap a bar's source in `bead::wayland::worker::Source::new(factory, fallback)` from `Options::primary_factory` the same way, because the CPU renderer prepares a scene with images on that worker. A custom `SceneSource` may return `Some(revision)` from `state_revision`, changed whenever its accessibility tree, state summary or saved state can differ, so the worker reuses its last copies; the default `None` reads them for every worker packet.

Start with [`examples/minimal-shell`](../../examples/minimal-shell/README.md) for a standalone Rust host using Bead alone. See [spec 0234](../specs/0234-general-host.md) for the folder host contract and [spec 0235](../specs/0235-folder-app-services.md) for services.

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

## Wallpaper crossfade

`Options::wallpaper_fade = Some(WallpaperFade { kind, duration_ms, curve })` fades a new wallpaper in on the GPU at display rate (`WallpaperFade::default()`: `Fade`, 700 ms, `in-out-cubic`; `None` keeps the cut). `WallpaperFadeKind::Grow { origin }` grows the new image as a soft-edged disc from `origin` (0..1 of the output). A provider may override kind, duration and curve for one change with `WallpaperProvider::fade(output)`; `None` uses the option. `WallpaperProvider::palette_only(output, revision)` (default `false`) says a frame's pixels equal what the output shows now and only the palette changed (a light/dark switch re-staging the current frame): the host keeps the shown wallpaper (no fade, no upload, `rgba` not read), calls `uploaded` and `committed` for the revision as a cut of identical pixels would, and eases the palette over the same `fade`/`wallpaper_fade` duration and curve, joining the tween other outputs of the change started and retargeting from the palette on screen if one runs. Reduced motion, no fade, a fade still running on the output, or a build without `native-gl` make it a cut (the palette fence, today's path). Reduced motion, missing hardware GL (a software adapter is refused; `BEAD_GL_ALLOW_SOFTWARE=1` is the private-host test knob) or a missing previous image is a cut, and a GL failure disables the fade for the process.

The previous image is read from the wallpaper buffer that is still attached, else from `prepare_transition`: a host that wants the fade after the idle buffer release implements it. `uploaded` fires with the first fade frame and `committed` right after it (`transition=true`); both textures are uploaded before the clock starts. A palette that comes with the wallpaper is applied in steps over the same duration (about one per 16.7 ms of it, 10 to 60) instead of through the palette fence. Each step runs right after a fade frame was submitted (a timer only backs it up when frames stop). `WALLPAPER_TRANSITION output= kind= frames= duration_ms= first_frame_ms= max_gap_ms= gap_at= render_top= palette_steps= gap_other_ms= setup= retarget_ms= clock_s= gap_peer_ms= gap_step_ms= step_max_ms= warm= start_ms= upload_wait_ms=` is printed when a fade ends (`palette_steps` counts the palette tween steps applied while it ran, 0 for an unchanged palette or the fence; `gap_other_ms` is the main-thread CPU time spent outside fade frames before the frame that ended the largest gap: close to `max_gap_ms` means our own repaints, near 0 means waiting (the compositor, the GPU or a blocking call: sleeping is not CPU time); `setup=read:,egl:,surface:,canvas:,upload:,first:` is the main-thread wall time of the fade start in ms (shown image read, EGL display and context, EGL surface, renderer and canvas, the two uploads, the held first pose's draw and present; `egl` is 0 when another output already made the context); `retarget_ms` the costliest retarget of the run, 0 if none; `clock_s` the second the fade's clock started, seconds since the host started: outputs of one change print the same value; `gap_peer_ms` and `gap_step_ms` split that largest gap's main-thread CPU into the partner outputs' fade frames and palette steps, host callback included, the rest is other work; `step_max_ms` the costliest single palette step of the run in wall ms; `warm=1` the fade started from a resident run, `start_ms` the main-thread wall time of the start or latest retarget, `upload_wait_ms` the wait from the accepted frame to its texture on the upload thread, 0 for a main-thread upload); `WALLPAPER_FADE_FINISH output= ms=` is the wall time of the fade's own finish (final buffer attach and GL release, or, warm, deleting `from`); a staged frame (`prepare_staged`) takes the same tween after its confirmation. [`wallpaper-fade`](../../crates/bead-wayland/examples/wallpaper-fade.rs) is a proof host.

`ShellControl::wallpaper_warm() -> bool` (default `false`, re-read after every change callback) keeps the fade's GL resident while it is true: every output with a wallpaper surface holds its EGL surface, canvas and the shown texture (about one output-sized RGBA texture per output at rest, two during a fade, plus the shared transport), a fade that ends does not attach the final SHM buffer (the EGL surface keeps showing the last frame), and a change starts with no shown-image read, transport, surface or canvas setup: one upload thread, on an EGL context shared with the transport's, uploads the new image and the fade starts when it is done. `committed` fires with the fade's first frame, as for a cold fade, so the provider contract is unchanged. When it turns false, a run at rest is released at once, a running fade releases when it ends (final buffer attach from the kept pixels, GL objects, upload thread, transport); an output removed, a GL failure and the host's shutdown release too. The host owns the guard: it decides how long the memory is worth holding. Without surfaceless EGL contexts or on an upload error the upload stays on the main thread (`WALLPAPER_FADE_FALLBACK reason=`). When it turns true, every output with a shown image and no run is primed (B0325): its resting run is built from the shown image, one output per idle loop turn, the texture uploaded by the upload thread, with nothing drawn, presented or reported to the provider, so the first change is already a warm one (`WALLPAPER_WARM_PRIME output= ms= upload_wait_ms= prepare_wait_ms=`; without a transport the EGL setup is prepared off the main thread first); an output with a run, or a frame arriving while its prime is in flight, is left to the normal path. Priming costs main-thread time per output, so a host should take the hold once its own client-frame motion has settled. `WALLPAPER_WARM state=on|off outputs= textures= bytes= ms=` is printed on each change (and again with nothing left when a release waited for a fade).

## Edge lips and the panel tab

`ShellControl::edge_chrome` describes lips (small strips on a screen edge that open a named panel on hover dwell or click); `Options::edge_art` draws them as a static SVG per `LipState`. While the panel a lip opens is resident, docked on the lip's edge and local to Bead, the lip becomes that panel's tab: Bead draws it from the same callback at the same size and poses it against the panel body's inner edge in the same commit as the panel's pose (open, close, retarget and body width changes alike). The tab is a subsurface, so it may extend past the panel surface (a lip thicker than the 32 px shadow margin is fine). `LipState::attach` is 0 for the free lip, 1 when docked, and the panel's eased open progress in between; `LipState::hovered` reports the pointer on the lip or tab. Hovering the tab holds a hover-opened panel open (the panel profile's 150 ms `leave_ms` after leaving both, for an edge dock and a bar popup alike), clicking it closes the panel, and a press or key on the panel pins it. Construct `LipState` with `..Default::default()` for the rest state. While a tab exists the panel's motion runs on the client frame path (a subsurface cannot ride a compositor-driven atlas). See [spec 0279](../specs/0279-lip-tab.md).

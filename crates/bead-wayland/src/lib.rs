// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor-neutral layer surfaces with fractional scaling and reusable SHM.
mod accessibility;
mod background;
mod clipboard;
mod decorations;
mod edges;
mod hidden;
pub mod intent;
mod panel;
mod panels;
pub use edges::{EdgeArt, EdgeChrome, LipState};
pub use panels::Panels;
mod policy;
mod protocols;
mod rendering;
mod report;
mod runtime;
mod transactions;
mod window;
use runtime::App;
pub use runtime::run_runtime;
pub(crate) use runtime::{output_event_is_current, output_scope_matches};
pub use window::{WindowFactory, WindowModel, WindowSpec};
pub mod worker;
use bead_render::{
    Damage, PixelFormat, Rect, RenderTarget, Renderer, RendererKind, Scene, ShmTarget,
};
use calloop::{
    EventLoop, LoopHandle, LoopSignal,
    signals::{Signal, Signals},
    timer::{TimeoutAction, Timer},
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::calloop_wayland_source::WaylandSource,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use std::{
    error::Error,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::registry_queue_init,
    protocol::{
        wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_subcompositor, wl_subsurface,
        wl_surface,
    },
};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::{self, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    presentation_time::client::{
        wp_presentation::{self, WpPresentation},
        wp_presentation_feedback::{self, WpPresentationFeedback},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};
#[derive(Debug)]
pub struct Remote {
    pub call: bead_services::ipc::Call,
    pub reply: std::sync::mpsc::SyncSender<bead_services::ipc::Reply>,
    pub context: CommandContext,
}
#[derive(Clone, Debug, Default)]
pub struct CommandContext {
    pub panel_open: bool,
    pub panel_configured: bool,
    pub panel_view: Option<String>,
    pub hidden: bool,
}
/// Output-sized pixels prepared by a product-owned passive wallpaper source.
pub struct WallpaperFrame {
    pub output: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub previous_rgba: Option<Vec<u8>>,
    pub palette: bead_render::Palette,
    /// Opaque product token keeps protocol-specific palette data paired with this raster.
    pub commit_token: std::sync::Arc<dyn std::any::Any + Send + Sync>,
}
impl std::fmt::Debug for WallpaperFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WallpaperFrame")
            .field("output", &self.output)
            .field("revision", &self.revision)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
pub trait WallpaperProvider: Send + Sync {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String>;
    fn prepare(&self, output: String, width: u32, height: u32) -> Result<WallpaperFrame, String>;
    fn prepare_transition(&self, _output: &str, _width: u32, _height: u32) -> Option<Vec<u8>> {
        None
    }
    fn is_current(&self, frame: &WallpaperFrame) -> bool;
    fn committed(
        &self,
        output: &str,
        revision: u64,
        token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
    );
    fn discarded(&self, output: &str, revision: u64);
    fn output_removed(&self, output: &str);
}
/// Product-owned switches the shell loop honours live: whether the bar exists, which
/// named panels may open, and a decoration variant that rebuilds the static tiles.
pub trait ShellControl: Send + Sync {
    /// Register the callback to run (from any thread) after any answer changed.
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>);
    fn bar_enabled(&self) -> bool {
        true
    }
    fn panel_allowed(&self, _view: &str) -> bool {
        true
    }
    fn decoration_variant(&self) -> u64 {
        0
    }
    /// Bar thickness along its edge, also its exclusive zone; `None` keeps the default.
    fn bar_thickness(&self) -> Option<u32> {
        None
    }
    /// Reserved screen edges and lips (B0047). Re-read after every change callback.
    fn edge_chrome(&self) -> EdgeChrome {
        EdgeChrome::default()
    }
    /// Whether hovering may open a lip's panel now; `open_panel` is the open panel's name.
    fn edge_hover_allowed(&self, _open_panel: Option<&str>) -> bool {
        true
    }
}
#[derive(Clone, Debug)]
pub enum UiEvent {
    /// A [`ShellControl`] answer changed.
    ControlChanged,
    /// Event delivered to one output-owned UI session. The epoch rejects callbacks
    /// queued by a session after that output has been removed.
    Output(u64, String, Box<UiEvent>),
    /// The compositor may publish an output name after its initial announcement.
    OutputName(String),
    Panel(u64, Box<UiEvent>),
    /// Event for the on-demand window session (epoch rejects a closed window's late wakeups).
    Window(u64, Box<UiEvent>),
    PanelVisibility(bool),
    CompanionVisibility(bool),
    /// Shell-wide hide: `None` toggles. Wallpaper surfaces are never affected.
    ShellHidden(Option<bool>),
    PointerRegion(bool),
    PanelDismissed,
    PanelSelected(String),
    CompanionSelect {
        view: String,
        output: Option<String>,
        toggle: bool,
        center: Option<f32>,
    },
    /// Service-initiated ambient view (transient notices). `open` shows `view` only
    /// while the panel slot is idle or holds another ambient view, so it never
    /// displaces a view the user opened; `!open` closes `view` only if it is the
    /// ambient view shown. The application defines view names.
    CompanionOffer {
        view: String,
        output: Option<String>,
        open: bool,
    },
    CompanionModel(std::collections::BTreeMap<String, bead_core::Value>),
    ClipboardSet(String),
    ClipboardRead,
    ClipboardText(String),
    Remote(std::sync::Arc<Remote>),
    Fatal(String),
    Drag {
        x: f32,
        y: f32,
    },
    /// The primary pointer button was released.
    Release,
    Reload,
    /// Deliberately block the script executor for a bounded diagnostic interval.
    DebugScriptStall(u64),
    WallpaperFrame(std::sync::Arc<WallpaperFrame>),
    WallpaperRefresh,
    WallpaperFailed(String),
    WallpaperTick(String),
    WallpaperPalette(std::collections::BTreeMap<String, [u8; 4]>),
    Worker(std::sync::Arc<worker::Packet>),
    Desktop(std::sync::Arc<bead_services::Snapshot>),
    Model(std::collections::BTreeMap<String, bead_core::Value>),
    Backend(std::sync::Arc<bead_backends::Snapshot>),
    Relayout,
    Timer {
        token: u64,
    },
    Hover {
        x: f32,
        y: f32,
    },
    Command {
        command: String,
        shift: bool,
    },
    Accessibility {
        target: u64,
        action: String,
        value: Option<String>,
    },
    Ime {
        preedit: String,
        commit: Option<String>,
        before: u32,
        after: u32,
    },
    WatchError(String),
    SecondaryClick {
        x: f32,
        y: f32,
    },
    ContextMenu {
        x: f32,
        y: f32,
    },
    Click {
        x: f32,
        y: f32,
    },
    HorizontalScroll {
        x: f32,
        y: f32,
        delta: f64,
    },
    Scroll {
        x: f32,
        y: f32,
        delta: f64,
    },
    Key {
        text: String,
        backspace: bool,
    },
}
/// Candidate state is committed only after render resources have been prepared.
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
#[derive(Debug)]
struct GpuUnavailable(String);
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
impl std::fmt::Display for GpuUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
impl Error for GpuUnavailable {}
#[derive(Clone)]
pub struct InputRegion {
    pub id: String,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub cursor: &'static str,
    pub dismiss: bool,
    /// Start bounded preparation when the pointer enters this host-selected region.
    pub prepare_panel: bool,
    /// Optional configured view; None retains the default preparation target.
    pub prepare_view: Option<String>,
    pub prepare_center: Option<f32>,
}
pub trait SceneSource {
    /// Successful staging without a scene schedules a later worker event.
    fn deferred_updates(&self) -> bool {
        false
    }
    /// Stage a product-provided presentation palette through the normal scene transaction.
    fn wallpaper_palette(
        &mut self,
        palette: bead_render::Palette,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        self.stage(UiEvent::WallpaperPalette(
            palette
                .0
                .iter()
                .map(|(key, value)| (key.clone(), *value))
                .collect(),
        ))
    }
    fn wallpaper_palette_pending(&self) -> bool {
        false
    }
    /// Invocation coordinate along the panel's edge, in output logical pixels.
    fn panel_center(&mut self, _center: Option<f32>) {}
    fn set_idle(&mut self, _idle: bool) {}
    fn focused_dismiss(&self) -> bool {
        false
    }
    fn dismiss_at(&self, x: f32, y: f32) -> bool {
        self.input_regions().iter().any(|r| {
            r.dismiss
                && x >= r.rect.x
                && y >= r.rect.y
                && x < r.rect.x + r.rect.w
                && y < r.rect.y + r.rect.h
                && r.clip
                    .is_none_or(|c| x >= c.x && y >= c.y && x < c.x + c.w && y < c.y + c.h)
        })
    }
    fn scales(&mut self, _: Vec<f32>) {}
    fn input_regions(&self) -> Vec<InputRegion> {
        Vec::new()
    }

    fn services(&mut self, _wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {}
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        None
    }
    fn pointer_cursor(&self, _x: f32, _y: f32) -> &str {
        "default"
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        None
    }
    /// The focused input is secret: the input method gets the password purpose and no context.
    fn sensitive_input(&self) -> bool {
        false
    }
    fn viewport(&mut self, _width: u32, _height: u32) -> bool {
        false
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>>;
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>>;
    fn validate(&self) -> Result<(), Box<dyn Error>>;
    fn state_summary(&self) -> String;
    fn resource_counts(&self) -> String {
        String::new()
    }
    fn commit(&mut self) -> bool;
    /// Delivered once per generation after successful presentation, off-thread for worker sources.
    fn presented(&mut self, _revision: u64) {}
    fn error_scene(&mut self, error: String) -> Scene;
}
pub type CommandMap = std::sync::Arc<
    dyn Fn(bead_services::ipc::Call) -> Result<bead_services::ipc::Call, bead_services::ipc::Reply>
        + Send
        + Sync,
>;
pub type PanelFactory =
    std::sync::Arc<dyn Fn(bool) -> Result<Box<dyn SceneSource>, String> + Send + Sync>;
pub type PrimaryFactory =
    std::sync::Arc<dyn Fn(String) -> Result<Box<dyn SceneSource>, String> + Send + Sync>;
/// Small static, input-transparent vector tiles outside a bar's own rectangle.
/// The application supplies policy; Wayland only owns bounded SHM/subsurfaces.
pub struct Decoration {
    pub rect: Rect,
    pub svg: std::sync::Arc<[u8]>,
}
#[derive(Clone, Copy)]
pub struct DecorationPanel<'a> {
    pub scene: &'a Scene,
    pub origin: (i32, i32),
}
pub type DecorationFactory = std::sync::Arc<
    dyn for<'a> Fn((u32, u32), bool, Option<DecorationPanel<'a>>) -> Result<Vec<Decoration>, String>
        + Send
        + Sync,
>;
/// Application-owned surface origin in output logical coordinates. The framework
/// does not know a shell's frame, bar, shadow insets or attachment policy.
pub type PanelPositioner =
    std::sync::Arc<dyn Fn(&Scene, Option<f32>, (i32, i32)) -> (i32, i32) + Send + Sync>;
#[derive(Clone)]
pub struct PanelSpec {
    /// Application-selected active decoration state; no view names in the framework.
    pub engages_decorations: bool,
    /// Passive notices never create a dismissal layer or request keyboard focus.
    pub passive: bool,
    /// Request keyboard focus and an outside-dismiss layer while the panel is open.
    pub keyboard_focus: bool,
    pub initial_open: bool,
    pub factory: PanelFactory,
    pub fallback: Scene,
    pub renderer: RendererKind,
    pub position: Option<PanelPositioner>,
}
#[derive(Clone)]
pub struct Options {
    /// Optional per-output primary UI source. When absent the legacy shared UI is used.
    pub primary_factory: Option<PrimaryFactory>,
    pub decorations: Option<DecorationFactory>,
    /// Artwork for the lips [`ShellControl::edge_chrome`] describes.
    pub edge_art: Option<EdgeArt>,
    /// Standard compositor blur, only if advertised with the blur capability.
    pub background_blur: bool,
    pub namespace: String,
    pub panel_spec: Option<PanelSpec>,
    /// Optional named views sharing the existing one-resident-panel lifecycle.
    pub panels: Option<Panels>,
    pub command_map: Option<CommandMap>,
    pub report: Option<std::path::PathBuf>,
    pub command_socket: Option<std::path::PathBuf>,
    pub animate: bool,
    pub debug_continuous: bool,
    pub debug_progress: bool,
    pub debug_luau_stall_ms: Option<u64>,
    pub damage: bool,
    pub force_argb: bool,
    pub seconds: Option<f64>,
    pub bar: bool,
    pub edge: bead_core::Edge,
    pub panel: bool,
    pub content_size: bool,
    pub renderer: RendererKind,
    pub output: Option<String>,
    pub auto_fallback: bool,
    pub lazy_gl: bool,
    pub debug_policy_cycle: bool,
    pub started: Instant,
    pub start_open: bool,
    pub memory_usage: bool,
    pub surface_count: usize,
    /// Distinct passive companion scene, sharing this process and GPU transport.
    pub companion: Option<(Scene, RendererKind)>,
    pub interactive: bool,
    pub keyboard_focus: bool,
    pub test_keyboard: bool,
    /// Click-through proof surfaces that reserve no desktop space.
    pub test_passive: bool,
    pub trace_state: bool,
    /// Optional product-owned wallpaper consumer; the framework owns passive surfaces.
    pub wallpaper: Option<std::sync::Arc<dyn WallpaperProvider>>,
    pub control: Option<std::sync::Arc<dyn ShellControl>>,
    /// On-demand xdg toplevel window (`bead.window open|close`).
    pub window: Option<WindowSpec>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            primary_factory: None,
            decorations: None,
            edge_art: None,
            background_blur: false,
            namespace: "bead-dev".into(),
            panel_spec: None,
            panels: None,
            command_map: None,
            command_socket: None,
            report: None,
            animate: false,
            debug_continuous: false,
            debug_progress: false,
            debug_luau_stall_ms: None,
            damage: true,
            force_argb: false,
            seconds: None,
            bar: false,
            edge: Default::default(),
            panel: false,
            content_size: false,
            renderer: RendererKind::Cpu,
            output: None,
            auto_fallback: false,
            lazy_gl: false,
            debug_policy_cycle: false,
            started: Instant::now(),
            start_open: false,
            memory_usage: false,
            surface_count: 1,
            companion: None,
            interactive: false,
            keyboard_focus: true,
            test_keyboard: false,
            test_passive: false,
            trace_state: false,
            wallpaper: None,
            control: None,
            window: None,
        }
    }
}
#[derive(Clone)]
struct FrameData {
    surface: wl_surface::WlSurface,
    epoch: u64,
    generation: u64,
    source_epoch: u64,
}
struct PresentationData {
    source_epoch: u64,
    update: u64,
    surface: wl_surface::WlSurface,
    generation: u64,
}
struct Reusable {
    buffer: Buffer,
    pending: Vec<Damage>,
    painted: bool,
}
struct Surface {
    decorations: Option<decorations::Tiles>,
    background:Option<wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    background_size:Option<(i32,i32)>,
    logical_size:(u32,u32),
    background_visible:bool,
    companion: bool,
    output: wl_output::WlOutput,
    output_name: String,
    layer: LayerSurface,
    pool: SlotPool,
    #[cfg(feature = "wgpu")]
    gpu: Option<bead_render::gpu::Surface>,
    #[cfg(feature = "native-gl")]
    gl: Option<bead_render::gl::Surface>,
    buffers: Vec<Reusable>,
    width: u32,
    height: u32,
    scale120: u32,
    configured: bool,
    ready: bool,
    callbacks: policy::Callbacks,
    input_visible: Option<bool>,
    input_rect: Option<[i32; 4]>,
    keyboard_visible: Option<bool>,
    redraw: bool,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    geometry: (i32, i32),
    offset: (f32, f32),
    format: PixelFormat,
    previous: Option<Rect>,
    visual_damage: bead_render::damage::FrameDamage,
    allocations: usize,
    shm_trim_pending: bool,
    heap_trimmed_ui_serial: u64,
    frames: usize,
    render_ms: Vec<f64>,
    frame_ms: Vec<f64>,
    acquire_ms: Vec<f64>,
    swap_ms: Vec<f64>,
    activate_ms: Vec<f64>,
    callbacks_ms: Vec<f64>,
    last_callback: Option<Instant>,
    presented_ms: Vec<f64>,
    last_presented: Option<u64>,
    refresh_ns: u32,
    presentation_kind: u32,
    discarded: usize,
    presented_generation: Option<u64>,
    recovered_generation: Option<(u64, u64)>,
    ui_serial: u64,
}
struct WallpaperSurface {
    output: wl_output::WlOutput,
    name: String,
    width: u32,
    height: u32,
    revision: u64,
    release_wait_ms: u32,
    release_timer_pending: bool,
    scale_factor: i32,
    layer: LayerSurface,
    pool: SlotPool,
    buffers: Vec<Buffer>,
    viewport: Option<WpViewport>,
    viewport_destination: Option<(i32, i32)>,
    pixels: Option<Vec<u8>>,
    transition: Option<WallpaperTransition>,
}
struct WallpaperTransition {
    pixels: Vec<u8>,
    target: Vec<u8>,
    remaining: u8,
}
impl Drop for Surface {
    fn drop(&mut self) {
        if let Some(effect) = self.background.take() {
            println!("BACKGROUND_EFFECT destroyed={}", effect.id().protocol_id());
            effect.destroy();
        }
        #[cfg(feature = "wgpu")]
        drop(self.gpu.take());
        #[cfg(feature = "native-gl")]
        drop(self.gl.take());
        if let Some(f) = &self.fractional {
            f.destroy();
        }
        if let Some(v) = &self.viewport {
            v.destroy();
        }
    }
}
pub fn run(options: Options, scene: Scene) -> Result<(), Box<dyn Error>> {
    let renderer = bead_render::renderer(options.renderer)?;
    run_with_renderer(options, scene, renderer)
}
/// Shell widgets supply Scene commands; they never call the rasterizer.
pub fn run_with_renderer(
    options: Options,
    scene: Scene,
    renderer: Box<dyn Renderer>,
) -> Result<(), Box<dyn Error>> {
    run_runtime(options, scene, renderer, None)
}
fn monotonic_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: initialized writable timespec for CLOCK_MONOTONIC.
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}
fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    let mut v = samples.to_vec();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).ceil() as usize]
}
fn buffer_dimensions(
    w: u32,
    h: u32,
    scale120: u32,
) -> Result<(i32, i32, i32, usize), Box<dyn Error>> {
    if w == 0 || h == 0 || scale120 == 0 {
        return Err("empty geometry or scale".into());
    }
    // Ceil ensures every edge pixel is covered; viewporter maps to logical size.
    let w = (u64::from(w) * u64::from(scale120)).div_ceil(120);
    let h = (u64::from(h) * u64::from(scale120)).div_ceil(120);
    let stride = w.checked_mul(4).ok_or("stride overflow")?;
    let size = stride.checked_mul(h).ok_or("size overflow")?;
    if size > 64 * 1024 * 1024 {
        return Err("surface buffer exceeds 64 MiB".into());
    }
    Ok((
        i32::try_from(w)?,
        i32::try_from(h)?,
        i32::try_from(stride)?,
        usize::try_from(size)?,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_dimensions_cover_edges_without_integer_inflation() {
        assert_eq!(
            buffer_dimensions(1920, 1200, 180).unwrap(),
            (2880, 1800, 11520, 20736000)
        );
        assert_eq!(
            buffer_dimensions(641, 719, 180).unwrap(),
            (962, 1079, 3848, 4151992)
        );
    }
    #[test]
    fn rejects_unbounded_geometry() {
        for (w, h, s) in [
            (0, 32, 120),
            (512, 0, 120),
            (512, 32, 0),
            (u32::MAX, 32, 240),
            (3840, 2160, 960),
        ] {
            assert!(buffer_dimensions(w, h, s).is_err());
        }
    }
}

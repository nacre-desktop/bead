// SPDX-License-Identifier: MIT OR Apache-2.0
//! Compositor-neutral layer surfaces with fractional scaling and reusable SHM.
mod accessibility;
mod background;
mod buffer_damage;
mod canvas;
mod capture;
pub use capture::{
    CaptureHub, Lease as CaptureLease, Preview as CapturePreview, Wake as CaptureWake,
    Window as CaptureWindow,
};
mod clipboard;
mod decorations;
mod edges;
mod hidden;
pub mod intent;
mod layer_role;
mod panel;
mod panels;
mod pointer_motion;
pub use edges::{EdgeArt, EdgeChrome, LipState};
mod pearls;
pub use panels::Panels;
pub use pearls::{
    Film, Guide, NativePearlComplete, NativePearlMotion, NativePearlOptions, NativePearlPlacement,
    NativePearlPrewarm, NativePearlPrewarmComplete, NativePearlTitle, NativePearlWatch,
    PearlOptions,
};
mod overlay;
mod policy;
mod protocols;
mod rendering;
mod report;
mod runtime;
mod scroll;
mod transactions;
mod window;
pub use overlay::Overlays;
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
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
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
/// Where an app process learns the live presentation palette (role to RGBA).
pub trait PaletteFeed: Send + Sync {
    fn current(&self) -> Option<std::collections::BTreeMap<String, [u8; 4]>>;
    /// Call `changed` (from any thread) with each new palette.
    fn watch(
        &self,
        changed: Box<dyn Fn(std::collections::BTreeMap<String, [u8; 4]>) + Send + Sync>,
    );
}
pub trait WallpaperProvider: Send + Sync {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) -> Result<(), String>;
    fn prepare(&self, output: String, width: u32, height: u32) -> Result<WallpaperFrame, String>;
    /// `Some` preserves legacy return behavior without calling `staged`; `None`
    /// requires one staged callback and successful confirmation. `Err` aborts it.
    fn prepare_staged(
        &self,
        output: String,
        width: u32,
        height: u32,
        _staged: &mut dyn FnMut(WallpaperFrame),
    ) -> Result<Option<WallpaperFrame>, String> {
        Ok(Some(self.prepare(output, width, height)?))
    }
    fn prepare_transition(&self, _output: &str, _width: u32, _height: u32) -> Option<Vec<u8>> {
        None
    }
    fn is_current(&self, frame: &WallpaperFrame) -> bool;
    /// The output buffer has been attached and populated, immediately before surface commit.
    fn uploaded(
        &self,
        _output: &str,
        _revision: u64,
        _token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
    ) {
    }
    fn committed(
        &self,
        output: &str,
        revision: u64,
        token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
    );
    /// The compositor reported presentation of this wallpaper revision.
    fn presented(&self, _output: &str, _revision: u64) {}
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
    /// The committed presentation palette changed (frame art follows it); bump the variant when it matters.
    fn palette_changed(&self, _palette: &std::collections::BTreeMap<String, [u8; 4]>) {}
    /// Pearls (detached panels) at all.
    fn pearls_enabled(&self) -> bool {
        true
    }
    /// Whether this view may be detached into a pearl.
    fn pearl_allowed(&self, _view: &str) -> bool {
        true
    }
    /// Whether the pull-out gesture (Super + drag) is on.
    fn pearl_drag(&self) -> bool {
        true
    }
    /// Bar thickness along its edge, also its exclusive zone; `None` keeps the default.
    fn bar_thickness(&self) -> Option<u32> {
        None
    }
    /// Reserved screen edges and lips (B0047). Re-read after every change callback.
    fn edge_chrome(&self) -> EdgeChrome {
        EdgeChrome::default()
    }
    /// Keep these output-space input regions reachable through outside-click backdrops.
    fn edge_input_regions(&self, output: (i32, i32)) -> Vec<Rect> {
        self.edge_chrome().input_regions(output)
    }
    /// An externally hosted panel: None means the local panel registry owns it.
    fn external_panel_open(&self, _view: &str, _output: &str) -> Option<bool> {
        None
    }
    /// Request an external target through its product adapter (open or toggle).
    fn external_panel_request(&self, _view: &str, _output: &str, _toggle: bool) {}
    /// The lip participates in its external panel's hover region during grace.
    fn external_panel_hover(&self, _view: &str, _output: &str, _inside: bool) {}
    /// Whether hovering may open a lip's panel now; `open_panel` is the open panel's name.
    fn edge_hover_allowed(&self, _open_panel: Option<&str>) -> bool {
        true
    }
}
#[derive(Clone, Debug)]
pub enum UiEvent {
    /// A [`ShellControl`] answer changed.
    ControlChanged,
    /// A new live palette from the options' palette feed.
    LivePalette(std::collections::BTreeMap<String, [u8; 4]>),
    /// Event delivered to one output-owned UI session. The epoch rejects callbacks
    /// queued by a session after that output has been removed.
    Output(u64, String, Box<UiEvent>),
    /// The compositor may publish an output name after its initial announcement.
    OutputName(String),
    Panel(u64, Box<UiEvent>),
    /// Event for the on-demand window session (epoch rejects a closed window's late wakeups).
    Window(u64, Box<UiEvent>),
    /// Event for the overlay slot (epoch rejects a released overlay's late wakeups).
    Overlay(u64, Box<UiEvent>),
    /// Event delivered to one lazily-created canvas; stale epochs are ignored.
    Canvas(u64, Box<UiEvent>),
    PanelVisibility(bool),
    /// The panel was detached into a pearl (true) or docked back (false).
    PanelDetached(bool),
    /// Drag guidance for a guide view.
    PearlGuide(std::sync::Arc<Guide>),
    /// Restore opaque view-owned state after a remembered pearl is recreated.
    Restore(serde_json::Value),
    /// Geometry/move state from a compositor adapter for a native pearl.
    NativePearlMotion(NativePearlMotion),
    /// Completion of an adapter's initial native placement, scoped to its panel lifetime.
    NativePearlPlaced {
        id: u32,
        epoch: u64,
        rect: bead_core::pearl::Rect,
        success: bool,
    },
    /// Completion of the hidden fixed-anchor prewarm; generation rejects a late
    /// result after a size change during the drag.
    NativePearlPrewarmed {
        id: u32,
        epoch: u64,
        generation: u64,
        rect: bead_core::pearl::Rect,
        anchor: Option<bead_core::pearl::Rect>,
    },
    CompanionVisibility(bool),
    /// Shell-wide hide: `None` toggles. Wallpaper surfaces are never affected.
    ShellHidden(Option<bool>),
    PointerRegion(bool),
    /// Hover origin and anchor contact; views choose how to project this lifecycle input.
    PanelHover {
        opened: Option<bool>,
        anchor: bool,
    },
    PanelDismissed,
    PanelSelected(String),
    /// Update the active view name without replacing the panel surface or worker.
    PanelNavigated(String),
    CompanionSelect {
        view: String,
        output: Option<String>,
        toggle: bool,
        center: Option<f32>,
    },
    CompanionSelectAt {
        view: String,
        output: Option<String>,
        anchor: PanelAnchor,
    },
    /// Service-initiated ambient view (transient notices). A view of the overlay
    /// catalog ([`Options::overlays`]) shows in the overlay slot beside whatever the
    /// panel shows. Any other `open` shows `view` only while the panel slot is idle or
    /// holds another ambient view, so it never displaces a view the user opened;
    /// `!open` closes `view` only if it is the ambient view shown. The application
    /// defines view names.
    CompanionOffer {
        view: String,
        output: Option<String>,
        open: bool,
    },
    /// Create or release one output-sized desktop canvas from the configured view catalog.
    CanvasOffer {
        view: String,
        output: Option<String>,
        open: bool,
        values: std::collections::BTreeMap<String, bead_core::Value>,
    },
    /// Prepare a canvas scene and renderer, with optional empty-layer source geometry.
    CanvasPrepare {
        view: String,
        output: Option<String>,
        values: std::collections::BTreeMap<String, bead_core::Value>,
        initial_bounds: Option<bead_render::Rect>,
    },
    CanvasModel {
        view: String,
        output: Option<String>,
        values: std::collections::BTreeMap<String, bead_core::Value>,
    },
    CompanionModel(std::collections::BTreeMap<String, bead_core::Value>),
    /// A child view projects durable edit state onto its output's primary view.
    HostModel(std::collections::BTreeMap<String, bead_core::Value>),
    /// Dispatch a view-authored action without synthesizing pointer/keyboard input.
    Action {
        name: String,
        argument: bead_core::Value,
    },
    CompanionAction {
        name: String,
        argument: bead_core::Value,
    },
    /// Latest-only authored action motion; ordered Action events remain barriers.
    MotionAction {
        name: String,
        argument: bead_core::Value,
    },
    /// Companion-host motion forwarded to the host's serialized action worker.
    CompanionMotionAction {
        name: String,
        argument: bead_core::Value,
    },
    ClipboardSet(String),
    ClipboardRead,
    ClipboardText(String),
    Remote(std::sync::Arc<Remote>),
    Fatal(String),
    Drag {
        x: f32,
        y: f32,
    },
    /// A retained range was painted locally; the source may update its value without layout.
    LocalDrag {
        x: f32,
        y: f32,
    },
    /// The primary pointer button was released.
    Release,
    Reload,
    /// Deliberately block the script executor for a bounded diagnostic interval.
    DebugScriptStall(u64),
    WallpaperFrame(WallpaperFrameEvent),
    WallpaperRefresh,
    WallpaperFailed(String),
    WallpaperTick(String),
    WallpaperPalette(std::collections::BTreeMap<String, [u8; 4]>),
    Worker(std::sync::Arc<worker::Packet>),
    /// Background image upload preparation completed; redraw accepted content.
    ResourcesPrepared(Result<(), String>),
    RetireResources(std::sync::Arc<worker::ResourceRetirement>),
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

#[derive(Clone)]
pub struct WallpaperFrameConfirmation {
    pub output: String,
    pub revision: u64,
    pub commit_token: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    pub accepted: bool,
}
impl std::fmt::Debug for WallpaperFrameConfirmation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WallpaperFrameConfirmation({}, {}, {})",
            self.output, self.revision, self.accepted
        )
    }
}
#[derive(Clone, Debug)]
pub enum WallpaperFrameEvent {
    Ready(std::sync::Arc<WallpaperFrame>),
    Staged(std::sync::Arc<WallpaperFrame>),
    Confirmation(WallpaperFrameConfirmation),
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
    /// Include transparent control hit areas in a content-sized surface's allocation.
    pub include_in_bounds: bool,
    /// Optional normalized even-odd polygon, shared with the rendered control.
    pub hit_polygon: Option<Vec<[f32; 2]>>,
    pub id: String,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub cursor: &'static str,
    /// Permit configured edge dwell while retaining the region's normal cursor.
    pub edge_hover: bool,
    pub dismiss: bool,
    /// Start bounded preparation when the pointer enters this host-selected region.
    pub prepare_panel: bool,
    /// Optional configured view; None retains the default preparation target.
    pub prepare_view: Option<String>,
    pub prepare_center: Option<f32>,
}
impl InputRegion {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect
            .hit_test(x, y, self.clip, self.hit_polygon.as_deref())
    }
}
/// A view-authored visual whose horizontal pose follows an absolute pointer.
/// Geometry is in layout coordinates; the input range is independent of its painted size.
#[derive(Clone)]
pub struct DragVisual {
    pub id: String,
    pub grab: Rect,
    pub travel: (f32, f32),
    pub pointer: (f32, f32),
    pub fill: Option<DragFill>,
    pub local_feedback: bool,
    /// Source-owned accepted release count, used for atomic visual handover.
    pub release_serial: u64,
}
#[derive(Clone, PartialEq)]
pub struct DragFill {
    pub id: String,
    pub rect: Rect,
    pub minimum: f32,
    pub radius: f32,
    pub gradient_min: f32,
    pub from: [u8; 4],
    pub to: [u8; 4],
}
pub trait SceneSource {
    fn drag_visuals(&self) -> Vec<DragVisual> {
        Vec::new()
    }
    /// Seed a lazy source before its first reload or presentation.
    fn initial_model(
        &mut self,
        _values: std::collections::BTreeMap<String, bead_core::Value>,
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
    fn panel_anchor(&mut self, _anchor: Option<PanelAnchor>) {}
    /// Optional local-state deadline. Workers wake a source without polling idle views.
    fn wakeup_at(&self) -> Option<std::time::Instant> {
        None
    }
    /// Set the display frame interval used by authored animations on this source.
    fn frame_interval(&mut self, _interval: Duration) {}

    /// Whether configured edge hover may start over this region.
    fn allows_edge_hover(&self, x: f32, y: f32) -> bool {
        self.pointer_cursor(x, y) == "default"
    }
    /// Successful staging without a scene schedules a later worker event.
    fn deferred_updates(&self) -> bool {
        false
    }
    /// Refill consumed GPU upload payloads outside the presentation thread.
    fn prepare_images(
        &mut self,
        _images: Vec<std::sync::Arc<bead_render::tree::ImagePayload>>,
    ) -> Result<(), Box<dyn Error>> {
        Err("asynchronous image preparation is unavailable".into())
    }
    fn prepare_cpu(
        &mut self,
        _request: std::sync::Arc<bead_render::tree::CpuPreparation>,
    ) -> Result<(), Box<dyn Error>> {
        Err("asynchronous CPU preparation is unavailable".into())
    }
    fn resources_retired(&self) -> bool {
        false
    }
    fn preparation_retry_pending(&self) -> bool {
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
    /// Content viewports; other wheel targets keep their original single gesture event.
    fn scroll_regions(&self) -> Vec<Rect> {
        Vec::new()
    }
    /// Logical content distance for one wheel notch; pixel-axis input stays precise.
    fn wheel_step(&self) -> f64 {
        600.
    }
    /// Whether outside pointer presses may dismiss this surface.
    fn outside_dismiss(&self) -> bool {
        true
    }
    /// Opaque state saved alongside a remembered pearl.
    fn saved_state(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    fn focused_dismiss(&self) -> bool {
        false
    }
    fn dismiss_at(&self, x: f32, y: f32) -> bool {
        self.input_regions()
            .iter()
            .rev()
            .find(|r| r.contains(x, y))
            .is_some_and(|r| r.dismiss)
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
    /// Opt into output-space origin stores for authored cross-surface drop targets.
    fn tracks_origin(&self) -> bool {
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
/// Optional output-sized snapshot used while a lazy panel builds its first live scene.
pub type PanelFallbackFactory =
    std::sync::Arc<dyn Fn((u32, u32)) -> Option<bead_render::Scene> + Send + Sync>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelAnchor {
    pub rect: Rect,
    pub edge: bead_core::Edge,
}
#[derive(Clone)]
pub struct PanelSpec {
    /// Application-selected active decoration state; no view names in the framework.
    pub engages_decorations: bool,
    /// Place above application fullscreen surfaces (layer-shell overlay).
    pub overlay: bool,
    /// Keep this single resident view prepared until another view replaces it.
    pub retain_when_closed: bool,
    /// Passive notices never create a dismissal layer or request keyboard focus.
    pub passive: bool,
    /// Request keyboard focus and an outside-dismiss layer while the panel is open.
    pub keyboard_focus: bool,
    /// Keep outside dismissal but request keyboard interactivity only for a text input.
    pub text_focus_only: bool,
    pub initial_open: bool,
    pub factory: PanelFactory,
    pub fallback: Scene,
    pub fallback_factory: Option<PanelFallbackFactory>,
    pub renderer: RendererKind,
    pub position: Option<PanelPositioner>,
}
/// Stacking policy for a named, input-empty or selectively interactive canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasLayer {
    #[default]
    Desktop,
    Overlay,
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
    /// B0053: views offered into the overlay slot (see [`UiEvent::CompanionOffer`]).
    pub overlays: Option<Overlays>,
    /// Optional output-sized canvas views, created on first offer per output and name.
    pub canvases: Option<Panels>,
    /// Unlisted canvases stay below windows; transient drag previews can use Overlay.
    pub canvas_layers: std::collections::BTreeMap<String, CanvasLayer>,
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
    /// B0090: leased per-window capture; the hub reports `supported()` once the compositor's
    /// image-copy-capture and foreign-toplevel globals are bound.
    pub capture: Option<std::sync::Arc<CaptureHub>>,
    /// Live presentation palette for a process without a wallpaper consumer (an app process):
    /// its window or panel wears the palette at start and follows every change.
    pub palette_feed: Option<std::sync::Arc<dyn PaletteFeed>>,
    /// On-demand xdg toplevel window (`bead.window open|close`).
    pub window: Option<WindowSpec>,
    /// Detachable panels; none: panels cannot detach.
    pub pearls: Option<PearlOptions>,
    /// On-demand app process (ADR 0005): `Some(args)` opens the window (or the default
    /// panel) with those arguments at start, creates no primary surface, and ends the
    /// process when that window or panel closes.
    pub app: Option<Vec<serde_json::Value>>,
    /// Start a panel app directly in a pearl, through the normal detach/dock lifecycle.
    pub app_as_pearl: bool,
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
            overlays: None,
            canvases: None,
            canvas_layers: Default::default(),
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
            capture: None,
            palette_feed: None,
            window: None,
            pearls: None,
            app: None,
            app_as_pearl: false,
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
    visibility: u64,
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
    pointer_visual: Option<pointer_motion::Visual>,
    decorations: Option<decorations::Tiles>,
    background:Option<wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    background_size:Option<(i32,i32)>,
    logical_size:(u32,u32),
    background_visible:bool,
    companion: bool,
    output: wl_output::WlOutput,
    output_name: String,
    layer: layer_role::LayerRole,
    /// The companion layer has no attached buffer after its close animation completes.
    panel_unmapped: bool,
    /// Fresh xdg role used while detached. The layer role remains available for
    /// docking because Wayland surfaces cannot change roles.
    native: Option<smithay_client_toolkit::shell::xdg::window::Window>,
    prepared_native: Option<pearls::PreparedNative>,
    handoff: Option<pearls::Handoff>,
    pool: SlotPool,
    #[cfg(feature = "wgpu")]
    gpu: Option<bead_render::gpu::Surface>,
    #[cfg(feature = "native-gl")]
    gl: Option<bead_render::gl::Surface>,
    #[cfg(feature = "native-gl")]
    gl_prepared: bool,
    #[cfg(feature = "native-gl")]
    gl_prepare_pending: bool,
    buffers: Vec<Reusable>,
    width: u32,
    height: u32,
    native_configure_size: Option<(u32, u32)>,
    scale120: u32,
    configured: bool,
    ready: bool,
    callbacks: policy::Callbacks,
    input_visible: Option<bool>,
    input_rect: Option<[i32; 4]>,
    keyboard_visible: Option<KeyboardInteractivity>,
    redraw: bool,
    fractional: Option<WpFractionalScaleV1>,
    viewport: Option<WpViewport>,
    geometry: (i32, i32),
    offset: (f32, f32),
    /// The renderer-only slide applied to reveal content; UI hit boxes remain in layout coordinates.
    input_content_offset: (f32, f32),
    format: PixelFormat,
    previous: Option<Rect>,
    visual_damage: bead_render::damage::FrameDamage,
    allocations: usize,
    shm_trim_pending: bool,
    heap_trimmed_ui_serial: u64,
    heap_trim_generation: Option<(u64, u64)>,
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
    last_submitted_ui_serial: u64,
    scale_relayout_pending: bool,
    resources_retry_pending: bool,
}
impl Surface {
    fn wl_surface(&self) -> &wl_surface::WlSurface {
        self.native
            .as_ref()
            .map_or_else(|| self.layer.wl_surface(), |window| window.wl_surface())
    }
    fn commit(&self) {
        self.wl_surface().commit();
    }
    /// Unmaps the layer role; it accepts buffers again only after a fresh configure.
    fn unmap_layer(&mut self) {
        self.layer.unmap();
        if self.native.is_none() {
            self.configured = false;
            self.callbacks.invalidate();
        }
    }
    fn owns_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.wl_surface() == surface
            || self.prepared_native.as_ref().is_some_and(|prepared| {
                prepared
                    .window
                    .as_ref()
                    .is_some_and(|window| window.wl_surface() == surface)
            })
    }
}
struct WallpaperSurface {
    palette: Option<bead_render::Palette>,
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
    staged: Option<StagedWallpaperBuffer>,
    viewport: Option<WpViewport>,
    viewport_destination: Option<(i32, i32)>,
    pixels: Option<Vec<u8>>,
    transition: Option<WallpaperTransition>,
}
struct StagedWallpaperBuffer {
    revision: u64,
    token: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    width: u32,
    height: u32,
    index: usize,
}
impl StagedWallpaperBuffer {
    fn matches(
        &self,
        revision: u64,
        token: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
        width: u32,
        height: u32,
    ) -> bool {
        self.revision == revision
            && self.width == width
            && self.height == height
            && std::sync::Arc::ptr_eq(&self.token, token)
    }
}
struct WallpaperTransition {
    commit: Option<(u64, std::sync::Arc<dyn std::any::Any + Send + Sync>)>,
    palette: bead_render::Palette,
    pixels: Vec<u8>,
    // None retains an already-final snap while both SHM buffers are busy.
    target: Option<Vec<u8>>,
    remaining: u8,
    uploaded: bool,
}
impl Drop for Surface {
    fn drop(&mut self) {
        drop(self.prepared_native.take());
        // A pending cover can still own EGL resources for the original layer.
        drop(self.handoff.take());
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

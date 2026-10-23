# Framework components

Luau views import `bead.kit`; components are embedded in the permissive
bead-ui framework rather than copied between shell views. `bead.ui` remains
available for custom primitives. The existing Orient palette supplies color
roles; no second palette engine is introduced.

`assets/size-scale.json` mirrors Nacre's `nacre/shell/widgets/size-scale.json`:
spacing, radii, text and icons share named values. Text/icon point sizes convert
to logical pixels at 96 DPI (4/3); physical pixels still follow output scale.
Motion color/state/text durations match NacreTokens at 200/120/400 ms.
Use `python3 tools/sync-size-scale.py SOURCE --check` to detect source drift;
without `--check` it updates only the framework scale mirror.

The kit exposes Button, IconButton, StatusIcon, Slider, Switch, Tile, SegmentedControl,
List/ListRow, Card, Popover/Peek, Tooltip and TextInput. Slider uses a bounded
step range: click/drag chooses a step, arrow keys change the focused step.
`TextInput masked=true` draws bullets and keeps its value out of accessibility, IME context, copy and traces (stores named `*.secret` are also left out of trace dumps); `kit.PasswordField` adds a show/hide button and Enter action, `kit.ConfirmButton` asks again in place before acting.
Tile takes distinct `on_toggle` and `on_detail` actions. SegmentedControl takes
stable item IDs and an action receiving the selected ID. List/TextInput keep
existing stable store bindings; component identity survives reload.

`StatusIcon` uses Button interaction; the shell supplies its size and theme.
Rust hosts can use `bead::ui::status_pill::StatusPill` with explicit `Timings`
to route icon hover previews and clicked panels. The host schedules
`next_deadline`, calls `advance`, and supplies content enter/leave and Escape
events. Names and appearance remain shell policy. Nacre uses separate small
peek surfaces and existing device views in compact right-edge panels; see
[B0018](specs/0018-control-center-status-pill.md).
Native `PanelSpec.keyboard_focus=false` keeps a pointer-interactive preview
nonmodal, without keyboard focus or a full-output dismissal layer. Clicked
panels request keyboard focus and retain outside-click dismissal.
Companion input follows the content bounds, including its animation; decorative
shadow padding does not intercept the owning bar's controls.

Bar and Panel accept `edge`: top/bottom/left/right. Bar changes row/column and
reverses bottom/right order. Panel animation moves inward from its edge;
layer anchors update when worker candidates arrive. Popover/Peek/Tooltip use
the owning bar's edge and open away from it. Current GUI acceptance covers top
bar and left/right panels only; bottom bars/panels remain unvalidated.
Panel `transition="ease-in-out"` selects cubic easing for hover previews;
the existing `ease` and `spring` transitions retain their behavior.
Tooltip visibility is supplied by the view's hover binding. LevelMeter accepts
a clamped 0–1 value. BlurredArtwork accepts a tracked image and bounded blur
radius; its raster is retained and shared by CPU/native GL. This is artwork
blur, not a compositor backdrop-blur promise.

Example: `shell/kit/main.luau` composes the complete kit; its behavior file
contains actions/stores only. The native bar and launcher compose shared kit
components. No physical keyboard/assistive-technology acceptance is claimed.

## Build and lifecycle policy

The normal shell build includes CPU plus native EGL/femtovg. `--renderer auto`
keeps the bar on CPU and gives companion panels a CPU first frame, lazy GL,
and a CPU return after idle. The launcher retains its original top-edge pose;
the kit sample demonstrates left/right placement. Explicit renderers remain
available for measurements.

`cargo build -p nacre-shell --no-default-features` builds CPU only.
`--features wgpu` (or `vello`) explicitly enables comparison paths. The old
`gpu` feature is a compatibility alias for both graphics families. The historical
blank-layer benchmark is a separate opt-in manifest under spikes/blank-layer;
its code and saved binaries remain intact.

`--features trace` includes allocator and thread diagnostics. Environment
trace switches cannot enable code excluded from the normal binary. UI/state
proof markers remain explicit developer options. The UI implementation is split
into generation, session and interaction modules; Wayland execution is split
into runtime, transactions, rendering and protocol dispatch. Public API exports
are retained.

## Nacre tray preview

The bar's private/legacy `--core-views` preview uses the existing headless tray
host and a lazy `tray` companion panel built with Popover. It preserves submenu,
disabled, separator and toggle state; primary, context, middle and both scroll
axes route to SNI/DBusMenu. Attention icons have normal-icon fallback. Bounded
plain tooltip text appears in the bar's title area, avoiding another resident
surface. Menu state/subscriptions retire with the panel; the host survives reload.

With a shared daemon client, tray rows remain hidden: the current protocol has
reserved tray snapshots but no published producer/action contract for this path.
There is no direct-service fallback for that mode. This is preview functionality,
not core-switch, QML parity or memory acceptance; see [B0016](specs/0016-system-tray.md).

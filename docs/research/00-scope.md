# 00 — The scope of replacing Qt/QML

Audited 2026-10-13. Read-only source: nacre-desktop/nacre
`68df8dd3b25cbf202d3859d0178a50cceffbea0d`. This report describes that revision,
not the installed personal desktop and not an assumed future Nacre version.

## Evidence and counting method

The shell contains **224 QML files / 27,854 QML lines**, agreeing with the supplied
rounded size. [scope-index.md](scope-index.md) enumerates every import, object type
spelling, IPC handler signature, Process/FileView declaration, helper task,
shortcut and discovered CLI IPC call site. [scope-inventory.json](scope-inventory.json)
contains every location, per-file imports/types, property type annotations, all namespace member references,
and the entire compositor declaration contract as data.

Reproduce: `python spikes/inventory.py NACRE_CHECKOUT docs/research/scope-inventory.json`.
The script scans all shell QML files and non-test shell-tools text. This is a
comment/string-aware lexical audit, not QML execution or a semantic compiler.
An audited JS regular-expression false positive is excluded. Dynamic commands,
paths, inherited properties and constructed target names are retained as source
expressions and must be resolved by feature tests. Counts do not mean active
processes or windows. Installed Qt/Quickshell qmltypes were cross-checked to
identify exported types, with project/inline types taking precedence over
same-named toolkit exports.

There are 82 distinct import spellings (including aliases and local JS imports),
239 object type spellings, **43 IpcHandler declarations**, **57 Process object
declarations**, **29 FileView object declarations**, 14 helper/palette task
declarations and 12 native/project shortcut declarations. All token references were also indexed: exactly 61 Process and 31 FileView
mentions, compared with the supplied initial 60/31 counts. The initial snapshot b547d06 had 27,723 lines / 42 handlers / 60 Process mentions;
concurrent upstream integration added a window-title IPC/process before the final
audit. The declaration-versus-reference difference comes from type references
in addition to declarations; typed properties and wrappers remain in scope.

## Feature inventory and rough size

Sizes below are current QML, not a Rust line estimate. Related services and JS
bring each feature above its module-only size. S/M/L/XL are relative effort
judgments: S is a contained adapter/widget, XL crosses many toolkit facilities.

| Feature | Module QML files / lines | What must be rebuilt | Rough effort |
| --- | ---: | --- | --- |
| Bar and top header | 19 / 1,996 | Workspaces, focus, status controls, clock, popouts, output placement | L |
| Drawers/shell orchestration | 9 / 927, plus IPC/shortcuts 302 lines | Output-local views, dismissal, focus, panel ownership | L |
| Dashboard | 23 / 3,003 | Media artwork/controls, calendar, resources, host/weather cards | L |
| Launcher | 11 / 2,567 | Catalog, ranking, grid virtualization, modes, inputs, selectors | XL |
| Extras/left drawer | 7 / 1,650 | AI streaming/sidebar, clipboard, keybindings, overview, palette | XL |
| Settings | 21 / 2,227 | Controls, preferences, helper actions, persistent window geometry | L |
| Colors and welcome | 11 / 1,921 | Palette previews, wallpaper/style workflows, tutorial posters | L |
| Background | 3 / 327 | Static/video wallpaper, image cropping, crossfade, frame sync | L |
| Pearls | 2 / 405 | xdg windows, docking, output coordinates, compositor-owned placement | XL |
| Notifications and OSD | 5 / 494 | D-Bus server, history, actions, inhibition, timed overlays | L |
| Session and control center | 5 / 647 | Power/session actions, audio/media/device controls | M |
| Reusable widgets | 22 / 1,365 | Typography, surfaces, sliders, text editing, clipping, range input | XL |
| Service layer | 72 / 9,572 | All persistent state, workers, compositor/device/media adapters | XL |
| Root/config/utils/lock preview | 11 / 451 | Tokens, paths, initialization, localization, lock-preview widgets | M |

Feature totals in the machine inventory distinguish root module files, avoiding
double counting. Additional JS carries behavior (ranking, color validation,
layout/geometry, calendar, translation, wallpaper rotation, view-state). Python
helpers remain external; QML line totals do not include that implementation.

## Types/services to replace

External imports are QtQuick, Controls, Controls.Basic, Dialogs, Effects, Layouts,
Shapes, Window and QtMultimedia; Quickshell core, Io, Wayland, Widgets,
Bluetooth, Hyprland, and Services.Mpris/Notifications/Pipewire/UPower. Internal
`qs.*` namespaces, inline components and imported JS are indexed separately.

| Capability | Toolkit types and service references observed | Bead responsibility |
| --- | --- | --- |
| Shell/window lifecycle | ShellRoot, Scope, Singleton, PanelWindow, FloatingWindow, Variants, LazyLoader, PersistentProperties, Region; Quickshell.screens/processId | Surface management, view lifetime, output-local stores, hit/input regions |
| IPC/process/files | IpcHandler, Process, SplitParser, StdioCollector, FileView, FileViewError; execDetached/env | Command transport, bounded subprocess streams, cancellation, file reads/watch/write |
| Compositor | Native.Hyprland, Native.GlobalShortcut, project HyprlandWindow/NiriWindow; WlrLayershell, ScreencopyView | Neutral backend state/events/actions, keyboard-focus policy, capture |
| Device/media | UPower.displayDevice/onBattery and battery enums, Bluez.Bluetooth/devices/defaultAdapter, Pipewire nodes/default/preferred sinks/sources, PwObjectTracker, Mpris.players, NotificationServer | D-Bus/PipeWire lifetime and reconnect, normalized models |
| Drawing | Rectangle, Image, AnimatedImage, Canvas, Gradient/GradientStop, Shape/ShapePath, PathSvg/PathAngleArc, LinearGradient, MultiEffect, ClippingRectangle | Renderer assets/paths/clips/effects, explicit caches, decode off UI thread |
| Text/input | Text, TextEdit, TextInput, TextField, TextArea, FontMetrics, FocusScope, MouseArea, HoverHandler, DragHandler, WheelHandler, DropArea, Shortcut | Shaping/editing, selection, focus, pointer gestures, clipboard/DnD, IME |
| Layout/collections | Item, Row/Column/Grid/Flow, RowLayout/GridLayout, Flickable, ListView/GridView, ListModel, Repeater, Loader, Component, Connections, QtObject | Existing layout engine, virtualized lists, event subscriptions |
| Controls/animation | Slider/SpinBox, Popup/ToolTip/ScrollBar/ScrollView/FileDialog; Timer/SystemClock; Number/Color/Property/ParallelAnimation, Transition/Translate | Controls and semantics, timing/easing/springs, native chooser via portal or helper |
| Multimedia | MediaPlayer, VideoOutput | Decode/frame delivery, lifecycle, GPU texture interop |

The index is authoritative for exact spellings/locations. Singletons used through
properties rather than object declarations are in `qualified_references` (e.g.
DesktopEntries, Quickshell.iconPath, UPower, Native.Hyprland and enum members).
No SNI/tray service import or SystemTray reference appears in the audited shell:
the requested tray is an addition to this snapshot, not measured QML parity.
Do not omit it from the planned bar.

## Command and keybinding compatibility

All 43 handlers and their complete method signatures are in the location index.
Targets cover `nacre`, `drawers`, `pearls`, `nacreCompositor`, device/media/state
queries, notifications, AI, settings/colors/welcome, wallpaper controls and
selectors. Some targets are per screen (`header-<screen>`, `barStatus-<screen>`,
`wallpaperFrame-<screen>`, `leftDrawer-<screen>`); detached views add window keys.
Preserve return types, state-query JSON, errors, defaults and target construction,
not just method names.

`control.sh` is the `nacre-shell` command dispatcher. `menu-cli.py` wraps Quickshell
IPC for routes and asynchronous pick/input sessions; `desktop-actions.py` also
calls brightness IPC; `session-watch.py` queries shell state. `cli-bridge.sh`
invokes the Python palette CLI and publisher under a commit lock. `cli.json` is
empty at this revision and cannot serve as the command catalog. The user-facing
`nacre_shell` palette commands in shell-cli are separate from runtime UI IPC.

`compositor-contract.json` version 1 is a **configuration declaration contract**,
not a complete runtime interface schema. It contains 6 output groups, extensions,
bindings, rules, unbinds and events. Actions include application.launch,
workspace.switch/scratchpad, window.focus-direction/move/resize/fullscreen/cycle/
floating/pin/pseudo, group.toggle/previous/next, layout.toggle-split,
keymap.switch, compositor.reload, shell.shortcut and panel.drag/finish. All
individual declarations/parameters/options remain in the JSON appendix.
The runtime contract additionally comes from
[Nacre's compositor interface](https://github.com/nacre-desktop/nacre/blob/68df8dd3b25cbf202d3859d0178a50cceffbea0d/docs/development/compositors.md)
and NacreCompositor.qml/compositor.py. Backend-specific unsupported declarations
are explicit; “mirroring the contract” does not make niri support Hyprland-only
pinning or release shortcuts.

Native Hyprland global shortcuts register through its Wayland protocol. Niri
generated bindings call `nacreCompositor shortcut NAME`; its adapter emits a
press pulse, with no real release event. This matters for the launch-on-release
chord behavior. Preserve the current documented fallback rather than synthesize
a release guarantee the compositor cannot supply.

## Processes, file views and helper service

Process declarations cover helper worker/palette server, compositor event stream
and actions, hardware/light channels, network/wallpaper/resource/weather polling,
AI/backend streaming, notification/device commands and pearl-window placement.
There are also detached launches through AppLaunch/Quickshell.execDetached and
helper wrappers. File views cover preferences/catalog/localization, scheme and
presentation, notification history, wallpaper paths/preferences, weather/resource
cache, `/proc` statistics and hardware brightness sysfs. The appendix records
every object location and command/path expression. No helper was invoked merely
to inventory it; some operations write state or act on hardware.

The existing worker is `nacre/shell-tools/helper-service.py`, source inspected
with NacreHelpers.qml and NacreHelperTask.qml:

- UTF-8 JSON per newline on stdin/stdout; stderr stays diagnostics. Startup:
  `{"ready":true,"version":1}`.
- Request has **exactly** `id`, `tool`, `arguments`, `input`; positive safe integer
  ID up to 9,007,199,254,740,991; argument list at most 8 strings, each at most
  65,536 characters and no NUL. Non-finite JSON is rejected.
- Tool allowlist: menu-backend, dashboard-data, dashboard-media,
  launcher-preferences, notification-history, network-state. Their supported
  argument/payload shapes are enforced by the existing dispatch function.
- Response: `{id,code,output}`, code 0/1. **output is a string** containing the
  helper's serialized JSON/newline, not an already-decoded object; save operations
  can return an empty string.
- 4 MiB request-read bound, 8 MiB response bound, 64 pending slots. Per-tool serial
  worker lanes permit independent helpers to progress. Duplicate active IDs and
  oversized requests are fatal. Oversized responses/queue full return errors.
- QML client startup deadline 10 s, request deadline 45 s; restart delay 250 ms
  doubling to 10 s, reset after 10 s stable. Death/malformed reply/timeout fails
  pending requests; **never automatically replay side-effecting requests**.

The Python worker bounds some lengths in characters while stdin readline is a
byte-oriented bound. Preserve compatibility and apply a stricter bounded decoder
at the Rust boundary; do not silently describe all limits as identical byte
limits. The palette publisher has a different `serve` stream: document and test
its request/reply/lifetime separately from helper-service.

## Orient and retained behavior

[Palette JSON format](https://github.com/nacre-desktop/orient/blob/a4ef06e86ea06dac8a6d35d179cfd770efc5981a/docs/palette-json.md)
was verified on the separate Rust-port branch, not Orient main. Format 1 maps
semantic `colours` roles to six RGB hex digits without `#`; validate the major
version, tolerate new metadata/roles and retain the last good palette on errors.
Current Nacre also consumes `presentation.json` (poster and colors committed as a
pair) and legacy `scheme/current.txt`. The compatibility reader must distinguish
these envelopes instead of claiming they are all raw Orient palette JSON. Do not
write a color engine, trigger palette generation on reload, or change publishers.

Localization, wall-clock/timezone behavior, notification history, clipboard,
catalog caching, selector cancellation, panel geometry, streaming markdown and
device permission failures are part of parity even though they are not names of
Quickshell types. The lock-preview is in scope as a UI; secure session lock is a
separate safety-critical protocol feature, not a “full-screen panel”.

## Verdict

**Concern:** this is a small desktop toolkit plus a shell, not a language-only
translation. Service/state logic is about one third of the QML lines; the toolkit
also supplies behavior absent from those line counts. Early gates should prove
text/input and transactional reload, before spending time on full panel parity.

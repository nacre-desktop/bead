# B0113 — Widget system: detachable widgets, tile-grid bars, widget menu, music widget

Status: design accepted by the owner, 2026-10-25. Phase 1 is implemented and its worker gates pass; phase 2 remains separate. Owner request: "everything on the bar is detachable … the top bar is gonna be a grid … start with a music widget … we need a widget menu … make a good spec."
Builds on: ROADMAP "widget hosts and movable widgets" (2026-10-23), pearls (B0048/B0054/B0104), kit, tokens, ADR 0004 (the Generalization list) and ADR 0005 (one UI process).

## Goals

1. Every piece of the top bar becomes its own **widget** that the user can move, remove, re-add and configure:
   - the launcher button (Nacre logo);
   - workspace pearls;
   - the window title;
   - each status item: updates, sound, Wi-Fi, Bluetooth, battery, notifications, calendar/clock, power;
   - the pill containers that group them.
2. Bars are **hidden tile grids**. Widgets occupy spans. Grid lines show only in edit mode.
3. Widgets have **host-dependent variants**: bar strip, side strip, dashboard/pearl card, desktop canvas. The music widget is the first rich one: a dancing line or circle in the bar with a media dropdown, and big visualisers on the desktop.
4. A **widget menu** (catalogue plus edit mode) to add, remove, rearrange and configure widgets. Each widget has a **settings page** generated from its schema.
5. **"Nacre default" preset equals today's bar exactly**, pixel for pixel. Switching to the widget system must not change the look until the user edits it.
6. Bead stays a general framework. The engine knows hosts, slots, spans, drag/drop and settings schemas, never "music" or "Nacre". Widgets are Luau packages. Nacre-specific data comes through the adapter, and the daemon never owns UI composition.

## Concepts

- **Widget package** (`shell/widgets/<id>/`, built-in; `~/.config/bead/widgets/<id>/`, user, live-reload in a later phase):
  - `widget.luau` returns `{id, title, icon, category, data = {topics…, actions…}, settings = <schema>, variants = {bar = …, side = …, card = …, canvas = …}, defaults = {…}, budget = {fps_active, fps_idle, pause_on = {"fullscreen","lock","battery_saver"}}}`.
  - Each variant is a render function `(props, settings, data) -> node`, plus a size contract `{min_span, max_span, preferred_span}` in host units. Bar units are tiles; canvas units are dp.
- **Host:** an engine primitive (`ui.Host{id, shape = "strip-h"|"strip-v"|"grid-2d"|"canvas", grid = {tiles, tile_size, gap, regions}, layout = <binding>}`).
  - It renders placed widget instances, exposes drop targets in edit mode, and resolves which variant to use.
  - Hosts: the top bar (strip-h), the bottom bar and frame side bars (later; strip-h/strip-v), the dashboard (grid-2d, later), pearls (card), and the desktop canvas (a new layer-shell surface above the wallpaper and below windows; canvas).
- **Instance:** `{iid, widget, host, slot = {region, start, span} | {x, y, w, h}, settings = {…}}`. Several instances of the same widget are allowed (e.g. two clocks).
- **Pill:** a container widget (`kit.Pill`) whose children are instances. It has its own span, and its children lay out inside it. Today's status pill = a pill containing updates, sound, Wi-Fi, Bluetooth, battery, notifications, clock and power. Dragging a child out of a pill makes it a free instance. Dropping onto a pill inserts it.
- **Layout document:** `{version, preset, hosts = {top_bar = {tiles, regions, instances = […]}, canvas = {…}}}`.
  - Stored as an opaque JSON value in a settings store (`settings.store.widgets` on Nacre; a file in standalone Bead). The daemon stores it but never interprets it.
  - Validated against the catalogue on load. Unknown widgets are kept but shown as a placeholder in edit mode.
  - Writes are revisioned (settings.set rev).
- **Presets:** "Nacre default" is generated so that it exactly reproduces today's bar. "Reset to default" exists. Users can save and load named layouts (export/import JSON).

## Tile-grid bar

- The top bar width is divided into N tiles. The default is derived from the width so that tiles ≈ 32 dp: 2880x1800 at 1.5 gives 1920 dp, or about 60 tiles. Regions: left, centre and right.
  - Left and right regions grow inward; the centre is centred.
  - A widget occupies `span` tiles within its min/max.
  - Spacing comes from tokens. The visual output for "Nacre default" equals today (B0096/B0101 parity is the gate).
- In edit mode, tiles show faintly. Drag shows the drop slot and a variant preview, plus a resize handle on span-capable widgets (e.g. music line length).
- Overflow: a widget that does not fit shows a compact variant or moves to an overflow pill. Nothing is ever lost from the layout.

### Flexible widgets (owner discussion, 2026-10-25)
- Variable-width widgets (the window title, the music line, the clock with optional seconds/date, counts such as updates) declare min/max spans.
- The host RESERVES the max span, so neighbours never move when the content changes. Inside the reservation, the visible widget renders at its NATURAL content width (not snapped to tiles), stays centred or anchored per its region, and ANIMATES width changes like QML.
- Long content is truncated with an ellipsis at max.
- The title widget's settings: min/max width, what it shows (window title / app + title / app only / icon), and its behaviour (hover opens the dashboard, which is the current default; click; or none).
- Multiple monitors (proposal, pending owner confirmation): one shared layout by default (region-relative slots keep it right on any width; tile count = width_dp/32), plus an optional per-monitor layout override chosen in edit mode.

## Widget menu and settings

- Opened from the bar's right-click menu "Edit widgets", the launcher route `widgets`, `nacrectl ui widgets open`, and Settings → Panels.
- **Edit mode:**
  - every host shows its grid;
  - widgets get handles; drag between hosts and pills; remove (drop on the trash, or the × button);
  - the catalogue sidebar lists all widgets with live previews per host;
  - Done/Cancel; a "Reset to Nacre default" action;
  - Undo for the last few edits.
- **Per-widget settings page:** generated from the widget's `settings` schema (types: toggle, choice/segmented, slider/number, color/palette role, text, icon picker, action picker). It opens from edit mode (click a widget), from a widget's own context menu, or inline in a widget's dropdown (as in the music widget's "Music effects" panel). Settings are per instance.
- **Examples:**
  - Launcher button: icon = Nacre / Arch / CachyOS / custom file; action = launcher / dashboard / app grid / custom command route; shape and size.
  - Clock: format, date, seconds, font.
  - Pearls: count mode, labels, colours (the B0101 blend), with the thread on or off.

## Widgets app and forms (owner design, 2026-10-25; replaces the right-click editor)

Owner decisions:
- The right-click "Edit widgets" list editor is REJECTED (debug-looking, confusing grid, too easy to lose widgets). Widget editing lives in a **Widgets app** window (like Settings: `nacre-shell --app widgets`, opened from Settings → Widgets, the launcher and a keybind/route).
- Pixel parity with QML is no longer required for widget placement: widgets may SNAP to the grid.
- Drag and drop must feel great: smooth, with a snap animation plus effect when a widget lands.

**Forms.** Each widget declares up to three forms, each with named STYLES:
- `bar` (strip host): e.g. music: line | circle; launcher: nacre | arch | cachyos | custom icon; status: icon | icon+value | icon+text; clock: time | time+date | seconds.
- `desktop` (canvas host): e.g. music: bars | wave | mirror | ring | nation | particles (+ lyrics, wallpaper effects).
- `panel` (the dropdown opened by clicking the bar or desktop form): e.g. the music media card with a "Music effects" page (the same settings as the app).
A widget can be placed in the bar, on the desktop, or BOTH. The forms share data and shared settings (e.g. intensity, colours), and each form has its own style and placement settings.

**Widgets app layout.**
- Gallery: categories (All, Bar essentials, Status, Media, System, Containers, Your widgets), search, and cards with LIVE animated previews (the bar form above, the desktop form below as a mini scene).
- Widget page:
  - a large preview STAGE: a mini replica of the user's screen (their wallpaper, palette and bar) with a fake data source (e.g. a playing track);
  - tabs "In the bar" / "On the desktop", each with an enable toggle and a row of STYLE tiles; selecting one updates the stage instantly;
  - settings below, generated from the schema plus layout hints.
- Placement:
  - enabling a form flies it to a sensible slot with a snap animation;
  - or drag the stage preview onto the real bar/desktop, with live reflow of neighbours (they slide aside), a magnetic snap, and a short settle/glow effect;
  - desktop snap targets: centre, edges, corners; resize handles.
- The bar while the app is open: widgets get a subtle hover outline and are draggable in place. Drop markers appear only near the pointer while dragging; NEVER a full checkerboard grid.
- Removal: drag off, or the hover ×. Removed widgets always stay in the gallery. Undo, plus "Reset to Nacre default" in the app header. Nothing is lost silently.
- Smoothness: every drag, reflow and drop is animated at the full frame rate (SMOOTHNESS FIRST); the app opens with content on the first frame.
- Containers: drag status items into or out of a pill; the pill animates its width.
- **Detach anywhere (owner, 2026-10-25):** WITHOUT the Widgets app open, a widget can be dragged off the bar and dropped onto the desktop (workspace/canvas), like pearls detach.
  - While dragging it MORPHS from its bar form into its desktop form (animated size/shape transition, e.g. the music line grows into the visualiser).
  - Dropping attaches it to the desktop at the snapped position; dragging a desktop widget onto the bar morphs it back.
  - NO floating-card form (owner, 2026-10-25): placement forms are only BAR and DESKTOP. Dragging out produces the widget's CONFIGURED desktop form (style and settings chosen in the Widgets app page or in the widget's dropdown settings, e.g. 'Music effects'); dragging back produces its configured bar form. Widgets without a desktop form rubber-band back to the bar. Dropdowns (panel forms) are NOT draggable; they are the place to control and customise. Exception: the big panels (AI chat sidebar, control panel) stay detachable as pearls.
  - A widget can still be in both places (bar + desktop) via the app.
  - The drag must be smooth and should start on a deliberate gesture (press-and-hold or drag threshold) so normal clicks are unaffected.


## Music widget (first rich widget)

Inspired by the owner's reference (r/unixporn "desktop dances to the music"): a bar visualiser plus a media card dropdown plus a "Music effects" settings panel.

- **Data:**
  - MPRIS via `dashboard.player` (exists): title, artist, album, art, position, length, controls, shuffle, loop, like (if exposed);
  - **audio spectrum** via a new lease-only daemon topic `audio.spectrum` (Nacre N0072): N log-spaced bands (e.g. 32/64), overall level and beat onset, at up to 60 Hz while leased. It comes from PipeWire monitor-capture of the default sink, computed in the daemon (one shared source for all widgets), and stops when unleased. Its CPU budget is ≤ 2% of one core at 60 Hz.
- **Bar variant (strip-h), setting "bar style":**
  - "Dancing line": a row of thin bars or a waveform line, like the reference; it can span many tiles;
  - "Circle": a small ring pulsing with the level.
  - It idles flat or invisible when nothing is playing (setting).
  - **Click** opens a **dropdown media card** joined to the bar contour like the status popouts. Its contents:
    - art;
    - source app (Spotify, etc.);
    - title, artist;
    - the like/heart, if available;
    - a seek bar with times;
    - prev/play-pause/next, shuffle and repeat;
    - a volume shortcut and a queue/lyrics button, if available;
    - a spectrum strip along the bottom;
    - a settings button opening "Music effects".
- **Canvas (desktop) variant, setting "desktop visualizer":**
  - modes: Bars, Wave, Mirror, Ring, Nation (radial), Particles;
  - Intensity (0–200%);
  - colours from palette roles or art-derived colours;
  - position, size and anchor: free on the canvas, or snapped, e.g. full-width bottom or centre circle;
  - optional "Desktop lyrics" (if lyrics data exists: dashboard.media lyrics enrichment, N0060).
- **Wallpaper effects (phase 4, B0116), a toggle plus a choice:** Glow, Blur, Zoom, Shake, driven by level/beat. They are implemented in the wallpaper renderer (B0099 GL path) as cheap shader uniforms fed by `audio.spectrum`, off by default, and paused on battery-saver/fullscreen.
- **"Music effects" panel** (the reference's second image): Intensity slider, desktop visualiser toggle plus mode grid, desktop lyrics toggle, wallpaper effects toggle plus effect chips, and "Customize desktop" (opens edit mode for the canvas). It is the music widget's generated settings page, with custom layout hints, reachable from the dropdown's settings button.
- **Budget:** animate only when audio is playing AND the host is visible. 60 fps only while the spectrum changes; 0 fps when paused or silent. Pause on fullscreen windows, lock and battery-saver. Bar variant ≤ 1% CPU and canvas ≤ 3% at 60 Hz on the laptop (Intel xe, GL path).

## Engine work (general primitives; ADR 0004 Generalization list)

- `ui.Host` with strip/grid/canvas layout, slots, spans, regions, drop targets and variant resolution.
- Drag/drop between surfaces: reuse the pearl drag (B0104) for cross-surface moves; the desktop canvas is a layer surface.
- Edit-mode overlay primitives (grid lines, handles, ghost previews).
- A settings-schema → page generator (reuse the Settings kit: toggles, selects (B0094), sliders (B0107)).
- A high-rate data channel for leased numeric arrays (spectrum) that avoids JSON per frame (a shared-memory ring or compact binary topic frames). The Nacre side is defined in N0072.
- Visualiser drawing primitives: bars, polyline/wave, ring, and particles in bead-render (CPU and GL), all budgeted.

## Phases (each one a unit; parity and budgets gate each)

1. **B0113 phase 1, model plus bar grid plus port.**
   - The widget package format, the registry, the Host primitive (strip-h), the layout document plus store, and the "Nacre default" preset.
   - Port today's bar pieces into widgets: launcher button, pearls, title, the status pill as a pill with 8 child widgets, and power.
   - **Gate:** the bar looks and behaves exactly as now (B0096/B0101/B0108 parity pairs ≤ current rmse; hover/click/dwell unchanged; first key ≤ 8 ms; memory per the noise rule). No edit UI yet; a layout edit via the store must take effect live.
2. **B0114 phase 2, edit mode plus widget menu plus settings pages plus desktop canvas host.** Drag between bar regions and pills, remove/add from the catalogue, per-widget settings pages (launcher icon/action as the first example), the canvas host surface, and presets/reset/undo.
3. **N0072 plus B0115 phase 3, music widget.** The daemon spectrum source; bar line/circle; the dropdown media card; canvas visualiser modes; the Music effects panel; lyrics if available.
4. **B0116 phase 4, wallpaper audio effects**, then more widgets (clock styles, weather, system monitor, calendar) and user widgets with live reload.

## Non-goals (now)

Side bars and the bottom bar as widget hosts (after phase 2), a third-party widget sandbox/permissions model (needed before sharing widgets publicly), and a widget store.

## Phase 1 API and persistence

`ui.Host{shape="strip-h",grid={tiles,gap,regions,mode},layout,catalogue,render}`
accepts a resolved instance list or live binding and resolves each package's
`bar` (or optional `compact`) size contract, including compact minima. Tile slots
are zero-based and region-relative; right slots count inward from the right.
Regions supply `{start,span}` in host tiles. Grid placement uses general
`column_start`/`column_span` properties, including real inter-tile gaps.
`grid.tile_size` can derive a count from a numeric width. The default preset
instead uses `mode="content"`: slots order naturally sized content, preserving
all historic bar geometry; switching a host to `mode="tiles"` sizes its spans.
The implicit default derives about 32 dp tiles from the current viewport.

The shell validates v1 documents and known package settings before accepting a
view transaction; bad edits retain the accepted view/stores. IIDs are unique
across hosts/children. Unknown packages, settings and extra fields remain opaque
and survive export. `pill.children` contains ordinary instances with the pill's
IID as their host. Empty settings schemas explicitly mean no controls yet.
The first copy retains legacy view IDs/anchors; extra copies receive IID scopes.

Nacre transport is `settings.store.widgets` with `data.layout` (null resets the
preset), `widgets.rev`, and revisioned writes through
`settings.set {store:"widgets",key:["layout"],value:<document>,rev:<revision>}`.
Registration request to Nacre: add an opaque `widgets` settings store with a
writable `layout` JSON value defaulting to null; the daemon must not interpret it.
Older daemons retain the default bar. Standalone uses
`$XDG_CONFIG_HOME/bead/widgets.json` (override `BEAD_WIDGET_LAYOUT`); edits and
atomic replacements are watched without polling. Launcher icon choices use the
bundled Nacre mark, theme icons `distributor-logo-archlinux` / `cachyos`, or a
custom icon path; actions select launcher, dashboard, or a custom view route.

## Phase 2 candidate — B0114: staged integration

The lead resumed B0114 under a 2 MiB always-on feature budget. The original
`23d2ffbc` measurements below use control `4925b6e8`; their maximum median cost
is 0.820 MiB, within that budget. The resumed candidate is rebased onto current
main including B0109, B0111, B0117, B0119 and B0120. Fresh integration checks are
in progress; the earlier full Arch CI and reviewed Panels golden remain recorded.
The editor, catalogue and launcher settings were inspected in private nested
CPU sessions at 2880×1800 / 1.5. Twelve accepted arms pass containment and swap
checks. No live session or installed files were changed.

The eight-step draft history writes only on Done: the standalone file is atomic,
and the daemon path waits for revisioned acknowledgement. Unknown empty objects,
arrays, nulls and 64-bit integers survive edits. Private pointer proofs move a
clock from its pill to the canvas, save it, open its anchored calendar and retain
the focused probe window while opening the editor. The shared layout stays outside
views; empty canvases allocate no layer, and visible content uses tight bounds.

| Five alternating CPU pairs; median candidate − control, MiB | Idle | Launcher | Typed | Closed + 12 s |
| --- | ---: | ---: | ---: | ---: |
| Unreclaimable | +0.554688 | +0.820312 | +0.753906 | +0.722656 |
| Clean file-backed PSS (informational) | +0.203125 | +0.236328 | +0.237305 | +0.237305 |

The earlier +0.5 MiB stop was superseded by the lead’s feature-budget decision.
The +0.5 MiB rule still guards regressions of existing behavior; the widget
feature’s total always-on allowance is 2 MiB. These measurements fit it.
The increases are anonymous; SHM is identical and DRM/SwapPss are zero. All five
pairs, including the near-zero fourth pair, remain in the medians. No allocation
attribution or measured jank-removal trade has been established.
First-key UI CPU is 3.578 ms median / 4.067 ms max; the first matching nested
presentation is 5.460 / 6.042 ms. These are first-response measurements with the
real menu service, not completed-query, daemon CPU or physical scanout timings.

The resumed integration fixes IPC status-open sending undeclared sidebar hover
stores to ordinary popouts. A generic lifecycle event lets each view project
only its declared policy state. Default parity and drag pacing are being rerun. Further phase-2 gaps are animated flexible
width, resize affordances, review of the new empty-value text controls, and
nested AI/control detach/re-dock validation.
The N0075 widget-store registration is still unpublished; standalone persistence
is proven. Idle canvas controls are clickable over an input-empty background;
the literal entirely-empty idle-region requirement conflicts with widget clicks
and remains unresolved. Per-monitor overrides remain a proposal.

General additions are canvas hosts, pointer transfers, inert metadata, anchored
panels, lazy output canvases, source-origin opt-in, child/host model and action
projection, direct app-to-pearl startup, shared asset snapshots and exact JSON
copying. Existing flexible titles reserve maximum width after grid conversion;
custom width changes currently happen immediately.

[All five pairs, components, source hashes and timing limits](evidence/0114/summary.json),
[containment](evidence/0114/containment.json), [CI](evidence/0114/checks.txt),
[save/dropdown/focus proofs](evidence/0114/widget-contracts.json).
Inspected editor crops: [layout](evidence/0114/editor-layout.png),
[catalogue](evidence/0114/editor-catalogue.png), [settings](evidence/0114/launcher-settings.png).

## Phase 1 evidence — 2026-10-25

Candidate `a9c2f75`, control `2e523f2` (B0118 daemon/bar recovery retained),
QML/daemon `421da0f`, Orient `3efe8d1`; Rust 1.99.0 and the checked-in release
profile. Full remote Arch CI, both isolated release bar `--check` commands and
four byte-identical regenerated/inspected bar goldens pass.

Captures are private nested CPU at 2880×1800/1.5, with matching generated
wallpaper/palette and twelve fixture updates. Parity uses a `12:34` display-input
fixture in private QML time-service and Bead behavior snapshots; drawing code,
wall/monotonic clocks and timers stay unchanged. Memory and key arms use the
unmodified snapshots. Exact fixture patches and binary hashes are in the data.

| Raw bar header RMSE % | Control | Widgets |
| --- | ---: | ---: |
| Idle | 5.726269 | 5.726269 |
| Title hover | 5.713135 | 5.713005 |
| Title active | 5.897943 | 5.897943 |
| Audio open | 5.775475 | 5.775475 |

All required original raw gates pass without masking any pixels. Direct native
control/candidate pixels are identical in all four states, including hover.
Against one common QML reference each state's RMSE is exactly equal; the tiny
per-run hover difference above comes from QML animation sampling. Dwell/peek,
anchor and hit-rectangle regressions retain the original timings and IDs. The
native title trace changes its focused title zero times in 73 post-focus states.

Earlier 180 ms hover samples varied in both directions (about ±0.036 percentage
points); all five are retained as superseded data. A further lossless burst uses
predeclared 75/125/175/225/275 ms offsets after recorded pointer dispatch, the same
nearest-time rule for every arm, one common QML reference and the full header.
Pooled RMSE is 5.708657% control / 5.699952% widgets. Every offset is reported;
there is no best-frame selection. Estimated selected times miss targets by
8–10 ms, but screencopy latency is 98–99 ms: half-latency frame-clock estimates
are coarse and do not establish precise dwell or physical scanout timing.

| Five alternating pairs, median Δ MiB | Idle | Launcher | Typed | Closed (2.5 s) |
| --- | ---: | ---: | ---: | ---: |
| Unreclaimable (candidate − control) | +0.132812 | +0.113281 | −0.492188 | +0.121094 |

All are below +0.5 MiB. All paired deltas are anonymous; SHM is identical and
DRM/SwapPss are zero. Small persistent growth is consistent with the extra Luau,
VM and watcher bookkeeping, without a per-allocation attribution claim.
Pair 4 has a +1.090 MiB open anonymous step; it is not a majority pattern. Pair 5
has a −2.223 MiB typed delta from higher control residency. Both remain in the
median calculation; the negative typed result is not claimed as an optimization.
File-backed PSS is informational: idle control/widgets 20.261/20.472 MiB, closed
20.914/21.060 MiB. Closed is the early 2.5 s sample, not a settled-idle/laptop proof.
First-key UI CPU is 3.819 ms median / 3.857 ms max; nested first presentation is
4.119 / 4.386 ms. The laptop absolute cap remains for the lead's reference run.
Twelve containment reports pass with no OOM, host swap-in or process SwapPss;
the complete active batch takes 382 seconds, under its 13-minute bound.

[Components, all paired deltas, timing samples, hashes and excluded attempts](evidence/0113/summary.json).
Paired crops (QML, Bead, amplified diff) are
[idle](evidence/0113/idle-pair.png), [hover](evidence/0113/hover-pair.png),
[active](evidence/0113/active-pair.png), [audio](evidence/0113/audio-pair.png).
Timed groups follow the five offsets above:
[control burst](evidence/0113/hover-control-burst.png),
[widget burst](evidence/0113/hover-candidate-burst.png).

Excluded attempts include a redundant legacy containment wrapper, a shared-target
stale SDK, QML-incompatible clock overlays and a private GVFS cleanup race. Owned
aborted resources were cleaned; separate targets, native admission, fixture
readiness and private local VFS now precede measurement. Earlier-source results
were superseded after integrating B0110/B0118 and refreshing QML/daemon main.
Nacre store registration remains the one-line request above. Phase 2 owns the
edit/menu/settings UI, content-preset grid editing policy and canvas host.

## B0151 finish-slice gate — 2026-10-26 (not accepted)

Occupied status-pill drops now save the desktop clock at 82% and 90% of bar width.
Six-state file/store drags and the app, resize, pearl detach/dock, and popup-focus captures pass.
The default bar remains byte-identical to its accepted goldens, and remote Bead CI passes.
Memory and 60 Hz CPU snap gates fail; Niri's private output did not reach 120 Hz.

| Gate | Measurement |
| --- | --- |
| First-key UI CPU | Candidate 26.45–28.17 ms; limit 8 ms. |
| 5-pair unreclaimable Δ, CPU | Idle +1.758 MiB; launcher +7.834; typed +12.670; closed +162.533; after drag/close +378.191. Budget 2 MiB. |
| 60 Hz snap | First overlay 143–166 ms; first bar paint 145–146 ms; 24–26 dropped bar frames/repeat. |
| 120 Hz | Not measured; nested Niri stayed at 60 Hz under the private 120 Hz parent/custom-mode requests. |

The official memory summarizer rejected the five pairs for first-key CPU >8 ms. A diagnostic copy relaxed only that validator to calculate the paired memory medians above; these numbers are evidence of a failed gate, not acceptance. No push was made. The preview-stage visual gap remains with B0152; existing default-bar paired QML/Bead crops remain linked above.

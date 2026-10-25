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

## Phase 2 candidate — B0114 (not accepted)

The editor keeps a bounded, eight-step draft history; only Done writes the
revisioned store or atomically replaces the standalone file. The first tile move
reserves the measured content widths of the content preset before changing its
mode. Reset restores the original preset. Unknown JSON values retain empty
objects, arrays, nulls and integers beyond the double-precision range across edits.

Canvas placement remains output-local. Canvas/card/bar variants share the same
catalogue; empty canvases allocate no surface, and visible content uses a tight
bottom-layer surface with two reusable buffers. Drag metadata is independent of
keyboard focus. Settings, catalogue, handles and drop targets remain in Luau.
No phase-2 parity, smoothness, first-key or memory gate is accepted yet.

| Gate | Candidate status |
| --- | --- |
| Remote focused transactions / canvas fallback / JSON preservation | Passed on the integrated candidate |
| Full CI after B0111/B0117/B0119/B0120 integration | Pending exact committed revision |
| Desktop paired captures, drag, key and five-pair memory | Pending |
| Idle canvas input | Decision needed: clickable controls versus entirely click-through |

The idle-input wording conflicts with pointer-activated dropdowns on desktop
widgets: a surface with an empty input region receives no widget clicks. The
candidate currently bounds input to visible controls; the background is empty.
The exact intended idle policy must be resolved before acceptance.

General APIs added here: canvas hosts, opaque drag/drop metadata, anchored panels,
lazy content-sized output canvases, child/host model and action projection,
direct app-to-pearl startup, shared image snapshots and JSON-preserving copying.
Editor drop zones opt into source-origin stores; metadata stays inert and does
not request pointer or keyboard focus. Title schemas reserve maximum widths
after the content preset becomes a grid. Custom width changes remain immediate;
animated flexible width is still a gap. Daemon store integration waits for the
published N0075 registration; standalone atomic-file persistence is available.

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

# B0113 — Widget system: detachable widgets, tile-grid bars, widget menu, music widget

Status: design accepted by the owner, 2026-10-25. Phase 1 is in progress. Owner request: "everything on the bar is detachable … the top bar is gonna be a grid … start with a music widget … we need a widget menu … make a good spec."
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

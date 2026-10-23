# B0047 — Desktop frame: live geometry, reserved edges, edge lips and hover intent

Status: **Review.** Owner 2026-10-23: the frame is crucial and must look and feel like the QML; Bead is a
general, editable framework, so the frame is configuration on general primitives.
QML reference: `drawers/NacreChrome`, `NacreFrameLips`, `NacreFrameRim`, `NacreReservedEdges`, `NacrePanelInput`,
`widgets/NacreFrameLip`, `NacreEdgeTab`, `NacreHoverDwell`, `services/NacreHoverIntent`, `config/NacreFrame`.

## What exists now

- **Frame look** (already there from B0036/placement): the rounded frame is static SVG tiles on the bar surface,
  joined to the bar and to an open panel by `contour.rs` (port of `contour.js`). Now **live**:
  `placement::Frame::current()` is set from `frameWidth` (0..30, default 10), `frameRounding` (0..40, 25),
  `frameLeft/Right/Bottom/TopEnabled` and the bar flag; the header is `40` (bar) `+` top edge, as `NacreFrame`.
  Tiles, panel placement (`positioner` reads it on every call), the rim radius bound
  (`min(rounding, opening/2)`), the bar's thickness and its exclusive zone all follow it without a restart.
  `Chrome::tiles(frame, ...)` builds only the strips that carry material (strip per enabled side, header band,
  a corner tile only where two frame edges meet), so each side toggle, zero rounding and a frame off all work and
  the default crops are byte-for-byte what they were (test).
- **Reserved edges** (`NacreReservedEdges`): `ShellControl::edge_chrome().reserved` -> one input-less layer
  surface per enabled side with the exclusive zone (left, right, bottom; the top only when the bar is off,
  because the bar's own zone is the header). Transparent 1x1 buffer scaled by the viewport, no per-pixel memory.
- **Edge lips** (`NacreFrameLips`): three lips declared in **`shell/frame/behavior.luau`** (id, edge, length,
  panel view); thickness is the edge's frame width. Side lips are small strip surfaces (frame width x 144 px) that
  draw the edge tab (`frame_art.rs`, port of `NacreEdgeTab`: accent cue filling while the pointer dwells, chevron
  that flips when the panel is open). The top lip is the bar's empty middle (208 px, invisible, as in the QML) and
  counts only where no bar control is under the pointer. A lip whose view is not registered or whose panel is
  disabled in settings is not created (QML `visible`). Shipped: top -> `dashboard-today`, right -> `sound`,
  left -> `assistant` (no Bead view yet, so hidden).
- **Hover intent** (`bead_wayland::intent`, pure and tested): `Dwell` (port of `NacreHoverDwell`: frame-stepped
  fill with a 35 ms step cap, two held full frames, plain timer without motion, zero delay fires at once,
  motion-off mid-dwell), `Gate` (`NacreHoverIntent` blocked/rearm: a panel dismissed under a resting pointer
  cannot reopen until the pointer leaves) and `Lip::rect` (the QML lip formulas). `lipHoverDelay` (integer 0..800,
  else 350) and `clickEdgeMenus` (click only) are live. Hover opens after the dwell when no button is held, hover
  is allowed (`frame.lips.blocked_by`: launcher and session, QML `lipsAvailable`) and the gate is open. Click
  toggles (open, or close if its panel is open). A hover-opened panel closes 130 ms after the pointer leaves both
  the panel and its lip; a click-opened one stays (QML `edgeMenu`).

## Framework primitives (ADR 0004: nothing Nacre-specific in `bead-*`)

| Primitive | Where | Note |
| --- | --- | --- |
| Edge-anchored chrome surface (reserved zone or lip strip) | `bead-wayland/src/edges.rs`, `ShellControl::edge_chrome` | Product returns `EdgeChrome {reserved, lips, hover_delay_ms, click_to_open}`; artwork is a product SVG (`Options::edge_art`) |
| Hover-intent helper | `bead-wayland/src/intent.rs` | `Dwell`, `Gate`, `Lip`, `may_dwell`; no Wayland, no clock |
| Contour joiner | `nacre-shell/src/contour.rs` | **Not general yet** (GPL Nacre port); ROADMAP Generalization list |
| Bar thickness | `ShellControl::bar_thickness` | Replaces the 50 px literal; the ROADMAP row is narrowed |

Which panels a lip opens, its length and its edge are Luau configuration; the Nacre settings ids stay in
`prefs.rs`/`shell_control.rs`; sizes come from settings, not literals.

## Memory (always on, 25 MiB laptop target)

Edge surfaces: four reserved strips are one `SlotPool` of 4 KiB each with a shared 1x1 transparent buffer; two lip
strips hold two small SVG buffers (10 x 144 at 1.5x is 13 KB). No fullscreen overlay and no timer while idle (the
dwell timer exists only during a dwell). Measured result below.

## Checks

- `bead-wayland` `intent::` (9) and `edges::` (2): dwell timing and state machine, gate, QML lip rectangles, input
  region math, strip geometry. `nacre-shell` `frame_config::`, `prefs::frame_and_lip...`, `shell_control::` (frame
  reserve/lips/blockers), `placement::` (frame from settings, tile crops incl. defaults unchanged, rounding bound,
  tile memory for widths 0..30), `frame_art::` (chevron direction, cue fill).
- Goldens `shell/tests/goldens/frame/frame-*.png` (800x500 at 1.5): default, frame off, bar only, rounding 0 and
  40, no left/right/bottom, top edge without bar, width 14, joined panel (default, wide+round, no sides), lips idle,
  mid-dwell and open. The default frame and all panels remain pinned against Qt by `placement-qt.json`.
- Nested proof `spikes/b0047-frame.py` (private labwc > niri, CPU): see Result.

## Not done / differences (one line each)

- `NacreFrameBarrier` (two-frame commit barrier for panel hand-off) and keyboard focus on lips (Tab/Enter/Space,
  `keyboardFocused`) are not ported; lips are pointer-only.
- Fullscreen suppression (`fullscreenFor`) is not applied: niri covers Top surfaces with fullscreen windows; on
  Hyprland a lip could open over a fullscreen window.
- The left (assistant) lip is configured but hidden: Bead has no AI sidebar view (B0026 blocker C).
- The right lip opens the `sound` panel; Nacre's control-center sidebar has no Bead view. Retarget in Luau.
- Frame and lip colours are fixed at start (as the static tiles already were); a wallpaper palette change does not
  recolour them yet.
- The cue and chevron redraw without the QML's 120 ms opacity fades (state is drawn at its target value).
- Tiles stay under the 2 MiB static-decoration cap for widths up to 30 at 2560x1440 at 1.5x; a 4K output at
  2x with a 30 px frame would exceed it and log `DECORATION_ERROR` (pre-existing cap).

## Temporary duplicates

None. `Frame::DEFAULT`/`Default` stay as the test baseline of the live frame.

## Result (2026-10-23)

- Full CI equivalent (`ci.yml` check job, Ubuntu container on prod): fmt, clippy `-D warnings`, boundary, workspace
  tests, trace tests, feature configs, facade/Luau check, spikes, packaging, service lifecycles, tray, cargo-deny: all passed.
- Nested proof `spikes/b0047-frame.py smoke` (headless labwc > niri 1278x692, CPU, no daemon), all six steps true:
  quick pass opens nothing; resting on the right lip opens after the dwell; leaving lip and panel closes it;
  click opens, a second click closes (the lips are recreated above an open panel); the top lip opens by hover in
  the bar's empty area. Reserved edges: a terminal's window shrank 615x610 -> 605x600 (10 px right, 10 px bottom
  on the 10 px frame; the left strip moves its origin). Screenshots showed the frame and the right chevron.
- Relative memory (3 alternating pairs, CPU/tiny-skia, no daemon, candidate - control, median): idle
  -0.043 MiB PSS (anon -0.055), after open/close -0.157 MiB (anon -0.078). Gate +0.5 MiB passes. Compositor-held
  SHM is excluded as in earlier gates; idle CPU is nil by construction (no timer while idle; not sampled).
- Not exercised nested: settings changes (no daemon: covered by unit tests and goldens), two outputs, fractional scale.

# B0104 — Pearl drag, dock and close

Status: in progress. Repairs B0048/B0054's moving-surface drag feedback and layer-only close limitation.

## Reference and design

The reference is Nacre `origin/main` `3afcdc64`: `NacrePearlWindow`, `NacreDockGuide`,
`geometry.js`, `NacrePanelDrag`, `NacreDockFilm`, `NacreDockTarget`, `NacrePearlBorder`,
`NacrePearls` and `NacrePanelDragRouting`. All geometry is logical pixels. A drag
uses the original body and a fixed grab point; framebuffer scale never multiplies
pointer travel. Docking keeps the existing 170 px capture / 72 px arm / 104 px hold.

During a docked pull the layer remains the pointer-grab owner. On detached release
the same worker and scene move to a fresh xdg toplevel. Native pearls receive the
compositor's close request. Nacre's existing `pearl-window` helper places owned
windows; the framework receives only generic identity and placement callbacks.
The helper queue keeps the newest position per pearl and releases its thread when
empty. Its work never blocks input handling.

On niri, QML intentionally hides the film during native compositor moves: there is
no live cursor contract. Landing guides remain visible and the completed geometry
determines docking. Bead consumes the same published compositor snapshot on the
existing UI-provider connection. It reuses the permanent notices topic when
present, otherwise holds a lease only while pearls exist. Both paths have a
subscription-lifecycle test.

## Close, dock and recovery

- Compositor close-window (including the owner's Super+Q binding).
- Ctrl+W on the focused pearl; view-owned close requests; numbered `pearls close ID`.
- Release armed at the view's edge, or `pearls dock ID`, to return it to the shell.
- `nacrectl ui pearls close all` closes open pearls and clears remembered entries,
  including disconnected outputs. Direct equivalent: `nacre-shell ipc --socket
  SOCKET bead.panel pearls-reset`. This extends the existing close route.
- Restored rectangles are bounded to the current output using the rendered body
  size, including stale undersized rectangles; invalid geometry is replaced.
  Only bounded, whitelisted view state is persisted, never provider data
  or credentials. QML's pearls do not use Escape as a window-close shortcut.

## Validation and evidence

Full `check` job replica passed on code `be24f66` in a disposable Arch container:
fmt, clippy with warnings denied, workspace/trace tests, both renderer feature
configurations, facade/Luau checks, Python suites, packaging, wallpaper smoke,
private services/tray lifecycle, and all dependency/license graphs. The archive
was restored to an exact shallow checkout for the packaging revision checks.
Arch needed its separate `swaybg` package and removal of Sway's unnecessary file
capability inside the container; the container privilege policy stayed intact.
Geometry tests include scales 1, 1.25 and 1.5, output clipping and stale saved
rectangles. State tests cover the view whitelist and owned compositor moves.
The native handoff resets idle-buffer timers tied to the old surface. Docking
retires GPU resources before replacing the native role with a fresh layer.
Private nested niri captures use 2880×1800 at scale 1.5, with a matching dark
poster, palette and fixture settings. A native pearl moved from `[410,600]` to
`[550,400]` for pointer travel `(+140,-200)`; a 180-step return to the bottom edge
docked it and removed that exact native window. Ctrl+W and Super+Q both closed
the focused pearl. These are synthetic input checks, not physical scanout timing.
Paired pull-out frames and settled memory checks are pending. No live session,
installation or configuration is changed by this unit.

## Remaining parity scope

Native pearls remain content-sized; compositor resizing still needs the generic
resize/layout work absent in B0048. The landing guide's 160 ms fade remains an
existing visual gap. Native Hyprland movement has not been captured by this unit.

## Generalization and expiry

Native surface handoff, motion notifications, saved-state hooks and output-bound
recovery are reusable framework primitives. Product titles, daemon decoding and
the placement helper remain in the shell adapter. No new temporary duplicate is
introduced; the existing Bead-local pearl store retains B0048's expiry after a
shared published store replaces it.

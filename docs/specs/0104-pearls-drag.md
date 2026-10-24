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
determines docking. Bead consumes the same published compositor snapshot through
a lease on the existing UI-provider connection, acquired only while pearls exist.

## Close, dock and recovery

- Compositor close-window (including the owner's Super+Q binding).
- Ctrl+W on the focused pearl; view-owned close requests; numbered `pearls close ID`.
- Release armed at the view's edge, or `pearls dock ID`, to return it to the shell.
- `nacrectl ui pearls close all` closes open pearls and clears remembered entries,
  including disconnected outputs. Direct equivalent: `nacre-shell ipc --socket
  SOCKET bead.panel pearls-reset`. This extends the existing close route.
- Restored rectangles are bounded to the current output; invalid geometry is
  replaced. Only bounded, whitelisted view state is persisted, never provider data
  or credentials. QML's pearls do not use Escape as a window-close shortcut.

## Validation and evidence

Pending remote full CI and paired desktop nested captures at 2880×1800 / 1.5.
No live session, installation or configuration is changed by this unit.

## Generalization and expiry

Native surface handoff, motion notifications, saved-state hooks and output-bound
recovery are reusable framework primitives. Product titles, daemon decoding and
the placement helper remain in the shell adapter. No new temporary duplicate is
introduced; the existing Bead-local pearl store retains B0048's expiry after a
shared published store replaces it.

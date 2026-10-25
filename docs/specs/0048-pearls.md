# B0048 - Pearls: detachable panels with dock guides

> B0128 retired the standalone `b0048-pearls.py` experiment. Current `pearl-release-proof.py`, pacing checks and Rust pearl tests remain; historical measurements below are preserved.

Status: **Review** (Claude worker, branch `task/b0048-pearls-20261023`). Owner 2026-10-23: pearls are crucial; Bead is a framework, so the
mechanics are general and the Nacre look and policy sit in one adapter. Reference: QML `NacrePearlWindow`, `NacreDockGuide`, `geometry.js`,
`NacrePearls`, `NacrePearlBorder`, `NacreDockFilm`, `NacreDockTarget`, `NacreWorkspacePearl`; Rust `pearl_state.rs`, `pearl_window.rs`.

## What a pearl is in the QML

A detachable panel (launcher, dashboard, AI drawer, control center, media mini player) becomes an **xdg floating window** (`FloatingWindow`,
title `Nacre Pearl <Kind> <n>`, positioned by `nacre-helper pearl-window` through niri/Hyprland actions, resizable, in the window list).
Super+drag a docked panel pulls it out (layer lift, then a native window opens under it behind a frame barrier); a held pearl shows the dock
guide (stationary landing field plus a "film" from the grab point to the panel edge, armed within 72 px, held to 104 px, shown within 170 px).
Released armed, it docks (`dock`); layouts persist in `~/.local/state/nacre/shell/pearls.json` (`nacre-helper pearl-state`, not a daemon
contract). Border: 2 px gradient, outline at rest, accent when focused or hovered. The workspace pearl in the bar is a separate widget (B0037).

## Structure

| Layer | Piece | Notes |
| --- | --- | --- |
| Framework | `bead_core::pearl` | Geometry ported from `geometry.js` (clamp, place, magnet with hysteresis, landing), `Tuning` (all distances), `Drag` state machine (docked pull / pearl move -> Return, Detach, Dock, Moved), `Layout` store (atomic, 0600/0700, unreadable file preserved). Edge, monitors, frame insets come from the caller. |
| Framework | `bead_wayland::pearls` | Detach/dock/close on the panel session layer: the resident panel group (worker, scene, renderer, surfaces) moves into a slot; a slot is processed by swapping it in around the existing panel code (as output sessions are). `PearlOptions`, `ShellControl::{pearls_enabled, pearl_allowed, pearl_drag}`, `UiEvent::{PanelDetached, PearlGuide}`, IPC `bead.panel detach / pearls / pearl-open / dock / pearl-close`, Super+drag gesture, guide slots, restore on output appearance. |
| Adapter (GPL) | `nacre-shell/src/pearls.rs` | Colours and shapes (body border, film, landing field), the guide `SceneSource`, `Policy` (which views), mapping of Nacre `pearls` routes. |
| Adapter | `placement.rs` `Attachment::Pearl`, `reload.rs` | Pearl chrome replaces the joined-frame art when `PanelDetached(true)` arrives; border lights up on hover. |
| Settings | `prefs.rs`, `shell_control.rs` | `detachablePanels`, `dragPanels` (`!== false`) and new `pearlPanels` (view names, trailing `*`; default `launcher`, `chat`, `dashboard-*`). Live. |

## Behavior

- Super+left-drag on a docked panel: after 16 px it becomes a pearl and follows the pointer; the guide appears on every output (landing field
  at the panel's edge, film while within 170 px). Release armed on its own edge: springs back docked; armed on another output: docks there;
  elsewhere: stays as a pearl. Super+drag a pearl: moves it; released armed, docks. Which edge a view docks to comes from its scene motion.
- `nacre pearls detach|open KIND|dock N|close N|state` map onto `bead.panel` (pearls are numbered). Ctrl+W closes the focused pearl. A pearl
  takes keyboard focus on click (on-demand), has no outside-dismiss layer and does not close on Esc.
- Remembered per output in `$XDG_STATE_HOME/bead/pearls.json` (version 1, same shape as Nacre's, `kind` = view name), restored when the
  output appears; entries for absent outputs are kept.
- A pearl keeps the panel's own worker, so state (tab, query, scroll) carries over exactly in both directions.

## Deviations (stated, not hidden)

- A pearl is a **layer surface** (Top layer, anchored top-left with margins), not an xdg toplevel: no window-list entry, no compositor resize,
  no sticky/pin, no tiling interaction, no compositor-specific code at all (niri and Hyprland behave identically; the only contract used is
  layer-shell). Reason: Bead has no xdg shell client and "published compositor contracts only". Cost: a pearl cannot be resized.
- The whole surface (shadow inset included) is kept inside its output: niri shrinks a layer surface that reaches past the output, which
  re-lays the view out. A pearl therefore stops about 22 px short of the screen edge. Observed in the first nested run (launcher shrank 654 -> 78 px).
- Border is painted under the content (a background image), not over it; panels have padding so nothing touches it.
- The guide has no 160 ms fade and the film does not animate its stretch (updated per pointer event).
- Art (film, landing field, border) is generated SVG in the adapter, coloured from the palette; only which views, tunings and colours are
  configuration. A different look needs a different adapter file, not a rebuild of the framework.

## Contract asks (Nacre side)

- **Persistence.** Nacre keeps `pearls.json` through `nacre-helper pearl-state`; there is no daemon topic. Bead keeps its own file. Ask: a
  `shell.pearls` store (or topic) with the version-1 layout so both shells share state. **Expiry:** delete the Bead-local file code 7 days
  after such a store is published and consumed.
- **Window kinds.** Nacre kinds (launcher, dashboard, ai, controls, media) map to Bead views in `commands.rs`; `controls` has no Bead view.

## Parity left

xdg window behaviours above (resize, window list, pin); captured view state is the live worker, but pearls restored after a restart reopen with
fresh view state (`initial` is not captured); control-center and media-mini pearls (no such Bead views); guide fade and film easing; QML's
native-gesture trace (`pearls gesture`); keyboard-less ways to close a pearl other than Ctrl+W and `nacre pearls close`; border drawn over content.

## Gates

Desktop nested niri (headless labwc > niri), CPU renderer (tiny-skia), no daemon, launcher from fixture, release builds of this branch
(candidate) and of `a0c5d6f` (control), 3 alternating pairs, PSS medians in MiB:

| Phase | control | candidate | delta | Gate (+0.5) |
| --- | --- | --- | --- | --- |
| idle | 24.71 | 24.69 | -0.01 | pass |
| docked panel open | 34.67 | 34.63 | -0.05 | pass |
| closed, settled 12 s | 25.08 | 26.46 | +1.38 (anon +0.02; the rest is file-backed) | **fail**, no anon growth |
| pearl open vs docked open (candidate) | - | 46.59 vs 34.63 | +11.97 (anon +10.5, shmem +1.5) | **fail**: not "about the same" |

Open: the pearl costs 10.5 MiB more anonymous memory than the same panel docked. Suspects: the full relayout `PanelDetached` triggers
(a second layout/bake of the 1100x590 launcher kept alongside the first) and a larger SHM pool. Not yet bisected. The +1.4 MiB file-backed
residue after close is code and font pages first touched by the pearl path.

Drag smoothness: **not met / not measurable here.** In the nested pixman compositor the guide scenes presented at p50 40 ms, p99 61 ms (38
frames over a 40-step drag); pearl moves themselves are margin commits with no scene work (guide scene prepare 0.3-0.5 ms), so the
presented cadence there is the nested compositor's. Needs a run on real output (lead capture).

Nested smoke (`spikes/b0048-pearls.py smoke`): detach via API lists one pearl and closes the docked panel; drag moves the pearl (known
off by about 57 px when the drag runs into the output edge clamp); guide surfaces are released after the drag; layout saved 0600; release at
the bottom edge docks; `pearl-close` removes the surface; a restarted shell restores the pearl at the same rectangle. 8 of 9 checks true,
`drag_moves_the_pearl` false (the 57 px).

## Validation

`bead_core::pearl` 16 tests (geometry of every edge, hysteresis, overlap, multi-output, drag outcomes, store). `nacre-shell` pearls: 5 art/policy
tests and 10 goldens (docked, detached, detached-engaged, drag-pulling, drag-armed at 1 and 1.5; eyeballed). Full Bead CI replica (fmt, clippy -D
warnings, boundary, workspace tests, renderer configs, facade, luau, py) passed on dev-1. Nested niri smoke (`spikes/b0048-pearls.py smoke`).

## Temporary duplicates

QML pearls stay until the switch is trusted (Nacre rule: delete a week after). The Bead-local `pearls.json` has the expiry above.

# B0200 — Detach ghost

Owner (laptop: eDP-1 1920×1200 logical at scale 1.5 on the left, DP-1 3440×1440 at
scale 1 on the right): detaching a panel shows "a huge ghost thing … appearing on the
left". Panels already open on the right output and slide smoothly (B0194, B0185).

## Causes

1. **Prepared target on the wrong output.** A drag press prewarms a transparent
   native toplevel (B0131/B0139). niri maps every new window on its *active*
   output at the `default-floating-position` bootstrap pose (`x=0 y=0`,
   `shell/niri-pearls.kdl`); the prewarm only *queried* that pose. On release the
   target moves by `wl_surface.offset(rect − anchor)`, which niri applies within the
   window's own output. With a panel on DP-1 and the active output eDP-1, the pearl
   content was committed into a window that lives on the left output, at the left
   output's scale. Single-output sessions (all earlier proofs) cannot show it.
2. **Dismiss catcher left mapped.** Detach only disabled the panel's full-output
   `bead-panel-dismiss` layer; it stayed mapped (transparent, 1×1 viewported) for the
   pearl's whole life. Docking recreated a second one.

## Fix

- Adapter (`nacre-shell/src/pearls.rs`): the niri prewarm first runs the existing
  `pearl-window place` (MoveWindowToMonitor + MoveFloatingWindow) to the docked
  body, waits the same 400 ms settle barrier as `place`, then reads the anchor.
  The target is transparent and input-less while it moves, so the move is invisible.
- Framework (`bead-wayland/src/pearls.rs`): `activate_prepared_native` admits a
  prepared target only when its anchor and the release rectangle are on the same
  output (`same_output`, general over `Monitor`). Otherwise the existing covered
  fallback creates the window and `place` moves it across outputs before its first
  content frame.
- `become_pearl` destroys the dismiss layer instead of disabling it;
  `dock_pearl` already recreates it. The disable-only path is gone.

## Evidence ([evidence/0200](evidence/0200/))

`spikes/detach-ghost-proof.py` wraps the parity harness with niri animations ON
(owner defaults; earlier proofs ran with `animations { off }`) and the owner's
rounded-window rule. Per view it drag-detaches with the held relative pointer,
captures full-output frames every 100 ms through release and settle, docks via
`bead.panel dock`, and replays the shell's `WAYLAND_DEBUG` requests to list the
role surfaces whose last commit carried a buffer. Desktop test host, private nested
niri 2880×1800 @1.5, CPU renderer, release builds of `0e9a208f` (base) and this
branch (cand), real `shell/` tree.

| View | Base: mapped after detach (vs closed shell) | Candidate | After close |
| --- | --- | --- | --- |
| launcher | pearl xdg + `bead-panel-dismiss` | pearl xdg only | clean / clean |
| dashboard | pearl xdg + `bead-panel-dismiss` | pearl xdg only | clean / clean |
| control | pearl xdg + `bead-panel-dismiss` | pearl xdg only | clean / clean |

Frames (`base-*.png`, `cand-*.png`): no stray content anywhere on the output during
detach or dock in either arm on one output; the candidate run is strict (fails on any
extra mapped role surface). 0 `UI_ERROR`. Unit tests: `same_output` (two outputs at
the owner's geometry) and the mapped-surface replay (`spikes/tests/test_detach_ghost_proof.py`, in CI).

## Gaps (stated)

- **No two-output niri reproduction.** niri nests only as one winit output and has no
  headless CLI backend; the cross-output cause is derived from niri's window-opening
  rule, the helper's global geometry and the offset handoff, and is covered by the unit
  test, not by frames. The lead's live laptop audit is the acceptance.
- The AI chat panel is a separate on-demand app process; it was not driven here.
- Status dropdowns are not in the default `pearlPanels` policy.
- Re-attach was driven through `bead.panel dock` (same `dock_pearl` path as a drag
  back), not a compositor window drag.
- Prewarm now costs one extra helper call and 400 ms before the target is admitted;
  an earlier release uses the covered fallback (existing behaviour). No memory change.

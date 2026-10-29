# B0195: initial fractional scale for every surface kind

Status: **Candidate.** Measured on the desktop TEST host (private labwc 2880x1800@120 > niri scale 1.5, release
`nacre-shell`, default renderer, `tools/daily-acceptance/animation_audit.py`, 3 repeats, base 35ac2f88 vs candidate,
two alternating runs each). The owner's laptop live audit stays the acceptance.

## Problem
B0192 found the overlay rastered its first frame at the integer `wl_output` scale (2 at 1.5x) until the
fractional-scale event corrected it. Each surface kind chose its own start scale: overlay and canvas had private
"look at a sibling surface" code, bars/panels/edges used the integer factor, and the window used a literal 120.

## Change
One helper, `App::initial_scale120(kind, output, fractional)` (runtime.rs), with the pure choice
`choose_initial_scale120(known, integer_factor)`. A surface that already holds its fractional scale on the same output
(any configured surface/companion, parked slot panel, edge lip, canvas or overlay) supplies it; otherwise the integer
factor (120 without output info). It runs once, at creation, so scale changes and surfaces on a second output keep
working: the fractional-scale event still sets `scale120` as before, and a scale-1 monitor only ever sees its own
surfaces' scale. Used by bar/panel/companion, edge lips, overlay, canvas and window. Every use logs
`INITIAL_SCALE kind= scale120= known=`.

Removed: the overlay's and canvas' private copies of this logic and the window's literal 120 (net about -20 product
lines in those modules; +50 for the helper, its field visibility and the test). No temporary duplicates.

## Measurements (journal `INITIAL_SCALE`, candidate)
| kind | start scale120 | known |
|---|---|---|
| bar (first surface of the output) | 120 | no (output info not yet delivered) |
| edge lips (created with the bar) | 240 | no (integer factor; no fractional surface yet) |
| panel / companion (15 per run) | 180 | yes |
| overlay | 180 | yes |
| canvas | 180 (same rule as before, now shared) | yes |

First-content time per panel route is unchanged within noise (base / candidate medians, ms, open first frame, two
runs each): launcher 13.1 / 13.1, dashboard 59.9 / 60.3, control-centre 60.0 / 60.4, popout-audio 46.2 / 46.3,
notification 7.3 / 7.2. Honest reading: the shell creates bar, panel companion and edge surfaces at output setup and
the first fractional-scale event corrects them long before any route opens, and a layer surface's first prepare waits
for its configure, which follows the scale event. Only the overlay prepared before the event, which B0192 already
fixed (40 -> 10 ms). So the shared helper gains consistency and removes duplicates but no measurable first-frame time
for panel/bar. Windows (xdg) could not be driven in the pacing harness (`widgets-gallery` is not a window route
there), so their gain is unmeasured; they also wait for configure before the first prepare.

## Tests and checks
- `runtime::initial_scale_tests` (2 tests): known fractional scale beats the integer factor (180 vs 2; a scale-1
  second output keeps 120); unknown output or no info falls back to integer or 120.
- Real shell (`shell/` tree, base and candidate, 4 runs of 6 panel routes + 2 runs window attempt): 0 `UI_ERROR`.
- dev-2 container (ubuntu 24.04, rust 1.99.0): `cargo fmt --all --check` (fixed one window.rs wrap, now clean),
  `clippy --workspace --all-targets -D warnings`, `check-service-boundary.py`, `cargo test --workspace --locked`
  all green (793 passed in the largest suite).

## Not done
- First surface of an output (bar, edge lips) still starts from the integer factor: nothing on that output knows the
  scale yet. A per-output cache that survives surface teardown would only help re-created surfaces.

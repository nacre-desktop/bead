# B0102 — Dashboard owner fixes

Bar-only pill state no longer reaches companion runtimes: `status.hovered` had
made dashboard reload fail on pointer entry. The urgent fix is published alone as
`ac194d7`. A real Runtime regression loads all 13 pages and exercises pointer,
timer, selection, dismissal and reload. Actual tab-click captures also exposed
host commands returning to the panel script; the Wayland host now dispatches
those commands after checking the panel's lifetime epoch.

Today restores the wallpaper hero and calendar/host/media columns. The host card
uses published system and updates data; the mini player has circular artwork,
progress and icon controls. Luau owns the layout and theme roles. `ui.Arc` is a
reusable signed-angle stroke for CPU/GL, with no angle-specific raster cache.
The existing descriptor-pinned worker supplies a bounded still wallpaper;
closing drops page sources and image ownership. The default fallback is compared
with an actual decorated Runtime, preventing a second SHM surface geometry.

The QML reference profile has no configured ICS calendar. Its Today summary is a
month calendar, so the former agenda/"Calendar is not configured" card was removed.
Media artwork/transport geometry and Weather's half-degree rounding were also
corrected. No launcher engine, daemon contract or backend implementation was added.

## Evidence

The captured product revision is `3efe724`, based on `44a2004`. Its matched CPU
control is that base plus the identical host-routing fix (`18dc581`). Both builds
use the same path, flags and output layout. Later main changes are integrated and
checked by CI; these numbers do not claim a combined memory pass. Private captures use 2880×1800 at 1.5,
the same read-only wallpaper/palette record, a playing MPRIS fixture with artwork,
and synthetic system display values. Real source transports remain active.

Every arm waits on blocking `flock` without a timeout and releases it afterwards.
Checks require the requested semantic page, a real companion scene commit,
playing artwork, full output geometry and no UI errors. System fixture values
are checked through the actual module symlinks and committed stores. Earlier
mismatched fixture/selection attempts were rejected. No live session was changed.

[Paired images and method](evidence/0102/README.md),
[raw-PNG hashes and RMSE](evidence/0102/visual.json) and
[memory components](evidence/0102/memory.json) record the comparison.
Clock and playback position advance between arms; the raw RMSE includes them.
JPEGs are labeled display copies; RMSE uses native RGB PNG pixels without resizing.

Full remote Arch CI passes: [commands](evidence/0102/check.sh),
[results](evidence/0102/check-results.txt). Twelve affected goldens were regenerated
at 1×/1.5× and visually inspected. The joined-reveal regression also checks Arc
translation/clipping in direct and worker-prepared frames.

## Memory review

The relative gate **fails**. These are median paired candidate-minus-control
unreclaimable deltas, in MiB; the limit is +0.5 MiB for each phase.

| Phase | Open | Closed + idle |
|---|---:|---:|
| Idle | +0.2422 | −0.0039 |
| Today | +7.5508 | +0.3555 |
| Media | +0.9531 | +1.0547 |
| Weather | +0.7109 | +0.9453 |
| System | +1.4727 | +1.1953 |

The gated sum is anonymous + SHM + DRM + SwapPss, using five captured component
samples per phase and three alternating pairs. Paired-delta medians need not equal
the difference of the two group medians; all per-arm component medians and
paired deltas are retained. Clean-file PSS
is informational in `memory.json`. DRM and SwapPss are zero for these CPU runs.
Today adds photo/shade raster residency while open. Traces report zero retained
image pixels, workers and panel SHM after retirement, but closed anonymous memory
still rises; its remaining cause is not established. An encoding-offload attempt
worsened memory and was removed. The final source keeps the existing decoder path.

This is a review candidate with a failed memory gate, not memory acceptance.
The absolute laptop cap needs the separate lead reference run; these private
captures do not establish laptop or live-desktop acceptance.

## Remaining visual differences

- Navigation: Today/Dashboard label, selected-tab hover/pressed fill and some text
  baselines/icons. Bead remains on its fallback during the sampled 1.4-second
  cold-open window, and its close is later than QML. These are capture-window
  observations, not physical scanout timestamps; timing parity is unresolved.
- Today: wallpaper actions/motion, home-time and palette captions, cached-weather
  icon/unit/label, hero rim, logo highlights, gradient rings and audio halo.
- Media: player identity, split artist/album and their placement, track-details
  fallback, blurred background/rounded art, shuffle/repeat, draggable seeking and
  timed lyrics. Existing ±10-second seeking remains available.
- Weather: unit labels, conditions details/sun path, forecast graph/range layout,
  weekday labels and typography. System: four ring gauges including Storage,
  network/process layouts and selection controls, and the full updates footer.

These gaps remain visible in the evidence; this is not an exact-parity claim.

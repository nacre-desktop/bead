# B0136 — notification icons, launcher favourites and Today wallpaper controls

Restore the QML notification app identities, filled favourite state and Today
wallpaper action layout. Ship existing contracts first; then publish the missing
daemon previous-wallpaper operation before consuming it. Keep shell layout in
Luau and wallpaper history in the daemon. Private nested captures only.

## A — existing contracts

- Preserve notification `icon`, `desktop_entry` and `image_path` in popup and
  history adapters; resolve local/themed icons with QML's compact notification
  brand fallback (including its special `nacre` / `nacre-notifications` identity).
- Match QML `favorite` with FILL=0/1. The two palette-templated SVG outlines are
  instantiated from Material Symbols Rounded at FILL=0/1, wght=400, GRAD=0,
  opsz=24 (Apache-2.0). This avoids a special renderer path for one icon.
- Today: Wallpapers and Colors, 111×36 with 6 px gap; a 228×38 transport below,
  6 px gap, right inset 18, bottom inset 12. Match QML glyphs and availability.
  Next uses `wallpaper.next`; dynamic pause uses the existing wallpaper-picker
  preference. Previous remains visible and disabled until B is published.
  A can gate Next on the advertised method and a fresh active topic; the exact
  rotation-pool readiness signal is added in B for both directions.

## B — daemon history and previous

Add a bounded daemon rotation history and an additive `wallpaper.previous`
contract, including availability in the wallpaper topic. Match QML: discard
history entries outside the current rotation pool, skip the current wallpaper,
use the last valid history entry, otherwise wrap to the preceding pool entry.
Only successful selections update history. Publish Nacre with full desktop CI,
then pin the published protocol and wire Bead's Previous action.

## Acceptance and evidence

Fake-daemon contracts and full repository CI on remote container hosts; regenerate
and inspect changed goldens. Capture QML-left/Bead-right notifications, favourites
and Today idle/hover/pressed in identical private desktop sessions under the shared
capture lock. Record measured gaps explicitly. Budget: ≤1 MiB new unreclaimable
always-on memory; no eager panel loading. No replaced duplicate path is retained.

A passed the full `.github/workflows/ci.yml` job equivalent in an Arch container
with Rust 1.99.0: formatting, Clippy with warnings denied, workspace tests
(636 shell tests passed, one ignored), trace memory-policy tests, renderer
configurations, Luau/schema, Python harness tests, packaging, video smoke,
private services/SNI and every dependency/license graph. Changed goldens were
regenerated and inspected. Paired desktop image/action proofs and memory
measurements are still in progress; no final parity or memory acceptance is
claimed at this A checkpoint.

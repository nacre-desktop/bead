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

Producer published: Nacre `70459b21c7010fb2422c7778c953b58ccf8011f0`.
The consumer pins that revision and its wallpaper-active transcript. The shared
protocol now also requires `base64` 0.23.1; no new OS build dependencies are added.

`wallpaper.active.previous_available` defaults false and carries QML's shared
manual-navigation readiness for both arrows. Bead requires that flag, a fresh
validated topic, an advertised method and an idle request lane. An older producer
that omits the flag leaves both arrows disabled. The daemon clears readiness
while selecting, asleep/locked, stale/unavailable or in a pool with fewer than
two entries, and rechecks it after asynchronous library lookups.

History is capped at 100 source paths of at most 4 KiB each. A successful helper
selection enters history only when the producer observes its valid presentation.
QML's separate history remains for the daily driver; delete it seven days after
the owner-approved Bead core switch is trusted (Nacre companion spec).

## Acceptance and evidence

Fake-daemon contracts and full repository CI on remote container hosts; regenerate
and inspect changed goldens. Capture QML-left/Bead-right notifications, favourites
and Today idle/hover/pressed in identical private desktop sessions under the shared
capture lock. Record measured gaps explicitly. UI budget: ≤1 MiB new
unreclaimable always-on memory; daemon/provider totals are separate. No eager
panel loading or replaced Bead path is retained.

A passed the full `.github/workflows/ci.yml` job equivalent in an Arch container
with Rust 1.99.0: formatting, Clippy with warnings denied, workspace tests
(636 shell tests passed, one ignored), trace memory-policy tests, renderer
configurations, Luau/schema, Python harness tests, packaging, video smoke,
private services/SNI and every dependency/license graph. Changed goldens were
regenerated and inspected. Paired desktop image/action proofs and memory
measurements are still in progress; no final parity or memory acceptance is
claimed at this A checkpoint.

B consumer passed the full Arch CI equivalent on validation candidate
`7b4763e1aed61c96b07bce8ab4f92cfd205387e3` (base `ef7496f7`), including
the published readiness/Previous calls, empty preference defaults and disabled
states. The protocol pin also refreshes the producer's bounded updates transcript.
Candidate/control private desktop proofs are in progress; measurements use CPU
rendering at 2880×1800, scale 1.5, with matching synthetic dashboard values.

Bead's private capture and full preview start the daemon with
`--notifications=daemon --behaviours=daemon`; QML retains legacy behaviour
authority. The latter flag is required for the readiness/session worker and
manual navigation. All capture buses, compositors and homes remain private.

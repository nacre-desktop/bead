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
  preference. A shipped first with Previous disabled; B enables both arrows
  through the published rotation-pool readiness signal.

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

## Validation

Shipped A: Bead `678ccd55`; shipped B consumer: `3747103b`. Both passed the
full `.github/workflows/ci.yml` equivalent in an Arch container with Rust 1.99.0:
formatting, Clippy with warnings denied, workspace/trace memory-policy tests,
renderer configurations, Luau/schema, Python harness tests, packaging, video
smoke, private services/SNI and every dependency/license graph. B's integration
run passed 650 shell tests, one ignored; the
[official Bead job](https://github.com/nacre-desktop/bead/actions/runs/40249072122)
also passed on the published consumer. Changed A goldens were regenerated and
inspected; B passes those goldens unchanged. Fake-daemon tests cover successful
Previous/Next, disabled/unavailable/older-producer states and pause preference
defaults with revision-checked full-document updates.

A real pointer-release capture exposed an additional host dispatch bug: Today
buttons activate on release, but `dashboard.do` accepted only click/Enter and its
native Lua fallback was missing. Published `5fafc0b6` accepts and consumes the
release and supplies the same host-noop fallback as other native views. The
regression drives the actual Runtime through press/release: no call on press,
exactly one call on release for Wallpapers, Colors, Previous, Pause and Next,
with no pending input. The old implementation failed this test. Five fresh
repeat runs pass; the exact published head passes full Arch CI (654 shell tests,
one ignored) and [official CI](https://github.com/nacre-desktop/bead/actions/runs/40253223560).

The final Colors correction is `2526dee3`: `colorsApp.open` requires one image
string, so Today now sends `[""]` for no image. The real provider rejected `[]`
before app startup; the fake-daemon test now also checks command-catalog
validation. Ten focused Today tests and the full Arch job pass (654 shell tests,
one ignored); the exact integrated published head also passes full Arch CI
(659 shell tests, one ignored), and
[official CI](https://github.com/nacre-desktop/bead/actions/runs/40260337705) passes.
This corrects the Today button and its five palette dots; the
separate Settings Colors caller has the same pre-existing empty-argument shape
and remains outside this unit.

The Nacre producer passed its full Arch desktop equivalent, published-head
fmt/Clippy and 797 helper tests, then the
[official desktop job](https://github.com/nacre-desktop/nacre/actions/runs/40244402477).
The consumer pin also refreshes the producer's bounded updates transcript.
No new OS build dependency is required; private verification used Python
D-Bus/GObject, and FontTools instantiated the two favourite glyph assets.

Bead's private capture and full preview start the daemon with
`--notifications=daemon --behaviours=daemon`; QML retains legacy behaviour
authority. The latter flag is required for the readiness/session worker and
manual navigation. All capture buses, compositors and homes remain private.
The private preview was refreshed to Bead `2526dee3` and newer Nacre
`c9adf0f4` (which contains the producer), without launching it on the live desktop.

## Paired evidence and memory

Private CPU captures use 2880×1800 at scale 1.5, the same generated wallpaper,
palette, settings and synthetic dashboard values. QML source is `70459b21`.
Notification/favourite captures use Bead `7b4763e1` on `ef7496f7`. Memory
candidate `a31bfe58` adds the exact release fix to that snapshot; control
`527323ea` is `ef7496f7` with A reversed. This isolates the unit's memory cost
from independent upstream changes. Final Today uses `ad75dad8`, the isolated
`5fafc0b6` base plus the exact Colors correction published as `2526dee3`.
The published integration has separate full CI. Nested SCHED_IDLE screenshot
sequences are image proxies, not physical scanout measurements. Pointer proof
uses `BEAD_PRIVATE_INPUT_PROOF=state`: presence exposes inspection state without
`=1`'s pearl-drag test mode, which intercepts ordinary presses. Action captures
made with that drag mode are excluded.

Paired crops (QML left, Bead right; identical crop without resizing):
[notification source variants](evidence/0136/notification-app-icons.png),
[actual bell history](evidence/0136/notification-history-popout.png), and
[favourite idle/hover](evidence/0136/launcher-favorite-stars.png). Adjacent JSON
records source/binary hashes, geometry and actions. The four icon fixtures cover
theme name, local app icon, desktop-entry hint and local image-path fallback.
Both launchers hide the favourite marker at rest and show the filled glyph on
hover; the inherited initial selection and grid styling still differ.

Today pairs cover [availability](evidence/0136/today-availability.png),
[navigation](evidence/0136/today-navigation.png),
[static/dynamic pause](evidence/0136/today-pause.png) and
[Colors](evidence/0136/today-colors.png), with
[per-state hashes and action receipts](evidence/0136/today-controls.json).
All 13 final Bead states pass. Next changes the actual active wallpaper; Previous
returns to the prior wallpaper; a singleton pool disables Previous without a
selection; Pause persists `paused=true`; Colors opens and draws a 1100×760
window. The corrected QML Colors route opens a 1100×780 window. Colors uses a final
static-only Bead verification arm: both active records name `generated-1.png`
at 38,237 bytes, and the crops show the same gradient. The original temporary
wallpaper files were not retained for a SHA comparison. Its three-item pool
versus QML's five-item pool does not serve as navigation evidence.

The [open/close frame strips](evidence/0136/today-motion-proxy.png) expose an
unresolved opening gap: QML reveals Today over the sampled 101–302 ms interval;
Bead Today is absent through the last 790 ms open-proxy sample, although the
later settled screenshot shows the complete panel. Concurrent screenshot
scheduling and SCHED_IDLE prevent an exact appearance-time or dropped-frame
claim. Smoothness parity is **not established**. Dynamic pause proves the
preference contract only: the private Bead video environment lacks libmpv,
and independently timed video frames are not a pixel-aligned comparison.

UI budget declared before measurement: ≤1 MiB new unreclaimable memory
(anonymous + shmem + DRM + SwapPss); the existing-behaviour regression limit is
+0.5 MiB. Require five alternating same-host pairs for idle, Today, hovered
favourites and the actual bell dropdown, including each closed+idle checkpoint.
Daemon/provider totals and clean-file/font PSS are reported separately.
All ten memory arms passed source/settings, geometry, swap and process-tree
receipts. Each checkpoint uses five samples 100 ms apart; the table gives
control/candidate medians and the median of five paired differences, in MiB.
The full [sanitized measurements](evidence/0136/memory.json) include component,
file/font and total-PSS values plus per-pair differences. Wrong-route captures,
legacy-daemon-authority runs and failed harness attempts are excluded.

| CPU checkpoint | UI control | UI candidate | UI paired Δ | Daemon/provider paired Δ | UI file Δ | UI font Δ |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Idle | 41.176 | 41.766 | +0.516 | +2.070 | +0.317 | +0.000 |
| Idle closed | 41.176 | 41.766 | +0.516 | +2.070 | +0.317 | +0.000 |
| Today open | 69.795 | 70.287 | +0.340 | +2.277 | +0.308 | −0.031 |
| Today closed | 62.197 | 61.299 | −0.941 | +2.285 | +0.308 | −0.031 |
| Favourites hovered | 66.100 | 67.775 | +1.703 | +2.285 | +0.209 | +0.004 |
| Launcher closed | 58.287 | 58.420 | +0.117 | +2.371 | +0.246 | +0.000 |
| Bell dropdown open | 55.447 | 55.553 | +0.383 | +2.363 | +0.180 | +0.000 |
| Bell dropdown closed | 53.898 | 53.781 | +0.176 | +2.203 | +0.345 | +0.000 |

**Memory gate fails.** Idle exceeds the +0.5 MiB regression limit; hovered
favourites exceed the declared 1 MiB feature budget. Both increases are anonymous
memory, not clean code/font pages. Idle paired deltas are
+0.148/+0.516/+0.512/+1.293/+0.543 MiB; hovered favourites are
+2.863/+1.703/+1.793/+1.340/+0.570 MiB. These are failing medians, not a
single discarded arena spike. No compensating smoothness benefit was measured.
Daemon/provider increases are also anonymous and require separate investigation;
the UI budget is not applied to them. DRM and SwapPss are zero throughout this
CPU cohort; host swap remained zero with no swap-in. Root/child member sums and
actual dropdown closure were checked. The closed dropdown remained resident,
matching the inherited keep-alive policy; the sample does not claim eviction.

The cohort predates only the final Colors argument correction and never opens
the Colors app. Final action proof uses a separately built corrected binary;
the frozen memory binaries were not replaced. These unoptimized private CPU
captures establish a relative failure, not laptop-cap, GL or physical scanout
acceptance. Stop at the memory gate; no further feature work or live switch.

Observed limits outside the repaired icon identities, favourite fill and control
row: QML notification contours/typography and taller history cards with a clipped
scroll viewport differ from Bead's compact rows. Launcher grid/detail-pane behaviour
and Today weather/palette-caption/media styling still differ. Image-path-only
notices use that image in Bead's header; QML uses its compact Nacre fallback.
Bead's transport has accessible labels but lacks QML's delayed tooltips. The legacy
`nacre controls notifications` route opens a dashboard page in Bead; bell
dropdown proof must use the native notifications view, not that older route.
No eager panel loading or replaced Bead path is retained.

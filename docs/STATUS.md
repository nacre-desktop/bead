# Status

## 2026-10-17 — Placement accepted; first dashboard pages next

Owner accepts measured placement/joined-frame candidate d88e94e. Its code is
integrated unchanged onto main. Fresh local checks pass: 220 workspace tests,
trace checks, fmt, Clippy with warnings denied and CPU-only build. Eleven real-Qt
geometry/rim cases and paired 2880×1800/1.5 placement masks pass, with exact
solid interiors. Prior measured-source checks: 220 workspace tests, trace tests,
fmt/clippy/CPU-only/release pass. Strict mapped-close hover race fixed; no proof
weakened.

Five alternating pairs per compositor plus native20: all 424 close/settled
samples (244 candidate) fit own bar + 5 MiB. Maximum final settled UI is
23.096/23.648 MiB H/N; maximum earlier closed-idle is 23.149/23.711 MiB.
Paired settled increase is +0.729/+0.861 MiB: file-backed +0.313/+0.428,
shared frame buffers about +0.260 both, anonymous +0.098/+0.109 MiB. Component
medians need not sum. No saving/noise claim. [Measured
report](research/feature9-gate-d.md#placement-accepted--2026-10-17).

New memory rule replaces the per-feature allowance: all closes ≤ own bar +
5 MiB, settled ≤ 25 MiB; explain every increase. STOP at settled ≥ 24.5 MiB,
unexplained anonymous growth > 1 MiB in one change, or any memory-raising pure
optimization. No placement hold remains. Stop after the first dashboard pages.

Dashboard WIP 911688e is preserved, unbuilt: scoped topic leases on the existing
provider link, three separate lazy bundles and schema-shaped fixtures. Next:
adopt full-green Nacre c2b26d98, build/test, add goldens and measure. Reserved
core topics are NOT hosted capabilities. Existing observational lane is
helper.execute / dashboard-data / ["read"]; media needs caller metadata, system
is standalone. No guessed player/MPRIS source or CPU percentage. Explicit
fixtures and unavailable real pages remain honest. Reader trim API is upstream
unpublished F3; do not adopt before published green. Device implementation is
post-F4. No new backend or action inference.

## Published baseline and open gates

Prior status-view source f1d66af and docs cbd7031 have green CI 39003597141 /
39004077385. Report ecc90fa CI 39016083571 is green. New placement CI pending.
Frozen helper 75157 remains the A/B control; no desktop job/slot. No live UI,
installation/config/unit/keybinding changes; validation plan HOLD. QML daily.

Typing remains unaccepted: last compatibility pair UI 9.963/10.922 ms per key,
presented p99 30.886/31.562 ms H/N. Targets 8 ms / 16.7 ms; keep 29.61 →
32.66% regression. Recheck new core chrome before claiming a typing gain. Public
ui.menu.activate stays denied; shared QML/executor/hosting wait upstream.

Core forecast: 40–81 eight-hour agent-days (320–648 task hours), practical 3–8
calendar weeks including waits. Full parity: 102–207 days / 7–18 weeks;
complete-core idle 90–150 MiB is an estimate. No measured dashboard pace yet.
Keep 6b niri pacing/strict idle, long chaos, reader/cache gaps and binary-name
clash open. CPU for small panels; GL for large animation (~20 MiB Mesa residue).
IME/reader beyond existing work deferred to public release.

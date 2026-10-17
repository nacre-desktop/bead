# Status

## 2026-10-17 — Calendar and launcher restoration verified

Calendar Source-only wakes now project changes; unchanged wakes stage no scene.
The date follows today until browsed and keeps that choice on reload. Retired
calendar view state releases after idle; year/month/query are never saved to disk.
The bar drops two unused startup-state copies. A restored Search/Actions route
now reconstructs its transient mode, fixing Apps-mode reopening after retirement.

197 tests, isolated trace allocator proof, fmt, all-feature warnings-denied clippy
and CPU-only build pass. Actual-widget red/green tests reproduce the old bugs.
The accepted native control also reproduces the route bug; the final candidate
passes Search on Hyprland/Actions on niri with exact presentation feedback.
All 22 closed/settled samples fit own bar+5; settled23.050/23.749 MiB is below25.
Cache, process-group and cleanup guards pass. No live test or service takeover.
[Source, provenance and measurements](research/feature9-gate-d.md#calendar-source-and-restored-launcher-modes--2026-10-17).

| UI MiB, earlier Source snapshot | Hyprland | niri |
| --- | ---: | ---: |
| One-pair settled, control → Source |23.110 → 22.833|23.780 → 23.785|
| Open calendar, Bead / actual QML calendar+header |28.486 /195.153|31.200 /176.293|

The memory screen is one pair, within preceding control spreads, not a new
optimization claim. Calendar comparison is matched geometry/content, four-second
warmup/60-second observation; header fixture data/shadow and service activity
differ. It is not full-bar parity or an isolated-renderer CPU comparison.
Nested CPU scale1.5, Hvirtual120/N60; refresh unknown, host uncontrolled,
no own build overlap. Missing/physical measurements are not inferred.

## Closed-panel cleanup — published 3fb011a, CI38989066777 green

Only the bar bundle stays loaded. Panel modules/icons/stores/subscriptions/rasters
release after the 2.2-second grace; unused preparation expires after2 seconds.
The launcher scalar mirror clears too. App-only4MiB mmap policy fixes older
large-raster retention, with no framework allocator/font rewrite.
Five memory pairs/compositor: warm third median48.102→38.350 /47.032→45.603MiB;
settled paired+0.147/+0.097 lies inside control spreads0.300/0.735.
All158 candidate closes pass ownbar+5, maxsettled23.120/23.924. One control
fails six close budgets and remains invalid/exit1. Warm ranges vary; no settled
saving or uniform peak reduction claimed. Five CPU pairs show no typing gain.
[Full audit](research/feature9-gate-d.md#closed-panel-residency-audit--2026-10-17).

Next: check this checkpoint's CI, then test/measure the preserved ranked-prefix
UI typing draft. Targets remain UI8ms/key and presented p9916.7ms; historical
29.61→32.66% typing regression stays open. No new panel or dashboard has started.

## Core-switch scope and standing rules

Continue daemon typing/contracts, then remaining bar popouts/service views.
STOP for review after those, BEFORE dashboard; QML remains the daily driver.
Typed action contract is upstream CI-green; executor/same-engine QML are pending.
Keep the frozen measured helper for A/B; no ranking/provider/backend duplication.
Full AP/audio/Bluetooth/power/tray contracts/hosting remain unavailable;
network.watch is only an availability/change trigger. Current views are partial.

Feature allowance0.5MiB is per explained feature, not commit. Pure optimizations
preserve warm/settled control spreads; every close fits ownbar+5 and settled UI
stays≤25MiB on both compositors. Closed panels load lazily and release after idle.
No new ceiling requested. Core forecast40–81 eight-hour agent-days,320–648hours,
roughly3–8weeks at an assumed16–20task-hours/day including waits. Always-on
90–150MiB remains an estimate; update after service views. On-demand QML windows
remain included. [Forecast](research/feature9-gate-d.md#gate-e--core-switch-active-2026-10-16).

Keep6b niri pacing/strict idle invariance open. CPU small panels, GL only large
animated surfaces (~20MiB Mesa residue). Preview still pins the older accepted
runtime until updated/smoked. [Desktop plan](research/gate-e-validation-plan.md)
is HOLD, not run; further IME/reader work deferred. No live/install/config/unit/
keybinding changes, Feature11 or release. Binary/control-script name clash open.

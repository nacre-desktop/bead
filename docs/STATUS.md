# Status

## 2026-10-17 — Typed contract replay verified

Protocol pin `1e2d6755` and 19 supplementary frames pass typed bounds/roundtrip.
A bounded injected provider ACKs the exact provider ID before a held outbound
activation reply. Production advertises only `ui.dispatch`; no executor added.
206 tests, trace, fmt, clippy and CPU-only pass. One native compatibility pair
per compositor passes all 44 close/settled observations, caps/group/cleanup;
settled UI 23.161/23.808 MiB stays below 25 and inside prior control spreads.
No typing gain or real non-app execution is implied. [Scope](research/feature9-gate-d.md#typed-ui-action-contract-replay--2026-10-17).

## Ranked prefixes — 97d743a, CI 38996873307 green

Edited queries request 64 rows from the existing Nacre engine. Initial/restored
views and catalog wakes stay full; navigation/scrolling expand before unloaded
rows. Failed expansion keeps the last good viewport. No ranking/provider copy.

| Five alternating typing pairs | Hyprland | niri |
| --- | ---: | ---: |
| UI CPU median, ms/key |10.172 → 9.793|11.025 → 10.560|
| Paired UI reduction, ms/key |0.583|0.575|
| Presented p99 medians, ms |30.198 → 30.914|32.781 → 32.953|

All 4,000 exact key/result/commit/presentation proofs pass with 320 apps and the
frozen 75157 engine. Search replies contain 101 full / 64 limited rows. Tails fit
control spreads; no consistent p99 improvement. UI 8 ms/key and p99 16.7 ms remain
unaccepted. UI above 8 also excludes the two-frame candidate rule. The historical
29.61 → 32.66% typing regression remains open. Stale queries still serialize.

Five memory pairs and both native 20-cycle proofs pass. Paired warm deltas
+0.050/−0.019 MiB and settled −0.183/−0.257 fit control spreads. All 168 candidate
close/settled observations fit bar + 5 MiB; maximum settled UI is 23.251/23.988,
below 25 MiB. Caps, group and cleanup checks pass. Warm ranges vary; no peak
reduction or settled saving is claimed. 204 tests, trace proof, fmt, warnings-denied
clippy and CPU-only build pass. [Full measurements](research/feature9-gate-d.md#ranked-prefixes-for-edited-queries--2026-10-17).

Nested CPU at scale 1.5, Hyprland virtual 120 Hz / niri nested 60 Hz; feedback
refresh unknown. Shared host and light editing concurrent; no own builds, tests
or other captures overlap. These are not physical measurements.

## Published correctness and cleanup

Source/restoration c13f2c9: [CI 38992602004 green](https://github.com/nacre-desktop/bead/actions/runs/38992602004).
Calendar follows today until browsed, preserves that choice on reload and releases
transient state on retirement. Source-only wakes work; unchanged data stages no
scene. Search/Actions reconstruct their mode from the saved route without queries.
The bar no longer keeps two unused startup-state copies. 197 tests and native
restore proofs pass. [Source/QML scope](research/feature9-gate-d.md#calendar-source-and-restored-launcher-modes--2026-10-17).

Cleanup 3fb011a, CI 38989066777 green: panel modules, icons, stores, subscriptions
and rasters release after the 2.2-second close grace; unused preparation expires
after 2 seconds. Launcher scalar mirror clears too. App-only 4 MiB mmap policy
fixes older large-raster retention. No allocator/font rewrite. No settled saving.

Next: check this checkpoint's CI; validate the lazy read-only network/Bluetooth/
battery drafts. Those drafts, including selected-field Source lifetime tests,
are outside the measured binary. Full device controls still require contracts.

## Core-switch boundaries

Continue typing/contracts, then remaining bar popouts/service views. STOP for
review after those, BEFORE dashboard. QML remains daily driver. Typed action
contract is upstream green; executor and same-engine QML remain pending. No new
backend, launcher catalog, ranking or provider; network.watch is only a trigger.
Full AP/audio/Bluetooth/power/tray contracts/hosting remain unavailable.

Allowance is 0.5 MiB per explained feature, not commit. Pure optimizations preserve
warm/settled control spreads; every close fits bar + 5 and settled UI stays ≤25 MiB
on both compositors. Demand-load panels and release their data after idle.
No new ceiling requested. Forecast remains 40–81 eight-hour agent-days, 320–648
hours, roughly 3–8 weeks at an assumed 16–20 task-hours/day including waits.
Always-on 90–150 MiB is an estimate; update after service views, including on-demand
QML windows. [Forecast](research/feature9-gate-d.md#gate-e--core-switch-active-2026-10-16).

Keep 6b niri pacing/strict idle invariance open. CPU for small panels; GL only for
large animated surfaces (~20 MiB Mesa residue). Preview still pins the earlier
runtime until updated/smoked. [Desktop plan](research/gate-e-validation-plan.md)
is HOLD. No live/install/config/unit/keybinding changes, Feature 11 or release.
Further IME/reader work deferred; binary/control-script name clash remains open.

# Status

## 2026-10-17 — First dashboard pages built; memory headroom stop

Placement/frame 9ea0a70 is published and [CI 39017972201](https://github.com/nacre-desktop/bead/actions/runs/39017972201)
is green. Its code/assets equal owner-accepted d88e94e; 220 workspace tests,
trace/fmt/Clippy/CPU-only pass. Measured settled maxima were 23.096/23.648 MiB
H/N, with accepted paired increase +0.729/+0.861 MiB.

Dashboard checkpoints e944b8f/e6e41c1 are LOCAL, not on main. They adopt green
Nacre c2b26d98 and add lazy Weather, enrichment-only Media, and process/disk
System with Updates. All 230 workspace tests, isolated trace, fmt/Clippy,
CPU-only/release and Luau47 pass. Six scale-1/1.5 goldens repeat; private
headless preview passes launcher/private notices and all three pages with
state/commit/presentation feedback. Explicit fixtures are visibly labeled;
real unsupported topics stay unavailable. No new backend or player discovery.

Nested checks stopped at Niri settled 24.768 MiB, within the owner's 0.5 MiB
reserve below 25. Hyprland settled 24.423 MiB. All 106 closed observations plus
four final settled samples fit own bar + 5 MiB; all four captures pass the
existing hard guards. New headroom gate FAILS; no COMPLETE marker, alternating
pairs, typing rerun or dashboard publication. No performance/noise claim.
[Measured report](research/feature9-gate-d.md#first-dashboard-pages--headroom-stop-2026-10-17).

Final file / anonymous / shared MiB: H 16.483/7.246/0.693;
N 16.510/7.152/1.105. Last-closed smaps are dominated by executable mappings
(including private writable parts). Reported caches are 0.187/0.185 MiB;
panel workers/rasters/SHM release, permanent decoration pool is 542,592 raw bytes.
No matched old/new cost attribution: the batch stopped before paired controls.
Propose UI ceiling 26 MiB with stop at 25.5; keep close +5, anonymous-growth and
pure-optimization rules. Owner decision needed before more runtime work.

## Scope and remaining work

Stop at first pages / this decision. QML remains daily driver; no live UI,
install/config/unit/keybinding changes. All owned capture artifacts stopped;
no desktop job/slot. Frozen helper 75157 remained unchanged. Nested refresh
feedback reports zero (unknown); no physical or measured-120-Hz claim.

Main protocol pin remains 1e2; c2 adoption is tested locally, not published.
Reserved schemas are not hosting: helper.execute / dashboard-data / ["read"]
is observational; Media needs caller metadata and System is standalone.
Device implementation/hosting is post-F4. Reader trim API remains unpublished
upstream F3; do not adopt before published green. Public ui.menu.activate denied.

Typing stays unaccepted: prior compatibility UI 9.963/10.922 ms/key, presented
p99 30.886/31.562 ms H/N; targets 8 ms / 16.7 ms. New core-frame typing and
matched QML dashboard measurements are missing, not zero. Keep 29.61 → 32.66%
CPU regression, 6b pacing/strict idle, long chaos, reader/cache gaps and binary
name clash open. CPU small panels; GL large animation (~20 MiB Mesa residue).

Core forecast remains 40–81 eight-hour agent-days (320–648 task hours), practical
3–8 calendar weeks at assumed 16–20 task hours/day including waits. Fixture UI
progress does not settle service hosting, hardware, session/wallpaper or packaging
work. Complete-core idle 90–150 MiB remains an estimate. Full parity historical
alternative: 102–207 days / 7–18 weeks. IME/reader beyond current work deferred.

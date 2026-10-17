# Status

## 2026-10-17 — Today dashboard candidate under memory validation

Local draft adds lazy Today/calendar, all four core tab links and legacy tab/
drawer routing. Concrete CalendarState decoding keeps unrelated services out.
234 workspace tests, fmt/Clippy, CPU-only/release and52 Luau modules pass.
The two Today goldens are new; the prior six remain byte-identical.
Native lifecycle/memory acceptance is PENDING; do not publish this draft yet.


The 1 h 50 min investigation resolved the memory stop. Keep the 25 MiB UI
ceiling and 24.5 MiB stop line: full 20-cycle proofs settle at 23.603 MiB on
Hyprland / 24.158 MiB on niri. All 306 focused/full close and settled samples
fit own bar + 5 MiB; SwapPss is zero. No live desktop or installation changed.
[Measured report](research/feature9-gate-d.md#first-dashboard-pages--memory-resolved-2026-10-17).

Page lifetimes already release correctly. Decode four dashboard payload types
directly, keeping identical validation, instead of linking every service parser.
The binary shrinks 666,848 B; text shrinks 518,272 B. Five alternating pairs per
compositor pass never-opened, first-open, warm and closed memory gates. Median
paired post-use changes are -0.443 / -0.325 MiB; no typing gain is claimed.
Source a818efe matches validated 3666f8b for Rust/UI/scripts/proofs/lockfile.
Original prototypes and unsuccessful/interrupted trials remain preserved.

Lazy Weather, enrichment-only Media, and process/disk System with Updates use
explicit labeled fixtures; unadvertised real capabilities stay unavailable.
All 230 workspace tests, trace, fmt/Clippy, CPU-only/release and Luau (47 modules)
checks pass; six scale-1/1.5 PNG goldens are unchanged. Private preview covers
launcher/notices/pages; native proofs join exact state/commit/presentation.
Published-head CI: [Actions](https://github.com/nacre-desktop/bead/actions).

Closed state returns to 5 sources, 1 watcher, 1 subscription, 502,024 Luau bytes.
Cache bytes are 192,609 / 195,632 H/N; panel workers, rasters and SHM release.
Final file/anonymous/shared MiB: H 15.745/7.164/0.693; N 16.111/6.941/1.105.
UI + daemon/workers settle at 55.442 / 56.198 MiB, excluding notice UI,
wallpaper and compositor. Nested shared host; DRM null and refresh 0 are unknown,
not physical 120 Hz evidence. Report-mode timing history adds diagnostic bytes.

## Stop point and dependencies

Owner accepts first pages at8558bd6 and authorizes remaining lazy dashboard
pages plus typing work between. Today/calendar is the next bounded page.
STOP at completed dashboard or a needed decision. QML remains daily driver;
no live tests, config/unit/keybinding changes or deployment.
Owned captures are stopped; no desktop job/slot. Core pin c2b26d98 is published
and fully green; reserved topics are not hosting. Revised compatibility/OSD
additions remain unpublished and unadopted; wait for publication and green CI.
helper.execute dashboard-data ["read"] is observational; Media needs caller
metadata and System is standalone. No player, sampler or backend is invented.
Device hosting is post-F4; the Reader trim API remains unpublished.

Typing stays unaccepted: prior UI 9.963 / 10.922 ms/key, presented p99
30.886 / 31.562 ms H/N; targets 8 ms / 16.7 ms. Keep the 29.61 → 32.66%
regression, 6b pacing/idle invariance, long chaos, reader/cache gaps, matched
QML dashboard comparison and binary-name clash open. CPU for small panels;
GL for large animation (~20 MiB Mesa residue). Core forecast remains 40–81
eight-hour agent-days (320–648 task hours), practical 3–8 calendar weeks at
assumed 16–20 task hours/day including waits. Providers/workers, hardware,
session/wallpaper and packaging dominate uncertainty; complete-core idle
90–150 MiB is an estimate. Full-parity historical range: 102–207 days / 7–18
weeks. IME/screen-reader work beyond current coverage remains deferred.

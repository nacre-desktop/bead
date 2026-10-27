# B0147 — Wallpaper Spotlight overlay and first-open readiness

Status: implementation candidate; memory and paired appearance pass, frame-pacing acceptance fails.

Spotlight is a full-output overlay: selected wallpaper centered at 760 logical px max, smaller dimmed neighbours, title and translucent controls; the large card/background is removed. The centered slide keeps the existing 300 ms OutCubic easing; the overlay enters over a 32 px rise.

An idle worker materializes the same Luau view for current settings/output, warms seven Spotlight raster variants in the 22 MiB large-image LRU, and publishes the hydrated catalogue. A per-output `PanelFallbackFactory` supplies its real-image CPU scene while native GL prepares; GL promotion waits for that opening motion to finish.

| Check | Target | Result |
| --- | --- | --- |
| First content | First presented panel frame has real wallpaper | Fallback ready before `PANEL_OPEN`; 4K selection on first panel frame, 92.1/93.0 ms after trigger at 60/120 Hz |
| Open motion, synthetic 60 Hz | No dropped frames | **Fail:** cold/expired p99 intervals 56.79/53.09 ms; 2/4 gaps >25 ms |
| Open motion, synthetic 120 Hz | No dropped frames | **Fail:** cold/expired p99 intervals 21.59/133.75 ms; 2/3 gaps >12.5 ms |
| Spotlight slide, synthetic 120 Hz | Frame-rate motion | **Fail:** three traced frames in 400 ms; p99 interval 22.73 ms, 2 gaps >12.5 ms |
| Closed retention | ≤40 MiB above own idle | Pass in two arms: 37.05 MiB (60 Hz), 39.65 MiB (120 Hz); anon + shmem, DRM and SwapPss |
| Full Bead CI | Remote Arch equivalent | `FULL_CI_OK` on dev-2; regenerated goldens pass |

Paired nested QML/Bead captures against Nacre `origin/main` 871b34c at 2880×1800, scale 1.5 include idle, open, hover, pressed and slide: [idle](evidence/0147/idle-qml.png) / [Bead](evidence/0147/idle-bead.png); [open](evidence/0147/wallpaper-spotlight-qml.png) / [Bead](evidence/0147/wallpaper-spotlight-bead.png); [hover](evidence/0147/wallpaper-spotlight-hover-qml.png) / [Bead](evidence/0147/wallpaper-spotlight-hover-bead.png); [pressed](evidence/0147/wallpaper-spotlight-pressed-qml.png) / [Bead](evidence/0147/wallpaper-spotlight-pressed-bead.png); [slide](evidence/0147/wallpaper-spotlight-slide-qml.png) / [Bead](evidence/0147/wallpaper-spotlight-slide-bead.png). The old QML full-screen selected-wallpaper backdrop is the intentional visual difference. The Bead slide probes are not clean animation passes: one reports `extras/main.luau` above 50 ms, and the current-main 4K slide reports `widgets/registry.luau` above 50 ms.

Real 4K Enter exercised the unchanged activation path but produced `retained cache byte budget exceeded`; switch/apply code belongs to B0146/B0148 and was not changed here. No live deployment. See [FEASIBILITY](../../FEASIBILITY.md) and [STATUS](../../STATUS.md).

Rates are nested synthetic scheduling, not physical scanout. The private 4K image copies and captures were removed.

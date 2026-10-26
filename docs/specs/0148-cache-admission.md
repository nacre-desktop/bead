# B0148 — Recoverable cache admission

Status: synthetic validation complete; real-wallpaper gate blocked because the
owner's desktop wallpaper directory is empty. Cache pressure logged as a UI error.

| Resource | Bound / estimate |
| --- | ---: |
| Picker retained owner | 40 MiB (B0134/B0141 ceiling) |
| Display card at scale 1.5 (460×380 logical) | 1.50 MiB RGBA |
| 17 visible/adjacent cards | 31.5 MiB raster estimate |
| 24-entry 4K-source regression | 36 MiB raster estimate |

Same-owner LRU caches evict before replacement fails. If a snapshot still cannot
be admitted, the panel redraws on its current renderer and emits a rate-limited
`CACHE_PRESSURE` marker. GL promotion happens as soon as transport is ready.

Remote CI passed fmt, clippy, workspace tests, no-default/all-feature checks,
shell/Luau, services, wallpaper-video and cargo-deny. The 24 × 4K CPU snapshot
rendered at 4320×2700 within 40 MiB.

Nested QML/Bead captures used generated 6-image/1-video fixtures at 2880×1800,
scale 1.5. CPU and GL passed all layouts with zero `UI_ERROR`, zero `CACHE_PRESSURE`,
swap 0 MiB and no OOM; the pressure marker was not triggered.
A separate cold GL carousel capture was blank at 3 s settle and visible at 8 s;
the first-frame timing gate therefore remains open.

| State | CPU QML / Bead | GL QML / Bead |
| --- | --- | --- |
| Idle | [QML](evidence/0148/cpu/idle-qml.jpg) / [Bead](evidence/0148/cpu/idle-bead.jpg) | [QML](evidence/0148/gl/idle-qml.jpg) / [Bead](evidence/0148/gl/idle-bead.jpg) |
| Carousel | [QML](evidence/0148/cpu/wallpaper-carousel-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-carousel-bead.jpg) | [QML](evidence/0148/gl/wallpaper-carousel-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-carousel-bead.jpg) |
| Spotlight | [QML](evidence/0148/cpu/wallpaper-spotlight-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-spotlight-bead.jpg) | [QML](evidence/0148/gl/wallpaper-spotlight-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-spotlight-bead.jpg) |
| Hexagons | [QML](evidence/0148/cpu/wallpaper-hexagons-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-hexagons-bead.jpg) | [QML](evidence/0148/gl/wallpaper-hexagons-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-hexagons-bead.jpg) |
| Dynamic | [QML](evidence/0148/cpu/wallpaper-dynamic-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-dynamic-bead.jpg) | [QML](evidence/0148/gl/wallpaper-dynamic-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-dynamic-bead.jpg) |
| Dynamic spotlight | [QML](evidence/0148/cpu/wallpaper-dynamic-spotlight-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-dynamic-spotlight-bead.jpg) | [QML](evidence/0148/gl/wallpaper-dynamic-spotlight-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-dynamic-spotlight-bead.jpg) |
| Dynamic hexagons | [QML](evidence/0148/cpu/wallpaper-dynamic-hexagons-qml.jpg) / [Bead](evidence/0148/cpu/wallpaper-dynamic-hexagons-bead.jpg) | [QML](evidence/0148/gl/wallpaper-dynamic-hexagons-qml.jpg) / [Bead](evidence/0148/gl/wallpaper-dynamic-hexagons-bead.jpg) |

Generated-fixture RMSE is high in Spotlight (21.825%) and Dynamic spotlight
(38.605%), matching B0134's known geometry/resampling gaps. Hover/pressed parity
could not run because private labwc lacks output-management support. Cross-owner
idle-cache eviction is not registered. Real wallpapers and these gaps remain open.

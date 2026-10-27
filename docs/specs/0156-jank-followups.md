# B0156 — live jank follow-ups

Clarify panel-open markers, investigate the isolated live bar jank, and remove the OSD's first cross-kind shadow bake. Keep the renderer cache bounded and build it at idle priority during shell startup.

## Changes

- `PANEL_OPEN_LATENCY` reports the active view or `panel.route`/`panel.page`; route wins and unlabelled panels retain `panel`.
- CPU shadow pixmaps are cached by geometry, radius, blur, and scale. A nice-10 startup worker warms brightness and volume at 1.5x and the 2x volume fallback before Wayland output scale is reported; the exact 396×120/136 (1.5x) and 396×136 (2x) shapes total 1.693 MiB. The task lead approved a 2 MiB OSD cache budget.
- The live bar markers at 22:10 and 00:26 are adjacent to startup recovery-bundle and font-selection work, with no input marker. A private 2880×1800/1.5 nested niri bar-matrix startup reproduced the same recovery generations but emitted no `FRAME_JANK`; startup is the best-supported inference, not a proven cause. Suppress only a bar animation that begins in the first 750 ms before input, preserving its count as `FRAME_JANK_SUPPRESSED`; later or interactive animation jank still reports normally.

## Acceptance and measurements

| Measure | Before | After / target |
| --- | ---: | ---: |
| OSD shadow raster proxy at 1.5x | 105.686 ms cold | 0.012 ms warmed (≤16.7 ms) |
| Panel open marker | `name=panel` | active route/view name |
| OSD shadow cache | none across renderer instances | 2 cached pixmaps, 0.870 MiB pixels |
| Bar startup jank | live `dropped=39` and `dropped=22` | no marker in matched nested startup; guard covers pre-input startup animation |

The warmed timing is a renderer-level shadow-cache hit, not an end-to-end overlay commit. B0149's 131.6 ms brightness publish-to-commit observation included the cold raster.

Full Bead CI runs in the selected remote Arch container. Desktop-test reproduction of the startup-adjacent bar marker is pending; no claim of a bar-jank fix is made.

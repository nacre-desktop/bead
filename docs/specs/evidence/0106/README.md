# B0106 dashboard evidence

Each JPEG has QML / B0102 control / B0106 columns. Numbers in `metrics.json`
come from native RGB 2880×1800 PNGs, with SHA256 receipts; gallery images are
reduced for review. All sessions are private nested CPU output at scale 1.5,
with the same recorded wallpaper/palette, semantic pages and explicit fixtures.
Displayed host identity is synthetic. Playing position advances between shots.

| State | Today | Media | Weather | System |
| --- | --- | --- | --- | --- |
| Open | [image](dashboard-0.jpg) | [image](dashboard-1.jpg) | [image](dashboard-2.jpg) | [image](dashboard-3.jpg) |
| Hover | [image](dashboard-0-hover.jpg) | [image](dashboard-1-hover.jpg) | [image](dashboard-2-hover.jpg) | [image](dashboard-3-hover.jpg) |
| Pressed | [image](dashboard-0-pressed.jpg) | [image](dashboard-1-pressed.jpg) | [image](dashboard-2-pressed.jpg) | [image](dashboard-3-pressed.jpg) |

[Idle](idle.jpg). Hover/pressed states target the selected page's navigation
slot. They do not establish parity for every page control; the spec lists those
gaps. Whole-frame and panel-crop RMSE use raw channel units (0–255).

The original pre-B0102 build plus the same host-routing fix is the memory
control. The later `ee5bf3b` baseline supplies the visual and release-timing
before comparison. Source/binary hashes distinguish them. Clean-file PSS is
reported separately; memory gates use anonymous + SHM + DRM + SwapPss and
median paired differences, never the difference of group medians.

`diagnostic-arenas.json` describes one pre-selected close-fix diagnostic, not
the selected five-pair gate. It groups anonymous mappings heuristically; it is
not a libc arena census. Raw smaps, heaptrack records and session logs remain
private. Diagnostic instrumentation is excluded from acceptance measurements.

[Timing](timing.json) merges three matched release triplets. Cold image-distance
median meets ±20 ms, but includes one +50 ms outlier; warm/close remain late.
[Allocator counters](allocator.json) support release but are not RSS gates.
[Check receipt](checks.txt). Open resident deltas are unadjusted and the
image-only exception is not accepted; a closed recovery pass does not imply it.

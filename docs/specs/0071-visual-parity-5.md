# B0071 — Visual parity, round 5

Status: **Review**. Code checkpoint `623beb8`; continues B0067's dashboard edge,
status popouts, pointer targets and missing OSD fixture states. Launcher files,
docked device pages and the Nacre repository were left outside this unit.

## Changes

- The dashboard's joined artwork is generated for the current revealed body;
  content and attachment stay fixed. `placement::Chrome` owns that contour;
  `frame_art.rs` owns the lip chevrons. Generic `RevealArt` carries no Nacre
  geometry, palette roles or route names into the framework. One current SVG
  survives during motion; completion restores the prepared open artwork.
- Wi-Fi, Bluetooth, battery and audio join the top bar contour, using its
  `surface` role (QML `m3surface`). Audio's command mapping preserves its anchor.
  Bodies remain Luau on the kit, with a shared 15 px panel-inset token. Wi-Fi's
  form routes remain accessible; audio projects actual input and stream names.
- Pointer targets select the available Bluetooth toggle independently per arm.
  The private parent is explicitly sized and verified at 2880×1800, scale 1;
  otherwise absolute coordinates hit outside the nested window. Hover devices
  stay alive through capture. Press is sampled around 90 ms, before QML's
  long-press detail timer, rather than comparing detail navigation or idle views.
- Owner-only `bead.osd preview` supplies brightness/keyboard fixtures without
  daemon changes. QML receives private read-only light descriptors; both arms
  show 68% display / 42% keyboard levels. All light writes are refused by that
  fixture. Overlays now receive the current/live palette like panels/windows;
  OSD body/track roles match QML. Headless checks initialize their own catalog.
- Every arm requires swap below 64 MiB; failed arms/OSD/pointer fixtures fail capture.

## Measurements

Control `a4917a5`; QML/daemon `2748720`. Both arms used private nested niri,
2880×1800 at scale 1.5, CPU rendering, the same presentation/settings and the
shared capture lock. Swap was 0–13 MiB; no reset or host configuration change.

| Whole-image RMSE % | Before | After |
| --- | ---: | ---: |
| Wi-Fi | 17.825 | 10.248 |
| Bluetooth | 15.277 | 5.532 |
| Battery | 14.339 | 4.846 |
| Audio, normalized starting state | 19.316 | 7.200 |
| Brightness / keyboard OSD | not available | 4.228 / 4.215 |
| Matching hover / short press | not available | 6.181 / 6.322 |

QML audio inherits the preceding popup's width. The old battery→audio lane had
RMSE 19.182% before / 13.179% after, with a 200 px QML body. The final harness
opens/closes Network first **in both builds**, establishing its 352 px content
width. Control and candidate were both re-captured with this precondition.
This removes state-order ambiguity; it does not reproduce QML's width-history bug.

Dashboard warm 10/50/90% image progress: QML 173/231/277 ms, Bead 248/308/352 ms.
Both span 104 ms from 10–90%; Bead still starts about 75 ms later in this lane.
The `edge` arrays record downward growth, not a slide across the bar. Timing is
nested capture/dispatch timing, not physical scanout. Static dashboard RMSE is
7.776% cold / 7.793% warm; its existing content differences remain.

[Evidence](evidence/0071/metrics.json) includes numeric diffs and paired popouts,
OSDs, pointer states and growing-corner crops. Full frames, sequences and gallery
remain private at `parity-gallery/b0071/`; committed motion images show only the
corners, excluding personal dashboard content. Nearest-height key frames are
not identical-time pairs; their actual sampled edges/times are in the JSON.

## Validation and remaining gaps

Full Arch-container CI passed with Rust 1.99.1: fmt, clippy `-D warnings`, all
workspace tests, trace memory-policy tests, renderer feature configurations,
facade/headless checks, Luau schema/analyzer, spike/packaging suites, wallpaper
smoke, private services/tray and cargo-deny for root and independent framework
graphs. Goldens were regenerated and inspected. The new reveal regression
covers the prepared renderer, rounded moving corners, fixed attachment, bounded
SVG ownership and full-open restoration. No launcher files changed.
Arch test additions: `swaybg` (the smoke's fallback background) and `wlr-randr`
(private pointer geometry); no host packages were installed.

QML's chrome draws a contour and 1 px rim, without an independent drop shadow;
that contour/rim now follows the growing edge. Remaining visual/capability gaps:
Wi-Fi empty-state wording/density and disabled Scan; read-only audio controls;
unwired battery profile selection; OSD font weight/vertical metrics and flat
progress fill instead of QML's gradient. Dashboard dispatch offset remains.

No new idle service, timer or cache was added. Token/catalog/motion metadata is
small; generated artwork, modules and extra model rows belong to transient
views and retire after closed+idle. Relative PSS+DRM was **not remeasured** here;
no new memory-acceptance claim is made. Temporary duplicates: none introduced.

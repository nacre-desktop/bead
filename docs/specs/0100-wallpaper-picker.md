# B0100 — Wallpaper picker

Status: review candidate; implementation and guarded private desktop parity complete,
but the final five-pair relative closed-memory gate **fails**. Product work stops at that gate.
N0070 supplies the catalogue. This is a review candidate with the remaining parity gaps listed below.

## QML reference

Read from Nacre `origin/main` at `3afcdc64ec5a2150d660bfb5aabb888d3d028ec7`: `NacreLauncherPanel.qml`, `NacreWallpaperPicker.qml`,
`NacreWallpaperHex.qml`, `NacreWallpaperBackdrop.qml`, `NacreWallpaperMotion.qml`,
`NacreWallpaperVideo.qml` and `services/NacreWallpapers.qml`.
Super+W calls `nacre launcher ">wallpaper "`; QML selects launcher mode `wallpaper`.
Bead previously treated this as menu route `style.wallpapers`, showing an inert picker row.
The visual picker is bottom attached and slides from the bottom in 300 ms OutCubic.
Carousel defaults to min(1360, viewport width − 96), at least 320 wide, and 320 high.
Spotlight and hexagons use the launcher’s full scene viewport (the picker’s standalone
implicit dimensions are viewport minus 96/100); both have a selected-image backdrop
with a #70000000 scrim. Backdrop crossfade is 300 ms InOutCubic, settled after 320 ms.

The toolbar contains Static / Dynamic, Carousel / Spotlight / Hexagons and Add.
Search is 40 high, min(640, width − 48) wide, and matches every whitespace-separated
lowercase word against name plus source path. Left/right wrap, wheel accumulates
120 units per selection, Enter applies, Escape closes, and detached Escape clears search.
Clicking a card browses and applies it after the service’s 35 ms selection debounce;
closing commits the queued selection. Current source path anchors selection across refresh.

Carousel has an odd number of slots, 16 px gaps, up to 280 px card width, radius 15,
a 2 px primary selected border and 3 px image inset/radius 12. Circular travel uses
300 ms OutCubic. Spotlight uses a widescreen hero (half available width, max 1000,
height constrained), overlapping smaller side cards and distance opacity.
Hexagons have 3–7 columns, tile width up to 340, height .87×width, .76×width pitch,
alternating half-height column offsets and a polygon mask with matching hit region.
Offscreen cards restrict decoding. Fullscreen selected previews decode up to 1600×1000;
card thumbnails use 640×420 (hexagons 480×420).

Dynamic selected cards wait 180 ms before starting AnimatedImage or QtMultimedia
video; only the selected card runs, and lock/sleep/paused suppress motion. The video
component is created lazily. Bead must retain at most one motion preview.
The source is the helper’s recursive catalogue of `cli.json` wallpaper.directory,
falling back to Pictures/Wallpapers (including Dynamic). The helper owns image/video
classification, posters, bounded thumbnails, import/pick, and wallpaper-picker preferences.

## Implementation and verification

UI/layout/filter/navigation belong to Luau on the kit. Nacre-specific contract calls
belong to the shell edge adapter. Catalogue leases and images retire with the panel.
Existing revision-only wallpaper.activate remains compatible; N0070 adds selection by id.
No replacement catalogue, ranking engine, or decoder is implemented in Bead.

The image loader consumes and premultiplies decoded RGBA bytes in place. Cover images
sample the centered source directly with bilinear filtering, avoiding resize temporaries.
Fill retains its Lanczos behavior, checked by transparent-image pixel-equivalence tests.
Oversized partial repairs paint clipped borrowed SHM; rounded radii are clamped and Arc
primitives retain their clip behavior. Scratch pixels and masks each have a 16 MiB limit.

The implementation is `shell/extras/wallpaper/main.luau`, with selection/filtering in
`extras/behavior.luau`; the extras factory acquires the catalogue/settings only on opening.
Super+W, `nacre mode wallpaper`, and the launcher's wallpaper route row select this surface.
Static and video entries activate through `wallpaper.activate {revision,id}`. Pointer clicks,
Left/Right, navigation buttons, Enter and wheel navigation share stable-ID Luau selection.
Only the latest selection waits behind an admitted call; revision-only background reads remain compatible.

Generic additions: normalized `clip_polygon` (3–32 vertices, even-odd image mask and matching
hit region), `image_fit="cover"`, and signed edge insets for clipped offscreen content.
The shell edge first requests metadata (`ids:[]`), then hydrates only IDs used by the
current Luau scene (at most 128 per call). Logical image handles carry a resource
generation so newly hydrated bytes trigger a fresh scene. Animated images join videos
in Dynamic. Missing/evicted advertised paths refresh on demand; a failed or still-missing
resource is attempted once per visible set, then retries after that set changes or reopening.
Encoded cache and scene attachments each have a 16 MiB byte budget, individual files
are at most 2 MiB and dimensions at most 1600×1000.
Closing cancels hydration while retaining the populated slide-out frame. Encoded bytes,
rows, subscriptions and rasters retire with the existing panel worker after 2.2 seconds idle.
Activation admission lives in the panel factory outside the view: one in-flight call plus
one latest pending stable ID survives retirement without retaining image or view data.
Panel retirement also restores the correct output’s primary accessibility tree; the
closed companion’s labels and focus no longer wait for a later bar redraw to disappear.
No product dependencies or duplicate implementations were added. The Arch CI container needs
`swaybg` for the video smoke fallback, and removes Sway’s packaged file capability inside
the container because Docker does not grant that capability.

Workspace/all-target clippy with warnings denied and the extras suite pass. Checks cover
real keyboard/pointer input, revision-fenced calls, stable filtered selection and cache
bounds/close release. Twelve picker goldens cover three layouts, two media kinds and
two scales (1×/1.5×); regenerated outputs were visually reviewed. The protocol
pin is Nacre `34e81afd` (N0070 bounded hydration); both wallpaper transcripts are mirrored.
Full remote Arch CI passes at `98d3de7` (serial test execution, unchanged deadlines),
including 580 shell tests, 44 renderer tests and video renderer smoke (12/12 checks),
private service and tray lifecycles, Luau/generated schemas and independent license graphs.

Guarded private desktop parity used Bead control `e2f17a4` and Bead candidate `98d3de7`
on the same 2880×1800 output at scale 1.5, with ten stills and two short videos.
These are pinned pre-B0107 sources; the integrated revision also includes the independent
slider follow-up from `0ce6f33`, with a separate full CI run. All 13
states passed; paired captures are in `evidence/0100/`. RMSE is percent of full-scale RGB,
computed from raw images. Bead uses CPU rendering; QML uses its native Qt Quick
default (graphics API not logged). The memory gate compares CPU Bead builds only.
Full-frame / picker-surface RMSE:

| State | Full frame | Picker surface |
| --- | ---: | ---: |
| Carousel | 2.891% | 6.055% |
| Search | 2.437% | 4.863% |
| Spotlight | 4.869% | 4.879% |
| Hexagons | 3.566% | 3.479% |
| Dynamic spotlight | 8.559% | 8.745% |
| Video activated | 14.885% | 7.269% |

Carousel open t10–t90 image-progress proxies were QML 186 ms / Bead 290 ms; close was
116 ms / 166 ms. Grim timestamps record capture-process spawn time relative to the trigger;
these image-sequence proxies include dispatch and capture scheduling uncertainty and do not
measure physical presentation. The first fullscreen static spotlight and hexagon image-progress thresholds
appeared about 321 ms and 292 ms after trigger respectively, despite the configured 300 ms
OutCubic panel motion. This is a visible hydration/first-content delay, not established
animation timing; it remains open.

The QML picker calls `query.forceActiveFocus`, but `NacreLauncherPanel.focusPage()` then
focuses the page root. In the measured nested session, both QML and Bead required clicking
the search field before typing; typed parity states click it in both arms.

Earlier normal CPU cohorts failed the relative closed-memory gate. Each row uses its
own matched control; the values below are candidate-minus-control paired medians in MiB.

| Candidate / control | Completed pairs | Carousel | Spotlight | Hexagons |
| --- | ---: | ---: | ---: | ---: |
| `71e4532` / `b1826ae` | 5 | +0.398 | +1.871 | +2.023 |
| `5ad0972` / `b1826ae` | 4 of 5 | +0.414 | +0.318 | +3.598 |
| `f22479a` / `e2f17a4` | 3 of 5 | +0.641 | +0.594 | +0.875 |

The two incomplete cohorts stopped after completed, guarded pairs when the remaining
samples could not restore a passing five-pair median. Their complete samples remain in
`memory-before.json`, `memory-reuse.json` and `memory-cover-thp.json` in the evidence.
Weak-raster checks show all observed picker image bytes release. Allocator snapshots
and a separate heaptrack trace show nearly flat live heap with larger anonymous residency
changes; these diagnostics do not uniquely identify the pages behind the residual.
Smaller mmap thresholds, private mutable font caches and a two-arena cap gave insufficient
benefit or increased memory. None is shipped; their diagnostic records and the rejected
font patch are retained. The existing 4 MiB mmap threshold remains unchanged.

Linux GNU startup requests process-only THP disable-except-advised when no parent disable
policy is inherited. Explicit huge-page advice remains available; unsupported kernels
continue normally. A diagnostic queried actual mode 3 and zero anonymous huge pages
after each layout. No host THP setting changed.

Five final normal alternating control/candidate pairs at `98d3de7` / `e2f17a4`
completed with all guards passing. They measure the UI process
at idle and about 9.5 seconds after each layout closes, with the initial static wallpaper
unchanged. The gated sum is Pss_Anon + Pss_Shmem + DRM + SwapPss; Total Pss and Pss_File are informational.
Daemon/provider/video-engine memory is separate. Every arm requires matching geometry,
contained scopes, swap used below 64 MiB, zero process SwapPss and zero host swap-in.

| Final CPU phase | Control / candidate unreclaimable MiB | Paired median Δ MiB | Difference of medians MiB | Gate |
| --- | ---: | ---: | ---: | --- |
| Idle | 28.199 / 28.309 | +0.102 | +0.109 | pass |
| Carousel closed | 28.652 / 29.250 | +0.617 | +0.598 | **fail** |
| Spotlight closed | 28.867 / 29.422 | +0.586 | +0.555 | **fail** |
| Hexagons closed | 29.027 / 29.672 | +0.633 | +0.645 | **fail** |

The relative limit is +0.5 MiB per phase. The gated excess is entirely anonymous;
shmem/DRM/SwapPss deltas are zero. No final paired anonymous delta exceeds 1 MiB.
`memory.json` includes all ten guarded arms, both median definitions, ranges and
informational total/file PSS. The laptop absolute cap is not evaluated here.
Repeated closed medians fail under the amended five-pair noise rule. Further product
optimization stops; no memory acceptance is claimed. The candidate is pushed for lead
review as instructed, retaining the rejected experiments and unresolved residency attribution.

Three populated-Apps first-key fixture pairs passed guarded sampling (320 apps,
12 favorites, 8 recents, 64 prefix matches). UI process CPU through the matched panel
presentation had first-key medians control 6.865 ms / candidate 6.719 ms; Backspace
7.567 / 7.456 ms. Candidate ranges were 6.669–6.780 / 7.070–7.600 ms, below the 8 ms
UI target. These exclude Home, standalone and daemon-engine CPU; presentation clocks
are nested feedback, not physical scanout. `first-key.json` preserves every sample.
A separate long-dwell trace confirms primary accessibility restoration (19 nodes),
zero retained bytes from six observed picker rasters and actual process THP mode 3;
see `a11y-retirement-proof.json`.

Product-source delta against control `e2f17a4` is 1,716 added / 150 removed lines, excluding
tests, generated schemas, documentation and the capture harness. N0070 adds 766 / removes 62
Rust product lines. No duplicated legacy implementation remains.

## Known gaps

- Cold fullscreen content appears about 0.3 seconds after opening; hydration trails the configured panel slide.
- Font rasterization and high-contrast image resampling differ between Qt and the CPU renderer.
- Reveal/close image-progress proxies remain slower than QML despite matching the configured 300 ms OutCubic motion.
- Search uses Luau byte-string lowercase; non-ASCII case variants do not match Qt’s Unicode lowercase behavior.

- Add is visibly disabled: the catalogue/selection contract does not offer file import.
- Candidate video/animated-image previews remain posters. The existing video renderer exposes
  output wallpaper surfaces, with no in-panel frame API; adding decoding is B0099's scope.
- Panel content slides from the bottom for 300 ms with OutCubic easing. Card travel, selected-backdrop crossfade
  and spotlight side-card opacity do not yet match QML. Hex captions lack QML's text outline.
- Gallery wheel navigation still does not reproduce QML's accumulated 120-unit threshold;
  the normalized event bridge does not preserve the original angle/pixel source semantics.
- Catalogue file changes refresh on reopen or reconnect.
- The video-activation pair shows differing palette refresh in bar controls; this is
  included in full-frame RMSE, alongside unsynchronized video phases.

Renderer limits outside the picker: `image_fit="cover"` currently applies to raster
images; SVG retains its existing stretch path. The existing Arc repair path still
rejects scratch allocations above 16 MiB (current dashboard arcs are small).

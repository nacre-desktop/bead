# B0106 — Dashboard follow-up

Review candidate: closed recovery passes five pairs; the cold image-distance
median meets ±20 ms. One cold outlier, warm/close motion and the image-only
open-memory allowance remain unaccepted.
No frame-pacing policy or installed desktop change is included.

## Change and source boundaries

Today's wallpaper decode/PNG encoding blocked model projection. One bounded
background job now prepares the poster while the first projection returns the
layout. The existing descriptor identity checks remain; superseded wallpaper
or size results are discarded. Unavailable sources keep unfinished jobs off the
projection path. Retirement joins the job and releases its result. A single
process-local trim follows job completion; no global allocator setting changes.
Effective output scale sizes the poster: 1410×384 at 1.5 instead of 1880×512.

Luau owns the page geometry and behavior. Media separates player/artist/album
and matches the details card; its full-panel blur is removed. Weather restores
six readings, units, weekday/range rows and the full 24-hour line/area series.
System uses four published gauges above network/process cards. Unknown readings
stay unknown; no provider, ranking or unpublished action is invented.
Generic primitive: `ui.Polyline`, at most 256 normalized finite points with
optional closed fill, implemented for CPU/GL and damage/accounting paths.

The memory control is `44a2004` plus the same `18dc581` host-routing fix.
Visual/timing before is `ee5bf3b`, with the same opt-in image-byte trace probe.
Selected runtime `0387de0` is based on integrated main `14a19be`; debug shell
SHA256 starts `261fbc03`, release starts `2f7a8b9e` (full hashes in evidence).
QML shell/shell-tools content matches Nacre origin/main `ab04e938`; only a
dangling test-gallery symlink is outside that content comparison.

## Cold preparation and visible motion

Matched release CPU builds, three alternating QML/before/selected triplets;
“cold” means first dashboard open in a fresh shell, with host page cache kept.
The 1.4-second observation was from debug/capture-window work and is not used
as a release baseline. Cumulative medians below start at panel admission.

| Bead milestone, ms | Before | Selected |
| --- | ---: | ---: |
| Worker spawn | 3.8 | 3.9 |
| VM ready | 9.5 | 9.7 |
| Generation ready | 17.7 | 16.7 |
| First worker layout | 211.4 | 24.0 |
| First worker paint | 269.5 | 56.5 |
| First content commit | 271.0 | 57.2 |
| Following panel buffer commit | 318.2 | 77.2 |
| First matching content presentation | 335.1 | 101.8 |

The asynchronous poster starts at 19.0 ms and is encoded at 164.6 ms, after the
first content commit; decode no longer blocks that preparation. QML and Bead
both specify 360 ms InOutCubic for opening and closing. No pacing policy changes.

| Image-distance t10 median, ms | QML | Before | Selected |
| --- | ---: | ---: | ---: |
| First open | 119.8 | 154.4 | 113.6 |
| Warm open | 110.3 | 164.1 | 147.5 |
| First close | 643.5 | 676.4 | 668.5 |
| Warm close | 644.7 | 676.1 | 667.9 |

Cold paired selected−QML differences are +50.2 / −19.5 / −11.1 ms; their
median −11.1 ms meets the ±20 target, but one of three runs does not. Warm
open's paired median is +37.2 ms; first/warm close are +26.2/+24.5 ms.
This is not consistent motion parity. The old fallback could reveal before
prepared content arrived; faster preparation does not establish every reveal.
The 25 ms camera uses 250 ms pre-roll and interpolated grayscale image-distance
progress, not geometric height or physical scanout. Asynchronous image arrival
changes that distance curve; close also includes the shared pointer/dispatch
sequence delay. `timing.json` retains all runs, clocks and source hashes.

## Closed recovery and open limits

CPU, same-host/flags/output, 2880×1800 at scale 1.5. Five alternating original /
selected pairs numbered 2–6 use the identical selected binary/UI hashes.
Pair 1 used a pre-integration binary and is excluded. Close waits six seconds
for retirement/idle reclaim, then takes five samples. Unreclaimable is
Pss_Anon + Pss_Shmem + DRM + SwapPss; clean-file PSS is informational.
The paired median need not equal the difference between the group medians.

| Closed MiB | Original median | Selected median | Median paired Δ |
| --- | ---: | ---: | ---: |
| Idle | 31.293 | 31.254 | −0.043 |
| Today | 31.695 | 31.805 | +0.160 |
| Media | 32.113 | 32.070 | −0.039 |
| Weather | 32.457 | 32.445 | +0.059 |
| System | 32.582 | 32.488 | −0.125 |

All closed deltas pass +0.5 MiB. Closed SHM remains 10.582 MiB; DRM and
SwapPss are zero. The paired closed differences are anonymous residency, with
no repeated unexplained >1 MiB candidate growth. File-backed components are
reported in `metrics.json`; no absolute laptop-cap claim is made.

| Open MiB, unadjusted | Original median | Selected median | Median paired Δ |
| --- | ---: | ---: | ---: |
| Today | 49.574 | 57.535 | +7.949 |
| Media | 50.066 | 56.449 | +6.375 |
| Weather | 53.320 | 55.789 | +2.844 |
| System | 51.676 | 56.680 | +5.004 |

These open increases do not pass the unadjusted half-MiB comparison. Image/cache
counters explain active storage but do not isolate every extra resident byte;
**the image-only open allowance is not accepted**. Today's requested RGBA is
2,165,760 bytes and its encoded poster is 1,270,674 bytes in the selected arm.
Prepared-image totals also include the joined frame, shade and artwork, so they
must not be presented as a before/after image-only delta.

Retirement's weak-raster probes report zero retained image bytes on every
selected close. Selected cache counters return from open 7.76–12.78 MB to
0.347–0.350 MB; median live heap returns within about 0.2 MB of its 16.25 MB
idle value (allocator counters, not resident memory). `allocator.json` retains
per-phase live/mapped/free/arena values; older missing counters stay null.

## Attribution and rejected experiments

A pre-selected close-fix diagnostic's heaptrack prefixes return below their own
idle outstanding allocations: −60.06K / −72.61K / −18.03K / −62.44K for
Today/Media/Weather/System, in heaptrack's displayed units. Instrumented runs
are excluded from gates. Smaps shows increasing resident anonymous pages in
aligned secondary-arena candidates, including huge-page backing; this is a
mapping heuristic, not a libc arena census or proof of a particular tcache.
The evidence supports freed allocator residency as a contributor.

Global mmap thresholds, arena limits and producer-side scene retirement were
rejected after inconsistent recovery/CPU results. None ships. The selected
Weather default merges two vectors into one 846×447 raster: 3,025,296 →
1,512,648 bytes; photo mode still uses two. Source vectors shrink 47,308 →
37,215 bytes. Composite-only pixel difference is 0.269 RGB RMSE, max channel 2.

An early Media close cleared artwork before the closing scene attached assets,
invalidating a large blur; a private debugger found the UI joining the worker
still in `image::imageops::blur`. Artwork now keeps its identity through the
close grace and releases at retirement. That failed arm is excluded. Rebase
review also fixed Polyline's borrowed-SHM strip and the unfinished-job join on
wallpaper source loss. Focused lifetime/parser/repair regressions pass.

## Native visual comparison and gaps

All 13 selected states pass capture gates: idle and open/hover/pressed for four
pages. Fixtures replace displayed host identity and fix Today's clock; media
position advances. RMSE uses native RGB PNGs, not resized gallery JPEGs.
Earlier reports mislabeled these raw channel values as percentages.

| Whole-output RGB RMSE, 0–255 | Before | Selected |
| --- | ---: | ---: |
| Today | 15.143 | 15.152 |
| Media | 15.513 | 13.927 |
| Weather | 17.318 | 14.974 |
| System | 19.822 | 16.183 |

Today is essentially unchanged. [Gallery and native hashes](evidence/0106/README.md)
also include panel-crop metrics. All 13 triptychs and the four page goldens were
inspected. Remaining unaccepted differences:

- Navigation: QML's selected pill and hover/pressed fill differ from the pearl.
- Today: wallpaper/motion/home controls, palette caption, cached-weather detail,
  crop/gradient and some typography differ.
- Media: rounded cover/crop, backdrop blur, seek drag, shuffle/repeat, timed
  lyrics and player raising remain incomplete.
- Weather: moving sun marker, tooltips, curve interpolation and gradient/text
  details differ. A legacy daemon translation still says “Next hours” instead
  of QML's “Next 24 hours”; the complete published series is retained.
- System: disk selector, battery-runtime details, CPU temperature/sorting,
  update/check/maintenance controls and some line/text details remain absent.

## Validation

Full remote Arch CI-equivalent: `nice -n 10 bash /w/check-integrated.sh`, exit 0,
`CI_ALL_OK`, on the selected integrated source. It mirrors `.github/workflows/ci.yml`
(the task's older `check.yml` path does not exist): fmt, warning-denied workspace
Clippy, workspace/trace tests, CPU/all-features, facade, generated Luau/schema
(143 modules), spike/packaging suites, private service/tray lifecycle, wallpaper
smoke and root/independent framework cargo-deny. Focused dashboard generation:
64 passed; Polyline renderer/parser regressions each pass. No dependency added.
Private arms use per-command blocking flock, swap preflight, containment,
watchdog, semantic page, fixture/source, geometry and full-window gates. No
live session, installation, cache drop or host tuning. No product duplicate.

# B0100 private desktop evidence

QML is left and Bead is right. Native captures are 2880×1800 at scale 1.5,
with ten generated images and two short fixture videos. Bead uses the CPU release
build. QML uses its native Qt Quick default; its graphics API was not logged.
No live desktop configuration or installed shell was changed.

`metrics.json` records source revisions, full-window and surface-crop RMSE
(percent of the RGB range), memory breakdowns and reveal/close image proxies.
The original PNGs were compared before JPEG pair export. `*-panel-pair.png`
retains lossless carousel crops. `*-open-*.jpg` and `*-close-*.jpg` contain twelve
frames sampled across each sequence. Capture timestamps refer to grim process
spawn relative to the trigger; they are not physical presentation timings.

All thirteen states passed geometry, interaction and activation assertions.
Both arms used private HOMEs, service buses and nested compositors; each arm
used the shared desktop lock and a guarded scope with memory/swap containment.
Host swap-in and process SwapPss were zero.

The full image metric includes the bar clock and active wallpaper. Video phases
are not synchronized, and Bead's selected preview is a poster; their error is
reported rather than hidden. A 300 ms OutCubic panel transition is configured,
but cold fullscreen image hydration trails it (about 0.3 seconds after trigger).
See the [specification](../../0100-wallpaper-picker.md) for remaining visual gaps
and the repeated relative closed-memory comparison.

`memory-before.json` preserves the five-pair failure before the RGBA-buffer change.
`allocator-diagnostics.json` contains separate single-arm diagnostic runs, including
weak-raster release checks and allocator totals immediately after retirement.
Those diagnostics used a private allocator-reporting preload and state tracing;
they are not the normal acceptance cohort. Smaller mmap thresholds gave no clear
benefit. The 1 MiB experiment also recorded 8 MiB of anonymous huge pages after
hexagons, with nearly unchanged live heap; this does not identify the pages behind
earlier outliers. The mmap threshold remains 4 MiB. The final shell requests process-only THP
disable-except-advised, retaining explicit huge-page advice; no host setting changed. Linux documents
[per-process transparent huge-page behavior](https://docs.kernel.org/admin-guide/mm/transhuge.html#process-thp-controls).

`memory-reuse.json` preserves four completed pairs after RGBA reuse (still failed).
`release-build.txt` records matched release sources and binary hashes.

Native state memory points describe Bead only. The relative gate compares matched
CPU Bead builds in `memory.json`; QML is the appearance reference, with no QML
memory acceptance comparison. Daemon/provider/video-engine memory stays separate.

The final five-pair CPU cohort fails the +0.5 MiB relative closed-memory gate:
carousel +0.617, spotlight +0.586 and hexagons +0.633 MiB paired medians. Idle
passes at +0.102 MiB. The excess is anonymous; all ten arms pass capture guards.
`first-key.json` records three populated-Apps typing pairs below 8 ms UI CPU/key.
`a11y-retirement-proof.json` separately confirms raster release and primary-tree restoration.

# B0146 — Live wallpaper switch latency

The live shell takes 0.4–1.6 s to switch between static wallpapers.
Nested measurements were 125–150 ms and used small fixtures, so they did not cover the owner's 4K JPEG workload.
Phase markers now report the selection-relative path through daemon calls, decode, upload, commit and presentation.
Release decoder and cache microbenchmarks use the 17-file private set at 2880×1800 output resolution and scale 1.5.

| Measure | Baseline | Gate |
| --- | ---: | ---: |
| Live static switch | p95 not yet established; observed range 412–1558 ms | selection to visible ≤150 ms |
| Full-resolution presentation | not yet split by phase | ≤400 ms |
| Owner-reported source set | 17 JPEGs, about 72 MB total | same files on nested desktop |
| Measured desktop copy | 17 files: 9 PNG, 8 JPEG, 71.6 MiB | use all copied inputs |
| Display cache | implemented: raw RGBA + zstd, 2880×1800 | 144 MiB LRU cap; keyed by path, mtime, size and output dimensions |

## Instrumentation

Unconditional `WALLPAPER_SWITCH_PHASE` journal entries include revision, phase, monotonic milliseconds since selection, encoded file bytes and source dimensions. Paths and filenames are omitted. The current phase set is `selection`, `request_sent`, `daemon_ack`, `decode_start`, `decode_end`, `scale_end`, `upload_end`, `commit` and `presented`. The daemon call around decode and the picker's activation call are both visible as request/ack pairs. Existing `WALLPAPER_SWITCH_LATENCY` remains the end-to-presentation summary.

The JPEG decoder scales while decoding, so current parent-side `decode_end` and `scale_end` entries share the same timestamp; a distinct decode/scale split would require worker timing output and is not claimed here.

## Optimization plan

The shell caches private, output-sized raw RGBA rasters compressed with zstd level 1. Entries are atomically written by a nice-10 worker, validated against path, source identity/mtime/size and output dimensions, and evicted least-recently-used above 144 MiB. Cache misses return decoded pixels immediately and regenerate in the background. Current and recently used wallpapers stay warm by access; the existing picker predecoder keeps the selected/hovered neighbour in memory and writes its disk entry. The 144 MiB budget holds seven uncompressed 2880×1800 rasters, and more when compression permits.

## Evidence and status

- Initial live static-to-static samples: 412, 441, 527, 822, 905, 1034, 1074, 1209, 1266, 1316, 1558 ms; video-to-static: 509 ms.
- Real wallpaper files were copied to the task-private desktop test directory. No live desktop shell, installed files or configuration were changed.
- Full remote Bead CI passes with phase telemetry and the neutral upload callback. A 34-run release decoder-only cohort on the copied files at 2880×1800 scale 1.5 measures wall p50/p95/p99 60.0/379.5/379.8 ms and CPU p95 224.7 ms; it excludes daemon, buffer upload, and compositor presentation.
- On dev-2, the optimized raw RGBA+zstd build measured 16.30 ms p50 / 22.57 ms p95 for cache load plus BGRA SHM staging, and 53.34 ms p50 / 319.29 ms p95 for decode misses across all 17 real files. The p95 hit is 2.57 ms over the ≤20 ms target; the lead's live run will measure the actual SHM attach boundary. The QOI prototype was replaced after a slower hit cohort.
- Cache unit tests cover identity/dimension invalidation, size-bound LRU eviction, and corrupt-entry fallback. Live selection-to-visible timing, SHM upload/presentation, and current-format cache timing remain for the lead's live measurement.

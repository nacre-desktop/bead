# B0146 — Live wallpaper switch latency

The live shell takes 0.4–1.6 s to switch between static wallpapers.
Nested measurements were 125–150 ms and used small fixtures, so they did not cover the owner's 4K JPEG workload.
Phase markers now report the selection-relative path through daemon calls, decode, upload, commit and presentation.
The next measurement uses the 17-file private desktop set at 2880×1800 output resolution and scale 1.5.

| Measure | Baseline | Gate |
| --- | ---: | ---: |
| Live static switch | p95 not yet established; observed range 412–1558 ms | selection to visible ≤150 ms |
| Full-resolution presentation | not yet split by phase | ≤400 ms |
| Owner-reported source set | 17 JPEGs, about 72 MB total | same files on nested desktop |
| Measured desktop copy | 17 files: 9 PNG, 8 JPEG, 71.6 MiB | use all copied inputs |
| Persistent decoded cache | not implemented | ≤384 MiB, identity/size/mtime invalidation |

## Instrumentation

Unconditional `WALLPAPER_SWITCH_PHASE` journal entries include revision, phase, monotonic milliseconds since selection, encoded file bytes and source dimensions. Paths and filenames are omitted. The current phase set is `selection`, `request_sent`, `daemon_ack`, `decode_start`, `decode_end`, `scale_end`, `upload_end`, `commit` and `presented`. The daemon call around decode and the picker's activation call are both visible as request/ack pairs. Existing `WALLPAPER_SWITCH_LATENCY` remains the end-to-presentation summary.

## Optimization plan

Use phase data from the real files to choose the dominant cost. If decode/scale dominates, reuse a bounded per-source-identity and output-size decoded cache, build entries off the interaction path, prefetch the next visible picker item, and show the already-loaded thumbnail until full resolution is ready. Cache files are private, output-size-specific and invalidated by source identity, size and mtime; cap disk use at 384 MiB with least-recently-used eviction. Keep the first frame visible and blend to the full image. If daemon work or process startup dominates, fix that path instead.

## Evidence and status

- Initial live static-to-static samples: 412, 441, 527, 822, 905, 1034, 1074, 1209, 1266, 1316, 1558 ms; video-to-static: 509 ms.
- Real wallpaper files were copied to the task-private desktop test directory. No live desktop shell, installed files or configuration were changed.
- Full remote Bead CI passes with phase telemetry and the neutral upload callback. A 34-run release decoder-only cohort on the copied files at 2880×1800 scale 1.5 measures wall p50/p95/p99 60.0/379.5/379.8 ms and CPU p95 224.7 ms; it excludes daemon, buffer upload, and compositor presentation.
- The private catalogue returns 17 entries, but `wallpaper.activate` fails during palette publication and the active revision does not change; QML's UI-provider check also times out. End-to-end selection/presentation and the p95/full-resolution gates remain unmeasured. No persistent cache was implemented.

# Cache owner snapshots

A private diagnostic harness copy queried `bead.panel state` after the 80-row finger pass, immediately after panel close, and after closed+12s. It does not affect input timing. The run was one new panel lifetime using default FemtovgGL, with source and swap checks; the scroll arm rc0 and no `UI_ERROR` messages were recorded.

| Phase | Panel open | Global used / limit | `native-gl-images` used / limit | `native-gl-composite` used / limit | `retained-cpu-rasters` used / limit |
|---|---|---:|---:|---:|---:|
| After finger gesture | yes |149,159,382 /201,326,592B |70,874,698 /71,303,168B |4,921,408 /67,108,864B |70,878,328 /142,606,336B |
| Immediately after close | no |149,159,382 /201,326,592B |70,874,698 /71,303,168B |4,921,408 /67,108,864B |70,878,328 /142,606,336B |
| Closed+12s | no |165,424,674 /201,326,592B |70,874,698 /71,303,168B |21,281,968 /67,108,864B |70,878,328 /142,606,336B |

The first two samples put the GL-image owner about428,470B below its68MiB limit while the global report is below192MiB. The later combined finger→close→wheel-open run failed with180 generic `retained cache byte budget exceeded` UI_ERRORs; this snapshot alone does not identify the rejected owner or attempted request size. The separate wheel-only fresh-session run passed UI safety but had only3 frames and a654ms reversal. All raw owner entries and monotonic timestamps are retained in [cache-probe.jsonl](cache-probe.jsonl); values are byte counts from the runtime report, not estimates.

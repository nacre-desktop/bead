# Bounded active finger cadence

Matched Bead CPU control e2d/f404 and candidate65aa/7f050 Settings-content run; one repeat each, finger stream160 events at requested8ms, direction alternated every3 samples. This bounded oscillation remained inside the short77px viewport extent and avoided the long edge-idle gaps that affect midpoint sweeps. Source pin passed; swap0 KiB; no UI errors. The control binary was later flagged for shared-target provenance ambiguity, so this pair is diagnostic and not a final before/after performance gate. The nested Niri mode report is60Hz and `refresh_ns=0`; feedback may still arrive faster, but this capture does not prove physical120Hz scanout.

| Arm | Input p95 | Presented frames | Render p95 | Presented p95 all-gap | Missed-slot upper bound | Discards | Reversal input→visible / delivery→visible |
|---|---:|---:|---:|---:|---:|---:|---:|
| Control |8.01ms|58|15.788ms|24.53ms|107|0|45.13 /44.86ms|
| Candidate |8.01ms|111|6.778ms|16.22ms|52|1|29.25 /20.93ms|

The actual `settings.scroll` state varied throughout the gesture (control0–63, candidate0–21; four offsets each), confirming continuous bounded movement rather than a clamped idle probe. Candidate render p95 meets8ms, but still exceeds the one8.33ms frame reversal target, and one presentation feedback was discarded. Long-gap/drop values retain every presentation interval and remain upper bounds. Per-state samples and raw windows are in [state JSON](scroll-state.json) and [pacing JSON](candidate-pacing.json).

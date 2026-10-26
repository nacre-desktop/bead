# Widgets gallery matched scroll capture

CPU Bead, control `e2d95769…` / binary `9bf08097…` versus unit candidate f327 snapshot / binary `d0376ab3…`; three repeats each, 160 finger events at requested 8ms, source pin verified, zero swap use and no UI errors. The final unit source later received a GL-only cache-eviction alignment; this capture measures the CPU widgets route and predates that packaging commit.

| Arm | Input→visible reversal | Delivery→visible | Render p95 | Presented p95 (all gaps) | Max presented interval | Dropped-slot upper bound | Discarded feedback |
|---|---:|---:|---:|---:|---:|---:|---:|
| Control | 43.34–74.78ms | 42.28–74.12ms | 17.54–17.83ms | 28.46–103.75ms | 128–155ms | 84–90/repeat | 0 |
| Candidate | 32.26–32.55ms | 23.78–23.98ms | 9.09–9.33ms | 16.04–16.15ms | 144–168ms | 58–72/repeat | 0–1 |

Candidate improves render cost and reversal time but still exceeds the ≤8ms render and ≤8.33ms one-frame reversal targets. The presented timestamps report inner-surface presentation; `refresh_ns=0` and the nested Niri advertises a60Hz mode, so this does not prove physical120Hz scanout or a hard60Hz callback cap. No QML widgets app counterpart was available; this is Bead-only evidence. Widget scroll state changed in both directions in all repeats. The app surface was 926×1108 logical (1389×1662 physical at 1.5); the right-side gradient is desktop wallpaper outside the app surface. See paired [control before](paired-ba0/control-before.png), [control after](paired-ba0/control-after.png), [candidate before](paired-ba0/candidate-before.png), and [candidate after](paired-ba0/candidate-after.png).

## Wheel follow-up (65aa source)

The 65aa CPU control/candidate follow-up ran three repeats with 12 wheel notches at120ms, the same `widgets.open` route and x800,y600 interior point. The control binary was later flagged for shared-target provenance ambiguity, so these control comparisons are diagnostic, not final before/after gates. The candidate had source/swap checks and no UI errors or discarded feedback.

| Arm | Input→visible reversal | Delivery→visible | Render p95 | All-gap presentation p95 | Max gap / missed upper bound |
|---|---:|---:|---:|---:|---:|
| Control | 44.32–46.53ms | 27.77–29.98ms | 17.273–17.536ms | 663.62–665.71ms | 663.6–665.7ms / 89–102 |
| Candidate | 22.76–22.96ms | 14.16–14.41ms | 7.786–8.556ms | 683.62–685.34ms | 683.6–685.3ms / 81–93 |

All three candidate repeats joined an opposite delivered wheel input to a presented `widgets.scroll` offset change. Candidate render p95 straddles the8ms target (median8.447ms), and reversal remains above one8.33ms frame. The all-gap presentation intervals include edge/clamp idle; they are not a zero-drops certification. QML has no widgets app counterpart. Screenshots and per-repeat JSON are in [`65aa-wheel/`](65aa-wheel/).

# Status

## 2026-10-15 — cache accepted/CI green; five-pair follow-up complete

Main e82c3c8 includes215ea35; CI38797186208 green.
[Cache report and bounded investigation](research/cache-budget.md).
Five alternating samples per variant/compositor completed in about14 minutes.
First-open medians MiB: H27.451→27.421,N29.855→30.065; deltas−0.030/+0.210
inside spreads1.259/1.518: noise under the owner's criterion.
Third-open medians improved1.182/1.071 MiB. All20 client runs pass close/lifecycle
bar+5. Bar/closed changes lie within measured spread; strict invariance unproven.
No large eager allocation found; no speculative fix. Known small code/metadata
footprint and buffer/allocator variation recorded; move on.

Next: opaque composition CPU/counter measurement, Feature9 Source UI+QML,
Feature10 from Nacre's engine, protocol adapter, STOP at GateD.
Nacre protocol v1 published10e86036; awaiting its CI before pinning.
Migration confirmed existing helper lane lacks ranked app/action/prefix queries;
F6 engine extraction is required for full Feature10; owner question pending.

111 tests/fmt/all-features clippy/CPU+release/deny/Luau pass for cache checkpoint.
First-open<1 MiB may proceed if bar+5 passes and no real steady/closed regression;
larger first-open or steady/closed regression still stops.
Backends frozen; no helper edits/catalog duplication. No live/install/release.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

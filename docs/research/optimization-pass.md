# Optimization pass

Gate C is accepted after the supervised 2026-10-15 physical test. All further
experiments are nested. Stop before Feature 9.

## Baseline and policy

The physical bar + CPU sound panel passed opening/closing p99 16.667/8.337 ms;
longest opening 58.330 ms and command→ready p50/p95 45.25/64.64 ms still
justify bounded hover preparation. Nested Gate C ready p50 was 48.63 ms
(Hyprland virtual 120 Hz) / 74.88 ms (niri nested 60 Hz).

Use CPU for small panels and GL for large animated surfaces. The earlier
explicit small-GL experiment retained about 20 MiB above the bar even after
EGL shutdown and allocator trimming, including Mesa/LLVM mappings. Dropping
Bead's caches cannot safely discard driver-owned process mappings. The CPU
sound panel passes the close/idle ≤5 MiB gate. Feature 6's ~105 MiB used a
larger GL stress panel, not this 400×286 content panel.

Typing 29.61→32.66% remains an optimization candidate. Nested niri/shared-GL
pacing and strict idle invariance from 6b step 0a remain open; the physical
small-CPU-panel result does not close those distinct experiments.

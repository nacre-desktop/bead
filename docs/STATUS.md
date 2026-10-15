# Status

## 2026-10-15 — Gate C accepted; optimization final checks pending

One supervised physical niri test passed: 20 cold sound-panel cycles,
opening/closing p99 16.667/8.337 ms, refresh 8.333313 ms at 120 Hz.
Longest opening 58.330 ms; cold ready p50/p95 45.25/64.64 ms.
Processes/PID file cleaned; physical authorization consumed. No install.

Retained: bounded hover preparation (2 s expiry, no surface/focus, closed
journal), generation bytecode, shared watcher, CGU1 and closed-error-scene
repair. CI through cb1ac45 passed. Shared watcher saves ~2.4 MiB warm,
one thread/three FDs; CGU1 saves 1.6–1.9 MiB mapped code on both.

Unread backend-model projection now measured: 200 background updates
cause ~200→0 bar commits, CPU 3.70→0.45% Hypr /3.43→0.25% niri.
Genuine focus change presents correctly; current models, future bindings,
computed reads and rejected-branch dependencies tested. 75 tests plus
fmt/clippy pass. No new services or Feature 9 work.

Rejected copies, two buffers (niri exhausted slots), smaller stacks/cache
cap, zero-stencil GL (wrong pixels), Thin LTO and stripping. CPU small
panels; GL large animated surfaces, prior ~20 MiB Mesa residue. Arena2
has no useful incremental gain with selected code; no tuning added.

Next: allocator hour endpoints (aging-*), then quiet final hover/cold,
typing, 60 s idle memory/CPU/pacing and lifecycle checks on both. Initial
aging attempts ended early; current memory-only runs record stress/deadline
failures and are not pacing/functional passes. Keep 29.61→32.66% typing,
6b 0a nested niri/shared-GL pacing and strict idle invariance qualified.
Stop after optimization, before Feature 9; no further live test authorized.

[Report](research/optimization-pass.md). Worktree: bead-optimization-pass;
existing shared Cargo target. Evidence: canonical .research/optimization.
Nested nice10/Cargo-j2; no installation or release.

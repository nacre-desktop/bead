# Status

## 2026-10-15 — Gate C accepted; optimization active

One supervised physical niri test passed: 20 cold sound-panel cycles,
opening/closing p99 16.667/8.337 ms, refresh 8.333313 ms at 120 Hz.
Longest opening 58.330 ms; cold ready p50/p95 45.25/64.64 ms.
Private services, passive IPC only; processes stopped and PID file removed.
The physical authorization is consumed. No install/config/audio changes.

On main: bounded audio-hover preparation (2 s expiry, no surface/focus,
closed journal) and generation bytecode reuse. CI 38724435974 and
38725224294 passed. Bytecode checkpoint: 71 tests, fmt/clippy pass.
Three pixel-checked hover trials: ready p50 9.99/7.39 ms Hypr/niri;
unused timeout and closed memory pass +5 MiB. Paired typing CPU
35.26→32.05% /36.89→36.26%; final quiet comparison still pending.

Shared watcher: warm 29.97→27.59 /29.85→27.48 MiB, one thread and three
FDs fewer. 72 tests/fmt/clippy and controls/clipboard/dismissal pass.
Stronger reload proof matches successful panel commit to exact presented
update; invalid recovery/reopen/SIGKILL pass both. Quiet checks pending.

Rejected scene-copy reduction, two SHM buffers (niri exhausted slots),
smaller stacks/cache cap, zero-stencil GL (wrong pixels). Depth-only still
selects 24/8/0. CPU for small panels, GL for large animated surfaces;
prior ~20 MiB Mesa residue remains the policy reason.

Allocator aging restarted separately from strict pacing after early
feedback failures. Aging runs active; release profiles next. Keep typing
29.61→32.66%, 6b 0a nested niri/shared-GL pacing and idle invariance open.
Stop after optimization, before Feature 9. No further live test authorized.

[Report](research/optimization-pass.md). Worktree: bead-optimization-pass;
existing shared Cargo target. Evidence: canonical .research/optimization.
Nested nice10/Cargo-j2; no installation or release.

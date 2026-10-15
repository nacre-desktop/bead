# Status

## 2026-10-15 — Gate C accepted; optimization active

The one supervised live niri test passed the owner's ≤16.7 ms gate:
20 cold open/close cycles, opening p99 16.667 ms, closing p99 8.337 ms.
Feedback refresh 8.333313 ms on 2880×1800/1.5/120 Hz. Opening longest
58.330 ms; command→ready p50/p95 45.25/64.64 ms remains a cold-start cost.
Private state/bus/audio; passive overlay, IPC only. Test processes exited,
PID file removed; authorization consumed. No install/config/audio change.

[Gate C report](research/feature8-gate-c.md) and
[optimization evidence](research/optimization-pass.md).
Physical bar/closed PSS+DRM 23.03/25.14 MiB (+2.11), no swap.
Two-second CPU samples ~5% are short live-backend observations, not the
nested 60-second idle comparison. Nested memory acceptance remains valid.

Next: bounded hover preparation, typing CPU, and ROADMAP optimization
candidates, each measured on both nested compositors. CPU for small
panels; GL for large animated surfaces (~20 MiB idle Mesa residue).
Keep 6b step 0a nested niri/shared-GL pacing and idle invariance open.
Stop after optimization, before Feature 9. No further live test authorized.

Worktree: ~/.local/share/nacre/ai/worktrees/bead-optimization-pass.
Shared target: /home/siverteh/Projects/bead/target.
Evidence: canonical .research/optimization. nice10/Cargo-j2.

# Status

## 2026-10-16 — Bounded rounded repairs kept; Gate D pending

The owner resumed the renderer with a two-hour time-box. Region-sized scratch
passes: first-open median +0.209/+0.120 MiB (H/N), warm +0.228/+0.068 and
closed +0.083/+0.130 fit their five-pair control ranges. Every hover/close
passes own bar +5 MiB. Scratch reuses buffers, has a process-accounted 16 MiB
cap, and is freed after 500 ms quiet or panel destruction. No canvas-sized
output mask is added. The old full-mask WIP remains local and out of the plan.

161 tests pass, including 6,400 exact partial/full pixel repairs and scratch
reuse/cap/idle release. Fmt, all-target/all-feature clippy with warnings denied,
CPU-only check and release pass. Five alternating 200-key pairs per compositor:
UI CPU H9.958→8.914 /N10.643→9.883 ms/key. Presented p99 H22.083→21.965 /
N20.675→25.010 ms; N increases inside its 9.096 ms control range, not a claimed
latency win. Both typing targets remain open. Nested Hvirtual120/N60; refresh
predictions unknown, shared-host contention uncontrolled, no own build overlap.
[Evidence](research/feature9-gate-d.md#bounded-rounded-repair--2026-10-16).

Next: conversion/guidance views, shared daemon UI-provider link and one-command
private nested preview. Daemon notification hosting and the official shared QML
adapter are still upstream work; existing notification hosting is transitional.
Keep typing, 6b pacing/idle, historical 29.61→32.66%, cache/IME/hardware and
binary-name gaps open. Stop at Gate D; no Feature 11/live/install/release.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

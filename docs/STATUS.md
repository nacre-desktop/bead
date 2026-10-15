# Status

## 2026-10-15 — Paused for foundation; typing targets not accepted

Owner redirected Features 9–10 to Foundation F0 / F5a–F5e, then STOP.
Current launcher/notification work is checkpointed; no new typing, catalog,
ranking or providers. Existing compositor backends are frozen except bug fixes.
The migration agent owns nacre-daemon / nacre-protocol and Nacre F0–F4.
[Checkpoint evidence and limits](research/feature9-gate-d.md).

81 workspace tests, fmt, clippy (-D warnings) and22-module Luau analysis pass.
Private Hyprland120 / nested niri60 proofs pass protocol, actual action clicks,
hover replacement, DND/history restart, passive OSD and owned-bus crash cleanup.
Empty/warm/closed PSS MiB: Hypr16.01/26.16/22.26; niri16.58/26.92/23.23.
No DRM;2-second idle samples are smoke only. No QML comparison or GateD verdict.
The child signal-mask fix prevents a private bus surviving the UI's SIGKILL.
Next: ADR0003 / roadmap, then host-agnostic notification API and headless tests.
No installation, live-session action or release is authorized.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

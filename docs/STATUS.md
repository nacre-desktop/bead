# Status

## 2026-10-15 — Paused for foundation; typing targets not accepted

Owner redirected Features 9–10 to Foundation F0 / F5a–F5e, then STOP.
Current launcher/notification work is checkpointed; no new typing, catalog,
ranking or providers. Existing compositor backends are frozen except bug fixes.
The migration agent owns nacre-daemon / nacre-protocol and Nacre F0–F4.
[Checkpoint evidence and limits](research/feature9-gate-d.md).

F0 ADR0003 main311a2f0 CI38764800206 green; pause32f741d CI38764519973 green.
F5a: injected notification connection/history API, serde snapshot, latest-wins
subscribe, bounded processed calls and async stop on the host's Tokio runtime.
Private-bus wrapper preserves shell use; CI enforces headless dependency graph.
83 tests, fmt, clippy pass; API parity has no UI/compositor. Nested H120/N60
notification interaction/restart/passive OSD/owned-bus cleanup pass.
Bar/first-warm/closed5s PSS+DRM MiB: H20.49/26.70/21.84,N22.32/28.34/22.62.
Three closes each and final lifecycle satisfy bar+5; no DRM. Other nested tests
were active: short regression smoke, not new CPU/pacing or GateC acceptance.
Next F5b: deterministic bead-test; then crash safety, Sources and background effect.
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

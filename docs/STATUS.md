# Status

## 2026-10-15 — Paused for foundation; typing targets not accepted

Owner redirected Features 9–10 to Foundation F0 / F5a–F5e, then STOP.
Current launcher/notification work is checkpointed; no new typing, catalog,
ranking or providers. Existing compositor backends are frozen except bug fixes.
The migration agent owns nacre-daemon / nacre-protocol and Nacre F0–F4.
[Checkpoint evidence and limits](research/feature9-gate-d.md).

F5b: MIT bead-test renders real Luau/store fixtures with fixed CPU/font/scale;
PNG goldens/diffs, input scripts, AccessKit assertions and replay budgets.
120 pose/opacity frames:0 actual Taffy passes,0 raster builds after one group
composite. Changed text triggers the counter. Shell --check now uses the harness.
88 tests/fmt/clippy, CPU-only check and independent framework cargo-deny pass.
Static symbol fixture matches full upstream font PNG exactly; test fonts only.
OSD scale1 golden exposes text beyond its pill; no Feature9 parity claim.
Next F5c crash safety, then Sources and background effect; STOP after F5e.

F5a40a979c CI38766103118 green: injected notification connection/history,
latest-one subscriptions, bounded processed calls and async host-runtime stop.
83 tests plus headless private-bus parity and nested wrapper interaction pass.
Bar/first-warm/closed5s PSS+DRM MiB: H20.49/26.70/21.84,N22.32/28.34/22.62.
Three closes + lifecycle pass bar+5; no DRM. Concurrent nested tests elsewhere;
short smoke, no renewed CPU/pacing acceptance. F0/pause CI also green.
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

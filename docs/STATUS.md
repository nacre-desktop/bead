# Status

## 2026-10-15 — Paused for foundation; typing targets not accepted

Owner redirected Features 9–10 to Foundation F0 / F5a–F5e, then STOP.
Current launcher/notification work is checkpointed; no new typing, catalog,
ranking or providers. Existing compositor backends are frozen except bug fixes.
The migration agent owns nacre-daemon / nacre-protocol and Nacre F0–F4.
[Checkpoint evidence and limits](research/feature9-gate-d.md).

F5c source checkpointf49217c: main-loop watchdog, first-frame last-good startup
fallback and crash-loop safe bar.94 tests/fmt/clippy/CPU-only check pass.
Nested H120/N60: watchdog abort5.078/5.071s after actual UI-stall start;
first-frame save, bad source/palette boot, fixed-source reload and safe bar pass.
Bar/first-warm/closed5s PSS+DRM MiB: H21.67/27.11/22.30,N23.19/30.50/23.18.
Three closes + lifecycle within bar+5; no DRM. Short contended regression smoke.
Pending validated bundles capped8MiB/32 generations; no old-feedback rollback.
Next F5d Sources / published protocol adapter or fake fallback, then F5e; STOP.

F5b39b4270 CI38767973872 green: MIT headless CPU/font/scale harness, PNG diffs,
input/AccessKit/replay;120 pose frames0 Taffy/0 rasters after initial composite.
OSD scale1 golden exposes text beyond its pill; no Feature9 parity acceptance.
F5a40a979c CI38766103118 green: injected headless notification host API,
bounded calls/latest-one subscription, private wire parity and nested use pass.
F0/pause CI green. Renderer/typing baseline acceptance remains unchanged.
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

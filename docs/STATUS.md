# Status

## 2026-10-15 — Paused for foundation; typing targets not accepted

Owner redirected Features 9–10 to Foundation F0 / F5a–F5e, then STOP.
Current launcher/notification work is checkpointed; no new typing, catalog,
ranking or providers. Existing compositor backends are frozen except bug fixes.
The migration agent owns nacre-daemon / nacre-protocol and Nacre F0–F4.
[Checkpoint evidence and limits](research/feature9-gate-d.md).

F5d sourcee3134fd + origin-order fix: neutral snapshots/merge patches, atomic
store projections, bounded shared payloads/listeners, deadlines and reconnect hooks.
Notification UI uses Source with unchanged Luau bindings; Desktop adapter tested,
accepted bar keeps its typed path.103 tests/fmt/clippy pass; CPU check/deny pass.
Protocol v1 unpublished at F5d start: allowed fake-source fallback, adapter skipped.
No draft GPL types/transcripts entered framework; resume adapter after publication.
Nested Source notification protocol/real buttons/hover/restart/cleanup pass H/N.
Bar/first-warm/closed5s PSS+DRM MiB: H21.09/27.14/22.00,N22.86/30.16/22.83;
three closes + lifecycle pass bar+5. Short contended smoke, not new pacing/CPU gate.
Next F5e capability-gated background effect + cache/font audit, then STOP.

F5c9dfc295 CI38772441876 green: startup last-good, watchdog and safe bar.
Nested watchdog actual busy5.078/5.071s; corrupt source/palette and fixed reload pass.
F5b39b4270 CI38767973872 green: deterministic MIT CPU/Luau/golden/input/a11y harness;
120 transform/opacity frames0 Taffy/0 rasters after initial composite.
F5a40a979c CI38766103118 green: hostable headless notification API/private parity.
OSD scale1 golden has text outside its pill; Feature9 parity/typing not accepted.
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

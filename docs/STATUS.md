# Status

## 2026-10-15 — Source adapter green; Feature 9 comparison complete

Main f154f47 / CI38810904821 green: pinned SDK6bdc22e, seven transcript families,
nine adapter tests, real private standalone daemon proof on H120/N60, cleanup.
All123 tests, fmt/clippy/CPU+release/Luau/framework/service/license checks pass.
[Adapter method](research/foundation-f5.md#nacre-source-adapter--2026-10-15).

[Feature 9 measurements and limits](research/feature9-gate-d.md#feature-9-source-ui-comparison--2026-10-15):
60-second PSS+DRM MiB, Bead/QML H and N: notice20.78/161.22,21.59/151.21;
volume OSD22.37/125.09,23.80/121.98. Same populated content; corrected same-SVG
notice captures replace rejected missing-icon samples. No heavy local builds.
Bead idle0.20–0.25% versus QML0 observed ticks; transitional private desktop
bus/audio tasks included. No notification server in these static fixtures.
Real private-bus actions/hover/history/DND/legacy/four OSDs/crash cleanup pass
on both; no ambient service takeover. Semibold/rich images/swipe/row-height
polish and matched QML presentation timing remain open, not full pixel parity.

Cache and opaque follow-ups complete; cached motion saves15.4%/42.0% paint CPU.
Next: Source menu.query fixture launcher UI and typing, then real Nacre engine
(after F2/before F3). End-to-end typing waits engine; no Bead ranking/providers.
STOP at Gate D. Backends frozen; no live/install/release permission.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

# Status

## 2026-10-15 — cache follow-up green; opaque composition completed

Main9f933a4 / CI38799006208 green. [Cache five-pair report](research/cache-budget.md).
First-open deltas−0.030/+0.210 MiB lie inside spread; no speculative fix.
Third-open savings1.182/1.071 MiB; all20 clients pass close/lifecycle bar+5.

[Opaque composition follow-up](research/foundation-f5.md#opaque-composition-follow-up):
release CPU ms/frame direct→cached: bar2.100→1.776, sound6.098→3.535.
Native bar geometry applied; cached only during motion, direct/release at rest.
Scale1.5 pixels/counter/idle-release pass;112 tests/fmt/all-features clippy/CPU+
release builds pass. Native H/N close/lifecycle bar+5 pass; contended smoke.
This checkpoint's CI is checked after push. No new physical/typing acceptance.

Next: Feature9 Source UI/controls/hover/legacy + matched QML and protocol adapter.
F1 protocol10e86036/goldens published; upstream CI checked before consumption.
Owner authorized Nacre engine afterF2/beforeF3. Feature10 UI uses fake menu.query
and published schema (QML result shape until available); UI typing with fixtures,
end-to-end waits real engine. No ranking/providers/catalog engine in Bead.
STOP at GateD. Backends frozen; no live/install/release permission.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar+5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

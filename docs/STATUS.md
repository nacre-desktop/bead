# Status

## 2026-10-16 — Measured Luau helpers; real typing targets still open

Published Sourcef664941 /CI38881989088 and catalog fixefacf80 /
CI38885455648 are green. This checkpoint moves only value constructors (brushes,
bind/action markers) into Luau.159 tests/fmt/clippy ALL targets/CPU-only/release
pass; coercions, shared args, nil/false, diagnostics and fresh sandbox remain.

Five typing pairs each: UI CPU H11.243→10.727/N11.309→10.695ms/key;
p99 medians22.514→22.360/24.149→21.145ms. Five memory pairs: warm deltas
+0.042/+0.012MiB,closed+0.120/−0.023,inside control spreads. All4000 exact
key and60 lifecycle/hover checks pass; first-open unchanged, no threshold waiver.
This is a fixture win, not an accepted8ms/16.7ms target or cross-run-set speedup.
[Method and exclusions](research/feature9-gate-d.md#native-luau-value-constructors--2026-10-16).

Prior real-engine1200-key medians remain UI12.230/12.047,daemon8.332/8.101,
worker1.944/1.856ms/key-window,p9931.559/30.360ms. Real35s settled groups
53.19/55.12MiB fit bar51.62/54.50+5. Native A/B on9374bc8d gives UI11.90→11.36/12.11→11.73ms/key in one pair
each; H p99 is higher once, so no native tail acceptance. Nested Hvirtual120/N60,scale1.5,refresh prediction unknown;
no physical validation, external host contention uncontrolled, no own build overlap.

Codec100-result decode~0.17ms: no replacement decoder justified. Keyboard-batch
diagnostic has no consistent win; dropped, original driver retained. Thread profile
accounts for whole-trial work separately from exact input→feedback CPU. No missing
measurement is zero. Property-map borrowing/duplicate bake copy/SDKopt2 rejected.

Migration F3 is ALREADY authorized after remaining F2; no extra owner gate and no
parallel QML adapter. Basic arithmetic is an engine gap; no client parser. Next:
fix the exact rounded-repair pixel regression found on the unchanged renderer
before its coverage optimization; then further measured UI work and QML on its shared
adapter and Gate D report. Keep6b pacing/idle,29.61→32.66% workload,conversion/
guidance visuals,UI-provider/reconnect,IME/hardware/cache/name-clash gaps open.
STOP at Gate D; no Feature11/live/install/release work. SDKb44d1afb/menu goldens
pinned; upstream9374bc8d /CI38883651768 green, query-cache candidate not published.

## Accepted Gate C and optimization baseline

[Optimization report](research/optimization-pass.md). Main baseline c8c8058.
One physical niri120Hz test:20 cycles, open/close p99 16.667/8.337 ms; accepted.
Permission consumed and cleanup verified. Optimized source tested nested only.
Bar/warm/closed60s MiB: Hypr20.13/26.67/21.32; niri21.48/29.03/22.05.
Every close/release/lifecycle stayed within bar + 5 MiB, including swap.
CPU small panels; GL large animated views; ~20 MiB Mesa residue documented.
Historical typing29.61→32.66%, 6b0a nested/shared-GL pacing and strict idle
invariance remain open. Real niri hotplug/IME/Orca/NVIDIA remain unvalidated.

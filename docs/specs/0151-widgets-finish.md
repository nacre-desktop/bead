# B0151 — Widgets finish slice

Occupied bar slots now nest the dragged clock into the overlapping status pill and persist the layout at 82% and 90% bar width.
The ghost has a tinted slot marker; settle and invalid-return motion use the reusable `out-back` curve.
Clock width follows its live format, and the shipped default content bar preserves its prior pixel output.
Widget design polish remains B0152; these checks cover the existing clock forms and bar-growth regressions only.

| Proof | Result |
| --- | --- |
| Desktop clock drop at 82% / 90% | File and store six-state runs pass; canonical store layout nests the clock in the status pill. |
| Drag, app, resize, pearls, focus | Both six-state drag runs, seven app states, resize, API detach/dock, and popup focus proof pass. |
| Default bar / full CI | Default bar goldens remain byte-identical; remote Bead CI passes at `ea5efd7c` (750 passed). |
| 5-pair CPU memory gate | Official gate rejects all candidate pairs: first-key UI CPU 26.45–28.17 ms (>8 ms). Diagnostic paired medians: idle +1.758 MiB; launcher +7.834; typed +12.670; closed +162.533; after drag/close +378.191. Budget: 2 MiB. |
| 60 Hz CPU snap cadence | Fails after source hover: first overlay 143–166 ms, first bar paint 145–146 ms, 24–26 dropped bar frames per repeat. |
| 120 Hz | Not measured: the private Niri Winit output remained at 60 Hz despite parent/custom-mode requests. |

The memory medians use all five same-host pairs; the diagnostic summarizer relaxes only its 8 ms first-key validator so the memory deltas remain visible. The official gate is failed, not passed. Desktop captures are private CPU sessions at 2880×1800/1.5 with host swap at 0 MiB. Existing QML/Bead default-bar pairs are in [B0113 evidence](evidence/0113/idle-pair.png); the Widgets app has no QML equivalent, and its preview-stage design remains an explicit B0152 gap. No push was made because the memory and snap gates are red.

The overlap fallback is in `shell/widgets/app/behavior.luau`; reusable easing is in ADR 0004. No temporary duplicate product paths were added.

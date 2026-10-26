# Direct Labwc finger stop, fling, and cancellation

This private stop/cancel probe bypassed nested Niri for the Bead client because Niri's winit backend forwarded the test finger stream as `CONTINUOUS` and omitted client `axis_stop`. The existing nested Labwc parent was set to 2880×1800 at scale 1.5; Nacre's helper and fake provider remained private. Bead used its default panel renderer (no `--cpu` override). Control was the clean-target `aac59abb…` source/binary `e3545eefe0bd…`; candidate was `84826bb5…` / `3db7085c909b…`, with the same Bead shell tree `52a5777a…` and exact presentation fixture. Source validator passed; host swap was 0 before and after; both arms returned rc 0 with no UI errors.

Both client protocol logs contain `axis_source(1)` (finger) and the first `wl_pointer.axis_stop`. The test sent 16 positive 10 px events at 8 ms, stopped, waited about 402 ms, then sent six negative 7 px events. Before the opposite input, control presented 21 post-stop positive state advances, offset 480→1203.93 px; candidate presented 48, offset 160→524.66 px. These offset changes join the same native-window epoch, generation, and update to actual `WINDOW_PRESENTED` records, so kinetic motion after axis stop is proven in this direct private-parent transport.

On opposite input, control delivered one additional positive sample and presented a stale positive offset before reversing at 16.41 ms from input (16.24 ms from delivery). Candidate delivered no positive sample after the negative input; its first presented state reversed at 11.35 ms from input (8.57 ms from delivery). This is improved cancellation but remains above the nominal 8.33 ms one-frame target. Candidate render p95 was 3.139 ms; whole-window presentation p95 was 8.57 ms, maximum 8.68 ms, with one missed-slot upper bound and no discarded feedback. `refresh_ns=0`; these nested timestamps do not prove physical 120 Hz scanout.

| Screenshot | Control | Candidate |
|---|---|---|
| Before | ![Control before](control-before.png) | ![Candidate before](candidate-before.png) |
| After | ![Control after](control-after.png) | ![Candidate after](candidate-after.png) |

Run: `b0135-kinetic-direct-labwc-84826-20261026-01`; private copied harness SHA-256 `d3ce4c47f576b5f51ff76d55d3cc6fd63ce44e80edeff0a1e8af345c1cb071a1`; stopv2 pointer SHA-256 `f69b088146a765f0b47497f16e93507c990a7413a95ed3cbd9ebb7565af4d6b7`.

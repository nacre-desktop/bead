# B0177 Prepared panel reveals (2026-10-28)

Status: implementation candidate; runtime acceptance is blocked by the private-capture swap gate. B0175 prevents the open pose from appearing before a reveal starts, but dense CPU reveals prepare content again on each slide frame. Prepared layers now retain that first-frame guarantee while reusing rendered content. Full Bead CI passed on code commit `ef2d873d`; control-centre warm-open and close-frame captures remain pending. QML/Bead motion parity is unverified. No live laptop test is included in this unit.

| Measure | B0175/live reference | Target | Candidate |
| --- | ---: | ---: | ---: |
| Dashboard / control-centre frames per slide | 36–43 / 16–22 | control-centre at least dashboard-like | pending |
| Control-centre first content frame on repeat opens | 120–160 ms | <60 ms | pending |
| Dashboard / control-centre slide CPU | pending per-frame samples | at most half of 1fe079e0 | pending |
| Ghost-bar frames on close, control-centre / launcher | reported; not isolated | 0/5 each | pending |
| First vs later open frames and max gap | pending | within 10% | pending |
| Reveal flash, dashboard / launcher / control-centre / session | B0175 0/3 dashboard and launcher | 0/3 each | pending |

The reference markers were recorded at 2880×1800, 120 Hz, scale 1.5, CPU renderer. At the last host check, swap use was 3.3 GiB and MemAvailable was 2.3 GiB; the reset rule requires more than swap use plus 8 GiB, so no capture ran. Capture evidence and per-frame CPU will be stored here after that gate passes. Close-frame sequences will identify the lingering surface or pixels before a source fix is chosen.

| Additional resource | Budget / treatment |
| --- | --- |
| Warm prepared control-centre content | record candidate unreclaimable memory and clean-file PSS separately; task permits memory for smooth repeat opens |

No temporary duplicate implementation is planned. The general prepared reveal primitive belongs to the renderer; panel-specific layout and behavior remain in shell Luau.

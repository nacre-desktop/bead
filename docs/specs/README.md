# Implementation spec queue

Updated 2026-10-17. [Standing owner rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply to every assignment.
Spec agent owns design, review, queue order and measurement grants. Implementers
must not infer permission from an old roadmap or reserve a measurement slot.

| Order | ID | Assignment | Status | Dependencies |
| --- | --- | --- | --- | --- |
| 1 | B0001 | [Continue core dashboard](0001-dashboard-core-completion-audit.md) | Blocked | functional accepted; owner0192 policy ruling |
| 1a | B0004 | [Dashboard review coverage fixes](0004-dashboard-review-coverage-fixes.md) | Done | reviewedef5406f; CI39075073594green |
| 2 | B0002 | [Dashboard lifecycle/memory](0002-dashboard-lifecycle-memory-gate.md) | Review | early artifact/preflight report submitted; captures gated |
| 2a | B0005 | [Capture-window correctness](0005-dashboard-capture-window-correctness.md) | Ready | B0002 preflight accepted; fake clocks only |
| 3 | B0003 | [Typing dominant-cost investigation](0003-launcher-typing-investigation.md) | Review | source-only checkpoint; remaining steps gated |

First Ready assignment is authorized after its listed prerequisites pass. Existing
work is preserved; send exact unpublished state before overlapping changes. Ready
with dependencies means fully specified, start only after dependencies are Done.

Status flow: Ready → In progress → Review → Done; Blocked names an exact missing
dependency or decision. Start the first Ready row whose dependencies are Done.
Report completion to the spec agent and continue to the next eligible Ready row.
Only the spec agent marks Done. Ask before deviating from scope.

Measurement slot: **released; no active grant**. The former F4 capture was
stopped and owned-worker cleanup confirmed. Partial evidence is preserved;
incomplete cells do not count. N0001 source audit may proceed. N0002 requires
source-ready review plus exact full artifact hashes, host scope and time window,
then a separate explicit grant. Neither implementer may start measurements from
this index. No live/physical testing is authorized.

Beyond this queue: device controls after published implemented Nacre capabilities,
then hosted notifications, session/hyprlock and static wallpaper. Hardware and
packaging remain owner-held. Optional dashboard extras require scope review.

B0003 source phase accepted at214b0f0; next eligible action is B0002 early
artifact/preflight request (45min, existing files/docs only), not measurement.

B0002 preflight accepted atbb75590; next eligible B0005 harness correctness
(fake-clock Python tests only), no binaries/builds or measurements granted.

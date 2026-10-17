# Implementation spec queue

Updated 2026-10-17. [Standing owner rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply to every assignment.
Spec agent owns design, review, queue order and measurement grants. Implementers
must not infer permission from an old roadmap or reserve a measurement slot.

| Order | ID | Assignment | Status | Dependencies |
| --- | --- | --- | --- | --- |
| 1 | B0001 | [Continue core dashboard](0001-dashboard-core-completion-audit.md) | Review | scoped advisory exception; green CI |
| 2 | B0002 | [Dashboard lifecycle/memory](0002-dashboard-lifecycle-memory-gate.md) | Ready | B0001 Done; measurement grant |
| 3 | B0003 | [Typing dominant-cost investigation](0003-launcher-typing-investigation.md) | Ready | B0002 Done; measurement grant |

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

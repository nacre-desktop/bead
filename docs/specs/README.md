# Implementation spec queue

Updated 2026-10-17. [Standing owner rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply to every assignment.
Spec agent owns design, review, queue order and measurement grants. Implementers
must not infer permission from an old roadmap or reserve a measurement slot.

| Order | ID | Assignment | Status | Dependencies |
| --- | --- | --- | --- | --- |

Specs are being prepared against the verified remote main. No new implementation
assignment is granted by this initial index. Existing work is preserved; send
its exact revision and state to the spec agent before overlapping changes.

Status flow: Ready → In progress → Review → Done; Blocked names an exact missing
dependency or decision. Start the first Ready row whose dependencies are Done.
Report completion to the spec agent and continue to the next eligible Ready row.
Only the spec agent marks Done. Ask before deviating from scope.

Measurement slot: **ungranted**. Existing captures, if any, must report ownership
and release before a new grant. No live/physical testing is authorized.

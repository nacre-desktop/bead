# Bead implementation queue

Updated2026-10-18. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [B0001 dashboard](0001-dashboard-core-completion-audit.md) | Done | Functional review accepted; owner ratified0192 |
| 1a | [B0004 coverage](0004-dashboard-review-coverage-fixes.md) | Done | Review accepted |
| 1b | [B0005 windows](0005-dashboard-capture-window-correctness.md) | Done | Eleven hosted mocked tests accepted with B0006 |
| 1c | [B0006 keeper](0006-dashboard-keyboard-keeper-lifetime.md) | Done | Review410bf09/CI39081728470green |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Blocked | Missing wayland-info; incident review and guarded capture preflight |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Ready measured phase | B0002 pass; same remaining slot, deliver evidence for next product fix |

Restart recovery2026-10-18: published head before recovery1cff9d1. Bead capture
worktree is clean there; retained calendar/dashboard/service-view drafts are
preserved. B0002 release artifact exists, but the first attempted cell failed
before compositor start because wayland-info is missing: no valid new samples.
B0002 is Blocked on that prerequisite and incident/safety preflight. No package
installation, replacement probe or harness assertion weakening is authorized.

The former00:55–10:55UTC grant is superseded. Reissue B0002 then B0003 only after
incident review, guarded private scope/memory checks and explicit peer release.
Spec agent supplies actual UTC window and artifacts in the grant; an idle host
or this queue is not a grant. No live tests/install/unit enablement.

No new tooling-only specs: small fixes/tests fold into B0002/B0003. Status changes
at start/review/Done; batch notes. No repeated audit/status-only reports while
product work is ready. New informational unmaintained exceptions may be approved
by spec agent under standing criteria; vulnerabilities/unsoundness go to owner.

Replacement reservations (not active capture grants): B0002 Oct18 11:00–19:00UTC,
then B0003 19:00–21:00UTC. Require incident/safety review, missing dependency
resolution and spec-agent activation; details in each spec. No automatic start.

# Bead implementation queue

Updated2026-10-18. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [B0001 dashboard](0001-dashboard-core-completion-audit.md) | Done | Functional review accepted; owner ratified0192 |
| 1a | [B0004 coverage](0004-dashboard-review-coverage-fixes.md) | Done | Review accepted |
| 1b | [B0005 windows](0005-dashboard-capture-window-correctness.md) | Done | Eleven hosted mocked tests accepted with B0006 |
| 1c | [B0006 keeper](0006-dashboard-keyboard-keeper-lifetime.md) | Done | Review410bf09/CI39081728470green |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | In progress, preflight blocked | `wayland-info` missing; minimal install denied (`sudo: a password is required`) |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Review (B0003-S) | Source/test checkpoint submitted; measured work remains gated on B0002 |
| 4 | [B0007 UI-provider failures](0007-ui-provider-task-failure-replies.md) | Done | Accepted at736e778a; exact-head CI39125066926 green |
| 5 | [B0008 UI-provider admission](0008-ui-provider-bounded-admission.md) | In progress | Bounded registered-method admission; source/tests only, no lab dependency |

Incident CLOSED by owner2026-10-18: likely lost Codex connection; no OOM/oomd
kills or relevant kernel crash attribution. Worktrees and artifacts preserved.

B0002 ACTIVE private nested grant Oct18 11:00–19:00UTC, followed by B0003
19:00–21:00UTC after B0002 PASS. Use the prepared source1cff9d1 release UI and
5f4 helper, with reviewed guarded harness4d0861d. Scope/memory/peer-pause and
quiet gates remain mandatory. Send CAPTURE_START and explicit release; no
incident/activation permission wait remains. Nacre N0002 follows host release.

Owner go-now authorizes the minimal missing diagnostic prerequisite needed for
this private-lab capture (wayland-info); preserve harness assertions and existing
artifacts. No broader toolchain/system upgrade, replacement probe or live desktop
change. If privilege or actual dependency setup fails, report the exact blocker
and do independent typing source/test preparation rather than wait idle.

No new tooling-only specs: small fixes/tests fold into B0002/B0003. Status changes
at start/review/Done; batch notes. No repeated audit/status-only reports while
product work is ready. New informational unmaintained exceptions may be approved
by spec agent under standing criteria; vulnerabilities/unsoundness go to owner.

Current B0002 capture is blocked: wayland-info is absent and the one authorized
minimal install attempt failed with `sudo: a password is required`. No capture
started; the private lab was released to Nacre. Preserve the prepared artifacts.
B0003-S source/tests are submitted for Review; measured B0003 remains gated on
B0002 passing and an explicit peer release. Do not overlap host work.

Independent product queue: B0007 is submitted for review; it has no
capture/B0002 dependency. No measurement was taken.

B0007 Done at736e778a, source review/CI39125066926 accepted. Next independent
Ready product task: [B0008 bounded UI-provider admission](0008-ui-provider-bounded-admission.md),
90min source/private tests+hostedCI, no B0002 or lab dependency.

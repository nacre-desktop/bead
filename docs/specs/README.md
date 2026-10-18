# Bead implementation queue

Updated2026-10-18. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [B0001 dashboard](0001-dashboard-core-completion-audit.md) | Done | Functional review accepted; owner ratified0192 |
| 1a | [B0004 coverage](0004-dashboard-review-coverage-fixes.md) | Done | Review accepted |
| 1b | [B0005 windows](0005-dashboard-capture-window-correctness.md) | Done | Eleven hosted mocked tests accepted with B0006 |
| 1c | [B0006 keeper](0006-dashboard-keyboard-keeper-lifetime.md) | Done | Review410bf09/CI39081728470green |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | In progress | Build/identify release artifacts, fold0192reason update, execute granted captures |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Ready measured phase | B0002 pass; same remaining slot, deliver evidence for next product fix |

Exclusive private/nested grant: Bead first,2026-10-18 00:55–10:55UTC, B0002 then
B0003, existing nested lab only. Record commit/build-profile/binaryhash; announce
CAPTURE_START to spec agent/Nacre after preparation and stop other jobs on that
host before measuring. Build only missing artifacts in existing isolated setup.
No separate owner permission or elaborate source-attestation chain required.
Release early; Nacre N0002 follows when ready. No live tests/install/unit enablement.

No new tooling-only specs: small fixes/tests fold into B0002/B0003. Status changes
at start/review/Done; batch notes. No repeated audit/status-only reports while
product work is ready. New informational unmaintained exceptions may be approved
by spec agent under standing criteria; vulnerabilities/unsoundness go to owner.

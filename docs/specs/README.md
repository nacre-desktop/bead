# Bead implementation queue

Current state2026-10-18. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md)
apply. This summary supersedes older schedules/dispositions inside historical
spec sections and thread messages. Batch status notes at start/review/Done.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [B0011 Battery popout](0011-daemon-battery-status-popout.md) | In progress | Typed BatteryState, optional readings preserved, lazy shared lease and private tests; no lab. |
| 2 | [B0010 Bluetooth popout](0010-daemon-bluetooth-status-popout.md) | Done | Accepted `e3e417e`; exact-head CI `39136586458` green. Spec review only; producer remains capability-gated. |
| 2 | [B0009 Network popout](0009-daemon-network-status-popout.md) | Done | Accepted bd7bf56/source5d96384; exact-head CI39134388246 green. Capability-gated, no measurement claim. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Blocked | Missing wayland-info; minimal authorized install failed sudo password required. No retry/probe/capture until prerequisite and new slot. |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Source B0003-S Done; measured phase gated | Source89afb381 accepted, CI39123319816 green. No repeated static reports; measurements after B0002 PASS/new slot. |
| 4 | [B0008 admission](0008-ui-provider-bounded-admission.md) | Done | Exact0deb314/CI39128553160 accepted; no further review wait. |
| 5 | [B0007 handler recovery](0007-ui-provider-task-failure-replies.md) | Done | Exact736e778/CI39125066926 accepted. |
| 6 | [B0001 dashboard](0001-dashboard-core-completion-audit.md) | Done | Functionality and owner0192 exception accepted. |
| 7 | [B0004 coverage](0004-dashboard-review-coverage-fixes.md) | Done | Accepted. |
| 8 | [B0005 windows](0005-dashboard-capture-window-correctness.md) | Done | Accepted with B0006; guardfix4d0861d/CI39120758403 green. |
| 9 | [B0006 keeper](0006-dashboard-keyboard-keeper-lifetime.md) | Done | Accepted410bf09/CI39081728470 historical receipt. |

## Current execution limits

**No Bead host/capture slot is active. All earlier00:55–10:55,11:00–19:00 and
19:00–21:00 grants/reservations are withdrawn or blocked.** Do not infer a grant
from old messages, Nacre release or an idle lock. No shared-host readiness probes
are needed while the dependency is unchanged. Spec agent schedules the next
actual window after prerequisites; no valid arm may be preempted.

Prepared release runtime1cff9d1/b01dfcca and helper5f4/4a65f4ec remain immutable
B0002 artifacts; later B0007/B0008/B0009 product code is independently tested,
not silently substituted into that lane. Failed cells are invalid, not zero.
Use reviewed guard,4GiB available start/abort below2GiB,private capped2GiB scope,
external watchdog, explicit peer pause and owned cleanup for any future capture.

Incident CLOSED by owner; likely lost Codex connection. Safeguards remain.
Nacre has a conditional correctness-only interval14:15–15:15 UTC; no Bead host work before explicit release;
its N0003 acceptance claims were withdrawn. B0009 may consume the already
published contract with fixtures but must not claim a running producer exists.

No further B0003-S/B0007/B0008 edits or report polishing requested. B0011 source /
private tests and hosted CI are eligible immediately without capture approval.
No runtime optimization/memory acceptance claim, new protocol/dependency or
live desktop/install/unit/config/AI-policy changes. No sudo retry authorized here.
Use isolated worktree, approved identity, direct normal main push, no force push,
check exact-head CI. Private raw artifacts stay outside public Git.

## Lead handoff — 2026-10-18, standing workflow installed

Latest owner workflow supersedes separate terminal worker chats: lead-workflow
skill with BUILT-IN subagents, maximum3 active, separate writer worktrees, one
heavy job including hosted full CI. Defaults Sol6.1/medium lead, Luna/medium
worker, explorer Luna/low; one clearer failed retry then Sol, reviewer before
nontrivial integration, Terra second opinion for high-risk work. Existing AI
permissions unchanged. Older visible-chat instructions are historical only.

Current product state: B0001/B0004/B0005/B0006/B0007/B0008/B0009/B0010 Done;
B0003-S source checkpoint Done, measured B0003 gated on B0002. Network and
Bluetooth typed consumers remain capability-gated; no running-producer claim.

B0011 separate implementer01a13c5f-bf69-7cd3-bc54-6c176ea0aabd finishes current
Battery task then retires without new assignments. At the final handoff refresh, main advanced through72c43f6,6f816eb and4e8ad73
(private lifecycle synchronization/lease follow-ups). CI39141528958/72c43f6
FAILED; CI39141806219/6f816eb was in progress. Verify exact current head and
latest run rather than treating any of these as accepted. Earlierbf02b13 /
39140928302 and4dac3b3/39139295854 also FAILED; don't claim them green. Worker must submit final exact-head evidence
for review; if another failure, preserve source and escalate to built-in Sol
rather than repeat Luna guesses. No shared-lab Bead work is active/authorized.

Owner installed wayland-utils; missing-wayland-info/sudo blocker is SUPERSEDED.
Priority now: Nacre F4 publication, reviewed owner switch-on/rollback plan, THEN
BeadB0002/B0003 captures, then core-service implementations. B0002 therefore waits
scheduling, not package installation. Preserve frozen1cff9d1 runtime artifact and
helper5f4; later Battery/Network/Bluetooth sources are not silently substituted.

Nacre N0002 separate worker01a14f98-71e5-7131-98cc-f9dbd3ff4561 holds conditional
15:30–18:30UTC window; capture source/mock held-child containment gate is still
outstanding at last report. Both retiring peers explicitly ACKed IDLE. No
capture payload can overlap Bead hostedCI. Lead must verify current CI/workers,
not infer idle from window expiry. Capture requires explicit exact artifact/host/
window grant,>=4GiB available start/abort<2GiB, verified private2GiB/swap0scope
and external watchdog; only own processes may be stopped. No new Bead heavyCI
while N2 capture is armed; batch documentation pushes to avoid unwanted suites.

Next: obtain B0011 exact-head result and reviewer disposition; finish/retire that
chat. After F4/plan gates, schedule B2 five alternating pairs per compositor
using corrected capture windows/keeper/guard, then B3 measured work. Use built-in
workers with one spec/worktree each; no new separate terminal chats. Preserve
all parked drafts and raw evidence. No package/live UI/config/unit action or
performance acceptance inferred from this handoff. Docs-only [skip ci] here
prevents an unintended full suite; code checks remain required.

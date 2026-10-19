# Bead implementation queue

Current owner handoff: morning2026-10-19. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Eligible work |
| --- | --- | --- | --- |
| 0 | [B0014 Bluetooth single-adapter power](0014-bluetooth-single-adapter-power.md) | Ready: light source | Exactly-one-adapter control against published schema; source-only while Nacre diagnostic owns heavy slot. |
| 0 | [B0013 Wi-Fi radio switch](0013-network-wifi-radio-switch.md) | Done | Sourcecdd9fb2 integrated021fdaad; exact-headCI39230443828 SUCCESS. Capability-gated, no live producer/enablement claim. |
| 1 | [B0012 light measurement prep](0012-measurement-readiness-light-prep.md) | Done: light prep accepted | Report14b93337 accepted; command routing and geometry/scope gaps recorded. No runtime/tool changes or captures. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Waiting for slot after F4 | Desktop wayland-utils installed by owner and command availability verified; prerequisite resolved. Preserve frozen runtime1cff9d1/helper5f4; no slot. |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Source Done; measured gated | B0003-S89afb381 accepted/CI39123319816 green. Measured work after B2 PASS/new slot; no static polishing. |
| — | [B0011 Battery](0011-daemon-battery-status-popout.md) | Done | Sourceaf61d18/fullCI39147491406green; docs5ad2551. Lazy lease/optional readings/private lifecycle tests accepted. |
| — | [B0010 Bluetooth](0010-daemon-bluetooth-status-popout.md) | Done | Sourcee3e417e/CI39136586458green. Capability-gated; no running producer claim. |
| — | [B0009 Network](0009-daemon-network-status-popout.md) | Done | Source5d96384/docsbd7bf56/CI39134388246green. Capability-gated. |
| — | B0001/B0004/B0005/B0006/B0007/B0008 | Done | Prior accepted reports retained; no duplicate work. |

## Current execution ownership

B0013 focused correctness finished and RELEASED08:39UTC. Final sourcecdd9fb2
independently ACCEPTED;11 actor ticket,5 Wi-Fi Runtime and1 template tests PASS,
fmt and affected all-target Clippy PASS. Earlier failures are retained. Exact integration021fdaad published; full hostedCI39230443828 SUCCESS. Evidence archive6ccfe29daa51e4c6a92fc445552a6cdd47fae08facfee0423a8284a0a6517f71.
Scope manager/effective12GiB/swap0 and held-child membership verified; no OOM,
peak2,021,376,000B. Actual SCHED_IDLE/priority19 preserved; no host policy change.

Nacre Niri-only source8a quick pair released09:28UTC with valid cells but failed
memory screen +5.756/+6.121/+0.488MiB. No H/full-eight; no current capture.
Bead used the released sole heavy slot for021fdaad CI39230443828, now completed
SUCCESS09:37:56UTC. All workers released except read-only Nacre diagnosis.
Bead B2/B3 measurement remains after F4 and explicit grant.
Owner clarified installation was first laptop, then desktop08:08:50UTC during
an earlier Nacre candidate warmup: ONLY wayland-utils, no other package changes.
Desktop /usr/bin/wayland-info availability subsequently verified; no probe run.

B0012 light prep is complete. B0013 remains capability-gated to actual Welcome
method plus fresh topic; no producer/pin change. All task workers released.
Other device controls/static wallpaper need a concrete next bounded spec.

Frozen B2 runtime1cff9d17/releaseb01dfcca and helper5f4/4a65 remain separate from
newer shell features. Source/profile/hash enough for nested provenance; unknown
artifact identity remains unknown. Raw private evidence stays outside Git.
Capture rules: owner-sized scope max(knownbaselinepeak×1.5,12GiB) onlab,swap0,
verified heldchild containment and independent watchdog; host>=4GiBstart/abort<2GiB;
quiet SwapPss0/hostswapin<1MiB; only own exactscope/PIDs stopped. No hostpressure,
cache drops orsysctl. Geometry means actual output proof, not config request.

## Workflow and owner boundaries

Built-in workers only, maximum3 active; separate old implementer chats retired.
One heavy job globally incl hosted full CI/build/nested/capture. Lead grants.
Light docs/read-only prep may proceed in parallel. Lead staysWorking while
workersrun, postsstarts/finishes and about30min long-job heartbeat (global skill).

Commit approvedsiverteh noreply author+committer, isolatedworktrees, normalmain
push onlywhenreviewed, noforce/rebaseothers. No live desktop/configs/units/
installedNacre/packages/tags/releases withoutowner; preserveAIpermissions/paru.
IncidentCLOSED. Informationaladvisory0192 ratified; exact0370/71/72 exceptions
retained; vulnerability/unsound advice owneronly. No new Nacre protocol changes.

## B0013 Done — 2026-10-19

Reviewed sourcecdd9fb2, structurally preserved in normal integration021fdaad735f3b1db835d267c117e325018d333e.
Exact-head [CI39230443828](https://github.com/nacre-desktop/bead/actions/runs/39230443828)
completed SUCCESS09:37:56UTC: fmt, Clippy, tests, feature/config/schema, capture
regressions, private lifecycle and dependency/license checks. No local full suite,
measurement, package, live enablement or producer availability claim. Source and
queue integrated normally; no force. B2/B3 remain explicitly unscheduled afterF4.

## Next light product unit

B0014 is source-eligible against published5f4 `bluetooth.set_power` contract and
B0013 CallTicket machinery. Exactly one adapter only; multiple adapters remain
read-only. No producer/pin/lab/measurement change or hosted CI until root grants
heavy slot. Nacre logging-only diagnostic10:10–11:05UTC has exclusive lab/heavy
ownership; Bead may read/edit/commit source in a separate worktree only.

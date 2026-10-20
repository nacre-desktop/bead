# Bead implementation queue

Current owner handoff: morning2026-10-19. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Eligible work |
| --- | --- | --- | --- |
| 0 | [B0014 Bluetooth single-adapter power](0014-bluetooth-single-adapter-power.md) | Done | Sourcedcf541a9 integrated normally; exact-source full hostedCI39240289443 SUCCESS. Capability-gated; no producer or live enablement claim. |
| 0 | [B0013 Wi-Fi radio switch](0013-network-wifi-radio-switch.md) | Done | Sourcecdd9fb2 integrated021fdaad; exact-headCI39230443828 SUCCESS. Capability-gated, no live producer/enablement claim. |
| 1 | [B0012 light measurement prep](0012-measurement-readiness-light-prep.md) | Done: light prep accepted | Report14b93337 accepted; command routing and geometry/scope gaps recorded. No runtime/tool changes or captures. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Waiting for slot after F4 | Desktop wayland-utils installed by owner and command availability verified; prerequisite resolved. Preserve frozen runtime1cff9d1/helper5f4; no slot. |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Source Done; measured gated | B0003-S89afb381 accepted/CI39123319816 green. Measured work after B2 PASS/new slot; no static polishing. |
| — | [B0011 Battery](0011-daemon-battery-status-popout.md) | Done | Sourceaf61d18/fullCI39147491406green; docs5ad2551. Lazy lease/optional readings/private lifecycle tests accepted. |
| — | [B0010 Bluetooth](0010-daemon-bluetooth-status-popout.md) | Done | Sourcee3e417e/CI39136586458green. Capability-gated; no running producer claim. |
| — | [B0009 Network](0009-daemon-network-status-popout.md) | Done | Source5d96384/docsbd7bf56/CI39134388246green. Capability-gated. |
| — | B0001/B0004/B0005/B0006/B0007/B0008 | Done | Prior accepted reports retained; no duplicate work. |

## Current execution ownership

B0014 reviewed source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d` is now on
main; exact-source full hosted CI39240289443 SUCCESS. All six acceptance groups,
additional retirement/boot cases and factory regression pass. Duplicate automatic
main CI39241346554 was canceled by lead instruction; no second full result is
claimed. Hosted slot released; no private host or lab used for B0014. Completion
receipt is documentation-only with CI skipped. Earlier ownership below is history.

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

## B0014 Done — 2026-10-19

The fresh sole-adapter power switch uses published5f4 `bluetooth.set_power` v1,
shared CallTicket retirement and authoritative topic state. Zero/multiple/invalid
or stale adapters omit the switch; unavailable method keeps a healthy sole
adapter read-only. Full hosted [CI39240289443](https://github.com/nacre-desktop/bead/actions/runs/39240289443)
passed on exact main source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d`.
No protocol/pin/producer, lab, install or measurement change. B2/B3 remain gated
on their explicit grants; no next device-control unit is assigned here.

## Owner scheduling update — 2026-10-19 afternoon

F4 standard five-pair/compositor acceptance is next after N9/N10 private checks.
Bead light source/review/preparation may continue; no hosted heavy CI, full suite,
release build, nested session or capture during exclusive N2. Desktop available
now; lead announces actual capture start/release rather than waiting for night.
Wayland-utils is installed; no missing-tool blocker is inferred.

B0015 Review: source dc8832fe16db0db39185f342a2ae9a82d62a9924 is integrated on
main. It adds a dedicated read-only core Sound view over the published AudioState,
lazy audio-topic lease, bounded typed initial state, and no-client unavailable
rendering. Legacy preview route and controls remain unchanged. No new provider,
producer-availability, device-control, or memory-measurement claim. See
[spec](0015-daemon-audio-readback-popout.md).

Exact-source full hosted [CI 39388972154](https://github.com/nacre-desktop/bead/actions/runs/39388972154)
passed on dc8832fe: formatting, Clippy, workspace tests, renderer configurations,
facade/headless UI/Luau schema, capture-window regressions, private service
lifecycle, and dependency/license checks. Coverage includes explicit default
output selection, partial/boosted/unknown readback, invalid snapshots,
disconnect recovery, legacy preview preservation, lazy lease/unadvertised topic,
and the unavailable read-only factory. The published-main push triggered CI
39390038309 on the same head; it was still running when this note was written.
No desktop capture, local full build/test, or producer availability was used to
claim acceptance. Lead review accepted source; mark Done only after final
published-head CI is green.

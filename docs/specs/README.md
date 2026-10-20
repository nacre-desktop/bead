# Bead implementation queue

Current owner handoff: 2026-10-20 lead transfer. F4/N9 are green. The owner
requested finishing only the active B2 arm, releasing the desktop and stopping
all workers for the new lead. The transfer receipt below supersedes earlier
single-arm stop rules, grants and results. No automatic capture continuation.
[Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Eligible work |
| --- | --- | --- | --- |
| 0 | [B0014 Bluetooth single-adapter power](0014-bluetooth-single-adapter-power.md) | Done | Sourcedcf541a9 integrated normally; exact-source full hostedCI39240289443 SUCCESS. Capability-gated; no producer or live enablement claim. |
| 0 | [B0013 Wi-Fi radio switch](0013-network-wifi-radio-switch.md) | Done | Sourcecdd9fb2 integrated021fdaad; exact-headCI39230443828 SUCCESS. Capability-gated, no live producer/enablement claim. |
| 1 | [B0012 light measurement prep](0012-measurement-readiness-light-prep.md) | Done: light prep accepted | Report14b93337 accepted; command routing and geometry/scope gaps recorded. No runtime/tool changes or captures. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Paused: owner lead transfer, incomplete fresh batch | Fresh local median-mode harness e66a747d/f8f504fd, 49 mocks pass; frozen UI/helper unchanged. Partial opened-once anonymous increase remains provisional. See transfer receipt; no batch PASS, no automatic resume. |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Source Done; measured gated | B0003-S89afb381 accepted/CI39123319816 green. Measured work after B2 PASS/new slot; no static polishing. |
| — | [B0015 Audio](0015-daemon-audio-readback-popout.md) | Done | Source dc8832fe / CI39388972154 green; docs e5fc3d88 / CI39390219914 green. Read-only capability-gated view; no producer or live claim. |
| — | [B0011 Battery](0011-daemon-battery-status-popout.md) | Done | Sourceaf61d18/fullCI39147491406green; docs5ad2551. Lazy lease/optional readings/private lifecycle tests accepted. |
| — | [B0010 Bluetooth](0010-daemon-bluetooth-status-popout.md) | Done | Sourcee3e417e/CI39136586458green. Capability-gated; no running producer claim. |
| — | [B0009 Network](0009-daemon-network-status-popout.md) | Done | Source5d96384/docsbd7bf56/CI39134388246green. Capability-gated. |
| — | B0001/B0004/B0005/B0006/B0007/B0008 | Done | Prior accepted reports retained; no duplicate work. |

## New lead — 2026-10-20 evening

Claude Code (Opus) is now lead, with Codex workers. Owner direction for B2: test
the hypothesis that Luau's incremental GC never finishes collecting after the
first close in an idle shell. The fix runs one full GC cycle after a panel
releases and the UI is idle, adding malloc_trim only if evidence shows it helps.
Phase A, assigned to a Luna worker with the desktop heavy slot, is an
opened-once-only comparison on Hyprland: control (frozen `b01dfcca`) against the
fixed build, 3 alternating arms each, with unchanged capture gates. If the fixed
median is under 25 MiB and the anonymous increase is under 1 MiB, the full B2
batch runs on both compositors and is decided on medians under the transfer
rule. If the fix changes nothing, B2 stops and the result is reported; there are
no speculative fixes. B3 follows when the slot is free.

### Lead update — 2026-10-20 night

- The B2 GC fix is implemented: UI `7948c3c` runs one full Luau collection on
  the first idle notification after a release. 37 bead-ui and 113 nacre-shell
  tests pass, along with clippy and 49 harness mocks. The release build is
  `306474f0`, and the harness opened-once mode with its research note is at
  `cb79930`. Both are local only.
- Provenance: the frozen control artifact `b01dfcca` no longer exists on the
  host, and source `1cff9d17` does not rebuild bit-identically. The rebuild is
  `e21caa16`. Lead decision for phase A: compare a same-toolchain rebuild of
  `1cff9d17` (control) against `7948c3c` (fixed), with the common helper
  `4a65f4ec`, which was recovered and rehashed.
- Captures are blocked on an unattended swap reset. Host swap is about
  6.7 GiB, and the standing rule needs it below 64 MiB. Processes started by
  the new lead run with no-new-privileges, so the reset is launched through
  the user manager, which also needs a narrow NOPASSWD sudo rule for
  `swapoff -a` / `swapon -a`. The owner has been asked for it.

## Lead-transfer receipt — 2026-10-20

### B2 decision and preserved source

Owner supersedes the single-arm24.5 stop for this fresh batch: decide each phase
from five matched alternating pairs per compositor. Candidate median below25MiB
with explained category changes passes; median25–27 only when the extra is
file-backed may use the one-time27 allowance; above27 or unexplained anonymous
increase above1MiB stops. Do not classify graphics as file-backed or unknown DRM
as zero. Quiet/swap, geometry, containment/watchdog and close bar+5 rules remain.

- Frozen UI source `1cff9d17d61fb05451546bb0ea13c826c93cb4c9`, release binary
  SHA256 `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b`;
  common helper source5f4 and SHA256
  `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`.
  No UI rebuild or feature-source substitution in this batch.
- Local source worktree `bead-b0002-failure-report-20261020`: commits
  `e66a747db0cd797ecdbf3e1c93d5cc56ec9ab90f` and
  `f8f504fde80c9c18fb0ca955780c562987e29b25`, not pushed. Final clean HEAD is
  `52d0df09ad1d7ef94d93475df454ef9160da90ec` on branch
  `b0002-capture-failure-report-20261020`; the two further local commits
  `cb1fca8` and `52d0df0` add then revert the unsuccessful Nice-property change.
  Preserve all four unpublished commits; do not blindly replay the reverted fix. These add explicit
  `BEAD_DASHBOARD_BATCH_MEDIANS=1`, paired category reducer and its collector,
  preserving strict defaults. Worker reports49 mocks/py_compile/diff-check pass;
  lead independently ran48 at the preceding revision. No hosted CI is claimed
  for these local changes; prior harness c807591 CI39410701291 is historical.
- Fresh batch begins15:22:31UTC after a pre-measurement failed priority check.
  Duplicate additive `nice` caused priority19; corrected controller ancestry and
  no-measure smoke proved priority10 plus12GiB/swap0 before the fresh start.
  Initial failure raw remains separate; it is not an accepted measurement arm.
- Initial raw endpoints from the old A1/B1 batch remain in the
  [earlier report](../research/b0002-hyprland-capture-20261020.md), not reused.
  New partial data below are a distinct fresh run, not a completed comparison.

### Partial Hyprland results, MiB

All listed completed arms have full endpoint samples; the worker reports quiet
and safety verification, SwapPss0 and zero host swap-in. DRM is absent/unknown;
reported PSS-plus-DRM field must not be described as measured graphics memory.

| Arm | Never opened | Opened once | Opened several | Combined |
| --- | ---: | ---: | ---: | ---: |
| A1 | 24.095703 | 24.123047 | 24.155273 | 24.426758 |
| A2 | 24.267578 | 24.275391 | 24.283203 | 24.565430 |
| A3 | 23.780273 | 23.780273 | 23.784180 | 24.141602 |
| B1 | 24.107422 | 25.345703 | 23.916016 | 24.587891 |
| B2 | 23.970703 | 25.310547 | 24.001953 | 24.502930 |
| B3 | 24.257813 | 24.109375 | 24.171875 | 24.883789 |
| A4 | 24.493164 | 24.499023 | 24.497070 | 24.238281 |
| B4 | 24.433594 | 23.828125 | 24.007813 | 24.673828 |

Final **four-pair partial** opened-once medians are control24.19921875,
candidate24.70996094MiB (+0.51074219); anonymous5.25390625→6.279296875
(+1.025390625), file18.25048828→17.67480469 (−0.57568359), shmem0.671875 both.
This is below25 total but slightly above the anonymous delta rule. It is NOT the
required five-pair batch and has no final PASS, GC trigger or causal leak verdict.

**Final stop/release:** A4 finished17:37:36UTC. Eight arms were captured
(A1–A4/B1–B4); seven have controller `CELL_ACCEPTED`. A4 capture returned0,
quiet/safety passed and immutable-evidence postcheck passed; an initial `Path`
import typo in the administrative stop hook ended the controller after verify.
The import was corrected and verify rerun on the same raw data, printing PASS
then explicit `OWNER_HANDOFF_STOP` rc75. No recapture and no B5 start occurred.
Two Hyprland arms (A5/B5) plus ten Niri arms remain unrun.

Private REMOTE raw/controller root is
`~/nacre-quiet/evidence/bead-b0002-20261020/median-cb1fca8/`;
`controller.sh`, `controller-status.txt`, `controller.log` and `postcheck.py`
remain there. Final postcheck SHA256
`8e47b1b8ca6b69c8b150971bbcae58b0125403032acc044a71c5701134417a82`.
Full raw archive sibling `median-cb1fca8.hyprland.raw.tar`, SHA256
`dcc9ce92f3c5da8643bb0fe8bf16142b897d3eba2af0e85e7bc2feb9533a52de`
(458 entries checked). Do not mistake these for local mirrored paths.
Worker confirmed controller PID and child absent, no capture processes/scopes,
bench-lock free; final MemAvailable12,982,712KiB and host pswpin unchanged.
**Heavy slot RELEASED; all workers stopped.** No continuation is armed or held.
The raw evidence is preserved, not deleted or reconstructed.

### Next units and boundaries

1. The new lead must explicitly schedule any continuation/fresh batch; this
   lead's owner-transfer stop is not a measurement failure or a retry grant.
   Preserve original rows and report any administrative gap without inventing
   completed pairs. No B3 measurements before B2 passes.
2. Conditional owner-authorized GC attempt only if the full opened-once median
   confirms above25: target one full Luau cycle after release/idle, and trim only
   if relevant. Read-only source mapping found the panel Runtime/VM is destroyed,
   but the retained bar Runtime receives `set_idle(true)` via its owning worker
   and currently uses the no-op trait default. This is a possible hook, not proof
   of causality. `malloc_trim` already runs immediately and after100ms. Verify
   actual mlua API and ordering before coding; no GC change was made.
3. If targeted fix is justified, owner asks three control and three fixed
   opened-once Hyprland arms with identical quiet/swap rules. Only below25 and
   anonymous growth below1 proceeds to a full five-pair batch both compositors;
   no improvement means stop/report, no speculative fixes. Base candidate on
   frozen1cff, not unrelated newer shell work.
4. N10 has priority for desktop builds between batches. Its latest local source
   and remaining gates are in the Nacre queue. Hosted GitHub CI does not consume
   the desktop slot. No package/live action is authorized here.
5. Other Bead status: B13 Wi-Fi, B14 Bluetooth and B15 Audio are Done,
   capability-gated; no producer/live availability claim. B3 source stage is Done,
   measured stage remains gated. No new independent worker was started at stop.

## Earlier execution receipts (historical)

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
One heavy job on the desktop: local builds/tests, nested sessions or captures.
GitHub-hosted CI may run concurrently; desktop-hosted CI may not. Lead grants.
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

B0015 Done: source dc8832fe16db0db39185f342a2ae9a82d62a9924 is integrated on
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
and the unavailable read-only factory. Final published-head full hosted
[CI 39390219914](https://github.com/nacre-desktop/bead/actions/runs/39390219914)
passed on exact head e5fc3d8826493147c05ae1ecf59df294678a36c0, including this
review evidence. No desktop capture, local full build/test, or producer
availability was used to claim acceptance.

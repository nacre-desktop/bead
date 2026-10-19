# Bead implementation queue

Current owner handoff: morning2026-10-19. [Standing rules](https://github.com/nacre-desktop/nacre/blob/main/docs/architecture/standing-rules.md) apply.

| Order | Spec | Status | Eligible work |
| --- | --- | --- | --- |
| 0 | [B0013 Wi-Fi radio switch](0013-network-wifi-radio-switch.md) | Ready: source/private tests | ONE capability-gated daemon Network switch; published reserved5f4 schema only. Existing IO actor tickets; no producer/pin/measurement. Build/full-CI slot separately granted. |
| 1 | [B0012 light measurement prep](0012-measurement-readiness-light-prep.md) | Done: light prep accepted | Report14b93337 accepted; command routing and geometry/scope gaps recorded. No runtime/tool changes or captures. |
| 2 | [B0002 dashboard memory](0002-dashboard-lifecycle-memory-gate.md) | Waiting for slot after F4 | Owner installation notice retained, but explicit lab retry found executable/package absent; owner notified. Preserve frozen runtime1cff9d1/helper5f4; no slot. |
| 3 | [B0003 typing](0003-launcher-typing-investigation.md) | Source Done; measured gated | B0003-S89afb381 accepted/CI39123319816 green. Measured work after B2 PASS/new slot; no static polishing. |
| — | [B0011 Battery](0011-daemon-battery-status-popout.md) | Done | Sourceaf61d18/fullCI39147491406green; docs5ad2551. Lazy lease/optional readings/private lifecycle tests accepted. |
| — | [B0010 Bluetooth](0010-daemon-bluetooth-status-popout.md) | Done | Sourcee3e417e/CI39136586458green. Capability-gated; no running producer claim. |
| — | [B0009 Network](0009-daemon-network-status-popout.md) | Done | Source5d96384/docsbd7bf56/CI39134388246green. Capability-gated. |
| — | B0001/B0004/B0005/B0006/B0007/B0008 | Done | Prior accepted reports retained; no duplicate work. |

## Current execution ownership

No Bead capture or heavy job is active/granted. Owner ended the previous heavy
window and restarted LIGHT work immediately. Nacre v14 proved both required
screen profiles but only NiriA/B passed; Hbaseline failed hostswap gate. Nacre
now fixes QML memory before quick pairs/full confirmation. No B2/B3 measurement
until a separate explicit grant. Never infer grant from an idle lock/window.

Owner installed-tool notice and lab read-only result disagree: explicit PATH
lookup found /usr/bin/wayland-info absent and pacman reports no wayland-utils.
Owner notified of minimal lab install command; no install/sudo retry authorized.
B0012 is complete, no further host probes needed. Source work continues.
B0013 defines the next light source unit against the published reserved Wi-Fi
radio schema. Runtime control remains unavailable without actual Welcome method
capability and fresh topic data. No producer/pin change or measurement grant.
Other device controls/static wallpaper still require a concrete next spec.

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

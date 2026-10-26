# B0149: Smooth held-key OSD

Status: **In progress.** The volume and brightness pill should follow each held-key step without process-spawn lag or queued jumps. Nacre routes repeated keys through daemon-backed adjustment with existing-tool fallback. Bead displays the newest value immediately and animates the fill from its current position. A nested 2-second hold must show monotonic per-frame movement with no jump larger than one step.

| Measure | Before | Candidate | Acceptance |
| --- | --- | --- | --- |
| Key repeat → OSD value | pending desktop capture | pending | record per-step ms |
| Key step → presented frame | pending desktop capture | pending | ≤ 1 frame after daemon publish |
| 2 s hold | reported delayed ~30-point jumps | pending | monotonic; max jump ≤ 1 step |
| Fill behavior | immediate width change | 130 ms out-cubic retarget, matching QML | no queued steps; text immediate |

## Scope and proof

- Replace the Nacre volume key's per-repeat `sh` + `wpctl` spawn with a daemon step call; brightness uses the existing daemon adjustment route. Preserve fallback commands if daemon IPC fails.
- Add a rate-limited `OSD_STEP` journal marker with kind, value and ms since key input for live verification.
- Animate only the Bead OSD level fill; keep percent text synchronized to the latest published value.
- Add both-repo tests and capture repeated-key timing plus matched QML/Bead OSD states in a private nested session.
- Temporary duplicates: none.

## Evidence

Pending remote CI, nested capture, paired QML/Bead screenshots, and final measurements.

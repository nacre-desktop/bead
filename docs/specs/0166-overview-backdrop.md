# B0166 — Niri overview wallpaper backdrop

Niri only shares `place-within-backdrop` surfaces when they use the Background layer.
Bead's still wallpaper already uses that layer, while its motion renderer used Bottom.
That split made a video wallpaper appear inside every workspace in the native overview.
Use Background for the video surface too; preserve the still surface underneath it.

| Surface | Layer before | Layer after | Overview behavior |
| --- | --- | --- | --- |
| `bead-wallpaper` still image | Background | Background | Shared backdrop |
| `bead-wallpaper-video` motion image | Bottom | Background | Shared backdrop |
| Picker preview | Panel content | Panel content | No separate wallpaper surface |
| Dimmed backdrop | Existing background effect | Existing background effect | No separate wallpaper surface |

## Change and constraints

The motion renderer now creates its per-output surface on Background. Niri's layer
rule already matches both wallpaper namespaces and enables `place-within-backdrop`.
The renderer remains a separate process and does not change decoder lifecycle,
switch protocol, fade/poster behavior, or idle-release policy. Its surface is mapped
after the shell's still wallpaper, keeping the video above the still frame on the
same layer while the latter remains available underneath. No temporary duplicate
or new dependency is introduced.

## Verification

Acceptance: on nested niri with the unit's layer rule, static and video wallpaper
each appear once behind the workspace cards when overview is open; compare QML and
Bead in paired captures at the same output size, scale, wallpaper, and palette.
Also verify switching, the poster/fade transition, and return to idle do not expose
a blank frame or retain the decoder. Check any additional wallpaper-like surface
for its own layer surface before treating it as backdrop-related.

Evidence and exact commands are recorded below after desktop verification. CI uses
the pinned full Bead workflow recipe in `.lead/rules.txt`; no laptop builds or tests.

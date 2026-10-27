# B0166 — Niri overview wallpaper backdrop

Niri only shares `place-within-backdrop` surfaces when they use the Background layer.
Bead's still wallpaper already uses that layer, while its motion renderer used Bottom.
That split made a video wallpaper appear inside every workspace in the native overview.
Use Background for the video surface too; preserve the still surface underneath it.

| Surface | Layer before | Layer after | Overview behavior |
| --- | --- | --- | --- |
| `bead-wallpaper` still image | Background | Background | Shared backdrop |
| `bead-wallpaper-video` motion image | Bottom | Background on Niri; Bottom elsewhere | Shared backdrop on Niri; existing placement elsewhere |
| Picker preview | Panel content | Panel content | No separate wallpaper surface |
| Dimmed backdrop | Existing background effect | Existing background effect | No separate wallpaper surface |

## Change and constraints

The motion renderer selects Background when the compositor is Niri, using
`NACRE_COMPOSITOR` or the existing `NIRI_SOCKET` environment marker. Niri's layer
rule already matches both wallpaper namespaces and enables `place-within-backdrop`.
Other compositors retain Bottom, where moving video to Background puts it beneath
their static wallpaper surface (reproduced by the full CI smoke). The renderer
remains a separate process and does not change decoder lifecycle, switch protocol,
fade/poster behavior, or idle-release policy. No temporary duplicate or new
dependency is introduced.

## Verification

Acceptance: on nested niri with the unit's layer rule, static and video wallpaper
each appear once behind the workspace cards when overview is open; compare QML and
Bead in paired captures at the same output size, scale, wallpaper, and palette.
Also verify switching, the poster/fade transition, and return to idle do not expose
a blank frame or retain the decoder. Check any additional wallpaper-like surface
for its own layer surface before treating it as backdrop-related.

## Results

- Full Bead CI (`.github/workflows/ci.yml`, Ubuntu 24.04 recipe from B0156) passed
  on `cf8ea1cf`, including the renderer smoke and `cargo deny`.
- The desktop test host reports `libmpv is not installed` for the candidate renderer.
  The private Niri preview committed the still wallpaper surface, then exited with
  a Wayland broken pipe before producing an overview frame. Static/video overview
  captures and the paired QML comparison remain unverified; no screenshot evidence
  is claimed.
- No laptop builds or tests were run.

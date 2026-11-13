# 0270 — Panel first frame

Where a panel open spends the time between the click and the first presented frame, and the root
cause fixed here (the second, a GL transport per panel, is pending a desktop gate). Harness:
`bead run examples/bar-with-panels` under a private headless sway (two 120 Hz outputs, `BEAD_FRAME_TRACE=1 BEAD_ACCEPTANCE_PANEL_LATENCY=1`,
`BEAD_PROFILE_INPUT=1` for stage marks); the status button opens a 488x332 surface whose 120 px body
reveals with a shadow, the calendar a 260x128 one.

## Stages (release build, dev-2 container, CPU panels, ms after the click)
| Stage | Cold status, before | after |
|---|---:|---:|
| worker thread + Luau VM + build + layout 3.8 → bake done (shadow raster dominates) | 10.6 | 6.9 |
| first commit (UI `prepare` 2.2 → 0.6, paint 0.4) | 14.3 | 8.9 |
| first frame presented (`PANEL_OPEN_LATENCY`) | 26-34 | 14.3 |
| reopen: first commit after its configure / presented; calendar cold / reopen | 0.2 / 29-32; 12.9 / 14.0 | 0.5 / 16; 13.0 / 14.5 |
| open-animation frame gap max (120 Hz; target <= 12) | 16 (reopen 23) | 8.5 |

Configures: sway configures every layer surface on the output whenever any of them commits a state
change (initial commit, dismiss layer, keyboard interactivity): 3 before the first frame, no wait.

## Root causes and fixes
1. `shadow_pixmap` (bead-render) drew a shadow whose width or height was below the nine-patch
   template side (2 x (radius + 6 x blur) x scale, 230 px at 1.5x) as a full-size SVG Gaussian with a
   200 % filter region. A revealing body re-shapes its shadow every frame, so each of the first ~10
   frames of a status open cost 2.2-10.7 ms of `reveal_art_raster` on the UI thread (the owner's
   dropped frames), and the worker's first bake paid it too. Now each axis is independently a
   corner/stretch/corner nine-patch or, when shorter than the template, rendered at its own extent,
   from a template cached per (radius, blur, scale, short axis); the filter region is the SVG itself (`filterUnits=userSpaceOnUse`; a bbox-relative 100 % would clip the blur to the rect).
   Thin shadows stay within 2/255 of the exact Gaussian (test `thin_shadows_stretch_the_narrow_...`).
2. Pending a desktop gate (branch `claude/b0270-gl-transport`, not landed): the EGL display, context
   and femtovg canvas (compiled shaders) are per panel, so a non-retained panel (every Nacre
   dashboard/popout) pays `eglInitialize` + context + shader compile + re-upload on its second frame
   (the 41-108 ms gap in the Platform compositor traces). Keeping them for the process fixes it but
   costs about 30 MiB idle on llvmpipe; the real-driver cost decides.

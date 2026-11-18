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

## Retained GL open (B0334)
A `retain_when_closed` panel on GL closes without a CPU snapshot: 1.5 s after the close its layer textures, EGL surface and SHM go, while its `GlRenderer` (`TreeCache`, femtovg canvas with compiled shaders, image-payload textures) and the shared transport stay (`PANEL_GPU_SUSPENDED shared_context_retained=true kept_renderer=true snapshot=false`; `has_gl_surfaces` counts the kept renderer, so warm-off, the warm prime and the release timers keep the context). A reopen makes only the EGL surface, uploads the layer rasters from `TreeCache` (no worker decode) and presents its first frame on GL in one draw, also while its motion runs; a budget refusal on any GL draw of a companion (B0337; B0334 caught only that first upload) takes the gpu-to-cpu fallback like a GL failure instead of ending the shell, and the CPU snapshot stays for CPU companions. A still motion (`slide_distance = 0`, no reveal, fade or `scale_from`) is drawn in client frames and never plans a layer-motion atlas. Split marks `handoff` and `layer_motion` time those commits.
Idle-footprint rule: keep the context while closed if it costs at most ~10 MiB (drm-total/drm-resident plus RSS on the real driver), else release it after a 5-minute grace. dev-1 llvmpipe (headless sway 1920x1080, 10 images, 3 reopens; relative only): no `first_frame=cpu`, no promotion, no transport prewarm (was 25-27 ms per reopen) or CPU snapshot (was 191-199 ms per close), no shader compile, first frame 11.6-13.6 ms (layer re-upload ~4.5 ms, first swap 6.7-9.4 ms; was 2.2-2.3 ms from the CPU snapshot, and 157-162 ms while image textures were released, waiting for the worker to decode them again), 29-30 frames with max gap 17-18 ms (was 18 with a 287-291 ms hole). Closed RSS medians +35.7 MiB VmRSS, +38.3 MiB anon on llvmpipe: the context plus the kept `TreeCache` and 2.4 MiB of image textures (`GL_CACHE_RELEASE kept_image_bytes=`), minus the snapshot.

# B0099 — Video wallpaper at the QML's cost (GPU decode + zero-copy presentation)

Status: **Review.** `bead-wallpaper-video` presents through EGL and libmpv's OpenGL render API when the host has
a GPU, so VA-API frames are drawn from dmabufs without touching the CPU; the shared-memory software path stays as
the automatic fallback. Measured on the owner's desktop against the QML shell with the same clip and output
(below). Builds on B0046 (renderer, policy, supervisor); ADR 0005 (separate renderer process) is unchanged.

## Why

N0069 measured, at 2880x1800 / scale 1.5 on the desktop: Bead's renderer 51 % of a core while playing, QML's
wallpaper 5.5 %, "libmpv software presentation; hardware decode unknown". B0046 had chosen mpv's *software* render
API with copy-back hardware decode (`hwdec=vaapi-copy`) and recorded the zero-copy path as a follow-up. On a laptop
battery that is the wrong trade.

## What the QML does

`NacreDesktopVideo.qml`: Qt Multimedia `MediaPlayer` + `VideoOutput` (FFmpeg backend). Qt picks a hardware
decoder itself (VA-API on Intel/AMD, CUDA on NVIDIA; the desktop log shows `Selected format cuda`) and hands the
decoded GPU surface to the scene graph, which scales and crops it on the GPU (`PreserveAspectCrop`). There is no
frame-rate cap and no downscale: the frame is drawn at the window's native size. `loops: Infinite`; pause is
`player.pause()` when `running` goes false (covered, locked, battery rule) and the last frame stays.

## What Bead did (B0046) and where the cost was

mpv software render API: libva decodes to a GPU surface, mpv **reads it back** into system memory, `swscale`
converts and scales it to a 2880x1800 `bgr0` buffer on the CPU (bilinear), the buffer is copied to the compositor
through `wl_shm`, and the compositor uploads it again. For a 4K clip that is a full-frame readback, a CPU
scale/convert and a 20 MiB shm frame, 30 times a second. Even with working VA-API this is 100 %+ of a core on the
desktop (table).

## Design (this unit)

- **Presentation through EGL.** `gl.rs`: one EGL display and context (glutin, EGL platform = Wayland, already in
  the tree) and one window surface per output on the existing layer surface. libmpv's OpenGL render API
  (`vo=libmpv`, `MPV_RENDER_PARAM_OPENGL_FBO` fbo 0, flip-y) draws the frame into it; `eglSwapBuffers` attaches the
  dmabuf and commits. mpv scales/crops on the GPU with the same `panscan` settings as before.
- **Zero-copy decode.** With the GL context `hwdec=vaapi` (`auto-safe` for `--hwdec=auto`) imports the VA surface
  as an EGL image, no copy-back. Failures inside mpv still fall back to software decode on their own; the frame
  is then uploaded to the GPU for scaling, which is still much cheaper than the shm path.
- **Automatic fallback to the old path (shared memory, `vaapi-copy`)** when any of these hold: `--render=shm`; no
  `/dev/dri/renderD*` (also avoids loading Mesa's software rasteriser: 10.8 MiB released footprint in the CI
  container, unchanged from B0046); libEGL/Mesa cannot make a context; the GL renderer is a software one
  (`llvmpipe`, `softpipe`); libmpv cannot create a GL render context. The decision is made once, logged on stderr
  (`bead-wallpaper-video: EGL presentation, GL_RENDERER=...` or `shared-memory presentation (reason)`), and never
  changes while the process lives. Nested/CI sessions without a GPU therefore keep the old behaviour untouched.
- **Everything else is unchanged:** surfaces and layers, viewport scaling (buffer at the output's native mode, the
  compositor maps it to the logical size), frame-callback pacing and the 1.5 s starvation watchdog, pause/release
  semantics, last frame kept, protocol, supervisor, policy. On *release* the decoder and libmpv are dropped as
  before; in the GPU path the EGL context and window surfaces stay (they hold the last frame), see the memory
  column.
- **Options:** `--render=auto|shm` (new), `--vaapi-device=/dev/dri/renderDNNN` (new; mpv `vaapi-device`, for
  multi-GPU hosts), `--hwdec=vaapi|auto|no` (as before; the copy-back/zero-copy variant follows the render path).
  `BEAD_WALLPAPER_MPV_LOG=<file>` writes a verbose libmpv log (hwdec selection and fallbacks) for diagnostics.
  The `hwdec` event is now sent after the first rendered frame (mpv reports the real decoder only then; before it
  said nothing or `no`).
- **Product-neutral:** nothing Nacre-specific was added; the protocol crate (`bead-wallpaper`) is untouched.

## Measurements (owner's desktop, private nested session)

Harness: `spikes/wallpaper-video-bench.py` (headless labwc parent, nested niri 2880x1800 at scale 1.5 = 1920x1200
logical, one wallpaper process per run, same ffmpeg-made clips for both arms; phases = playing, covered (decoder
paused, as the policy does; QML `running=false`), resumed; CPU = utime+stime delta of the wallpaper process over
the phase window). Clips: `testsrc2` H.264 4K30 (18 Mbit/s) and 1080p30 (8 Mbit/s), made on the host.
Desktop: AMD Ryzen 5 7600 (Radeon 610M iGPU, renderD129) + NVIDIA RTX 4070 (renderD128), Mesa 26.2.4, libmpv 0.41
(private copy, see N0069), niri 26.04, labwc.

One run per row (10-12 s windows after an 8 s warm-up; the lock queue on the desktop did not allow repeats, the
differences below are 20x-50x, far above run-to-run noise seen between the duplicate rows). "CPU" is percent of
one core of the wallpaper process (QML: the whole `quickshell` process). Memory is PSS of that process while
playing (anon / file-backed / shmem, MiB). Backend column = presentation + decode.

**4K30 clip, 2880x1800 at scale 1.5**

| Arm | Presentation / decode | Playing CPU | Covered | Resumed | PSS (anon / file / shm) | nested niri CPU |
| --- | --- | --- | --- | --- | --- | --- |
| QML (target), Qt default | scene graph GPU, CUDA/NVDEC (NVIDIA) | 6.9 % | 0.0 % | 6.8 % | 648 (408 / 233 / 8) | 1.3 % |
| QML, Qt forced to VA-API | scene graph GPU, VA-API (iGPU) | 45.7 % | 0.0 % | 43.3 % | 663 (560 / 103 / 0) | 1.4 % |
| Bead **before** (B0046), default devices | wl_shm, vaapi-copy asked | 93.9 % | 0.1 % | 95.3 % | 435 (221 / 186 / 28) | 4.5 % |
| Bead **before**, iGPU VA-API pinned | wl_shm, hardware decode + readback | 117-120 % | 0.0 % | 116-120 % | 264 (202 / 42 / 20) | 4.1 % |
| Bead **after**, default flags | EGL dmabuf, GPU render path | 2.0 % | 0.0 % | 2.2 % | 202 (140 / 62 / 0) | 1.1 % |
| Bead **after**, iGPU VA-API pinned | EGL dmabuf, zero-copy VA-API | 2.1 % | 0.0 % | 1.9 % | 202 (141 / 62 / 0) | 1.1 % |

**1080p30 clip, same output**

| Arm | Presentation / decode | Playing CPU | Covered | Resumed | PSS (anon / file / shm) | nested niri CPU |
| --- | --- | --- | --- | --- | --- | --- |
| QML, Qt forced to VA-API | scene graph GPU, VA-API (iGPU) | 15.5 % | 0.0 % | 14.9 % | 386 (283 / 103 / 0) | 1.4 % |
| Bead before = new binary with `--render=shm` (the unchanged B0046 path), iGPU VA-API | wl_shm, hardware decode + readback | 53.4 % | 0.0 % | 54.2 % | 135 (73 / 42 / 20) | 4.1 % |
| same, `--hwdec=no` | wl_shm, software decode | 57.8 % | 0.0 % | 57.6 % | 126 (73 / 33 / 20) | 3.4 % |
| Bead **after**, iGPU VA-API | EGL dmabuf, zero-copy VA-API | 2.0 % | 0.0 % | 1.9 % | 139 (77 / 62 / 0) | 1.0 % |

Result: the target (within 2x of QML's CPU, i.e. <= 13.8 % at 4K against QML's best 6.9 %) is met by a wide margin:
Bead is now at 2 % playing, 0 % covered, 2 % after resume, about a third of the QML's cost on this desktop and
about 50x cheaper than before. The old shm path was never close because the cost is the readback plus the CPU
scale/convert, not the decode: even with hardware decode it needs more than one core at 4K. The fallback path
is unchanged and, as measured, still costs what it did.

**Released** (decoder dropped after a long pause, last frame kept; second desktop run, 4K clip): GPU path 0.0 % CPU,
PSS 66.5-69.8 MiB (anon 31-34, file 36), against 10.8 MiB for the shm path released in the CI container. This is
the EGL context and window surfaces that stay alive; renderer memory is reported separately from the UI gate.
A second run reproduced playing CPU of 1.3-2.1 % (default flags 2.1 %, iGPU pinned 1.3 %), covered 0 %, resumed
1.2-1.9 %, and the shm fallback 117.9 % / 122.3 %. QML at 1080p with Qt defaults (CUDA) was 3.8 % playing,
PSS 451 MiB. On this desktop a default launch (no flags) already lands on the iGPU: `EGL presentation,
GL_RENDERER=AMD Ryzen 5 7600 ... radeonsi`, `hwdec` event `vaapi`, mpv `Using hardware decoding (vaapi)`.

Backend of every Bead "after" row: render backend = GPU (EGL + OpenGL, Mesa radeonsi), no CPU rasteriser; the
fallback rows are CPU (libmpv software render, wl_shm). The `hwdec` event of the GPU runs reported `vaapi`;
the mpv log of the pinned run shows `Using hardware decoding (vaapi)` with `Using EGL dmabuf interop`.


## Desktop GPU versus the laptop

The desktop is not the laptop: AMD iGPU + NVIDIA dGPU, VA-API through `radeonsi` (and `nvidia-vaapi` for the
dGPU), whereas the laptop is Intel Lunar Lake (`xe`, `iHD`). What carries over: the render path (EGL window surface
+ mpv OpenGL + dmabuf import) is vendor-neutral Mesa/EGL, and the cost split (CPU only demuxes, parses and
submits) is the same on any VA-API GPU. What does not: the NVIDIA node is renderD128 here, so a default launch
(no flags) decodes through whatever the compositor's EGL device is; the numbers above pin the iGPU for both
arms. On the laptop there is one GPU and the compositor, EGL and VA-API all use it, which is the easy case. The
laptop itself was not measured (rule: no tests on the laptop). The lead should run the same bench there once:
`wallpaper-video-bench.py --arm bead ...` and `--arm qml ...` inside a private session.

## Tests

- `bead-wallpaper-video` unit tests: argument parsing (defaults, `--render`, `--hwdec`, `--vaapi-device`, refused
  arguments) and the hwdec-value selection per render path (`vaapi` vs `vaapi-copy`, `auto-safe` vs `auto-copy`).
- `spikes/wallpaper-video-smoke.py` (CI step) now runs with `--expect-presentation shm`: under the headless
  pixman sway there is no GPU, so the run proves the automatic fallback end to end (play, pause, release, resume,
  bad source, crash) and prints which path was used. It accepts `--engine-arg --render=shm` and
  `--expect-presentation egl` for GPU hosts.
- The GPU path itself is only testable with a real render node, so it is covered by the desktop bench, not CI.

## Temporary duplicates

None. The shared-memory path is not a legacy copy: it is the permanent fallback for hosts without a GPU
(headless CI, VMs, broken drivers).

## Known gaps / follow-ups

- Laptop (Intel xe) not measured by this unit, see above.
- GPU path keeps the EGL context while the decoder is released (about 66-70 MiB PSS vs 10.8 MiB for shm);
  tearing it down on release and re-creating on resume is possible but was not needed for the cost target.
- Fractional-scale-exact buffers are still the output's native mode scaled by the compositor (B0046).
- `wp_presentation` feedback is not used; pacing remains frame callbacks.

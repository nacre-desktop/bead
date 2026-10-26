# B0142 — Warm wallpaper video switching

Status: implementation candidate; remote CI and desktop gates in progress.

The motion fence returns no source while each output commits its matching poster
and palette. Previously this killed the video process, so the next source paid
for process, EGL and libmpv startup again. The renderer now stays alive after its
first video, stops decoding and detaches its video surface during a static/poster
interval, then reuses its player/context through `loadfile replace`.

Every source replacement detaches the old video so it cannot cover the new
poster. A fresh empty layer commit/configure precedes remapping. Source-generation
checks reject old frame callbacks. The existing identity-checked cached PNG
poster/predecode path and palette transaction remain authoritative; no duplicate
poster cache or decoder is added. The first-ever video still starts lazily.

Poster blends into motion over 120 ms. Alpha-modifier-v1 is preferred; native GL
also supports an alpha-capable window and one cached shader/VAO which scales the
existing framebuffer in place. No intermediate texture/FBO or CPU image is added.
GL state is restored after the pass. Shared-memory presentation without the alpha
global retains clean replacement (known fallback gap). VA-API explicitly selects
its libmpv interop to avoid CUDA probing: see the
[mpv manual](https://mpv.io/manual/master/#options-gpu-hwdec-interop).

`WALLPAPER_SWITCH_LATENCY` is always enabled for picker requests. It measures
selection admission through receipt of compositor presentation feedback, per
output/revision, in milliseconds. `path` distinguishes all four static/video
switch directions; video endpoints are explicitly labelled `video-poster`. This is a
conservative feedback-delivery time, not physical click/scanout timing or time to
motion. No presentation feedback means no claimed visibility. The generic
`WallpaperProvider::presented` callback is the only new framework primitive.

`WALLPAPER_BUFFER_RETAINED reason=no_release` is a bounded release-poll diagnostic,
not a switch fence: after 3.1 s polling stops and active SHM is retained safely.
It does not wait before attaching a replacement. No unsafe buffer reuse is added.
The supplied rotation warning is the daemon supervisor reporting a returned
service error. Current producer registers `wallpaper-rotation`; its session watcher
can fail/exit, which disables rotation readiness and enters supervised retry.
The generic warning discards the underlying error. The supplied excerpt cannot
identify that error; private sessions without logind also exercise this path.
It does not gate wallpaper activation. No daemon behaviour is changed.

Memory: only one renderer/player is retained. UI bookkeeping is bounded by output
count (budget <=1 MiB). Warm player/EGL/driver memory is an explicit smoothness
trade, measured separately from the UI; acceptance requires matched control pairs.
No live deployment or laptop-cap acceptance is authorized or claimed.

## Validation

An early diagnostic accidentally used an older helper without palette prewarming.
It measured 0.54–1.07 s selection→feedback; a private helper-call trace attributed
most of the wait to `wallpaper-media select`. Those runs are excluded from gates.
Final captures use the published/pinned producer `6abe2498`, binary SHA-256
`3bb410c8e098a7f7a1363a53024484f0f3282fe714ff1ca0dd78e589bfa436ea`,
for both control and candidate. The source is the same producer as B0134.


The expanded 108×108 shared-memory/libmpv software smoke passes every check:
video replacement, static reveal, warm reload, release/resume and failure recovery.
First motion callbacks are 15–53 ms in that tiny synthetic fixture; this is not a
desktop or hardware-decoding latency claim. Colour checks wait for fade completion
while first-frame timing includes its start. Arch CI needs `swaybg` and Sway's
container-incompatible file capability removed inside the disposable container.

Pending: full remote Arch CI, private desktop switch/visibility/memory gates and
paired QML states. Required p95 <=150 ms for static/video switch types is not yet
claimed; a cached poster does not imply motion is ready within that interval.

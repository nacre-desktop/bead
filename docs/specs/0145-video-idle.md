# B0145 — Release idle wallpaper video renderer

When all active wallpaper outputs are static, the B0142 renderer may remain
alive for a short grace period so quick wallpaper changes preserve the fast path.
After 60 seconds, release its child process and graphics resources. A video source
or an explicit picker warm hold cancels the deadline; a later video starts lazily.

The timeout is a fixed default, not a new setting. The renderer continues to own
its child lifecycle through `bead-wallpaper::Supervisor`; no process-name lookup
or external kill is used. `WALLPAPER_VIDEO_RENDERER` records warm/released
transitions and the reason. The generic supervisor exposes a picker hold for the
shell integration to assert while the wallpaper picker is open.

## Acceptance

- Unit coverage: static idle releases, active video stays warm, picker hold warms
  and retains the renderer, and a quick video toggle cancels idle release.
- Nested desktop check: after the grace on static wallpaper the child is gone;
  selecting video afterwards shows its poster within 150 ms.
- Full Bead desktop CI job on the first available remote CI host.

## Results

Supervisor unit tests cover idle release, quick-toggle cancellation and picker
warm hold. Full `.github/workflows/ci.yml` passes in the isolated remote Ubuntu
container, including workspace tests, renderer smoke and cargo-deny.

Private nested desktop labwc check: after 60 seconds on a static wallpaper the
renderer PID was gone and the `released reason=static_idle` marker was present.
After switching to video, the poster capture completed in 63.8 ms; its center
pixel matched the static poster before and after (`[32, 160, 220]`). A new renderer
PID started lazily. The desktop does not have libmpv, so motion decoding and its
first frame were unavailable; this check measures poster visibility and idle
release only. No installed shell or desktop configuration was changed.

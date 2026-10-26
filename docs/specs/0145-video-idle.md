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

## Implementation results

Pending validation.

# B0217 — shared Bead scroll speed

Bead now uses one logical-pixel axis conversion for every scroll surface.
Touchpad finger deltas reach content unchanged, including the first event.
Wheel input uses a shared 54 px notch token (three nominal 18 px lines at scale 1).
Per-SceneSource wheel overrides were removed so windows, overlays and panels cannot tune it independently.

| Gesture | Before | After |
| --- | ---: | ---: |
| Touchpad axis event of N px | N px | N px |
| Wheel value120=120 / discrete notch | 100 px | 54 px |
| High-resolution value120=30 | 25 px | 13.5 px |

`value120` keeps fractional notches; legacy wheel axes still normalize their 15-unit notch to 54 px.
Wheel glide remains over 125 ms. Finger input remains immediate; only release inertia follows it.
The B0208 100 px value was introduced to match Chrome's desktop wheel distance, then supplied through a
shared Bead store. B0217 replaces that policy with the owner's uniform 54 px token.
The separate `extras.next` / `extras.prev` selection actions and their row-pitch scrolling are keyboard actions,
not pointer scroll views, and retain their one-row behavior.

All steps of `.github/workflows/ci.yml` passed in the required Ubuntu 24.04 container.
The run continued after clearing rerun-only Luau scratch; no product checks were skipped.
Formatting, clippy, workspace/trace tests, renderer configurations, Luau, proofs, packaging,
wallpaper smoke, private services and every independent license graph passed.
The tested code tree was verified against the task commit. No local tests or builds ran.
The test suite covers 1:1 finger input, full and fractional `value120`, stale wheel-unit precedence,
discrete notches and legacy wheel input. Product scene tests exercise exact 23.5 px and 54 px deltas in
launcher, Widgets, Settings, chat and notification history view stores.
Live desktop comparison remains unavailable; the desktop test host timed out.

The bundled Settings registry includes N0105 `scrollSpeed` (0.25–2.0, default 0.75),
so the same control is available in both frontends. Niri applies it to touchpad and mouse.

N0105 changes niri's touchpad factor from 0.25 to 0.75: a base 100 px gesture reaches
Bead as 25 px before and 75 px after (3×). Mouse previously used niri's 1.0 default;
sharing 0.75 reduces its compositor multiplier by 25%. Thus a physical wheel detent
scaled to value120=90 produces 40.5 px in Bead, versus the previous 100 px at value120=120.
These are configured distances, not physical measurements; application wheel policies vary.

References: [Wayland axis and value120](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_pointer),
[niri input configuration](https://github.com/niri-wm/niri/wiki/Configuration%3A-Input),
[kitty scrolling options](https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.wheel_scroll_multiplier).
Kitty's minimum-lines option does not establish a universal three-line default; 54 px is the requested Bead policy.

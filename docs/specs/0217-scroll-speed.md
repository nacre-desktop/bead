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
shared Bead store. B0217 replaces that policy with the owner's faster, uniform 54 px token.
The separate `extras.next` / `extras.prev` selection actions and their row-pitch scrolling are keyboard actions,
not pointer scroll views, and retain their one-row behavior.

Validation is pending the coordinator's remote full CI. No local builds, tests or nested sessions were run.
The test suite covers 1:1 finger input, full and fractional `value120`, discrete notches, legacy wheel input,
and the shared conversion used by launcher, Widgets, Settings, chat and notifications.
Live desktop comparison remains unavailable; the desktop test host timed out.

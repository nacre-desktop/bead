# B0217 — shared Bead scroll speed

Bead retains one shared 100 logical px wheel notch across launcher, Widgets, Settings, chat and notifications.
Touchpad finger events remain immediate and pixel-for-pixel, including the first event.
The per-SceneSource wheel override and product-store multiplier are removed; surfaces cannot tune wheel speed independently.
Nacre's separate 0.75 setting applies only to touchpads; mouse speed remains at its existing 1.0 default.

| Gesture | Before | After |
| --- | ---: | ---: |
| Touchpad event of N logical px in Bead | N px | N px |
| Nacre touchpad base event of 100 px, factor 0.25 → 0.75 | 25 px | 75 px |
| Wheel `value120=120` / discrete notch | 100 px | 100 px |
| High-resolution wheel `value120=30` | 25 px | 25 px |

High-resolution wheel values retain fractional detents; the legacy 15-unit wheel fallback maps to 100 px per notch.
Wheel glide remains over 125 ms. Touchpad inertia follows immediate finger movement.
The Bead Settings registry mirrors Nacre's scroll-speed row metadata and describes it as “Adjust touchpad scrolling speed.”

Product scene tests send 23.5 px then 100 px and verify 123.5 px in launcher, Widgets, Settings, chat and notification history.
Wayland conversion tests cover finger precedence, discrete notches, full and fractional `value120`, and legacy wheel input.
The required full Bead CI is being rerun after this correction; no local builds or tests are allowed.
Live/nested comparison remains unavailable because the desktop test host timed out.

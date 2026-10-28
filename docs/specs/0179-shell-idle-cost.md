# B0179 — Shell idle cost

The supplied 60-second profile measured 3.9% of one core and about 30 wakeups/s while the desktop was idle. Repeated daemon snapshots were decoded, mirrored and restaged, while static wallpaper subscribed to environment changes that cannot affect it. Idle frame callbacks must not schedule another draw; HH:MM changes only at a minute boundary.

| Measurement | Before | After |
|---|---:|---:|
| Shell CPU, 60 s idle | 3.9% core | pending private capture |
| Wakeups | ~30/s | pending private capture |
| Same-data source restages in simulated 60 s | repeated | 0 |
| Idle redraws between minute updates | repeated | 0; one changed minute frame |

## Change and proof

- `Mirror::accept` advances same-data revisions without projection or publish; same-revision divergent data remains an error.
- The wallpaper motion thread keeps compositor and desktop subscriptions only while video is selected. Wallpaper and picker preference watches remain available while static.
- The Wayland frame callback gate is tested over 60 idle callbacks with exactly one redraw for the HH:MM boundary.
- No temporary duplicate code or dependencies were added.

## Acceptance status

Unit tests and goldens: pending remote CI. Private nested idle capture: pending; no live desktop run. Full Bead CI: pending.

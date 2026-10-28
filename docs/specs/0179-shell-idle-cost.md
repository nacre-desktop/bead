# B0179 — Shell idle cost

The supplied 60-second profile measured 3.9% of one core and about 30 wakeups/s while the desktop was idle. Repeated daemon snapshots were decoded, mirrored and restaged, while static wallpaper subscribed to environment changes that cannot affect it. This change prevents same-data restaging and removes those static-mode subscriptions.

| Measurement | Before | After |
|---|---:|---:|
| Shell CPU, 60 s idle | 3.9% core (profile) | 0.033% core (2 ticks) |
| Reported wakeups | ~30/s (profile) | no `perf` tracepoint available on TEST host |
| Voluntary context-switch proxy, all shell threads | not captured | 3.30/s; main thread 1.10/s, `blocking-1` 2.00/s |
| Same-data source restages in simulated 60 s | repeated | 0 |
| Idle redraws between minute updates | repeated | 0; one changed minute frame |

The private CPU capture used the release shell in nested niri at 2880×1800 physical, 1920×1200 logical, scale 1.5. The bar showed HH:MM and no video wallpaper was selected. A screenshot is in [evidence/0179/idle-bar.png](evidence/0179/idle-bar.png). The all-thread voluntary-context-switch proxy is above the 3/s target and is not a scheduler wakeup counter; the direct wakeup gate remains unverified.

## Change and proof

- `Mirror::accept` advances same-data revisions without projection or publish; same-revision divergent data remains an error.
- The wallpaper motion thread keeps compositor and desktop subscriptions only while video is selected. Wallpaper and picker preference watches remain available while static.
- The Wayland frame callback gate is tested over 60 idle callbacks with exactly one redraw for the HH:MM boundary.
- `trim_idle_shm` compacted once after startup and once after the minute update; there was no repeating idle compaction.
- No temporary duplicate code or dependencies were added.

## Acceptance status

Unit tests and goldens: full Bead CI passed. The private nested CPU capture passed the <0.5% core target. The direct <3 wakeups/s gate remains open because the host lacked `perf`; the available all-thread proxy measured 3.30/s. No live desktop run.

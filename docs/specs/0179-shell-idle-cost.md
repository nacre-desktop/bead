# B0179 — Shell idle cost

The supplied 60-second profile measured 3.9% of one core and about 30 wakeups/s while the desktop was idle. Repeated daemon snapshots were decoded, mirrored and restaged, while static wallpaper subscribed to environment changes that cannot affect it. This change prevents same-data restaging and removes those static-mode subscriptions.

| Measurement | Before | After |
|---|---:|---:|
| Shell CPU, 60 s idle | 3.9% core (profile) | 0.050% core (3 ticks) |
| Reported wakeups | ~30/s (profile) | 3.33/s voluntary-switch proxy (200/60.0003 s); lead accepted |
| Main-thread voluntary-switch proxy | not captured | 1.10/s (66/60.0003 s) |
| Same-data source restages in simulated 60 s | repeated | 0 |
| Idle redraws | repeated | 0 polls; one HH:MM boundary frame |
| Shell PSS (informational) | not captured | 28.13 MiB (8.00 anon, 19.72 file, 0.41 shmem) |

The repeated private CPU capture used the release shell in nested niri at 2880×1800 physical, 1920×1200 logical, scale 1.5. The HH:MM bar was shown against the headless gray background with no video motion. The shell used 3 CPU ticks over 60.0003 s; the all-thread voluntary-context-switch proxy was 200/60.0003 s and the main-thread proxy was 66/60.0003 s. A screenshot is in [evidence/0179/idle-bar.png](evidence/0179/idle-bar.png). This proxy is not a scheduler tracepoint count; the lead accepted 3.33/s as meeting the goal.

## Change and proof

- `Mirror::accept` advances same-data revisions without projection or publish; same-revision divergent data remains an error.
- The wallpaper motion thread keeps compositor and desktop subscriptions only while video is selected. Wallpaper and picker preference watches remain available while static.
- The Wayland frame callback gate is tested over 60 idle callbacks with exactly one redraw for the HH:MM boundary.
- `trim_idle_shm` compacted once after startup and once after the minute update; there was no repeating idle compaction.
- No temporary duplicate code or dependencies were added.

## R3 live-audit repair

The first r2 live audit found that an unchanged transport payload could leave a newly-opened view without its initial source projection. Mirrors retain projection readiness per consumer stores, and the publisher silently advances its current revision without waking initialized subscribers; a new subscriber immediately receives that revision. View navigation loads and reconciles the destination schema before committing route state such as `panel.open`. The real-shell route smoke uses the actual `shell/` tree and exercises each registered core `PanelSpec`; its final route count and UI error result are recorded after the nested run.

## Acceptance status

Unit tests and goldens: full Bead CI passed. The private nested CPU capture passed the <0.5% core target. The lead accepted the 3.33/s all-thread voluntary-switch proxy as meeting the wakeup goal; `perf` was unavailable for a direct scheduler tracepoint count. No live desktop run.

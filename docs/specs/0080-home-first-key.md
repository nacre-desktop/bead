# B0080 — populated Home first key

Status: **Investigation; no acceptance claim yet.**

Measure the owner's Home → search path separately from the standalone launcher.
Home must remain the opening view; the first typed key must show search results.
The private fixture supplies 12 favourite and 8 recent application entries and
the existing three-row Quick controls column, followed by 320 search matches
(64 projected rows). It does not implement ranking or a second menu engine.

CPU tiny-skia, private nested compositor, 2880×1800 at scale 1.5. Compare five
alternating control/candidate pairs with 60 serial, presentation-gated keys.
Use clocks-only profiling for acceptance and separate stage traces to diagnose
markup/view construction, layout, shaping/raster, composition and damage.
Stage spans can nest; their inclusive times must not be summed as disjoint CPU.

| Gate | Required |
| --- | ---: |
| Home → search first-key UI process CPU | ≤8 ms |
| Standalone first-key CPU | ≤8 ms |
| Later-key median CPU | ≤8 ms |
| Paired launcher-open latency change | ≤+2 ms |
| Paired idle/closed PSS + DRM + SwapPss change | ≤+0.5 MiB |

Before every arm require host swap below 64 MiB; require zero host swap-in and
process SwapPss throughout. Keep the existing containment/watchdog checks.
Record memory backend and anonymous/file/shared residency separately.
No live desktop tests, deployment or Nacre changes are part of this unit.

General primitive under evaluation: an opt-in, one-pass asynchronous projection
recheck after markup can replace an already superseded view before raster work;
it never waits. Buffer adoption copies only the complement of complete repairs.
Neither change adds persistent caches or changes the focused compaction timeout.
The launcher also opts into bounded post-frame sandbox cleanup: every build uses
a fresh VM, at most one completed VM is charged/held, and frame completion or
close releases it. This moves destruction after presentation; it does not claim
to remove that CPU work. Linux frame allocations receive best-effort huge-page
advice only within their existing page-aligned extent; host policy is unchanged.
Fresh SHM adoption leaves already-zero transparent blocks untouched. The fixture
can seed canonical `menu.preferences` so favourites have their real star state;
the same fixture support and numeric readiness probes are used for the control.

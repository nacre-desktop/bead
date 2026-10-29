Status: blocked on owner laptop live acceptance; the rejected panel-state regression is fixed and the requested headless work is pushed.
Branch: `task/codex-b0180-draw-cost-r2-20261029`; code commit `ec573ada`, documentation `a1415438` and final report `a8d7e552`, pushed without force.
R2 commits use `[no-publish]`, the required author/committer identity, and Codex trailer.
Cause: motion coalescing deferred the initial schema Reload, then PanelVisibility(true) tried to update `panel.open` before route state existed.
Fix: initial schema Reload completes during motion; later refreshes remain coalesced. `panel.open` retains its open/closed meaning.
Real-shell smoke opens/closes 12 routes: dashboard, session, launcher, network, bluetooth, battery, sound, sound/core, calendar, chat, control and wallpaper. Each yields a prepared CPU scene with no UI errors.
Startup warming begins 750 ms after first primary frame while no panel is open, uses output logical geometry/scale, and makes no mapped surface or `presented` write.
Consume-once CPU cache cap: 2 MiB including strong assets. Smoke warmed 12/12 routes using 1,429,994 bytes.
B0178 scheduler proof: 37 commits in 300 ms; maximum gap 8.33 ms.
First-content wall ms, baseline `3b6de41f` → candidate: dashboard 213.51 → 3.04; control 140.39 → 1.46; sound 41.27 → 1.73.
Debug tick p50/p99 ms: dashboard 397.08/612.16; control 237.45/384.70; sound 1.60/1.62. Dashboard/control remain above the 8.33 ms target.
Full CI on `a1415438`: exact task recipe completed with `FULL_CI_OK`; workspace 784 passed, 3 ignored; remaining workflow checks passed.
Focused checks: worker schema-bootstrap regression, real-shell panel smoke, warm-cache pixel/scale/budget tests, and 120 Hz scheduler proof passed.
Profiling ELF: `.lead/profiling/nacre-shell`, 385,863,616 bytes, x86-64 with debug info. SHA-256 `f5a5d3f1cd1ea52605cfc227c2131b8d85bc7405919e14ef7e48f5e1f7777bd2`.
Removed: 0 product lines and 0 files. Direct-render fallback remains for contour scenes unsafe to split.
Open: software timing is not physical scanout; per-pose production Chrome SVG rasterization and presented-state serialization counts remain unverified. No laptop live audit rerun.

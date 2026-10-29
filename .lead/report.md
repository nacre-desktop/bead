Status: blocked on owner laptop live acceptance; the rejected panel-state regression is fixed and the requested headless work is pushed.
Branch: `task/codex-b0180-draw-cost-r2-20261029`; R2 code commit `ec573ada` plus verification docs commit `0f1d502b`, pushed without force.
R2 commits `a1fc48f3` through `ec573ada` and the docs follow-up use `[no-publish]`, required author/committer identity and Codex trailer.
Cause: motion coalescing deferred the initial Reload that declares route stores, then PanelVisibility(true) attempted to update `panel.open` before the schema existed.
Fix: the first schema-building Reload now completes during motion; later refreshes remain coalesced. `panel.open` keeps its existing open/closed meaning.
Smoke test opens/closes dashboard, session, launcher, network, bluetooth, battery, sound, sound/core, calendar, chat, control and wallpaper through the real shell Runtime; all prepared CPU scenes pass without Runtime errors.
Startup warming begins 750 ms after the first primary frame while no panel is open, uses actual logical output geometry/scale, and creates no mapped surface or presented-state write.
The consume-once CPU cache is capped at 2 MiB including strong asset references. The 12-route smoke retained 1,429,994 bytes and warmed 12/12 routes.
B0178 scheduler proof: 37 commits in 300 ms; maximum gap 8.33 ms.
First-content wall ms, baseline 3b6de41f → warmed candidate: dashboard 213.51 → 3.04; control 140.39 → 1.46; sound 41.27 → 1.73.
Debug tick p50/p99 ms: dashboard 454.6/728.8 → 397.08/612.16; control 282.3/414.3 → 237.45/384.70; sound 40.9/41.9 → 1.60/1.62.
Full Bead CI on `0f1d502b`: `FULL_CI_OK`; workspace 784 passed, 3 ignored; all remaining workflow checks passed.
Profiling ELF: `.lead/profiling/nacre-shell`, 385,863,616 bytes, x86-64 with debug info. Build: `RUSTFLAGS=-C force-frame-pointers=yes nice -n 10 cargo build --profile profiling -p nacre-shell --locked -j2`; SHA-256 `f5a5d3f1cd1ea52605cfc227c2131b8d85bc7405919e14ef7e48f5e1f7777bd2`.
Removed: 0 old routes or files. The direct-render fallback remains for scenes whose contour order cannot be safely split.
Open: debug software timing does not prove physical 120 Hz scanout; per-pose Chrome SVG rasterization and per-frame presented-state serialization counts remain unverified. No laptop live audit was rerun.

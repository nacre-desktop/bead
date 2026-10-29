Status: blocked on owner laptop live acceptance; the rejection fix and requested headless validation are complete.
Branch: `task/codex-b0180-draw-cost-r2-20261029`; code HEAD `ec573ada` pushed without force. Report/docs follow-up only.
R2 commits: `a1fc48f3`, `76f993ff`, `675d0b9d`, `8be49a6d`, `fe55055e`, `ec573ada`; every commit uses `[no-publish]`, required identity and Codex trailer.
Cause: motion coalescing deferred the first Reload, then PanelVisibility(true) committed `panel.open` before the route schema existed.
Fix: first schema-building Reload is admitted during motion; later refreshes remain coalesced. `panel.open` remains the open/closed state and is verified true/false.
Real-shell smoke loads and opens/closes 12 routes: dashboard, session, launcher, network, bluetooth, battery, sound, sound/core, calendar, chat, control, wallpaper. Every route produced a CPU-prepared scene without Runtime errors.
Startup warming begins after the first primary frame, uses actual output logical size/scale, and runs at nice 10 without creating a surface or calling `presented`.
CPU cache is consume-once and bounded at 2 MiB including asset references. Smoke warmed 12/12 routes; retained 1,429,994 bytes.
B0178 120 Hz scheduler proof passed: 37 commits in 300 ms, maximum gap 8.33 ms.
Warmed first-content wall ms, baseline 3b6de41f → candidate: dashboard 213.51 → 2.84; control 140.39 → 1.45; sound 41.27 → 1.74.
Debug tick p50/p99 ms: dashboard 454.6/728.8 → 398.61/612.80; control 282.3/414.3 → 238.03/385.71; sound 40.9/41.9 → 1.60/1.61.
Full Bead CI on `ec573ada`: `FULL_CI_OK`; 784 passed, 3 ignored; remaining workflow steps passed.
Profiling ELF built in 5m48s with `RUSTFLAGS='-C force-frame-pointers=yes' nice -n 10 cargo build --profile profiling -p nacre-shell --locked -j2`; 385,863,616 bytes, x86-64 with debug info; SHA-256 `f5a5d3f1cd1ea52605cfc227c2131b8d85bc7405919e14ef7e48f5e1f7777bd2`.
Removed: 0 old routes/files; the existing direct-render fallback remains for scenes whose contour order cannot be safely split.
Open: benchmark is debug software rendering, not physical 120 Hz scanout; Nacre Chrome SVG rasterization and per-frame serialization counts remain unverified. No laptop live audit was rerun.

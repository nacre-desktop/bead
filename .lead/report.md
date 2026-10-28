Status: failed/incomplete — B0178 still lacks the required CPU stage+draw benchmark and prepared panel/contour warmup work.
Branch: task/codex-b0178-frame-cost-20261028, to push; HEAD 5dcdeb709d64c2397d877fb6d7ebb08d16c5188d.
Commits since origin/main: all include `[no-publish]`; author/committer and Co-Authored-By trailer verified.

Changes: defer Luau scalar-animation worker deadlines through the panel slide, measured from first presentation; catch-up runs afterward. Dashboard projections/chrome memoize by source epoch and store revision. Notice icon paths and `(theme,name,size)` lookups use byte-bounded LRU caches, refreshed on theme-changing reload. `Client::status` shares immutable `Arc<Status>` snapshots. Existing three-buffer reuse and the earlier allocation-free damage-copy setup remain.

Call-count evidence: slide scheduler 0 stage wakeups across 40 slide ticks; Luau materialize/build 1 initial and 0 across 40 unchanged wakeups; dashboard project/chrome 1 initial + 1 open update, then 0 across 40 unchanged wakeups; icon lookup 1 across 40 identical `(theme,name,size)` requests; status reads share one Arc.
Before profile (owner, live): stage 44–50%; docked detail projection 47% of stage; dashboard project/chrome 13%; reload/build 9%; status 6%; BTreeMap clone 6%.
After CPU p50/p99 per full stage+draw tick: NOT MEASURED. Do not compare those sample percentages as latency.

Profiling ELF: `.lead/profiling/nacre-shell` (381 MiB x86-64, debug_info + eh_frame confirmed).
Build: `RUSTFLAGS='-C force-frame-pointers=yes' cargo build -p nacre-shell --profile profiling --locked -j2` in Arch task container; `PROFILE_BUILD_OK`, exit 0.
Full Bead CI: `bash /workspace/.lead-ci.sh` in Ubuntu 24.04 task container; `FULL_CI_PASS`, exit 0 (fmt, clippy, tests, analyzer, service checks, cargo-deny).
`git diff --check` passes. Lines vs origin/main: +696/−62 total; product crates +659/−62.

Open acceptance: matched control-centre/sound/dashboard 2880x1800 scale-1.5 stage+compose+damage-copy CPU benchmark before/after; cached prepared content/contour; idle first-open warmup; lead live audit. No laptop install or live test was run.

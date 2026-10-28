# B0178: panel slide frame cost

The owner profile shows full source work is being scheduled from view animation deadlines even though Wayland already re-poses panel slides from frame callbacks. The candidate defers those view timers through the configured panel transition, then allows one catch-up update. Real source notifications remain immediate.

| Cause | Change / evidence |
|---|---|
| Animation timer stages | `Runtime::wakeup_at` holds scalar view animation deadlines through panel motion; 40-frame scheduler test expects 0 stage wakeups |
| Dashboard project/chrome | Memoized by external notification epoch and the store revisions it actually reads; unit test counts 1 initial build + 1 open update, then 0 across 40 unchanged wakeups |
| Luau materialize/build | Session unit test counts 1 initial materialize/build and 0 across 40 unchanged model wakeups |
| Icon lookup/stat | 512-entry LRU caches full icon queries and `(theme,name,size)` lookups; cache clears when `Reload` observes a changed GTK theme; test counts 1 lookup across 40 calls and checks negative caching/theme change |
| Daemon status clones | `Client::status` shares immutable `Arc<Status>`; copy-on-write only on status updates; pointer identity test covers repeated reads |
| Full stage+draw CPU benchmark | Pending |
| Laptop slide acceptance | Lead-owned live audit pending |

The profiling profile inherits release optimization and keeps debuginfo. Build with `RUSTFLAGS="-C force-frame-pointers=yes" cargo build -p nacre-shell --profile profiling --locked -j2`.

Remaining: CPU stage+draw benchmark before/after at 2880×1800 scale 1.5, prepared panel/contour rasterization, and idle warmup. The previous buffer-copy allocation reduction remains in the branch. No temporary duplicate implementation introduced.

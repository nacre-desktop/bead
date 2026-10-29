# B0186: markup budget and the stuck "old UI kept" notice

Live evidence (laptop, 2026-10-29 08:07, build 6f6bde99): after a lid-open resume, the journal showed `widgets/registry.luau:174:1: runtime error: markup exceeded 50 ms limit` twice, and the top-left "Reload failed, old UI kept" notice stayed until the shell was restarted.

## Cause

1. Markup build normally costs about 2 ms (release; `bar_markup_stays_well_under_the_budget`, median 2.2 ms, max 2.5 ms for a bar with status pill, clock, title, updates and notifications). Nothing algorithmic approaches 50 ms. The overrun was the budget itself: it compared `Instant::now()` against a wall deadline, so a resume stall, CPU starvation or a descheduled thread during Wi-Fi reconnect/wake counted as Lua time.
2. The notice was sticky by design: `Session::error` was only cleared by a successful full generation commit. A failed update (or a reload that only overran) left the error set, and every later successful update was replaced by the error scene, so a transient failure never cleared without a file change.

## Fix

- The markup budget is now thread CPU time (`bead_core::profile::thread_cpu_ns`, `CLOCK_THREAD_CPUTIME_ID`), sampled every 64th VM interrupt. The 50 ms limit stays as a safety net against runaway markup. The behaviour-VM budget in `script.rs` still uses wall time (out of scope, recorded as a follow-up).
- `Session` tracks `source_fault` (reload failed on the source, notice stays until a reload succeeds, as before) and `retry_reload` (reload failed only with a budget overrun). The next staged change retries the reload. A successful commit of any transaction clears a non-source notice.

No temporary duplicate paths added.

## Checks

| Check | Result |
|---|---|
| `a_budget_overrun_is_retried_and_its_notice_clears` (bead-ui) | notice clears after a later successful reload |
| `a_failed_render_notice_clears_after_a_later_render_but_a_broken_source_keeps_it` | transient notice clears; broken source keeps it |
| `bar_markup_stays_well_under_the_budget` (nacre-shell) | release median 2.17 ms, limit 10 ms (debug limit 40 ms) |

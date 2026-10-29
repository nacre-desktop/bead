# B0179 r3 report

Status: validation running; r2 live audit rejected for fresh-view `panel.open` errors.
Branch: `task/codex-b0179-shell-idle-r3-20261029` (push pending validation).
Diagnosis: identical payload skips could bypass a new consumer's first projection; navigation committed route state before destination schema reconciliation.
Fix: `Mirror` tracks projection against consumer stores; navigation loads/reconciles destination state before committing route changes.
Regression tests: fresh consumer projects identical payload once; initialized consumer skips repeats; navigation projects `panel.open` after target schema loads.
Real-shell smoke: opens/closes all registered core panel views from the actual `shell/` tree; fresh network session reopens on unchanged private data.
Smoke result: pending.
Idle capture retained: 0.050% core/60 s; voluntary-switch proxy 3.33/s; lead accepted.
120 Hz proof retained: 37 commits/300 ms, maximum gap asserted ≤8.33 ms, no per-frame staging.
Full Bead CI: pending on task-scoped remote host.
No `bead-render/tree.rs` or slide-drawing changes; no live laptop run.

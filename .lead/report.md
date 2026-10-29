# B0179 r3 report

Status: incomplete; r2 live audit was rejected for fresh-view `panel.open` errors.
Branch: `task/codex-b0179-shell-idle-r3-20261029` pushed at `cffae0e8`.
Fix: per-consumer projection readiness, silent publisher revision advance, and destination-schema reconciliation before route commits.
Regression tests: fresh consumer projects identical payload once; initialized consumer skips repeats; navigation declares `panel.open` before commit.
Full CI: `bash run-ci.sh` in task Ubuntu 24.04 container passed; 783 tests passed, 2 ignored; formatting, Clippy, feature checks, release build, and cargo-deny passed.
Real-shell smoke: `scripts/try-bead.py --panel-route-smoke --cpu` loaded the actual `shell/` tree in nested niri.
Smoke opened and closed the launcher; Sound became target-open but stayed unprepared without a first presentation after 20 s.
The smoke log had zero `UI_ERROR` and zero unknown-state errors; remaining routes were not reached.
Idle result retained: 0.050% core/60 s; voluntary-switch proxy 3.33/s, accepted by lead.
120 Hz proof retained: 37 commits/300 ms, maximum gap asserted ≤8.33 ms, no per-frame staging.
No `bead-render/tree.rs` or slide-drawing changes; no live laptop run.

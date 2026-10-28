# B0179 report

Status: failed acceptance gate — CPU/redraw tests and CI pass; the direct <3 wakeups/s gate remains unverified.
Commits: aebcc80a, f29ba7ce, 399c23e8, f6462c8f, 31d8b9a1, 1c4cb3e0, 2769b613 (all [no-publish], required author/committer and Codex Luna trailer).
Branch: pushed `task/codex-b0179-shell-idle-20261028` only; not integrated. origin/main advanced to 3b6de41f while working; live system untouched.

Changes: same-data snapshots advance revision without publish/restage; daemon status reads share Arc; static wallpaper drops compositor/desktop subscriptions; idle frame gating and fake-minute tests added.
Baseline: supplied profile 3.9% core and ~30 wakeups/s over 60 s.
Private nested release capture: 0.0333% core (2 ticks) over 60.0 s at 2880x1800, logical 1920x1200, scale 1.5; HH:MM visible, no motion requested. Screenshot: docs/specs/evidence/0179/idle-bar.png.
Idle UI trace: 5 commits total including startup; one minute-boundary update after idle. SHM compaction ran at startup and the minute update.
Same-data mirror test: 60 simulated snapshots, 0 restages. Fake 60 s: 0 static timeout wakeups and exactly 1 minute redraw.
Wakeup limitation: TEST host has no `perf`; `/proc` voluntary-switch proxy was 198/59.995 = 3.30/s total (main 1.10/s, blocking-1 2.00/s). This proxy is not a scheduler wakeup count and is above target.

Checks: `git diff --check` passed. Full `.github/workflows/ci.yml` replica in `b0179-shell-idle-20261028-ci` exited 0; workspace tests 778 passed, 2 ignored; fmt, Clippy, feature, packaging, lifecycle and cargo-deny steps passed. Release shell and tray fixture builds passed on prod. Private capture safety reported zero OOM and zero swap use.
Product source delta: 203 lines added, 58 removed. No live desktop run.

# Status

## 2026-10-15 — Gate C final measurements

Features 7–8 only. Stop at Gate C; no optimization pass or Feature 9.
Latest published source c72fbde; CI 38710897960 passed. Isolated task
worktree: ~/.local/share/nacre/ai/worktrees/bead-feature7-gate-c.

Current checkpoint fixes independent view drawing: worker preparation and
no-change events do not repaint; bar commits draw bar surfaces only; each
surface tracks its accepted update for presentation feedback. Pixel checks
now verify the visible volume track on every reopen, beyond markers/actions.
Ten default niri and Hypr pixel checks pass, including rapid reversal and
keyboard close. Temporary scene/SHM dump hooks have been removed.

Close drops the view worker/VM/rasters/SHM/renderer, then trims allocator
pages. The remaining bar baker drops font/glyph scratch and offscreen cache
entries while retaining its visible rasters. Test proves unchanged pixels
and lazy rebuilding after trim. Latest Hypr ten-cycle/post-output/rapid
cleanup delta +4.38 MiB; earlier niri before final trim +4.99. Final 60s
samples pending. Preserve +5.13 and expanded-lifecycle +5.03 FAIL samples.
Do not use earlier blank-body runs as final visual/pacing evidence.

70 tests, fmt/clippy and CPU-only check pass. Facade/example, generated
80-line SDK + 18-module analyzer, native core/ext clipboard UTF8/plain
peer transfer (ext before input), headless check, numeric-only 0600 30s
report, real controls, all dismissals, state/reload/SIGKILL and legacy
visibility query proofs are implemented. Hypr output move/removal passes;
nested niri off retains its global, real niri hotplug remains unverified.

QML reference smoke passes all 20 real presentation phases. It uses the
audited read-only Nacre 68df8dd archive, unchanged widgets and output-control
subset with static transport shims. Full 60s reference samples pending.
Native CPU policy avoids GL's ~20 MiB closed driver/anonymous floor; GL
remains explicit. Explain warm scene/renderer differences versus ~105 MiB
Feature 6 control. Keep typing 29.6→32.7% and inherited 6b niri pacing/idle
invariance concerns. No live/physical test, installation or release.

Next: commit/push/CI checkpoint; final 60s native + QML on both, report,
STATUS/CI/brain, then STOP at Gate C. No concurrent local builds in samples.
CARGO_TARGET_DIR=/home/siverteh/Projects/bead/target; TMPDIR=/var/tmp.
Evidence: canonical .research/feature8. Nested nice10/Cargo-j2 only.

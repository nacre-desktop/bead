# Status

## 2026-10-15 — Gate C final measurements in progress

Features 7–8 only; stop at Gate C. No optimization pass or Feature 9.
Feature 7 4d74cba CI 38693436696 passed. Framework/panel 7117954 CI
38704774853 and recovery 60a537c CI 38705774910 passed.

Real sound-output panel: shared native audio service, per-view worker,
CPU renderer; explicit native GL stays available. Slider click/drag/keys,
mute, Escape/outside/button dismissal pass both private compositors.
Hypr output move/removal passes; nested niri output-off retains its global,
so real niri hotplug remains unverified. Actual panel reload/reopen/invalid
recovery/SIGKILL restore pass both; page and open state preserved.

Core and extension clipboard both transfer UTF-8/plain text with private
peer clients. Extension works before keyboard input. Fallback now matches
kit geometry, avoiding duplicate SHM allocation. View close drops worker,
VM, rasters, renderer and SHM; a second trim follows retired packet drainage.

Latest core Hypr three-cycle max idle delta +2.70 MiB (before deferred trim:
+5.13 FAIL). Earlier long runs: Hypr +4.99 MiB after60s; niri +2.11 MiB,
plus2.60 SwapPss. Preserve failures; final repeated60s default-mode runs next.
CPU policy avoids GL's ~20 MiB closed driver/anonymous overhead. Full native
pacing is NOT established: opening p99 Hypr16.65ms / niri100.32ms in those runs.

69 tests, fmt/clippy/CPU-only pass; two wall-clock50ms Luau test failures
under host load are retained, isolated rerun passed without budget changes.
Earlier all-features/facade/analyzer pass. --report proved30.001s,0600,
numeric-only with closed/current surface history. Final QML subset comparison
uses audited Nacre68df8dd archive, read-only. No microphone/device-list parity claim.

Next: final60s bar/warm/closed CPU+PSS+DRM+SwapPss on both; matched QML
output-controls subset; final report/CI/brain, then STOP at Gate C.
Keep typing29.6→32.7% and inherited6b niri pacing/idle invariance concerns.
Worktree: ~/.local/share/nacre/ai/worktrees/bead-feature7-gate-c.
Canonical target: /home/siverteh/Projects/bead/target; TMPDIR=/var/tmp.
Nested only, nice10/Cargo-j2; no physical/live test, install or release.

# Status

## 2026-10-15 — Feature 8 / Gate C in progress

Feature 7 published at 4d74cba; CI 38693436696 passed. Private versioned
commands, audited route fixtures, bounded files/process/helper transport done.
Owner accepted 6b; authorized Features 7–8 only. Stop at Gate C.

Current source checkpoint: facade/example, Rust-generated Luau SDK with
18-module analyzer CI, headless --check, numeric-only 30s --report, native
core/ext clipboard, and a lazy real sound-output panel sharing bar services.
Local 69 tests, clippy, CPU-only check pass; previous all-features/facade pass.
Generated SDK analysis and headless sound/launcher checks pass. Private Hypr
passes clipboard, slider click/drag/keys, mute, Escape/outside/button dismissal,
ten close/idle cycles, output movement/removal. Not yet a Gate C pass.

Latest ten-cycle Hypr probe (MiB PSS + deduplicated DRM): bar 20.78; small
CPU sound panel warm 29–34; closed idle +0.38–3.86. Earlier two-cycle bar
22.83; warm 31.81–34.57; closed +2.07/+3.93. Matched deltas pass <=5.
GL alternative failed idle: +~20 MiB, including Mesa/LLVM mappings after
EGL shutdown. CPU selected provisionally for this small view, pending pacing
and repeated <=5 MiB checks. Larger native-GL benchmark remains selectable.
Evidence: canonical .research/feature8/panel-hypr-*; failed probes retained.

Source checkpoint 7117954 CI 38704774853 passed. Latest recovery fixes:
size changes invalidate parked callbacks, panel reload uses50ms debounce,
owned stale command sockets recover safely; saved open/page restore after
SIGKILL. Private Hypr real-view reload/reopen/crash proof passes (five reload
file→presentation64.73–189.45ms,50ms debounce included). 30s report proof
passed30.001s, numeric-only0600, including closed-view frame history.

Next: niri reload/crash and final60s CPU/pacing/memory samples; matched QML output-control subset; commit/push/check CI at checkpoints.

Worktree: ~/.local/share/nacre/ai/worktrees/bead-feature7-gate-c.
Canonical checkout read-only; CARGO_TARGET_DIR=/home/siverteh/Projects/bead/target.
Use TMPDIR=/var/tmp (host /tmp full; long paths break Hypr IPC sockets).
Nested only, nice10/Cargo-j2; no desktop install, physical test or release.
Keep 6b niri pacing/idle invariance open; retain typing CPU 29.6→32.7% for
post-Gate-C optimization. Report warm memory against Feature 6 ~105 MiB
with scene-size/renderer differences disclosed. No optimization pass yet.

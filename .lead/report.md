status: failed — cadence and adaptive scrim acceptance remain unmet.
HEAD: 45a5bce2; pushed safely to `task/b0167-widget-polish-20261027`; no main integration while acceptance fails.
Commits use siverteh <92440237+Siverteh@users.noreply.github.com>; every task commit has `[no-publish]`.
CI: Ubuntu 24.04 full Bead recipe on dev-2, `bash /ci.sh` → `B0167_FULL_CI_PASS` (exit 0).
Captures: `bash .lead/capture-wrapper.sh` on desktop; seven states complete, rc=0, OOM=0, swap=0, min MemAvailable 10.5 GiB.
Cadence: final CPU 60/120 Hz sweep, five repeats; 2 commits each; presentation p99 1256/1258 ms at 60 Hz, 1253/1241 ms at 120 Hz; drop rate 98.7–99.3%.
Render p99 0.138–0.167 ms; physical refresh timestamp unavailable. Adaptive scrim needs a radial renderer and wallpaper sampling API; paired HTML is not QML parity.
removed: 36 product text lines (net change; report and binary evidence excluded).
Remote B0167 CI and desktop task directories and sessions cleaned; no laptop builds/tests/captures.

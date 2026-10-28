Status: done; branch: `task/codex-b0178-frame-cost-20261028`; pushed code commit `f1746a9b`.

Cause: suppressing `Session::preview_deadline()` during the panel hold removed the actor timeout that had also prompted redraws. Wayland’s companion frame callback only continued on `panel_animations.active()`, which did not reliably cover scene-motion slides. This made ordinary panel transitions wait for unrelated wakeups; the audit saw only 3–6 frames, gaps 300–1000 ms, and ~1 s duration. The separate extras animation still ran at 39 frames / 11–12 ms gaps.

Fix: record a companion motion deadline when its first frame commits; keep scheduling/drawing from Wayland frame callbacks through that deadline. This redraws and re-poses the already prepared scene without calling `SceneSource::stage`; reset the deadline on each panel target change. Retained the icon cache, dashboard memoization, shared status snapshots, and existing damage-copy improvements.

Regression test `companion_slide_callbacks_at_120_hz_keep_committing_without_staging`: fake callbacks at 120 Hz for 300 ms produced 37 commits, max gap 8.33 ms, and B0170 `PANEL_SLIDE` marker `frames=37`. Existing counter tests passed: 0 stage wakeups in 40 held ticks; 0 Luau materialize/build calls in 40 unchanged ticks after the initial build. Existing icon test counts one lookup per key across repeated requests.

Full Bead CI: `bash /ci.sh` in the task Ubuntu 24.04 container; `FULL_CI_PASS`, exit 0. Profiling build pending final report commit; it will be at `.lead/profiling/nacre-shell`.

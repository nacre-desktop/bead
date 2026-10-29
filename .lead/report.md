Status: done; pushed branch only, no publish.
Branch: `task/codex-b0181-close-ghost-r2-20261029`.
Fix: exits use accelerated `t^4` and shorter durations; vertical closes retain x, side closes retain y.
The r1 whole-scene translation and transparent terminal frame remain.
Close-pose proof: `revealing_panel_close_moves_every_part_as_one_scene_at_120_hz` passed; final 3/26 frames move 30.5% of travel, x fixed.
Axis proof: `vertical_close_keeps_x_fixed_while_side_close_keeps_y_fixed` passed.
B0178 proof: `companion_slide_callbacks_at_120_hz_keep_committing_without_staging` passed.
Real-shell smoke: `real_shell_panel_routes_open_and_close_with_prepared_scenes` passed for 12 routes; 0 UI_ERROR.
Full Bead CI: `bash /ci.sh` in task Ubuntu 24.04 container; `FULL_CI_PASS`, exit 0.
Commits: `f3a64ee1`, `9f0ced1d` (code), `b5d52313` (report); author/committer siverteh, `[no-publish]`, Codex Luna trailer.
CI setup retries: mirror needed git metadata; final run used a task snapshot for git-dependent harnesses.

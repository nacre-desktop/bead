Status: done; task branch pushed, no publish.
Branch: `task/codex-b0181-close-ghost-20261029`.
Commit: `40982329` — author/committer siverteh; `[no-publish]` and Codex Luna trailer verified.

Cause: revealing motion reshaped the body and shadow while content moved behind a shrinking edge clip. On close, content could escape the body and leave the launcher header at the dock.
Fix: shared `Scene::panel_offset` preserves reveal openings and closes by restoring full geometry then translating the whole scene. Panel and overlay paths use it; opacity reaches zero before existing frame-gated unmap.

Close test: `revealing_panel_close_moves_every_part_as_one_scene_at_120_hz` passed; 37 poses at 8.33 ms, shared scene offset, no close clip, zero terminal opacity.
B0178 scheduler proof: `forty_slide_frames_do_not_wake_the_script_stage_for_scalar_animation` passed.
B0180 route smoke: `real_shell_panel_routes_open_and_close_with_prepared_scenes` passed for 12 routes, checking `panel.open` and prepared scenes without runtime errors.
Full Bead CI: `bash /workspace/b0181-ci-inner.sh` in the Ubuntu 24.04 task container; `FULL_CI_PASS`, exit 0. `cargo test --workspace --locked -j2`: 784 passed, 2 ignored.

Visual validation: private QML arm passed 19 states at 2880×1800, scale 1.5, requested 120 Hz. The Bead compositor arm timed out at Nacre `ui.dispatch` provider startup with both current-main and protocol-pinned helpers; its captures were discarded. Paired compositor screenshots remain a known gap; the route smoke validates Bead’s actual Luau runtime.
Preview refresh completed after push.
Source/test lines: +422/−33; the 33 removed lines were the separate reveal/slide pose branches. No files removed.

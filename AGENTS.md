# Bead experiment

- Public repository by owner request. Integrate into main and push each feature.
- ADR 0001 approved 2026-10-13; Retained CPU/femtovg/Vello comparison authorized before Feature 3. Stop after each feature.
- Foundation exception (2026-10-15): additive nacre-protocol changes are allowed
  through Nacre CONTRIBUTING (optional fields/new methods plus transcripts).
  Breaking changes require the owner. Otherwise Nacre and Orient are read-only.
- Do not duplicate Orient or port
  Python helpers. Use the existing palette JSON and helper JSON-lines protocol.
- Framework crates: MIT OR Apache-2.0, permissive dependencies only. Never copy
  GPL/LGPL code into framework crates. Shell code/assets retain GPL-3.0 terms.
- Also permitted: CC0-1.0, 0BSD, Unlicense and MIT-0. Use cargo-deny in CI, including
  independent bead-* graphs. GPL dependencies belong only in GPL nacre-shell:
  niri-ipc backend adapter and later libmpv; do not reimplement niri's protocol.
- Keep paperwork lean: no new ledgers. Each new FEASIBILITY entry begins with
  3–5 plain-language lines and a short numbers table, under about 40 lines total.
  Document less than you build. Normal system build dependencies are allowed;
  list additions. Do not build more isolation tooling.
- Feature 2 must stress a launcher-sized rounded/shadowed text/emoji panel at real
  monitor resolution/scale/refresh with damage tracking; report p50/p99 frame
  times, CPU and loaded-font PSS. Report CPU-rendering failure before Feature 3.
- Owner update 2026-10-14: commits use siverteh
  <92440237+Siverteh@users.noreply.github.com> for author and committer.
  This supersedes the generic Nacre contributors identity; keep work email private.
- Feature 2: fractional-scale + viewporter, direct reusable wl_shm buffers,
  renderer interface and correct damage history before stress measurements.
  Compare damage enabled/disabled at effective scale 1.5.
- Every crate, including spikes, sets publish = false.
- Never test on the live desktop without asking the owner first. Prior live
  authorization does not persist. Retained comparison uses private/nested
  sessions; synthetic 120 Hz scheduling is not physical scanout timing.
- Feature 3 approved: femtovg content-sized retained layers, tiny-skia fallback.
  Compare native GL/wgpu and allocation hints; prove shared context with 1/3
  surfaces. Report PSS + DRM total for all memory comparisons. One supervised
  60-second live niri comparison is authorized: announce exact stop command
  beforehand and stop it afterwards. This does not authorize additional live runs.
- Test in an isolated nested compositor by default. Never stop the daily shell,
  change desktop configs, keybindings, autostart or installed Nacre.
- No personal paths, usernames or private machine inventory in committed files.
- Stable-ID state stores live outside views. Reload is validate-then-atomic-swap;
  failure keeps the previous UI and reports file/line/message.
- Hyprland and niri are first-class behind the same neutral backend interface.
- Update docs/STATUS.md at each research milestone; FEASIBILITY.md records honest
  measurements, failures and verdicts. Missing measurements are not zeros.
- At each feature run fmt, clippy with warnings denied, tests and feature proofs;
  compare matched QML components, never a blank Bead surface against full Nacre.
- Stop at owner gates. Use the one-feature workflow described in docs/ROADMAP.md.

- Overnight authorization 2026-10-13: finish Feature4 and Feature5 without stopping
  at intermediate owner gates. Choose Luau markup by default after ADR0002.
  Keep frontend independent of stores/reload/layout/rendering. Nested only; no
  live tests. Local commands nice10, Cargo two jobs. Commit/push each milestone:
  format ADR, layout/lists, launcher port, Gate B tests, input/focus, backends.
  Defer off-thread baking and shader-cache startup work. Stop after Feature5;
  morning summary at top of STATUS, blocked owner decisions recorded.

- Owner resumption 2026-10-15: close F5 gaps, then Features9–10; STOP at GateD.
  Use Nacre's existing helper serve menu-backend lane until menu.query is ready;
  transport stays in GPL nacre-shell, shaped as a Source call. No engine copy.
  Byte-account caches and measure opaque composition before resuming UI work.
  Services run outside views (ADR0003); nacre-daemon owns Nacre state and menu.query.
  Freeze nacre-shell/src/backends except bug fixes; do not build a second launcher
  catalog, ranking or providers. Existing in-process services are transitional.
  The migration agent owns nacre-daemon / nacre-protocol; coordinate additive needs.
  No live desktop permission carries over. Local commands nice10, Cargo-j2.

- Owner memory rule (2026-10-15): first-open increases under 1 MiB may proceed
  if every close/lifecycle stays within bar+5 MiB and steady/closed memory does
  not regress. Report the change; increases over 1 MiB or steady/closed regressions
  still stop. Cache checkpoint215ea35 accepted with a roughly one-hour diagnostic
  time-box and five alternating before/candidate samples per compositor.

- Owner update2026-10-15: migration builds the shared launcher engine after F2,
  before F3. Bead builds its launcher UI against fake menu.query meanwhile,
  using published nacre-protocol schema (current QML result shape until then).
  Fixture UI typing is measured separately; end-to-end typing waits real engine.
  Pin published protocol v1/goldens; never implement ranking in Bead.

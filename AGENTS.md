# Bead experiment

- Public repository by owner request. Integrate into main and push each feature.
- ADR 0001 approved 2026-10-13; Retained CPU/femtovg/Vello comparison authorized before Feature 3. Stop after each feature.
- Nacre and Orient are read-only references. Do not duplicate Orient or port
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
- Commits use Nacre contributors <nacre-desktop@users.noreply.github.com> for
  author and committer. Never expose a personal email in Git metadata.
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

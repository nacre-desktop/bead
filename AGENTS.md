# Bead experiment

- Private repository. Commit directly to main and push at each completed feature.
- Phase 0 only until the owner approves ADR 0001. Throwaway measurement spikes
  are allowed; production framework code is not.
- Nacre and Orient are read-only references. Do not duplicate Orient or port
  Python helpers. Use the existing palette JSON and helper JSON-lines protocol.
- Framework crates: MIT OR Apache-2.0, permissive dependencies only. Never copy
  GPL/LGPL code into framework crates. Shell code/assets retain GPL-3.0 terms.
- Every crate, including spikes, sets publish = false.
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

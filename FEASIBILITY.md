# Feasibility log

Every observation is scoped to a source revision or measurement environment.
User-reported full Nacre reference: approximately 212 MiB PSS fresh and 306 MiB
after panels have been used; these are context, not a matched spike baseline.

## Phase 0 — research (started 2026-10-13)

Built: research repository and tooling only. No production framework/UI.

Verified obstacles:

- Private remote creation rejected with HTTP 401 (existing GitHub login invalid).
  Research continues locally; remote creation/push remains pending.
- No Rust compiler initially on PATH. Installed isolated toolchain in `.tools/`,
  without modifying shell profiles or system packages.
- Orient palette format document is on the active Rust-port branch, not main;
  consume the documented palette, never duplicate its color engine.
- Slint runtime license options do not satisfy the framework's permissive-only
  dependency policy. Study as a comparison, not an eligible Bead dependency.

Measurements: pending. Time/line counts and verdict will be finalized with the
research recommendation. Missing numbers must remain explicitly missing.

Next: complete research, renderer spikes and ADR; stop for the owner's decision.

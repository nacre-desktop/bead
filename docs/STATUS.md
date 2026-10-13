# Status

Updated: 2026-10-13. **Phase 0 complete and pushed. Waiting for the owner’s
architecture decision. Feature 1 is not approved.**

Private remote: https://github.com/nacre-desktop/bead, isPrivate=true,
visibility=PRIVATE, default branch main verified. Isolated main worktree backed
by a separate bare repository. No production framework/shell code yet.

## Done

- Research 00–05, full type/property/import/IPC/keybinding/helper/process/file index,
  dated source/version/API ledgers, ADR 0001 and gated roadmap.
- Final Nacre snapshot: 68df8dd3b25cbf202d3859d0178a50cceffbea0d, immutable ignored
  export. 224 QML files / 27,854 lines / 43 handlers / 61 Process references /
  31 FileView references. Initial b547d06 scope superseded after other integrations.
- Orient format read at a4ef06e86ea06dac8a6d35d179cfd770efc5981a on its separate
  Rust-port branch; no engine duplication. Neither reference checkout edited.
- Twelve final measurements: three 60-second runs per renderer, zero recorded
  CPU ticks/SwapPss. Vello first-run 177.48 MiB PSS and QML 71.85 MiB sample retained.
- Workspace/lock/license split; CI fmt/clippy/tests passed for research code
  revision 7abc5af in [run 38381794452](https://github.com/nacre-desktop/bead/actions/runs/38381794452).
  Subsequent handoff commit changes documentation only. Spike only.
- Local fmt --all --check, clippy --workspace --all-targets --locked with
  -D warnings, cargo test --workspace --locked pass. Rust spike has zero unit tests;
  functional evidence is actual surface mapping and the measured runs.
- Root runner smoke passed for all four modes after workspace relocation; restored
  the final 60-second baseline afterward. Python/shell syntax and artifact checks.
- Reader review addressed cache labels, Feature 3 minimal input/layout dependency,
  helper wording, fresh-checkout setup and numerical pacing concern thresholds.
- Native tool archive signatures verified. All task-private compositor/client/
  activated portal processes stopped. Daily Nacre untouched.

## Recommendation and limitations

SCTK + tiny-skia + cosmic-text + Taffy; small typed markup/Luau, durable stable-ID
stores, complete candidate validation and atomic in-process generation swap.
GPU renderer lazy/measurement-driven. [ADR 0001](decisions/0001-architecture.md).

[Numbers](research/baseline-summary.md), [all rows](../spikes/results/baseline.json),
[log](../FEASIBILITY.md). Blank surface only. No text/effects/input/video/services/
full-shell/reload/pacing benefit established. Startup markers differ between Rust
and QML. PSS excludes some GPU memory; first-run is not guaranteed cold-cache.
Earlier interrupted/short/incompletely isolated attempts are superseded.

## Commands

Owner, from the prepared Bead folder: `./spikes/run.sh`.
Inspect printed PSS/USS/CPU and spikes/results/baseline.json. Offscreen nested
niri; three 60-second samples per client, roughly 13–15 minutes plus build.
Ctrl+C stops the task. No daily shell replacement. Fresh setup:
[spike-setup.md](research/spike-setup.md).

Checks: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`. Task-local Rust is under ignored .tools/.

## Exact next step

**Wait for owner architecture OK.** No implementation work remains authorized
before that gate. Research code 7abc5af and its CI pass are published on private
main; the final handoff update is documentation only.
After OK start only Feature 1: minimum production surface crate/nacre-shell,
blank surface per output on nested Hyprland and niri, clean startup/exit/hotplug
and matched common-boundary metrics. Earlier “continue” does not approve Feature 1.

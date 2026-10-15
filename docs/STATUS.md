# Status

## 2026-10-15 — Feature 7 complete; Gate C foundation next

Owner accepted6b and authorized7–8 throughGateC. Stop atGateC.
Feature7: owner-only private versioned socket/client,120 audited methods
across43 targets,30 CLI-source locations and12 shortcut declarations indexed.
Typed decoding/selector fixtures cover their contracts. Native bar/state
handlers work; later-feature routes return explicit unavailable errors.
Existing Python helper JSON-lines protocol remains external, via bounded
argv/input/output/deadline transport. Regular-file snapshots/watch/replacement
retain last-good validated JSON; no live config/binding changes.

66 workspace tests and fmt/clippy pass. Real CLI/socket/state/error/cleanup
proofs pass on nested niri and virtual120 Hypr. New source checkpoint CI
checked after publication. Evidence: .research/feature7/commands-*.

Next: facade/example, Rust-generated Luau types+analyzerCI, native clipboard,
headless --check and30s --report; then real sound-output panel and GateC proof.

New Gate C limit: after panel closes and idles, process PSS + deduplicated
DRM must return within 5 MiB of matched bar-only (~23 MiB). Release panel
rasters, SHM buffers, CPU pools and worker caches. Report warm bar+panel
against Feature6 (~105 MiB), including the cause of any increase. Retain
typing CPU 29.6→32.7% on the later optimization list.

Worktree: feature7-gate-c, forked from accepted beed15c. Prior worktree,
WIP snapshot, measurements and failed probes remain untouched. Canonical
checkout stays read-only. Tool/target directories remain canonical.

Plan/checkpoints:
1. Private versioned command socket/client and audited compatibility catalog;
   bounded subprocess, files/watch and existing helper JSON-lines transport.
2. Feature7 integration/fixture proofs, commit/push/CI and concise results.
3. GateC foundation: facade/example, generated Luau types+analyzer, native
   clipboard, headless --check and private --report.
4. Real audio-control panel with command open/close/focus/dismiss/output
   lifecycle; aggressive closed-idle release with preserved small state.
5. Matched GateC bar+panel/QML, warm/idle PSS+DRM, CPU/pacing/action timings,
   repeat close/reopen/output removal and honest gate recommendation.

Keep 6b's nested niri pacing/idle-invariance concerns separate. Nested only,
nice10/Cargo-j2; no desktop/session installation, physical test or release.

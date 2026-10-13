# Feasibility log

User-reported full Nacre reference: about 212 MiB PSS fresh / 306 MiB after panel
use. These are context, not the matched blank-surface baseline.

## Phase 0 — 2026-10-13

Built: research 00–05, exact scope/commands/helpers/type/property indexes, 29
source pins and published crate/API/license ledgers, ADR 0001/roadmap, research-only
Cargo workspace/CI, blank layer-shell harness and one-command runner.
**Production framework/shell LOC: 0. Feature 1 awaits owner approval.**

Final immutable Nacre reference: 68df8dd3b25cbf202d3859d0178a50cceffbea0d,
224 QML files / 27,854 lines / 43 IPC handlers / 61 Process references / 31 FileView
references. Initial b547d06 had 27,723 lines / 42 handlers / 60 Process references;
concurrent upstream integration changed window-title/connectivity/notifications.
Final scope reconciled without editing either Nacre or Orient. Palette format
read at a4ef06e on the separate Orient Rust-port branch; no duplicated engine.

### Measurements

Three complete 60-second runs per candidate, after 5 s settling, 512×32 static
layer surface / scale 1. Nested niri 26.04 on headless labwc 0.20.2; Intel ARL,
Mesa 26.2.4. Rust GPU modes Vulkan; QML OpenGL. XDG isolated before private bus
startup, including activated helpers. Client totals exclude parent/helper processes.

| Candidate | First PSS / USS MiB | Later PSS MiB | Median PSS / USS MiB | Max idle CPU, one core |
| --- | ---: | ---: | ---: | ---: |
| tiny-skia | 1.86 / 1.81 | 1.90 / 1.88 | 1.88 / 1.83 | 0.000% |
| Vello/wgpu | 177.48 / 173.04 | 32.05 / 32.39 | 32.39 / 27.98 | 0.000% |
| femtovg/wgpu | 26.79 / 22.36 | 19.80 / 19.88 | 19.88 / 15.46 | 0.000% |
| Blank Quickshell | 59.51 / 47.31 | 71.85 / 59.57 | 59.57 / 47.31 | 0.000% |

Zero ticks at CLK_TCK=100 means below about 0.0167% over 60 s, not zero power.
SwapPss zero. PSS omits GPU allocations; deduplicated driver counters retained
separately, not added to PSS. First/later differences are not fully controlled
cold/warm guarantees. QML's higher second sample is unexplained and retained.
[All rows](spikes/results/baseline.json), [method/summary](docs/research/baseline-summary.md).

Common-boundary startup: NOT MEASURED; Rust frame callback and QML component
completion are different markers. Diagnostic times retained, no speedup claim.
Reload latency/animated pacing: N/A, unimplemented. Text/images/effects/IME,
services, multiple outputs and Hyprland runtime parity: NOT TESTED in this spike.

### Problems and responses

- GitHub creation initially HTTP 401; owner refreshed CLI login. Organization
  repo then created and private visibility/default main verified. No auth/config
  bypass or account change by Bead.
- Rust initially absent. Isolated 1.99.0/compiler/fmt/clippy installed under
  .tools/, with locked Cargo dependencies; no shell/system setup changes.
- Visible nested niri exited cleanly during early trials; client broken pipe
  recorded, external closure cause unconfirmed. Offscreen parent stabilized final
  runs. Interrupted/5-second attempts are excluded.
- Preliminary Vello ~182–187 MiB and later ~4 MiB reading are superseded, not
  trimming evidence. Final first-run 177.48 MiB is a concern despite later ~32 MiB.
- QML READY log filtered by default; explicit qml.debug fixed the harness.
  Missing markers did not mean QML could not load.
- Initial private bus started before isolated activation environment. Corrected
  and repeated all 12 samples; only final campaign supplies the table.
- Slint/libcosmic/niri-ipc/Hyprland wrapper fail framework license policy. Native
  video needs a GPL shell-side boundary. No GPL/LGPL implementation copied into
  Bead crates. Locked spike audit covers 164 packages, no disallowed metadata
  expression; future/native dependencies need their own audits.
- Existing renderer/shaper/layout crates do not solve controls, reactivity,
  safe state-preserving reload, IME/a11y or daemon policy. Those remain work.
- Reference main advanced; frozen final audit supersedes original counts.

### Validation, tool changes, work cost

Local fmt, clippy -D warnings and cargo test pass. Spike has zero unit tests;
functional proof is real surfaces and 12 valid idle runs. Nested screenshot
inspected; root run.sh smoke passed all four clients after relocation, and final
baseline restored. JSON/TOML/link/privacy/publish=false checks pass. GitHub CI passed all steps for research code 7abc5af in run 38381794452;
final handoff is documentation only.

Task-local additions: Rust 1.99.0/rustfmt/clippy, Cargo dependencies, extracted
labwc 0.20.2/wlroots 0.20.2/libsfdo 0.1.4. Native archives verified against official
Arch public keyring; URLs/hashes/signature result retained. No pacman/paru system
install, Git identity/config, desktop/keybinding/autostart or installed Nacre change.
All task-private compositor/client/portal processes stopped.

At research review: about 640 authored Rust/Python/runner lines; 1,900 Markdown
research/index lines; approximately 33384 generated JSON evidence lines including the property appendix.
Generated data is not authored implementation. Framework/shell LOC 0.
Wall time from first checkpoint 10:22:26 +02 to review ~12:05 +02: about 103 minutes,
excluding pre-checkpoint setup and later CI/publication. Includes downloads,
failures/repeated campaigns/reader review; no full-parity forecast from that pace.

### Verdict and next action

**Concern / bounded proceed.** Tiny CPU baseline is promising; full-shell gains
are unproven. Vello first-run cost, toolkit/input/effects scope, atomic reload and
backend semantics are serious risks. Propose SCTK/tiny-skia/cosmic-text/Taffy with
small markup/Luau and durable stores; adopt GPU only when actual scenes justify it.

Next: review ADR 0001 and stop for owner's OK. After approval, Feature 1 only:
production skeleton, blank surfaces per output on both nested compositors, clean
startup/exit/hotplug and fair matched metrics. No deployment or publication.

## Feature ledger

| Feature | Built / measurements | Verdict / unresolved work |
| --- | --- | --- |
| 1. Skeleton/surfaces | Not started; research workspace/CI only | Awaiting architecture approval |
| 2. Drawing/text/palette | Not started | Effects/font/caches unproven |
| 3. Reactive markup/reload | Not started | State/atomicity highest concern; Gate B |
| 4. Layout/animation/input | Not started | IME/a11y/pacing/bandwidth unproven |
| 5. Backends | Not started | IDs/capabilities/licenses documented |
| 6. Bar/services | Not started | Real service idle/aggregate memory unknown |
| 7. Commands/process/files/helpers | Not started | Existing contracts indexed, no replacement |
| 8. Panels/popouts | Not started | Real-size/multi-output/pacing; Gate C |
| 9. Notifications/OSD | Not started | Private bus only, no live takeover |
| 10. Launcher | Not started | Catalog/ranking and pace estimate; Gate D |
| 11. Dashboard | Not started | Rich views/artwork/helper costs unknown |
| 12. Pearls | Not started | Managed geometry/docking unproven |
| 13. Wallpaper/video/effects | Not started | GPL native video remains outside Bead graph |
| 14. AI/remainder | Not started | Streaming/markdown/full parity cost unproven |

# Status

2026-10-14: Feature6 ACTIVE; step1 reference capture complete.
Service foundation b9e1769 retained; no service implementation repeated.
Actual NacreHeader.qml and visual widgets captured unchanged, reference main e9e80a4.
Disposable transport shims use the Bead palette and synthetic title/workspaces;
no daily shell, settings, services or reference files changed. Both nested captures
visually inspected:50logical px,34px capsules/radius17, margins15, centered title.
Public captures: examples/bar-reference-{niri,hyprland}.png.
Niri reference output1278×692/scale1; Hypr reference2880×1800/scale1.5.
Private evidence canonical .research/feature6/capture; reproducible with
NACRE_REFERENCE set and measure.py --bar-reference (Hypr adds --virtual120).
Header has no SNI tray: add a matching compact native tray extension in step2.
Step2 Luau bar/screenshots, step3 real lifecycle, step4 full measurement matrix
remain pending, in that order. No Feature7 or optimization work authorized.
No live tests; Nacre/Orient read-only; nice10/Cargo-j2; stop after Feature6.

## Exact next work — Feature6 is incomplete

1. DONE: unchanged visual QML reference captured and inspected on both private
   compositors; see examples/bar-reference-*.png and spikes/bar-reference.py.
2. Prove real private PipeWire null sink/default metadata, volume/mute and restart;
   current private sensor test deliberately uses an ABSENT private PW socket.
   Add actual SNI+DBusMenu item/add/quit/update/menu-action fixtures and repeat churn
   to check FD/subscription/task/memory stability. Whole-bus replacement untested.
   Native service source is crates/bead-services/{runtime,bus,audio}.rs. No shell
   consumer yet. Direct zbus tray action propagates failures; library state reader
   owns SNI/DBusMenu. Bus executor is recreated on disconnect to cancel old tasks.
3. Wire service snapshots/actions to runtime wakeups/stores, add Luau bar markup,
   native tray icons/menu and audio scroll/mute. No UI-side transport/processes.
   Keep unavailable state explicit. Existing backend interface reused for workspaces.
4. Extend shared-context surface handling for different bar/panel scenes: existing
   --surfaces duplicates ONE scene. That is not a bar+panel proof. App currently
   owns one Scene/Renderer; its GL Transport already shares one context. Measure
   CPU bar alone, GL bar alone, CPU bar+GL panel and GL bar+GL panel with correct
   actual surfaces and aggregate PSS+DRM. Bar must be50logical pixels/full-width;
   current --ui defaults to a content-sized panel with32px margins, needing bar mode.
5. Use real-sized private outputs (target2880×1800/scale1.5, as prior tests) on both.
   Existing nested niri defaults1278×692; resize/configure its private parent/window,
   not daily display settings. Capture matched bar-only QML fixture too. Record
   60s idle CPU, pacing and200+samples p50/p95/p99 for backend/audio actions.
   Diagnose Hypr tail through request/event/state boundaries and CPU-vs-wait timing.
   Feature5 sampler enables synchronous BEAD_RECORD_IPC; compare recording on/off
   rather than assert it caused the tail. No200-sample Feature6 run has happened.

Private logs: canonical .research/feature6/{services-*,roadmap-*}; prior Feature5
harness is spikes/measure.py plus backend-reference/restart helpers. Existing tools
live in canonical .tools, Cargo target in canonical target. Use installed isolated
Cargo/Rustup with nice10/-j2 as before. No machine packages/configs changed.
Before EVERY push fetch; git log --oneline origin/main..HEAD must show only own
handful of new commits. Owner author+committer identity in AGENTS; no old roots.
Nacre/Orient read-only; attribution chat owns shared pins/packaging/identity policy.
Optimization pass only scheduled after GateC; none of its experiments are started.
Read this checkpoint and continue; do not claim Feature6 complete or start Feature7.


Feature5 code milestone af985ba published after concurrent attribution rewrite.
Equivalent code at earlier5687bea passed CI38524233362; final CI checked separately.
Final proof/publication includes non-focusing indicator mode and asserted live title,
reference comparison harness, restart/stale-action verifier and nested demo configs.
Source revision 8481e4b CI passed; the final docs checkpoint intentionally skips CI.
Features1–4 remain published. Luau ADR0002 chosen; owner may veto.
Feature5 accepted and Luau kept; Feature6 active. No live tests authorized or performed.
Real IME-engine composition and screen-reader end-to-end remain unvalidated;
GateB allocator/driver memory tolerance is2MiB, while owned counts/FDs stay stable.
Off-thread baking/shader-cache startup improvements remain deferred.

Nested demonstration: `./scripts/dev.sh niri --backend` or
`./scripts/dev.sh hyprland --backend`. Two test fixture windows supply live titles;
focus a window and click workspace buttons. Close nested window/Ctrl+C to stop.
The fixture clients use Quickshell only for testing; native shell has no Qt dependency.
Scripts use nice10/Cargo-j2. Never install these test configs as a session.
Recorded events are reduced in one shared backend; views/reload own no transports.
Socket replacement at a known endpoint reconnects. A new real-session socket path
requires reinitializing with the new environment, never guessing another session.
Niri IPC-only output geometry changes have no dedicated event; topology/config
reloads refresh outputs. Physical niri output unplug/replug still needs acceptance.
GPL adapters should be reused for Nacre's Rust migration (Wave2).

Canonical ignored evidence: `.research/feature5` (recovered-raw, final raw/logs,
check logs and final niri stale-ID restart proof). Preserve it; no raw logs in Git.
Public screenshots: `examples/backend-niri.png`, `examples/backend-hyprland.png`.
Measurement boundaries/sample limits and earlier failures are in FEASIBILITY.md.
Reload demonstration remains `./scripts/dev.sh niri --demo` (nested only).

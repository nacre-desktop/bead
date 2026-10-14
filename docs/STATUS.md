# Status

2026-10-14: Feature6 ACTIVE; steps1–2 complete; context handoff required.
Service foundation b9e1769 retained, not reimplemented. Step1 source734a4bc/CI
38535278252 passed: actual unchanged Nacre QML header captured in private sessions.
Step2 adds Luau bar, native snapshot wakeups/actions,50px/full-width/non-focusing
mode, native tray ARGB icons, inline DBusMenu choices and audio click/scroll routing.
Look follows the actual QML:34px capsules/radius17, margins15, pearl workspaces,
centered mono title, warm dark palette and compact status/clock; tray is an extension.
Clock/audio/battery/NM/BlueZ snapshots drive the UI; unavailable icons are explicit.
Source bar logo resolves existing palette tokens; it is a fixed fixture-palette asset.
Native backend transports/store ownership reused; no UI polling/process transports.
Four screenshots inspected: examples/bar-{niri,hyprland}-{cpu,gl}.png.
Niri capture1278×692/scale1; Hypr2880×1800/scale1.5 (bar2880×75physical).
Private screenshot app fixture serves SNI+DBusMenu and a real ARGB icon. This is
NOT yet a tray churn/menu-action/restart proof; real private PipeWire still pending.
43default tests,fmt,clippy -D warnings,CPU-only compile,private sensor proof and
workspace+all six independent framework deny policies pass. No machine installs.
No live tests; Nacre/Orient read-only; test processes stopped/surfaces=0.
No measured leak/idle/performance verdict yet. Feature6 remains incomplete.
No Feature7/optimization work. Stop only after Feature6 (unless context handoff).

## Exact next work — continue steps3 then4

1. DONE: actual NacreHeader.qml with unchanged drawn widgets captured on both
   private compositors using deterministic transport shims. Reference main e9e80a4.
   Reproduce: NACRE_REFERENCE set, spikes/measure.py --bar-reference; Hypr adds
   --compositor hyprland --virtual120. Public examples/bar-reference-*.png.
2. DONE: shell/bar/{main,behavior}.luau and nacre-shell --bar on CPU/native GL.
   Reproduce privately: spikes/measure.py --bar-ui; Hypr adds --virtual120.
   Set CARGO_TARGET_DIR,BEAD_RESEARCH_TOOLS,BEAD_RAW_DIR to canonical paths.
   Build release nacre-shell and bead-services --example tray-fixture first.
   Standalone --bar needs a private session and private DBUS_SYSTEM_BUS_ADDRESS /
   PIPEWIRE_REMOTE: NEVER launch directly in the daily desktop for these proofs.
3. PENDING: prove tray apps appearing/quitting,updates,actual menu/action calls and
   repeated churn; real private PipeWire null sink/default metadata,volume/mute and
   daemon restart; test bounded FDs/subscriptions/tasks/PSS and no busy loops.
   Sensor lifecycle test still uses an ABSENT PW socket. Reuse services foundation
   crates/bead-services/{runtime,bus,audio}.rs; new native SNI fixture example is
   crates/bead-services/examples/tray-fixture.rs (explicit private-test env required).
   Menu choices currently inline in title band; left activation/item-is-menu and
   right-click prepare route natively; ensure actual fixture methods execute.
   Whole-bus replacement still unproved. Do not infer leak safety from clean exits.
4. PENDING: support different bar/panel scenes in shared-context surface handling.
   Existing --surfaces duplicates ONE scene, insufficient for bar+one GL panel.
   Measure CPU bar alone,GL bar alone,CPU bar+GL panel,GL bar+GL panel: PSS+DRM
   aggregate at2880×1800/scale1.5 private outputs on BOTH compositors. Resize/configure
   private niri parent/window; current default1278×692 is NOT a real-size matrix.
   Capture matched50px QML bar-only fixture too.60s idle CPU/pacing,200+action
   samples with p50/p95/p99. Trace Hypr's7ms tail at request/event/state boundaries
   and CPU-vs-wait timing; compare BEAD_RECORD_IPC recording on/off before blaming
   its synchronous append. Distinguish metadata arrival from frame presentation.

Private evidence: canonical .research/feature6/capture (step1),bar-{niri,hyprland}
(step2 screenshots/logs),bar-*.log (checks). Harness reuses spikes/measure.py;
new bar-reference.py/bar-ui.py are fixture callbacks, not another isolation stack.
Existing tools canonical .tools; Cargo target canonical target; nice10/Cargo-j2.
Task worktree bead-feature6-bar, branchfeature6-reference-bar. Preserve prior tasks.
Before EVERY push fetch and inspect FULL origin/main..HEAD; only own new commits,
owner author+committer in AGENTS. Normal FF integration/push; no PR/force/old roots.
Nacre/Orient read-only; shared packaging/pins/identity belong to attribution task.
Finish3,push+STATUS;finish4,push+STATUS;STOP with short Feature6 summary.
Context is large: start fresh with "Read ~/Projects/bead/docs/STATUS.md and continue."


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

# Status

2026-10-13: Gate B approved. Overnight Feature4/5 authorized; in progress.
Worktree bead-feature4-authoring from mainfeae2b2. Authoring ADR0002 complete: Luau chosen (31UI lines vs KDL27/QML-like34),
real Lua/KDL syntax checks pass. Frontend/Taffy/layout/lists complete;25 tests and both nested toy smokes pass.
Real launcher sidebar/grid/search port complete:69UI +34behavior lines vs232
selected QML regions (full browser1310; parity limitations in FEASIBILITY).
Matched60s first/later GL102.81/88.79MiB total vsQML193.77/192.75; CPU0both.
27tests pass. GateB100reload + keyed-mutation proofs complete: GL+1.08MiB/FD17stable;
CPU+.40MiB/FD13stable; owned counts fixed. Memory tolerance2MiB, not exactzero.
Input/focus/animation/basicIME/AccessKit milestone implemented and validated.
Current milestone Feature5 native compositor backends (neutral draft notyetcompiled).
31tests pass, clippy passes; keyboard/editor/selection, focus fallback, retained
ease/spring motion, text-input-v3 binding and AccessKit Unix bridge implemented.
Private animatedGL104.69MiB total/2.38%CPU, syntheticpresent8.343/9.327ms vs
QML201.48MiB/4.97%,8.330/9.412. Native keyboard proof passes both nested compositors; virtual keyboard needs
300ms keymap/focus settling. PrivateATSPi status activation succeeds both; registry
connection hits private dbus-broker/systemd integration issue, not yet screen-reader
end-to-end proof. No realIME engine composition run. Platform100reloadGL FD22
stable,1layer/1GPUimage; total128.99→129.46MiB (+.48), latency108.20/125.33ms.
Next publish this milestone, thenimplement Feature5 neutralinterface + GPLadapters,
reconnect/fixtures/hotplug and nestedindicator/title/latency/idle proofs.
IME-engine/screen-reader integration gaps remain honest limitations, not doneclaims.
Choose Luau unless concrete evidence rejects it; no intermediate choice wait.
Nice10/Cargo-j2, nested only. Stop afterFeature5; morning summary will replace
this progress note. Deferred baking/shader-cache work stays deferred.
Code/UI revision8265525 published; CI38461567225 passed. This handoff changes docs only.
Typed JSON/Luau, stable-ID typed stores, reactive bindings, asset snapshots,
atomic CPU/GPU staging, error overlay and event-driven watch are built.
State/input/scroll/open/focus survive view replacement; broken files keep old UI.
Simple input is a proof; full controls/layout/animation remain Feature4.

Shared native GL/femtovg selected for auto; CPU fallback when hardware GL absent.
Private GL, wgpu and CPU-fallback GUI reload/error tests pass (79–103ms latest).
20 tests, fmt and clippy pass on final code; both nested owner-demo proofs pass.
CPU-only build and workspace + all independent framework cargo-deny graphs pass.
No system dependencies installed. Nacre/Orient remain read-only.

ONE approved60s live niri run consumed and stopped; all test PIDs verified absent.
2880×1800/1.5/120Hz; physical presentation p50/p99 GL and QML8.333/8.337ms.
GL total95.64MiB vs QML187.89MiB; CPU1.51/3.29% over55s after5s warmup.
Physical flags7, refresh8333313ns; 21/22 discarded updates. Paired desktop load.
Color emoji confirmed renderer log, not just mapped files. No further live approval.
Cold UI startup314.89ms vs QML228.35; later108.01 vs162.31ms.
Earlier nested GL102.43ms callback p99 remains recorded; do not hide it.

Next: Feature5 neutraloutputs/workspaces/windows/focus/event/actions/capabilities;
100reload bounded-resource test + keyed mutation state test; animation/input/focus,
basic IME/AccessKit; then neutral backends + GPL adapters/events/actions/reconnect
and nested proofs. Commit/push + short FEASIBILITY entry each milestone.
No stopping for owner tonight; record genuine owner-only blockers, do independent
work, and stop afterFeature5. No live tests authorized tonight.

Owner command: `./scripts/dev.sh niri --demo` (nested only; valid/error/recovery).
Click Search/type/scroll/Home; state should survive. Close window or Ctrl+C stops it.
Private raw evidence stays ignored; preserve feature3 logs in canonical .research.

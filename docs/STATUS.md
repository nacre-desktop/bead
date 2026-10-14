# Status

2026-10-14: Feature 5 native backend code milestone validated; final proof in progress.
Features 1–4 published; continuing the preserved bead-feature4-authoring worktree.
Neutral bead-backends API plus self-contained GPL Hyprland/niri modules implemented.
Direct Hyprland sockets; niri-ipc with persistent command socket and event stream.
Recorded fixtures and connection-drop/replaced-socket recovery tests pass both.
IDs survive reconnect/index/output changes; replaced sockets invalidate stale IDs.
Hotplug/topology/move reductions and subscription cancellation/reentrant read pass.
Luau workspace indicator and focused title use shared event-driven state.
Fmt/clippy/tests and workspace plus all permissive-only framework deny graphs pass.
Next: finish nested hotplug/moves and matched QML/Python latency/idle; then stop.

Recovered raw logs and screenshots are preserved privately in canonical
`.research/feature5/recovered-raw`; earlier subscribed60s UI idle recorded zero
CPU ticks on both. These are not final matched-comparison measurements.
Current Rust checks cover38 tests including doctests.
Backend probe timestamps action submission to normalized state observation, not
physical display presentation. QML reference harness uses actual backend files
from a read-only Nacre checkout in a disposable minimal shell/private socket.

No live tests authorized. Nice10/Cargo-j2; no Nacre configs or running shell changes.
Stop after Feature5; Feature6, off-thread baking and shader-cache startup deferred.
Owner may veto Luau ADR0002. Real IME-engine composition and screen-reader
end-to-end remain unvalidated from Feature4. GateB memory tolerance is2MiB;
owned-resource/FD counts stable but allocator/driver memory is not exactly constant.
Feature3 one authorized live run was consumed and stopped; no further live permission.
Prior feature results and failures remain in FEASIBILITY.md and ADRs.

Owner reload demonstration: `./scripts/dev.sh niri --demo` (nested only).
Final Feature5 proof/run command will be recorded after validation/publication.

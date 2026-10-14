# Status

2026-10-14: Feature 5 complete with concerns; published at 8481e4b.
Attribution hold released; final commit uses the owner-approved identity.
Published code revision 8481e4b verified remotely and in both local checkouts.
CI38526465164 passed. STOP before Feature 6; this final checkpoint changes docs only.
Neutral outputs/workspaces/windows/focus/events/actions/capabilities implemented.
GPL Hyprland direct sockets and niri-ipc persistent command/event adapters built.
Live Luau workspace indicator and focused title pass both nested compositors.
Recorded fixtures, reconnect/restart, stale IDs and topology/move tests pass.
Hypr nested output add/move/remove passes; physical niri hotplug remains unvalidated.
Subscribed60s idle: zero recorded CPU ticks both; below~.017% of one core.
Bead/QML totals: Hypr66.75/60.66MiB; niri65.14/105.10MiB (PSS+DRM).
Bead/QML focus medians: Hypr.178/1ms; niri32.026/37ms; Bead tails worse.
38 tests, fmt, clippy, CPU-only and both cargo-deny policies pass.

Feature5 code milestone af985ba published after concurrent attribution rewrite.
Equivalent code at earlier5687bea passed CI38524233362; final CI checked separately.
Final proof/publication includes non-focusing indicator mode and asserted live title,
reference comparison harness, restart/stale-action verifier and nested demo configs.
Source revision 8481e4b CI passed; the final docs checkpoint intentionally skips CI.
Features1–4 remain published. Luau ADR0002 chosen; owner may veto.
Owner decision: accept Feature5's disclosed concerns and authorize the next feature,
or adjust/stop. No Feature6 work started. No live tests authorized or performed.
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

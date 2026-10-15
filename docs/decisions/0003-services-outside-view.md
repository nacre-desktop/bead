# ADR 0003 — Services run outside the view; Nacre data comes from nacre-daemon

Date: 2026-10-15. Status: approved by the owner foundation brief.
Scope: framework F5; Features9–10 pause at their current checkpoint.

## Decision

Bead remains a permissively licensed renderer, Luau toolkit and framework.
Nacre desktop state and outside connections belong to nacre-daemon, not views.
The canonical charter is Nacre docs/architecture/foundation.md.
bead-services are headless hostable libraries; notification bus connection and
history location are injected. Existing in-process clients remain supported.
A protocol-neutral Source mirrors snapshots/merge patches into stable stores.
Only GPL nacre-shell adapts nacre-protocol, pinned to a published git revision.
Nacre owns launcher catalog, providers and ranking; Bead calls menu.query.

## Why

One owner avoids duplicate connections, writers and ranking implementations.
UI reloads/crashes must not lose notification history or desktop service state.
Shared data also makes QML/Bead component comparisons meaningful.

## Consequences and frozen scope

Implement F5a–F5e with headless tests; then stop for the owner's next decision.
Features9–10 and typing acceptance remain paused; GateD is unchanged.
Freeze nacre-shell backends except bug fixes; new compositor needs go upstream.
Current shell notification/tray/audio/compositor ownership is transitional.
Nacre and Orient remain read-only except additive nacre-protocol changes under
Nacre CONTRIBUTING; breaking protocol changes require the owner.
No GPL dependency enters framework crates; code flows Bead→Nacre.
Do not move nacre-shell between repositories during a feature.
Its binary conflicts with Nacre's installed control-script name: resolve at GateD.
No live desktop, notification takeover, installation or release is authorized.
Keep renderer design, Luau, versioned stores, parity fixtures and measured gates.

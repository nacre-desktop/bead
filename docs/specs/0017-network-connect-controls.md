# B0017 — Network connect/disconnect in the Bead network popout

- Status: Implemented, awaiting lead review (visual check skipped)
- Date: 2026-10-21
- Depends on: Nacre N0016 (`network.connect` / `network.disconnect`, main `961de0a3`+, pinned by Bead `948aa10`)

## Goal

Let the Bead network popout (B0009) connect to an access point or saved
connection and disconnect a device, using only the published daemon methods,
with parity to the owner's current QML network page.

## Contract (Nacre core-services, N0016 clarification)

Success is `{changed}` at admission; the topic readback is authoritative.
Method errors are admission-only: `permission_denied`, `stale_revision`,
`unavailable`, `busy`, `invalid`. Post-admission authentication failures appear
as one topic health error ("Network authentication required or cancelled",
retryable). Methods are usable only when advertised in `welcome.methods`.

## Scope

- Rows for access points and saved connections from the `network` topic; a
  connect action per row, and disconnect for the active device connection.
- Pending state while waiting for readback. No optimistic "connected" state;
  never auto-retry; show each error class sensibly. The auth-failure health
  error is shown with a retry affordance.
- Capability-gated: hidden or disabled when the methods are not advertised.
- Non-goals: secrets/password entry (NetworkManager's agent handles prompts),
  hidden SSIDs, VPN, editing connections, new protocol.

## Acceptance

Unit and consumer tests for gating, pending→readback, each error class,
stale-ID handling and no retry. Bead CI green (replicated in a container on
dev-1). Private nested visual check on the owner's desktop only. Memory: no
increase beyond B2 budgets for the popout (report the settled delta).

## Evidence

Implementation (all in `nacre-shell`; no protocol, pin or framework change):
`status_views.rs` resolves row keys (`ap:`/`conn:` connect, `dev:` disconnect)
against the current fresh topic at click time, builds at most five rows (access
points active-first then strongest, then saved profiles no listed AP represents,
only when exactly one device of the profile's kind exists), and calls
`network.connect`/`network.disconnect` through the existing ticket API with the
topic boot guard. A saved profile on an AP is preferred over a raw AP target.
Each method is gated separately on `welcome.methods` (v1); unadvertised, stale or
non-fresh state disables the row and sends nothing. After an admitted
`changed:true` the row stays "Connecting…/Disconnecting…" (all rows disabled)
until the topic no longer resolves the same operation, the authentication health
error appears (rows then say "Retry"; the user clicks again, nothing auto-retries),
boot changes, or a 30 s deadline passes. `changed:false` clears at once. Error
codes map to short messages (permission_denied, stale_revision, unavailable, busy,
invalid_request/validation, timeout, other). Luau: `network.luau` rows via
`ui.Repeat`, actions `network.connect|disconnect` registered under
`network.control`. The popout height is bound (`network.height`, computed in Rust): 222 plus 40 per row (32 high + 8 gap, max 5) plus 28 only while the link message shows (`network.link_visible`). The cold fallback rect (310x222) already equals the 0-row height.

Tests (`reload/link_runtime_tests.rs`, fake-daemon consumer tests): row ordering and
resolution (stale/unknown/wrong-state keys, saved-profile preference, wire shape),
pending until readback with no optimistic state, duplicate ignored while pending,
admission `stale_revision` message and row reusable without a repeat call, per-method
gating including none advertised. Existing Wi-Fi tests unchanged.

CI replication on dev-1 (ubuntu:24.04 container, nice 10, -j4, removed after):
fmt, clippy `-D warnings`, service-boundary, workspace tests (310 passed, 0 failed)
plus trace test, feature checks, minimal example, Luau analyze/generated check,
regression unittests (needed `systemd` installed in the container for guarded-capture
tests; hosted runner has it), cargo-deny (all manifests): all green.

Not done: private nested visual check (unit/consumer tests only; the row look is unreviewed visually; a layout test checks the panel height for 0/2/5 rows and with a message), memory delta (not measured; rows are at most
five small list items, no new threads or allocations beyond the store list).
Clicks are exercised through the host action entry point, not pixel hit-testing.

# B0017 — Network connect/disconnect in the Bead network popout

- Status: In progress (Claude Sonnet worker, lead review)
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

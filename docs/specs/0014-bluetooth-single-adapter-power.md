# B0014 — Single-adapter Bluetooth power control

- ID: B0014
- Repo: nacre-desktop/bead
- Implementer: one Luna worker; Sol review before integration
- Status: In progress — source-only during the Nacre heavy slot
- Start: current main85f89e9, containing B0013 source021fdaad
- Depends on: B0010/B0013 Done; published5f4c0bf3 reserved contract
- Date: 2026-10-19
- Size: one bounded control, report within3hours

## Goal and contract

Add one power switch to the daemon-backed Bluetooth popout ONLY when exactly
one valid adapter exists. Published method `bluetooth.set_power` v1 receives
exactly `{adapter: EntityId, powered: bool}` and returns `CoreResult {changed: bool}`.
The topic remains authoritative. No producer availability follows from a reserved
schema or this UI. Contract: published Nacre5f4 protocol/schema/core-service-call.json
BluetoothPower and core-service-result.json; adapter fields name/powered/discovering
and explicit adapters.order/items in core-service-state.json. No protocol/pin change.

## Scope

Reuse B0013 shared Client/CallTicket lifecycle, its controlled Runtime/store and
private fake-daemon patterns. Files: nacre-shell status_views.rs and tests,
popouts.rs, reload.rs and private sibling tests; new shell/status/bluetooth.luau;
status/behavior.luau only inert host-action declaration; STATUS/spec/queue docs.
Small private helpers may be shared, but no transport/framework redesign or broad
B0013 test refactor. Existing Network/Bluetooth/Battery read-only and legacy
previews remain unchanged. No device pairing/connect/discovery, adapter selector,
multiple-adapter write, direct D-Bus/files fallback, producer/schema/dependency,
backend, live desktop, installation, measurement or packaging work.

## Required behavior

1. Add controlled Bluetooth construction for Runtime with a shared daemon Client.
   Keep the existing lazy bluetooth topic lease: none before open, release at
   Runtime retirement. Subscription stays alive through pending control only
   while Runtime exists; no detached work or new connection.
2. Derive eligibility from fresh valid healthy BluetoothState and connected
   Welcome on the same boot/version, advertising EXACT method v1. Require
   adapters.order length1, items length1 and matching ID. No missing, dangling,
   malformed or ambiguous adapter may be guessed. Zero/multiple adapters keep
   the existing aggregate read-only view; omit switch/action eligibility.
3. Use a Bluetooth-specific template, matching Network's controlled switch style.
   Checked comes ONLY from the sole adapter's powered field. Fixed human failure
   text, sanitized existing adapter display rules; no raw RPC messages/paths.
   Expose inert `bluetooth.toggle_power`; the Runtime handler rechecks eligibility,
   captures that exact opaque adapter ID and desired inverse bool, then invokes
   begin_call with `{adapter, powered}` and existing boot/version guard.
4. One pending ticket per Runtime, disable while busy; no optimistic state change.
   A success reply validates CoreResult and ends that request, but never rewrites
   checked/health/topic. changed:false is a valid acknowledgement, not failure or
   a reason to retry; it need not cause a new snapshot. Only authoritative topic
   data changes the displayed value. No synchronous wait on the UI thread.
5. Revalidate boot/link and target ID on topic/link/pending completion. Losing
   eligibility, changing target or retiring Runtime invalidates its UI intent:
   cancel queued work through the existing ticket; dispatched work may already
   happen but its late reply cannot affect a replacement target/runtime/boot.
   Never retarget a request, replay, retry automatically, spawn fallback or
   reinterpret uncertain dispatch as definitely not sent. Preserve shared Client
   responsiveness; existing ticket queue/deadline/admission limits unchanged.
6. Commit small logical local commits with approved author+committer. Do not push,
   start CI/build/tests or access the lab during Nacre's heavy grant. Local source
   checks only with existing tools/auto-install disabled; report missing tools.
   Root reviews source and grants required checks separately, then normal main
   integration/exact-head CI. Never force or include others' changes.

## Acceptance and evidence

Use existing private Runtime/fake-daemon fixtures, not solely projection mocks.
Required named tests (table subcases allowed):

- bluetooth_power_targets_exact_single_adapter: actual open Runtime sends exact
  method/ID/desired bool once; busy suppresses duplicates; Ret alone does not
  change checked; later authoritative Snap does; changed:false accepted.
- bluetooth_power_requires_fresh_unambiguous_adapter: zero/multiple/dangling or
  malformed adapters and stale/unhealthy state expose no control and send no call.
- bluetooth_power_unadvertised_method_stays_read_only: absent/wrong-version method,
  no Client or disconnected link cannot send, including direct host-action input.
- bluetooth_power_lifetime_never_replays: queued close, dispatched close/late reply,
  adapter replacement and boot replacement do not replay or update new state;
  assert exact dispatch count and same-link Ping/Pong/subsequent call where alive.
- bluetooth_power_terminal_failures_preserve_authoritative_state: Busy, uncertain
  dispatch, timeout and malformed result finish safely, fixed message/no flip,
  no fallback or automatic retry.
- bluetooth_power_legacy_preview_unchanged: existing no-client preview/render-state
  behavior and Network/Wi-Fi/Battery tests stay intact.

Verify the real popout/factory chooses the controlled template only with a Client;
legacy constructors must not silently opt in. Reuse existing inert-action schema
validation. Run focused tests, fmt and affected Clippy only when granted; final
exact-head hosted full CI is required for Done. No memory/latency claim.

## Stop and report

Stop for contract mismatch, new transport/framework API requirements, ambiguous
ownership or scope expansion. Report source/head/commits/files, exact tests and
commands/output, unrun checks, exact CI link if any, open question and cleanup.
Missing checks are NOT RUN. Keep capability-gated unavailable truthfully until
an actual daemon advertises the producer; never enable live units.

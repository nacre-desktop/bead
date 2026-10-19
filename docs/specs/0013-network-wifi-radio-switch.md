# B0013 — Capability-gated Wi-Fi radio switch

- ID: B0013
- Repo: nacre-desktop/bead
- Implementer: BEAD (Luna, source/private tests; Sol review of actor lifecycle)
- Status: Ready; source implementation may proceed during F4 captures
- Depends on: B0009 Done; published reserved Nacre5f4c0bf3 contract
- Date: 2026-10-19
- Size: one focused unit, maximum3hours before reporting a gap

## Goal

Add ONE Wi-Fi radio switch to the daemon-backed Network popout. It stays
unavailable until the connected daemon actually advertises `network.set_wifi`
v1 and supplies healthy fresh network state. Reserved schema publication alone
does not enable it or imply a running producer.

## Context and scope

Start current main containing7a04d31. Published contract already pinned in
Cargo.lock: `network.set_wifi`, params exactly `{enabled:bool}`, success
`CoreResult {changed:bool}`; `NetworkState.wireless_enabled` is authoritative.
Use core-services.md and published protocol/schema/core-service-call.json and
core-service-result.json at5f4c0bf3. No pin/schema/producer/new capability, direct
D-Bus fallback, scan/connect/passwords, Bluetooth/battery control, dependency,
framework/backend change, live access, installation or measurement.

Files: crates/nacre-shell/src/{nacre_source.rs,nacre_source/tests.rs,
status_views.rs,status_views_tests.rs,reload.rs,popouts.rs}, new private sibling
test module if useful, and shell/status/network.luau. Keep status/main.luau and
legacy/no-client Network, Bluetooth and Battery behavior unchanged.

## Changes and steps

1. Add a nonblocking ticket API on the EXISTING Client IO actor, not a new
   runtime/thread/task per click. Concrete API in nacre_source.rs:
   `pub struct CallGuard { pub boot:String, pub version:u16 }`;
   `pub type CallWake = Arc<dyn Fn()+Send+Sync>`;
   `Client::begin_call(&self,method:String,params:Value,duration:Duration,
   guard:CallGuard,wake:Option<CallWake>)->source::Result<CallTicket>`;
   `CallTicket::try_take(&mut self)->Option<source::Result<Value>>` (one terminal
   result; empty means pending, closed sender maps to unavailable). Reuse current
   duration/method validation, bounded `try_send(Command::Call)`, oneshot and
   actor deadlines. Begin returns immediately with no Tokio caller context.
   Keep queue capacity and pending limit MAX_CALLS unchanged. Do not refactor
   existing async call/call_id into tickets: they retain existing cancellation
   and dispatch semantics.
2. Extend Command::Call/Pending with optional ticket dispatch state and a small
   completion sender wrapper. Async callers use no guard/no wake. Ticket state
   is shared AtomicU8 Queued/Dispatched/Cancelled. Add private Shared transport
   generation AtomicU64, incremented on every disconnect; begin_call captures it
   before checking link.connected/expected boot, rejecting disconnected input.
   Ticket guard carries that generation internally. Actor checks generation,
   deadline, session.boot==guard.boot, and exact `session.methods.get(method)` with
   version==guard.version before admission/encoding. Missing/mismatched method
   returns unavailable, never substitutes another method. Immediately before
   pending insertion/send, actor CAS Queued->Dispatched; ticket Drop CAS
   Queued->Cancelled. Drop wins => no wire Call; actor wins => request may already
   take effect, drop only abandons reply, never sends wire cancellation or rolls
   back. This CAS is the dispatch boundary even if subsequent send fails. Keep
   sent abandoned calls within existing pending/deadline budget until terminal.
   Check guards only for tickets; do not skip ordinary calls whose receivers
   were dropped. Never replay after reconnect, including same-boot reconnect.
3. Completion wrapper sends/stores terminal result BEFORE taking/invoking its
   optional wake exactly once. Route all ticket exits through it: Ret, expiry,
   unavailable disconnect/handshake/backoff, encoding/admission failure, shutdown.
   Wrapper Drop closes sender then invokes remaining wake, so actor exit also
   wakes receiver-closed completion. Catch callback panic to protect IO actor.
   Admission failure returned synchronously creates no ticket and needs no wake.
   The callback only queues `UiEvent::Model(BTreeMap::new())`; it never captures
   Runtime/Rc/session or changes stores. A queued wake after retirement is safe.
4. Add `Status::core_network_control(source:Arc<dyn Source>,client:Client)` and
   `Runtime::with_core_network_control(source,client)`. Retain existing read-only
   constructor for tests/other callers. In popouts::status factory only daemon
   Network chooses `status/network.luau`, leases network on open, and passes a
   clone of the EXISTING shared Client. No preopen lease/control/ticket. Status
   owns optional private `WifiControl {client,pending:Option<(boot,CallTicket)>}`
   in RefCell; Runtime owns its Rc and existing topic subscription. Add optional
   status link subscription in Runtime; `Status::subscribe_control_link(wake)`
   subscribes to Client Source only for controlled mode, so actual Welcome and
   disconnect changes wake projection even without a network Snap. Drop both
   subscriptions, controller/ticket and lease with retired Runtime. Existing
   close grace/quick reopen lifecycle stays intact.
5. Add `Status::wifi_action(wake:CallWake)` and register/authorize EXACT host
   action `network.toggle_wifi` under private Registry grant `network.control`
   only in with_core_network_control. Runtime::stage consumes this action only
   for Click or Enter, before normal Session handling; ignore scroll/context/
   drag. Call wifi_action then session.update(empty) and finish_scene to show
   busy/error immediately. Runtime services supplies completion wake from its
   existing event channel. Registering this UI host action is not a daemon
   capability advertisement. No Source::invoke or async blocking on UI thread.
6. One helper reads a coherent candidate each projection/action: current leased
   source snapshot, narrow valid NetworkState, Client link snapshot/status.
   Eligible iff topic/link connected, health.available&&!health.stale,
   topic.epoch==status.boot==link.data.boot, and exact Welcome.methods key
   `network.set_wifi` has v1. Never infer capability from reserved schemas,
   Source::invoke, similarly named methods or topic presence. Recheck immediately
   on action, then pass captured boot/version guard to actor. If already busy,
   ignore duplicate action. Derive `enabled = !current.wireless_enabled`, send
   once with params exactly `{enabled}`, timeout=min(advertised timeout,10000ms).
   Set busy only on accepted ticket. Failed enqueue leaves busy false.
   Existing Session.methods comes directly from Welcome; sync_leases restarts
   topic Publisher epoch with Session.boot. Compare status.boot.as_deref() and
   link.data["boot"].as_str() against topic.epoch.as_str(); the link Publisher's
   own epoch is local transport identity and MUST NOT be compared to daemon boot.
7. Every controlled projection first discards ticket on disconnect/boot mismatch
   (busy false), otherwise polls try_take. Every terminal reply clears busy.
   Decode successful payload narrowly as published CoreResult; changed true or
   false is acknowledgement only. Never modify wireless_enabled/cache from a
   request or reply. Ret-before-Snap keeps old checked value. No retry/replay,
   forced resync/readback RPC, new timer, revision fence or waiting-for-Snap busy
   state. A later DELIBERATE click is allowed after terminal outcome and must
   rerun the healthy-state/capability checks; absence of a subsequent Snap must
   not strand changed=false or failure. Old ticket result has no controller in
   a new Runtime; an old callback may only wake its event channel.
8. Controlled projection adds `network.wifi_checked` (last healthy authoritative
   bool; false placeholder before first data), `network.wifi_enabled`
   (eligible&&!busy), `network.wifi_busy`, `network.wifi_message`. Keep B0009
   last-good status text semantics. Fixed messages: unavailable => "Wi-Fi control
   unavailable"; busy => "Changing Wi-Fi…"; successful terminal => empty; any
   error/malformed success/enqueue failure => "Wi-Fi change failed". Error stays
   until next deliberate eligible action or new boot; never expose daemon error
   text/params. Unavailable/busy take display priority over stored error.
   Dedicated network.luau preserves310x222 geometry: same heading/summary, detail
   height24, then36px row labeled "Wi-Fi", kit.Switch id="wifi-radio" checked
   binding and fixed aria_label="Wi-Fi radio", plus20px message line. Disable
   by omitting on_activate entirely when !wifi_enabled (current kit has no
   disabled API); use onSurfaceVariant unavailable styling. No local toggle
   store binding or action argument changes checked state. Cold fallback stays.

## Acceptance criteria and evidence

- [ ] Private ACTUAL Runtime/popouts factory with explicit fake Unix daemon
  verifies `wifi_runtime_requires_exact_capability_and_fresh_topic`: absent
  method, misleading name, v2, disconnected, stale/invalid state, mismatched
  epoch and no topic cannot send. Healthy v1 sends both true and false params
  exactly in `wifi_runtime_sends_enabled_once_and_ignores_busy_duplicate`.
- [ ] `wifi_runtime_ret_before_snap_never_changes_checked` covers changed true
  AND false: busy clears on Ret, checked changes only on later healthy topic;
  no extra Call without click. `wifi_runtime_failures_disconnect_never_replay`
  covers policy/unavailable/timeout/malformed success, disconnect and boot
  replacement; no replay and explicit later click uses newly read current state.
- [ ] `wifi_runtime_retire_reopen_isolates_old_reply` drops actual Runtime while
  request pending, verifies lease Unsub/callback release, opens replacement,
  delivers old Ret then new Snap: old completion cannot clear new busy/error/
  checked state. Shared client remains usable via Ping/Pong. Keep peer alive
  with existing fixture shutdown synchronization; no elapsed memory claim.
- [ ] Actor private tests `call_ticket_reply_and_timeout_wake_once`,
  `call_ticket_drop_before_dispatch_sends_nothing`,
  `call_ticket_drop_after_dispatch_does_not_cancel_or_replay`,
  `call_ticket_guard_rejects_changed_boot_or_method` (also same-boot reconnect
  transport generation),
  `call_ticket_disconnect_shutdown_and_drop_wake_once` cover immediate poll,
  terminal wake with no topic updates, cancellation CAS ordering via explicit
  test barrier (no sleeps), bounded admission and callback panic protection.
  Existing async dropped-receiver dispatch behavior remains covered unchanged.
- [ ] Preserve Network/Bluetooth/Battery projection, lazy lease, source/client
  deadlines/admission and placement tests; add dedicated controlled-template
  parse/hit-region check proving unavailable switch has no action. Test public
  UI event path, not only controller helpers. fmt, affected private tests and
  nacre-shell all-target Clippy must pass. Report exact commands/results.

## Validation boundary and stop conditions

This spec authorizes source/private-test work, not a heavy job or hosted full CI
during F4. Coordinator grants any build/test slot separately. No lab/host probe,
capture, live desktop, package installation, Nacre edit or measurement; B2/B3
remain gated and frozen artifacts unchanged. Follow AGENTS.md and standing rules,
approved author+committer, isolated worktree, no force push. Implementer makes
local source commit and reports to coordinator; no push until lead review and
CI grant. Implementation integration/full CI requires coordinator scheduling.

Stop for missing published contract, need to change framework/schema/pin,
unclear epoch consistency, inability to prove actor completion/cancellation,
test requiring a live producer, or timebox exhaustion. Report exact gap rather
than broadening controls or changing unrelated async semantics. Final report:
start/final hashes, files, named test evidence, pending CI/slot, unavailable
producer limitation, owned process cleanup, and no measurement claim.

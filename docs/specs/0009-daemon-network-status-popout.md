# B0009 — Read-only daemon network status popout

- ID: B0009
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Review
- Depends on: published network contract at Nacre5f4c0bf3; Bead B0008 Done
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

Display the daemon's structured network state in the existing Network popout.
Choose the explicitly identified primary connection, show real signal readback
when known, and retain stale last-good display across disconnects. This prepares
the core-switch consumer while Nacre N0003 implements the published producer.

## Context

Start latest main containing0deb314c1e82a2d798a1009184be02cde8571ce4. Inspect
status_views.rs/status_views_tests.rs,popouts.rs,main.rs,reload.rs and existing
lazy dashboard Choice/client.lease_topic implementation. Network currently uses
bead::services::Desktop summary through DesktopView. Published nacre-protocol
core_services::NetworkState/Entities/Health and CoreSnapshot::valid define the
consumer contract, including optional primary_connection. Reserved schemas do
not advertise a running service; no new capability or producer claim is allowed.

## Scope

In: existing Network popout source choice/narrow projection/private tests, bounded
last-good display cache. Bluetooth/battery and no-daemon preview remain unchanged.
Files: crates/nacre-shell/src/status_views.rs,status_views_tests.rs,popouts.rs,
main.rs,reload.rs only for the private product constructor; small existing fixture
files and source lease tests as needed. No Bead-framework/backend/Nacre edits,
new dependencies/schema/pin, mutators, scanning, credentials, live reads or capture.

## Steps

1. Add an explicit typed-network constructor/mode to product Status and its
   Runtime attachment. Do not guess format by falling back from malformed typed
   JSON into the legacy summary. Decode only NetworkState directly, then apply
   CoreSnapshot::Network(state).valid(); never deserialize the entire CoreSnapshot
   enum or unrelated topics. Enforce existing2048/ID/reference/strength rules;
   invalid data is unavailable/stale, never healthy zero or disconnected proof.
2. Pass Option<nacre_source::Client> from main's existing _ui_provider to popouts
   status configuration. Only for Kind::Network when this client exists, acquire
   client.lease_topic("network") INSIDE panel factory at first open, and attach
   the typed mode. Keep existing shared connection, no new daemon client, thread,
   polling or discovery. Closed factory retains configuration only. When no
   client exists use existing DesktopView legacy preview unchanged. When client
   exists but topic is not advertised, show unavailable; do not silently fall
   back to system D-Bus/direct Desktop network reads.
3. Typed projection uses existing status store keys/layout/310x222 geometry.
   Ready available state: primary is NetworkState.primary_connection ONLY. If
   present, use that connection's display name (or "Connected" if empty), never
   first map/order active profile. An active access point may supply SSID/signal
   only if its connection equals primary; choose first matching access_points.order
   entry. Prefer nonempty matched AP name for primary text, else connection name.
   Without primary show "Not connected", even if another profile is marked active.
   Secondary uses actual wireless_enabled =>"Wi-Fi on"/"Wi-Fi off". Detail is
   "Signal strength N%" only for that matched AP; absent signal => existing
   "Connection controls unavailable" (never fake0%). ready=true; active=primary
   present OR wireless_enabled. Reuse existing line/control-character sanitizing.
4. Keep one small view-owned last-good PROJECTED display map, not duplicate full
   network state. Refresh only on validated health.available && !health.stale &&
   snapshot.connected. On stale health, unavailable, invalid payload or transport
   disconnect, preserve last-good primary/detail/active, set ready=false and
   secondary="Status stale". Without last-good use existing Network unavailable
   defaults. No error payload/private path in text/log. Recovery replaces cache
   atomically. Cache and topic lease drop with Runtime retirement; no global cache.
   Do not add controls/buttons or change existing Bluetooth/battery semantics.
5. Add private tests named network_core_primary_is_explicit_not_map_order,
   network_core_signal_requires_primary_ap_mapping,
   network_core_unknown_or_invalid_state_never_becomes_zero,
   network_core_disconnect_retains_last_good_until_recovery,
   network_core_lease_exists_only_for_open_runtime,
   network_legacy_preview_remains_unchanged. Include wired primary with active
   Wi-Fi secondary, omitted primary, mapped0% signal, missing/dangling IDs,
   invalid2049collection and strength101, missing advertised topic, text controls.
   Use FakeSource/existing private fake-daemon and Runtime ownership proof. Keep
   providers alive while dropping Runtime; verify lease/subscription release and
   no callback afterward. Do not claim elapsed2.2s or measured raster reclamation.
6. Validate affected status/source/lifecycle tests,fmt,nacre-shell all-targetClippy
   in independent existing environment or hostedCI. No access to active Nacre lab.
   Integrate normally with approved identity, exact-head full CI. This is safe
   consumer code against already published schemas; runtime capability remains
   unavailable until producer publication. No daemon pin update or install.

## Measurements

None now. No memory/latency gain claim. Existing B0002 uses frozen1cff release
artifact, unchanged by this source task. Later final core-switch runtime needs
its own matched lifecycle gate. Capture safety remains required: explicit slot,
peer pause,>=4GiB available,abort<2GiB,private capped scope; no host overlap.

## Acceptance criteria

- [x] Existing Network popout lazily consumes shared daemon network lease.
- [x] Narrow validated decoding; explicit primary/AP mapping, no fabricated signal.
- [x] Unsupported/stale/disconnected/invalid data honest; recovery atomic.
- [x] Only bounded display cache; release callbacks/leases at Runtime drop.
- [x] Legacy no-client preview and Bluetooth/battery unchanged; no network action.
- [x] Named private tests,fmt,Clippy and exact-head full CI green.

## Implementation evidence

The implementation is `b58041a` (following start commit `be4305c`); exact-head
hosted CI run `39132356640` passed. The named projection tests verify explicit
primary selection rather than map order, AP signal only when its connection
matches primary, malformed/unknown data never becoming zero, stale disconnect
retention and atomic recovery, and unchanged legacy preview. The private fake
daemon test verifies the network subscription is absent before opening, added
when the panel Runtime opens, and removed when it drops while the shared client
remains usable and no later callback is delivered. Validation ran in hosted CI;
no local build, lab session, capture, memory measurement or latency claim was
made. The code consumes the published schema but does not advertise runtime
availability; the view stays unavailable until Nacre publishes the producer.

## Stop conditions

Published schema lacks needed field, client lease cannot preserve lazy ownership,
real producer differs, need a protocol/pin/new thread/action/direct bus, or3hours
exhausted: report exact gap; do not invent capabilities. No hardware prerequisite
or wayland-info is required for this source/private-test task.

## General rules

Apply standing rules. Approved author+committer siverteh
<92440237+Siverteh@users.noreply.github.com>; no private paths/inventory. Isolated
worktree/direct normal main integration, no force push, rebase own only, checkCI.
nice10/Cargo-j2/private tests. No live desktop,installedNacre/config/keybinding /
autostart/systemd/publishing/AI permissions/paru flag changes. Stop only owned PIDs;
never pattern-kill or touch Codex/app-server/others. Framework permissive and
publish=false; GPL nacre-shell only, backends frozen except fixes, no ranking /
provider engine port. Extras report-only. Advisories follow standing delegation;
vulnerability/unsoundness to owner. No implicit measurement grant.

## Report format

B0009 Review: start/final commits, changed files, named test/CI evidence, fixture
primary mapping and lease/drop proof, capability-gated runtime limitation and
cleanup. Keep private raw paths outside Git; hosted-only validation stated honestly.

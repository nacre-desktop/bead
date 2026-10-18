# B0010 — Read-only daemon Bluetooth status popout

- ID: B0010
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready
- Depends on: B0009 Done; published BluetoothState contract
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

Show structured daemon Bluetooth status in the existing Bluetooth popout. Keep unavailable and stale states honest while preparing this core-switch consumer for the daemon producer.

## Context

Start latest main containing bd7bf56. Reuse B0009 status_views.rs, status_views_tests.rs, popouts.rs, reload.rs and shared client lease fixtures. Published nacre-protocol core_services::BluetoothState has health, ordered adapters(name,powered,discovering) and devices(adapter,name,connected,paired,trusted,pairing,battery?,icon?). CoreSnapshot::Bluetooth.valid() owns structural validation. No producer availability is implied by a reserved schema.

## Scope

Only existing Bluetooth popout typed source/projection, bounded display cache and private tests. Same B0009 file boundaries, small source lease fixture helpers allowed. No controls, pairing, discovery requests, direct bus fallback, new daemon client, schema/pin/dependency/backend changes, layout or golden changes. Network and Battery behavior stay covered by regressions.

## Steps

1. Add explicit typed Bluetooth constructor/attachment. Replace the network-mode bool with a small explicit private mode enum if needed; do not infer mode from JSON. Decode BluetoothState only and validate through CoreSnapshot::Bluetooth(state).valid(). No broad snapshot enum decode.
2. Inside Bluetooth panel open factory, use the existing optional shared client to lease_topic("bluetooth"). No lease before open. With a client but absent capability show unavailable; with no client retain existing DesktopView preview. Runtime owns lease/subscriptions/cache and releases them on drop. No new connection, watcher thread or poll.
3. For connected transport, validated available nonstale data: powered means any adapter powered. Connected device names come from devices.order entries with connected=true whose referenced adapter is powered; ignore paired-only devices. Primary is "Bluetooth off" if none powered; otherwise first connected name (empty name becomes "Connected device"), or "No connected devices". Secondary is "Bluetooth on" or "Bluetooth off". Detail is "N connected devices" when N>0, otherwise "Connection controls unavailable". active=powered, ready=true. Sanitize/truncate primary/detail using existing line helper. Do not invent a preferred adapter, battery percentage, action or signal strength.
4. Match B0009 freshness policy: cache only small projected primary/detail/active after valid healthy connected input. Invalid, unavailable, stale or transport-disconnected input retains this display with ready=false and secondary="Status stale". Without cache use Bluetooth unavailable/default unavailable secondary. Recovery atomically replaces cache. Never retain full decoded state or a global cache.
5. Add bluetooth_core_power_and_connected_order (multiple adapters, off-adapter connected device excluded, paired-only excluded, order differs from map, empty/control-character names); bluetooth_core_invalid_is_not_powered_off (dangling adapter,2049 collection, battery101, missing advertised topic); bluetooth_core_disconnect_retains_last_good_until_recovery; bluetooth_core_lease_exists_only_for_open_runtime; bluetooth_legacy_preview_remains_unchanged. Lease fixture must prove no preopen Sub, open Sub, Runtime-drop Unsub/no callbacks, and same shared connection Ping/Pong afterward. Use existing peer keepalive and explicit shutdown synchronization. No elapsed retirement or driver-memory claim.
6. Run affected private tests, fmt, helper status/source regressions and nacre-shell all-target Clippy via existing hosted CI; no shared lab access required. Integrate normal fast-forward main, verify exact-head full CI, fix task-caused failures. Preserve B0009 tests.

## Measurements

None. No capture/host probe, memory or latency claim. Frozen B0002 artifacts unchanged. Future measurements require explicit slot, peer pause,>=4GiB MemAvailable start/abort<2GiB, private capped scope and verified payload containment; stop only owned processes.

## Acceptance criteria

- [ ] Existing Bluetooth popout lazily leases shared daemon topic with narrow validated decoding.
- [ ] Deterministic powered/connected mapping and exact strings above; no fabricated battery/action.
- [ ] Last-good projection survives stale/invalid/disconnect and recovers atomically.
- [ ] Private fixture proves Runtime lease and callback retirement; shared client stays usable.
- [ ] Legacy preview, Network and Battery regressions pass; exact-head CI green.

## Stop conditions

Missing contract fields, incompatibility with existing validator/lease lifecycle, need a schema/pin/action/new backend, or3hours exhausted: report exact gap rather than widen scope. No lab prerequisite blocks this source task.

## General rules

Follow linked Nacre standing rules. Approved author+committer siverteh <92440237+Siverteh@users.noreply.github.com>; isolated worktree, normal direct main integration, never force push, rebase own commits only, check CI. Commands nice10/Cargo-j2/private tests. No personal paths/inventory in commits; no live desktop/config/install/units/keybindings/publishing/AI-permission or paru flag changes. No killing other processes or host pressure/cache/sysctl changes. Framework permissive dependencies, publish=false, GPL shell only; backends frozen except fixes; Bead never edits Nacre. Advisory delegation applies; vulnerability/unsoundness goes to owner. Extras report-only.

## Report format

B0010 Review: start/source/final commits, changed files, named test and exact-head CI evidence, powered/device-order and lease/drop proof, remaining capability gate, cleanup and no measurement claim.

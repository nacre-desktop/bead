# B0011 — Read-only daemon Battery status popout

- ID: B0011
- Repo: nacre-desktop/bead
- Implementer: BEAD
- Status: Ready
- Depends on: B0010 Done; published BatteryState contract
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

Complete typed daemon readback for the existing Battery popout. Preserve missing percentage and charging as unknown independently so the UI never fabricates zero or discharge state.

## Context

Start latest main containing a379eda. Follow B0009/B0010 implementations in status_views.rs, status_views_tests.rs, popouts.rs, reload.rs and nacre_source/tests.rs. Published nacre-protocol core_services::BatteryState contains health, percentage:Option<f64> and charging:Option<bool>; CoreSnapshot::Battery.valid() validates finite0..100. No power-profile selection, device presence, remaining time or producer availability can be inferred from this contract.

## Scope

Typed Battery mode/projection, shared lazy topic lease and private tests only. Same files/fixture helpers as B0010. No new protocol/dependency/pin/backend/client/thread, direct D-Bus fallback, profile controls, actions, notifications, polling, layout/golden or Network/Bluetooth changes.

## Steps

1. Add explicit CoreBattery mode, Status::core_battery and Runtime attachment. Decode BatteryState only, validate via CoreSnapshot::Battery.valid(). Existing shared optional daemon client leases "battery" inside the Battery open factory, never before open. No advertised topic => unavailable without fallback. No daemon client => legacy DesktopView preview unchanged.
2. Healthy available nonstale connected state: percentage Some => primary format!("{percentage:.0}%"), including legitimate0 and100; None => "Battery level unknown". Secondary charging Some(true)=>"Charging", Some(false)=>"Not charging", None=>"Charging state unknown". ready=true even for individually unknown readings; active only when charging==Some(true). Detail="Power controls unavailable". Unknown percentage never becomes0; unknown charging never becomesfalse text. Do not classify absent battery or remaining time. Keep existing Battery heading/icon/kind and geometry.
3. Cache only projected primary/secondary/detail/active for validated fresh input. On stale/unavailable/invalid/disconnect retain last-good primary/detail/active but override secondary="Status stale",ready=false. Without cache use existing Battery unavailable/default unavailable secondary. Fresh recovery replaces cache atomically. Cache/lease/subscriptions retire with Runtime, no global/full-state cache. Minor private cache helper reuse permitted; no general projection framework or unrelated refactor.
4. Tests: battery_core_optional_readings_remain_unknown (all four Some/None combinations, true/false charging, actual0,100, fractional rounding12.4=>12 and12.6=>13); battery_core_invalid_is_not_zero (negative,101,string/nonfinite JSON if representable, missing health); battery_core_disconnect_retains_last_good_until_recovery (invalid,unavailable,stale,disconnect and fresh recovery); battery_core_lease_exists_only_for_open_runtime; battery_lease_without_advertised_topic_stays_unavailable; battery_legacy_preview_remains_unchanged. Fake daemon proves no preopen Sub, open Sub, drop Unsub/no callback and shared-link Ping/Pong, tolerating keepalives with explicit shutdown synchronization. Retain Network/Bluetooth tests.
5. Run affected private tests,fmt,nacre-shell all-target Clippy and full exact-head hosted CI. Hosted-only validation acceptable with explicit report. Integrate normally to main, approved identity, fix task-caused failures. No lab access, install or producer claim.

## Measurements

None; no memory/latency claim. Frozen B0002 artifacts unchanged. Future capture requires separate slot, peer pause,4GiB available start/abort below2GiB, verified private capped scope and external monitor; stop only owned processes.

## Acceptance criteria

- [ ] Narrow BatteryState validation and lazy shared battery lease.
- [ ] Missing readings remain independently unknown; legitimate0 is displayed correctly.
- [ ] Stale/invalid/disconnected display retains only bounded last-good projection, recovery atomic.
- [ ] Runtime drop releases lease/callbacks while shared client stays usable; missing topic honest.
- [ ] Legacy preview and Network/Bluetooth regressions pass; exact-head full CI green.

## Stop conditions

Need protocol fields, different topic shape, a new action/backend/client/dependency or3hours exceeded: report precise gap rather than invent behavior. Missing private lab tools do not block this source/hosted-test task.

## General rules

Standing rules apply. Approved author+committer siverteh <92440237+Siverteh@users.noreply.github.com>; isolated worktree, normal direct main push/checkCI, no forcepush, rebase own commits only. nice10/Cargo-j2/private tests. No private paths/inventory, live desktop/installedNacre/config/keybindings/autostart/systemd/publishing/AI-policy/paru flag change. No killing other processes, host pressure/cache/sysctl changes. Framework permissive/publish=false, GPL shell only, backends frozen except fixes, Bead never edits Nacre. Informational advisory delegation per standing rules; vulnerabilities/unsoundness owner-only. Extras report-only; no implicit host/capture grant.

## Report format

B0011 Review: exact start/source/final commits, changed files, optional-value projection proof, lease/drop/missing-capability tests, exact-head CI, validation limitations and cleanup; no measurement claim.

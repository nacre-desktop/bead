# Status

## 2026-10-22 — B0002 desktop relative gate passed

Desktop Hyprland and niri both passed the owner-relative gate. Candidate-minus-control medians were +0.121/+0.134/+0.219/+0.047 MiB for Hyprland and +0.237/+0.249/+0.229/−0.998 MiB for niri; anonymous deltas stayed below +1 MiB. DRM was unknown and excluded. Candidate 306474f0 / UI 7948c3c. B3 measured stage is unblocked when the lead grants the desktop heavy slot. See [the batch report](research/b0002-desktop-batch-20261021.md).

## 2026-10-20 — B0002 Hyprland capture stopped at combined-legacy

A1 passed all four settled gates after a strict postcheck schema correction; its raw evidence is unchanged. B1 passed the first three settled phases but stopped at the required combined-legacy endpoint: 24.541992 MiB against the then-active24.5MiB single-run stop line. The endpoint was persisted before the assertion; progress.json therefore has only three completed rows. This is a valid historical stop, not a completed A/B pair or product-regression finding. Both arms used the same frozen UI, helper, fixture tree, and lifecycle-only menu-engine route. On 2026-10-20 the owner authorized a fresh20-arm batch with explicit per-phase medians and unchanged quiet/safety/swap gates; the source patch is pending lead review and no fresh arm has started. See [the capture report](research/b0002-hyprland-capture-20261020.md).

## 2026-10-19 — B0014 accepted

The Bluetooth popout exposes a power switch only for one fresh, valid adapter.
Method v1 advertisement enables it; absent/wrong-version methods keep that sole
adapter's switch disabled, while zero/multiple/invalid/stale adapters retain the
aggregate read-only view without a switch. Topic snapshots remain the only power
readback. Requests use the shared Client/CallTicket lifecycle without replay.

Reviewed source `dcf541a9f19d37b4ef493380664ff4ff1f5a123d` integrated by normal
fast-forward to main. Exact-source full hosted
[CI39240289443](https://github.com/nacre-desktop/bead/actions/runs/39240289443)
passed: formatting, all-target Clippy, workspace tests, feature configurations,
Luau/schema, capture regressions, private lifecycle and dependency/license checks.
All six named B0014 acceptance groups passed, including real Runtime/fake-daemon
eligibility, authoritative readback, queued/dispatched retirement, fenced late
replies, boot replacement, timeout/uncertain dispatch and legacy preview. The
actual factory test also passed. Shell tests: 164 passed, 0 failed, 1 ignored.
Duplicate automatic main [CI39241346554](https://github.com/nacre-desktop/bead/actions/runs/39241346554)
on the same source was canceled by lead instruction to avoid repeating checks.
The completion receipt is documentation-only with CI skipped. No private-host,
live desktop, install, measurement, producer availability or memory/latency claim.

## 2026-10-18 — B0011 accepted

The Battery popout validates published `BatteryState`, preserves percentage and
charging as independently unknown, and caches only bounded last-good display
fields through stale/disconnected input. Its topic lease exists only while the
Runtime is open. Private Battery/Network/Bluetooth lifecycle tests require actual
Sub, fresh typed data, drained startup callbacks, Unsub and consumed late data;
the shared desktop connection stays usable. Named optional-value, invalid-data,
recovery, missing-capability and legacy tests pass. Built-in source review is
accepted; exact-source `af61d18` passed full [CI39147491406](https://github.com/nacre-desktop/bead/actions/runs/39147491406).
Validation was hosted-only: shell138passed,0failed,1ignored. No lab, capture,
installation, memory/latency or producer availability claim; unavailable until
Battery is advertised. The final receipt is documentation-only with CI skipped.

## 2026-10-18 — B0010 submitted for review

The Bluetooth popout now leases the published topic only while its Runtime is
open. It validates `BluetoothState`, follows explicit device order, filters
powered adapters and caches only the last-good display fields. Private tests
cover invalid/missing data, recovery, legacy preview, lease retirement and the
shared link. Exact-head CI `39136586458` is green at `e3e417e`; validation was
hosted-only because no local Rust tools are installed. No measurement or
producer availability is claimed; the view stays unavailable until advertised.

## 2026-10-18 — B0009 accepted

The Network popout lazily leases the shared daemon topic and displays only a
validated, explicitly mapped primary connection. Stale data retains a bounded
last-good display; Runtime retirement releases the lease without closing the
shared client. Legacy preview behavior is unchanged. Named projection and
private fake-daemon lifecycle tests pass. The test peer tolerates interleaved
keepalive traffic and stays open through the shared-link assertion. Exact-head
CI `39133884809` is green at `5d96384`; validation was hosted-only. No lab,
capture, memory or latency claim. Runtime display remains unavailable until
Nacre advertises the topic.

## 2026-10-18 — B0008 submitted for review

Registered UI-provider overload now returns Busy/retryable/not-dispatched;
unknown or missing methods return UnknownMethod/not-retryable/not-dispatched.
The 64-call cap and no-replay behavior are unchanged. Three saturated-pool
socket tests, missing-provider coverage and B0007 regressions pass. Exact-head
CI `39127924726` at `b042e47` is green. No measurements or lab use.

## 2026-10-18 — B0008 in progress

B0007 was accepted Done at `736e778`. B0008 separates registered-method overload
from unknown methods without changing `MAX_CALLS`; work is source/tests only,
with no lab use or measurement.

## 2026-10-18 — B0007 accepted

UI-provider factory/future panics and task cancellation return one terminal
Internal error for the original request. The daemon link remains usable, and
session teardown drops task metadata without replay across boots. Four private
socket tests pass; exact-head CI39125066926 is green. No measurement or live
work occurred.

## 2026-10-18 — B0005 review submitted

Acceptance timing and hosted regression tests are published at `bb98474`; CI
`39079511622` is green. Seven fake-clock tests and syntax checks pass. No capture
ran and no measurement is granted. B0002 still lacks source-proven artifacts,
the `0192` ruling and an explicit slot grant.

## 2026-10-18 — B0002 artifact/preflight submitted

The current B1/B4 production runtime inputs are byte-identical. Available old
bundles are from earlier code, harness-dirty and incomplete; no source-proven
B0002 binaries or helper pairing are ready. Five pairs with full 60s phase-idle
fit about 94–124 minutes; 600s warmups raise this to about 4h54–5h24, so request
separate three-hour compositor windows if needed. No build or capture ran.
`0192` owner-policy and measurement grants remain blockers.

## 2026-10-18 — B0003 source-only preparation

The typed query path and clock boundaries are mapped in
[the source report](research/feature9-gate-d.md).
UI active-view work is the best-supported CPU candidate; transport, row decode
and compositor tails remain unresolved. No builds, product edits or measurements.
Capture requires B0001/B0002 completion and a separate exact-hash/window grant.
B0001 publication remains blocked on the owner ruling for inherited RUSTSEC-2026-0192.

## 2026-10-18 — B0004 dashboard review coverage fixes

B0004 closes ready-state default and primary disconnect evidence gaps from
B0001. Exact fixtures are checked before projection; all four page defaults and
primary-source stale retention pass, including Today's clock/store path. The
41-row inventory remains exact. CI39074426606 is green on `56210a6`; no runtime,
fixture, PNG golden or measurement changed. B0001 and B0004 are submitted for
spec review.

## 2026-10-17 — B0001 core dashboard completion

B0001 adds the five named deterministic dashboard proofs and a 41-file QML
coverage inventory at Nacre `5f4c0bf3`. Tests cover page defaults, four-tab and
legacy routing, unknown/stale source behavior, reachable clipped rows, and page
Runtime retirement. Retirement restores fake-source `Arc` counts and stops
callbacks while providers remain alive; Session state and CPU scene drop with
Runtime. GPU/raster memory is not measured. No page behavior or golden changed.
Media/player discovery, GPU metrics and reserved dashboard topics lack runtime
providers; unsupported optional cards remain deferred. No measurements run.
Validation: workspace tests 241 passed / 0 failed (3 ignored), trace memory tests
1 passed (1 ignored), fmt, warning-denied Clippy, service boundary, CPU-only and
all-feature checks, minimal facade, generated Luau (52 modules), UI check,
private service-owner proof and workspace/framework cargo-deny all pass.
[CI39072111811](https://github.com/nacre-desktop/bead/actions/runs/39072111811)
is green.

Coverage inventory: [dashboard-core-coverage.md](research/dashboard-core-coverage.md).

## 2026-10-17 — Scoped zbus advisory exceptions green

Nacre protocol remains pinned to published 5f4c0bf3, fully green in
[Source CI39052564401](https://github.com/nacre-desktop/nacre/actions/runs/39052564401).
Bead commit 3512dd7 adds only RUSTSEC-2026-0370/0371/0372 to both cargo-deny
configs. These informational unmaintained notices concern crates merged into
zbus 6; no vulnerability is reported. AccessKit, AT-SPI and tray still require
zbus 5. The comment links upstream and requires review when those consumers
support zbus 6 or by 2027-01-31. Workspace and eight framework cargo-deny checks
pass locally; [CI39069240648](https://github.com/nacre-desktop/bead/actions/runs/39069240648)
is green. The existing ttf-parser exception and all other deny rules remain.

Seven exact compatibility transcripts replay through Bead's Mirror; two new
consumer tests cover 7 snapshots / 29 patches. Published Luau checks pass 100
positive cases, 7 negative guards and 32 semantic assertions; original 30
fixtures stay byte-exact. Reserved contracts are not runtime capabilities.
No production consumer, UI, backend or helper behavior is changed.

236 workspace tests, trace, fmt/all-feature Clippy, CPU-only/release, service
boundary and 52 Luau modules pass. Both full native20 proofs pass: settled
H 23.756 / N 24.258 MiB, max closed 23.760 / 24.289; all 76 close/settled samples
fit own-bar + 5 MiB / 25 MiB and settled < 24.5. SwapPss 0; process audit 0.
File/anonymous/shared H 15.859/7.203/0.693, N 15.973/7.180/1.105 MiB.
Decoration SHM stays 542,592 B; executable +38,728 B (text +15,104 B).
Single fresh nested run/compositor with frozen75157 helper, no own build/test
overlap. Differences from earlier runs are not a paired regression or speed
claim. DRM/refresh remain unknown. [Report](research/feature9-gate-d.md#green-compatibility-protocol-pin--2026-10-17).
Published-head CI: [Actions](https://github.com/nacre-desktop/bead/actions).

## Four core dashboard tabs and stop point

Today/calendar, Weather, enrichment-only Media, and System with Updates have
lazy presentations, navigation and legacy tab/drawer routing. Page bundles,
state, subscriptions and rasters release after closed + idle. Concrete per-page
decoding stays in place. Eight PNG goldens are unchanged by this pin update.
Explicit labeled fixtures supply preview data; unadvertised real topics remain
unavailable. No MPRIS/player, backend or extra capability is invented.
The earlier five alternating pairs pass never/once/repeated-open phases;
repeated settled medians H 22.098 → 22.742 / N 22.672 → 23.264 MiB.
[Core-page report](research/feature9-gate-d.md#four-core-dashboard-tabs--measured-continuation-2026-10-17).

Advisory policy is resolved. Dashboard B0001 is in progress under its published
spec; later lifecycle/memory and typing work still require their own grants.
Optional tabs remain incomplete and unclaimed.
Optional tabs, customization/QML behavior and real provider hosting are not
claimed complete. QML stays daily driver; no later feature, live test, install,
config/unit/keybinding change or desktop job/slot. Private preview remains
prepared with immutable artifacts and explicit fixtures, not run live.
Revised compatibility/OSD schemas are adopted; Reader trim API is now published
but its application/cache gap stays open. Runtime hosting follows Nacre F4;
helper.execute is the actual compatibility method, with no guessed RPC.

Keep typing targets, CPU regression 29.61 → 32.66%, 6b pacing/idle, long chaos,
remaining caches, matched QML dashboard comparison and binary-name clash open.
No brush optimization ships: the prior exact-pixel screen had mixed CPU direction.
CPU small views / GL large animation keeps the ~20 MiB Mesa-residue policy.
Core forecast stays 40–81 eight-hour agent-days (320–648 task hours), practical
3–8 calendar weeks at 16–20 task hours/day availability plus contract/hardware
gates. Full-core idle 90–150 MiB is an estimate. Further IME/reader work and live
validation remain deferred/held; the 25 MiB ceiling / 24.5 MiB stop are unchanged.

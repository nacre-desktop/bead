# Status

## 2026-10-22 — B0025 per-output sessions accepted

Each output now owns UI state/update/commit/input/teardown with shared services,
fonts/assets and immutable palette data. Old-scale caches/pools and idle spare
frames release; allocator slack is returned after quiet UI generations.
Matched CPU PSS deltas: Hyprland one -0.992 MiB; niri initial/1.25/return
-1.401/-0.880/-1.017 MiB.
Extra-output cost +1.862 MiB passes ~3 MiB. Named counters explain
0.678 MiB of 1.033 MiB extra live heap, leaving ~0.356 MiB unclassified.
Full Arch Bead CI and private lifecycle/input/idle/scale proofs pass. Niri's
single Winit output has no supported private hotplug route; that limitation
remains explicit. B0022's architecture blocker is cleared; its action-contract
and remaining consumer proofs stay separate. See [B0025](specs/0025-per-output-ui-sessions.md).
No installed-shell change or live session.

## 2026-10-22 — B0022 bar composition and lifecycle

At the B0022 spec start, Wayland already created/removed primary bar surfaces
per output and updated their geometry/scale, while the bar model was global.
B0025 now provides accepted session isolation. Published protocol v1
still has no daemon routes for workspace,
focus, session, calendar peek, status adjustments, or tray actions. B0022 is
therefore partial; see [spec 0022](specs/0022-bar-composition-lifecycle.md)
for the versioned action matrix and contract gates. Existing reconnect tests
fence stale daemon calls; the ignored private tray-bus supervisor covers bus
loss and item replacement. A nested bar restart/multi-output smoke is still
needed. No new action route was invented.

## 2026-10-22 — B0021 wallpaper memory fixed; review

[B0021](specs/0021-static-wallpaper-surface.md) memory diagnosis is complete at
`ac27ca3`; full Bead CI passed in the prod Arch container. The earlier
26.64 → 59.79 MiB comparison changed CPU-only release to native GL release.
Matched CPU main/pre/post/fixed captures passed privately on both compositors.
The companion decoder removes settled GdkPixbuf/Glycin residency: fixed UI PSS
is 9.340 MiB on Hyprland and 19.526 MiB on niri, +0.251/+0.619 MiB over main
after excluding niri's additional 9.889 MiB compositor-held wallpaper SHM.
niri does not release that buffer; its documented safe fallback remains.
A private wallpaper switch returned to 10.069 MiB after a 78.653 MiB UI-only
transient peak. See [FEASIBILITY](../FEASIBILITY.md) for mapping/heap evidence,
matched flags and limitations. Ship the companion beside the UI binary.
Native GL's absolute memory is a pre-existing acceptance issue. Centre remains
unsupported. Stop for review; no install, live desktop change or push occurred.

## 2026-10-22 — B0020 producer published; default-action contract gap

[B0020](specs/0020-notifications-osd-product.md) now pins Nacre producer source
`f468060038edfc33b99a1b06bf773ea9ffd90b7e`; N0017 phases A+B and owner policy
decisions clear the former producer/policy gate. Zero-timeout popup hiding uses
the configured QML default; dismiss retains and explicit Forget deletes. History
is bounded with independent storage health. Keyboard-layout OSD is deferred.
Implementation remains blocked on a narrower published-contract gap: actions
have opaque IDs and labels, but no default-action marker/reference. QML needs it
to filter buttons and optionally invoke the default on card click. Do not infer
it from action order/labels/IDs. Upstream must publish and test the additive
metadata before repinning; no producer edit or partial consumer was integrated.
Product line delta is +0/-0. No nested test, memory delta, install or live change.
Full `.github/workflows/ci.yml` equivalent ran on source `4434768` in the
PROD Arch container: fmt, warnings-denied Clippy, workspace/trace tests, renderer
configurations, facade/headless/Luau checks, capture regressions, private services
and SNI/DBusMenu lifecycles, and all cargo-deny graphs passed. The first archive
transfer omitted Git metadata, failing two revision-dependent Python suites;
after restoring the exact Git commit, their reruns passed 10/10 and 8/8 tests.
All 28 checks passed across the full run and those environment-only reruns.
This receipt changes documentation only; it is not a product-parity claim.

## 2026-10-22 — B0019 launcher review candidate

Source `c8b55b5` adds asynchronous daemon queries, same-scope pending rows,
route/epoch fencing, favourite stars and published pin/unpin, ordered catalogue
results and group routes, keyboard activation, and focused preserved input on
reopen. Rows/tickets/actors retire after closed+idle; bounded scalars survive.
Full remote Arch CI and native private input/reopen passed on both compositors.

Niri's complete 200-key diagnostic measured 6.269 ms steady presented p99 and
25.016 ms first key; steady UI CPU p99 was 5.351 ms, first-key CPU 14.777 ms.
Hyprland's 200-key trace measured 9.871 / 25.214 ms, but its arm failed a new
post-window harness waiter; that raw arm remains invalid. The fixed two-key
contract smoke passed without repeating its timing window. Query/order identity
and RPC RTT now join through exact commit/presentation; target acceptance,
matched memory, physical timing and Gate D remain open.
The close-memory diagnostic points to allocator residency, with only +0.262 MiB
live heap; DRM remains unknown. See [B0019](specs/0019-launcher-parity.md) for
full numbers, failed-attempt receipts, raw provenance and limits. No cheap
rounded-painting change was identified. No push, live test, install or switch.

## 2026-10-22 — Laptop reference capture kit prepared

The [runbook](research/laptop-reference-capture-runbook.md) now provides a
single owner-signaled Niri command, with optional Hyprland timing. The artifact
manifest pins the recovered frozen control binary and matching frozen `shell/`
tree against the current main candidate. Exact-source Bead CI passed on dev-1.
The private desktop rehearsal completed all six Niri arms in 41m18s; geometry,
scope/OOM, host swap-in and close checks passed for every arm. Desktop DRM
fdinfo was unavailable, so its summary is **REHEARSAL ONLY** with PSS-only
relative deltas, not a laptop verdict.
Two earlier A1 rehearsal attempts stopped before valid windows while the frozen
shell bundle and UI-only fixture observer were corrected; their partial outputs
remain separate and invalid.
The laptop GPU path has not been inspected and no laptop capture was run; the
prepared reference capture still awaits the owner's signal and AC idle window.

## 2026-10-22 — B0018 status-pill routing complete

Source `c6ba4b3` groups the right-side icons in one pill and routes the four
existing device views through nonmodal hover peeks and compact top-right
control panels. Other icons retain hover/pointer behavior while peeks are
suppressed; native click switches content, pointer-away and Escape close, and
closed/idle resources retire. Shell code owns appearance and timing policy.
Requested B0017 was renumbered because 0017 already covers network controls.

Exact-source private nested CPU input and the full remote Arch CI equivalent
passed, including the existing matched QML placement/status goldens. Native
testing found missing surfaces for warmed peeks; input-bound review also fixed
shadow input over the bar. One existing watchdog deadline failed once; isolated
and complete unchanged reruns passed, without establishing a cause or relaxing
a deadline.
See [B0018](specs/0018-control-center-status-pill.md) for source/binary identity
and validation. The baseline has no edge-charge graphic; no edge activation,
new controls/providers, live switch, memory clearance, install or push is claimed.

## 2026-10-22 — B0016 tray preview ready for review; core producer gate

Source `6efca12` adds bounded headless SNI/DBusMenu behavior and a lazy bar menu.
Private-bus proofs cover duplicate/custom-path items, menu/icon updates, owner
loss, watcher takeover and session reconnect. Nested CPU input covers native
menu open/close, both wheel axes, middle/primary activation and idle retirement.
The shared dismissal-buffer configure failure found there is fixed. See
[B0016](specs/0016-system-tray.md) for validation and the remaining boundaries.
Core enablement needs published daemon hosting/action contracts; rows stay
hidden with a shared daemon client. UI PSS+DRM+SwapPss is unmeasured, so the
memory gate remains open. Current Nacre QML has no tray reference to compare.
No live session, deployment or push was performed.

## 2026-10-22 — B0003 measured stage

Five alternating clock-only/stage-enabled pairs per compositor and one valid
200-key stage diagnostic per compositor were captured with the frozen B2 UI and
real `menu.query` helper. All 10 pairs passed per-key trace, quiet, geometry,
scope, memory and cleanup gates. Stage observation added about 0.6ms UI CPU/key;
the first typed key remained the cold outlier. No product patch or target
acceptance is claimed. B0003 remains in Review because exact changed-item IDs,
query identity and RPC RTT are not present in the published markers. See the
[measured report](research/b0003-measured-20261022.md).

## 2026-10-22 — B0002 desktop relative gate passed

Desktop Hyprland and niri both passed the owner-relative gate. Candidate-minus-control medians were +0.121/+0.134/+0.219/+0.047 MiB for Hyprland and +0.237/+0.249/+0.229/−0.998 MiB for niri; anonymous deltas stayed below +1 MiB. DRM was unknown and excluded. Candidate 306474f0 / UI 7948c3c. See [the batch report](research/b0002-desktop-batch-20261021.md).

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

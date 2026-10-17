# Status

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

Advisory policy is resolved. Future dashboard implementation waits for a Ready
spec from the spec agent; optional tabs remain incomplete and unclaimed.
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

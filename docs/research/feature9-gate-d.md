# Features 9–10: active research

Owner authorized notifications/OSD, then the core launcher, stopping at Gate D.
No live-session test, notification-name takeover, installation or release is
allowed. The earlier single physical Gate C authorization was consumed.
Nacre and Orient remain read-only. This is an interim checkpoint, not Gate D.

## Typing foundation

The new proof uses 320 apps and one edit in flight. CLOCK_MONOTONIC timestamps
join keyboard receipt to the exact validated UI commit and presentation update.
Whole-process CPU includes application/driver/accessibility threads through the
presentation callback; compositor and injector CPU are separate. This measures
application input receipt, not physical keyboard latency. Profiles separate
computed bindings, markup evaluation/serialization, layout, fonts, shaping,
assets, baking and worker metadata.

| Trial | Hypr p99 presented ms | CPU median / mean ms per key |
| --- | ---: | ---: |
| Initial instrumented grid, 40 keys, state tracing | 55.49 | 41.35 / pending matched repeat |
| Same initial grid, nested niri60 | 49.59 | 42.76 / pending matched repeat |
| Windowed grid, 40 keys | 42.48 | 31.64 / preliminary |
| Native filter, 80 keys, numeric-only tracing | 582.92 | 17.65 / preliminary |
| Font selection cache, 80 keys | 24.04 | 12.85 / preliminary |
| Text-only candidate patch, 80 keys | 17.29 | 7.32 / 7.32 |

These are development trials, not a completed paired comparison. State tracing,
host contention and sample counts differ; preserve failed/contended runs. One
startup exceeded the unchanged 50 ms script limit while another task built
niri. The native-filter latency tail also worsened during contention. Do not
claim those timing differences as optimization wins. Repeat quiet matched
baselines and final code on both compositors. Existing nested niri is60 Hz;
120 Hz physical acceptance remains unverified. CPU p99 in the latest trial was
11.12 ms, including a cold font-selection key; the target is not accepted yet.
The historical 29.61→32.66% regression remains a separate, differently paced
measurement, not a percentage convertible from the new CPU-per-key proof.

Candidates retain the complete model/scroll extent but construct viewport rows
plus overscan. Immutable markup reuse tracks its actual model reads; templates
must be deterministic over bindings. Text-only patching preserves input boxes,
placeholder color and accessibility labels; structural bindings, hover, focus,
viewport or markup changes take full validation/layout. A bounded font-path
cache avoids file rescans and resets on reload. No font data is stored there.
Generation/worker ownership releases these caches with a closed panel.

## Private notification foundation

The server starts and owns a dedicated dbus-daemon/socket in a private0700
directory. It never discovers or connects to the live session notification bus.
It implements bounded replacement/actions/expiry/history, DND, connection-owned
inhibition, hover pause and close reasons. Inhibition changes emit
PropertiesChanged; the disconnect fixture caught a stale client cache before
that signal was added. API follows the [freedesktop notification protocol](https://specifications.freedesktop.org/notification/latest/protocol.html).

Both nested proofs pass standard protocol behavior, policy ownership, history
restart, all four exact validated OSD presentations, owned-daemon cleanup and
unchanged ambient notification ownership. CPU mode has no DRM allocation.
Empty/warm process PSS MiB is15.60/27.49 Hypr,16.17/29.91 niri; owned bus
adds0.66/0.69. Idle samples are only2 s, not an idle-CPU acceptance.
Actual visual controls/hover and matched QML proofs remain in progress. The initial trial used
the old automatic GL default; it is not a CPU-small-panel memory acceptance.
Notice width/radius400/14 and bottom OSD340×80/radius22/duration150 ms follow
Nacre's current components. Native volume/mic changes and validated private OSD
commands feed the same model. Hardware integration remains nested-fixture-only.
The OSD is passive, shares the process's notification/service owner, and releases
its worker/raster/SHM state after close. Last-notice protocol closure and visual
exit lifetimes are separate; DND/inhibition suppress immediately.

Standalone notification mode currently owns the primary notice surface and one
passive OSD companion; it cannot be combined with bar/audio mode flags. Full
shell composition is not implied by this proof. Feature9 final controls,
matched QML comparison and Feature10 catalog/ranking/providers remain active.

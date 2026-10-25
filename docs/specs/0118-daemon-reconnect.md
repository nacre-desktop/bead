# B0118 — recover daemon sources after boot and restart

The bar's status icons used the transitional direct-bus desktop snapshot, while
its popouts read daemon topics. The bar now holds network, Bluetooth, battery and audio
sources on the shared daemon link and overlays their validated state on desktop
updates. Disconnected or invalid topics show the existing unavailable icons.

`compositor_topic_lease` was a local acquisition error: notifications already
requested the permanent compositor topic. Workspace origins now reuse it through
`view_topic`, which leases only when there is no permanent subscription.

The welcome registry is a startup snapshot: later topics and readiness-dependent
methods were never discovered on an otherwise healthy connection. The transport
now refreshes `daemon.status` with 50 ms–2 s backoff while active topics are missing,
and subscribes them when published. Main shell, apps and wallpaper also follow
`daemon` and refresh the optional N0074 method registry after source changes.
Healthy idle links have no registry timer; refreshes never drop healthy topics.
Unsupported advertised topic versions remain unavailable. Dashboard pages reuse
permanent subscriptions (including notifications) through `view_topic`.

The existing transport retries socket connections, watches private socket creation,
and re-subscribes permanent topics and every active lease after handshake.
Regression coverage starts it before the socket exists, then destroys
and recreates the fake daemon twice (new boot and same-boot socket reconnect).
Every active topic across bar, controls, dashboard, extras, notifications and
settings must recover within two seconds; duplicate subscriptions fail the test.
The fake welcome advertises only one topic; registry reads discover the others,
and a control method appears only after topic recovery, without reconnecting.
A transient registry error retries; a stalled registry closes its socket before
retrying so remote calls cannot accumulate beyond the protocol call limit.

Wallpaper startup previously dropped its reconnecting consumer after three
seconds without a contract. It now retains the consumer immediately; state reads
and activation still require the published healthy contract. The private wallpaper
activation test now starts the consumer before its fake daemon.

Validation: full remote Arch CI passed: formatting, warning-denied Clippy,
workspace/trace tests, renderer feature configurations, Luau/schema checks,
capture-harness tests, packaging, wallpaper smoke, private service/SNI lifecycles,
and cargo-deny for the workspace and every independent framework graph.
No style/layout changes, live sessions, installs or host configuration changes.
N0074 adds service ordering as a separate defence; reconnect remains necessary
for daemon restarts.

Transitional duplicate: daemon-configured bars still project the existing local
sensor snapshot before applying the daemon values. Delete that redundant local
bar projection 14 days after the daemon replacement is trusted; daemon-less
framework previews remain supported.

# Headless service hosts

No Bead store, UI, renderer, Wayland or compositor transport is in this crate's
transitive graph. CI checks default and all-feature graphs with cargo tree.
All snapshots implement serde (notification snapshots are serde_json::Value).

The notification host supplies an already-open zbus connection and history path:

```rust,ignore
let service = bead_services::notification_bus::start(
    bead_services::notification_bus::Config { connection, history }
).await?;
let initial = service.snapshot();
let mut snapshots = service.subscribe(); // one latest snapshot, no growing queue
service.call(bead_services::notification_bus::Control::Dnd(true)).await?;
while snapshots.changed().await.is_ok() {
    let latest = snapshots.borrow_and_update().clone();
    // Publish to this host's clients; no rendering/store dependency here.
}
service.stop().await?;
```

Run on the host's Tokio runtime, including current-thread. No runtime, thread,
bus discovery or process is created by this API. Calls have a 64-request bound,
3-second deadline and acknowledge processing errors; timeout does not undo a
side effect. Drop requests nonblocking cancellation even with a full queue.
Stop unregisters only this instance's interfaces/name and preserves history.
Name acquisition uses DoNotQueue, never ReplaceExisting or AllowReplacement.
The host controls notification ownership; starting on a live bus is not authorized
by these examples or tests. An injected connection must stay driven by its host.

`PrivateBus` explicitly owns a private dbus-daemon for fixtures. `Server::start`
keeps the transitional in-process shell API: its private bus and runtime are
outside the hostable service. Existing UI subscriptions remain unchanged.

The existing `Desktop` composite exposes start_config(Config), snapshot,
subscribe, call and stop. Default start preserves the bar's existing behavior;
config can disable audio/bus lanes independently. Individual desktop backends
remain transitional; moving their tasks into nacre-daemon is later work.

`cargo test -p bead-services --test notification_api` runs the same private-bus
wire parity fixture as the nested shell, without a compositor or UI. It covers
replacement, actions, expiry, history, DND, inhibition and disconnect, plus host
start/stop/restart, competing ownership, call errors and slow subscriptions.

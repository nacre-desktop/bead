# Headless service hosts

This crate depends on neither Bead stores/UI/rendering nor Wayland/compositor transport. CI checks the default and all-feature dependency graphs. Snapshots implement serde; notification snapshots use `serde_json::Value`.

## Hostable notification service

The host supplies an already-open zbus connection and a history path. The service runs on the host's Tokio runtime, including a current-thread runtime; it performs no connection discovery and creates no runtime or thread. A connection injected here must remain driven by its host.

```rust,ignore
let service = bead_services::notification_bus::start_with_options(
    bead_services::notification_bus::Config { connection, history },
    bead_services::notification_bus::Options {
        information: bead_services::notification_bus::ServerInformation {
            name: "Example".into(),
            ..Default::default()
        },
        ..Default::default()
    },
).await?;
let initial = service.snapshot();
let mut snapshots = service.subscribe(); // latest value only; no growing update queue
service.call(bead_services::notification_bus::Control::Dnd(true)).await?;
while snapshots.changed().await.is_ok() {
    let latest = snapshots.borrow_and_update().clone();
    // Publish to host clients; no rendering/store dependency here.
}
service.stop().await?;
```

The request queue holds at most 64 requests. `call` has a three-second deadline and acknowledges processing errors; timeout does not undo a completed side effect. Dropping the handle requests nonblocking cancellation, including when a host cancels its owning task. `stop` waits for service shutdown, unregisters only this instance's interfaces and name, and preserves history. The standard notification name is acquired with `DoNotQueue`, never replacement or queuing.

`PrivateBus` explicitly owns a private dbus-daemon for fixtures. `Server::start_with_options` is a compatibility wrapper that owns a private bus, current-thread runtime, and worker thread; the hostable `start` API above creates none of those. `Desktop::start_config(Config)` exposes independently selectable audio and D-Bus service lanes and provides snapshot, subscription, call, and stop methods.

`cargo test -p bead-services --test notification_api` runs the notification API tests on a private bus without a compositor or UI.

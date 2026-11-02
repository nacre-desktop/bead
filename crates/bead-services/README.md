# Headless service hosts

No Bead store, UI, renderer, Wayland, or compositor transport is in this crate's transitive graph. CI checks default and all-feature graphs with `cargo tree`. Snapshots implement serde (notification snapshots use `serde_json::Value`).

The notification host supplies an already-open zbus connection and history path. The service runs on the host's Tokio runtime; it creates no runtime, thread, bus discovery, or process. Calls are bounded and report processing errors. Stop unregisters only this instance's interfaces and preserves history.

`PrivateBus` explicitly owns a private dbus-daemon for fixtures. `Server::start` is an in-process host API with its own private bus and runtime. `Desktop` composes the individual desktop backends; the host selects the needed lanes.

Run `cargo test -p bead-services --test notification_api` for private-bus API coverage without a compositor or UI.

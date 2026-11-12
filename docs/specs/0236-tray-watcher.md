# 0236 — Optional StatusNotifierWatcher

`bead-services::tray_watcher::spawn(Connection)` provides a session-bus
`org.kde.StatusNotifierWatcher` at `/StatusNotifierWatcher`. It implements item
and host registration, the three standard properties, and item/host lifecycle
signals. Item object paths are qualified with the registering sender; bare bus
names use `/StatusNotifierItem`. Registrations are unique and disappear when
their connection owner leaves the bus.

The fallback requests the well-known name with `DoNotQueue | AllowReplacement`
and never `ReplaceExisting`. It remains passive while another owner exists,
claims the name after it becomes free, and clears state and its exported object
after being replaced. Dropping its handle cancels the task and releases the name.
`Config::tray_watcher` defaults to false; the desktop bus worker starts it before
the existing tray client only when enabled.

Out of scope: icon lookup, menus, tray-client changes, and any application
integration beyond the opt-in configuration. This uses public StatusNotifier
contracts and Bead's existing zbus APIs only. No temporary duplicate or expiry.

Evidence: private-bus proof in `tray_watcher_private_bus.rs`; focused framework
checks recorded in the unit report. Folder-host wiring was skipped because this
base's `crates/bead/src/host.rs` does not wire folder services.

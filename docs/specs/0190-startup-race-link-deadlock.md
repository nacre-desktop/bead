# B0190: shell deaf to ui.dispatch after an early start

Symptom: after a login or shell switch the Bead shell drew frames (clock ticks)
but never opened a panel; every `nacrectl ui drawers toggle dashboard` returned
"Daemon call timed out" and the daemon logged `UI call timed out or was canceled
before reply ... provider=N method=ui.dispatch elapsed_ms≈3000`. Restarting only
the shell fixed it. Hung shells had started 0.10–0.12 s after "Daemon ready";
shells started ≥ 0.67 s after it worked.

## Cause: the nacre-link thread deadlocked on its own status lock

Backtrace of a hung cba984d4 shell (`.lead/hang-bt-cba984d4.txt`), thread
"nacre-link" (the one tokio thread that reads and writes the daemon socket):

    #1 Mutex::lock_contended
    #2 nacre_shell::bar_routes::ready          -> Client::status() -> status.lock()
    #3 nacre_shell::bar_routes::stores
    #4 Routes::watch::{closure}                -> link subscriber
    #5 bead_core::source::Publisher::change
    #6 nacre_shell::nacre_source::run::{closure} -> registry refresh reply

In `nacre_source::run`, the `daemon.status` registry-refresh reply did

    let mut status = shared.status.lock();
    Arc::make_mut(&mut *status).methods = methods;
    shared.link.publish(...);   // guard still alive

`Publisher::change` runs subscriber callbacks synchronously; the bar routes
callback reads `Client::status()`, which locks the same non-reentrant mutex on
the same thread. The link thread blocks forever, so no daemon frame (Call,
Ping) is read again while the socket stays open: the daemon keeps routing
ui.dispatch to a live-looking provider that never answers.

Why only early starts: the refresh runs only when Welcome lacks a configured
topic (or a `daemon` topic change arrives), and the publish only fires its
callbacks when the method set changed. A shell that connects ~0.1 s after
"Daemon ready" gets a Welcome from a daemon still bringing services up, so the
refresh returns more methods and fires the callback. A later shell gets the
complete registry in Welcome, the refresh never changes anything, and the
deadlock path is never taken.

## Fix

The status guard is a temporary dropped at the end of its statement, before
`link.publish` (one-line change in `nacre_source.rs`). No other path publishes
while holding `status`. Nacre N0096 adds the daemon-side safety net: the newest
UI provider registration wins, and a provider that misses three consecutive
ui.dispatch deadlines has its link closed so the shell reconnects and
re-registers.

## Test

`nacre_source::tests::early_registry_refresh_with_status_reading_subscriber_keeps_link_live`:
a fake daemon sends a Welcome without the configured topic (the early-start
state), answers the registry refresh with a grown method set, then pings. The
real `bar_routes::Routes::watch` subscriber reads `Client::status()` in its
callback. The test requires the routes to observe the new method and the link
to answer the Ping. With the old line restored it fails (link thread deadlocked;
the test leaks the stuck client instead of hanging). The old/new shell overlap
during a switch is covered daemon-side by N0096's hub tests.

No memory or visual change; no temporary duplicates.

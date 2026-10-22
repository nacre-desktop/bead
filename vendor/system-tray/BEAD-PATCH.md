# system-tray 0.8.9 lifecycle patch

MIT source from the published system-tray 0.8.9 crate, upstream revision
`1fbc4877d188ce9b6be881f5c9887736d8bec7fc`. The crate metadata declares MIT;
its README credits the eww SNI host. LICENSE preserves that host license. Cargo.toml sets publish=false and README whitespace is normalized.
The tray host reuses the existing protocol implementation; it adds no transport.
The lifecycle patch terminates DBusMenu listeners on their application owner
loss or connection EOF. It reaps completed watcher JoinSet handles, subscribes
to owner-loss before initial menu calls, checks owner presence after subscribing,
and reacquires the host registration plus resnapshots current items when this
connection becomes the StatusNotifierWatcher owner again. Duplicate item events
are ignored so they cannot create duplicate property/menu tasks.

Bound retained watcher and client item identities to 64. Validate SNI pixmap
dimensions with checked arithmetic, retain at most eight pixmaps per property,
and bound the aggregate retained item icons to 1 MiB. Retained item/tooltip text
is control-stripped and bounded. DBusMenu trees are limited to 256 entries and
depth 8; retained menu icon data is limited to 1 MiB total, 4 KiB per entry.
Identity strings are limited to 512 bytes. These limits apply after zbus has
materialized the incoming D-Bus message; transport-level message allocation is
still controlled by zbus and remains an upstream gap.

The private 100-item churn proof exposed retained tasks and growing memory.
Watcher and item caps prevent unbounded retained registrations, and stale tasks
are retired with owner/session loss.

Rendered UI churn additionally exposed completed watcher JoinSet handles retaining
allocations. Reap them on registration/unregistration. Subscribe to owner-loss
before initial menu calls and check owner presence after subscribing to cover
items which exit during setup. A Rust-only diagnostic counts retained handles;
it adds no D-Bus interface method. Alive task counts alone missed this retention.

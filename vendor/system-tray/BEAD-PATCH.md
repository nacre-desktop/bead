# system-tray 0.8.9 lifecycle patch

MIT source from the published system-tray 0.8.9 crate, upstream revision
`1fbc4877d188ce9b6be881f5c9887736d8bec7fc`. The crate metadata declares MIT;
its README credits the eww SNI host. LICENSE preserves that host license. Cargo.toml sets publish=false and README whitespace is normalized.
The protocol implementation differs only in client.rs watch_menu:
terminate DBusMenu listeners on their application owner loss or connection EOF.
Upstream item listeners already watch owner loss; menu listeners did not.
The private 100-item churn proof exposed retained tasks and growing memory.
This patch reuses the existing protocol implementation; it adds no transport.

Rendered UI churn additionally exposed completed watcher JoinSet handles retaining
allocations. Reap them on registration/unregistration. Subscribe to owner-loss
before initial menu calls and check owner presence after subscribing to cover
items which exit during setup. A Rust-only diagnostic counts retained handles;
it adds no D-Bus interface method. Alive task counts alone missed this retention.

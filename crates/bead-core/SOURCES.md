# Protocol-neutral sources

`Source` is a view-side interface, not a second desktop service or wire protocol. It provides a serde snapshot, scoped subscriptions, calls with deadlines, a reconnect hook, and stop. Calls use the caller's Tokio time driver; the interface creates no runtime or thread. A timeout does not undo an action or replay it on reconnect.

## Publisher and mirror

`Publisher` retains one shared `Arc` payload. Snapshots are limited to 8 MiB of encoded JSON, 64 nesting levels, and 65,536 values. There are at most 64 scoped listeners; dropping a `Subscription` unregisters it. Callbacks run without holding the publisher lock, so they may reenter. With concurrent publishers, a mirror rejects obsolete callback revisions. Adapters should publish their origin revision so a late callback cannot relabel older data as a new update.

`Mirror` accepts object snapshots and RFC 7396 merge patches. A patch carries its base and new revision: coalescing may skip revisions, but a wrong base requests a resync without changing the last picture. Obsolete revisions are ignored. After a transport boot, call `reconnected(epoch)` before accepting its snapshots; callbacks from an older reader cannot roll back a new boot. JSON null object members delete keys; arrays replace as a whole. See [RFC 7396](https://www.rfc-editor.org/rfc/rfc7396).

`project` and `project_patch` stage the mirror and stable-store projection together. A bad projection, schema, or value preserves both previous states, while unrelated UI stores survive. `values(prefix, projection, now)` also supplies `<prefix>.stale` and `<prefix>.reconnecting`; reconnecting becomes true after two seconds. The view or transport schedules that one-shot change. Reconnect backoff ranges from 50 ms to 2 s. Collections, ordering, and topic versions belong to the provider's schema.

## In-process adapters

`bead::source` adapts in-process services (`Notifications`, `Clock`, `DesktopView`) to a `Source`; they forward snapshots and calls without defining an application wire protocol. `bead-test::source::FakeSource` drives deterministic headless UI and timeout tests. Wire framing, peer authentication, and automatic socket reconnect belong to whichever transport a host supplies.

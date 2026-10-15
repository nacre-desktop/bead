# Protocol-neutral Sources

Source is a view-side interface, not a second desktop service or wire protocol.
It has a serde snapshot, scoped subscription, calls with deadlines, reconnect
hook and stop. It uses the caller's Tokio time driver for calls and never creates
a runtime or thread. Timeout does not undo an action or replay it on reconnect.

Publisher retains one Arc payload, capped at8MiB encoded JSON,64 levels and65536
values. Disconnect shares the last payload. There are at most64 scoped listeners;
dropping a Subscription unregisters it. Callbacks run unlocked and may reenter.
With concurrent publishers, a mirror rejects obsolete callback revisions.

Mirror accepts object snapshots and RFC7396 merge patches. A patch names its base
and new revision: coalescing may skip revisions, but a wrong base requests resync
without changing the last picture. Obsolete revisions are ignored. After a new
transport boot, call reconnected(epoch) before accepting its snapshots; old-reader
callbacks cannot roll back a new boot. Snapshots can contain null object members;
merge-patch null deletes them. Arrays replace as a whole. No GPL protocol code is
used. [RFC7396](https://www.rfc-editor.org/rfc/rfc7396) defines these semantics.

project/project_patch stage both the mirror and a stable-store projection. Bad
projection/schema/value keeps both previous states; unrelated UI stores survive.
values(prefix, projection, now) also supplies prefix.stale and prefix.reconnecting.
The reconnecting mark starts after2seconds; the last payload remains available.
The view/transport schedules that one-shot wakeup. Reconnect backoff is50ms→2s.
Collections/order and topic versions belong to the provider's schema.

bead::source has in-process notification and Desktop adapters. HostedNotifications
attaches the headless handle using the caller's runtime. Notifications::private
preserves the existing shell's explicit private bus/runtime wrapper; it never
finds the ambient notification bus. Desktop wraps the current composite service,
using the host's bounded blocking pool for its transitional synchronous actions.
The accepted bar retains its typed service path. Notification UI uses Source
and Mirror with unchanged Luau bindings; no catalog/ranking/providers are added.

bead-test::source::FakeSource drives deterministic headless UI and timeout tests.
At F5d start, nacre-protocol was not present on published Nacre main (draft only).
The Nacre adapter and its upstream transcript replay are therefore deferred per
the owner brief; no unpinned GPL draft or transcript enters framework crates.
Wire framing, peer authentication and automatic socket reconnect belong to that
future GPL shell adapter. The local framework tests do not accept those claims.

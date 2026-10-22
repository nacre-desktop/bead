# Nacre protocol v1 fixtures

GPL-3.0-only; generated Nacre contract data, retained in the GPL application.
Original v1 frames came from nacre-desktop/nacre at
`10e860363fb5c37631925873ea81eddebd89e012`, `protocol/fixtures`; menu and core
examples were added with later reviewed pins. The seven `core-compat-*` files
are byte-identical to published `961de0a334259ee0f537f201ec62244e5eebfbb7`.
The protocol crate is pinned to Nacre `294d3f4034e0ffc8c6ee4fe38a3ed4109aa56c84`.
The `calls.jsonl` transcript follows that export, and
`wallpaper-active.jsonl` is the published N0027 v1 topic/activation golden.
They are reserved contracts, not proof of runtime service availability.
Tests parse these committed bytes and compare their JSON wire objects with the
pinned crate's exports before replaying. Object key order is not a wire contract;
array order and every field remain checked. Update only with a reviewed protocol pin.
No protocol dependency or fixture is embedded in a Bead framework crate.

# Nacre protocol v1 fixtures

GPL-3.0-only; generated Nacre contract data, retained in the GPL application.
The v1 fixtures are pinned to nacre-desktop/nacre commit
`294d3f4034e0ffc8c6ee4fe38a3ed4109aa56c84`, `protocol/fixtures`. This includes
the core `ui.dispatch` timeout update, the matching calls transcript, and the
N0027 `wallpaper-active.jsonl` topic/activation golden.

They are reserved contracts, not proof of runtime service availability.
Tests parse these committed bytes and compare their JSON wire objects with the
pinned crate's exports before replaying. Object key order is not a wire contract;
array order and every field remain checked. Update only with a reviewed protocol pin.
No protocol dependency or fixture is embedded in a Bead framework crate.

B0020 adds the six `notifications-*.jsonl` producer transcripts from Nacre
`652d6c4f` (N0017 default-action marker included); `notice_service` tests replay
them through the bounded mirror.

# Internationalization consumer

`bead-ui` provides catalog lookup without knowing where catalog data comes from:

- `set_builtin(json)` installs the built-in message table once. Entries are text strings or `{one, other}` plural forms.
- `install(revision, entries)` replaces the fetched catalog as a whole. It drops the previous revision and notifies listeners only when revision or content changes.
- `on_change(callback)` subscribes a host to catalog changes; dropping the returned subscription removes the listener. `clear()` drops the fetched catalog while retaining built-in fallback text.
- Rust callers use `tr(id, args)` or `tr_n(id, count, args)`. Luau exposes `ui.tr(id, args?)` in markup and `tr(id, args?)` in behavior scripts. `{name}` placeholders are substituted; `count` selects `one` only when it equals 1 and is also available as a `{count}` placeholder.

Lookup order is fetched catalog, built-in catalog, then the ID itself. Hosts own locale selection, namespaces, catalogs, and transport. When a catalog changes, subscribed views rebuild from the new text while preserving UI stores; hosts must also refresh Rust-projected text as needed. No daemon or socket transport is part of this API.

# B0032 — i18n consumer: `tr()`, string ids, live language refresh

Status: **Review — implemented; remote CI and fake-daemon tests below. Not pushed to a live switch.**
Consumes Nacre N0035 (`i18n` topic, `i18n.catalog`, commit `b038ef18`;
`docs/specs/0035-i18n-publication-contract.md` in Nacre). Structure per
[ADR 0004](../decisions/0004-structure-licensing-generalization.md): the framework
crates stay Nacre-agnostic, the Nacre provider lives in GPL `nacre-shell`.

## Design

- **bead-ui `i18n` (MIT/Apache, generic).** `tr(id, args)`, `tr_n(id, count, args)`
  (`{one, other}` plurals, `{count}` filled in), `{name}` placeholders. A process-wide
  catalog with three inputs the host injects: `set_builtin(json)` (English table),
  `install(revision, entries)` (replaces the whole fetched catalog; old revision is
  dropped, listeners run only on a real change) and `on_change(cb)`. Lookup order:
  fetched, built-in, then the id itself. No dependency on Nacre or sockets.
- **Luau.** `ui.tr(id, args?)` in markup and a global `tr(id, args?)` in behaviour
  scripts (set before the sandbox is sealed). A numeric `count` argument selects the
  plural form. Declared in the generated SDK types; `tools/check-luau.py` declares the
  behaviour global for the analyzer.
- **Rust-side text.** `crate::i18n::tr/tr_n` in `nacre-shell` (embeds
  `shell/i18n/en.json` as the built-in table). Transient messages that used to be
  English literals now hold message ids and are translated when projected, so a
  language switch also rewrites a message already on screen.
- **Nacre provider (`nacre-shell/src/i18n_source.rs`).** Follows the `i18n` topic
  (added to the daemon source's topics; filtered by what the daemon advertises). On a
  new `revision` it pages `i18n.catalog {namespace:"bead", revision, after}` to the end
  (bounds: 32 pages, 2048 entries, 256 KiB, ids must be `bead.*` and strictly
  increasing), retries `stale_revision` up to 3 times against the re-read topic, keeps
  the last good catalog on any other failure (retry 3 times, 5 s apart), and installs
  the result. No daemon, no topic or a failed fetch leaves the built-in English.
- **Live refresh.** Every runtime subscribes to `on_change`; a change sends `Reload`
  (new generation: markup, behaviour VM and all caches rebuilt from the new text, state
  stores kept) then an empty `Model` so Rust projectors rewrite their text. No restart.
  Known limit: a stale status view that shows a cached last-good display keeps that
  text's old language until the next fresh snapshot.

## Ids

`bead.<surface>.<name>`, 283 ids (bar 11, panel 84, dashboard 125, launcher 16,
osd 7, notifications 5, common 35). Authoring source is `shell/i18n/en.json` +
`context.json` (translator note for every id); `tools/i18n-sync.py <nacre-checkout>
[--write]` keeps Nacre's `nacre/shell/i18n/{en,context}.json` `bead.*` entries identical
(Nacre `tools/i18n/check.py` stays green). The two `bead` pages are the real paging case
(283 entries > 200 per page). Left English on purpose: product and file names, glyph
labels, developer-only views (`kit`, `layout-example`, `legacy-ui`), the headless
preview in `main.rs`, daemon-supplied text (app names, notification bodies, error
messages from the daemon), diagnostic strings.

## Memory

Only the `bead` namespace is held: 12.1 KB of ids and texts. Each parsed table
(`BTreeMap<String, Message>`) is estimated at about 40 KB of anonymous heap (283 entries,
about 140 bytes of node, enum and two allocations each), so about 40 KB with the
built-in English only and about 80 KB while a daemon catalog is installed (the built-in
table is kept as the per-id fallback). The 14.9 KB JSON is `include_str!` (file-backed
read-only data). This is an estimate from the data structure, not a matched capture;
against the +0.5 MiB median gate it is under 0.1 MiB, so no capture was scheduled. A
new revision replaces the fetched table whole; nothing else is cached per revision.

## Tests

- bead-ui: placeholders, `ui.tr`/`tr` in markup and behaviour, plural, missing id,
  en → pseudo install + reload redraws, listener fires once, clear returns to English.
- nacre-shell fake daemon (`i18n_source.rs`): paging with cursor and revision, stale
  revision, contract violations refused, en → pseudo language switch, a racing revision
  recovering after bounded stale retries.
- nacre-shell: every `bead.*` id used by Luau/Rust exists in `en.json` with a context
  note, no id is unused, id shape, plural/placeholder rendering from the built-in table.
- Packaging: the message tables are not installed (embedded in the binary).

## Temporary duplicates

`shell/i18n/*.json` mirror the `bead.*` entries in Nacre until the owner decides Nacre
owns them: keep both while `tools/i18n-sync.py --check` is in use; delete the Bead
copy 30 days after Nacre generates the table at build time. Store defaults in the
`behavior.luau` files still call `tr`, they are replaced by the first projection.

# B0045 — Settings window framework (QML retirement unit B-D1)

Status: **Review** (Claude worker, branch `task/b0045-settings-framework-20261023`). Owner 2026-10-23: no QML after the
switch; the QML Settings app is the biggest remaining gap. This unit lands the framework and two real pages that prove it
(Appearance, Notifications). The other 34 registry pages are later units and need **no Rust**: see "Adding a page".
Nacre reference: `origin/main` at `4d816ca2` (registry `nacre/shell/modules/settings/registry/*`, pin of `nacre-protocol`).

## Framework rule (owner 2026-10-23)

Bead is a general framework. Nothing here hard-codes the owner's setup:

- **Engine**: `bead-wayland::window` (generic xdg toplevel, MIT/Apache) and the form kit in `bead-ui/assets/kit.luau`
  (`SectionHeader`, `HelpText`, `FormRow`, `ToggleRow`, `ChoiceRow`, `NumberRow`, `TextRow`, `ListEditor`, `Swatch`, `ApplyBar`).
  They take labels, values and ready-made actions; sizes, radii and colours are theme tokens/palette roles. No setting id.
- **Adapter** (`nacre-shell/src/settings.rs`, GPL): the only code that knows Nacre's registry shape and the `settings.store.<id>`
  / `settings.set` contract. It knows value *types* (boolean, integer/number, enum, string, array), not setting names.
- **Data**: `shell/settings/registry/{navigation,settings,en}.json` (Nacre's registry, verbatim) and `bead.json` (which pages
  Bead may edit, default page). **Luau**: `shell/settings/{main,behavior}.luau` maps a row kind to a kit component.

### Adding a page or widget

- A page of existing kinds: add its id to `editable` in `bead.json`. Rows, search and read/write come from the registry.
- A new widget: add a component to the kit and a branch in `shell/settings/main.luau` plus a `kind` in the adapter's row
  builder only if it needs a new *value type*. A custom section (`customType` in the registry, e.g. the notification history
  list) is a Luau page over a published topic; it shows a placeholder line until built.

## Window (bead-wayland)

`WindowSpec { app_id, title, size, min_size, fallback, factory, model }` in `Options.window`; `nacre-shell` sets
`app_id nacre-settings`, 960x640, min 560x400. xdg-shell toplevel (sctk `XdgShell`, server-side decorations requested),
configure/resize, `request_close`, min size, fractional scale + viewporter, CPU/SHM renderer, three SHM buffers sized to the
configured geometry. Keyboard (Ctrl+W/Q closes, Escape and editing keys reach the view), pointer, scroll and cursor shape go
through the same `UiEvent`s as panels. Calls (to the app process's own socket): `bead.window open [page]`, `bead.window close`. `nacre settings [page]` reaches
them through `app::route` (the existing QML route, now with an optional page: the deep link).
Missing xdg-shell makes `open` fail with a reply; nothing else changes.

Not done (listed, not hidden): no IME for the window (typing uses key events), no frame-callback pacing (draws are
throttled to 8 ms and wait for a free buffer), no GL path (CPU only, by design for a form window), no narrow layout.

## Process placement (ADR 0005)

Superseded by B0052: Settings is an on-demand app process (`nacre-shell --app settings [page]`), as ADR 0005 says. The
first version of this unit ran it as a session inside the shell and left +0.69 to +0.92 MiB PSS behind after one
open/close (gate +0.5 not met); the shell no longer holds any Settings state. The window, factory and adapter below are
unchanged; only the host moved.

## Lifecycle

Nothing is resident until `open`: no source, renderer, buffers, registry or store lease (the registry JSON is read from the shell bundle on open, not embedded in the binary). `close` (button, Ctrl+W, compositor close)
drops the window, its worker (joined), renderer, pool and buffers, the adapter (registry, store leases, subscriptions) and
calls `malloc_trim`. Store topics: permanent topics (`settings.store.desktop`) are followed, other stores are leased only
while the shown page reads them and released on page change.

## Store contract

Reads: `settings.store.<id>` (`{store, rev, exists, data, dirty, error}`); unset keys read the registry default and a wrongly
typed value falls back to it (like QML). Writes: published `settings.set {store, key[], value, rev}` (nacre-protocol
`SettingsSet`), enabled only while the daemon advertises it at v1, is connected, and the store is readable. The `rev` fence
comes from the topic; `stale_revision` shows "changed elsewhere" and nothing is replayed; one call at a time. Live updates:
every followed store wakes the window. Argument text never names a store, key or method (ids are re-resolved from the
registry). `apply: "confirm"` settings stage the value and show `ApplyBar` ("requires apply").
**Nacre ask: none for this unit.** Stores written by helpers only (`colors`, `wallpaper-picker`, ...) stay read-only until their
apply contracts exist (N-A5); that is why `bead.json` lists editable pages.

## Search

Pages and settings by label, description, synonyms and key (all tokens must match, 40 results). A result opens its page.
Typing (key into the search field, stage + commit): release p50 0.20 ms, max 3.1 ms (budget 8 ms); debug p50 1.7 ms.

## Temporary duplicates and expiry

- `shell/settings/registry/*.json` is a verbatim copy of Nacre's registry at `4d816ca2` (and the English text for its ids,
  extracted from `nacre/shell/i18n/en.json`). Delete the copy 7 days after the daemon publishes the registry (or Bead reads it
  from the Nacre bundle); re-sync with a Nacre bump until then. Registry labels are English-only: the daemon `bead` namespace
  fetch does not cover `settings.registry.*` (ask for a wider namespace page when a translation exists).
- The QML Settings app stays until the switch; nothing is removed here.

## Checks

See the report; goldens: `settings-{appearance-1,appearance-1.5,notifications-1,search-1,kit-1}.png` (eyeballed).

## Follow-ups (one line each)

- Notifications history section (needs the `notifications` topic list + clear method; B0030 owns the service).
- Remaining pages: add to `bead.json` as their apply contracts land; custom sections as Luau pages.
- Narrow layout, IME, window title from the page.

## Memory (desktop, private nested niri, CPU renderer (tiny-skia), no daemon, 3 alternating pairs, release builds)

Candidate minus control (`ca4c19c`), median Pss MiB: idle (window never opened) -0.01 to +0.15 (runs vary; gate +0.5: pass);
open window +5.4 (anon +3.6, SHM +1.4; expected, not gated); **closed and settled +0.69 to +0.92 (anon +0.6): gate +0.5 NOT met**,
anon growth stays under the 1 MiB rule. Tried without effect: second `malloc_trim` after the worker queue drains, handing the
accessibility bridge the bar tree on close, registry read from disk instead of embedded, `MALLOC_ARENA_MAX=1` (worse, +0.9).
The ~0.6 MiB anon residue after one open/close is unexplained; next step is a heap profile of the closed state. Window mapped
as `nacre-settings`, close and reopen with a page deep link worked in every arm. GL not used.

## Checks (prod Ubuntu 24.04 container, CI job steps)

fmt, clippy -D warnings, `cargo test --workspace` (settings: 9 tests), memory_policy trace tests, renderer feature checks,
minimal example, Luau analyze, spikes/packaging python tests (git checkout), services check, cargo-deny: all passed.
Existing `bead-services ipc::inherited_listener` test flaked once under parallel load (passes alone and on rerun; untouched code).

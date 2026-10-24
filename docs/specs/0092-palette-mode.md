# B0092 — The command palette consumes Nacre's palette query mode (N0058)

Status: **Review**. Consumer of Nacre N0058 (`menu.query {mode:"palette", route:"palette", query}`), pinned at
Nacre `d07efe48`.

## What changed

- `nacre-protocol` is pinned to `d07efe48e992446d158765b6134ecc9edd30a876` (was `c6dc627b`).
- `crates/nacre-shell/src/extras.rs`: the palette view sends `mode:"palette"`, `route:"palette"` for the empty and every
  typed query. The daemon owns content, matching, ranking and order (actions, then windows, then at most eight apps; `>`
  and `brain:` handled in Nacre as in `Palette.qml`); Bead displays `MenuQueryResult.order` unchanged. The old
  `mode:"apps"` with route `actions` (empty) or `""` (typed), which produced launcher-alias rows ("Capture, Clipboard
  history, Emoji…") that differed from QML, is gone. Stale-reply guards are kept: a reply is used only if its query,
  route and daemon boot match the request that is currently wanted, and `activate:` is accepted only for the displayed
  query, id and `catalog_rev`. A route returned by an activation (`view.route`) still replaces the default route.
- Prefixes: the input shows what the user typed. QML has one placeholder and one title for all prefixes ("Actions, apps,
  windows, or brain: something", "Commands"; no mode label), which Bead already had (`shell/extras/behavior.luau`), so
  `>` and `brain:` need no Bead-side display logic: the daemon turns `brain: x` into one "Search brain for x" row.
- No Luau change: `shell/extras` already treats palette rows as final (no filtering or ordering).

## Tests

`crates/nacre-shell/src/extras/tests.rs` (fake daemon returns a deliberately non-alphabetical `order`):

- `palette_uses_opaque_engine_activation_and_rejects_obsolete_query`: the empty query sends
  `{"query":"","mode":"palette","route":"palette"}`; activation of a different query is dropped.
- `palette_mode_keeps_the_daemon_order_for_empty_typed_and_prefixed_queries`: `""`, `set`, `> set`, `brain: x`, `>brain:x`
  each send mode and route `palette`; rows come out in daemon order.
- `palette_activates_a_typed_row_with_its_own_ticket`.
- `extras_goldens`: the palette golden now shows QML's first seven empty-palette rows (wallpaper picker, Calculator,
  Colors, Settings, AI, Brain, files) at scale 1 and 1.5.

Harness: `spikes/parity-compare.py` gets a `type` step (wtype into the focused panel, same keystrokes for both arms) and
the states `palette-typed` (`set`) and `palette-brain` (`> brain: hello`).

## Evidence

`docs/specs/evidence/0092/{palette,palette-typed,palette-brain}.png` are QML (left) | Bead (right), bottom-centre crop of
the full frame, from `spikes/parity-compare.py` on the owner's desktop (private nested niri, 2880x1800 at scale 1.5, the
owner's presentation record; private daemon built from Nacre `8b704fb1`, which contains N0058; swap 12 MiB before and
after). `rmse.json` has the harness metrics (whole-frame rmse: palette 4.19, palette-typed 4.89, palette-brain 4.80).

- Empty palette: identical content and order in both arms (Open wallpaper picker, Calculator, Open Nacre Colors, Open
  Nacre Settings, Open Nacre AI, Open Nacre Brain, Open files) with the same descriptions and icons, title "Commands" and
  the same placeholder. The old "Capture, Clipboard history, Emoji..." rows are gone. Differences: Bead shows a text
  caret in the field and the focused-row fill is the same; row text baselines are ~1 px apart.
- `set`: Bead shows Open Nacre Settings, Wi-Fi/Bluetooth/Sound settings (actions), then Application rows (NVIDIA X Server
  Settings, Qt6 Settings, Volume Control) from the daemon. **Known gap: the QML arm never receives the typed text**
  (`wtype` keystrokes are not delivered to Quickshell's panel in the nested session, even 2.5 s after opening), so the
  QML half of the typed pairs still shows the empty palette. The typed order is therefore verified by the daemon (N0058
  tests and fixture) and by Bead's fake-daemon tests, not by a QML pair.
- `> brain: hello`: Bead shows one row "Search brain for n: hello / Open the brain drawer". The "n:" is QML's own
  `query.slice(6).trim()` applied to the raw text including `>` (N0058 keeps that behaviour on purpose); with `brain: hello`
  and no `>` the value is `hello`.
- Behaviour fix found by the pairs: Bead kept the typed text after the panel closed (the next opening showed "set> brain:
  hello"); QML builds its search surface per opening. `extras.rs` now clears `extras.query` while `panel.open` is false
  (`a_closed_panel_forgets_the_typed_query`).

## Temporary duplicates

None added. Window move (N0058 deferred half) is not part of this unit.

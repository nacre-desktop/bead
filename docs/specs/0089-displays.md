# B0089 - Settings Displays page (Nacre N0063)

Status: **Review** (Claude worker, branch `task/b0089-displays-20261024`). Consumes Nacre N0063 (`nacre-protocol` pinned at the N0063 commit;
Nacre `docs/specs/0063-displays-contract.md` records what the QML does and how a change is applied). Builds on B0045/B0058/B0063.
The control-centre "displays" entry needs nothing: see below.

## What the QML had, and what Bead now has

| QML (`NacreDisplaysPage`) | Bead |
| --- | --- |
| "Displays" card: primary-display chips (>1 output), Extend right, Extend left, Mirror (hidden when the compositor cannot mirror), description by capability | the same card: `actions` rows built from `displays.outputs`, `can_mirror`, `can_configure`; the three descriptions are the QML sentences |
| one card per output titled `{name} · {w}×{h} · {Hz} Hz` with "Display changes revert after 20 seconds unless you keep them." | same title and note (the 20 is the contract's `CONFIRM_SECONDS`) |
| Scale select, 100/125/150/175/200/250/300 % | `ChipsRow` of `displays.scales`; the current scale selected |
| "Resolution and refresh rate" select (searchable above 8 entries) | chips up to 8 modes, above that `PickerList` with the page's shared filter field (the daemon resolves the current mode) |
| "Keep this display change?" card with Keep / Revert while a change is pending | same card with Keep and Revert chips; **additions:** its description counts down ("Reverting in 14 s") from `pending.deadline_ms` (one wake a second only while it is shown), and the card is the first on the page (QML put it last, below the fold on a small window) |
| controls blocked while pending, busy, or the compositor cannot configure | same; also while the method is not advertised |

**Arrangement view:** QML has none (no canvas, no dragging, no rotation, no enable toggle, no VRR control), so none is built; the page
must look like QML. `make`, `model`, `x`, `y`, `transform`, `vrr` are in the topic for a later unit (writers would need controller work in Nacre).

**Control centre:** QML's `NacreControlDisplays` is brightness sliders and the keyboard backlight. Bead's control centre already has both
tiles (`brightness.level`, `keyboard` in `shell/control/tiles.luau`; N0036/N0046). Nothing to add.

**Display recovery** (`DisplayRecovery.qml`, `display-recover.py`) rebuilds QML per-screen surfaces after output changes. Bead's layer surfaces
follow compositor output events natively, so there is nothing to port (Nacre N0063 spec).

## Design (nothing Nacre-shaped in Rust layout)

- Adapter `crates/nacre-shell/src/settings/displays.rs` (GPL, the Nacre edge): reads the `displays` topic through the contract's type
  (`DisplaysState`, validated), emits generic rows (`actions`, `picker`), sends `display.apply|confirm|revert`. Output names, scales and mode
  indexes in a click are re-resolved against the topic; an unknown one sends nothing; a name containing the argument separator is never actionable.
  One call at a time (the Settings rule); the primary output is local page state.
- `page_items` replaces the registry's single `displays.devices` section by the several cards above (the registry copy stays verbatim).
- Luau: **no new kit component and no new row kind.** `actions` rows may ask for wrapping chips (`chips`, `per_row`) so a two- or three-button row is left-aligned as in QML; `kit.ChipsRow` now omits its label line when the label is empty (general, other users unchanged). `displays` added to the editable overlay (`bead.json`); the page-level test that used it as the example of a read-only page now asserts that every page is editable.
- Refusals: the daemon's `busy` says "Keep or revert the current display change first."; other refusals use the page's generic action message.
- i18n: 19 `bead.settings.displays-*` ids (Bead authors them; Nacre mirror synced with `tools/i18n-sync.py --write`).

## Checks

Fake-daemon tests (`settings_pages_tests.rs`): cards and rows from the topic (QML titles, chips, picker, filter, mirror gating); exact
`display.apply` shapes (arrangement with the chosen primary, scale alone, mode with the output's scale); nothing unlisted is sent
(unknown output/scale/mode, mirror on niri, keep/revert without a pending change); the countdown text, controls off while pending,
Keep and Revert calls, `busy` message; read-only without the methods or with `can_configure:false`; no topic -> one line. Goldens
`settings-displays-1` and `settings-displays-keep-1` (CPU renderer, test font).

## Look-and-feel parity (item 5)

PARITY-PLACEHOLDER

## CI

CI-PLACEHOLDER

## Temporary duplicates

None added. `shell/settings/registry/*` stays the verbatim registry copy (B0045).

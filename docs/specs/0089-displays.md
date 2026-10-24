# B0089 - Settings Displays page (Nacre N0063)

Status: **Review** (Claude worker, branch `task/b0089-displays-20261024`). Consumes Nacre N0063 (`nacre-protocol` pinned at the N0063 commit;
Nacre `docs/specs/0063-displays-contract.md` records what the QML does and how a change is applied). Builds on B0045/B0058/B0063.
The control-centre "displays" entry needs nothing: see below.

## What the QML had, and what Bead now has

| QML (`NacreDisplaysPage`) | Bead |
| --- | --- |
| inside the registry card "Connected devices": a "Displays" part with primary-display chips (>1 output), Extend right, Extend left, Mirror (hidden when the compositor cannot mirror), description by capability | the same part, in the same card (as the Sound page): `actions` rows built from `displays.outputs`, `can_mirror`, `can_configure`; the three descriptions are the QML sentences |
| one part per output titled `{name} · {w}×{h} · {Hz} Hz` with "Display changes revert after 20 seconds unless you keep them." | same title and note (the 20 is the contract's `CONFIRM_SECONDS`) |
| Scale select, 100/125/150/175/200/250/300 % | `ChipsRow` of `displays.scales`; the current scale selected |
| "Resolution and refresh rate" select (searchable above 8 entries) | chips up to 8 modes, above that `PickerList` with the page's shared filter field (the daemon resolves the current mode) |
| "Keep this display change?" part with Keep / Revert while a change is pending | same part with Keep and Revert chips; **additions:** its description counts down ("Reverting in 14 s") from `pending.deadline_ms` (one wake a second only while it is shown), and the part is the first in the card (QML put it last, below the fold on a small window) |
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
- `displays.devices` is a registry `device-content` section like Sound's: `custom_rows` returns its rows (the registry copy stays verbatim); parts are flat `section` heading rows.
- Luau: **no new kit component and no new row kind.** `actions` rows may ask for wrapping chips (`chips`, `per_row`) so a two- or three-button row is left-aligned as in QML; `kit.ChipsRow` now omits its label line when the label is empty (general, other users unchanged). `displays` added to the editable overlay (`bead.json`); the page-level test that used it as the example of a read-only page now asserts that every page is editable.
- Refusals: the daemon's `busy` says "Keep or revert the current display change first."; other refusals use the page's generic action message.
- i18n: 19 `bead.settings.displays-*` ids (Bead authors them; the Nacre mirror was synced with `tools/i18n-sync.py --write`, Nacre `a1b9bcb6`).

## Checks

Fake-daemon tests (`settings_pages_tests.rs`): cards and rows from the topic (QML titles, chips, picker, filter, mirror gating); exact
`display.apply` shapes (arrangement with the chosen primary, scale alone, mode with the output's scale); nothing unlisted is sent
(unknown output/scale/mode, mirror on niri, keep/revert without a pending change); the countdown text, controls off while pending,
Keep and Revert calls, `busy` message; read-only without the methods or with `can_configure:false`; no topic -> one line. Goldens
`settings-displays-1` and `settings-displays-keep-1` (CPU renderer, test font).

## Look-and-feel parity (item 5)

Desktop (nested niri 2880x1800 at 1.5, one `winit` output, no applies: the pending state is a journal file in the private config, the
rollback unit cannot be armed there because the private bus has no systemd), QML at Nacre `a1b9bcb6` against this branch's release build,
same helper, default (GL) renderer, gallery `~/nacre-lead/b0089/g3/index.html`. Pairs in `0089-evidence/`
(`{qml,bead}-settings-displays{,-keep}.png`). rmse 7.87 % (idle), 8.01 % (Keep). No arm errors. Swap used 12 MiB before the arms.
Same structure and wording in both: "Connected devices" card, "Displays" part and sentence, per-output title and 20 s note, Scale,
Resolution and refresh rate, "Keep this display change?" with Keep and Revert.

Known gaps (listed, not fixed):
1. QML's Scale and Resolution are drop-down selects; the kit has none, so Bead draws chips (modes above 8 in a filtered list).
2. Keep part: Bead shows it first with a countdown; QML last, no countdown, and dims the other controls while pending.
3. QML groups digits in the title (2,880×1,800); Bead's i18n has no locale number format.
4. QML's caption "Changes save automatically" (controller message), the header's current-page label and the "..." page menu are not in Bead (shell-wide).
5. A long part description is one clipped line in Bead (the Mirror sentence is cut at the card edge on a 960 px window).
6. Sidebar differences are shell-wide (B0063 list).
Not compared: more than one output, a real change/Keep/Revert round trip and the rollback timer on a compositor (no real output may be changed; covered by the
Nacre N0063 fake-niri/fake-systemd daemon tests), Hyprland.

## CI

Arch-equivalent for Bead is the Ubuntu 24.04 container of `ci.yml` on prod (all steps of the job: fmt, clippy -D warnings, boundary, workspace
tests with 517+ in nacre-shell incl. 6 new Displays tests, memory policy, features, facade/Luau analyze, spikes, packaging, smoke, services, tray,
cargo-deny) passed at the rebased head (`FULL_CI_PASS`). Nacre side: the whole `desktop` job (N0063 spec) passed twice (1065 tests).
Moving the pin also needed the `menu-palette` transcript in `tests/protocol-v1` (Nacre's `menu_transcripts()` grew); main already carried
the same pin bump from B0092, so the pin is the later Nacre `a1b9bcb6` (contains N0063 and that).
Found on the way and fixed: a dim `ChipsRow` passed no action name and raised a Luau error live (goldens enable every row, so they missed it);
`displays_page_draws_while_every_row_is_off` now covers it. Not fixed (out of scope): `kit.ApplyBar` buttons collapse (see `settings-kit-1`).

## Temporary duplicates

None added. `shell/settings/registry/*` stays the verbatim registry copy (B0045).

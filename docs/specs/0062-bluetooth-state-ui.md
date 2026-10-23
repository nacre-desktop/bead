# B0062 — Bluetooth state, power, discovery, connect

Status: **Review** (Claude worker, branch `task/b0062-bt-state-20261023`). Pin `edfda155` (Nacre N0047, `docs/specs/0047-bluetooth-welcome-osd-state.md`), up from `393b4aa6`; the lock changes only the `rev`. Builds on B0055 (list and pairing flow, which were waiting for this topic) and B0056 (parity harness).

## What N0047 serves, and what Bead now does with it

| Contract | Bead |
| --- | --- |
| Topic `bluetooth`: adapters and devices (`name`, `icon`, `paired`, `connected`, `trusted`, `pairing`, `battery`, `rssi`), connected first then by name; no address | The popout list is built from it. Rows sort connected, then devices that can be paired, then the rest, by name (B0055); at most 5 rows. `rssi` is not shown (the QML does not) |
| `bluetooth.set_power {adapter, powered}` (canonical) | Popout switch (as B0018) and the control-centre tile. **The tile no longer calls `bluetooth.set_enabled`** (the deprecated alias; Nacre deletes it 14 days after Bead's pill uses `set_power`: it does now). The tile needs the sole adapter's id from the topic, like the popout |
| `bluetooth.set_discovery {adapter, discovering}`, daemon-side stop after 60 s | **Find devices** in the popout header |
| `bluetooth.connect` / `.disconnect {device}` | **Connect** / **Disconnect** button on each paired device row |
| `bluetooth.pair` family (N0045) | B0055's flow, unchanged; **Pair** on unpaired rows |

## Behavior

- **Layout (as `NacreBluetoothPopup`)**: heading "Bluetooth", **Find devices** button and the power switch in one row; one 68 px card per device (glyph by BlueZ icon name, name, caption, button); then the empty text ("Bluetooth is off", "No known devices", "No Bluetooth adapter") and a message line. The popout is 352 px wide (the QML popup width; was 310) and its height follows the content. The B0018 summary block (big glyph, "Bluetooth on", "1 device connected") is gone: the QML popup has none. The popout still reads `status.*` for the uncontrolled variant only.
- **Row captions**: battery percentage with the QML's battery glyph (under 20 % `battery_1_bar`, under 60 % `battery_4_bar`, else `battery_full`), else Connected / Paired / Ready to pair. A connected row is `primaryContainer`.
- **Row button** (one host action per kind, resolved against the current topic when pressed): unpaired → **Pair** (B0055 flow); paired → **Connect**; connected → **Disconnect**; `pairing:true` → disabled "Pairing…". While a connect or disconnect call runs, that row shows "Connecting…" / "Disconnecting…" and every other button is disabled (no queue; one operation at a time, never retried). Buttons are disabled while the adapter is off, the topic is stale, the power call runs, a pairing runs, or the method is not advertised.
- **Find devices**: sends `set_discovery {discovering:true}` on the sole adapter; the label becomes "Scanning…" with a searching glyph and the button is disabled for as long as the topic says `discovering` (or the call is in flight). There is no Stop (the QML has none); the daemon stops the scan after 60 s and the topic flips back. Closing the panel stops a scan this panel started; a scan that already ended on its own sends nothing. Starting a pairing or a connection stops a running scan first (BlueZ is more reliable without one).
- **Errors** (message line under the list, cleared by the next action): scan could not start; connect or disconnect `busy` ("Bluetooth is busy. Try again."), `timeout` ("The connection timed out."), `stale_revision` ("That device is no longer available. Try again."), anything else "Could not change the connection.".
- Power failures and "power control unavailable" keep B0018's texts, shown under the header row.

## Split (framework vs Nacre edge)

- No engine or kit change. Luau: `shell/status/bluetooth.luau` (list and pair pages, sizes from the token scale; the list is a column of header, cards and texts whose heights add up to the panel height). Two actions registered in `shell/status/behavior.luau` (`bluetooth.scan`, `bluetooth.link`).
- Nacre edge: new `nacre-shell/src/status_views/devices.rs` (contract names, rows, scan, connect/disconnect, error map; the rows and glyph map moved here from `pairing.rs`), the stores and actions in `reload.rs`, the 352 px width in `popouts.rs`, and the tile in `control_center.rs`.
- Stores added: `bluetooth.scan_label|scan_enabled|scanning|empty_text`; row fields `kind`, `battery`, `action` (Pair/Connect/Disconnect text).
- 13 new `bead.panel.bluetooth-*` ids (en + context). Mirror: `python3 tools/i18n-sync.py NACRE --write` (lead).

## Checks

- Fake-daemon runtime tests in `reload/pairing_runtime_tests.rs` (5 new; the B0055 ones only gained the new methods in the fake Welcome and the new row fields): list follows the topic (device added later, adapter off, no devices, no adapter) with captions and battery glyphs; scan sends `set_discovery` once, shows Scanning…, follows the topic and the daemon's auto-stop, sends no call when closed afterwards; a refused scan shows its message; closing stops a running scan; connect and disconnect send the right method and device, show the busy row, disable the others, follow the topic, map `busy` and other failures; a running scan is stopped before a connection; nothing is sent without the advertised methods. Control centre: the tile calls `set_power {adapter, powered}`.
- 18 goldens `shell/tests/goldens/bluetooth-{list,scanning,off,waiting,pin,confirm,authorize,display,error}-{1,1.5}.png` (the pair pages are now 352 px wide), eyeballed. The fixture font has no glyph for `battery_1_bar`/`battery_4_bar` (its subset text has no digits): the goldens use `battery_full`; the thresholds are a unit test.
- CI-equivalent in the Arch container with `ttf-dejavu ttf-ibm-plex` and a git checkout for the spikes: see the report for the exact commands and results.

PARITY_SECTION

## Asks to Nacre / lead

1. Mirror the new ids (`i18n-sync.py NACRE --write`).
2. The daemon's `bluetooth` topic has no address, so the QML detail line with it has no Bead counterpart (N0047 records the gap).
3. Delete `bluetooth.set_enabled` 14 days after this lands and is trusted (the Nacre-side duplicate; Bead no longer uses it).

## Temporary duplicates

None added. `spikes/fake-bluez.py` is a test fixture for the private parity sessions (no real BlueZ), kept as long as `parity-compare.py`.

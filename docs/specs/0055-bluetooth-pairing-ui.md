# B0055 — Bluetooth device list and pairing

Status: **Review.** Consumes Nacre N0045 (pin `1432a440`, already Bead's pin since B0051): topic `bluetooth.pairing` and the methods `bluetooth.pair`, `bluetooth.pair.respond`, `bluetooth.pair.cancel`. Builds on B0018 (Bluetooth popout) and B0050 (secret hygiene, `kit.PasswordField`).

## What B0018 read, and what is missing

The Bluetooth popout read only the `bluetooth` state topic (`BluetoothState`: adapters, devices) and sent `bluetooth.set_power`; it showed a summary and the power switch, no device list. N0045 serves neither the `bluetooth` topic nor `set_power`/`connect`/`disconnect` ("reserved and unserved"). So against today's daemon the new device list is empty and Pair cannot start; everything is built and tested against the published `BluetoothState`/`BluetoothPairingState` shapes with a fake daemon. Asks to Nacre are at the end.

## Behavior

- **Device list** under the power row (from `bluetooth.devices` of the `bluetooth` topic): connected first, then devices that can be paired, then the rest, by name; at most 5 rows. A row has a glyph (headphones/keyboard/mouse/smartphone by the BlueZ icon name, as the QML list), name, caption (battery %, Connected, Paired or Ready to pair) and a **Pair** button on unpaired devices. The button is disabled while another pairing runs, the adapter is off, the topic is stale, or `bluetooth.pair` is not advertised. The popout grows by `rows x (56 + 8)` px.
- **Pair** opens the pairing page (replaces the list, like B0050's pages) and sends `bluetooth.pair {device}` without blocking: the call waits for the whole pairing, so the page follows `bluetooth.pairing`. The topic is leased only while a pairing runs (Unsub after it ends). The call deadline is `min(advertised, 120 s)` (the client refuses longer).
- **Page by prompt kind** (title "Pair with NAME", text, countdown "Expires in N s" from `timeout_ms`, ticking once a second only while a prompt shows):
  - waiting (no prompt yet): "Confirm pairing on your device." and Cancel;
  - `pin`: masked field with show/hide (`kit.PasswordField`), focus on arrival, Enter or **Pair**; 1..16 printable ASCII, else "Enter 1 to 16 characters." and nothing is sent, the text stays;
  - `passkey`: same field; digits only, 0..999999, sent as a number, else "Enter the six-digit passkey.";
  - `confirm`: "Does CODE match the code on your device?", **Confirm** / **Reject**;
  - `authorize`: "Allow this device to connect?", **Allow** / **Reject**;
  - `display`: "Type CODE on your device, then press Enter." and Cancel only (no response exists).
- **Cancel** (button or Esc) sends `bluetooth.pair.cancel` and returns to the list at once; Reject sends `accept:false` and does the same. The failure that follows is not shown, and the Pair buttons stay disabled until the waiting call has finished (no `busy`). Closing the panel mid-pairing sends the cancel too (the sent calls are kept alive on a short-lived thread).
- **Errors** stay on the page with the daemon's fixed strings mapped to messages: wrong code (retry), timeout (retry), busy (retry), rejected, cancelled, device gone (`stale_revision`), unavailable, anything else. **Retry** (only where the daemon says retry) leases the topic again and sends a fresh `bluetooth.pair`; Cancel closes the page. A respond refusal (`validation`) keeps the prompt open with "That answer was not accepted."
- Success returns to the list with "Device paired." No connect follows (QML trusts and connects: `bluetooth.connect` is unserved).

## PIN hygiene (as B0050)

PIN and passkey exist only in the `bluetooth.secret` input store (`*.secret`: not traced, not saved) until the respond call is built; the host clears it after every action (except a local format refusal, so the user can correct it), on Cancel, on Esc and on close. It is not in messages, store projections, `UI_STATE`, accessibility, IME context or logs (the daemon's `BluetoothPairRespond` also redacts it in `Debug`). Test: the PIN stays out of the state summary, a11y tree, saved panel state and the cancel call.

## Split (framework vs Nacre edge)

- `bead-ui` kit: new general `kit.ActionButton` (centred label, primary fill while armed, dimmed when it has no action; the helper B0050 kept local in `network.luau`, now shared). Added to ADR 0004's list and COMPONENTS.md. No engine change.
- Luau: `shell/status/bluetooth.luau` (list and every page; sizes from tokens, wrapped-text lines estimated from the text) and `shell/status/behavior.luau` (typed-field stores, local reveal toggle).
- Nacre edge: `nacre-shell/src/status_views/pairing.rs` (contract names, lease, flow, error mapping, rows) projects `bluetooth.view|devices|list_message|pair_*`; host actions `bluetooth.pair|pair_respond|pair_cancel|pair_retry` are registered under `bluetooth.control`.

## Checks

- Fake-daemon runtime tests (`reload/pairing_runtime_tests.rs`, 12): row order/captions/glyphs; no Pair without the method; confirm, authorize+reject, PIN (focus on arrival, bullets, sent once, cleared), bad entries sent nowhere, passkey as a number, display (Cancel only), wrong PIN then Retry (Unsub, fresh Sub and call), failure messages and retry flags, panel close cancels, hygiene (summary, a11y, saved state, Esc). Unit tests for the error map and glyphs.
- 14 goldens `shell/tests/goldens/bluetooth-{list,waiting,pin,confirm,authorize,display,error}-{1,1.5}.png`, all eyeballed.
- Full CI job in the Ubuntu 24.04 container on dev-2 (see the report for the result).

## Look-and-feel parity with QML (`NacreBluetoothPopup`, `NacreBluetoothPairing`)

Not compared side by side: B0056's `parity-compare.py` is not on main, and a nested comparison would need a BlueZ with devices on both sides plus the unserved `bluetooth` topic. Gaps known from reading the QML, to close in the nested comparison unit:

| QML | Bead now |
| --- | --- |
| Pairing block sits inline above the list in the 352 px popup; the list stays visible, disabled | Separate page in the 310 px popout (as B0050) |
| Buttons content-sized, left aligned: Confirm/Pair, Cancel | Two equal full-width buttons, primary first |
| Confirm mode: Confirm + Cancel (Cancel kills the agent) | Confirm/Allow + Reject (`accept:false`); Cancel elsewhere |
| PIN field 12 px radius, outline turns accent on focus, maxLength 16 | kit `TextInput` style; length checked on send |
| Rows 68 px cards, 12 px radius, 18 pt glyph, 9 pt caption, compact action button | 56 px, radius 8, 18 pt glyph, caption 0.85 x caption size |
| Order: connected, then alphabetical; list scrolls (max 420 px) | connected, unpaired, rest; first 5 rows, no scroll |
| "Find devices" button (discovery, 20 s) and empty texts ("Bluetooth is off", "No known devices", "No Bluetooth adapter") | Not ported: no discovery method in the daemon; the summary carries on/off |
| No countdown (hidden 60 s timer) | Countdown line (task requirement) |
| Pairing success trusts and connects | Back to the list with a notice |
| Page change instant | Same (no cross-fade) |

## Asks to Nacre (lead to record)

1. **Bug in N0045:** the daemon advertises `bluetooth.pair` with `timeout: 155000` (`PAIR_TIMEOUT + 5 s`), but the protocol validates a Welcome method timeout as 1..=120000. A fake Welcome with that value fails `framing::write` ("Invalid message fields"), so with BlueZ on the bus the daemon's Welcome would be refused. Advertise at most 120000 (a Call timeout is capped at 120000 too, so the pairing deadline is 120 s on the wire either way).
2. Serve the `bluetooth` state topic with the same device IDs as `bluetooth.pair` (BlueZ `dev_XX_...`), plus `bluetooth.set_power` (Bead uses it; N0045's text mentions `bluetooth.set_enabled`: reconcile), `connect`, `disconnect`.
3. A discovery method (`bluetooth.discover {adapter, on}`) so Bead can offer "Find devices"; `BluetoothAdapter.discovering` already exists.
4. Mirror the new ids: `python3 tools/i18n-sync.py NACRE --write` (33 `bead.panel.bluetooth-*` ids).

## Not done / notes

- No nested live visual check, no memory capture (relative gate open, CPU backend, would be compared with a control build on the same host): added state is a handful of stores, at most 5 rows, and one flow object that exists only while pairing (one lease, one 1 s ticker thread while a prompt shows).
- The harness frames show the pre-existing power switch clipped at the frame edge (as B0050 noted); not changed here.
- Remote-initiated pairing and authorization of already paired devices are not handled by N0045, so not here.

No temporary duplicates.

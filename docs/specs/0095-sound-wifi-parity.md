# B0095 — Parity capture and fixes for the sound streams, input meter and Wi-Fi views

Status: **Review**. Owner rule: Bead looks and feels like QML. B0086 built the per-application stream cards, the
input level meter and Wi-Fi Scan without comparing them to QML; this unit captures QML-left/Bead-right pairs and
fixes the visible differences. Evidence: `evidence/0095/` (`*-before.png`, `*-after.png`, `rmse.json`).

## Harness (spikes, no product code)

- `fake_system.py`: `pactl list sink-inputs|source-outputs` (Firefox 80 %, Spotify 50 %, OBS Studio 60 % on the pactl scale),
  a `parec` stand-in for the daemon's `audio.peak` (silence, or the level in `$FAKE_AUDIO_DIR/peak`), mute of the default
  source (`mute` step), and one unlinked `pw-cat` node per stream in the private PipeWire (started one at a time so node
  ids, and with them Quickshell's list order, repeat).
- `fake-nm.py --wifi`: a `wlan0` connected to "Home Wi-Fi", Guest and Workshop in range, Neighbour and Cafe appearing when a
  scan finishes (`RequestScan` takes 2 s, then `LastScan` moves), two saved profiles, a writable `WirelessEnabled` in the
  introspection (the daemon offers its Wi-Fi controls from it). `fake-nmcli.py` answers `device wifi list --rescan yes`,
  `device status`, the saved-SSID reads and field-selected `connection show` (the QML arm).
- `parity-compare.py --fake-wifi` and states `popout-audio`, `control-audio` (streams and meter), `*-muted`, `popout-audio-peak`
  (Bead only), `popout-network-wifi`, `control-network-wifi`, `*-scanned` (the scan is asked of the fake directly),
  `*-scanning` (a pointer click, see gaps). Pointer tools: `virtual-pointer`/`wlr-randr` as for B0067.

## What changed

| Difference found | Fix |
| --- | --- |
| Stream sliders showed the cube of QML's value (Firefox 51 % where QML 80 %) | QML's stream slider is Quickshell's volume, the cube root of PipeWire's: `control_center.rs` reads streams with the device `level()` and writes `linear(percent)` like a device. |
| Section headings 24 px, rows 3-5 px low, cards squeezed (column heights were too small), icon glyphs 3 px off | `shell/sound/body.luau`: text centred by its line box (about 1.295 x font size), heading one 12 pt line, device and level rows and stream cards at QML's offsets, 32 px mute buttons with a centred 17 pt glyph, exact column heights. |
| Meter caption column was 88 px wide, the track 35 px right of QML's | The caption is as wide as its text (estimated, see gaps) and the 5 px track follows after 12 px. |
| Wi-Fi list was flat, QML groups Connected / Known / Available (always all three, with a hint when empty) | `link_rows` carries `group` (saved profile with the same SSID = known) and `strength`; saved Wi-Fi profiles are shown through their access point (no duplicate "Workshop") and not at all out of range, as QML. `network-body.luau` draws the groups, signal glyphs by strength (>75 / >45 / else), 56 px rows, compact pills, and caps the popout list at 210 px in a scroll area (`network.scroll`). |
| Scan button pill/icon misplaced | The pill is a content-sized compact `ActionButton` (icon + label, 34 px). |

No kit component was added (pills are local to the view); nothing for ADR 0004's list.

## Before / after (whole frame / panel only, rmse %, QML | Bead, desktop, CPU renderer, 2880x1800 at 1.5)

| State | frame before | frame after | panel before | panel after |
| --- | --- | --- | --- | --- |
| popout-audio | 6.26 | 4.83 | 10.34 | 6.19 |
| control-audio (docked) | 6.23 | 4.56 | 10.45 | 5.57 |
| popout-audio-muted | 6.12 | 4.63 | 9.98 | 5.50 |
| control-audio-muted | 6.09 | 4.37 | 10.08 | 4.86 |
| popout-network-wifi | 9.65 | 7.45 | 18.47 | 13.34 |
| popout-network-scanned | 9.71 | 7.45 | 18.60 | 13.33 |
| control-network-wifi | 5.01 | 4.56 | 7.05 | 5.57 |
| control-network-scanned | 5.26 | 4.77 | 7.80 | 6.30 |

Before = origin/main `8b62f83` (binary and shell), after = this branch; same harness, one run per arm and state.

## Known gaps

- **During-scan state not captured.** A virtual-pointer click on Scan reaches neither shell in the nested session
  (no effect in QML or Bead; the `*-scanning` states exist for when it does). The pairs show the idle and the scanned list,
  with the scan requested from the fake NetworkManager. The dimmed button while scanning (QML `enabled: !busy`, Bead `scan_enabled`)
  is covered by `join_runtime_tests`, not by a picture.
- QML lists its own peak-monitor stream as a fourth card ("Quickshell Peak Detect", 0 %) while the meter runs; Bead hides its
  `nacre-peak` stream. Bead does not copy that. The `*-muted` pairs (no monitor, no card) are the clean comparison.
- QML's meter reads a PipeWire capture stream, which needs a session manager to link; the private PipeWire has none, so the QML
  meter stays empty in every pair. Bead's level (`level peak` step, `popout-audio-peak`) is checked by looking only. No cheap
  matching source exists; the meter measures the same quantity (sample peak of the default source). The 60 ms width tween is
  not ported (updates come every 66 ms, so it would be invisible); Bead has no per-property animation primitive.
- Stream order in QML follows PipeWire's object order; the harness fixes it by starting streams one at a time.
- Text widths are estimated (Bead sets the box; 0.49 / 0.45 x font size per character), so pills and the meter caption differ from
  QML by up to a few pixels and by more in other languages.
- The Wi-Fi popout keeps the "Hidden network" and "Saved networks" buttons (B0050), QML has none; the QML list shows a thin
  scroll indicator, Bead's scroll area none. The lock icon is not shown (QML never has the field). Back button focus ring and
  title offset on docked pages are B0091/B0094 gaps.
- Bar Wi-Fi icon differs (QML connected, Bead "off" in the fixture) and the update count: not this unit.

Temporary duplicates: none.

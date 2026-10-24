# B0083 — Weather location hint in Settings, i18n sync, parity-harness fakes

Status: **Review**. Three independent parts; no product visual changes beyond one new Settings row.

## 1. Settings: weather location

Owner decision 2026-10-24: IP-based weather location is being removed from Nacre entirely (a separate Nacre unit),
so Bead adds **no** consent switch. Settings keeps the city field (`date-time-language.locale.weatherLocation`, store
`desktop`, written with `settings.set`), the Fahrenheit units switch on the same page, and the weather-photo switch on
the Dashboard Weather sub-page (`weatherPhotos`, unchanged). Under the city field the description is now
"Set a city to show weather." (`bead.settings.weather-city-hint`, referenced from the Bead registry copy; QML's text
"City or coordinates used for forecasts." is replaced, and nothing mentions automatic location).
Test: `settings_pages_tests::weather_location_is_a_city_field_with_a_hint_and_no_automatic_lookup`. The Date, time and
language golden is refreshed.

## 2. i18n sync

`tools/i18n-sync.py <nacre> --write` copied Bead's `bead.*` tables (now including the B0082 battery estimate
keys, `bead.dashboard.weather-photo-credit`, the removal of `battery-charging-unknown`, and the Settings/Welcome/
Launcher/panel ids of B0063-B0081 that had not been synced) into Nacre `nacre/shell/i18n/{en,context}.json`.
Non-English tables do not exist in Nacre yet, so no translations were needed. Nacre's `tools/i18n/check.py`,
`generate.py` and the translation tests pass; `tools/i18n-sync.py <nacre>` (check mode) passes.

## 3. Parity-harness fakes for audio, battery and power

`spikes/parity-compare.py` runs a private daemon with no PipeWire, UPower or power-profiles-daemon, so the control
centre's audio, battery and profile pages could not be compared. Harness-only fixtures (one fixed machine for both
arms; values in `spikes/fake_system.py`):

| What | Fixture | Consumer |
| --- | --- | --- |
| `pactl` (wrapper on PATH, `spikes/fake_system.py`) | `--format=json list sinks|sources`, `get-default-sink|source`, `subscribe`; 2 sinks ("Fixture speakers", "Fixture headphones"), 1 source ("Fixture microphone"), all 45 % | Nacre daemon (Bead arm) audio producer (N0059) |
| UPower (`spikes/fake-power.py`, private bus) | DisplayDevice + `battery_BAT0`: 63 %, State 2 (discharging), TimeToEmpty 7800 s (2 h 10 m), `OnBattery` true | daemon battery producer and Quickshell UPower (QML arm) |
| power-profiles-daemon (same process, both names) | `org.freedesktop.UPower.PowerProfiles` and `net.hadess.PowerProfiles`: power-saver, balanced (active), performance; `ActiveProfile` is writable and signals | daemon `power.profiles` and Quickshell PowerProfiles |

The `level` step still changes the default sink/source level (the daemon re-reads after the event). The previous
stand-in (`fake-sink`/`fake-source` node names) is replaced, not kept.

QML audio: Quickshell reads audio through PipeWire, not pactl. The QML arm therefore gets a private PipeWire with
two null sinks and one null source of the same names (see "QML audio" under Evidence).

Tests: `python3 -m unittest discover -s spikes/tests -p test_fake_system.py` (pactl JSON fields the daemon parses,
defaults resolve to listed names, levels follow files, `subscribe` execs `tail -F events`, unknown commands are
silent, battery/profile constants). `fake-power.py` is py_compiled in CI (python-dbus is not installed there).

## Evidence

`docs/specs/evidence/0083/*.png` are QML (left) | Bead (right) pairs from `spikes/parity-compare.py` on the owner's
desktop (private nested niri, 2880x1800 at scale 1.5, the owner's presentation record, one run per arm; CPU-independent,
Bead on its default renderer; swap 13 MiB before and after; both arms started from one build of this tree plus Nacre
`c6dc627b`). `rmse.json` has the harness metrics; rmse is the per-pixel RMS over 0-255 on the whole frame, so the
shared wallpaper keeps it low. `bar_rmse` is the bar strip alone.

| State | rmse | changed % | bar rmse |
| --- | --- | --- | --- |
| idle | 3.890 | 7.64 | 6.92 |
| control-center (home) | 6.552 | 8.93 | 6.95 |
| control-audio (Sound detail) | 5.929 | 9.78 | 7.01 |
| control-battery (Power & brightness detail) | 4.508 | 7.20 | 6.97 |
| popout-audio | 5.862 | 10.21 | 6.96 |
| popout-battery | 5.040 | 8.04 | 6.96 |
| settings-date-time-language | 8.979 | 7.30 | 7.58 |

Same fixtures in both arms: header "63% 2h 10m remaining" matches, Sound lists "Fixture speakers"/"Fixture headphones"
and "Fixture microphone" at 45 %, Power & brightness has Power saver / Balanced (active) / Performance.

QML audio: Quickshell reads PipeWire, so the QML arm runs a private `pipewire` with null sinks/source of the same
names, a `default` metadata object naming the defaults, and `channelVolumes` 0.45 cubed (`spikes/parity-compare.py`
`start_pipewire`, config from `fake_system.pipewire_conf()`); Quickshell reads 0.45, so QML and the daemon agree on
the percentage. The QML arm also lists its own "Quickshell Peak Detect" stream under Applications (a Quickshell
artifact, not a fixture).

### Differences found (product visuals are not changed in this unit)

Control centre home:
- QML has the volume slider with device name and a thumb, Bead shows "Volume" with a thumbless bar; QML tile grid
  (Wi-Fi, Bluetooth, Airplane mode, VPN, Power mode, Stay awake, Do Not Disturb) and the Screenshot/Clipboard/Updates/
  Display settings/Wallpaper/Next wallpaper/Notifications quick-action grid are absent or reduced in Bead (Airplane, VPN
  and the grid do not appear; Wi-Fi, Do Not Disturb and Stay awake are dimmed); the Bluetooth subtitle reads "Fixture
  headphones" in QML and "Fixture" in Bead; the notifications header has DND/Clear pills in QML, icon buttons in Bead.
- Bar: Bead's updates counter shows 13 where QML shows 0 (different update sources in the private session), and its
  volume glyph is the speaker without waves.

Sound detail (also the Sound popout): Bead's sliders have no fill or thumb (the level is only the "45%" text), it adds a device caption under "Output" and "Input", and has no "Input level" meter; the Applications
list is empty in Bead (QML's one row is its own peak detector). The back button is an outlined square in Bead.

Power & brightness detail: Bead's three profile buttons are shorter, dimmed, with the icon at the left instead of
centered above the label (QML: taller, icon centered). Battery popout: same text ("63%", "2h 10m remaining", "Balanced")
and wrapping of "Performance", same card width, but Bead's profile buttons are dimmed with a left icon and its text
block sits 4 px higher.

Settings, Date, time and language: both show Language, Fahrenheit and Weather location; Bead's hint reads
"Set a city to show weather." where QML says "City or coordinates used for forecasts." QML shows the Date and time controls from `timedatectl` (unavailable in the private
session), Bead says "Time settings are not available on this system."

These are listed for follow-up units; none is a harness defect.

## Findings fixed on the way

- `status_views.rs`: with the audio topic live the control centre and the Sound popout reported `UI_ERROR unknown state
  audio.level` (their stores are declared from the first, unavailable projection, which lacked `audio.level` and
  `audio.level_available`). Both keys are now in the unavailable projection too, with a test that live keys never exceed
  the unavailable ones. This was invisible in unit tests (no live projection after an unavailable one) and only appeared
  with the harness fakes.
- `i18n.rs` test now also reads label ids from the Settings registry JSON.

## Temporary duplicates

None. The earlier minimal `pactl`/`wpctl` stand-ins in `parity-compare.py` were replaced in place.

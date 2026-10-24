# B0083 — Weather location consent in Settings, i18n sync, parity-harness fakes

Status: **Review**. Three independent parts; no product visual changes beyond one new Settings row.

## 1. Settings: weather location and IP-lookup consent

Nacre N0057 (`c6dc627b`) lets the daemon look up the city from the IP address (ipinfo.io) only when the desktop
setting `weatherIpLookup` is explicitly `true` **and** `weatherLocation` is empty. QML has no control for the
consent (N0057: "adding a Settings UI control is outside this unit"), so Bead adds it.

- Page: **Date, time and language**, section "Locale", the page where QML shows `weatherLocation` and
  `weatherFahrenheit` (units). The city field (`date-time-language.locale.weatherLocation`) and the units switch
  already existed; the weather-photo switch lives on the Weather sub-page of the Dashboard page (`weatherPhotos`,
  store `dashboard-extras`) and is unchanged.
- New row `date-time-language.locale.weatherIpLookup`, a switch on the `desktop` store, default off:
  label "Find location automatically (sends your IP address to ipinfo.io)", description "Used only while no weather
  location is set above. Off by default." Written with `settings.set` like every other row (`{store, key, value, rev}`).
- It is dimmed while a city is set (`weatherLocation` non-blank), matching the daemon rule; the stored value is kept.
  This uses a new general registry condition, `enabledWhenEmpty: <setting id>` (the inverse of `dependsOn`), read
  by `Registry::depends_on` in `settings.rs`. A string parent counts as set when it has non-blank text, a boolean
  parent when it is `true`. Nothing in the engine knows the weather ids.
- Wording: the label names what is sent (the IP address) and where (ipinfo.io). The new strings are Bead-owned
  `bead.settings.weather-ip-label` and `-description` ids (Bead's `shell/i18n` is the authoring source); the registry entry is a Bead
  addition to the Nacre registry copy, so QML's registry has no such row. Known gap: the city field's existing
  description ("City or coordinates used for forecasts.") is QML's; the daemon geocodes city names, so the word
  "coordinates" may overpromise. It is left as QML words it, and flagged for Nacre.
- Tests: `settings_pages_tests::ip_lookup_consent_is_a_switch_that_waits_for_an_empty_city` (fake daemon): default
  off and enabled with no city, exact `settings.set` call (`weatherIpLookup` true, fenced by `rev`), dimmed with a
  city, whitespace is not a city. The Date, time and language golden is refreshed.

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

See `docs/specs/evidence/0083/` (QML left, Bead right; rmse is the harness's per-pixel RMS over 0-255). Results and
the list of differences are appended below by the capture step.

## Temporary duplicates

None. The earlier minimal `pactl`/`wpctl` stand-ins in `parity-compare.py` were replaced in place.

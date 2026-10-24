# B0082 — Consume the N0059, N0056 and N0057 daemon producers

Status: **Review**. Pin `nacre-protocol` moved from `f3193ec0` to `c6dc627b` (Nacre origin/main
after N0055/N0056/N0057/N0059). No Nacre change is required; the asks are listed at the end.

## What the producers publish and what Bead reads

| Topic | Producer fields | Bead reading |
| --- | --- | --- |
| `audio` (N0059) | `nodes` keyed by PipeWire `node.name`, `name` = device description, `kind` output/input/playback/capture, `ready`, `readback.{linear,perceptual}_volume/muted`, `default_output/input` | Control tiles (`audio.output/input`: level = `readback.perceptual_volume`, else cube root of linear when `ready`; unready nodes with no readback are not a level), pickers (`cc.audio_devices`: output/input nodes only, in daemon order, selected = default, enabled = `ready` and `audio.set_default` advertised). Streams stay status rows. |
| `battery` (N0059) | `percentage`, `charging`, `state`, `time_to_empty_seconds`, `time_to_full_seconds`, `on_battery` | Control header percentage, glyph and QML estimate; popout estimate line. |
| `power.profiles` (N0059) | `active`, ordered `profiles` (id = power-profiles-daemon id; `name` may be the id or a display name), `degraded`, `holds` | Tile status/icon, picker, degradation line in the battery page. |
| `dashboard.weather_photo` (N0056) | `enabled`, `ready`, `status`, `location`, `asset{path,checked,author,license,source}` | Weather hero photo, credit and Commons link. |
| `dashboard.weather` (N0057) | additive `refreshing`, `current.latitude/longitude` | Verified: every field the dashboard reads (location, wmo, is_day, temperature, feels_like, wind, hourly `time/temperature/rain`, daily `date/low/high/rain`, description) is unchanged; the additions are ignored. `dashboard.updates.count` (N0055) already consumed by B0073; the refreshed fixture still decodes. |

Methods are unchanged: `audio.set_volume`/`audio.set_mute` take `{target: sink|source, value|muted}`
(the control-centre contract, not the `node` form of the reserved service-v1 schema), `audio.set_default`
takes `{node}`, `power.set_profile` takes `{profile}`. Tests send each through a fake daemon.

## Control centre

Most of the B0073/B0076 adapter was already shaped like the producers; what changed:

- Power profiles key on the **entity id** (`power-saver`, `balanced`, `performance`), not on the
  entry's `name`. A display name such as "Power saver" no longer drops the choice, and the call
  carries the id. Labels come from the existing `bead.osd.*` strings, so the tile reads "Power saver".
  Unavailable profiles are not offered.
- A live `power.profiles` topic without a backend (no power-profiles-daemon) keeps the tile with the
  QML wording "Power profiles unavailable": inert, no picker, no call. QML never hides this tile
  (it is always `available`, `supported()` is true), so Bead follows QML instead of hiding it. A stale
  or disconnected topic still yields no source (dimmed tile). Audio tiles (`hide_unsupported`) vanish
  when the audio backend is unavailable or there is no default input, as before.
- The header reads the battery topic: percentage, glyph from `on_battery` (QML: battery glyph on battery,
  charging glyph otherwise, including no battery), and QML's estimate ("2h 3m remaining", "30m until
  full", "Remaining time unavailable", "Connected to power") next to it. Without a battery the text is
  "Connected to power" and there is no estimate.
- The profile daemon's `degraded` reason appears under the picker in the battery page (QML shows
  `PowerProfiles.degradationReason`); a failed call's message replaces it.

Remaining local derivations are the protocol's own fallbacks (cube root for legacy volume, never zero for
unknown readback). No "unavailable" placeholder remains where a producer now answers; the
"Power profiles unavailable" string is only the unsupported-backend state described above.

## Bar popouts: one source per session (decision)

In a Nacre session the Battery and Sound popouts already read the daemon (`with_core_battery_status`,
`with_core_audio_status`, `power.profiles` through the shared control calls); `bead-services` is only the
`Legacy` path for standalone Bead. Both drive the same Luau, and a Nacre session has one source of truth per
topic. Recommendation: keep it so; do not replace the standalone path, since Bead is a framework that runs
without Nacre. The popout estimate line now uses the same helper as the control header (the previous
"Charging / Not charging" line was a stand-in for the missing estimate). Nothing else changed.

## Weather photo

- **Lease**: only the Weather page opens `dashboard.weather_photo`, with the page's own sources, so the
  subscription lives and dies with the visible hero (like QML's `syncPhoto`). The daemon enforces consent,
  sleep/lock and cancellation. If the topic is not offered the page works without it.
- **Display**: `ready` + `status:"ready"` + consent (`weatherPhotos`, same setting as before) + a valid file
  show the photo. Rust reads the file once per `checked` version, only from
  `<cache>/nacre/daemon-weather-photo/` (absolute, regular, not a symlink, ≤ 2 MiB, JPEG/PNG by content,
  ≤ 1400 per side) and hands the raw bytes to the scene (no re-encode); the renderer decodes it once to the
  display-cover size. `shell/dashboard/weather-sky.luau` crops it centred in the existing Scroll clip under
  the existing shade. Any other state, a missing or refused file, consent off or a disconnected daemon: the
  B0078 local-photo map, then the sky illustration, unchanged.
- **Credit**: "Photo: {author} · {license} · cropped/softened" (the QML text, new key
  `bead.dashboard.weather-photo-credit`, also used for local photos). With a daemon photo it is a transparent
  button; activating it runs `xdg-open` on the published source only if it is an `https://commons.wikimedia.org/wiki/File:`
  URL without query/fragment/spaces/controls (never in previews).
- **Memory**: the held bytes drop when the panel hides and with the view; the decoded raster follows B0078's rule.
  `memory_policy::tests::weather_daemon_photo_pixels_return_to_zero_after_close` (trace) passes with
  `open_bytes=3661488 closed_bytes=0` (a 960×720 photo plus the shade at scale 1.5), and
  `weather_daemon_photo_crops_credits_links_and_releases_the_asset` proves the scene bytes are weakly released.
  The open-phase image working set is the accepted B0078 class (about 2.2 MiB for a 4:3 photo at scale 1.5).

## Tests

Fake-daemon and fixture tests for every new field and action: PipeWire-shaped ids and unready nodes
(volume, mute, default, wrong kind, not ready, no readback, no input, backend down), profile ids/availability/
degraded/no-backend/stale, battery header and estimate (source, minutes/hours, unknown, missing, stale), Nacre's own
`core-audio`, `core-power-profiles` and `weather-photo` transcripts (vendored in `tests/protocol-v1`,
with the refreshed battery, updates and weather transcripts), popout estimate, degradation line,
photo states/consent/paths/links/hide, crop, credit link, scene placeholder swap and release. The five control
goldens changed only in the header (estimate text); regenerated and inspected.

## Validation

Full CI-equivalent in an Arch container (prod, `--cpus=12 --memory=24g`, Rust 1.99.0, DejaVu/IBM Plex fonts,
swaybg, wlr-randr, git checkout for the spike tests, Sway's file capability removed inside the container): `cargo fmt --check`,
clippy `-D warnings`, service-boundary check, `cargo test --workspace --locked` (nacre-shell 511 passed), trace
`memory_policy::tests::` (4 passed incl. the daemon-photo release), renderer feature checks, facade example,
`nacre-shell --check`, generated Luau schema and analyzer (0.740), spike/packaging suites, wallpaper smoke,
private services/tray, and `cargo deny` for the root and every framework crate. Build/test host only; nothing ran on the laptop.

## Evidence and parity

Daemon photo hero (synthetic 1000×100 JPEG, centre crop): [image](evidence/0082/weather-daemon-photo.png).

**Not compared against QML.** The desktop heavy lock was held by B0081's measurement arms, and a faithful pair needs
the harness's private daemon to see audio and power: the N0059 producers read `pactl` and UPower/power-profiles-daemon over
the private bus, which the parity harness does not fake yet (B0076 added fake light descriptors only). The photo needs
network. The header estimate is covered by golden and unit tests; its type size, weight and colour use the same tokens as
the QML header (percentage 10 pt medium, estimate 9 pt muted) but were not overlaid. A follow-up should add fake
`pactl`, a UPower battery and a profile daemon to `spikes/parity-compare.py`, then capture `control-center`,
`control-battery`, `control-audio`, `popout-battery`, `popout-audio` and `dashboard-1` with a cached photo.

## Temporary duplicates and expiry

None added. New `bead.*` strings (five battery estimate keys, the photo credit) are authored in Bead and
must be mirrored with `tools/i18n-sync.py NACRE --write`; the removed `bead.panel.battery-charging-unknown`
disappears there too.

## Asks for Nacre

None blocking. The Weather photo needs the daemon's `weatherPhotos` consent to follow the setting Bead already
reads; both do. Optional: publish `dashboard.weather_photo` `location` city equal to `weather.current.location`
(it is: N0057) so Bead could drop the local city map later.

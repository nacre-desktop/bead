# B0086 — Wi-Fi Scan, per-application audio streams and the microphone level meter

Status: **Review**. Consumes Nacre N0061 (`network.scan`, application stream nodes, stream
volume/mute, lease-only `audio.peak`). Closes the B0073/B0076 gaps "Wi-Fi Scan remains disabled",
"stream volume/mute and input peak are not ported". Control-centre layout, Settings, routes,
memory tooling and VPN are untouched.

## Behaviour

- **Scan networks** (bar popout and docked Wi-Fi page share `status/network-body.luau`): enabled while
  Wi-Fi is on, a Wi-Fi device exists, `network.scan` is advertised and nothing else (toggle, join,
  link) is in flight. A press sends `network.scan {device}`; the call in flight is the progress,
  because the daemon replies when NetworkManager has finished (QML ran `nmcli --rescan yes` and
  dimmed the button while busy; Bead dims it the same way and sets the accessible label to
  "Scanning…"). A refused or failed scan shows "Could not scan for networks." in the existing link
  message line. Edge: `status_views/scan.rs`; the view is Luau.
- **Application streams** (sound popout and docked Audio page, `shell/sound/body.luau`): one card per
  playback/capture stream, in daemon order, at most 12: themed icon (only when the icon theme has it,
  else the muted `apps` glyph as QML), name, volume slider, percent, mute button. The slider is the
  stream's **linear** volume because QML wrote the stream's own volume (devices use the perceptual
  cube-root scale). Calls: `audio.set_volume`/`audio.set_mute` with `target:"stream"` and the stream id.
  `cc.do` carries `toggle:stream:<id>` and `set:stream:<id>:<percent>` (ids contain a colon). Rows are
  inert when the stream is not ready. The old read-only `audio.streams` projection is removed.
- **Input level meter**: "Input level" caption and a 5 px track (`kit.LevelMeter`, now with optional
  `track`, `ink`, `label` props; one general kit change) under the microphone row. The `audio.peak`
  lease is taken only while the default input is ready and not muted (QML disabled its monitor when
  muted) **and** the audio view is shown: the popout holds it for the open panel, the docked page
  parks it (`Status::park`) whenever another page is current; closing drops it with the view.
  Nothing is always on and no new store outlives the view.
- Strings: `bead.panel.network-scan-failed`, `network-scanning`, `audio-input-level`,
  `audio-app-volume`, `audio-app-mute` (Bead tables; the Nacre mirror was added by the N0061 commit).
- Pin: `nacre-protocol` moved from `c6dc627b` to `4db189af` (Nacre origin/main with N0061, N0062, N0063 and the menu palette query).

## Validation

Remote Arch/Ubuntu CI-equivalent (see the report): fmt, clippy `-D warnings`, workspace tests (515
in `nacre-shell`), trace policy tests, renderer feature checks, Luau schema, spikes and packaging
suites, cargo-deny. New tests: stream rows and `stream` calls (including refusals of malformed ids)
in `control_center_tests`; Scan progress through a fake daemon (`join_runtime_tests`); the peak lease
following the live input, released on mute, with stream cards drawn (`audio_runtime_tests`).
New goldens `sound-streams-*` and `network-scan-*` (1 and 1.5), updated `control-docked-audio-*`.

## Parity and known gaps

- **Not compared against QML**: B0083's harness fakes for pactl/UPower/PPD have not landed, so no
  QML-left/Bead-right pairs were captured (the desktop would show no streams and no microphone).
  Compare once they exist: card geometry (64 px, radius 18, icon at 14/13, slider at 8/32, percent
  right of the 32 px mute button), the 5 px meter and its 100 px caption column.
- Slider look follows the kit (round thumb, 5 % steps) not QML's gradient track with a bar handle.
- No 60 ms width animation on the meter (QML `Behavior on width`); updates are at most 15 a second.
- The meter reads 0..1 sample peak of the default source; QML read PipeWire's node peak.
- `Scan networks` has no spinner glyph, only the dimmed state, as QML.
- Temporary duplicates: none.

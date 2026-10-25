# B0111 evidence

All images are private nested CPU sessions at 2880×1800, scale 1.5.
They are cropped to exclude unrelated system inventory. Original captures and
raw logs remain private. No physical scanout or laptop-cap result is claimed.

- `provenance.json`: source/build identity and frontend file hashes.
- `checks.txt`: final remote CI and focused dashboard outcomes.
- `bead-notifications-control.png` / `bead-notifications-candidate.png`: saved
  synthetic history stays on Loading in control and renders in candidate.
- `qml-notifications-instrumented.png` and `qml-notification-reader-*`: successful
  synthetic-history QML reference with a fixture-only readiness gate. The exact
  unified diff and original/modified file hashes are retained. The unmodified
  QML daemon-helper arm stayed on Loading and is not a content-parity pass.
- `notification-verification.json`: control/candidate fixture and Quick checks.
- `tab-measurements.json` and `{qml,bead}-tabs-{0,1,2,3}-4x.png`: real-font
  caption measurements and nearest-neighbour 4× crops, in Today/Media/Weather/
  System order. The `*-selected-4x-qml-left-bead-right.png` composites use a
  common absolute slot crop in both roles. Candidate states were captured before a later sidebar-only
  harness assertion failed; the failure does not invalidate these tab states.
- `notifications-paired-instrumented-qml-left-bead-right.png`: matching absolute
  crop of the successful synthetic-history reference and candidate.
- `peek-entry-timings.json`: four verified control-build entry measurements.
- `audio-entry.jpg`, `network-entry.jpg`, `bluetooth-entry.jpg`,
  `battery-entry.jpg`: QML left, exact reported control build right;
  actual content types were verified after correcting initial nominal labels.
- `notifications-entry.jpg`: QML left, fixed candidate right. This run's QML
  history fixture did not load (zero notifications), while Bead loaded one;
  use it for reveal direction only, not content parity. Timestamps are relative
  to sequence start before its trigger worker; 25 ms sampling is a proxy, not
  scanout or exact trigger latency. `peek-progress-frame-selections.json` records
  the chosen progress stages and actual timestamps.

Motion contacts show entry. Earlier exit sequences began before native panels
were fully open and are excluded from duration claims. The final semantic bell
entry/exit check waits for prepared content, then verifies open/closed state.

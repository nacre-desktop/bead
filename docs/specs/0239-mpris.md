# 0239 — MPRIS media service

`bead_services::media` is a separate session-bus service over the public freedesktop MPRIS `org.mpris.MediaPlayer2` and `.Player` interfaces. It discovers player names at start and on owner changes, reads initial properties, follows property and `Seeked` signals, and drops vanished owners. Consumers receive immutable snapshots and can send PlayPause, Next, Previous, and track-bound SetPosition actions. Position timestamps use the monotonic clock; clients extrapolate while Playing. No periodic D-Bus polling occurs.

The active player is the one most recently observed entering `Playing`. When none is playing, the previous active player remains selected while present; otherwise the first player by bus name is selected. Folder apps opt in with `[services].desktop = [..., "media"]`; only those apps receive the active player's raw stores and host actions. Their playing position store is extrapolated at 1 Hz without D-Bus traffic.

Out of scope: artwork downloads/caching, lyrics, per-app player preference, TrackList/Playlists, MPRIS volume, and product-specific behavior. Media remains separate from `Config` and `Snapshot` to preserve existing struct-literal compatibility. No temporary duplicate implementation was introduced.

Evidence: implementation follows the public [MPRIS v2.2 specification](https://specifications.freedesktop.org/mpris/latest/) and Bead's existing service and folder-host patterns. Private-bus lifecycle/action tests and controlled-clock projection/tick tests are run by `tools/check-private-services.py` and the Bead CI checks.

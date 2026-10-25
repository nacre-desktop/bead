# B0131 evidence

`spikes/pearl-release-proof.py` wraps the existing guarded parity harness.
The final cohort uses private nested niri at 2880×1800, scale 1.5, CPU rendering,
and direct relative virtual-pointer input with a held button. Screencopy ready
supplies sample timestamps; the C tool supplies the release timestamp. Source
ordering tests cover stale placement, configure/bootstrap, failed/busy draws and
retirement after content commit. Fifteen classifier tests cover the measurement.

[Aggregate](release-summary.json) and [compressed frame data](release-trials.json.gz)
retain all 20 trials. Decompress the latter with `gzip -dc release-trials.json.gz`.
The 1644×879 measurement is an interior fill bounding box; the requested window
is 1100×590 logical (1650×885 physical). Raw forward movement before native
handoff remains recorded in every trial; blanks and reversals are gated from
exact input release, stationary position/size from native content commit, with
at least eight post-handoff samples. Handoff takes 1.031 s median. Long capture
and render gaps prevent a pacing/latency acceptance claim; refresh_ns is zero.

[Paired crops and functional outcomes](pairs/README.md) cover idle, open, hover,
held drag, detached and redocked endpoints, including lower joining contours.
Control and AI commit native content and restore docked state; AI's detached
flag clears. Bead's 300 ms host easing measures 306 ms in its margin trace.
The correctly routed QML API dock leaves no visible attached launcher at the
endpoint, so no successful QML return trajectory or full visual parity is claimed.
Drag starts differ: Bead uses a valid input region, QML its header with Meta.
Interior hover/focus states therefore differ; these are contour/placement
comparisons, not a full pixel-style parity pass. Frontend definitions are unchanged.

[Complete check replica](full-ci.sh) and [receipt](ci.txt) describe full remote
Arch CI. Test threads are serialized after shared-host parallel runs intermittently
exceeded the existing 50 ms Luau evaluation budget; the product budget is unchanged.
The capture build's binary SHA and source manifest SHA are distinct recorded
identities. Runtime source includes the cold-size and upstream spotlight repairs.
Capture build: `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_STRIP=debuginfo cargo build -p nacre-shell --locked -j2` (unoptimized development profile).
Memory rows cover the main UI process; daemon/provider totals are separate.
The later Today/notification/power rebase changes separate UI modules; the
combined tree passed another full CI run. Runtime captures precede that rebase.

Full-output screenshots remain private because provider content includes a host
package update inventory. Published crops omit that content and the top bar.
No live installation or desktop configuration was changed. Memory phases are
CPU information without matched controls; no relative gate or laptop-cap pass.

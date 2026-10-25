# B0108 evidence

Pairs place QML on the left and Bead on the right. All inputs are native
2880×1800 captures at scale 1.5. Title crops cover `(1000,0,1880,75)`;
navigation crops cover `(700,75,2180,175)`. Full-frame SHA-256 hashes and dimensions
are in `capture-metadata.json`; original page bodies remain private.

`series-*` sheets sample the 25 ms frame series, including intermediate pearl
positions. JSON contains every measured pearl position. Timestamps estimate
screencopy time from process launch/completion; they do not measure scanout.

The capture adapter imports the existing parity driver and safety guard from
Bead `0ce6f33` (driver SHA-256
`8674c12f55b07bd81a323d56f671ad496cf7df4296cf1de8ae7cc42c517a6735`).
It uses normal complete pointer gestures: the private pearl-drag proof flag would
consume ordinary panel clicks. `--trace-state` supplies the committed state oracle.
QML and Bead use registry indices Today=0, Weather=1, Media=2, System=3 despite
the displayed Today/Media/Weather/System order. Native sequence scale is 1.5.

Run `capture-arm.sh` from a prepared private lab root containing this adapter,
`candidate/spikes`, Nacre reference sources, selected shell bundle/binaries and the
existing pointer/output helpers. Take the shared desktop-heavy lock per command;
the existing guard supplies cgroup containment, swap and memory watchdogs.
Memory arms set `B0108_MEMORY=1`, use five alternating control/candidate pairs,
and sample idle/Today/Media/Weather/System/closed five times each. `analyze-memory.py`
computes the median of paired phase medians. CPU backend and all memory components
are recorded. Rejected candidates are retained separately rather than discarded.

`check.sh` is the complete remote Arch CI-equivalent invocation. No tests, builds
or compositor sessions ran on the laptop. Pilot failures (missing private helper
PATH entries, logical-resolution screencopy, wrong tab-index oracle and intercepted
private-proof clicks) were corrected before the reported native captures.

The visual revision is `da5473b`; final memory/CI revision is `4a4ed0c`:
Media +0.513672 MiB median exceeds +0.500 MiB by 14 KiB. Its visual rerun did
not start during the original gate stop. Lead-accepted 2026-10-25: the lead judged
the 14 KiB excess within measurement noise and authorized integration. No live
change is authorized or claimed.
`capture-arm.sh` expects the existing owner-approved `swap-preflight.py` in the
private lab root. Reported arms had zero swap used, so no swap reset was needed.

After lead acceptance, `accepted-refresh/` records a successful quick final-code
`4a4ed0c` capture: idle, title hover-open and Media selection, plus sampled series.
Its native image hashes, binary hash, state errors and containment result are in
`metadata.json`. The first refresh arm passed visual states but hit a disconnected
private GVFS mount during cleanup; the retry disabled GVFS mounting in that arm's
environment and exited 0. No host or live settings were changed.

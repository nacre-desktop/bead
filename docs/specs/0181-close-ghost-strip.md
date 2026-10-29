# B0181 — Ghost strip after panel close

Status: implementation, route smoke and full Bead CI pass. Paired Bead compositor
visual evidence remains open because the private UI-provider handshake timed out.

The launcher contact sheet shows its header surviving at the bottom after the
shrinking body disappears. Reveal motion kept the body/shadow reshaped in place
while translating ordinary content behind a shrinking clip. On close, that split
could expose content at the edge. Revealing openings remain unchanged; closes
restore the full scene geometry and translate all primitives together.
The control-centre sheet does not show the same lingering strip; its surface is
gone by frame 2 and stays gone.

## R2 — decisive axis-locked exit

The follow-up contact sheet shows the complete panel still present, with a slow
ease-out tail and horizontal recentering during its bottom-edge close. The shared
motion selector now gives exits an emphasized-accelerate `t^4` curve and a
shorter duration (70% of open, capped at 220 ms; authored return durations are
capped at 75% of open). The renderer was recalculating horizontal centering from
the live surface width on every vertical-motion frame, overwriting the stationary
x pose. Vertical exits now retain their last open x origin; side exits retain y.
Opening centering is unchanged.
The earlier whole-scene translation and transparent terminal frame remain.

`Scene::panel_offset` is shared by panel and overlay rendering. Surface placement
uses its single returned offset; close mode disables the reveal clip and keeps
opacity at zero on progress 0. Existing Wayland unmap gating requires the final
draw to finish before unmapping. No Nacre-specific UI or new primitive added.

| Check | Result |
|---|---|
| R1 120 Hz close-pose scene test | Pass — 37 samples at 8.33 ms; terminal opacity is zero |
| R2 120 Hz close pose | Pass — 27 frames over 217 ms; last 3 cover ≥30% of travel; bottom x fixed |
| R2 axis anchor | Pass — bottom close x fixed, right close y fixed |
| B0178 scheduler proof | Pass — `companion_slide_callbacks_at_120_hz_keep_committing_without_staging` |
| B0180 real-shell route smoke | Pass — 12 routes open/close with prepared scenes; 0 UI_ERROR |
| Full Bead CI | Pass — workflow-equivalent CI recipe, `FULL_CI_PASS`, exit 0 |
| B0178 120 Hz scheduler proof | Pass — retained 40-frame scheduler test |
| B0180 route smoke | Pass — 12 routes open/close with prepared scenes and no runtime errors |
| Full Bead CI | Pass — 784 passed, 2 ignored; `FULL_CI_PASS`, exit 0 |

The QML-only arm completed the 19 selected states and saved close sequences in
task-local scratch. The paired Bead compositor arm did not reach route driving: `ui.dispatch`
timed out while establishing the Nacre UI provider with both current-main and
protocol-pinned helpers. Bead error-panel captures were discarded. Paired
compositor screenshots remain a known gap; the route smoke above exercises the
actual Bead Luau entries and prepared CPU scenes without that external provider.

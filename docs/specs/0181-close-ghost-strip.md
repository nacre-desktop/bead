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

`Scene::panel_offset` is shared by panel and overlay rendering. Surface placement
uses its single returned offset; close mode disables the reveal clip and keeps
opacity at zero on progress 0. Existing Wayland unmap gating requires the final
draw to finish before unmapping. No Nacre-specific UI or new primitive added.

| Check | Result |
|---|---|
| 120 Hz close-pose scene test | Pass — 37 samples at 8.33 ms; terminal opacity is zero |
| B0178 120 Hz scheduler proof | Pass — retained 40-frame scheduler test |
| B0180 route smoke | Pass — 12 routes open/close with prepared scenes and no runtime errors |
| Full Bead CI | Pass — 784 passed, 2 ignored; `FULL_CI_PASS`, exit 0 |

The QML-only arm completed the 19 selected states and saved close sequences in
task-local scratch. The paired Bead compositor arm did not reach route driving: `ui.dispatch`
timed out while establishing the Nacre UI provider with both current-main and
protocol-pinned helpers. Bead error-panel captures were discarded. Paired
compositor screenshots remain a known gap; the route smoke above exercises the
actual Bead Luau entries and prepared CPU scenes without that external provider.

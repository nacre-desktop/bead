# Sanitized paired contours

Each PNG is a 48×48 logical (72×72 physical) crop around the upper-left contour,
or the lower-left join for `*-bottom.png`. Crops cover idle, open, hover, pressed,
detached and redocked states, omitting text, provider inventory and the top bar.
`pair-summary.json` records geometry and the control/AI API outcomes.

QML is pinned to the public source recorded in the summary; the Bead state and
return captures use source `2d6cb5c7`. Both private sessions use the same output,
scale, palette and wallpaper. Full screenshots remain private. These crops
compare contour placement, not complete pixel-style parity: gesture input
regions and hover/focus content differ.

Both return trajectories are monotonic and blank-free, using 180 ms OutCubic.
QML/Bead settle within 2 px after 182.964/135.557 ms respectively. Sampling gaps
make the first changed image an upper bound on onset, not a measured stall.
Control and AI cold API detach commit native content and restore docked state;
AI's detached flag clears. No daily desktop changes were made.

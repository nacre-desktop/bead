# B0102 paired evidence

QML is left; Bead is right. Both private nested sessions use a complete 2880×1800
output at scale 1.5 and the same wallpaper/palette record and fixture programs.
Bead uses the CPU renderer (`--no-default-features`, debug build, stripped).
Clock and playing-track position advance between arms. System display data is
synthetic; native source transports still run. No live desktop session is used.

RMSE is computed from native RGB PNG pixels on the 0–255 scale, with no resizing,
alignment correction or dynamic-content masks. The dashboard crop is physical
`[702,75,2178,1050]` (984×650 logical). The JPEGs are quality-85 display copies.
Native PNG SHA-256 values and output geometry are in [visual.json](visual.json).
The idle result shows the pre-existing whole-output/background contribution.

| State | Whole output RMSE | Dashboard RMSE | Images |
|---|---:|---:|---|
| Idle | 9.5610 | — | [whole](idle-whole-window.jpg) |
| Today | 15.1960 | 23.1519 | [whole](dashboard-0-whole-window.jpg), [crop](dashboard-0-dashboard-crop.jpg) |
| Media | 15.5028 | 23.8734 | [whole](dashboard-1-whole-window.jpg), [crop](dashboard-1-dashboard-crop.jpg) |
| Weather | 17.3090 | 27.9897 | [whole](dashboard-2-whole-window.jpg), [crop](dashboard-2-dashboard-crop.jpg) |
| System | 19.8144 | 33.4434 | [whole](dashboard-3-whole-window.jpg), [crop](dashboard-3-dashboard-crop.jpg) |
| Today hover | 15.2909 | 23.3698 | [whole](dashboard-hover-whole-window.jpg), [crop](dashboard-hover-dashboard-crop.jpg) |
| Today pressed | 15.3477 | 23.5036 | [whole](dashboard-pressed-whole-window.jpg), [crop](dashboard-pressed-dashboard-crop.jpg) |

[Capture provenance](captures.json), [memory components](memory.json),
[memory summary](memory.md), [CI commands](check.sh), [CI results](check-results.txt).
The [specification](../../0102-dashboard-owner.md) lists the remaining visual gaps.

[Opening samples](motion-open.jpg), [closing samples](motion-close.jpg), and
[timing records](timing.json) retain the short transition comparison. Sample times
are nominal frame-index times, not physical presentation timestamps. Bead remains
on its fallback during the sampled 1.4-second cold open; its close is also later
than QML in that window. These timing differences remain unresolved.

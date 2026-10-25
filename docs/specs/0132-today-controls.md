# B0132 — Today controls

Today now has the Colors button and five independently clickable palette dots.
They use the published `ui.dispatch` → `colorsApp.open` route. The existing media
buttons remain in place and follow `dashboard.player`: previous, play/pause and
next, with capability checks and disabled controls when the player disappears.
Disabled icons now use QML's 35% ink opacity. All controls activate on release.

Colors matches QML's 111×36 compact button, radius 10, caption/icon theme tokens,
surface/accent tint and hero position. Media retains 34 px circles, 6 px gaps and
17 pt icons. UI/layout stays in Luau using kit components; Rust adds only the
Nacre route adapter and availability projection. No new primitive or dependency.
Integration keeps the still-used Colors accessible label after Welcome cleanup;
the fake-daemon test identifies Colors controls by action rather than English text.
The old palette boxes are replaced in this unit; no temporary product duplicate.

Reference: Nacre origin/main `836df1a`, `NacreTodayPage.qml`'s `WallpaperAction`
and `overview/NacreMediaCard.qml`'s `Transport`. Media controls already existed
in Bead's source and goldens; this unit verifies them instead of duplicating them.
The fake-daemon test checks the visible actions, all three transport calls, all
six Colors targets, playing→paused glyph updates and disconnect disablement.

## Evidence

[Full remote Arch CI](evidence/0132/checks.txt) passes on integrated code `1904f96c`; its
exact tree matches the tested snapshot. Four 1×/1.5× Today goldens were refreshed
and inspected. No local build/test, installed shell or live desktop change.

Private nested niri, CPU, 2880×1800/1.5, same wallpaper/palette/settings, playing
MPRIS fixture and synthetic host display values. Per-command blocking flock,
swap preflight, containment/watchdog and full-output geometry gates pass.
QML is left, Bead right: [whole Today](evidence/0132/dashboard-0-whole.jpg),
[native dashboard crop](evidence/0132/dashboard-0.png),
[media hover](evidence/0132/today-media-hover.png) / [pressed](evidence/0132/today-media-pressed.png),
[Colors hover](evidence/0132/today-colors-hover.png) / [pressed](evidence/0132/today-colors-pressed.png).
[Native PNG hashes, crops and metrics](evidence/0132/parity.json) include idle.
JPEGs are display copies; PNG crops are unscaled. Clocks/position advance.
[Open](evidence/0132/motion-open.jpg) / [close](evidence/0132/motion-close.jpg)
show QML above Bead at nominal 25 ms capture stamps, not physical scanout timing.

Existing Today differences remain: hero photo/shade/rim, wallpaper actions,
palette caption, weather detail, logo/gradient rings, some icon/text baselines
and the private profile's stale-data footer. Shared-kit pressed tint differs
from QML's 12% state overlay; hover/pressed timing and the earlier cold/warm
motion gaps are not newly accepted here. Media hover shows a clear Bead fill
without a clear QML fill in this private-input capture; identical hover behavior
is not established. This unit keeps the rest of Today intact.

## Memory

New-control budget: ≤1 MiB unreclaimable shell-tree residency per phase.
Five alternating matched CPU pairs pass both the +0.5 MiB relative gate and
1 MiB feature budget. Each phase uses five samples of all descendant shell
processes; unreclaimable is anonymous + SHM + DRM + SwapPss. No samples excluded.

| Phase, MiB | Control median | Candidate median | Median paired Δ |
| --- | ---: | ---: | ---: |
| Idle | 40.332 | 40.453 | +0.168 |
| Idle closed | 40.332 | 40.250 | +0.168 |
| Today open | 69.186 | 69.303 | +0.117 |
| Today closed + idle | 59.232 | 59.318 | +0.066 |

[All pairs and component samples](evidence/0132/memory.json) retain the spread:
first-pair open +1.027 MiB is SHM (+1.031), with anonymous −0.004. Median SHM
deltas are zero; the small median increases are anonymous. DRM/SwapPss are zero.
File PSS is informational. Paired medians need not equal differences of medians.
Source is `99640153` plus this unit only (tree `607a2aaf`), before subsequent
main integrations; these isolate the controls rather than accepting combined-main
memory. Today's visual/layout and adapter are unchanged by those integrations. No laptop
absolute-cap or Hyprland acceptance is claimed by this private niri capture.

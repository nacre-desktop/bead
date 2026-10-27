Status: INCOMPLETE — safe-pushed to main; memory-control, analog smoothness and app-window-size acceptance gates remain open.
Slices: Clock, Battery, Controls, Feeds and Widgets app.
Code/evidence commits: b6e84db7, e0ee98e7, 369ec980, 76334baf, 955d3cb3, 839fc185; all contain `[no-publish]`.
Push: fast-forwarded code/evidence `ba77e92d..839fc185`; this report-only `[no-publish]` commit follows.
Full CI: Ubuntu 24.04 on dev-2, `bash /ci/full-ci-final.sh`, PASS (`CI_EXIT=0`) on final evidence revision `839fc185`; the following commit changes only this report.
Text root cause: eight hard-offset halo copies visually outlined Outfit-200; replaced by a blurred glyph-alpha mask behind solid ink. Digit-interior and soft-shadow goldens pass.
Captures: `docs/specs/evidence/0161/bead/{Main,Clock,Battery,Controls,Feeds,App}.png`, 2880×1800/1.5 CPU; all complete, zero OOM, zero host swap.
Pairs in `docs/specs/evidence/0161/`: `comparison/Main.png` ↔ `reference/Main.png`; `comparison/Clock.png` ↔ `reference/Clock.png`; `comparison/Battery.png` ↔ `reference/Battery.png`; `comparison/Controls.png` ↔ `reference/Controls.png`; `comparison/Feeds.png` ↔ `reference/Feeds.png`; `comparison/App.png` ↔ `reference/App.png`.
Memory: one CPU sample each; Empty 48.400 MiB unreclaimable. Deltas vs Empty: Clock +1.936, Battery +4.934, Power +4.494, Sound +7.852, Bluetooth +4.150, Wi-Fi +6.713, Notifications +3.518, Launcher +5.633, Updates +5.039 MiB. File PSS 56.294–56.885 MiB (informational).
Memory gate: ≤2 MiB always-on delta versus a same-host control was not isolated; open.
Analog, synthetic 60 Hz: five windows/phase; commit p50 178.06–178.22 ms, p95/p99 209.17–209.36 ms, estimated drops 91.2%, render p99 31.2 ms; fails target.
Analog, synthetic 120 Hz: five windows/phase; commit p50 178.35–178.46 ms, p95/p99 209.40–209.46 ms, estimated drops 95.4%, render p99 31.2 ms; fails target.
Pacing used CPU; presentation refresh timestamps are unavailable (`refresh_ns=0`), so no physical scanout claim.
Visual: digits are solid, shadows soft, and app tiles do not overlap/clip; the 926×1108 logical window is smaller than the reference (open). No equivalent QML widgets/editor exists; Bluetooth/Wi-Fi/update capture data is unavailable.
Removed: 47 product lines.

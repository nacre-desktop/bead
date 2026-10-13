# Status

2026-10-13: Feature 1 built and locally validated. Public repository; ADR 0001
approved with amendments. Feature 1 integrated and pushed; CI passed for code
8a5069d in run 38391925119. Final screenshot/handoff update is documentation only.

Done: MIT/Apache bead-wayland + GPL nacre-shell, full-width 32px strip per output,
no keyboard focus/input capture/reserved space; event-driven idle; signal cleanup.
Nested Hyprland second-output add/remove/re-add and niri scale redraws passed.
Three matched 60-second Bead/QML samples on each compositor; Bead exits 0 and
releases all surfaces. fmt/clippy/tests/cargo-deny pass; strict framework license
check rejects GPL in its negative test. Details: [FEASIBILITY](../FEASIBILITY.md).

Issues: headless labwc parent gives no Hypr outputs; desktop-nested Hypr works.
Hypr screenshot requests time out for both clients; bounded, explicitly reported.
Its backend also logged an xdg configure warning on preview shutdown.
No animated pacing/text/emoji or reload claim. Nacre/Orient remain read-only.

Run from a Wayland terminal: `./scripts/dev.sh niri` (or `hyprland`). Inspect the
dark strip at the top of the separate nested desktop. Close its window or Ctrl+C
to stop. No session config/autostart/install changes. Existing Rust/labwc used;
cargo-deny 0.20.2 added locally, no system packages installed.

Final cleanup found/stopped an old QML screenshot attempt. One fresh 60-second
pair per compositor supersedes earlier PSS: Bead 1.06/1.10 MiB, QML 71.52/70.95.
QML/niri one CPU tick; others zero. See FEASIBILITY for startup/USS and caveats.

Next action: **WAIT for next/feedback.** Feature 1 complete; do not start Feature 2.
[Screenshot](../examples/feature1.png): dark top strip is Bead; central overlay is niri.
Feature 2 must add drawing/palette and stress a launcher-sized rounded/shadowed,
lots-of-text/color-emoji panel with damage tracking at real 2880×1800 / effective
1.5 scale / 120 Hz. Report p50/p99, CPU and loaded-font PSS. Tell owner if CPU
cannot keep up before Feature 3. Do not start Feature 2 without next/feedback.

Policies: no new ledgers; cargo-deny in CI. CC0/0BSD/Unlicense/MIT-0 permitted.
niri-ipc adapter belongs in GPL nacre-shell behind neutral Bead interface in
Feature 5; libmpv later. Each future feature log under about 40 lines.

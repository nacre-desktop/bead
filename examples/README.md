# Nested previews

From a Wayland terminal, `./scripts/dev.sh niri` or `./scripts/dev.sh hyprland`
opens a separate nested desktop with the blank strip. Close the nested window
or press Ctrl+C to stop. These fixtures are never installed as session configs.

Feature6 reference: `bar-reference-niri.png` and `bar-reference-hyprland.png`
show the unchanged Nacre QML header with deterministic private service inputs.
Capture with `NACRE_REFERENCE=... python3 spikes/measure.py --bar-reference`;
add `--compositor hyprland --virtual120` for the private Hyprland capture.

Native Feature6 bar: `bar-{niri,hyprland}-{cpu,gl}.png`. Use
`python3 spikes/measure.py --bar-ui` in the private measurement harness;
Hyprland adds `--compositor hyprland --virtual120`. Build the release shell
and `bead-services --example tray-fixture` first. The screenshot fixture
uses private buses and unavailable private PipeWire, never daily services.

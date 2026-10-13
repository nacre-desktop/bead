# Status

2026-10-13: Feature 2 implemented and measured. **Concern: CPU drawing misses120Hz.**
ADR0001 approved; no Feature3/GPU follow-up authorization yet. Stop for feedback.

Done: bead-render scene/renderer boundary; rounded corners/borders, cached SVG
Gaussian shadow, cosmic-text fallback/color emoji, SVG/PNG/theme icon loading;
Orient format 1 JSON consumer, no Orient engine duplication. Fractional-scale+
viewporter with buffer_scale=1; direct PixmapMut over SHM, ABGR preferred or damaged
ARGB swizzle. Three buffers reused, each accumulates damage since its last use.
Full-vs-partial pixel equality includes text/images/shadow/edges at 1.5 in both formats.
Nested niri RGBA and Hypr ARGB static 1.5 proofs passed, all own processes stopped.

Real monitor stress completed:2880×1800/1.5/120Hz,1100×590 logical panel. 60s
untraced client CPU damageON51.30%,OFF59.18%,QML5.75%; PSS42.49/42.44/105.60MiB.
Draw p99 ON23.63ms/OFF24.61ms versus8.33ms budget. Matched10s callback p99
25.51/29.05/9.22ms. Startup400.11/404.08/143.25ms, callback log not photons.
Private nested idle60s recorded zero ticks; fonts/geometry differ from live stress.
Exact scope, USS, memory variation and limitations: [FEASIBILITY](../FEASIBILITY.md).
Raw evidence retained in ignored canonical .research/feature2; not new ledgers.

Owner stopped white live overlays via another active chat. **Do not respawn on
active niri.** Future checks private/nested only unless owner authorizes otherwise.
Never stop installed Nacre or modify configs/keybindings/autostart. No system
packages added. Installed-font experiment tracks unmaintained ttf-parser through
an explicit cargo-deny exception; MPL icon resolver dependency belongs only to
GPL shell. Framework graphs remain permissive. SVG text/embedded images untested.

Run from a Wayland terminal: `./scripts/dev.sh niri` (or `hyprland`). Look for the
sliding drawing panel, readable fallback/emoji, corners/shadow and sample icons.
Maximize the nested window if needed to see the full panel. Close it or Ctrl+C.
[Private nested screenshot](../examples/feature2.png). This is not a working launcher.

Local fmt/clippy -D warnings/workspace tests/cargo-deny pass. Feature code
0f71173 integrated and pushed; CI38400965824 passed (fmt/clippy/tests/cargo-deny).
Final handoff only adjusts preview configs/docs; verify its exact-head CI and stop.
Active isolated worktree: bead-feature2. Next proposed step: matched GPU panel
comparison before Feature3. Owner decides; no further feature work now.

Repository remains public. Original seven commits were recreated with identical
trees/messages/dates under Nacre contributors/noreply author+committer identity.
Old SHA APIs unavailable; old public HEAD page404. Repo-local identity protects
future Bead commits; global Git identity unchanged. Old CI/commit links historical.
No releases/packages/crates.io. Policies: cargo-deny/no new ledgers; short logs;
GPL niri-ipc/libmpv adapters later in shell behind neutral Bead interfaces.

Default preview corrected to an existing Orient dark palette matching all six
active Nacre rendering roles. Matched measurements used the retained light
spikes/palette-stress.json input; future benchmark runs explicitly select it.

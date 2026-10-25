# B0120 — Status pill optical alignment and workspace wallpaper continuity

The owner requested even visible gaps, overriding B0101's QML cell-spacing match.
Keep icon/count pairs tight and align IBM Plex Sans numbers with Material Symbols
Rounded at scale 1.5. Click/focus (B0119), frame cost (B0117), and the widget port
(B0113) are outside this unit.

## Changes

The shell widgets' Luau tokens define the inter-group gap, group padding and each icon's
optical width. A full symbol line paints centered inside that optical width;
narrow Bluetooth and battery glyphs no longer reserve the same blank cell as Wi-Fi.
Counts use measured text advances and one shared line-box policy. The clock uses
an explicit optical baseline offset, and all groups have matching vertical padding.
These remain shell/theme choices, with no new engine primitive or recompilation.

Nacre `30892db7` extends both generated and shipped niri rules from `^nacre-background$`
to also match `bead-wallpaper` (Background) and `bead-wallpaper-video` (Bottom), with
`place-within-backdrop true`. This is placement policy in Nacre's config generator
and `niri/wallpaper.kdl`; Bead's reusable namespace/layer code stays intact.
The generated-config change does not modify an existing live configuration.
Niri only places Background surfaces in the backdrop: the static poster supplies
continuity while the video stays on Bottom. See the [niri layer-rule contract](https://niri-wm.github.io/niri/Configuration%3A-Layer-Rules.html).

## Validation

Pill widget integration: `50bb0059`; full CI/release capture: `5a31c419` (the rebase
adds only upstream documentation). B0113 landed during validation; the changes
were moved into shared widget context and status widgets, preserving its Host
composition. All four bar idle/hover goldens at 1× and
1.5× were regenerated remotely and inspected at 4×, with the owner's values
`9`, `56`, `15`, `11:02`. Headless fonts are bundled fixtures; native crops are
the optical reference. The port matches all four accepted pre-widget golden
PNGs byte-for-byte. Fresh native captures use the widget release and Nacre `30892db7`.
Full Bead CI on the integrated tree passed in a remote Arch container: fmt,
clippy `-D warnings`, workspace tests (nacre-shell: 597 passed, one ignored), trace tests, renderer
variants, Luau schema/analyzer, spike/packaging tests, wallpaper smoke, services,
private tray lifecycle and cargo-deny on all graphs. Arch needed explicit
`swaybg` and removal of Sway's container-incompatible file capability; no product
dependency was added. Nacre's full remote Arch desktop job passed on `30892db7`,
including 500 tools, 120 AI, 52 brain and 490 shell-tools tests; 534 helper parity
cases; and translation audits of 244/185 states per language. Both source entry
points and their 16 frozen preparation outputs are covered.
The private capture uses headless labwc → nested niri, 2880×1800 at scale 1.5,
private HOME/config/bus, the shared desktop lock and shipped containment/watchdog
gates. Capture timestamps are screencopy scheduling proxies, not scanout timing.
The fixture includes Nacre's transparent workspace background; omitting it initially
covered the QML wallpaper with niri's gray workspace fill, and that diagnostic run
was discarded. Same QML source and helper serve both corrected-rule arms.

| Placement proof, 60 frames per open transition | Peak dark row fraction, old rule | Corrected rule |
| --- | ---: | ---: |
| Bead static | 12% | 0% |
| Bead video (EGL/OpenGL, hardware) | 12% | 0% |
| QML static / video | 0% / 0% | 0% / 0% |

Dark rows are rows with mean RGB below 45 in the central 80% of the output,
downsampled to 160×100 for analysis. The old rule exposes a dark band between
moving workspaces; the corrected rule fills it with wallpaper. The reported
full-screen one-second blackout was not reproduced with niri's default animation.
Initial video placement proof drives the existing renderer directly; final proof
also exercises the normal shell supervisor and matching poster/palette activation.
The supervised video kept one PID across both switches; the shell created its
static layer once. All containment/watchdog runs completed, with zero OOM and
zero host swap-in; swap used was zero before every arm. No memory delta is claimed.

The final native ink gaps are **18, 17, 19, 18, 18, 18, 19 physical px**, compared
with the old 34 px Bluetooth→battery gap in this fixture. Battery and notification
digit ink centers are y=37.5 px; clock digits moved from y=36 to y=37 px, within
half a pixel of the calendar's y=37.5 center. Whole line boxes remain centered;
the initial empty clock retains a minimum width until its first model arrives.

Evidence: [paired idle](evidence/0120/widget-bar-idle-paired-4x.png),
[hover](evidence/0120/widget-bar-pill-hover-paired-4x.png),
[pressed](evidence/0120/widget-bar-pill-pressed-paired-4x.png),
[owner-value golden](evidence/0120/owner-values-headless-4x.png),
[old static](evidence/0120/before2-bead-switch-static.jpg) /
[fixed static](evidence/0120/widget-bead-switch-static.jpg),
[old video](evidence/0120/before2-bead-switch-video.jpg) /
[fixed video](evidence/0120/widget-bead-switch-video.jpg).
QML static/video montages, per-frame measurements, safety summary and source hashes
are alongside them. Full private frame series and CI output are retained with the unit report.

## Known evidence gaps

- The native notification fixture settles at QML=15 and Bead=0; the headless
  owner-value golden verifies Bead's two-digit `56`/`15` alignment explicitly.
- Video remains workspace-local on Bottom; the inter-workspace backdrop shows its
  matching static poster. QML's video is inside its global Background surface.
- Power hover/press uses the existing kit rounded rectangle; QML uses a circular
  highlight. This existing interaction styling remains visible in the paired crops.
- The native clock advanced between arms; its exact owner text is in the golden.
- The reported whole-screen one-second blackout was not reproduced; the measured
  dark inter-workspace band is removed. No new animation/latency claim is made.

## Temporary duplicates

None.

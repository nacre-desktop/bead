# B0098 — Sidebar and Settings from owner use

Status: **blocked at final capture and push**. Code/harness `411f9d9` passes the
full remote Arch CI-equivalent; product binary `33bbc50` is staged. The final
run waited 36 minutes for an existing interactive desktop lease and was cancelled
before any arm started. Its owner must release that lease before verification
can resume. Commits are unpublished pending the required final evidence.

The images and numbers below are **reference captures**, before the last pill
focus-outline and recreated-surface focus fixes. They do not validate the final
product revision. QML reference Nacre `3cc38598`: `LeftDrawer.qml`,
`ChatPane.qml`, `ActionButton.qml`, `FastScroll.qml`, `NacreSettingsWindow.qml`,
N0032 and published `ai.chat` contracts. Control is Bead `8e0c487` (B0096);
candidate binary is the product tree at `e830672`, with subsequent harness-only
reveal-wait and unpin proofs. No daemon contract or Nacre source change. Control includes matching
[private read-only probes](evidence/0098/control-probes.patch).

The sidebar used oversized rectangular tabs and percentage-width controls,
lacked pin state, and its external app registration omitted usable geometry.
Settings requested 960×640/minimum 560×400. Raw Wayland axes also moved content
without QML's conversion or easing. QML's actual FastScroll overrides the Qt
default: 600 logical px/notch, 3× pixel delta, 210 ms OutCubic; launcher lists/grid
use 300 px/notch.

## Changes

- Luau composes the branded header, pin, measured rounded controls, transcript
  bubbles and compact composer footer over existing `ai.chat`. Pin disables the
  catcher's input and changes keyboard focus to on-demand. Unpin restores input
  on the same surface; configure preserves it. Escape and explicit close work.
- External assistant lips reuse the app host and track a configured nonzero panel
  size. Overlay lips remain visible over Top panels. Reveal is 400 ms EaseInOut,
  matching LeftDrawer. [Opening frames](evidence/0098/sidebar-motion.jpg) retain
  capture timestamps; these are image proxies, not presentation feedback.
- Settings dimensions live in Luau: up to 1100×800, 75%/80% logical output,
  preferred floors 720×540, minimum 544×480. The measured initial size is
  1100×800. Shipped B0084 niri rules already float its app id;
  both private arms include those rules.
- Shared scroll conversion handles value120/discrete wheel units and pixel input,
  glides wheel input, and clamps layout overflow in lists and grids. Conversion
  applies only inside scroll viewports; volume/workspace/tray actions keep raw,
  single-event gestures. Launcher retains its 300 px setting.
- General primitives (ADR 0004): `kit.PillButton`, shaped `Meter::width` with a
  byte-limited cache, source outside-dismiss/wheel/viewport policy, external lip
  routing, and output-aware window sizing. Product layout stays in Luau.

## Reference evidence — final refresh blocked

Identical nested niri, 2880×1800 at 1.5, owner's wallpaper/palette, CPU tiny-skia,
same debug build flags without debuginfo. Synthetic provider reuses Nacre's
sidebar test fixture; no real provider/network chat. Every arm holds the desktop
lock and checks swap (35 MiB, below 64); no reset was needed. No live UI, laptop
build/test, installation, or desktop configuration change.

Full-frame RGB RMSE %, QML on the left and Bead on the right. Before uses the
same B0096 bar and shipped float rules, avoiding attribution of another unit's
bar or a tiled-window harness error to this change.

| State / pair | Before | After |
| --- | ---: | ---: |
| [sidebar](evidence/0098/sidebar-pair.jpg) | 4.144 | 3.462 |
| [sidebar-conversation](evidence/0098/sidebar-conversation-pair.jpg) | 4.899 | 4.165 |
| [sidebar-pinned](evidence/0098/sidebar-pinned-pair.jpg) | — | 4.736 |
| [sidebar-hover](evidence/0098/sidebar-hover-pair.jpg) | — | 4.72 |
| [sidebar-pressed](evidence/0098/sidebar-pressed-pair.jpg) | — | 4.712 |
| [settings](evidence/0098/settings-pair.jpg) | 19.61 | 7.366 |
| [settings-scroll](evidence/0098/settings-scroll-pair.jpg) | 19.673 | 7.649 |

Both shells pass pinned-after-outside and unpin-then-outside-close probes; native
uses actual pointer clicks after waiting for reveal. Hover/pressed images are
included. [Metrics](evidence/0098/metrics.json) contain all state errors, sizes,
axis inputs, app totals, anon/shmem/DRM/SwapPss and informational PSS/file PSS.
A 10-pixel touchpad event moves 10 px in control and 30 px in QML/candidate.
The virtual wheel includes a pixel payload: Qt chooses that path (45 px), while
native uses discrete/value120 (600 px requested, clipped at the page's 385 px
limit: 355 px after the initial 30). This is not a matched physical-notch proof;
unit tests verify 600 px/full notch and 150 px/quarter notch. Physical wheel and
release-momentum acceptance remain for an authorized reference run.

Always-on unreclaimable candidate-minus-control medians across four matched
states: open +0.260, closed +0.270 MiB. This is one control/candidate
sweep, not a five-pair laptop acceptance measurement. Per-state values and file
PSS are retained; app totals stay separate. Settings app unreclaimable changes
23.688→26.230 MiB; SHM 5.273→7.555 MiB reflects the larger
window. Every app exits after closed+3 s. CPU captures show zero DRM/SwapPss.
No absolute laptop-cap claim.

## Checks and limits

Full remote Arch CI-equivalent: every run step of `.github/workflows/ci.yml`,
including fmt, clippy -D warnings, workspace/trace tests, feature matrix, Luau
checksum/analyzer, capture/packaging suites, wallpaper/services/private SNI,
and cargo-deny for all independent framework graphs. Rust 1.99.1 (Arch patch
release; workflow pin 1.99.0). Chat/pearl goldens regenerated remotely and inspected
at 1/1.5; pin/release/detach, scroll bounds and raw action gestures tested.
The latest wheel-angle probe and balanced seven-opening memory sweep are ready
but have not run; neither has final visual validation of the focus fixes.

B0043 integration gaps remain: Brain, attachments/screenshots, saved/workspace
lists, richer markdown, transcript editing/selection and question-chip parity.
Unavailable controls are disabled; no unpublished calls. Touchpad release momentum,
hover colour transitions and external-app hover-exit timing differ from Qt.
Cold app startup enters later than resident QML (opening frames); matching the
configured 400 ms reveal does not establish cold-opening latency parity.
Settings body typography/content density and generic scrollbars remain earlier
parity gaps; only root sizing and shared axis behavior change here. Four English
ids need the lead's Nacre i18n mirror. Private diagnostic/helper/source copies are
removed from the remote hosts. Private local resume bundles expire seven days
after the final capture is trusted; no temporary production duplicate is added.

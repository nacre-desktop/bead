# B0098 — Sidebar and Settings from owner use

Status: review candidate; memory outlier and physical-wheel validation remain
for lead review. QML reference Nacre `3cc38598`: `LeftDrawer.qml`,
`ChatPane.qml`, `ActionButton.qml`, `FastScroll.qml`, `NacreSettingsWindow.qml`,
N0032 and published `ai.chat` contracts. Control is Bead `ac194d7` (B0096/B0102);
candidate binary is the rebased product tree at `200c65e` (includes B0103 integration), including reveal-wait, unpin proofs and the pill focus appearance. No daemon contract or Nacre source change. Control adds only the matching
[private read-only state probes](evidence/0098/control-probes.patch).

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

## Private evidence

Identical nested niri, 2880×1800 at 1.5, owner's wallpaper/palette, CPU tiny-skia,
same debug build flags without debuginfo. Synthetic provider reuses Nacre's
sidebar test fixture; no real provider/network chat. Every arm holds the desktop
lock and checks swap (0 MiB used, below 64); no reset was performed by this task.
The initial 72 MiB preflight stopped before any arm because sudo required a
password; the subsequent host state passed the unchanged swap gate. No live UI, laptop
build/test, installation, or desktop configuration change.

Full-frame RGB RMSE %, QML on the left and Bead on the right. Before uses the
same B0096 bar and shipped float rules, avoiding attribution of another unit's
bar or a tiled-window harness error to this change.

| State / pair | Before | After |
| --- | ---: | ---: |
| [sidebar](evidence/0098/sidebar-pair.jpg) | 4.16 | 3.474 |
| [sidebar-conversation](evidence/0098/sidebar-conversation-pair.jpg) | 4.906 | 4.175 |
| [sidebar-pinned](evidence/0098/sidebar-pinned-pair.jpg) | — | 4.736 |
| [sidebar-hover](evidence/0098/sidebar-hover-pair.jpg) | — | 4.729 |
| [sidebar-pressed](evidence/0098/sidebar-pressed-pair.jpg) | — | 4.721 |
| [settings](evidence/0098/settings-pair.jpg) | 19.613 | 7.369 |
| [settings-scroll](evidence/0098/settings-scroll-pair.jpg) | 19.612 | 7.37 |

Both shells pass pinned-after-outside and unpin-then-outside-close probes; native
uses actual pointer clicks after waiting for reveal. Hover/pressed images are
included. [Metrics](evidence/0098/metrics.json) contain all state errors, sizes,
axis inputs, app totals, anon/shmem/DRM/SwapPss and informational PSS/file PSS.
A 10-pixel touchpad event moves 10 px in control and 30 px in QML/candidate.
The wheel with a pixel payload moves Qt 45 px; native requests 600 and clips to
355 px after the initial 30 px. A pure angle notch from zero moves QML
0, control 0 and candidate 0 px; the candidate page's maximum is
385 px because its existing content density differs. All three angle-only probes
were zero: the zero-distance virtual event did not demonstrate Qt/native notch
parity. This remains a virtual-input limitation, not a measured zero wheel step.
Unit tests verify the
unclamped conversion: 600 px/full notch and 150 px/quarter notch. These are
virtual-input measurements, without a physical laptop wheel acceptance claim.
Touchpad release momentum remains a Qt parity gap.

Always-on unreclaimable candidate-minus-control medians across the matched
states in balanced seven-opening control/candidate arms: open +0.350, closed +0.383 MiB. This is one balanced control/candidate
sweep, not a five-pair laptop acceptance measurement. The closed pressed sample
is +1.705 MiB (anon +1.692); candidate anon then falls by 1.192 MiB at the
next closed state.
Its transient cause is unproven; this is an unresolved memory outlier for lead
review, despite phase medians below +0.5 MiB. B0103 is in the candidate but not
the control, so the remaining delta is not exclusively attributable to B0098.
Per-state values and file
PSS are retained; app totals stay separate. Settings app unreclaimable changes
23.617→26.242 MiB; SHM 5.273→7.555 MiB reflects the larger
window. Every app exits after closed+3 s. CPU captures show zero DRM/SwapPss.
No absolute laptop-cap claim.

## Checks and limits

Full remote Arch CI-equivalent: every run step of `.github/workflows/ci.yml`,
on captured `200c65e` (including B0103), including fmt, clippy -D warnings, workspace/trace tests, feature matrix, Luau
checksum/analyzer, capture/packaging suites, wallpaper/services/private SNI,
and cargo-deny for all independent framework graphs. Rust 1.99.1 (Arch patch
release; workflow pin 1.99.0). The final rebase adds only upstream private-pointer
logging and parent decoration syntax; `crates/` and `shell/` are unchanged from
the captured/CI-tested product. Chat/pearl goldens regenerated remotely and inspected
at 1/1.5; pin/release/detach, scroll bounds and raw action gestures tested.

B0043 integration gaps remain: Brain, attachments/screenshots, saved/workspace
lists, richer markdown, transcript editing/selection and question-chip parity.
Unavailable controls are disabled; no unpublished calls. Paired images retain
residual icon-glyph weight, text baselines, measured button widths and transcript
row-spacing differences. Brain/attachment disabled opacity also differs because
the corresponding integration is unavailable. Touchpad release momentum,
hover colour transitions and external-app hover-exit timing differ from Qt.
Cold app startup enters later than resident QML (see opening frames); matching
the configured 400 ms reveal is not a cold-opening latency parity claim.
Existing Settings gaps remain: font dropdowns/Fonts-in-launcher link, numeric
sliders represented by steppers, header page caption/menu, content density and
scrollbars. Only root sizing and shared axis behavior change here. Four English
ids are present in Bead's `shell/i18n/en.json` and `context.json`. Lead mirror:

- `bead.panel.chat-attach` — Attach
- `bead.panel.chat-pin` — Pin sidebar
- `bead.panel.chat-screenshot` — Screenshot
- `bead.panel.chat-tab-brain` — Brain

Private diagnostic/helper/source copies are deleted immediately after captures;
local resume bundles expire seven days after the final capture is trusted.
No temporary production duplicate is added.

# B0122 — Key repeat and bar/dashboard feedback

Holding a text-editing or navigation key must follow the compositor's repeat rate
and delay. Connected Wi-Fi must show the connected icon with the daemon's actual
`wifi` device shape, even when profile or access-point discovery is empty.
The status pill needs wider equal gaps, and the dashboard selection dot must move
immediately without rebuilding the persistent navigation strip.

## Changes

- Enable SCTK's existing keyboard repeat source on the shared Wayland event loop.
  It respects `wl_keyboard.repeat_info`, keymap repeatability, key release and
  focus loss. All text fields and navigation use the same existing key dispatcher;
  launcher, palette, picker, Settings, composer and prompt need no shell-specific
  timers. Preserve the press serial when routing keyboard events to child slots.
- Project Wi-Fi from connected devices with kind `wifi` (and the existing fixture
  alias `wireless`), retaining topic validity, freshness, radio and transport gates.
  Bluetooth already uses powered adapters; battery uses the published percentage
  and charging fields. Neither depends on a primary connection or signal strength.
- Set the shared Luau `status_gap` token to 12 logical pixels (previously 6),
  retaining optical glyph widths and tight icon/count groups, including battery.
  Preserve B0114's widget Host composition.
- Set both dashboard Luau selection durations to zero. The existing navigation
  transaction updates the dot immediately; page switching and retained strip IDs
  keep their existing lifecycle. No new engine primitive or dependency is needed.

## Validation

Remote focused checks passed: all four bar daemon projections, retained dashboard
navigation, and bar goldens regenerated at 1×/1.5× and visually inspected.
The full `.github/workflows/ci.yml` check job passed in a disposable Arch
checkout at code commit `fb6c385d` (`FULL_CI_OK`, exit 0; Rust 1.99.1).
This includes fmt, clippy with warnings denied, workspace tests (622 shell tests,
one ignored manual benchmark), trace memory tests, renderer variants, Luau SDK /
analyzer, spike and packaging suites, wallpaper smoke, private service/tray
lifecycles, and cargo-deny on the workspace and independent framework graphs.
The final amendment adds only documentation and capture evidence.
No dependencies were added; the CI environment needs `swaybg` and removal of
Sway's container-incompatible file capability, as in B0120.

Read-only `nacrectl watch network`, `watch bluetooth`, and `watch battery`
confirmed the published live field shapes. The public fixture removes private
network names. Wi-Fi health/radio/connected-device mapping is regression tested;
Bluetooth powered adapters and battery percentage/charging already match.

Private headless labwc → nested niri, 2880×1800 at scale 1.5, matched wallpaper,
palette and settings; CPU Bead development build and QML origin/main `a6da4350`.
Nine idle/hover/pressed/tab states passed in each arm. Captures wait three
seconds for cold page opening before clicking a tab. All four tab selections
were checked before screenshots; no captured dashboard state scheduled dot
motion. The retained-strip test checks immediate x=target and moving=false.

| Held input, compositor delay 300 ms / rate 20 Hz | Result |
| --- | --- |
| `wtype -P BackSpace -s 850 -p BackSpace` | 24 → 12 characters |
| Release, then wait 400 ms | No further deletion |
| `wtype -P Left -s 650 -p Left`, then insert X | `abcdef` → `Xabcdef` |
| Swap used before either arm / host swap-in | 0 / 0 |

Evidence: [pill idle](evidence/0122/idle-paired.png),
[hover](evidence/0122/pill-hover-paired.png),
[pressed](evidence/0122/pill-pressed-paired.png),
[Dashboard](evidence/0122/tab-0-paired.png), [Media](evidence/0122/tab-1-paired.png),
[Weather](evidence/0122/tab-2-paired.png), [System](evidence/0122/tab-3-paired.png),
[tab hover](evidence/0122/tab-hover-paired.png),
[tab pressed](evidence/0122/tab-pressed-paired.png),
[assertions and safety summary](evidence/0122/validation.json).

Existing visual differences remain explicit: QML's selected tab retains its
pointer hover fill; Bead's selected tab has no hover action. QML's pearl has a
highlight and sits about 13 logical pixels left of Bead's caption-centred dot;
media/system and connected Wi-Fi glyph variants differ. These are existing
styling differences; this unit changes selection timing and status spacing.
The native clock advanced between arms. Still captures and state assertions do
not establish physical scanout timing or a new animation performance result.
No live desktop, installed shell, configuration, bindings or units are changed.
No new caches, threads or feature memory budget; repeat uses the existing event
loop and toolkit timer only while a repeatable key is held. No memory delta is
claimed without a measurement.

## Temporary duplicates

None.

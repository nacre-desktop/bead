# B0119 — keyboard focus after closing a warm panel

The owner reports that application clicks reach the window but typing remains
unavailable under Bead; the same application works normally under QML.
The suspected mechanism is a retained panel requesting exclusive keyboard focus
after close, rather than pointer input interception.

## Cause and change

`transactions.rs` requested Exclusive/OnDemand for every accepted panel scene,
including background updates during the 120-second warm retention period. Close
had already cached keyboard visibility as false. The renderer therefore skipped
clearing the new protocol request. Background updates could repeatedly take the
seat from applications without a visible panel.

Drawing now owns keyboard policy and caches the actual protocol mode, including
pinned OnDemand transitions. Close immediately commits None and an empty input
region; closed newly configured companions also start without keyboard/input.
Background commits cannot restore focus or the outside dismissal catcher.
Input policy also applies to panels without an animation definition.

Title-only backend publications stop logging BACKEND_UPDATE. Complete window
models remain current, and unchanged unobserved values skip materialization.
`ui.text_width(ui.bind(name), family, size)` tracks measured geometry separately
from string contents, skipping the markup VM when the width stays equal. Fixed-box
Text patches its primitive and accessible label without layout. The text cache
flattens transparent Groups exactly as layout does, including widget-wrapped titles.
Width dependencies are bounded and byte-accounted; memo fragments conservatively
track the text value. Shell Luau still chooses fonts, sizes, geometry and actions.
Existing surface frame callbacks coalesce presentation while a callback is pending.

## Validation

The harness is `spikes/click-focus.py`, reusing the existing guarded private
parity session. It opens and closes the launcher, runs a raw-input terminal with
a 4 Hz OSC title spinner, clicks client interior/edges and checks actual input.
Niri uses focus-follows-mouse at 2880×1800 / 1.5. Terminal client bounds are verified
from a distinctive fixture background before click injection. Panels reopen every
20 trials to stay warm; a final key checks focus after 30 seconds of title churn.
All arms pass the existing capture guard and zero-swap-in gates; host swap was zero.

The original control delivered 0/10 keys; QML and the focus fix deliver 100/100.
The first normal-logging candidate used 1.033% core, above the 1% gate. Adding bound
width dependencies reduced this to 0.400% without reducing the 4 Hz title rate.
A generated wallpaper and identical presentation palette are verified in both
final arms; initial default-fallback screenshots were discarded as parity evidence.

| Private steady-state run | Keys delivered | Shell CPU, % core | Niri CPU, % core |
| --- | ---: | ---: | ---: |
| Original control, protocol tracing on | 0/10 | 1.233 | 0.500 |
| QML, matched inputs, normal logging | 100/100 | 1.033 | 0.500 |
| Bead, widgets + B0120 integrated, CPU renderer | 100/100 | 0.400 | 0.533 |

Final code `8c63827` and its binary hash are pinned in the numbers artifact.
Every trial focuses another window first, then clicks once and checks keyboard
focus plus the unique token in the target's raw input. Keyboard input still works
after the quiet interval. Final Bead logs zero BACKEND_UPDATE lines during it.
The real-bar regression checks actual Taffy pass counts, unchanged node geometry,
and that only the title primitive changes. Group and memo regressions also pass.
The full CI job passes in an isolated Arch container: fmt, denied-warning clippy,
workspace/trace tests, renderer configurations, actual Luau analysis/generated
types, Python/package suites, wallpaper smoke, private service/SNI lifecycles and
all independent license graphs. Tests use two threads to avoid unrelated wall-clock
sandbox deadline flakes under concurrent CI load. No deadlines were changed.
CI-only environment additions: `swaybg`; Arch Sway's file capability is removed
inside the disposable container so it can execute under Docker's capability set.

## Evidence and limits

[Numbers and source/binary identities](evidence/0119/results.json),
[control keyboard trace](evidence/0119/control-keyboard.txt),
[fixed keyboard trace](evidence/0119/fixed-keyboard.txt).
The control requests Exclusive 27.7 ms after closing and the keyboard re-enters
the same hidden surface. The fixed trace leaves it; subsequent application keys arrive.
QML's 371 shell files match Nacre `30892db7` exactly. Private sessions use synthetic
parent refresh, not a measurement of physical scanout timing.

[QML focused](evidence/0119/qml-focused.png),
[Bead control](evidence/0119/bead-control.png),
[Bead focused](evidence/0119/bead-focused.png).
Clock time and spinner phase differ between screenshots. B0120's alignment changes
are retained in the final rebase. The fixture update readouts differ (QML 134,
Bead 13); that data/pill parity is outside this keyboard/title unit.
Width-changing captions still rebuild geometry to keep the icon
and caption centred. No temporary legacy implementation or new desktop packages.
No live desktop, installed shell, configurations or keybindings changed.

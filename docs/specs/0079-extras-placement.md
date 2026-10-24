# B0079 — Extras placement, reveal and window glyphs

Status: **Review candidate — placement and field styling corrected.** Reference: Nacre `16436a34` origin/main,
`modules/extras/{SearchSurface,Overview,Palette,Keybindings}.qml` and
`modules/launcher/NacreLauncherPanel.qml`.

The QML defaults are bottom attachment, centered within the frame, 1020 logical
px width for Windows and 860 for Commands/Keyboard shortcuts. Overview body
height is `min(690, screen height - 230)`; other bodies are 450, with 112 px
chrome. At 2880×1800/1.5 this gives physical body rectangles
Windows `(675,582)–(2205,1785)` and other views `(795,942)–(2085,1785)`.
The resized review pictures do not redefine these source dimensions.

`shell/extras/tokens.luau` owns edge, dimensions and motion: 300 ms OutCubic,
fixed content revealed from the attached edge, no fade. The shared reveal
primitive and joined-frame contour follow that edge; changing it to top requires
no recompilation. The opt-in shell `SceneEdge` adapter follows that token while
existing fixed attachments keep their semantics. SearchSurface overrides the QML text field background with a
plain surfaceContainerHigh fill, radius 22: focused search therefore uses
`focus_ring=false`. The new general UI property defaults to true elsewhere.

Overview reuses the bar's bounded desktop-category glyph resolver (B0037/38),
with view-scoped cache and Material glyphs instead of a theme-image lookup that
could fall back to the embedded terminal sample. This follows the owner's
requested bar-icon behavior. Exact QML currently uses the generic `window`
glyph when no capture exists; that deliberate difference is recorded here.

No launcher files, Nacre sources, ranking, window move or capture engine change.
No temporary duplicate implementation. Existing B0077 content and interaction
gaps remain outside this placement unit.

## Validation

The six extras goldens were regenerated remotely at 1 and 1.5 and visually
reviewed (only the two Overview images changed). The focus-ring regression
covers default-on, suppression and incremental text edits. Placement coverage
keeps fixed-edge QML masks unchanged and compares token-driven top/left artwork
with the corresponding fixed-edge artwork. All 12 focused placement tests pass.
The complete `.github/workflows/ci.yml` job passed in a remote Arch container,
after rebasing onto the accepted B0075 renderer changes: formatting, clippy with
warnings denied, workspace tests (495 shell tests, one manual benchmark ignored),
trace memory-policy tests, no-default/all-feature checks, facade/headless checks,
135-module generated-SDK Luau analysis, Python/packaging suites, wallpaper smoke,
private service/SNI lifecycles, and cargo-deny including independent framework
graphs. The clean final replay emitted `FULL_CI_PASS` and exited 0.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo test --workspace --locked -j2
cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::
cargo check -p nacre-shell --no-default-features --locked -j2
cargo check --workspace --all-features --locked -j2
```

The full suite caught an overbroad first attachment implementation; the final
opt-in `SceneEdge` leaves fixed-edge QML masks unchanged. Missing private swaybg
and an obsolete CI target path were corrected before final validation.

Private nested QML/Bead captures use the same 2880×1800/1.5 output, wallpaper,
palette, shortcut fixture and daemon binary. Every idle/open/hover/pressed state
completed without drive errors; per-arm swap was 13 MiB (below 64 MiB), and
captured process SwapPss gates passed. No live UI or local build/test was run.
RMSE is percent of full-scale RGB, computed from raw PNGs.
Before values are the B0077 baseline; after values use identical physical panel
rectangles in both shells, rather than moving the Bead crop to the opposite edge.

| View | Whole-frame RMSE before → after | Panel RMSE before → after |
| --- | ---: | ---: |
| Windows | 25.606% → 7.803% | 3.652% → 3.153% |
| Commands | 29.112% → 4.369% | 8.832% → 8.481% |
| Keyboard shortcuts | 30.109% → 2.851% | 5.058% → 4.43% |

Pairs, full frames, hover/pressed crops and open/close strips are in
[evidence/0079](evidence/0079/). Machine-readable crop rectangles and motion
proxies are in [panel-metrics.json](evidence/0079/panel-metrics.json).
The private B0079 gallery retains all 40 sampled frames per transition, with a
nominal 25 ms capture interval. Settings is prepared before Overview’s timed open
and closed after its timed close. The old B0077 Overview strip recorded that
prerequisite window, so it is not copied as valid before-motion evidence.

Final captures use the helper binary pinned to the same Nacre `16436a34` as the
QML archive. The initial tool bundle had an older helper, which made the Bead
shortcut sheet unavailable despite the harness reporting no drive error. Visual
review caught this; both arms were repeated with the matched helper, and the
final sheet shows the same three fixture bindings. The historical B0077 sheet
baseline also used that unavailable state; its before/after content error is
therefore not attributable only to placement. No provider/backend code changed.

Both implementations specify 300 ms OutCubic, rising from the bottom and closing
downward without fading. The strips verify direction and fixed-content reveal;
screenshot image-progress proxies include command/admission delays and content
changes, and do not establish equal physical scanout timing. Open/close 90%
image proxies (QML/Bead, ms from sequence start) are Windows 502/497 and 430/455,
Commands 589/461 and 436/480, shortcuts 413/418 and 441/493. These remaining
sampled timing differences are recorded rather than called exact timing parity.

Remaining visual gaps: palette content/order awaits N0057; the owner-requested
category glyph differs from QML's generic window glyph; font rasterization,
caret phase, key-sheet row text/Refresh presentation and immediate wheel motion
retain B0077 differences. Overview's separate Settings window and the bar also
differ outside this unit, and remain in the whole-frame error. No new PSS/DRM memory-acceptance claim is made.
Container-only additions were the CI dependencies plus DejaVu/IBM Plex fonts,
swaybg, wlr-randr and wlr-protocols; no product dependency was added.

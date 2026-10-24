# B0077 — Overview, palette and keybindings

Status: **Review — native surfaces implemented; exact palette content parity awaits an engine contract.** QML reference: Nacre `16436a34` (`origin/main` when this unit started)
(`nacre/shell/modules/extras/{Overview,Palette,Keybindings}.qml`).

## Reference behavior and parity targets

All three use `SearchSurface`: 16 px outer margins, a 30 px heading with 17 pt
primary-colored title and Close action, then a 44 px search field with 15 px
horizontal padding, radius 22 and surfaceContainerHigh fill. The body begins at
y=122 and ends 16 px above the bottom. Default body height 450 gives total 562;
Overview overrides it as below. Search receives focus on creation. Down/Up move
selection (or scroll the sheet), Enter chooses, Escape dismisses; attached
activation closes the surface. The shared launcher host animates the entire
content with a 300 ms OutCubic slide from the bottom, without fading. Bead's
existing `ease` curve is that cubic; its top attachment follows the owner's
launcher-edge choice rather than the QML bottom attachment.

Wheel behavior is shared `FastScroll`: 600 logical pixels per wheel notch,
210 ms OutCubic smoothing, pixel deltas multiplied by 3, and a 65 ms release
interval before kinetic flicking (velocity bounded to 2600). Bead currently uses
the kit's immediate wheel scrolling; kinetic/smoothed wheel parity remains open.

### Overview

QML uses a `SearchSurface` titled “Windows”, width 1020, body height
`min(690, screen height - 230)`. It searches lowercase window title and class,
omits hidden windows, and filters to the focused output when the compositor says
workspaces are per-output (`Overview.qml:10-21`). On completion it refreshes
windows. Choosing a row focuses the window and closes the attached launcher;
moving a row updates selection, and changing the query resets selection
(`:22-35`).

The workspace grid has at most three columns, 12 px spacing, workspace cells up
to 320×215 px with radius 18. Cell backgrounds use `m3surfaceContainer`, and a
drop target uses `m3primaryContainer`. Each cell has a workspace icon/name and
clicking its header switches workspace (`:46-89`). Window tiles show titles,
selection through `m3secondaryContainer`, and a window icon when capture has no
content. A tile click focuses its window; hover selects it. Dragging a tile onto
a workspace moves that window there. The drag ghost is 140×60, radius 12,
opacity .9, with “Move window” text (`:91-220`).

QML requests captures only while the launcher is visible and live previews are
enabled, and asks for a frame every 350 ms when `windowCapture` is supported
(`:113-135`). The tile preview area is 160 px high; one window uses the whole
cell width, otherwise tiles use two columns, with up to two rows (`:91-100`).
The implemented icon fallback is the required baseline when capture is
unavailable. Keep preview resources scoped to the open view; do not retain
captures while closed.

Bead should consume daemon `compositor` window/workspace state and use the
published `window.focus` request. That request requires a stable `window` ID and
optionally accepts an `output` guard (`protocol/schema/window-focus.json:3-24`).
The QML move behavior has no published semantic move contract in this unit.
Request a stable window ID plus workspace and output guards before implementing
move; do not guess or embed niri commands. Close behavior is not present in this
QML view.

Live thumbnails need a general compositor-neutral per-window capture primitive
and a daemon transport for capture handles that carry generation and output
ownership. Investigate a Wayland route such as ext-image-copy-capture together
with foreign-toplevel association, but do not assume those protocols provide a
correct or portable pairing without research. Keep the capture cadence bounded
to the QML open-only 350 ms budget. Until that primitive and ownership contract
exist, show the window icon fallback.

### Command palette

QML uses a `SearchSurface` titled “Commands” with placeholder “Actions, apps,
windows, or brain: something” (`Palette.qml:6-9`). It strips a leading `>` and
trims for query matching. `brain:` creates a brain-query entry. Empty query shows
launcher-eligible `DesktopActions`; nonempty query yields local actions, then
matching compositor windows, then up to eight `NacreApps.fuzzyQuery` results
(`:10-42`). This ordering and local QML filtering are reference behavior only:
Bead must use the existing `menu.query` route and engine ranking, without adding
local ranking or changing the launcher typing path. Confirm that the menu engine
provides the empty-query action set and nonempty windows/apps behavior. Brain
query parity requires an engine route; do not reproduce it as a local provider.

Activation launches an app, focuses a window, executes a desktop action, or
dispatches the brain query; attached launcher activation closes the launcher.
Brain activation sets the query, executes `left`, then requests the brain view
(`:44-67`). The list uses 58 px rows, 6 px spacing, radius 14, 12 px margins and
spacing, with `m3surfaceContainer` rows and `m3secondaryContainer` selection
(`:68-119`). Hover selects and click activates. Its enclosing `NacreLauncherPanel` slides the whole fixed-height content from the bottom in 300 ms with OutCubic; it does not fade. Bead uses the same cubic/300 ms slide, from the owner-requested top edge.

### Keybindings

QML titles the view “Keyboard shortcuts” and searches key plus description
case-insensitively. On open it requests `DesktopExtras.request("keys", {})`;
selection movement scrolls by 47 px per step (`Keybindings.qml:6-18`). The list
uses 42 px rows, 5 px spacing and radius 10. Key text is JetBrains Mono NF at
11 pt, primary color, x=12 and width=340; description is 11 pt at x=365 and
elided. Empty state distinguishes loading from no matches (`:20-68`). No mouse
interaction or explicit animation appears in this module.

Use the published `shortcuts` topic and `shortcuts.refresh`, rather than a local
keybinding source. The topic reports current compositor bindings, with SUPER
bindings first; its shape is `{health, items}`, where `items` is a stable-ID
map plus explicit `order` (`protocol/schema/shortcuts-state.json:3-16,89-115`).
Shortcut entries require `key` and `description`, and may include nullable
`submap` and `menu` (`:117-151`). Existing Nacre routes include palette
`SUPER+CTRL+SPACE`, shortcuts `SUPER+K` and `SUPER+F1`, and AI sidebar
`SUPER+A` (`nacre/shell-tools/shortcuts.lua:2-5`).

## Bead implementation

`shell/extras/{main,behavior,tokens}.luau` contains all three views. The shared kit
Panel, Button, TextInput, Grid and Scroll primitives provide the surface and
controls. A reusable `shell/kit/virtual-list.luau` materializes only viewport rows
and two overscan rows, preserving the complete source order and extent. Its
chunked automatic extents handle the 2048-row contract without exceeding the
SDK's 20,000-pixel single-dimension limit. A 1000-row fixture retains all data,
paints fewer than 200 primitives, and scrolls into later rows. Tokens own geometry, palette roles, typography, grid columns and
preview slots; the Luau workspace-count/scroll defaults are replaceable settings
inputs. Stable daemon window/workspace IDs join rows, rather than title/index.
The overview filters output-local state, groups and sorts workspaces, searches
window titles/classes, focuses windows, and switches workspaces. The sheet lists
published shortcuts in daemon order, searches key/description, and refreshes on
open or the explicit refresh button. Palette queries and opaque activation use
the existing menu engine with its returned order; no ranking/provider copy.

`nacre mode overview|palette|keys` maps to independent `overview`, `palette`, and
`keybindings` panel factories. `nacre menu overview|palette|keys` deliberately
retains the published menu aliases; it is a different command. Launcher source
and UI files are untouched. The common Runtime adds extras input dispatch and
uses an embedded fallback icon when a theme icon cannot be resolved, so the
new view also works outside the repository working directory.

The new general framework primitive is `Session::action_values(name, argument)`:
evaluate a declared Luau action for host key bindings and stage its values through
the existing validated update transaction. It contains no Nacre key or route IDs.
Factories retain only their construction inputs. Runtime creation leases the
compositor/shortcuts topic when not already subscribed by the bar. The existing
permanent compositor topic is reused without a second subscription. Worker
retirement drops view subscriptions, owned leases, pending calls, results, the VM
and assets. Trace coverage checks an owned topic payload's weak reference disappears
after retirement; the bar's existing permanent payload correctly remains its own. Fake-daemon tests exercise the actual
Luau view's search/Enter path as well as RPC shapes and activation fencing.

Acceptance: each view matches the QML idle/open/hover/pressed states in paired
private nested sessions at identical output, scale, wallpaper and palette;
record motion direction/timing/easing where present. Add paired evidence under
`docs/specs/evidence/0077/`, and list any unverified or blocked parity items.
Add fake-daemon coverage for compositor state, window focus guards, shortcut
refresh/state and menu query behavior. Run the specified remote full CI and
feature checks; no local build, test, nested session or capture.

## Open contracts and temporary duplicates

- Nacre/Bead: general compositor-neutral capture API, daemon capture-handle
  transport with generation/output ownership, and proof of a portable
  window-to-capture association. Until settled, retain icon fallback.
- Nacre: semantic window-move request with stable window/workspace/output
  guards. Until published, no move action is implemented.
- Nacre menu engine: publish a palette query mode/route returning the existing
  `MenuQueryResult`/opaque `menu.activate` contract. Required semantics: empty
  query = launcher-eligible `DesktopActions` in their original order; nonempty
  query = matching actions, then windows, then up to eight fuzzy app results;
  leading `>` and literal `brain:` must match Palette.qml. Current Bead uses
  `{mode:"apps",route:"actions",query:""}` at rest and root queries when typing.
  These are the existing engine menu results, **not exact QML palette content**;
  parity for content/ordering/brain behavior remains blocked on this publication.
- No temporary duplicate implementation planned.

## Validation and evidence

The complete `.github/workflows/ci.yml` job was replayed in a remote Arch
container: fmt, clippy with warnings denied, workspace tests, trace memory-policy
tests, renderer feature configurations, facade example, actual Luau SDK/schema
analysis (131 modules), Python spike/packaging suites, motion-wallpaper smoke,
private service/SNI lifecycles and cargo-deny including independent framework
crate graphs. All passed. Commands include:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -j2 -- -D warnings
cargo test --workspace --locked -j2
cargo test -p nacre-shell --features trace --locked -j2 memory_policy::tests::
cargo test -p nacre-shell --no-default-features --locked -j2 extras::
```

The nine extras tests cover focus/workspace admission, shortcut refresh,
opaque activation and engine navigation, nullable menu fields, the existing
bar-owned compositor topic, real-view CPU image decoding, keyboard Close,
release activation, immediate reopen, long-list windowing, and six rendered
goldens at scales 1 and 1.5. Goldens were regenerated and visually reviewed.
`RUST_TEST_THREADS=1 RAYON_NUM_THREADS=2` avoids scheduling flakes in the existing
50 ms recovery/markup guards on the shared CI host; the guards were unchanged.
The native harness's Python checks were repeated after its fixture/environment
changes. No local build or test, live desktop run, install, or Nacre source
change was performed. Normal container additions included the workflow's build
libraries, DejaVu/IBM Plex fonts, swaybg, wlr-randr and wlr-protocols for the
private pointer tool.

Private nested niri comparison uses identical 2880×1800 outputs at scale 1.5,
wallpaper, palette and settings. The parent output is synthetic 60 Hz, not
physical scanout timing. Three harmless generated-format bindings are included
in each private compositor for a populated shortcut sheet; both QML's helper
and N0051 read that same file. HOME/XDG paths are private before D-Bus and the
parent compositor start. Swap prechecks and per-process SwapPss gates remain
unchanged. Open/close frame sequences and hover/pressed Close states are in the
private gallery. Pairs and machine-readable results are in
[evidence/0077](evidence/0077/).

| Open view | Aligned panel RMSE | Whole-frame RMSE |
| --- | ---: | ---: |
| Overview | pending | pending |
| Commands | pending | pending |
| Keyboard shortcuts | pending | pending |

RMSE is percent of full-scale RGB. Panel crops use the known body rectangles
recorded in `panel-metrics.json`; they align QML's bottom attachment with Bead's
owner-requested top attachment. Whole-frame values retain that placement
difference, the bar and the independently ported Settings window behind the
overview. These are measurements, not a claim of complete visual parity.
Lifecycle traces prove scoped ownership; no new relative PSS/DRM acceptance
measurement is claimed for this unit.

Known gaps: thumbnails and workspace drag/drop await the contracts above;
the existing engine's action-group content/order and `brain:` behavior differ
from Palette.qml. Custom workspace settings icons/names, ChatWindowTitle PID
captions and full window-caption normalization are not fed into this view yet.
The kit's focused-input outline and font rasterization differ from QML; wheel
scrolling is immediate rather than FastScroll's smoothing/inertia. Search
resets effective selection on query changes, but scroll resets on the next
navigation event. Loading uses a generic unavailable message before the first
snapshot/result. The sheet adds a Refresh control. UI strings and local search
case folding still need localization/Unicode parity. No duplicate catalog,
provider or temporary legacy implementation was added.

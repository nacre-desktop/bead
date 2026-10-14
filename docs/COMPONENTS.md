# Framework components

Luau views import `bead.kit`; components are embedded in the permissive
bead-ui framework rather than copied between shell views. `bead.ui` remains
available for custom primitives. The existing Orient palette supplies color
roles; no second palette engine is introduced.

`assets/size-scale.json` mirrors Nacre's `nacre/shell/widgets/size-scale.json`:
spacing, radii, text and icons share named values. Text/icon point sizes convert
to logical pixels at 96 DPI (4/3); physical pixels still follow output scale.
Motion color/state/text durations match NacreTokens at 200/120/400 ms.
Use `python3 tools/sync-size-scale.py SOURCE --check` to detect source drift;
without `--check` it updates only the framework scale mirror.

The kit exposes Button, IconButton, Slider, Switch, Tile, SegmentedControl,
List/ListRow, Card, Popover/Peek, Tooltip and TextInput. Slider uses a bounded
step range: click/drag chooses a step, arrow keys change the focused step.
Tile takes distinct `on_toggle` and `on_detail` actions. SegmentedControl takes
stable item IDs and an action receiving the selected ID. List/TextInput keep
existing stable store bindings; component identity survives reload.

Bar and Panel accept `edge`: top/bottom/left/right. Bar changes row/column and
reverses bottom/right order. Panel animation moves inward from its edge;
layer anchors update when worker candidates arrive. Popover/Peek/Tooltip use
the owning bar's edge and open away from it. Current GUI acceptance covers top
bar and left/right panels only; bottom bars/panels remain unvalidated.
Tooltip visibility is supplied by the view's hover binding. LevelMeter accepts
a clamped 0–1 value. BlurredArtwork accepts a tracked image and bounded blur
radius; its raster is retained and shared by CPU/native GL. This is artwork
blur, not a compositor backdrop-blur promise.

Example: `shell/kit/main.luau` composes the complete kit; its behavior file
contains actions/stores only. The native bar and launcher compose shared kit
components. No physical keyboard/assistive-technology acceptance is claimed.

# B0208 Widgets editor: edit mode, bar sections, desktop grid

Status: candidate on `task/claude-b0208-widgets-editor-20261030`; lead live gate pending.
Builds on B0198/B0198b (drag-only placement, copies) and B0203 (plugins v1).

## Owner feedback (2026-10-29 late)
1. Clicking a style card restyled the placed widget at once; wanted: an edit mode where
   clicking the widget on the bar edits it; dragging in a new clock was messy.
2. Desktop drags lag. 3. The preview did not show the clock. 4. App scrolling too slow.
5. No free placement; a drop in the middle landed on top of the title; the title should be
   a widget. 6. The gallery pill should be an empty container. 7. Remove "Advanced".
8. Install plugins from the launcher, not the Widgets app. 9. Follow established patterns.

## Research takeaways
- KDE Plasma: an explicit edit mode (desktop/panel context menu "Enter Edit Mode"); the
  "Add Widgets" sidebar is only a source to drag new widgets from; placed widgets are
  configured by clicking/right-clicking them; panels are ordered strips with spacers (Plasma 6
  moved spacers to the panel's own "Add new" controls); a blue insertion line shows the slot.
- macOS Sonoma: "Edit Widgets" opens a gallery drawer; drag from it onto the desktop; widgets
  snap to an invisible grid/alignment lines near edges and other widgets; remove with the
  corner (-) or the widget's context menu; existing widgets are edited in place.
- Windows 11 / iOS jiggle mode: one global edit state; every placed widget shows handles and a
  remove badge; tapping a widget opens ITS options; layout is grid based with collision push.
- Waybar, Noctalia, HyprPanel, AGS/Quickshell bars: three sections (left/centre/right), order
  within a section is the layout; the centre section is centred on the bar, independent of the
  side sections; no free x positioning (spacers where offered).
- Common rule: the gallery creates, the placed instance is edited; changes apply to that
  instance only; moves are cheap visual drags that commit once on drop.

## Design
- **Edit mode = the Widgets app is open** (from the launcher, `nacre widgets edit`, or the
  app's desktop entry). Placed widgets show the existing outline/remove badge. A click on a
  placed widget (bar or desktop) no longer opens its dropdown: it sends
  `widgets.edit {op="inspect", widget, iid, group}`; the host routes `widgets open` with that
  record and the app's open model selects that widget page on that copy
  (`widgets::open_model`). Pills keep their child clicks.
- **The widget page has two jobs.** "Add" holds the gallery: style cards are drag sources for a
  NEW copy; a click only chooses the style "Add to bar/desktop" uses (`widgets.new_style`,
  no layout change). "On your screen" lists the placed copies ("Bar · Right", "Desktop", ...);
  the selected copy's editor shows its own style cards (restyle THAT instance: `style` op with
  `iid`), colour, settings and Remove. The live stage outlines that copy.
- **Bar sections.** `registry.bar_regions` (shared by the bar and the app stage): left and right
  pack from their edges; the centre is a real centred section. A lone title keeps its QML band
  (absolute, identical pixels); any other centre content renders as a centred row in the
  28–72 % band with the title in its local (inline) form, so a drop in the middle sits beside
  the title, never on top. The title has been a regular widget since B0151; the owner's file
  keeps it as `title` in `centre`. Insertion keeps B0198's pointer-order rule and placeholder.
- **Desktop grid.** Cell = `kit.tokens.spacing.huge` (32 px), passed to the drop model as
  `grid_cell` in the host seed. A drop snaps to the nearest cell inside the output; if the
  rectangle overlaps another desktop widget it moves to the nearest free cell (rings up to 16
  cells). The drag ghost shows the grid around the landing cell (3 cells each way) and the
  landing outline.
- **Desktop drag cost.** Before: every motion ran the full placement transaction, cloned the
  document several times and published a new transient layout, so the canvas (all desktop
  widgets), the bar and the ghost re-rendered per motion. Now a desktop-destination motion
  keeps `widgets.drag.layout` equal to the frozen base (unchanged value, so no host re-renders;
  the source dims in place) and only computes the landing cell; the ghost overlay is the only
  surface that changes per motion. The layout is written once, on drop.
- **Gallery containers** (`category == "containers"`) render empty with an outline and an "add"
  glyph; a moved pill's ghost still shows its real children.
- **Advanced removed.** Settings fields may declare `shown_when={key=value}`; the launcher's
  custom path/route show only when its icon/action is "custom".
- **Scrolling.** The gallery and a widget page have separate scroll stores (`widgets.scroll`,
  `widgets.detail.scroll`), so a page no longer inherits the other's offset. A wheel notch moves
  the framework's shared 100 px step (same as the launcher); headless cost per step is measured
  in the test below. The live speed complaint still needs the lead's live check.
- **Preview clock.** The page's previews (gallery cards, the copy's own cards, the stage) all
  render the clock; covered by a test with the owner-shaped layout.
- **Plugins.** Installing moves to the launcher: typing `>plugin <git URL or folder>` replaces
  the results with the plugin mode (`shell/ui/components/plugins.luau`): Install, Check for
  updates, and every plugin with its declared access, Enable/Disable and Update. The host's
  `plugins.do` (`install|<source>`, `enable|id`, ...) and `plugins::launcher_values` (folders are
  scanned only while the query has the prefix) do the work; user plugins still arrive disabled
  and show their access before enabling, as in v1. The Plugins page stays for listing,
  enabling, settings and updates (settings and access belong next to the widgets a plugin adds);
  its source field and "Add" are gone, replaced by a pointer to the launcher.

## Removed
The style-click-restyles-everything path, the Advanced toggle/section and its store/action,
`form_state`/`config_for`, the stage's style/settings override in `render_bar`, the Plugins page source field/`add` op and `plugins.source`,
`default_preview_children`, the desktop magnet (`snap_distance`) and the ghost's magnet pass.

## Proof (`crates/nacre-shell/src/reload/widget_editor_tests.rs`)
- `owner_layout_loads_unchanged_with_the_title_as_a_widget` (fixture
  `widget_owner_layout_fixture.json`, shaped like the owner's file).
- `a_widget_dropped_in_the_middle_sits_beside_the_title_never_on_top`.
- `edit_mode_clicks_inspect_the_placed_instance_instead_of_its_dropdown`,
  `an_inspect_request_opens_the_app_on_that_copy`.
- `a_style_change_restyles_only_the_clicked_copy`.
- `desktop_drops_snap_to_the_grid_and_never_overlap`.
- `desktop_motion_is_a_cheap_transform_and_writes_only_on_drop` (120 motions: frozen layout,
  no write, on-grid landing; median cost below the bar transaction's).
- `the_gallery_pill_is_an_empty_container_and_there_is_no_advanced_section`.
- `the_clock_page_shows_the_clock_in_its_previews_and_edits_one_copy`.
- `widgets_app_scroll_moves_at_the_shared_wheel_distance`.
- `plugins::tests::launcher_plugin_mode_installs_a_local_folder_and_lists_its_access`,
  `launcher_plugin_mode_renders_install_and_the_declared_access`.

## Open
- Real-input captures on the desktop test host were not run in this unit (time box).
- Memory: no new always-on state (a small transient grid in the drag overlay only).

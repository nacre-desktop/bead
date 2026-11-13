# `bead.kit`

`bead.kit` is the embedded Luau module of reusable components. It exposes `kit.tokens`, `kit.metrics` (read-only review radii `panel` 24, `tile` 18, `row` 12, `chip` 18 and text sizes `caption` 12, `label` 13, `body` 14, `title` 16, in logical px), `kit.motion` (the `ui.motion.duration` tokens plus the kit's `color`, `state` and `text` durations) and component constructors; property details can evolve with the source. The list below names the current component surface, with its primary role and key inputs. All components use caller-provided IDs and content. Consult [COMPONENTS.md](../COMPONENTS.md) for kit/theme context; this page is the API name index and avoids duplicating that overview.

- `Button`, `IconButton`, `StatusIcon`, `Switch`: controls; `text`, `checked`, `on_activate`, dimensions, colors.
- `Fill`: gradient fill primitive; `from`, `to`, `width`, `height`.
- `SliderValue`, `Slider`: numeric range control; `value`, `min`, `max`, `step`, `on_change`.
- `Tile`, `ToggleTile`, `LinkTile`, `ButtonTile`, `SliderTile`: standard tiles; `label`, `detail`, `icon`, `on`, `value` (`SliderTile`), action props; `SliderTile` also takes an end slot, a 28 px round icon button: `end_icon`, `on_end`, `end_label`, `end_color`.
- `TileGrid`: rows of tiles; `tiles` (`{size, build}`), `sizes` (per-row count and height for each size), `width`, `gap`.
- `ReorderList`: drag-to-reorder rows; `items` (`{id, label, build}`), `width`, `item_height`, `gap`, `on_touch`, optional `remove`.
- `SegmentedControl`: exclusive choices; `items` (`{id, label}`), `value`, `on_change`.
- `Segment`: pill segment control (spec 0274); `options` (`{id, label, selected?, enabled?}`), `value` (marks the selected id when options do not), `enabled`, `aria_label`, `on_change` (action name, receives the option id).
- `LinkRow`: settings row that navigates; `title` (or `label`), optional `subtitle` (or `description`) and `icon`, `enabled`, `on_activate`; chevron at the end.
- `Chip`: 36 px pill; `text`, optional `icon`, `on` (primaryContainer fill), `on_activate`. `Tag`: 11 px mono tag with an outline border; `text`.
- `VirtualGrid`: virtualized keyed model; `model`, `key`, `render`, `scroll`.
- `Transcript`: scrolling message list; `cells` (windowed model), `extent`, `width`, `on_action`. The Rust `transcript::Entry` that produces the cells takes `align_end` and `max_width` (a fraction of the row; 0 = full) per message, and an empty `label` drops the caption band (spec 0274).
- `Composer`: multi-line message input; `text`, `placeholder`, `on_activate` (Enter; Shift+Enter breaks the line).
- `PasswordField`: masked/revealed text; `text`, `revealed`, `on_toggle`.
- `ActionButton`, `PillButton`, `ConfirmButton`: action variants; label/text and activation props.
- `List`, `ListRow`, `Card`, `TextInput`, `Pill`: scrolling, row, container, input, and compact row primitives.
- `Badge`: 15 px count pill anchored to its parent's top-right corner (spec 0264); `text`, `tone` (fill role; ink is its on-colour or `surface`), optional `top`/`right`/`fill`/`color`; hidden for `""` or `"0"`.
- `Bar`, `Panel`, `Popover`, `Tooltip`: edge-placed containers; `edge`, dimensions and children; `Popover` and `Tooltip` sit `bar_size` plus spacing from that edge. `Peek` is an alias of `Popover`.
- `CollectionSwitcher`, `CollectionSwitcherMenu`: collection selection; `groups` (`{id, label}`), `current`, `on_toggle` / `on_switch`, `on_rename`, `editing`, `actions`.
- `LevelMeter`, `HelpText`, `Swatch`: meter, helper copy, and color sample primitives.
- `FormRow`, `ToggleRow`, `ChoiceRow`, `NumberRow`, `TextRow`, `ListEditor`, `InfoRow`, `ChipsRow`, `DeviceRow`: settings/form rows; label, value, enabled, description and row-specific callbacks.
- `PickerList`: searchable item list; items, field, selection callback.
- `SecretRow`: password/join row; field, revealed state, join and cancel actions.
- `ApplyBar`: pending change prompt; text, apply/discard labels and callbacks.
- `Select`, `SelectRow`: choice fields; options, selected text, open state, filter and actions.
- `BlurredArtwork`: blurred image; `icon`, dimensions.
- `Paragraph`: wrapped text; `text`, `width`, `font_size`, optional `max_lines`.
- `ColorSample`, `ColorSampleRow`, `ColorTarget`: color selection pieces; color, caption/value, selected/armed state, callbacks.
- `TabStrip`: tabs; items, selected flags, `on_change`.
- `ChipRow`: compact choices; items, `per_row`, `on_change`.
- `ImagePick`: image hit grid; `icon`, `cols`, `rows`, `on_pick`.
- `PreviewCard`: miniature desktop preview; dimensions, labels, colors, optional image.
- `GraphCanvas`: force-directed graph view (spec 0232); `graph = ui.graph(nodes, edges)` (nodes `{id, label, weight}`, edges `{from, to}`; handed over once, best from a module), `style` (palette roles `node`, `edge`, `hover`, `selected`, `focus`, `label`; numbers `node_radius`, `label_size`, `label_zoom`, `max_labels`, `lod_zoom`, `far_edge_alpha`), `highlight` (node ids, or a bound list store of `{id=...}` records: those nodes and their edges at full strength, the rest dimmed; nil/empty dims nothing), `on_hover` (id or `""`), `on_select` (click; `""` when Escape clears), `on_activate` (double-click, or Enter on the focused node). The canvas is focusable: arrows walk the focused node to the nearest neighbour in that direction (camera glides only if it is off screen), Enter activates, Escape clears; the focused node's label is always shown. Layout, hit testing, pan, zoom, selection and labels run in Rust; data is reconciled by stable id and lives once in Rust after hand-over.

Item callbacks (`on_change`, `on_pick`, `on_touch`, `on_action`, `ListEditor`'s `on_remove`, and the `CollectionSwitcher` callbacks) take an action name, and the component supplies the argument, such as the chosen item's `id`. Other `on_*` props take a `ui.action(...)` descriptor.

Names and props are read from `crates/bead-kit/assets/kit.luau`; private helper functions inside that module are internal and are not part of this list. Refer to [Luau UI](luau-ui.md) for the underlying primitives.

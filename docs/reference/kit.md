# `bead.kit`

`bead.kit` is the embedded Luau module of reusable components. It exposes `kit.tokens` and component constructors; property details can evolve with the source. The list below names the current component surface, with its primary role and key inputs. All components use caller-provided IDs and content. Consult [COMPONENTS.md](../COMPONENTS.md) for kit/theme context; this page is the API name index and avoids duplicating that overview.

- `Button`, `IconButton`, `StatusIcon`, `Switch`: controls; `text`, `checked`, `on_activate`, dimensions, colors.
- `Fill`: gradient fill primitive; `from`, `to`, `width`, `height`.
- `SliderValue`, `Slider`: numeric range control; `value`, `min`, `max`, `step`, `on_change`.
- `Tile`, `ToggleTile`, `LinkTile`, `ButtonTile`, `SliderTile`: standard tiles; `label`, `detail`, `icon`, `on`, `value` (`SliderTile`), action props.
- `TileGrid`: rows of tiles; `tiles` (`{size, build}`), `sizes` (per-row count and height for each size), `width`, `gap`.
- `ReorderList`: drag-to-reorder rows; `items` (`{id, label, build}`), `width`, `item_height`, `gap`, `on_touch`, optional `remove`.
- `SegmentedControl`: exclusive choices; `items` (`{id, label}`), `value`, `on_change`.
- `VirtualGrid`: virtualized keyed model; `model`, `key`, `render`, `scroll`.
- `Transcript`: scrolling message list; `cells` (windowed model), `extent`, `width`, `on_action`.
- `Composer`: multi-line message input; `text`, `placeholder`, `on_activate` (Enter; Shift+Enter breaks the line).
- `PasswordField`: masked/revealed text; `text`, `revealed`, `on_toggle`.
- `ActionButton`, `PillButton`, `ConfirmButton`: action variants; label/text and activation props.
- `List`, `ListRow`, `Card`, `TextInput`, `Pill`: scrolling, row, container, input, and compact row primitives.
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
- `GraphCanvas`: force-directed graph view (spec 0232); `graph = ui.graph(nodes, edges)` (handed over once, best from a module), `style`, `on_hover`, `on_select`. Layout, hit testing, pan, zoom, selection and labels run in Rust; data is reconciled by stable id and lives once in Rust after hand-over.

Item callbacks (`on_change`, `on_pick`, `on_touch`, `on_action`, `ListEditor`'s `on_remove`, and the `CollectionSwitcher` callbacks) take an action name, and the component supplies the argument, such as the chosen item's `id`. Other `on_*` props take a `ui.action(...)` descriptor.

Names and props are read from `crates/bead-kit/assets/kit.luau`; private helper functions inside that module are internal and are not part of this list. Refer to [Luau UI](luau-ui.md) for the underlying primitives.

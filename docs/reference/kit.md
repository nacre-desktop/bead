# `bead.kit`

`bead.kit` is the embedded Luau module of reusable components. It exposes `kit.tokens` and component constructors; property details can evolve with the source. The list below names the current component surface, with its primary role and key inputs. All components use caller-provided IDs and content. Consult [COMPONENTS.md](../COMPONENTS.md) for kit/theme context; this page is the API name index and avoids duplicating that overview.

- `Button`, `IconButton`, `StatusIcon`, `Switch`: controls; `text`, `checked`, `on_activate`, dimensions, colors.
- `Fill`: gradient fill primitive; `from`, `to`, `width`, `height`.
- `SliderValue`, `Slider`: numeric range control; `value`, `minimum`, `maximum`, `step`, `on_change`.
- `Tile`, `ToggleTile`, `LinkTile`, `ButtonTile`, `SliderTile`: standard tiles; `label`, `detail`, `icon`, `value`, action props.
- `TileGrid`: grid of rendered items; `items`, dimensions, columns.
- `ReorderList`: reorderable entries; `items`, `on_move`, row renderer.
- `SegmentedControl`: exclusive choices; `items`, `selected`, `on_change`.
- `VirtualGrid`: virtualized keyed model; `model`, `key`, `render`, `scroll`.
- `Transcript`: bounded transcript display; entries/model and scroll state.
- `Composer`: message input/action; `text`, `on_send`, placeholder.
- `PasswordField`: masked/revealed text; `text`, `revealed`, `on_toggle`.
- `ActionButton`, `PillButton`, `ConfirmButton`: action variants; label/text and activation props.
- `List`, `ListRow`, `Card`, `TextInput`, `Pill`: scrolling, row, container, input, and compact row primitives.
- `Bar`, `Panel`, `Popover`, `Tooltip`: positioned surfaces; dimensions, edge/content, open/placement props.
- `CollectionSwitcher`, `CollectionSwitcherMenu`: collection selection; items, selected ID, actions.
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

Names and props are read from `crates/bead-kit/assets/kit.luau`; private helper functions inside that module are internal and are not part of this list. Refer to [Luau UI](luau-ui.md) for the underlying primitives.

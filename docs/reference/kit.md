# `bead.kit`

`bead.kit` is the embedded Luau module of reusable components. It exposes `kit.tokens`, `kit.metrics` (read-only review sizes in logical px: `kit.metrics.radius.panel` 24, `.tile` 18, `.row` 12, `.chip` 18 and `kit.metrics.text.caption` 12, `.label` 13, `.body` 14, `.title` 16), `kit.motion` (the `ui.motion.duration` tokens plus the kit's `color`, `state` and `text` durations) and component constructors; property details can evolve with the source. The list below names the current component surface, with its primary role and key inputs. All components use caller-provided IDs and content. Consult [COMPONENTS.md](../COMPONENTS.md) for kit/theme context; this page is the API name index and avoids duplicating that overview.

- `Button`, `IconButton`, `StatusIcon`, `Switch`: controls; `text`, `checked`, `on_activate`, dimensions, colors. Every kit `Button` presses to `press_scale=.97` (pass `press_scale=1` to opt out); chips, segment options, pill buttons and tiles also set a `pressed_fill` (spec 0280). Tiles scale as a whole.
- `Fill`: gradient fill primitive; `from`, `to`, `width`, `height`.
- `SliderValue`, `Slider`: numeric range control; `value`, `min`, `max`, `step`, `on_change`.
- `Tile`, `ToggleTile`, `LinkTile`, `ButtonTile`, `SliderTile`: standard tiles; `label`, `detail`, `icon`, `on`, `value` (`SliderTile`), action props; `SliderTile` also takes an end slot, a 28 px round icon button: `end_icon`, `on_end`, `end_label`, `end_color`.
- `TileGrid`: rows of tiles; `tiles` (`{size, build}`), `sizes` (per-row count and height for each size), `width`, `gap`.
- `ReorderList`: drag-to-reorder rows; `items` (`{id, label, build}`), `width`, `item_height`, `gap`, `on_touch`, optional `remove`.
- `SegmentedControl`: exclusive choices; `items` (`{id, label}`), `value`, `on_change`.
- `Segment`: pill segment control (spec 0274); `options` (`{id, label, selected?, enabled?}`), `value` (marks the selected id when options do not), `enabled`, `aria_label`, `on_change` (action name, receives the option id).
- `LinkRow`: settings row that navigates; `title` (or `label`), optional `subtitle` (or `description`) and `icon`, `enabled`, `on_activate`; chevron at the end.
- `Chip`: 36 px pill; `text`, optional `icon`, `on` (primaryContainer fill), `on_activate`; presses to .97 with the hover tint one step deeper. `Tag`: 11 px mono tag with an outline border; `text`.
- `EmptyState`: centred hero outline glyph, `title`, optional `detail` and at most one `chip` pill; `glyph`, `title`, `detail`, `chip`, `chip_glyph`, `on_chip`, `width`, `height`, optional `left`/`top` (absolute placement) (spec 0276).
- `NoticeRow`: 72 px notification row; `id`, `icon` (host icon name; a bell glyph when empty), `caption`, `title`, `body`, `on_dismiss` (nil disables the button), `dismiss_label`, `width`, `fill` (spec 0276); the dismiss button shows only while the row is hovered or keyboard-focused (spec 0280).
- `VirtualGrid`: virtualized keyed model; `model`, `key`, `render`, `scroll`.
- `Transcript`: scrolling message list; `cells` (windowed model), `extent`, `width`, `on_action`. The Rust `transcript::Entry` that produces the cells takes `align_end` and `max_width` (a fraction of the row; 0 = full) per message, and an empty `label` drops the caption band (spec 0274).
- `Composer`: multi-line message input; `text`, `placeholder`, `on_activate` (Enter; Shift+Enter breaks the line).
- `PasswordField`: masked/revealed text; `text`, `revealed`, `on_toggle`.
- `ActionButton`, `PillButton`, `ConfirmButton`: action variants; label/text and activation props.
- `List`, `ListRow`, `Card`, `TextInput`, `Pill`: scrolling, row, container, input, and compact row primitives; `Card { shadow = true }` draws the panel shadow (spec 0280).
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
- `GraphCanvas`: force-directed graph view (spec 0232); `graph = ui.graph(nodes, edges)` (nodes `{id, label, weight, group}`, edges `{from, to}`; handed over once, best from a module), `style` (palette roles `node`, `edge`, `hover`, `selected`, `focus`, `label`; numbers `node_radius`, `label_size`, `label_zoom`, `max_labels`, `lod_zoom`, `far_edge_alpha`), `style.groups` (a list of palette roles; a node with integer `group = N` takes entry (N-1) % #groups + 1, so the list wraps; 0 or no group keeps `node`; hover and selection rings still show; at most 16), `fit` (a number: the camera frames every node, its radius and the label margin on first data, and again each time this number changes, so pan and zoom survive data updates; bind it to a store counter), `highlight` (node ids, or a bound list store of `{id=...}` records: those nodes and their edges at full strength, the rest dimmed; nil/empty dims nothing), `on_hover` (id or `""`), `on_select` (click; `""` when Escape clears), `on_activate` (double-click, or Enter on the focused node). The canvas is focusable: arrows walk the focused node to the nearest neighbour in that direction (camera glides only if it is off screen), Enter activates, Escape clears; the focused node's label is always shown. Layout, hit testing, pan, zoom, selection and labels run in Rust; data is reconciled by stable id and lives once in Rust after hand-over.

Item callbacks (`on_change`, `on_pick`, `on_touch`, `on_action`, `ListEditor`'s `on_remove`, and the `CollectionSwitcher` callbacks) take an action name, and the component supplies the argument, such as the chosen item's `id`. Other `on_*` props take a `ui.action(...)` descriptor.

The components added since the design review (specs 0264, 0274, 0276), one each; the stores and actions they name are declared in `behavior.luau`, as in [`examples/kit-gallery`](../../examples/kit-gallery/), whose `palette.json` also defines the roles these components paint with (`surfaceContainer`, ...; the minimal `hello-bar` palette does not):

```luau
local ui = require("bead.ui")
local kit = require("bead.kit")
return ui.Column { id = "kit", width = "fill", height = "fill", padding = 16, gap = 8, fill = "surface",
    ui.Box { id = "inbox", width = 32, height = 32, radius = 8, fill = "surfaceContainer",
        kit.Badge { id = "count", text = "3", tone = "error" } },
    kit.Segment { id = "mode", aria_label = "Mode", value = ui.bind("mode"), on_change = "set_mode",
        options = { { id = "light", label = "Light" }, { id = "dark", label = "Dark" } } },
    kit.Chip { id = "wifi", text = "Wi-Fi", icon = "wifi", on = true, on_activate = ui.action("toggle_wifi") },
    kit.Tag { id = "tag", text = "beta" },
    kit.LinkRow { id = "network", title = "Network", subtitle = "Wi-Fi and VPN", icon = "wifi", on_activate = ui.action("open_network") },
    kit.SliderTile { id = "volume", width = 380, height = 56, label = "Volume", icon = "volume_up", value = ui.bind("volume"),
        on_change = "set_volume", end_icon = "mic", end_label = "Mute microphone", on_end = ui.action("toggle_mic") },
    kit.EmptyState { id = "empty", width = 200, height = 120, glyph = "inbox", title = "All clear", detail = "No notifications" },
    kit.NoticeRow { id = "mail", width = 380, caption = "Mail", title = "New message", body = "Lunch at noon?", on_dismiss = ui.action("dismiss") },
    kit.GraphCanvas { id = "graph", width = 380, height = 120, on_select = ui.action("select"),
        graph = ui.graph({ { id = "a", label = "A", weight = 1 }, { id = "b", label = "B", weight = 1 } }, { { from = "a", to = "b" } }) },
}
```

Names and props are read from `crates/bead-kit/assets/kit.luau`; private helper functions inside that module are internal and are not part of this list. Refer to [Luau UI](luau-ui.md) for the underlying primitives.

# Luau UI

Folder views import `bead.ui`. Each element is a constructor taking a property table and child elements. Use stable `id` values for interactive elements and explicit sizes where the layout needs them. `width`/`height` accept numeric sizes and supported layout strings such as `"fill"`; `Text.width = "content"` opts into shaped text width plus horizontal padding. Its height behavior is unchanged, and omitted Text width keeps the existing fill default. `grow`, `shrink`, `gap`, `padding`, `align`, and `justify` control layout. Exact accepted values are validated by Bead and unsupported properties are errors.

## Elements

The following are the folder-view primitives in `bead.ui`; examples show the constructor shape. Common useful properties: `id`, `width`, `height`, `padding`, `gap`, `fill`, `color`, and child elements.

```luau
ui.Panel { id="panel", width=400, height=240, child }
-- Root surface container; common props include width, height, fill, padding.
```
```luau
ui.Row { id="row", width="fill", height=40, align="center", child }
-- Horizontal layout; align and justify control child placement.
```
```luau
ui.Column { id="column", width="fill", gap=8, child }
-- Vertical layout; gap and padding control spacing.
```
```luau
ui.Grid { id="grid", width=300, columns=3, child }
-- Grid layout; columns and cell sizing control placement.
```
```luau
ui.Scroll { id="scroll", width=300, height=200, scroll=ui.bind("offset"), child }
-- Scroll container; scroll must bind a mutable numeric store.
```

Overflowing `Scroll` regions get a themed overlay scrollbar automatically. Use `scrollbar=false` to opt out (also accepted by the kit virtual list). Hosts can override the optional `tokens.scrollbar` geometry, palette roles, alpha, and idle/fade durations; see [the scrolling contract](../specs/0135-scrolling.md). PgUp/PgDn, Home/End, and Up/Down navigate the focused viewport or the viewport under the pointer; TextInput keeps its editing keys.
```luau
ui.Box { id="box", width=80, height=40, fill="surface", child }
-- General container; supports layout, fill, and border properties.
```
```luau
ui.Text { id="label", text="Hello", width=100, height=24 }
-- Label; text, font, alignment, color, and dimensions are useful props.
```
```luau
ui.Button { id="save", text="Save", on_activate=ui.action("save") }
-- Clickable control; on_activate must be an ui.action descriptor.
```
```luau
ui.TextInput { id="name", text=ui.bind("name"), placeholder="Name" }
-- Editable input; text must bind to a mutable store.
```
```luau
ui.Image { id="image", icon="wallpaper", width=120, height=80 }
-- Image slot; the host supplies the asset represented by icon (folder apps look it up in the
-- hicolor icon directories; `path` names a bundle file instead).
-- tint="secondary" (a palette role, `ui.alpha(...)` or `ui.tint(...)`) draws the image's alpha as a
-- mask filled with that colour, so themed icons and symbol glyphs follow the palette.
```
```luau
ui.Arc { id="arc", width=40, height=40 }
-- Arc drawing primitive; detailed props are unstable.
```
```luau
ui.Polyline { id="line", width=100, height=30, points={{0,0},{1,1}} }
-- Polyline drawing primitive; points are at most 256 pairs normalized to 0-1.
```
```luau
ui.Group { id="group", child }
-- Groups children; layout behavior depends on group properties.
```

`Arc` and `Polyline` are drawing primitives; their property contract is unstable. `Panel` is the root panel, `Row` lays out horizontally, and `Column` vertically. `Box` is a container. `Button` exposes `on_activate`; `TextInput.text` must be a mutable store binding. `Scroll.scroll` must bind a mutable numeric store. See source validation for additional fields on each element; this short reference is not an exhaustive property schema.

## Bindings and behavior

`ui.bind(name)` creates a live binding to a declared store or computed value. `ui.alpha(role, amount)` and `ui.mix(from, to, amount)` are colour expressions whose amount may be a number, a binding or an animation marker. `ui.action(name, argument)` creates an action descriptor for `on_activate`. Hosts may declare typed defaults for values they publish; folder apps get stores for their declared services and commands automatically. Other hosts must declare published values with `Session::declare_host_stores` or the view behavior schema.

```luau
-- behavior.luau
return { stores={ count=0 }, actions={ increment=function(s) s.count += 1 end },
    computed={ label=function(s) return "Count " .. tostring(s.count) end } }
-- main.luau
return ui.Button { id="count", text=ui.bind("label"), on_activate=ui.action("increment") }
```

For a live display, derive text in `computed` and bind it. `ui.resolve_for_check` resolves values during checks only; it does not create a runtime subscription. A view that reads an undeclared host key will not receive that value. Use a store for editable input and bind it directly to `TextInput.text`.

Render a list store with `ui.Repeat { id = "rows", model = ui.bind("NAME"), key = "id", render = function(item) return ui.Text { text = item.name } end }`: one element per record, keyed by that field (see [hello-status](../../examples/hello-status/)). Other helpers: `ui.alpha(role, alpha)` and `ui.tint(base, over, alpha)` build colour values, `ui.text_width(text, family, size)` measures text (`ui.default_font_family` is the default family), and `ui.component(name, schema, render)` wraps a render function with property type checks.

## Motion

`ui.motion` is a frozen table of tokens: `ui.motion.duration.<name>` (seconds: `hover` .09, `press` .06, `release` .12, `trace` .12, `badge` .14, `close` .16, `fade` .16, `osd_in` .16, `switch` .18, `peek` .2, `osd_out` .2, `open` .22, `toast` .22, `dock` .22, `slide` .24, `drop` .26, `pulse` 1.6) and `ui.motion.curve.<name>` (the curve strings `linear`, `out-cubic`, `in-cubic`, `out-quint`, `in-out-cubic`, `out-back`, `in-quart`, `spring`; `ease`, `ease-out`, `ease-in` and `ease-in-out` remain accepted). The same names work in `ui.animate`, `ui.animated` and `Panel.transition`; see [spec 0263](../specs/0263-motion-tokens-and-retarget.md).

```luau
local m = ui.motion
local width = ui.animate("pill.w", target, m.duration.switch, m.curve.out_cubic)       -- a number, re-evaluated per frame
ui.Box { id = "badge", width = 16, height = 16, radius = 8,
    fill = ui.mix("surfaceContainer", "primary", ui.animated("badge.on", on and 1 or 0, m.duration.badge, "out-back", 0)),
    color = ui.alpha("error", ui.pulse("badge.pulse", m.duration.pulse, .55, 1)) }
```

`ui.animated(key, target, duration, curve[, from])` and `ui.pulse(key, period, lo, hi)` return markers instead of numbers: the view is built once and the paint stage samples the value every frame, so animation frames run no Luau. A marker on `fill`/`color`/`border_color` (through `ui.alpha` or `ui.mix`), `radius`, `join` or `translate_x`/`translate_y` is patched into the cached scene; on any other property (width, height, left, ...) the layout is recomputed without Luau. `ui.animated` retargets from its current value and starts a fresh key at `from` (default: the target); `ui.pulse` loops `lo -> hi -> lo` every `period` until the key leaves the view and holds at `hi` under reduced motion.

## Panel body and joined edges

A root `Panel` may name a child as its `body`: the reveal (`reveal=true`) grows that child out of the panel's edge, its box and shadow keep their corners while growing, and pointer input stops at it, so a transparent root can stay at the surface's maximum size while the body is resized, moved or reshaped with `ui.animated` markers (a hover peek that grows into a panel as one surface). `shadow=true` is accepted on `Box` for such a body. `join_edge="top"|"bottom"|"left"|"right"` with `join` (0..1, default 1, animatable) makes a box's two corners on that edge square and flush with the surface it hangs from; `join=0` rounds all four corners (a detached panel).

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

`overscan` (0..3 viewports, default 1) is how far past its viewport a `Scroll` rasters its content while the host names it in `bead.scrolling`; `0` keeps the viewport only. See [scroll planes](../specs/0286-scroll-planes.md).

`ready={from=,to=}` (content px, `from <= to`, `Scroll` only) is the span a kit has materialized; the plane's `ready` is that span within its interest, and the default is the whole content. `kit-virtual-list` and `kit.VirtualGrid` set it. See the same spec.

`axis="x"` (`Scroll` only, default `"y"`) scrolls along x: the store is the x offset, only horizontal wheel and touchpad deltas move it, and it draws no scrollbar. Lay its content out as a `Row`. See the same spec.

`on_settle=ui.action(name)` (`Scroll` only) runs once when a wheel or touchpad burst that moved the `Scroll` comes to rest, with the final offset as its argument. See the same spec.

`snap=<pitch px>` (`Scroll` only, `>= 0`) makes a wheel detent move one pitch and finger releases come to rest on a multiple of it. A vertical wheel that no vertical `Scroll` under the pointer can take moves an `axis="x"` `Scroll` with `snap`. See the same spec.

Any other node may take `scroll=ui.action(name)`: the action receives the scroll delta as its number argument. Add `scroll_steps={min_interval_ms=0, threshold=40}` (both optional; 0..2000 ms and 1..1000 px) to receive whole `+1` (down or right) and `-1` steps instead of smoothed pixels: a wheel detent is one step, finger travel steps each `threshold` px, and at most one step per `min_interval_ms` is delivered (extra steps are dropped). `scroll_steps` with a store binding, or without a scroll action, is a markup error.
```luau
ui.Box { id="box", width=80, height=40, fill="surface", child }
-- General container; supports layout, fill, and border properties.
```
```luau
ui.Text { id="label", text="Hello", width=100, height=24 }
-- Label; text, font, alignment, color, and dimensions are useful props.
-- font_weight is an integer 1..1000 (a continuous wght on variable fonts); font_features="tnum" sets OpenType features (see spec 0280).
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
ui.Image { id="image", path="logo.png", width=120, height=80 }
-- Image file inside the app folder. icon="NAME" names a host slot instead: the folder host uses
-- NAME.svg, else NAME.png, from icons/hicolor or pixmaps under $XDG_DATA_HOME and $XDG_DATA_DIRS.
-- tint="secondary" (a palette role, `ui.alpha(...)` or `ui.tint(...)`) draws the image's alpha as a
-- mask filled with that colour, so themed icons and symbol glyphs follow the palette.
-- radius=16 clips the image to an antialiased rounded rectangle (album art inside a rounded card).
-- opacity=0.5 (0..1, default 1) multiplies the image's alpha when it is drawn; tint replaces the colour instead.
-- The decoded raster is shared across opacities, and a hover-only child (`visible_on_hover`) fades it with the rest.
-- bake_size={w=96,h=96} (logical px, 1..4096 each) bakes the raster once at that size and scales it to the
-- rect when drawn on GL, so an image whose rect resizes every frame is not re-baked (bilinear; set it to the
-- largest size the image reaches). radius and clip_polygon are baked in bake_size space and scale with the draw.
-- The CPU path ignores it and rasters exactly per rect. Absent = bake at the rect size.
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

`Arc` and `Polyline` are drawing primitives; their property contract is unstable. `Arc` `start_angle` and `sweep_angle` accept a numeric `ui.bind(...)`; a store change repaints the arc with no Luau run (values outside -360..360 fall back to a full layout, which reports the usual error). `Panel` is the root panel, `Row` lays out horizontally, and `Column` vertically. `Box` is a container. `Button` exposes `on_activate`; `TextInput.text` must be a mutable store binding. `Scroll.scroll` must bind a mutable numeric store. See source validation for additional fields on each element; this short reference is not an exhaustive property schema.

## Bindings and behavior

`ui.bind(name)` creates a live binding to a declared store or computed value. `ui.alpha(role, amount)` and `ui.mix(from, to, amount)` are colour expressions whose amount may be a number, a binding or an animation marker. `ui.action(name, argument)` creates an action descriptor for `on_activate`. Hosts may declare typed defaults for values they publish; folder apps get stores for their declared services and commands automatically. Other hosts must declare published values with `Session::declare_host_stores` or the view behavior schema.

```luau
-- behavior.luau
return { stores={ count=0 }, actions={ increment=function(s) s.count += 1 end },
    computed={ label=function(s) return "Count " .. tostring(s.count) end } }
-- main.luau
local ui = require("bead.ui")
return ui.Button { id="count", text=ui.bind("label"), on_activate=ui.action("increment") }
```

For a live display, derive text in `computed` and bind it. `ui.resolve_for_check` resolves values during checks only; it does not create a runtime subscription. A view that reads an undeclared host key will not receive that value. Use a store for editable input and bind it directly to `TextInput.text`.

Render a list store with `ui.Repeat { id = "rows", model = ui.bind("NAME"), key = "id", render = function(item) return ui.Text { text = item.name } end }`: one element per record, keyed by that field (see [hello-status](../../examples/hello-status/)). Other helpers: `ui.alpha(role, alpha)` and `ui.tint(base, over, alpha)` build colour values, `ui.text_width(text, family, size[, weight[, features]])` measures text exactly as it paints (pass the node's `font_weight` and `font_features`; `ui.default_font_family` is the default family), `ui.component(name, schema, render)` wraps a render function with property type checks, and `ui.graph(nodes, edges)` hands graph data to Rust once for [`kit.GraphCanvas`](kit.md).

## Motion

`ui.motion` is a frozen table of tokens: `ui.motion.duration.<name>` (seconds: `hover` .09, `press` .06, `release` .12, `trace` .12, `badge` .14, `close` .16, `fade` .16, `osd_in` .16, `switch` .18, `peek` .2, `osd_out` .2, `open` .22, `toast` .22, `dock` .22, `slide` .24, `drop` .26, `pulse` 1.6) `ui.motion.panel` (the panel profile, from the same Rust source: `speed` 1600 px/s, `open_min` .18, `open_max` .32, `close` .18, `leave_ms` 150) and `ui.motion.curve.<name>` (keyed with `_` for `-`, so `m.curve.out_cubic` is `"out-cubic"`; the curve strings `linear`, `out-cubic`, `in-cubic`, `out-quint`, `in-out-cubic`, `out-back`, `in-quart`, `spring`; `ease`, `ease-out`, `ease-in` and `ease-in-out` remain accepted). `ui.animate` and `ui.animated` take these names. `Panel.transition` currently honours only `spring` and `ease-in-out` (any other value eases out) and `return_transition` takes `ease`, `ease-in-out` or `spring`; see [spec 0263](../specs/0263-motion-tokens-and-retarget.md).

The snippet below uses the `surfaceContainer` role, which the minimal `hello-bar` palette lacks; use a palette that defines it, like [`examples/kit-gallery/palette.json`](../../examples/kit-gallery/palette.json).

```luau
local m = ui.motion
local width = ui.animate("pill.w", target, m.duration.switch, m.curve.out_cubic)       -- a number, re-evaluated per frame
ui.Box { id = "badge", width = 16, height = 16, radius = 8,
    fill = ui.mix("surfaceContainer", "primary", ui.animated("badge.on", on and 1 or 0, m.duration.badge, "out-back", 0)),
    color = ui.alpha("error", ui.pulse("badge.pulse", m.duration.pulse, .55, 1)) }
```

`ui.animated(key, target, duration, curve[, from])` and `ui.pulse(key, period, lo, hi)` return markers instead of numbers: the view is built once and the paint stage samples the value every frame, so animation frames run no Luau. A marker on `fill`/`color`/`border_color` (through `ui.alpha` or `ui.mix`), `radius`, `join` or `translate_x`/`translate_y` is patched into the cached scene; on any other property (width, height, left, ...) the layout is recomputed without Luau. `ui.animated` retargets from its current value and starts a fresh key at `from` (default: the target); `ui.pulse` loops `lo -> hi -> lo` every `period` until the key leaves the view and holds at `hi` under reduced motion.

`ui.track(driver, keys, values)` returns a marker whose value is a piecewise-linear function of the `driver` (a `ui.animated` or `ui.pulse` marker, not another track): `keys` are 2 to 128 strictly increasing finite numbers in the driver's own value space, `values` one finite number per key, and the sample clamps to the first and last value outside the keys. `ui.range(driver)` returns the driver's `from, to` (for `ui.pulse`, `lo, hi`; for `ui.animated` without `from`, the first build has `from == to`). Tracks patch like the marker they wrap, so several props can follow one driver with no Luau per frame; bad input is a Luau error.

```lua
-- A dot that moves along a curve as one driver runs 0 -> 1 (from = 0 starts a fresh key at 0).
local drive = ui.animated("dot", open and 1 or 0, m.duration.open, m.curve.out_cubic, 0)
ui.Box { width = 8, height = 8, radius = 4, fill = "primary",
    translate_x = ui.track(drive, { 0, 1 }, { 0, 120 }),
    translate_y = ui.track(drive, { 0, .5, 1 }, { 0, -30, 0 }) }
```

## Interaction poses

Spec [0280](../specs/0280-kit-batch3.md). The snippet paints with `surfaceContainer` and `onSurface`, so use a palette that defines them, like [`examples/kit-gallery/palette.json`](../../examples/kit-gallery/palette.json). These follow the pointer and focus without running Luau or laying out again: the paint stage samples them on every frame, and a view that uses none costs nothing.

```luau
ui.Row { id = "row", fill = "surfaceContainer", shadow = true,           -- the panel shadow on any surface
    ui.Button { id = "close", text = "close", visible_on_hover = "row",   -- fades in while `row` is hovered or keyboard-focused
        press_scale = .97, pressed_fill = ui.tint("surfaceContainer", "onSurface", .12), on_activate = ui.action("close") } }
ui.Column { id = "stack", gap = 8, reflow = ui.motion.duration.switch, ui.Repeat { ... } } -- children slide to new places
```

- `shadow=true` on `Panel`, `Box`, `Row`, `Column`, `Grid` or `Button` draws the panel/peek shadow (30 px margin, blur 10, the box's radius). `shadow` also takes a table, `shadow={blur=<0..64 px>, spread=<-32..64 px>, dy=<-64..64 px, positive is down>, alpha=<0..1>}`, every field optional with the `true` defaults (blur 10, spread 0, dy 0, alpha 1; an empty table is `true`): the raster is the shadow's own extent, `3 * blur + spread` around the box moved by `dy`, so a light pill shadow such as `{blur=4, dy=1, alpha=.5}` costs a small raster. Colour stays black.
- `radial = { {cx=, cy=, rx=, ry=, stops={{0, color}, {0.6, color}, {1, color}}}, ... }` on any element that takes `fill` (Box, Row, Column, Button, ...): up to 6 radial-gradient layers, first backmost, drawn over `fill`, under the border and clipped to the box's `radius`. `cx`, `cy` are the centre as a fraction of the box (-1..2, so a highlight can sit off the box); `rx`, `ry` the radii as fractions of the width and height (above 0, up to 4); `stops` are 2..8 `{offset, color}` entries with strictly increasing offsets in 0..1 and colours in any `fill` form (palette roles, `ui.alpha`, RGBA); the colour past the last stop is the last colour. Any bad value is a markup error; absent changes nothing. A box with layers is one cached raster (shared by identical boxes, re-rastered when a palette colour changes) and its border is drawn inside the box edge. `fade` fades the stops with the fill.
- `visible_on_hover="<ancestor id>"` (the ancestor's local id): the subtree's alpha fades (`fade`, out-cubic) to 1 while the pointer is anywhere in that ancestor or keyboard focus (`bead.focus_visible`) is inside it, else to 0. The ancestor's plain background then sets `bead.hovered` to its id when no control is under the pointer (a non-focusable view's hover id paints nothing). Hidden children stay nodes: the pointer, keyboard and accessibility reach them. Untinted images, shadows and graphs inside do not fade.
- `scale` (number, default 1, 0 < scale <= 8): scales the view and its subtree about its laid-out centre after layout, so siblings and layout are unaffected. Nested scales multiply and compose with `translate_x`/`translate_y` and scroll. Rects, corner radii, text size and hit boxes scale; stroke and border widths, shadow blur and an image's `bake_size` (with the radius baked in it, which the draw scales) do not. It accepts a `ui.animated` or `ui.track` marker, so motion frames patch without Luau or layout; text re-rasters while its scale changes.
- `press_scale` (0.5..1.5) and `pressed_fill`: while the pointer is down on this view or a descendant (from the press until the release or until it leaves the view), the subtree scales about the view's centre and the view's box fill mixes into `pressed_fill`, over `press` (.06 s) down and `release` (.12 s) up. Input keeps the laid-out boxes. `bead.pressed` stays the global press flag.
- `reflow=<seconds>` (0..2) on a container: after a relayout, each direct child (a `ui.Repeat` row keeps its key's id) slides from where it was drawn to its new place relative to the container; interruptible, a new child appears in place, reduced motion jumps. Clips inside a sliding child stay at its new place.

## Panel body and joined edges

A root `Panel` may name a child as its `body`: the reveal (`reveal=true`) grows that child out of the panel's edge, its box and shadow keep their corners while growing, and pointer input stops at it, so a transparent root can stay at the surface's maximum size while the body is resized, moved or reshaped with `ui.animated` markers (a hover peek that grows into a panel as one surface). `shadow=true` is accepted on `Box` for such a body (and on any surface, see below). `join_edge="top"|"bottom"|"left"|"right"` with `join` (0..1, default 1, animatable) makes a box's two corners on that edge square and flush with the surface it hangs from; `join=0` rounds all four corners (a detached panel).

`input = "children"` on the root (any root element) makes a bar's layer surface take pointer input only on the root's direct children that are interactive or paint something (a fill, border, image, text or shadow), counting their descendants: a pill takes clicks on its whole box, while spacers and the gaps between children pass clicks to the windows below. Without it the surface keeps its default input. The only accepted value is `"children"`, and only on the root.

`key_repeat = {delay, interval, min_interval, ramp_ms}` on the root `Panel` sets how a key held while the panel has keyboard focus repeats, in whole milliseconds, all fields optional: `delay` 0..2000 (press to first repeat; default the system delay), `interval` 16..1000 (default the system interval), `min_interval` 16..1000, not above `interval` (default `interval`), `ramp_ms` 0..5000 (default 0). The gap after each repeat eases linearly from `interval` to `min_interval` by how long the key has been held, counted from the press: `interval` at the first repeat (press + `delay`), `min_interval` at press + `ramp_ms`, then it stays there (a `ramp_ms` not past `delay` means `min_interval` from the first repeat). Without the prop the system repeat applies, and other surfaces always keep it. Repeats arrive as ordinary key events.

`palette_tween = false` on the root `Panel` (default true, a bool; a markup error elsewhere) makes a wallpaper palette tween deliver only its final palette to that panel, never the intermediate steps: use it on a panel that restages expensively and does not need the ease. Bars and overlays still get every step, and a fenced palette change always reaches the panel. A closed retained panel gets only the final step with or without the prop.

A `Panel` with `open` animates: `open` (default true) is the target state, `edge` (default `top`) the side it opens from, `duration` (seconds; unauthored, the panel profile's `clamp(travel / speed, open_min, open_max)`, where travel is `slide_distance`, else the body extent along `edge`), `transition`, `fade` (default true), and `return_duration` (default the profile's `close`) and `return_transition` (default an accelerating exit) for the close. With `reveal=true` the panel grows out of `edge` in place and closes as the open reversed, shrinking back into that edge; `slide_distance` (0..4000 px) slides the whole panel that far instead. `scale_from` (0.5..1, default none; an error with `reveal`) closes the panel to that scale about its own centre and opens it back (combines with `slide_distance` and `fade`); it needs the compositor layer-motion path and is ignored on the client-frame fallback. A peek whose body grows on a store, as in the [bar-with-panels](../../examples/bar-with-panels/) Status panel:

```luau
local ui = require("bead.ui")
local m = ui.motion
local tall = ui.resolve_for_check(ui.bind("tall"))
return ui.Panel { id = "peek", width = "fill", height = "fill", fill = "transparent", body = "body",
    open = true, reveal = true, edge = "top", duration = m.duration.peek,
    ui.Box { id = "body", position = "absolute", top = 0, left = 0, width = 300, shadow = true, fill = "surface",
        radius = 16, join_edge = "top", height = ui.animated("peek.h", tall and 300 or 120, m.duration.open, m.curve.out_cubic) } }
```

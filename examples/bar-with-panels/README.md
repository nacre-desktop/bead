# Bar with panels

A top bar whose clock button opens a calendar panel, whose volume button opens a volume panel, and whose Status button opens a small peek that grows into a larger panel as one surface (More/Less, [spec 0263](../../docs/specs/0263-motion-tokens-and-retarget.md)), all declared under `[panels]` in `bead.toml` and written in Luau. The bar and both panels share the declared `clock` and `audio` services. Escape, a click outside, or the panel's Close button closes the open panel; opening the other panel replaces it.

```sh
bead check examples/bar-with-panels
bead run examples/bar-with-panels
```

See the [popup tutorial](../../docs/tutorial/popup.md).

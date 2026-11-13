# Bar with panels

A top bar whose clock button opens a calendar panel and whose volume button opens a volume panel, both declared under `[panels]` in `bead.toml` and written in Luau. The bar and both panels share the declared `clock` and `audio` services. Escape, a click outside, or the panel's Close button closes the open panel; opening the other panel replaces it.

```sh
bead check examples/bar-with-panels
bead run examples/bar-with-panels
```

See the [popup tutorial](../../docs/tutorial/popup.md).

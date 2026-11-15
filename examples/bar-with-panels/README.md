# Bar with panels

A top bar whose clock button opens a calendar panel, whose volume button opens a volume panel, and whose Status button opens a small peek that grows into a larger panel as one surface (More/Less, [spec 0263](../../docs/specs/0263-motion-tokens-and-retarget.md)), all declared under `[panels]` in `bead.toml` and written in Luau. Three `[lips]` add edge handles: resting the pointer on the left or right screen edge (centre) or on the top edge inside the bar opens a side dock or the status peek, and the lip rides the opening panel's inner edge as its tab ([spec 0279](../../docs/specs/0279-lip-tab.md)); hovering the tab keeps a hover-opened panel open, clicking it closes. The bar and its panels share the declared `clock` and `audio` services. Escape, a click outside, or the panel's Close button closes the open panel; opening another panel replaces it.

```sh
bead check examples/bar-with-panels
bead run examples/bar-with-panels
```

See the [popup tutorial](../../docs/tutorial/popup.md).

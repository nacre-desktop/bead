# Window framework

`bead-wayland::Options::window` optionally configures an on-demand xdg toplevel through `WindowSpec`. Hosts build it with `WindowSpec::new(app_id, title, size, fallback, factory)` and then assign the optional fields (see [0262](0262-non-exhaustive.md)). The current spec supplies:

- `app_id` and `title` for window identity;
- `command_targets` for the exact commands dispatched to its source;
- initial logical `size` and `min_size`;
- optional `size_for_output` and `on_resize` host callbacks;
- a fallback scene, a `factory` with the shape `&[serde_json::Value] -> Result<Box<dyn SceneSource>, String>`, and a `model` mapper with the shape `&[serde_json::Value] -> BTreeMap<String, bead_core::Value>` that turns arguments into a store patch for an already-open window.

The framework owns window configure/resize, rendering, input routing, and close lifecycle. Keyboard, pointer, scroll, and cursor events use the same `UiEvent` model as panels. Closing retires the window's source, renderer, and buffers; a window is created only when opened. Hosts own route names, settings schemas, persistence, and process policy.

# Per-output UI sessions

A host may create one UI session per announced Wayland output through `Options::primary_factory` (`PrimaryFactory` receives the output name and returns a `SceneSource`). When unset, the existing shared primary UI remains in use. Each output session owns its source/runtime, stable-ID stores, committed scene, viewport, update/commit serial, and input target. Geometry and scale changes apply only to the matching session. Existing callers without a session factory retain single-source behavior.

Removing an output retires its source, subscriptions, and view resources. Events and callbacks from the retired generation must be fenced so they cannot affect a replacement session. A script failure or reload failure is isolated to that session: successful reload preserves its stable-ID state, failed reload leaves its previous UI active, and other sessions and services continue.

Service owners, immutable font/image resources, and render transport may remain shared across sessions. Service lifetime is outside view lifetime. The framework owns session routing and teardown; the host chooses which outputs to populate and supplies each session's content and output-specific model.

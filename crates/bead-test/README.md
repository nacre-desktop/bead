# bead-test (MIT)

A compositor-free harness through real Session/Luau, stores, Taffy, AccessKit, and the retained CPU renderer. It creates no Wayland connection, GL context, service connection, or system font lookup. Fonts retain their separate licenses in `fonts/`.

Load with `Harness::load(root, entry, palette, store_fixture, Config)`. Explicit icons use `load_with_icons` and a name-to-fixture-path map; missing mappings fail rather than consulting the machine theme. Images are snapshotted. Scale and viewport are fixed and render targets are capped. Input scripts use logical view coordinates. Accessibility returns the actual AccessKit tree, including focus.

`render()` and `pose()` return straight-alpha RGBA frames. `compare()` reads committed PNGs; failures write actual and diff images. `replay()` validates recorded input/presentation/CPU boundaries against configured budgets. Deterministic CI replay is not a hardware timing measurement.

Run `cargo test -p bead-test`. `BEAD_TEST_ARTIFACTS` selects a private failure directory. Goldens are immutable by default; `BEAD_UPDATE_GOLDENS=1` is an explicit capture operation. Review generated images before committing. Never regenerate to hide a failure.

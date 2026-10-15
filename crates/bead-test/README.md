# bead-test (MIT)

A compositor-free harness through real Session/Luau, stores, Taffy, AccessKit and
the retained CPU renderer. No Wayland connection, GL context, service connection
or system font lookup is created. Fonts and their separate permissive licenses
are bundled in fonts/. Application families map to those fonts for deterministic
Latin/icon fixtures; desktop fallback/emoji parity remains a separate proof.

Load with Harness::load(root, entry, palette, store_fixture, Config). Explicit
icons use load_with_icons and a name→fixture-path map; missing mappings fail
rather than silently consulting the machine's theme. Images are snapshotted.
Scale/viewport are fixed and render targets are capped at64MiB. Input scripts
use logical view coordinates, before Scene.offset: pointer/click/scroll, text,
backspace, Tab and editing keys. The public Session remains available for richer
assertions. Accessibility returns the actual AccessKit tree, including focus.

render()/pose() return straight-alpha RGBA frames. compare() reads committed
PNGs; default tolerance is2 channel levels and0.1% changed pixels. Failure writes
actual.png and diff.png. CI uploads them. Counters use actual thread-local Taffy
passes and content raster builds, including the opacity-group composite. The
first translucent frame prepares one composite; later transform/opacity frames
reuse it. Counters on one thread include other harnesses used on that thread.

replay() validates recorded numeric input/presentation/CPU boundaries against
presentation p99 and maximum per-frame CPU budgets. It rejects empty/reversed
samples. Deterministic CI replay is not a hardware timing measurement and does
not accept the paused launcher target.

Run cargo test -p bead-test; shell checks also run under cargo test -p nacre-shell.
BEAD_TEST_ARTIFACTS selects a private failure directory. Goldens are immutable
by default. BEAD_UPDATE_GOLDENS=1 is an explicit developer capture operation;
review the PNG and diff before committing. Never regenerate to hide a failure.

The shell notification/OSD goldens capture the paused checkpoint, not QML parity.
The static symbol fixture matches the full upstream font exactly for the notice
scene. The OSD fixture shows text beyond its pill edge at scale1; that visual
gap is recorded, not a parity acceptance. Feature9 itself remains paused.

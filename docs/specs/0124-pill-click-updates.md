# B0124 — Inert status pill and compact updates dropdown

The status pill is a visual container: its background, padding and gaps have no
click action. Each icon/count group retains its own widget-owned dropdown and
anchor. The existing widget registry defaults to click; hover is opt-in.
The control panel retains its own keybind, edge lip and explicit widget triggers.

The updates dropdown sizes to its content instead of reserving blank title/detail
lines. Its header shows the count (or Up to date / Updates unavailable) and local
last-checked time. Health and refresh messages occupy space only when present.
Update now uses the existing `tools.launch {tool:"updates"}` route; Refresh uses
`dashboard.updates.refresh`. Neither action is executed during validation.

## Optional package data

Lead continuation authorizes N0077's additive `packages` array with
`{name, old, new, repo}` records. The shell edge adapter reads it only when present;
older producers remain count-only. Invalid records are omitted and absent repo
labels have no badge. Package records are bounded to 4096, with control-free fields
up to 256 bytes. No package query, provider, dependency or ranking engine is added.
The deterministic fixture is `shell/tests/fixtures/status/updates-packages.json`;
the frozen B0002 dashboard fixture tree is unchanged.

All dropdown spacing, text sizes, row sizes and the list-height cap derive from
kit/theme tokens. Package rows use the existing virtual-list component, with a
224 logical-pixel viewport cap at the default theme. The panel stays joined to
the bar through the existing popout attachment and widget-centre anchor. No new
framework primitive or temporary duplicate implementation is introduced.

This is an existing-surface repair; no new always-on source or cache is added.
Package data lives only in the lazy updates view and releases with that view.
No memory improvement or laptop-cap acceptance is claimed without measurements.

## Validation

All Bead CI workflow stages passed in a disposable GHCR Arch container: Rust
1.99.0, nice 10, Cargo -j2, RUST_TEST_THREADS=2. Workspace/doc tests, trace memory
policy tests, renderer feature configurations, facade, Luau/schema, seven Python
regression suites, packaging, wallpaper smoke, private services/SNI/DBusMenu and
all dependency/license graphs passed. After relocating the test fixture, fmt,
Clippy and both updates tests passed again; no runtime code changed in that step.

The two-thread cap avoids existing 50 ms markup-test contention. Arch required
`swaybg` and removal of sway's file capability inside that disposable container;
no host policy changed. Archive tests used the actual committed Git tree/index.
Ten count/package/scroll/empty/unavailable goldens were regenerated and visually
checked at scales 1 and 1.5; badges have explicit in-panel bounds assertions.
The default count-only body is about 125 logical pixels high, down from 222.

Private nested desktop captures use CPU Bead, QML source at `1ab8bcda`, identical
palette, wallpaper, settings and fixtures, physical 2880 × 1800 at scale 1.5.
The fixture proxy supplies count 3 plus optional package rows and rejects updater
and refresh calls. QML uses a staged helper with recorded binary hash but an
unrecorded source revision; this proves UI parity against controlled data, not
the current production helper. Early failed setup attempts launched a read-only
`checkupdates` before private shims were added; no desktop upgrade or updater action ran.

QML's old view has count, optional Qt preflight warning and actions only. Bead's
checked-time header, primary Update now action, compact sizing, centred widget
anchor, click default and optional package list are intentional owner changes.
The local Qt preflight warning still has no daemon field and is not synthesized.
The 32-frame-per-arm opening proxy shows a later Bead reveal than QML: QML's
changed edge begins around 51 ms, Bead's around 251 ms. Bead remains partly
clipped at the 792 ms sample and is complete in the later settled shot. Visible
content already has the correct header and packages; this does not establish a
late data insertion or blank fallback. Overlapped `grim` spawn timestamps and a
CPU development build are not presentation clocks. This timing/parity gap is
retained explicitly; no animation, memory, latency or physical-scanout acceptance
is claimed by these captures.

Paired QML-left/Bead-right evidence: [idle](evidence/0124/bar-idle-paired.png),
[hover](evidence/0124/updates-hover-paired.png),
[pressed](evidence/0124/updates-pressed-paired.png),
[open](evidence/0124/updates-open-paired.png),
[inert pill padding](evidence/0124/pill-gap-click-paired.png),
[opening frames](evidence/0124/updates-open-sequence.png),
[validation, fixture and safety gates](evidence/0124/validation.json).
Both shells activate on release; hover and padding clicks keep the panel closed.
Inherited bar hover ornament still differs (QML's broader highlight versus Bead's
compact widget highlight); this unit preserves the existing bar styling.

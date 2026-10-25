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
The deterministic fixture is `shell/tests/fixtures/status/updates-packages.json`.

All dropdown spacing, text sizes, row sizes and the list-height cap derive from
kit/theme tokens. Package rows use the existing virtual-list component, with a
224 logical-pixel viewport cap at the default theme. The panel stays joined to
the bar through the existing popout attachment and widget-centre anchor. No new
framework primitive or temporary duplicate implementation is introduced.

This is an existing-surface repair; no new always-on source or cache is added.
Package data lives only in the lazy updates view and releases with that view.
No memory improvement or laptop-cap acceptance is claimed without measurements.

## Validation

Pending remote full CI, regenerated and visually inspected count/package/scroll/
empty/unavailable goldens at scales 1 and 1.5, and matched private nested desktop
QML-left/Bead-right captures. The old QML updates view has count, optional preflight
warning and actions only; Bead's checked-time header, compact sizing and optional
package list are intentional design changes requested by the owner.

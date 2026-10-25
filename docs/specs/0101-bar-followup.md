# B0101 — Bar follow-up: pearls, live title cue, status pill

Owner live use after B0096 found four differences from the QML header. QML reference: Nacre `origin/main` (`NacreWorkspaceRow.qml`, `NacreWorkspacePearl.qml`, `NacreHoverCue.qml`, `NacreHoverDwell.qml`, `NacreHeader.qml`, `NacreStatusIcons.qml`).

| Item | Cause | Change |
| --- | --- | --- |
| Pearl hues | Bead cycled primary/secondary/tertiary by workspace number. QML (`workspaceColors` off, the default) blends primary → secondary → tertiary by position `min(1, index / (workspaceCount − 1))`, so the pearls form a left-to-right gradient | `shell/bar/main.luau` computes the same blend with composable `ui.tint` from `bar.workspace.count` |
| Pearl enamel | One pre-mixed SVG per fixed accent name could not follow a blended accent | The pearl is the plain accent; two palette-templated neutral overlays (`pearl-current` / `pearl-occupied.themed.svg`: cream alpha 0.88→0.40→0 / 0.48→0.10→0 and a secondary shade 0→0.28 at the bottom, plus the specular highlight) reproduce the enamel gradient for any accent. Gradients off: flat warmed colour as QML `numberSurface` |
| Workspace thread | Missing | 1 px primary → secondary → tertiary line at 12 % behind the pearls (`workspace-thread.themed.svg`), hidden in icon mode and with gradients off |
| Title cue | The cue appeared at once and only as an underline; its visibility hung on `bead.hovered`, which the same event's later model update could discard (below) | QML cue: pill outline and tint (alpha `0.03+0.09p`, border `0.06+0.18p`, halo `0.025p`) fading in/out linearly over 120 ms, and the 2 px trace growing from the centre linearly with the dwell progress `p` over the configured hover delay (default 350 ms); it vanishes when the dashboard opens |
| Hover never reached the committed frame | `Session::update` always built on the **committed** stores, so a second change in the same event (the bar's tray-tooltip update after the pointer move, or any projector change) re-staged a scene without the staged `bead.hovered` and replaced the hover scene | `Session::update` builds on the staged stores when a same-generation transaction is pending; regression test `an_update_in_the_same_event_keeps_the_staged_hover` |

Generic additions (ADR 0004): `ui.alpha` accepts a colour expression (alpha of a tint) and both `ui.alpha`/`ui.tint` accept a bound amount (`ui.alpha(role, ui.bind(store))`; nested bindings in colour properties are resolved at layout); the host publishes `bead.edge.dwell` (0..1) and `bead.edge.fade` (0..1, 120 ms linear) for a bar-hosted lip, and a view that declares those stores draws them. No shell ids entered framework crates.

## Status pill spacing

Measured in a 2x crop of the same private session (2880x1800 at 1.5, physical px between glyph ink): QML sound→Wi-Fi 22, Wi-Fi→Bluetooth 24, Bluetooth→battery 35; Bead 22, 24, 35. Glyph centre pitch is 46/48/48 in both. The wider Bluetooth→battery gap is the narrow Bluetooth and battery glyphs inside the same 30 px cells; QML draws it identically, so no layout change was made. Evidence `status-pill-qml-top-bead-bottom-2x.png`.

## Validation and evidence

Private headless labwc → nested niri, 2880x1800 at scale 1.5, the desktop owner's presentation record (wallpaper and palette), two workspaces (one window on workspace 1, two on workspace 2; workspace 2 active), `--cpu`, under the desktop heavy lock, `spikes/parity-compare.py` through a small entry that adds the `b0101-*` states. Control = `8e0c487` (Bead), candidate = this change; QML arm = Nacre `3cc38598`. `evidence/0101/`:

- `before-pearls-...` / `after-pearls-...` (4x, QML top, Bead bottom): the thread line is back and enamel/ring match; `after-title-hover-...`: the cue and the half-grown trace appear live (control showed nothing in the same real session); `title-dwell-frames-...`: QML row above Bead row, every other frame of a ~1 s capture from ~300 ms: the trace grows from the centre with the dwell, the pill fades in and both vanish when the dashboard opens at ~700 ms in both arms.
- Remaining visible difference: the current pearl's grey ring is a little lighter and thinner in Bead.

Full `.github/workflows/ci.yml` equivalent (fmt, clippy `-D warnings`, workspace and trace tests, service boundary, Luau analyzer and generated schema, spike/packaging/lifecycle proofs, cargo-deny) passed in `ubuntu:24.04` on dev-2. Bar goldens (`bar-parity-*`, `bar-parity-hover-*` at 1x/1.5x) were regenerated and viewed. No temporary duplicate implementations were added.

## Known gaps

- The cue tint is flat primary; QML blends it horizontally into secondary (alpha 0.03→0.02 plus 0.09/0.10 p), a sub-1 % difference.
- QML tints the title text toward the ink by `0.25 p`; not ported.
- `workspaceColors` (per-wallpaper presentation option, three-colour cycle) is not read by Bead's bar; the default blended mode is implemented.
- The thread is hidden when gradients are off (QML draws it flat primary).

# B0155 — workspaces in one bar pill

Put the existing workspace indicators inside one token-styled bar pill and make the active capsule slide between them. Preserve workspace actions, app/window indicators, hover feedback, and widget drag behavior. Keep layout and colors token-driven; no Rust or backend changes.

## Implementation

- Add shared `pill_height` and `pill_radius` bar tokens and use them in the workspace and status pill shells.
- Keep the workspace rendering in `shell/widgets/workspaces/widget.luau`; no Rust or backend changes.
- Draw one shared animated active capsule behind the repeated workspace buttons; animate its position with QML's 180 ms glide and its width, cell widths, and outer pill width with QML's 160 ms resize, all `out-cubic`.
- Keep workspace cells and app-focus overlays as the existing UI nodes so click, scroll, focus, hover, and widget drag contracts remain intact.
- Mirror QML's 12% pressed feedback over the workspace cell while a pointer press is held.
- Match QML workspace spacing, pearl sizes, capsule height/radius, and motion; the owner-requested single outer pill is the intentional difference from separately surfaced indicators.
- No new kit primitive; no temporary duplicate paths.

## Acceptance and evidence

- Bar goldens at 1x and 1.5x, plus hover states, update with the unified pill.
- Paired QML/Bead screenshots and switch frame captures on the desktop test machine at 60 Hz and 120 Hz; verify idle, hover, pressed, switch, and workspace-count changes at identical geometry/palette.
- No dropped frames; active position and width change continuously when switching or adding/removing workspaces.
- Preserve widget drag behavior without editing drag implementation.
- Run the complete Bead CI job defined by `.github/workflows/ci.yml` in a remote container.
- Paired captures and measurement notes: [evidence](evidence/0155/README.md). Captures use synthetic 60/120 Hz nested scheduling, not physical scanout. Bead hover changes pixels; Bead pressed overlay did not register in the captured interaction and remains a known parity gap.

| Measure | Budget / target |
| --- | --- |
| Added always-on unreclaimable memory | ≤ 1 MiB |
| Active position / width, cell resize, and outer pill resize | 180 / 160 ms, out-cubic |
| Capture refresh rates | 60 / 120 Hz |

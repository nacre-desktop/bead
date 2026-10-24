# B0096 — Owner top-bar parity

The status values now share their icon's button, with centered line boxes instead of unrelated text heights.
The bar defaults to the QML header's visible tray behavior, and keeps an explicit `desktop.tray` opt-in.
Workspace cells use the QML logo/capsule dimensions and inside borders; pearl enamel is palette-templated SVG artwork.
The title removes the gray hover fill, defines a Luau underline and participates in the existing configured dashboard dwell without losing click or wheel input.

## Reference and implementation

QML reference: Nacre `origin/main` at `3cc38598ec508f8cbf11237080f1f22c8cf38ef4`.
Read `modules/topbar/NacreHeader.qml`, `NacreWorkspaceRow.qml`, `NacreWorkspacePearl.qml`,
`NacreWorkspaceAppIcon.qml`, `modules/bar/components/NacreStatusIcons.qml`, `widgets/BrandLogo.qml`,
`widgets/NacreHoverCue.qml`, and `modules/drawers/NacreFrameLips.qml` before changing the bar.

| Item | Cause | Change |
| --- | --- | --- |
| Status pill | Different text heights top-aligned glyphs/counts; battery/notification tails were siblings of buttons | Font-line-sized boxes centered in shared row buttons; clock and count widths keep complete text |
| Tray | Bead rendered all SNI rows while this QML header renders none | `Prefs` projects saved `desktop.tray`; omitted/false hides the row, true enables it; no application blacklist |
| Workspaces | 30 px logo instead of 32 shifted the capsule; 28 px cells misplaced app glyphs; centered strokes enlarged pearls | 30 px cells, 28 px active capsule, centered app line boxes; inside rings and 18/20/22 px enamel circles |
| Title | Generic action hover fill; actionable regions excluded edge dwell | Transparent action with Luau underline; reusable `edge_hover` opt-in retains the hand cursor and the existing 350 ms default/configurable 0–800 ms dwell |

The QML registry declares `tray=true` but marks it **planned** and hides its Settings row; the rendered QML header has no tray implementation.
Bead follows that rendered default. An explicitly saved true enables the existing Bead capability; this unit does not change Settings (B0098).
Generic additions: `edge_hover` / `SceneSource::allows_edge_hover`, composable `ui.tint` colours, and themed SVG `@base|overlay|amount@` colour stops (ADR 0004).
Geometry/colours/artwork remain in shell Luau, tokens and SVG assets; no shell IDs entered framework crates.

## Validation and evidence

Private headless labwc → nested niri, 2880×1800 at scale 1.5, identical read-only wallpaper/palette record, private homes/buses and daemon.
Two real private SNI objects advertise Bluetooth and Wi-Fi icons; their published tray snapshot is checked. A private app window populates workspace hints.
`spikes/parity-compare.py --states bar-idle,bar-workspace,bar-title-hover,bar-title-open,bar-memory-1,bar-memory-2,bar-memory-3` drives both arms.
The early hover uses a short pointer settle; the open state records `nacre state`. The memory cycles force identical dashboard opens/closes in both builds.
Capture work runs under the shared desktop heavy lock; every arm checks host swap <64 MiB and process SwapPss=0. No swap reset or desktop installation was used.
Evidence in `evidence/0096/` is QML-left/Bead-right at 2×. Crops use physical pixel rectangles: status `(2220,0,2880,75)`, tray `(2050,0,2325,75)`, workspaces `(75,0,420,75)`, title `(1260,0,1620,75)`.
Raw crop RMSE includes changing clock digits and native Qt/Bead glyph rasterization. Status improves **15.818→11.052%**, tray **14.313→3.120%**, workspaces **9.068→3.707%**; early title hover **8.474→7.832%**.
Full-frame captures were reviewed privately; committed crops exclude unrelated dashboard personal/system data.

Five alternating same-host control/candidate pairs, with three forced dashboard cycles per arm, pass the **+0.5 MiB median per-phase** unreclaimable gate. Build: debug; renderer: CPU bar / native-GL dashboard.

| Phase | Median paired Δ MiB | Pair range MiB | Clean-file PSS control/candidate MiB (information) |
| --- | ---: | ---: | ---: |
| Open | −0.314 | −0.873…+1.643 | 51.402 / 51.418 |
| Closed | +0.375 | +0.066…+0.391 | 51.402 / 51.418 |

Per-arm anon medians are 30.723→30.977 MiB open and 30.230→30.500 closed; shmem medians are unchanged at 6.555/7.686 MiB (open/closed), with DRM and SwapPss zero. PSS totals and every pair are in `memory-final.json`; paired and per-arm medians need not add. The settled increase is anonymous, with no framebuffer increase; individual allocator origins were not instrumented. The largest open pair (+1.643 MiB) includes +1.264 MiB shmem residency; open shmem varies by arm while its five-arm median stays unchanged. An earlier three-cycle pilot (+0.645 MiB open) was rejected. These are desktop relative measurements, not laptop-cap results.

Full `.github/workflows/ci.yml` equivalent passed in an isolated Arch container on dev-2: fmt, workspace/all-target clippy with warnings denied, workspace/default/feature tests, service boundaries, Luau analyzer/generated schema, spike/packaging/lifecycle/SNI/wallpaper proofs and cargo-deny including independent graphs. Bar idle/hover goldens at 1×/1.5× were regenerated and reviewed. The worker input-mirror regression proves that an opted-in actionable title preserves its pointer cursor and edge dwell. No tests or builds ran on the laptop.
Temporary capture executables: `wlr-randr` and `swaybg` were staged privately, not installed. No product dependencies or temporary duplicate implementations were added.

## Known gaps

The headless underline paints and its input path passes, but it is absent from the private compositor's 180 ms title crop; a private trace also showed no committed bar hover ID. Dashboard hover-open succeeds. This remains a compositor/runtime hover-visual gap, not established underline parity. QML's progressive trace and subtle halo are also missing; Bead currently defines a static underline. The configured dwell/open delay is retained.
Updates has no standalone Bead popout; its combined button opens the existing System dashboard with updates information. Adding that popout is outside this bar unit.
Qt native text and Bead's shaper/rasterizer retain edge/metric differences, including the Desktop symbol's fill; this is a parity candidate, with nonzero RMSE reported rather than pixel identity claimed.

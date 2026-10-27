# B0162: daily-use acceptance harness and regression repair

One command must exercise the production Bead shell against the current Nacre daemon build in a private nested niri session.
The run stages only the laptop wallpaper, desktop-entry and icon data into a private HOME and removes it on exit.
Four rounds cover click/Escape/outside dismissal for every bar dropdown, launcher, dashboard/calendar, control and AI panels, notifications, wallpaper selection/apply, and a widget drag; round four follows a two-minute idle.
Each surface must unmap with a pixel-identical closed screen, first content must arrive within two output frames, logs must contain no `UI_ERROR`, no thread may stay above 80% CPU for five seconds, and shell `Pss_Anon` must settle within +20 MiB.
The full run targets at most 20 minutes; `tools/daily-acceptance/run.sh --quick` runs one round without the idle or repeat.
Run against `900369fb` and current main; preserve failure screenshots and bisect only reproduced failures.

| Gate | Evidence |
| --- | --- |
| Reproducibility | One desktop command; private HOME, nested niri 2880×1800 at 1.5, lock per capture, exact PIDs cleaned |
| Functional | All four rounds and all dismiss paths; unmapped layer and empty pixel diff per close |
| Stability | ≤2-frame first content; zero `UI_ERROR`; no thread >80% CPU for 5 s; idle `Pss_Anon` delta ≤20 MiB |
| Regression | `900369fb` baseline, current main before/after, first bad commit and source-level cause |
| CI | Full `.github/workflows/ci.yml` recipe in the approved remote CI container |

No new daemon protocol or permanent desktop changes. Copied wallpaper, desktop-entry, icon and presentation data live in a mode-0700 temporary HOME and are deleted at run cleanup. Failure screenshots stay under private task evidence and are not committed. This replaces no existing harness.

## Source fix and full nested arms

`bda20168` first set `text_focus_only=true` on bar popouts. `bead-wayland` then withheld keyboard focus when no text input was active, so Escape could not dismiss a clicked status panel. The task branch restores keyboard interactivity and commits a null buffer on close. The main-before full capture reproduced the panel remaining visible after Escape; this is consistent with the reported upper-right artifact. `4c206d51` separately made the control panel text-only; `24709327` already restores its keyboard behavior.

| Arm | Full-run evidence |
| --- | --- |
| `900369fb` | The legacy `UI_PRESENTED` fallback measured 84/96 opens at p50 80.781 / p99 81.352 ms; its boundary differs from `PANEL_OPEN_LATENCY`. Outside close and idle left 7 mapped layers vs 6. Pss_Anon +9.074 MiB; no UI errors. |
| `24709327` main-before | 42 marker samples p50 246.056 / p99 278.685 ms. Escape left the Updates panel visible; 24 widget UI errors; Pss_Anon +18.578 MiB; idle layers returned to 6. |
| Rebased current-main candidate (`5a39635d`) | 35 marker samples p50 245.732 / p99 270.954 ms, 72/96 close screenshot diffs, and no immediate unmapped close. Widget UI errors are 0; drag commits, Pss_Anon +18.195 MiB, thread CPU, swap and idle-layer checks pass. Battery gap/re-arm passes. |

The 900369fb fallback and main marker use different start events, so their timings are not a matched comparison. The current candidate remains above the two-frame first-content limit. Its `routes.log` is empty: virtual keyboard Super chords did not invoke nested Niri shortcuts, so launcher, dashboard, control and wallpaper hotkeys remain unverified. The main-before full screenshots show a still-open Updates panel after Escape; current candidate screenshots still differ after close. The exact live black-square pixel was not isolated, and no first-bad commit for slow opening was established. The 230 ms popout reveal in `popouts.rs` predates the baseline.

Widget canvas captures found missing `session.confirm`, `session.message`, `status.hovered` and view dimensions in isolated stores. Current main includes the state declarations and runtime defaults; the candidate reports zero UI errors. Full CI-equivalent on dev-2 passed for `5a39635d` (workspace suite, Luau analyzer 0.742, service checks, cargo-deny and release arms). The later input-only harness commit `df6c8b4e` was syntax-checked locally and exercised by the full desktop capture; it routes modifier chords through the parent seat and text/Escape through nested Niri. Private screenshots and full reports are retained under `.lead/evidence/0162/` and are not committed.

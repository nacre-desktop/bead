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

## Evidence and result

Source bisect confirms `bda20168` first sets `text_focus_only=true` on the bar popout specs. In `bead-wayland` this suppresses keyboard interactivity while no text input is active, so Escape cannot reach the status panel to dismiss it. The fix restores ordinary keyboard interactivity for the bar, session, notification and AI panels and adds a constructor regression check. Closed companion surfaces now commit a null buffer after their close frame while retaining the resident scene/renderer for a warm reopen. The separate control-center regression from `4c206d51` was already fixed on current main by `24709327`.

Still pending: nested reproduction, baseline `900369fb`, first bad commits for the reported slow openings and black square, and post-fix acceptance run. `.lead/live-journal-24709327.txt` records 242–367 ms `PANEL_OPEN_LATENCY` samples but no panel-map state or stack evidence tying those symptoms to a Bead commit; B0160's separate CPU-compression trigger also remained unreproduced.

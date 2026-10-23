# B0064 - Colors app and Welcome colours page on N0050

Status: **Review** (Claude worker, branch `task/b0064-colors-live-20261023`). Consumes Nacre N0050 (`09cf91fc`,
`docs/specs/0050-colors-app-contracts.md` in Nacre): `colors.catalog`, `colors.preview`, `colors.apply`, `colors.export`,
`colors.targets`, `colors.set_target`. Pin `nacre-protocol` moved to `09cf91fc0596010a9c66411469bba300b1120113`.
Builds on B0060 (Colors app, `0060-colors-app.md`) and B0059 (Welcome, `0059-welcome-app.md`).

## What changed against B0060's asks

| B0060 ask | N0050 as published | Bead |
| --- | --- | --- |
| A2 older preview waits for the newer | a newer `colors.preview` cancels the older one in the daemon (older call fails `busy`, retry true) | **newest wins**: every request goes out at once; the older ticket is dropped, a stale reply is ignored by generation |
| vision keys | `protan`/`deutan`/`tritan` | unchanged (already read as such) |
| `comparisons[]` | extra `minimumContrast` | Compare cards show "Text contrast N" under the style name |
| A3 apply | `{applied, name}` | `applied:false` shows the action-failed line; `busy` (one action lane) shows "another action is still running" |
| A6 targets `current|stale|tool-missing`, `settings.set colors appThemes.<id>` re-themes | statuses `current|not-published|tool-missing|paused`; **only `colors.set_target` re-themes** | Settings, Colors / App theming: each `app-theme` row is On/Off buttons that call `colors.set_target {id, enabled}`; the line under it is the status from `colors.targets` (read when the page opens and again after a change). Ids not in the daemon's list never become a call. Without the methods the rows stay "not available here" |
| A7 file chooser | not published | "Open image" stays out (capability-gated: nothing to gate on yet); image by argument or wallpaper row |

## Welcome colours page demo

Wallpaper scenes (first four catalog entries, static first, list thumbnails), the eight style chips (incl. `tide`),
dark/light buttons, the miniature desktop (`kit.PreviewCard` with the preview poster) and the four palette swatches
from the preview. The page's adapter state is a second `Colors` instance in `welcome.rs` (`Colors::demo`): the catalog
and first preview are fetched only while the page is shown; actions are `demo:<op>:<payload>` through `welcome.do`.
Without the daemon methods the page shows the window's own palette (B0059 behaviour).

**Deviation from the QML:** the QML demo applies each choice to the real desktop (snapshot, restore on close). Bead only
previews through `colors.preview`; nothing changes on the desktop, so no `demo-start`/restore is needed and a crash
cannot leave the owner's wallpaper changed. The page says so ("This is a preview..."). Apply stays in the Colors app.
`bead.welcome.colors-intro/-more` changed accordingly.

## Live palette

Apps already start with the live wallpaper palette (B0056: `app::live_palette_file`); nothing to add. The Welcome app now
follows `settings.store.colors` and `wallpaper.active` like the Colors app (`app.rs`).

## Tests

Fake daemon over a real socket: preview cancellation (`busy` older reply dropped, newest request used), apply, export,
apply refused `busy`, refused preview, `minimumContrast` caption, demo (no fetch while hidden, eight styles, `tide`,
second scene, never `colors.apply`), theme targets (status per id, `set_target` payload, list read again, unknown ids and
values send nothing). Goldens, eyeballed: `colors-compare-1` (contrast captions), new `welcome-colors-demo-1`; `welcome-colors-1`
is the no-daemon fallback.

## Known gaps

- Open image (A7) and file drop: wait for the portal method.
- The Colors app itself has no target list (as in the QML; targets live in Settings).
- Look-and-feel parity run against the QML: see "Parity" below.
- Welcome demo scene thumbnails are the 256 px list thumbnails (QML used 512x288 previews): slightly softer.
- Page transition and scrollbar gaps of B0059 are unchanged.

## Temporary duplicates

None added. The QML Colors and Welcome stay until the switch is trusted (delete 7 days after).

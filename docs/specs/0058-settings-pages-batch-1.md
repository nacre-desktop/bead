# B0058 - Settings pages batch 1 (store-only pages)

Status: **In progress (slices 1-2 landed)** (Claude worker, branch `task/b0058-settings-pages-1-20261023`). Builds on B0045 (framework).
Nacre reference: registry copy at `4d816ca2`; QML source `nacre/shell/modules/settings/{prototype,registry}/*`.

## Slices

1. **Done: Panels (top bar, dashboard, control center, AI sidebar, bottom), Windows & workspaces, Date/time/language.**
   Added to `shell/settings/registry/bead.json` (no Rust for the pages). Lock & login stays view only (locker is external;
   login-theme install is privileged: Nacre ask below).
2. **Done: Launcher (+ favorites), Apps (default apps), dashboard sub-pages (pages, extras, weather, media, home-time,
   calendar-focus).** New general kit control: `kit.ListEditor{reorder=true}` (up/down per entry); which array settings are
   reorderable is data (`orderedLists` in `bead.json`, which also lifts `editorOwned`). `entityType` app, command, text-list and
   navigation lists are plain string lists.
3. Moved to B0063 (done there): Notifications history (needs a published notifications list/clear route), Workflows (`device-content`), launcher
   custom entries (object list), startup apps (planned, hidden), app picker control, Date & time section, panels diagram.

## Parity rule found in QML (adapter change)

QML (`registry.js settingsFor`, `SettingsView.qml`) hides `backend: "planned"` settings and sections that end up empty
(outside development). The adapter now does the same (`Settings::planned`), in pages and in search. This removes top-bar
Modules/Clock format/Tray, lip size, bottom panel enabled, workspace count/names/icons, startup apps from the pages.

## Page status

| Page | Bead | Gap |
| --- | --- | --- |
| panels | lips toggles/choice/number, links to sub-pages | "Your desktop edges" diagram (`panels-diagram` custom section) shows a placeholder |
| panels/top-bar | enabled, window hints | none beyond hidden planned rows |
| windows-workspaces | gaps, border, corners, blur, shadow, focus follows pointer, overview, workflows link | none; `monitorWorkspaceActions` enabledWhen capability `niriFixedWorkspaces` is not evaluated (Bead has no capability set) |
| date-time-language | language, Fahrenheit, weather location | "Date & time" section (`device-content`: time zone, 24 h) needs a published system-clock route: **Nacre ask** |
| lock-login | read-only | editing waits on the locker contract; login-theme install is privileged: **Nacre ask** |

Known gaps shared by all pages: registry `visibleWhen`/`enabledWhen` capabilities are not evaluated; retention keys have
no consumer anywhere (B0035) and are shown as QML shows them.

Slice 2 rows: launcher (sidebar order/hidden lists, search sources), favorites/hidden as string lists (no app picker yet:
entries are desktop ids typed in), apps defaults as argv lists (one argument per entry, reorderable), dashboard pages and
extras order as reorderable lists, editor-owned scalars (photos, lyrics, album palette, home zone/city, calendar file,
focus minutes) as normal rows. Control-center tile order stays with its own editor (B0049); launcher entries stay text
("not available in Bead yet") until an object-list editor exists.

## Checks

Goldens `settings-{panels,panels-top-bar,windows-workspaces,date-time-language,lock-login}-1.png` (eyeballed, CPU
renderer, test symbols font so sidebar icons differ from the real font). Fake-daemon test
`panels_windows_date_and_lock_pages_follow_the_registry`. Side-by-side nested comparison with the QML window: not run in
slice 1 (listed as open in the report).

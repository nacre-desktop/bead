# B0052 - On-demand app mode (Settings and the AI sidebar as separate short-lived processes)

> B0128 retired `b0052-app.py` and its only importing script, `b0059-welcome.py`. Current Rust app lifecycle/Welcome tests and packaging CI remain; historical measurements below are preserved.

Status: **Review** (Claude worker, branch `task/b0052-app-mode-20261023`). Lead decision 2026-10-23: ADR 0005 says on-demand
windows stay separate apps. B0045 (Settings) and B0043 (sidebar) were first built inside the shell; B0045 measured +0.69 to
+0.92 MiB PSS left behind after one open/close (gate +0.5). A process returns all of it.

## What it is

`nacre-shell --app NAME [ARG] --command-socket PATH [--daemon-socket PATH] [--palette PATH] [--output NAME]`, the same binary
(kit, theme, fonts, renderer and Luau are shared code, not shared memory).

| Piece | Where | Rule |
| --- | --- | --- |
| Framework primitive | `bead_wayland::Options::app: Option<Vec<Value>>` | `Some(args)`: no primary surface (the bar-off path), opens the window (or default panel) with `args` at start, `signal.stop()` when that window or panel closes. Product-agnostic; no app names. |
| Host (always-on shell) | `nacre-shell/src/app.rs` `Host::route` | Calls the running app on `<dir of the shell's command socket>/app-NAME.sock` (owner-only dir): `bead.window open [page]` deep-links, `bead.panel toggle` closes the sidebar. No answer: start the app (`--detach`: the new process forks and exits at once, the shell waits ~1 ms, the app is reparented to init: no reaper thread, no handle). The shell keeps one path and nothing else. |
| Single instance | socket bind (`ipc::Server`) | A live socket refuses a second bind (that process exits); a stale one is replaced. |
| App process | `app::run` | Own `nacre_source::Client` with hello `nacre-app-NAME`, topics `i18n` + the four settings stores, **no provider registration** (daemon still sees one UI provider). Applies the same prefs as the shell (reduced motion, text style, sheen, live frame size; the sidebar waits at most 60 ms for the first settings so its position is right). |
| Routes | `commands::map_views` | `nacre settings [page]` -> `app::route(settings)`; `nacre left` / `drawers toggle left` -> `app::route(chat, toggle)`. Both reply immediately. |
| Views | unchanged | `settings::window_spec`, `popouts::chat`, `shell/settings/**`, `shell/chat/**` and their adapters are untouched; only the host changed. |

Removed from the shell process: the `chat` panel view, `options.window`, and the `bead.window` command route. Nothing of either app is
built, subscribed or resident in the shell; the always-on shell has no Settings or chat code path that runs.

Third-party apps: add a name to `APPS`, a branch in `app::run` (window spec or panel spec) and a route. ADR 0004's Generalization list has the entry.

## Gates (desktop, private nested niri on headless labwc, CPU renderer (tiny-skia), fake daemon, release build, `spikes/b0052-app.py`)

| Gate | Result |
| --- | --- |
| App process gone after close | Settings (window close) and sidebar (toggle) both gone within the poll window in every cycle; also after `SIGKILL` of the sidebar the shell kept running and its bar layer stayed (steps all true). |
| Single instance | A second `nacre settings notifications` while open: same pid, one window, deep-linked. |
| Cold open, IPC call -> first niri poll that sees the surface (n=6 each, poll cost ~5-10 ms included) | Settings p50 33 ms, max 86 ms. Sidebar p50 73 ms, max 90 ms. Budget 150 ms: met. Fake daemon (local), CPU. The real daemon adds its connect time to the sidebar's 60 ms settings wait only if it is slower than that. |
| Shell PSS vs control, 3 alternating pairs, median candidate - control, 3 x (Settings open/close + sidebar open/close) | idle +0.03 / +0.13 MiB (two runs); after close and settle **+0.48 / +0.52 MiB (anon +0.39 / +0.41)**. Gate +0.5: **borderline, 0.48 passes, 0.52 misses by 0.02**. |
| Where the residue is (2 pairs, one app at a time, closed phase) | sidebar alone **-0.03** (a layer surface: nothing tracked); Settings alone **+0.34**; a foreign `alacritty` window opened and closed 3x **+0.12** (anon +0.25). The shell's bar/backend observes any toplevel (title, app id, icon lookup), so the residue is window tracking in the always-on shell, not app mode. The same conditions with Settings inside the shell measured +0.69 to +0.92. |
| Goldens | unchanged (no view code touched; `cargo test --workspace` in CI). |

Not measured: the real daemon, fractional scale, two outputs, a GL renderer, the sidebar's join with the bar's frame (below).

## Open issues (one line each)

- The sidebar draws its own joined edge (`with_frame`), but the bar's frame tiles are made by the shell and cannot know a panel in another process is open (`engages_decorations`): the frame rim under the panel is not cut. Fix is a general "external panel" description sent to the shell (origin + body rect); left to B0047's owner since it edits the same decoration path.
- Panel apps open on the first output (or `--output`); the QML sidebar followed the focused output. Needs the focused output from the shell (B0053 owns focused-output parity).
- Settings window tracking in the shell (+0.12 to +0.34 MiB) is outside this unit; a cheaper icon/title cache in the bar is the lever.
- The frame lip `assistant` is still unregistered; retarget it with a `bead.app` open when a lip may start an app.

## Temporary duplicates

None. The QML Settings app and sidebar stay until the switch is trusted (Nacre rule).

## Checks

`app::` tests (route unknown app, spawn argv, a live app gets the call and nothing is started); nested smoke and memory above (`spikes/b0052-app.py`);
full CI job on dev-2 (see report).

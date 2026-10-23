# ADR 0005 — One always-on UI process and one UI provider

Status: Accepted 2026-10-23 (lead as spec agent, under the owner's standing delegation).

## Context
B0020 implemented notifications and OSD as a separate `notice_service` process.
Its candidate − control PSS was +1.9 to +2.7 MiB, mostly anonymous memory: a
second process duplicates the runtime, Luau VM and renderer. The Nacre daemon
also accepts one UI provider, so two Bead processes conflict over provider-only
calls (hover pause, suppression).

## Decision
Bead's always-on UI runs in ONE process that registers as the single UI
provider. The bar (per-output sessions, B0025), launcher, panels, dashboard,
notifications/OSD and session menu are session types inside that process. They
share services, fonts, palette, renderer and the daemon connection. On-demand
windows (Settings, Colors, Welcome, AI sidebar, Brain) stay separate apps.
Crash isolation (ADR 0003) still holds, because services live outside the view
in the Nacre daemon.

## On-demand apps (B0052, 2026-10-23)
"On-demand windows stay separate apps" is implemented as app mode: the same `nacre-shell` binary run as
`nacre-shell --app NAME [ARG]`. An app is one xdg window (Settings) or one layer-shell edge panel (AI
sidebar). It shares code (kit, theme, fonts, renderer, Luau), never memory with the shell.
- The always-on shell only routes: `nacre settings [page]` and the sidebar toggle go to `app::route`, which calls
  the running app on its own owner-only socket (a second open deep-links the window; a toggle closes the panel) or
  starts it. The shell keeps no app state (B0045's Settings and B0043's sidebar were first built inside the shell
  process and left +0.7 to +0.9 MiB PSS behind after one open/close; a process returns all of it).
- Single instance is the socket bind. The app ends when its window or panel closes; a crash never reaches the shell.
- An app has its own daemon connection with a client role. It registers nothing, so the daemon still sees exactly
  one UI provider.
- Reports: an app's memory and CPU are listed per app and not charged to the always-on 25 MiB budget; the shell's
  own gate is relative to a control with the same binary left idle.

## Consequences
- B0030 moves notices into the main process; the separate-process path gets an
  expiry.
- New always-on features are added as sessions, not processes.
- Per-feature memory is judged by the relative gate within the one process.

## Exception: the wallpaper renderer (B0046, 2026-10-23)
Video decoding is not a UI session and not a UI provider. `bead-wallpaper-video` is a
separate small process supervised by the shell: it registers nothing with the daemon, links
no Luau, fonts or UI renderer, and owns only its own background layer surfaces and one
decoder. The reasons are the memory gate and fault isolation: libmpv and its codec libraries
(about 40 MiB of file-backed and 35-75 MiB of anonymous memory while decoding) must never
count against, or crash, the UI process, and the process can drop its decoder and shrink to
the last frame while paused. Its memory and CPU are reported separately from the UI budget.
Rule for later features: a helper process is acceptable only when it is not a UI provider,
has no daemon registration, and is reported separately.

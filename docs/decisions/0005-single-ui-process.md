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

## Consequences
- B0030 moves notices into the main process; the separate-process path gets an
  expiry.
- New always-on features are added as sessions, not processes.
- Per-feature memory is judged by the relative gate within the one process.

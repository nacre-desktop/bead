# B0030 — Notices and OSD inside the main shell process

Status: **Implemented; review.** Replaces the separate notice process of
[B0020](0020-notifications-osd-product.md). No live ownership or switch authorization.

## Why

B0020 shipped notices/OSD as a second `nacre-shell` process. That failed the relative
memory gate (+1.9 MiB idle, +2.7 one popup, +2.3 closed over the control; mostly anonymous
memory: a second runtime, Luau VM, renderer and daemon link), and the daemon allows only
one UI provider, so hover pause and suppression only worked when the notice process was
the provider and the bar/launcher process was not.

Decision (lead, 2026-10-23): Bead's always-on UI is **one process and one UI provider**.
Notices and the OSD are views inside it. They share services, fonts, palette, renderer
and the single daemon connection / provider registration; idle they hold no surface.

## Design

- **Views.** `notices` (top-right popup stack, 12 px insets below the bar) and `osd`
  (bottom-centre level card) are entries of the shell's panel catalog
  (`notice_views.rs`), built only with `--notices`. They reuse the B0020 Luau views, so
  the lazy open/closed+idle release of the panel lifecycle applies unchanged: when no
  popup or OSD is up, no worker, raster or surface exists.
- **One daemon link.** `ui_provider::start` subscribes `notifications`, `osd` and `audio`
  next to `menu`; `notice_service::Service` consumes those topics on that client and
  sends `set_hover`/`set_inhibited` as the registered provider. The service no longer
  opens a connection or registers a provider of its own. `notifs` dispatch from the
  daemon reaches the shell's command socket; brightness feedback (N0036 `level_percent`)
  is delivered in-process by the key route.
- **Offering a view.** Each bar session registers as a *driver* (`Notices::drive`); the
  oldest one leads. The service sends it `UiEvent::CompanionOffer {view, output, open}`
  when popups or the OSD should be up (OSD wins) and when they should close. The
  framework event (bead-wayland, permissive, application-neutral names) opens `view`
  only while the panel slot is idle or holds another *offered* view, so an offer never
  displaces a launcher, dashboard, popout or session the user opened; a closing offer
  only closes the offered view that is shown. Offers use the leading bar session's
  output.
- **Suppression.** `PanelSelected`/`PanelDismissed` already reach the bar sessions; they
  forward the shown view to `Notices::panel_changed`. While a view other than `notices`
  or `osd` is shown, or the OSD is visible, the service sends
  `notifications.set_inhibited(true)` from its worker (never the transport callback);
  the daemon then publishes no popups, and leaving suppression never replays them.
- **Removed.** `--notification-daemon`, `--notification-provider` and the second
  process. Kept: `--notification-bus` plus `--ui-notification-socket` (private
  compatibility owner, see Duplicates). New flags: `--notices` (needs `--bar
  --core-views --ui-provider`) and `--notification-action-on-click`.

## Behaviour change to accept

The panel slot is single, so a popup stack yields to **any** other shown view,
including the launcher and dashboard. QML suppresses popups only for popouts, the
session menu and the OSD, so with the launcher or dashboard open a notice no longer
appears over it (it is hidden by the daemon like any suppressed popup). A third
independent surface slot in `bead-wayland` would restore that; it is a framework
change of a different size and is not part of this unit. Popups also show on the leading
bar session's output rather than a compositor-focused one.

## Acceptance (2026-10-23)

- `nacre-shell` + `bead-wayland` unit tests (service offers/yields, published-transcript
  replays, fake-daemon calls); fmt, warnings-denied clippy and the full `ci.yml` job in
  the Ubuntu CI container on dev-2: see Evidence.
- Nested smoke, desktop, private dbus, labwc + niri, real `nacre-helper daemon
  --notifications=daemon` from Nacre `origin/main` (`3ccdccd2`) on the private bus,
  `spikes/notice-daemon-smoke.py` against the **main shell** (`--bar --core-views
  --ui-provider --notices`): the 32 B0020 checks plus three for the in-process design
  (notices are the `notices` panel view; another view suppresses and lifts without
  replay): **35/35**. The real notification name is never touched.
- Relative memory gate, `spikes/b0030-memory.py`: five alternating control/candidate
  pairs, same host, CPU tiny-skia renderer, one nested output, idle / one popup /
  closed. Control = the same main shell without `--notices` (B0020 base `3ad5bc7`).

## Evidence

Memory (desktop, nested niri on labwc headless, 1280x720@1, **CPU tiny-skia** renderer,
release builds with default features, `--bar --core-views --ui-provider`, five alternating
control/candidate pairs, medians of five 1 s samples per arm and phase, then medians over
arms; control `3ad5bc7` = the same shell without `--notices`; swap 0 B before every arm,
MemAvailable 12 GiB; DRM not measured, unknown). Candidate `nacre-shell` sha256
`5e443958af28...`, control `8378c4a6560c...`.

| Phase | Control Pss [min-max] MiB | Candidate Pss [min-max] MiB | Delta Pss | Delta anon | Delta file | Delta shmem | SwapPss | Gate (+0.5) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| idle | 30.41 [30.33-30.76] | 30.79 [30.41-30.85] | **+0.381** | +0.070 | +0.297 | 0 | 0 | PASS |
| one popup shown | 30.44 [30.29-30.79] | 31.71 [31.27-31.83] | +1.270 | +0.641 | +0.502 | +0.154 | 0 | cost while shown |
| after close (10 s) | 30.51 [30.39-30.89] | 29.99 [29.60-30.12] | -0.515 | +0.246 | -0.726 | 0 | 0 | PASS |

B0020 (separate process, same host class): +1.9 idle / +2.7 popup / +2.3 closed. Idle is
within the gate: anonymous growth is 0.07 MiB and the rest is file-backed text of the
larger binary (shared libraries are the same). The popup phase is the cost of the
resident view (Luau VM and scene for the stack, one 444x181 CPU surface: anon +0.64, SHM
+0.15, file +0.50 of touched code); it is released when the popup closes (closed anon
+0.25 MiB, below the 1 MiB unexplained-growth limit; file-backed pages read below the control).
Nothing is retained per notice.

Nested smoke, final binary: `spikes/notice-daemon-smoke.py` 35/35 (the 32 B0020 checks,
including shell restart, daemon loss without owner fallback and the untouched real name,
plus: popups are the `notices` panel view; another view suppresses via `set_inhibited`
and lifting it replays nothing).

Defect found and fixed on the way: a popup resize right after opening could find all three
SHM buffers still held by the compositor and `draw` failed the whole shell ("three buffers
still in use at callback"; 1 in 14 nested popups before, 0 in 70 after; the same code
serves every panel). The draw is now retried 8 ms later (`bead-wayland/rendering.rs`).

CI receipt: see STATUS.

## Temporary duplicates and expiry

- The private notification owner (`--notification-bus`, `--ui-notification-socket`,
  `notifications::Owner` and `bead-services` private Server/Center) stays only as the
  B0020 compatibility fixture used by tests and `spikes/foundation-preview.py`. Delete
  it **7 days after the daemon replacement is trusted** (B0020 rule, unchanged).
- The separate daemon notice process is deleted outright; no duplicate remains.

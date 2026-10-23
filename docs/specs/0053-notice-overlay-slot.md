# B0053 — Notice popups: own surface slot and focused output

Status: **Implemented; review (CI and nested evidence below).** Restores the QML behaviour
that [B0030](0030-notices-in-main-shell.md) gave up. No live ownership or switch authorization.

## Why

B0030 made notices and the OSD views of the shell's single panel slot, so a popup stack
yielded to any other shown view (launcher, dashboard) and appeared on the leading bar
session's output. The QML (`NacrePanelHost.qml`) suppresses the stack only while a popout is
open, the session menu is open, or the control drawer (`visibilities.osd`) is shown, and the
owner wants the same behaviour.

## Design

- **Framework primitive: the overlay slot** (`bead-wayland/overlay.rs`). A second view slot
  beside the one resident panel and the window: a content-sized layer surface
  (`Layer::Overlay`, no keyboard focus) on one output, hosting a product `SceneSource` from
  the same `PanelSpec` the application already builds (factory, fallback scene, positioner,
  passive). `Options::overlays` is a named catalog (`bead_wayland::Overlays`, one to eight
  views). Nothing is resident until a view is offered; closing fades out (the scene's own
  motion) and then drops the source, renderer and every buffer.
- **Offering.** `UiEvent::CompanionOffer {view, output, open}` routes by catalog: an overlay
  view opens in the overlay slot whatever the panel shows; other views keep the B0030 panel
  rule. The framework has no view names, placement or suppression: placement is the spec's
  positioner (or its motion edge), and the application decides when to offer or withdraw.
  A shown view keeps its output (the offer's output only chooses it when the slot is idle);
  offering another overlay view replaces the shown one. Hiding the shell or removing the
  output releases the overlay. Introspection: `bead overlay` (state) and, with the private
  input proof, `bead overlay-regions`.
- **Notices and OSD use it.** `nacre-shell` registers both in `options.overlays`; they are
  no longer panel views, so `PanelSelected` never names them. The OSD card keeps its
  configured bottom-centre placement and now also shows over a launcher or dashboard.
- **Suppression (QML parity).** `Options::coexist` (set in `main.rs`: `launcher`,
  `dashboard`, `chat`) names the view families that may stay shown beside popups: a view `p`
  or `p-...`. Every other panel view (calendar, sound, status popouts and peeks, session,
  control) suppresses popups through `notifications.set_inhibited`, as before; the OSD still
  suppresses them. Leaving suppression never replays popups.
- **Focused output.** The service subscribes the daemon `compositor` topic on the existing
  link (best effort) and reads `focus.outputId`. Offers carry it; the overlay uses the
  output of that name, else the leading bar session's. A focus change applies to the next
  popup that opens; a stack already shown does not jump between outputs.

## Not in scope

- The overlay has no keyboard, dismiss layer, accessibility bridge entry, slide
  (`Scene::slide`) or GL renderer: it is a CPU transient surface for passive and pointer-only
  views. A companion that needs more should use the panel slot.
- Moving a shown stack when focus changes (new popups follow focus; see above).

## Generalization (ADR 0004)

`bead_wayland::Overlays` and `Options::overlays` (second view slot, offered by name),
`notice_service::Options::coexist` (view families that do not suppress). Both are
application-neutral; Nacre's view names appear only in `nacre-shell/src/main.rs`.

## Temporary duplicates and expiry

None. The overlay's draw/fade is a trimmed copy of the panel path's motion handling; unify
both on one slot abstraction when a third companion slot is needed, not before.

## Acceptance

- Unit and fake-daemon tests: offers carry the focused output and fall back to the leading
  one; a daemon-published focus change reaches new offers; launcher, dashboard and chat
  coexist while popouts, session and control suppress.
- Nested smoke (`spikes/notice-daemon-smoke.py`): popup is the `notices` overlay view with the
  panel slot idle; a popup shows while the launcher is open; a popout suppresses and releases
  the overlay; suppression lifts without replay.
- Relative memory gate against the pre-B0053 main (idle at most +0.5 MiB; after close
  settled), CPU tiny-skia renderer: see Evidence.

## Evidence

- Full `ci.yml` job replicated in `ubuntu:24.04` on dev-2 (rustc 1.99.0): fmt, clippy
  `-D warnings`, boundary check, `cargo test --workspace` (35 suites, 0 failed incl. the new
  fake-daemon test for the published focus), feature configurations, Luau schema, spike unit
  tests, packaging, wallpaper smoke, services, cargo-deny: exit 0.
- Nested smoke (desktop, private bus/labwc/niri, real `nacre-helper daemon
  --notifications=daemon` from Nacre `299463d4`, `spikes/notice-daemon-smoke.py` against the main
  shell): **37/37** (the 35 B0030 checks adapted to the overlay view, plus: a popup shows while
  the launcher is open with the panel slot holding the launcher and no inhibition; a popout
  suppresses popups and the overlay is released). Not covered nested: a second output (the nested
  niri has one), so the focused-output choice is proven by the unit/fake-daemon tests only.
- Relative memory gate (`spikes/b0030-memory.py`, `NOTICES_BOTH=1`): five alternating pairs, same
  host, **CPU tiny-skia**, one nested output, `--bar --core-views --ui-provider --notices` in both
  arms. Control = parent `06526ac` (popups in the panel slot), candidate = B0053. Medians of five
  1 s samples, then over arms; swap 0 B and MemAvailable 12 GiB before and after (`free -h`
  logged); DRM not measured.

| Phase | Control Pss MiB | Candidate Pss MiB | Delta Pss | Delta anon | Delta file | Delta shmem | Gate (+0.5) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| idle | 33.19 | 33.12 | -0.07 | +0.02 | -0.09 | 0 | PASS |
| one popup shown | 34.37 | 34.87 | +0.50 | +0.34 | -0.14 | +0.31 | cost while shown |
| after close (10 s) | 32.55 | 32.39 | -0.16 | -0.06 | -0.10 | 0 | PASS |

  A first candidate (overlay release without the primary-session idle hint a closing panel
  sends) read +1.12 MiB after close, all file-backed (the control sheds 1 MiB of bar pages on
  its first panel close); `release_overlay` now sends the same hint when no view of either slot
  is up, which restores parity. The compositor-topic subscription adds no measurable idle cost.

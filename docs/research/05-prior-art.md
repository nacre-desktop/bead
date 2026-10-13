# 05 — Prior art: what an independent shell must own

Researched 2026-10-13. All repositories below were inspected read-only at the
exact commit/date in [source-pins.json](source-pins.json). Licenses are taken
from manifests/notices, not from the language used. No implementation from these
shells was copied into a Bead crate. The measurement spike's only upstream
protocol example reference is MIT SCTK, with its notice retained.

## Quickshell: Qt does a large fraction of the job

Quickshell's src/core/rootwrapper.cpp and generation.cpp show configuration
generation loading, QML compiler diagnostics, reloadable object/singleton
coordination and handover to the next generation. Failed component loading exits
that path without replacing the current generation. Source inspection also
shows specialized Wayland window/screencopy/lock and service implementations.
This is a useful conceptual model, not code to reuse.
Sources: [core](https://github.com/quickshell-mirror/quickshell/tree/master/src/core),
[Wayland](https://github.com/quickshell-mirror/quickshell/tree/master/src/wayland),
[services](https://github.com/quickshell-mirror/quickshell/tree/master/src/services).

Qt supplies the QML engine, property bindings/signals, scene graph, text/fonts,
controls/layouts, image codecs, multimedia, input integration and much of native
window management. Quickshell adds shell-specific APIs, protocol/lifecycle glue,
services, process/files and configuration reload. Removing Qt means assembling
equivalents for both layers, not merely porting Quickshell's types. The most
relevant lesson is ownership across generations: retained native resources,
service subscriptions and state must not die just because their views do.

Installed comparison is Quickshell 0.3.2; the audited upstream source pin is not
assumed to be exactly that distro build. QML API scope comes from Nacre and the
installed qmltypes. Noctalia/ashell measurements are not substituted for our
matched Quickshell process measurements.

## Noctalia v5: verified rewrite, unverified development cost

The team's [announcement](https://noctalia.dev/blog/announcing-noctalia-v5), dated
2026-04-24, confirms a C++ rewrite to control its event loop/rendering/input.
It reports roughly 300 MB per monitor on the previous stack and about one sixth
in the rewrite. That is the team's claim: no PSS/USS methodology or controlled
same-component result is provided there, so it is not a Bead performance target.
It also identifies broken old plugin compatibility, feature reevaluation and
limited capacity to maintain both versions. Those are real transition costs.

The inspected source VERSION is **5.2.1**, under MIT at that pin.
[ARCHITECTURE.md](https://github.com/noctalia-dev/noctalia/blob/main/ARCHITECTURE.md)
documents explicit update/layout/render phases, independent surface lifecycle,
controls atop scene nodes and renderer abstraction. Rendering/text use existing
native components while Noctalia supplies shell/control/input/service machinery.
The [build manifest](https://github.com/noctalia-dev/noctalia/blob/main/meson.build)
lists Wayland, sdbus-c++, Pango/Cairo, fontconfig/FreeType/HarfBuzz, librsvg,
xkbcommon and additional desktop dependencies. Source main has changed beyond
the April announcement; do not describe it solely from launch-era prose.

The pinned tree contains **1,311 C/C++/Rust source/header files under src/** after
excluding paths explicitly named vendor/third_party. This is an observable size
proxy, not 1,311 newly authored files: generated/test/shared infrastructure can
still be included. It does show a much wider undertaking than “draw a bar”.
No reliable person-hours, team staffing or full-parity timeline was found in the
primary material. Mark those costs **unverified**, not a guessed duration.

Lesson: a purpose-built shell can lower measured overhead, but it must fund the
controls, lifecycle, diagnostics and service work a toolkit supplied. Noctalia's
TOML/theme hot reload is not evidence of arbitrary UI/script transactional reload
with all open panel/editor state retained.

## Other implementations

| Project / inspected source version | What was inspected | Practical lesson for Bead | License boundary |
| --- | --- | --- | --- |
| ashell 0.11.0 | Cargo manifest, src module layout, iced_layershell integration and service dependencies | A Rust bar can reuse iced plus a layer-shell runtime, including CPU/GPU renderers; it still needs its own services/state/config. Smaller bar scope does not demonstrate full shell/pearls/video parity | ashell GPL-3.0-or-later: no code copied. iced itself MIT; layer-shell fork must be audited separately |
| ironbar 0.20.0-pre source | Cargo manifest, modules/clients and IPC command handling | Async client/module separation and typed command responses are useful architectural patterns; GTK supplies controls/text/layout | ironbar MIT, but GTK/native dependencies include LGPL; copying or linking its whole stack would violate Bead's dependency boundary |
| Eww source pin (unreleased source) | crates/eww/src/app.rs and script_var_handler.rs; yuck/scope graph layout | Declarative config, global variable graph and poll/listen worker ownership are relevant; restoring open windows is not automatically preserving editor/scroll state. Watchers and workers need cancellation ownership | Eww MIT at root; GTK supplies much of the UI under separate terms |
| Waybar source pin | meson.build, modules and utility layout | Mature SNI/DBusMenu, icon and device/module coverage shows the work beyond a row of labels; GTK/gtk-layer-shell handles surface and widget behavior | Waybar MIT, with native GTK and optional third-party library licenses. Study rather than transplant |
| cosmic-panel source pin | workspace/package manifest, applet/client toolkit boundaries | Panel/app process separation isolates failure but adds IPC/process/context cost; multiple surfaces/applets complicate lifecycle accounting | panel GPL-3.0-only; do not copy. libcosmic is MPL-2.0, not the same license as cosmic-text |
| cosmic-applets source pin | audio/network/status-area manifests and service structure | Individual service/control features carry substantial policy and daemon recovery logic; applet architecture is not a free “whole desktop shell” | applet source licensing must be per crate/file; no code reused. cosmic-text is independently MIT/Apache and eligible |
| niri 26.4.0 source / installed 26.04 | niri-ipc requests/socket/events, protocol and backend modules | Stable workspace IDs differ from output-relative changing display indices; event stream requires a separate socket; compositor-owned positioning/focus must be respected | niri and niri-ipc GPL-3.0-or-later: no framework code reuse; documented protocol interoperability remains possible |

Primary repositories: [ashell](https://github.com/MalpenZibo/ashell),
[ironbar](https://github.com/JakeStanger/ironbar),
[Eww](https://github.com/elkowar/eww), [Waybar](https://github.com/Alexays/Waybar),
[cosmic-panel](https://github.com/pop-os/cosmic-panel),
[cosmic-applets](https://github.com/pop-os/cosmic-applets),
[niri](https://github.com/niri-wm/niri).
Mutable links are reading aids; source-pins.json is the exact revision ledger.

## What follows from the evidence

1. Use existing renderer, shaper and layout engines. Independent shell projects
   still lean on major libraries; writing these engines would expand the scope
   beyond an experiment.
2. Count services/worker/context allocations and teardown behavior, not just the
   UI tree. A process-separated applet architecture may buy resilience while
   losing the shared-device/font-cache benefit.
3. Favor event-driven state publication. A static surface should sleep; the
   blank 60-second measurements verify that only for the tiny harness, not the
   eventual service-rich shell.
4. Treat binding graphs/reload diagnostics/editor/input semantics as product
   infrastructure. Do not defer them to “remaining panels”.
5. Preserve capability differences. Workspace numbering, native placement,
   drag/release events and capture differ between niri and Hyprland even when
   the rendered shell looks identical.
6. Keep a matched benchmark. Prior-art marketing RAM figures and source-size
   proxies establish questions to test, not a numerical forecast for Bead.

**Verdict: feasible to experiment; full parity unproven.** Prior art validates
the architecture family, not the completion cost or the hot-reload promise.

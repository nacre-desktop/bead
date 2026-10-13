# 01 — Wayland surfaces, input and compositor differences

Research date: 2026-10-13. Published candidates: SCTK 0.21.1 (2026-07-23),
wayland-client 0.31.15 (2026-07-22), wayland-protocols 0.32.13 (2026-06-19),
wlr protocols 0.3.12 (2026-03-31). Exact upstream commits/dates are in
[source-pins.json](source-pins.json); release metadata in
[crate-versions.json](crate-versions.json). Installed test compositors are niri
26.04 (`8ed0da4`) and Hyprland 0.56.2 (`efb5099`). Protocol availability must be
discovered from each running registry, not inferred from compositor branding.

## Toolkit fit

[SCTK source](https://github.com/Smithay/client-toolkit) was read in
src/{compositor,output,registry,seat,shell,shm,session_lock} and examples/{simple_layer,
wgpu}. It manages client-side globals, dispatch, output/seat updates, configure
events, buffers and shell roles. It is not a renderer, widget library, layout
engine or service framework. MIT SCTK and MIT Wayland crates fit Bead. Use one
Wayland connection/event loop and share renderer/font/device contexts across
surfaces. Tokio workers can send bounded messages to the UI thread; they should
not race protocol dispatch or own UI objects.

## Required protocols and acceptance checks

| Protocol | Use and behavior | Cheap proof / failure policy |
| --- | --- | --- |
| wl_compositor / wl_shm / wl_output / wl_seat | Surfaces, CPU buffers, outputs, input; GPU WSI uses the same surface connection | Registry enumeration, first configure, buffer release, disconnect cleanup |
| zwlr_layer_shell_v1 (v5 XML installed) | Bar, wallpaper, drawers, OSD; anchor/margins/exclusive zone/layer/input | One independently named surface per output, correct logical sizes and no accidental work-area reservation |
| xdg_wm_base / xdg_toplevel | Managed detachable pearl windows | Configure/ack, resize, close, floating placement through backend; client cannot choose arbitrary global xdg position |
| ext_session_lock_manager_v1 (v1) | Secure lock ownership and per-output lock surfaces | Fail closed, locked/finished handling, output addition while locked; never treat regular layer surface as lock |
| wp_fractional_scale_manager_v1 (v1) + wp_viewporter (v1) | Fractional rendering without blurry integer upscale | 1.25×/1.5× output, ceil(logical×scale) buffers, viewport logical destination, coordinate conversion |
| wp_cursor_shape_manager_v1 | Named cursors following compositor theme | Seat/serial-valid cursor change; fallback themed bitmap cursor where absent |
| zwp_text_input_manager_v3 | IME/preedit/commit/surrounding text | Non-Latin composing, cursor rectangle, focus transitions, editing/deletion offsets |
| wp_presentation | Presentation timestamps and discard events | Frame pacing using compositor presentation clock; distinguish submission/callback from presentation |
| xdg_activation_v1 | Legitimate focus activation after input/launch | Token lifetime/serial checks; compositor decides whether to focus |
| wl_data_device + primary-selection | Clipboard, drag/drop and text selection | Async transfers, cancellation, MIME negotiation; avoid blocking UI on pipe reads |
| ext-image-copy-capture + image-capture-source, or wlr screencopy | Overview previews / ScreencopyView parity | Advertised protocols + permission + actual capture test; capability unavailable is explicit |

Version labels come from XML inspected in installed wayland-protocols. Bind only
supported versions. The staging `ext-*` protocols can change by version and are
not universal guarantees. The selected published Wayland crate must expose the
required XML; if not, generate from permissively licensed upstream XML with
wayland-scanner, retaining notices and exact source hash.

Primary protocol definitions:
[layer shell](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml),
[xdg shell](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/xdg-shell/xdg-shell.xml),
[session lock](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-session-lock/ext-session-lock-v1.xml),
[fractional scale](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/fractional-scale/fractional-scale-v1.xml),
[viewporter](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/viewporter/viewporter.xml),
[cursor shape](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/cursor-shape/cursor-shape-v1.xml),
[text input v3](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/unstable/text-input/text-input-unstable-v3.xml).
These URLs are specifications, not permission to copy compositor implementation.

## Blur is compositor policy, not a renderer effect

Niri 26.04 documents background effects for ordinary and layer surfaces. An
advertised `ext_background_effect_manager_v1` capability lets clients submit a
surface-local blur region, double-buffered until wl_surface.commit. Rounded
corners require a matching blur shape rather than a bounding rectangle. Handle
capability changes and cleanly remove regions. Algorithm and quality remain the
compositor's choice. See
[niri Window Effects](https://niri-wm.github.io/niri/Window-Effects.html) and
[protocol XML](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/staging/ext-background-effect/ext-background-effect-v1.xml).

Hyprland uses compositor-owned layer/window blur rules; its documented layer rule
matches namespace. A client cannot assume niri's blur request is available or
force background sampling itself. See
[Hyprland layer rules](https://wiki.hypr.land/configuring/core/rules/layer-rules/).
Bead can request advertised effects, otherwise report unavailable or rely on
existing rules. This experiment must not edit real rules to make blur pass.
Future nested fixtures may supply test-only rules. Local rounded corners,
shadows and content blur are separate renderer tasks; “GPU supports blur” does
not prove background blur protocol support.

## Focus and keyboard semantics

Layer-shell has None, Exclusive and OnDemand keyboard modes. Bars/OSD/wallpaper
default to None; launcher/interactive panels request focus deliberately and
release it when closed. Exclusive focus is constrained by compositor policy and
layer; it is not a global keyboard grab. A surface with an empty input region is
click-through; transparent pixels alone do not imply click-through.

Use xkbcommon for the supplied keymap, modifiers, key symbols, compose and repeat.
SCTK's optional xkbcommon integration currently depends on its compatible 0.8
line; do not blindly add 0.9 just because it is latest. Keep keycodes distinct
from Unicode text. IME commit/preedit belongs to a focused text editor; do not
insert raw key text during composition. Shortcut registration is a backend
capability, not a reason to read every keyboard event globally.

Published wayland-protocols-hyprland 1.2.0 is LGPL-3.0-only: excluded from Bead.
Independently generate bindings from the official XML only after verifying each
XML license, or expose a GPL shell adapter. Niri does not have Hyprland's global
shortcut registration semantics; preserve the existing generated-binding/IPC
route and its press-only capability. No live keybinding edits are authorized.

## Multi-monitor lifecycle

Keep output identity separate from a vector index and from a human display name.
Create views only after output properties are available; configure each surface
before attaching a buffer. Hotplug destroys surfaces/render targets but preserves
stable output-associated stores for reattachment, with a fallback policy for a
panel whose output disappears. At scale/transform changes invalidate layout/text
cache keys and map input in logical coordinates. Do not conflate compositor window
coordinates, surface-local coordinates and physical buffer pixels.

A viewport cannot atomically commit several independent Wayland surfaces as one
desktop-wide transaction. Hot reload can atomically change Bead's generation;
different outputs may display it on adjacent refreshes. Test for no mixed state
or partially validated trees, but do not promise same-photon multi-output swaps.

## Test isolation and current proof

Blank renderer spikes use namespace `bead-dev-spike`, no keyboard focus and zero
exclusive zone, inside nested niri on a headless labwc parent. Both have private
D-Bus and XDG config/cache/state/data/runtime directories. No session-mode launch,
global activation import, real shell IPC, output/hardware changes or daily shell
termination. Parent/compositor costs are outside client PSS/CPU and must be
reported separately if the deployment comparison needs them.

Phase 0 proves one blank output only. Both compositor startup/exit, one surface
per output, hotplug, mixed DPI and reconnect are Feature 1/4 acceptance work, not
inferred from the spike. Session lock/capture/IME are **not runtime verified** yet.

## Verdict

**On track for ordinary surfaces; concern for parity.** SCTK covers the basic
mechanics. Pearls/global shortcut release, blur, capture and output-relative
workspace identity need explicit backend capabilities. Secure locking needs its
own gate rather than being bundled into an attractive UI demo.

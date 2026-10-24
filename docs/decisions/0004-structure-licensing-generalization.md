# ADR 0004 — Structure, licensing, and future generalization

Date: 2026-10-21. Status: **Accepted by owner**.

## Context

Bead is the general Rust framework intended as a future alternative to
Quickshell. Nacre is its flagship desktop. Keeping their boundaries explicit
allows Bead and third-party shells to use protocol definitions without taking
on Nacre's GPL-3.0 implementation.

## Decision

1. Bead remains MIT OR Apache-2.0, with permissive-only dependencies. It is the
   general framework. Nacre remains GPL-3.0 and is the flagship desktop built on
   Bead. GPL-only integrations (including niri-ipc, Hyprland bindings,
   mpv/GStreamer, and the daemon) stay on Nacre's side behind neutral adapters.
   Orient remains MIT.
2. `nacre-protocol` is relicensed MIT OR Apache-2.0, covering its schemas,
   generated types, golden transcripts, and protocol documentation. It has its
   own license files and Cargo metadata. The daemon and all other Nacre code
   remain GPL-3.0. Before relicensing, verify every commit touching the
   protocol is by siverteh or their agents. On 2026-10-21, against
   `origin/main` at `84f4656b776f7ffdfe590166af24f8fbcbd5d061`, the file list
   came from that base and the per-file history command was:
   ```sh
   git ls-tree -r --name-only origin/main -- crates/nacre-protocol protocol docs/architecture/protocol.md |
     while IFS= read -r f; do
       git log --follow --format='%H%x09%an <%ae>' origin/main -- "$f"
     done
   ```
   Result: 176 per-file commit records representing 21 unique commits; the only
   author identity was siverteh
   `<92440237+Siverteh@users.noreply.github.com>`. The co-author audit used:
   ```sh
   git ls-tree -r --name-only origin/main -- crates/nacre-protocol protocol docs/architecture/protocol.md |
     while IFS= read -r f; do
       git log --follow --format='%(trailers:key=Co-authored-by)' origin/main -- "$f"
     done | sed '/^$/d' | sort -u
   ```
   It returned no co-author trailers. Stop and list any non-owner identity or
   co-author found by this check.
   Afterwards, Bead's `deny-framework.toml` may allow `bead-*` crates to depend
   on `nacre-protocol` under MIT OR Apache-2.0.
3. Do not split the daemon into another repository now. Revisit only if it
   becomes a general services daemon with outside users. Work Nacre-first:
   prioritize daemon and F4 quality on the owner's machines, then move Bead
   onto always-on Nacre parts (bar, launcher, popouts, dashboard, notifications,
   static wallpaper, lock/session, and packaging). Do not begin generalization
   work now beyond the protocol relicense. Keep Nacre-specific look, sizing,
   colours, Orient coupling, and daemon assumptions out of `bead-*`; keep
   compositor specifics behind adapters. Record discovered framework
   assumptions in the roadmap without fixing them during unrelated work.

## Consequences

The protocol becomes usable by third-party Bead shells without pulling GPL
code into their dependency graph. Nacre retains its GPL boundary and owns the
concrete daemon and compositor integrations. Bead's framework crates remain
permissive and compositor-neutral by design.

## Future milestone: Bead for others

After Bead's core switch on Nacre, plan support for:

- Services without Nacre: a standalone daemon enabled by the MIT protocol, or
  basic built-in Bead services (clock, audio, battery, network, notifications,
  tray, media, and brightness).
- Editor support: generated Luau type definitions from schemas and a
  `luau-lsp` setup for autocomplete and early errors.
- A “Make your first bar” template, getting-started documentation, and a
  generic default theme separate from Nacre's theme.
- More compositor adapters for Sway, river, Labwc, and Wayfire, with bottom
  bars and panels validated.
- A stable, versioned public view API with a deprecation policy.
- Later: plugins with explicit permissions, IME and screen-reader support,
  and published packages.

## Generalization list (framework pieces added by feature work)

- B0103: `preview_key`/`preview_value` and `ui.preview_value` supply transient numeric control feedback;
  `SceneSource::wakeup_at` expires it without idle polling; Session scopes pointer focus, range capture and keyboard steps.
  `Session::with_local_feedback` lets hosts stage presentation-only input without service projection; model wakes keep it.
  `Desktop::queue_volume` admits levels without blocking UI input; snapshots confirm the result. Neither names a shell, transport or service.

- B0102: `ui.Arc` paints a bounded, signed-angle stroke directly on CPU/GL; shell markup supplies progress, geometry, colour and caps.

- B0096: `edge_hover` lets an interactive region opt into edge hover; `ui.tint` composes palette/RGBA colours;
  SVG `@base|overlay|amount@` tokens blend palette/literal colours. Artwork and shell behavior stay in the bundle.
- B0097: combining `reveal` and `slide_distance` slides retained content behind a growing contour; the stationary edge join and reusable rasters are independent of shell policy.

- B0090: `bead_wayland::CaptureHub` / `Options::capture` (leased, rate-limited toplevel capture over
  ext-image-copy-capture with foreign-toplevel sources; thumbnails as small files the renderer already loads by path;
  capability detection, no product ids). Which windows to lease, the interval and the bound are the application's
  (`nacre-shell/src/extras.rs`); the tile is Luau.

- B0080: `ui.memo(key, inputs, render)` is a generation-owned, byte-bounded pure markup cache
  with automatic store-read invalidation; the engine has no product or component IDs.

- B0079: `focus_ring` lets a view suppress the default focus outline while retaining input focus.

- B0076: optional `bundle.json` lists root-relative shared Luau modules in a view’s validated reload snapshot;
  no sibling views are loaded implicitly. Optional `kit.SliderTile.on_detail` opens caller-owned details without changing its level or toggle action.

- B0072: opt-in `Button.activate_on_release` with pointer/model/reload cancellation and `focus_on_pointer`,
  and `kit.Paragraph.advance` for a theme's average glyph width; neither names a shell or service.
- B0074: `RevealArt` callbacks return content-sized SVG bounds as well as bytes; the scene keeps its layout bounds and restores settled artwork. Reveal clocks start at admission; contour generation stays in the shell adapter.

- B0043: `bead_ui::transcript` (message list layout, windowing, bounded caches),
  `bead_ui::markdown` (block subset), `kit.Transcript`, `kit.Composer`, and
  `TextInput multiline` (Shift+Enter, wrapped caret, vertical moves). Chat-agnostic;
  the `ai.chat` contract lives only in `nacre-shell/src/chat.rs`.
- B0050: `TextInput masked` (bullets, no accessibility value, IME password purpose, no copy),
  `kit.PasswordField`, `kit.ConfirmButton`, and the `*.secret` store convention (never traced or saved).
  The Wi-Fi flow lives only in `nacre-shell/src/status_views/join.rs`.
- B0055: `kit.ActionButton` (text button for forms and prompts, shared by the Wi-Fi and Bluetooth pages).
  The Bluetooth flow lives only in `nacre-shell/src/status_views/pairing.rs`.

- B0063: `kit.InfoRow`, `kit.ChipsRow`, `kit.DeviceRow` (icon, title, detail, buttons, optional slider line), `kit.PickerList`
  (filter field plus entries, the zone picker) and `kit.SecretRow`; per-option `enabled` in `kit.ChoiceRow`. The registry engine
  evaluates `visibleWhen`/`enabledWhen` against a flag map; which flags exist and the topics and methods behind each custom
  section stay in `nacre-shell/src/settings/pages.rs`.

- B0094: `kit.Select`/`kit.SelectRow` (drop-down over plain option lists; open state, scroll and Esc are the caller's, the
  Settings adapter keeps them in `settings.select*`), `kit.Fill` (two-colour bar from palette roles; steps of alpha, no renderer
  gradient) and `bead_ui::text_style::gradients()` (the host's solid-colour choice, read into `tokens.gradients` while a view is built).

- B0048: `bead_core::pearl` (magnet/landing/place geometry, drag state machine, layout store) and
  `bead_wayland::pearls` (detach/dock any panel session into a floating surface, guide slots). Edge, insets,
  tunings and the allowed views come from the application; the film/landing/border art is Nacre-specific in
  `nacre-shell/src/pearls.rs` and is not yet a replaceable kit component (B0054 cut the border into small pieces
  for memory; it is still Rust-generated SVG).

- B0052: `bead_wayland::Options::app` (on-demand app process: no primary surface, opens one window or the default
  panel at start with its arguments, ends when it closes). Product-agnostic; the launch, single-instance and
  deep-link routing (`nacre-shell/src/app.rs`, `Host::route`) is the template for third-party apps: add a name
  to `APPS`, a branch in `app::run` and a route. The framework crate has no app names.

- B0053: `bead_wayland::Overlays` / `Options::overlays` (a second view slot offered by name via
  `CompanionOffer`, beside the panel and window slots) and `notice_service::Options::coexist`
  (view families that do not suppress popups). Nacre's view names stay in `nacre-shell/src/main.rs`.

- B0060: `kit.ColorSample`/`ColorSampleRow`, `kit.ColorTarget`, `kit.PreviewCard` (a miniature desktop from a role-to-RGBA
  map), `kit.ImagePick` (image slot with a hit grid), `kit.TabStrip`, `kit.ChipRow`, `kit.Paragraph` (wrapped text without a
  text-measurement primitive: an Image-fit/measure primitive in `bead-render` would replace the approximations). Slot-sized
  image attach (`@icon:colors-*` rectangles decide the PNG) is Nacre-side in `nacre-shell/src/colors.rs`; the colours
  methods live only there.

- B0059: a window's view can retire itself (`CompanionVisibility(false)` in the window slot, as in the panel slot), so a
  Done or Skip button ends a window and, in app mode, its process. The tour pages, tiles, help and links are Luau/catalog
  data; `nacre-shell/src/welcome.rs` only maps closed vocabularies (routes, links, settings pages) and projects the window
  size, availability and the daemon's `learn.shortcuts` rows. Text blocks are sized by a Luau estimate (`textH`) until a
  text-measurement primitive exists.

- B0068: `bead_wayland::PaletteFeed` (`Options::palette_feed`): a live presentation palette for a process without a
  wallpaper consumer (an app process); the Nacre adapter (`wallpaper::Feed`) reads the daemon's `wallpaper.active` topic.
- B0066/B0067: panel motion is `edge`, `duration`, `curve`, `slide_distance`, `fade`, `reveal` (+ the runtime `progress`): a
  revealing panel stays put and grows out of its edge (`Scene::reveal`: its fill and shadow shrink with the progress, rounded on
  the growing edge; the content is clipped to the revealed window). Product-agnostic; which panel reveals is Luau/Rust spec data.
  `ui.Column{radius,fill,...}` section cards (`kit.Card`) and the nested-rows projection (`rows` inside a settings `section`) stay
  in `nacre-shell/src/settings.rs`; the pointer tool `spikes/virtual-pointer.c` is harness-only.

- B0071: `bead_render::RevealArt` supplies progress-aware vector artwork for an existing motion body;
  the renderer retains one current SVG and restores the open-state artwork at completion. Frame contour
  geometry, palette and attachment policy remain in Nacre's `placement::Chrome`, outside the framework.

- B0073: text `font_weight` (1–1000, default 400) flows to the neutral renderer and its cache key; the shell chooses the weights.

- B0077: `Session::action_values(name, argument)` evaluates a declared Luau action for host key bindings; the host stages its values through the existing validate-then-commit update path. No product key names enter the framework. The reusable Luau `shell/kit/virtual-list.luau` accepts model/scroll/row metrics/render callback; it uses bounded spacers and automatic extents for long lists, with no product IDs. Its module is imported only by views that use it.

- B0098: measured `kit.PillButton` labels (`Meter::width`), source-controlled outside
  dismissal/wheel distance, external lip targets and output-aware window sizing.
  All are neutral hooks; Nacre routes and Settings dimensions stay at the edge.

## Housekeeping (later)

- Move the GPL `nacre-shell` crate and `shell/` assets from Bead to Nacre at a
  natural boundary, not mid-feature, so Bead's repository is purely permissive.
- Before accepting outside contributions, require DCO sign-off in CONTRIBUTING
  for Bead, Nacre, and Orient.
- Before publishing, choose a distinctive main crate name: `bead` and
  `bead-core` are taken by an abandoned 2021 container runtime; `bead-ui`,
  `bead-render`, `bead-wayland`, `bead-services`, `bead-test`, and
  `bead-backends` are available. Present the project as “Bead, the Nacre shell
  framework” to distinguish it from “Beads,” an AI issue tracker. Publishing
  remains an owner decision.

## Reference project

[Pleamar](https://github.com/k4ditano/pleamar) is a similar Rust + Luau
Quickshell alternative under BSD-3-Clause. Evaluate its ideas at the right
time, not now: animation on a separate render thread for smoothness during Luau
stalls (including the current launcher smoothness work if it fits), language
server/editor integration, and plugins with declared user-approved
permissions. Protect Bead's differentiators: about 10 MiB of unreclaimable UI memory (anon + shmem + DRM + SwapPss; B0081, formerly "about 25 MiB PSS"),
services outside views for crash safety, state-preserving live reload, and a
real flagship desktop.

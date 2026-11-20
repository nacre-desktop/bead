# Framework specifications

This index contains current framework contracts and repository-boundary decisions. Historical Nacre product specifications and evidence live in [Nacre's bead-shell/docs](https://github.com/nacre-desktop/nacre/tree/main/bead-shell/docs).

| Specification | Scope |
|---|---|
| [0025 — Per-output UI sessions](0025-per-output-ui-sessions.md) | Host-owned output identity and independent UI sessions |
| [0032 — Internationalization consumer](0032-i18n-consumer.md) | Application-provided translation catalogs |
| [0035 — Settings stores](0035-settings-stores.md) | Stable-ID state and host-backed persistence |
| [0045 — Settings window framework](0045-settings-window-framework.md) | Reusable window/session framework boundary |
| [0135 — Scrolling](0135-scrolling.md) | Bounded lists, input routing, and scroll state |
| [0186 — Markup budget](0186-markup-budget.md) | Markup execution limits and recovery behavior |
| [0203 — Plugins v1](0203-plugins-v1.md) | Plugin format, capabilities, and lifecycle |
| [0214 — Framework split](0214-framework-split.md) | Ownership boundary and migration completion |
| [0218 — Compositor-driven layer motion](0218-layer-motion.md) | Optional grouped atlas-plane animation for layer surfaces; `scale_from` as a destination-size transform (B0324) |
| [0219 — Native layer promotion](0219-native-layer-promotion.md) | Optional compositor-owned floating placement for the existing layer surface |
| [0232 — GraphCanvas feasibility](0232-graph-canvas.md) | Draft: renderer/layout measurements and general canvas architecture |
| [0234 — General folder host](0234-general-host.md) | Run and CPU-check Luau application folders with the `bead` CLI |
| [0235 — Folder app services](0235-folder-app-services.md) | Opt-in desktop snapshots, actions, and declared commands for folder apps |
| [0236 — Optional StatusNotifierWatcher](0236-tray-watcher.md) | Optional session-bus watcher ownership and registration lifecycle |
| [0238 — API stability and versioning](0238-api-stability.md) | Proposed stable surfaces, compatibility rules, hazards, and publishing readiness |
| [0239 — MPRIS media service](0239-mpris.md) | Event-driven media snapshots and controls for folder apps |
| [0244 — Power profiles service](0244-power-profiles.md) | Event-driven system power profile snapshots and controls for folder apps |
| [0245 — Shell interaction CPU](0245-interaction-cpu.md) | Interaction CPU attribution and renderer/power trade-offs; Phase 3 end-to-end proxy (B0272: named threads, demand-driven seconds lease; prepare reuse open) |
| [0246 — Bluetooth devices service](0246-bluetooth.md) | Event-driven BlueZ adapter and device snapshots and controls for folder apps |
| [0247 — Network devices service](0247-network.md) | Event-driven NetworkManager device, access point and saved-connection snapshots and controls for folder apps |
| [0249 — Compositor adapters](0249-compositor-adapters.md) | Clean-room niri and Hyprland IPC snapshots and folder workspace focus |
| [0250 — Popup panels for folder apps](0250-folder-app-panels.md) | Manifest-declared Luau popups a bar opens with `panel.*` actions; a panel whose worker is dead (factory `Err` or panic) is torn down at once with its dismissal layer (`SceneSource::alive`, `PANEL_FAILED`, B0345) |
| [0254 — Render and worker hot paths](0254-hotpath-render.md) | Scene handoff, action isolation, and render preparation costs; worker queue backpressure (B0332: the newest palette wins, key repeats are dropped while one is queued) an image policy that follows the companion renderer (B0336), and queued resource jobs that run before the ack wait (B0337) |
| [0264 — Tinted icons and badge](0264-tinted-icons-and-badge.md) | Palette-tinted monochrome images and the kit count badge |
| [0274 — Kit batch](0274-kit-batch.md) | `kit.Segment`, `LinkRow`, `Chip`, `Tag`, the `SliderTile` end slot, `kit.metrics`, honoured close durations, transcript alignment |
| [0276 — Kit batch 2](0276-kit-batch2.md) | `kit.EmptyState`, `kit.NoticeRow`, `radius` on `ui.Image`, chip/button label centring |
| [0262 — Non-exhaustive host types](0262-non-exhaustive.md) | `#[non_exhaustive]` service config/snapshots, `Options` and `WindowSpec::new`; host construction |
| [0263 — Motion tokens and panel retarget](0263-motion-tokens-and-retarget.md) | Named curves and durations, sampled `ui.animated` markers, and a panel body that regrows mid-animation |
| [0270 — Panel first frame](0270-panel-first-frame.md) | Cold and reopened panel-open latency: per-axis shadow templates; a retained GL panel keeps its renderer, image textures and context across close and reopens on GL from frame 1 (B0334); at close it keeps only the last frame's textures, and after 300 s closed it releases to the CPU snapshot path (B0339); the cold-open prewarm also runs one off-thread warm-up draw, and the first GL frame is split into `gl_canvas_new`, `gl_image_upload` and `gl_flush` spans (B0342) |
| [0271 — Idle memory ownership](0271-idle-memory.md) | Idle PSS and heap ownership of a Bead process (Luau VMs, keep-alive panels, a11y threads, arenas); fixes landed; GL textures charge a per-renderer 64 MiB native-gl-images budget (B0338); the implicit system font database is loaded once per process (B0350); a closed panel's data update stages without a bake (B0351); a closed panel's palette and data updates stage without a bake, also when never opened; empty-Model animation frames still bake so a closing animation runs (B0359); the small image raster cache is 3 MiB (was 1.5 MiB) so two output scales' warmed icons fit (B0354, coordinator-approved inside the owner's Strand envelope) |
| [0263 — Reveal close (B0277)](0263-motion-tokens-and-retarget.md#reveal-close-b0277) | A stationary reveal closes as the open reversed (shrinks into its anchor edge) on the CPU and compositor paths |
| [0275 — Clock resync](0275-clock-resync.md) | The clock republishes at once after resume, wall-clock steps and timezone switches without the system bus (Linux timerfd + `/etc/localtime` watch) |
| [0263 — Slide pacing and terminal pose (B0283)](0263-motion-tokens-and-retarget.md#slide-pacing-and-terminal-pose-b0283) | A CPU motion commits its terminal pose on the first frame callback after its end (no unrelated redraw needed); start pacing findings |
| [0263 — Desktop pass (B0273)](0263-motion-tokens-and-retarget.md#desktop-pass-b0273) | Real-GPU 120 Hz frame gaps per phase: client vs compositor reveal under nested niri and labwc; B0277 close strips; lip tab arm; B0267 held |
| [0263 — Wallpaper crossfade (B0313)](0263-motion-tokens-and-retarget.md#wallpaper-crossfade-b0313) | GPU fade or grow of a wallpaper change at display rate with a frame-rate palette tween (B0327, also for palette-only frames, adopted mid-fade without a restart when they match its target, B0341); GL held only for the change, or resident and primed while the host asks for warm mode (B0317, B0325); `palette_tween = false` panels and closed retained panels take only a tween's final palette (B0333); decoration tiles re-raster once per palette tween, at its end (B0362) |
| [0278 — Lip open continuity](0278-lip-open-continuity.md) | An edge lip holds its completed-dwell look from fire until its panel is active instead of flashing to the rest bar |
| [0280 — Kit batch 3](0280-kit-batch3.md) | Paint-only interaction poses (`visible_on_hover`, `press_scale`/`pressed_fill`, `reflow`), `shadow` on any surface and its parameters, `prewarm_shadows`, key repeat schedules, image `opacity` and `bake_size`, `font_features`, `scroll_steps` |
| [0284 — Mirror validates once](0284-mirror-validate-once.md) | A mirrored snapshot is validated once per patch; `Publisher::publish_mirror` republishes it; one compare per publication |
| [0281 — Cleanup and bloat pass](0281-cleanup-pass.md) | One snapshot hub for the event services, unused probes and dependency, stale spec text; deferred cleanup by owner |
| [0285 — UI package](0285-ui-package.md) | `UiPackage` reads a UI root once; `FolderView::from_package` builds more views from memory (a lock screen survives a package upgrade) |
| [0279 — Lip as its panel's tab](0279-lip-tab.md) | The edge lip rides its panel's inner edge as a subsurface posed with the panel each frame; 150 ms exit grace (panel profile), press/key pin; hover tooltip `Options::edge_tip`, a host-drawn Scene placed beside the lip after 400 ms (B0340) |
| [0279 addendum — top tab (B0291)](0279-lip-tab.md) | Bar cue off while the tab is attached; top-tab rect tests at 50 px; detach before every unmap |
| [0261 — Docs and tutorials pass](0261-docs-and-tutorials.md) | Reference pages and tutorials match the code; every snippet and tutorial step executed |

This index covers Bead framework contracts. See the [documentation index](../README.md) for the current framework overview and the [framework ownership decision](0214-framework-split.md).

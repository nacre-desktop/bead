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
| [0218 — Compositor-driven layer motion](0218-layer-motion.md) | Optional grouped atlas-plane animation for layer surfaces |
| [0219 — Native layer promotion](0219-native-layer-promotion.md) | Optional compositor-owned floating placement for the existing layer surface |
| [0232 — GraphCanvas feasibility](0232-graph-canvas.md) | Draft: renderer/layout measurements and general canvas architecture |
| [0234 — General folder host](0234-general-host.md) | Run and CPU-check Luau application folders with the `bead` CLI |
| [0235 — Folder app services](0235-folder-app-services.md) | Opt-in desktop snapshots, actions, and declared commands for folder apps |
| [0236 — Optional StatusNotifierWatcher](0236-tray-watcher.md) | Optional session-bus watcher ownership and registration lifecycle |
| [0238 — API stability and versioning](0238-api-stability.md) | Proposed stable surfaces, compatibility rules, hazards, and publishing readiness |
| [0239 — MPRIS media service](0239-mpris.md) | Event-driven media snapshots and controls for folder apps |
| [0244 — Power profiles service](0244-power-profiles.md) | Event-driven system power profile snapshots and controls for folder apps |
| [0245 — Shell interaction CPU](0245-interaction-cpu.md) | Interaction CPU attribution and renderer/power trade-offs |
| [0246 — Bluetooth devices service](0246-bluetooth.md) | Event-driven BlueZ adapter and device snapshots and controls for folder apps |
| [0247 — Network devices service](0247-network.md) | Event-driven NetworkManager device, access point and saved-connection snapshots and controls for folder apps |
| [0249 — Compositor adapters](0249-compositor-adapters.md) | Clean-room niri and Hyprland IPC snapshots and folder workspace focus |
| [0250 — Popup panels for folder apps](0250-folder-app-panels.md) | Manifest-declared Luau popups a bar opens with `panel.*` actions |
| [0254 — Render and worker hot paths](0254-hotpath-render.md) | Scene handoff, action isolation, and render preparation costs |
| [0264 — Tinted icons and badge](0264-tinted-icons-and-badge.md) | Palette-tinted monochrome images and the kit count badge |
| [0274 — Kit batch](0274-kit-batch.md) | `kit.Segment`, `LinkRow`, `Chip`, `Tag`, the `SliderTile` end slot, `kit.metrics`, honoured close durations, transcript alignment |
| [0276 — Kit batch 2](0276-kit-batch2.md) | `kit.EmptyState`, `kit.NoticeRow`, `radius` on `ui.Image`, chip/button label centring |
| [0262 — Non-exhaustive host types](0262-non-exhaustive.md) | `#[non_exhaustive]` service config/snapshots, `Options` and `WindowSpec::new`; host construction and migration |
| [0263 — Motion tokens and panel retarget](0263-motion-tokens-and-retarget.md) | Named curves and durations, sampled `ui.animated` markers, and a panel body that regrows mid-animation |
| [0270 — Panel first frame](0270-panel-first-frame.md) | Cold and reopened panel-open latency: per-axis shadow templates (the GL context lifetime is pending a desktop gate) |
| [0271 — Idle memory ownership](0271-idle-memory.md) | Idle PSS and heap ownership of a Bead process (Luau VMs, keep-alive panels, a11y threads, arenas); proposed fixes |
| [0271 — Idle memory ownership](0271-idle-memory.md) | Idle PSS and heap ownership of a Bead process (Luau VMs, keep-alive panels, a11y threads, arenas); fixes landed |
| [0263 — Reveal close (B0277)](0263-motion-tokens-and-retarget.md#reveal-close-b0277) | A stationary reveal closes as the open reversed (shrinks into its anchor edge) on the CPU and compositor paths |
| [0278 — Lip open continuity](0278-lip-open-continuity.md) | An edge lip holds its completed-dwell look from fire until its panel is active instead of flashing to the rest bar |

This index covers Bead framework contracts. See the [documentation index](../README.md) for the current framework overview and the [framework ownership decision](0214-framework-split.md).

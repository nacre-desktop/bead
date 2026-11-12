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

This index covers Bead framework contracts. See [status](../STATUS.md) and the [framework split decision](0214-framework-split.md) for migration state; step 5 closure checks remain with the lead.

# B0152 — wallpaper widgets and Widgets app

Desktop widgets now use purpose-built wallpaper compositions and the Widgets page previews both placements together. Their colors come from the active palette: `primary` accents, `onSurface` text, and `surfaceContainer` glass. IBM Plex Sans and the shared spacing/radius tokens keep the controls legible and consistent with Nacre.

| Area | Design |
| --- | --- |
| Clock | Glass digital, analog dial, and minimal type; weekday/date and live time. |
| Battery / sound | Large palette ring; sound includes its device label, volume slider, and mute action. |
| Power | Glass action card; destructive actions use the existing confirmation flow. |
| Connectivity | Wi-Fi network/signal card with toggle; Bluetooth connected-device stack with battery and toggle. |
| Notifications / launcher / updates | Recent dismissible notification stack, favorites dock, and update count/check/run card. |
| App | Both forms and style galleries, selected desktop preview outline, physical output label, and Advanced settings. |

Wallpaper-level widget surfaces use the existing canvas layer, which defaults to Wayland `Bottom`; the drag preview alone is explicitly `Overlay`. Frameless clock, battery, sound, Wi-Fi, and update styles keep transparent backgrounds. All dimensions and colors use the shared tokens or the saved widget slot.

Memory budget: at most 2 MiB unreclaimable for the always-on widget system across phases; the active-widget cost is included in that cap. This change has no measured memory result yet. Smoothness targets are 60/120 Hz with no dropped frames; no frame timing result is claimed before the private capture run.

Known source limits: battery contracts currently provide percentage and charging icon but no remaining-time estimate. Network rows provide label and signal; Bluetooth rows provide connected device and optional battery. The output source publishes physical and logical dimensions but no fractional display-scale field to this view; the stage preserves the logical output aspect and labels source-reported dimensions.

Known verification gaps: wallpaper-level visual styling has not yet been paired against QML; the compositor-owned niri focus ring is outside the Bead window surface. Captures and visual review are pending. Evidence will be stored in `docs/specs/evidence/0152/`.

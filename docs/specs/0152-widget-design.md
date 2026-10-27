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

Saved legacy style IDs remain hidden aliases and map into the new desktop forms, so existing layouts continue to load without showing duplicate choices. The wallpaper preview resolves the `presentation.json` poster through the existing icon-thumbnail attachment path.

Capture evidence: B0161 supersedes the rejected B0152 visuals. The 28 B0152 screenshots and contact sheet were deleted in B0161 to remove the rejected presentation; the historical capture-safety result above is retained as a record, not as design acceptance.

Known source limits: battery data has no remaining-time estimate or charging pulse. The current desktop snapshot adapter does not populate Bluetooth devices/battery, Wi-Fi network/signal, notification rows or update details, so those forms render empty/default data until their published contracts supply it. The output source reports physical and logical dimensions but no fractional display-scale field. The selected style is outlined in the preview; niri owns the visible blue focus ring around the app and Bead cannot remove it.

Known verification gaps: paired QML screenshots and hover/pressed/animation-frame comparisons were not captured. The 2 MiB unreclaimable memory budget and 60/120 Hz no-drop target remain unmeasured; no result is claimed.

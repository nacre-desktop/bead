# B0172 — Widgets app usability

The Widgets editor is a standalone “Nacre Widgets” app with a stable widget-list sidebar on its overview and detail pages. Every widget has a translated description, clear add/remove controls, a palette-role choice saved with its instance, and an outline while desktop placement is being edited.

The overview keeps the live widget miniatures inside the existing reference-sized preview cells. The detail page keeps the wallpaper and bar stage, with direct placement buttons beside the shared kit switches.

General primitive: WindowSpec.on_resize reports committed logical size changes; each product owns its persistence policy.

| Area | Evidence |
| --- | --- |
| App identity and size | Window title and app ID are Nacre Widgets / nacre-widgets; default 1100×800 logical size, output-capped to 70%, last size stored in Nacre state. |
| Sidebar and descriptions | One widget list stays visible across overview and detail; all twelve descriptions have English i18n entries. |
| Palette roles | Per-instance Colour choices map the widget’s existing accent role to Primary, Secondary, Tertiary or Neutral for bar and desktop forms. |
| Settings cleanup | 35 Settings goldens were refreshed after removing the Widgets shortcut; the freed sidebar space now shows the Devices heading and Displays row. |
| Drag/edit | The prior full-stage `stage-drag-source` was appended above both preview regions, so its transparent hit area intercepted drags and used the selected form's payload for every point on the stage. The fix gives the bar preview and desktop miniature separate drag sources. Nested tracing also found that fallback output geometry named no output; app drops were rejected by the terminal-drop guard. The fallback now uses the active output ID. |
| Toggle | The shared kit switch clips the full button hit background to the rounded track and removes the differently colored track stroke. Sixteen switch-bearing Settings goldens were refreshed for this shared-kit change. |
| Visual evidence | docs/specs/evidence/0172/ — 2880×1800, scale-1.5 nested captures, 4× switch crop, input traces, and reference comparison notes. |
| Validation | Full Bead CI and real-input persistence checks are recorded after execution. |

The owner-approved `.lead/reference/App.dc.html` is the visual reference. The app follows its stable widget sidebar, compact descriptions, large wallpaper stage, paired form controls and three-column style previews; the Bead app adds the four-role color selector and accessible add/remove buttons requested by the owner. No matching QML Widgets editor exists.

## B0176 — visual review fixes

The widget-list icons now match Settings sizing and center against the complete two-line row; descriptions fit without truncation in [app-home.png](evidence/0172/b0176/app-home.png) and [choose-list.png](evidence/0172/b0176/choose-list.png). The stage now uses the output's logical 1920×1200 dimensions and centers that aspect in [battery-page.png](evidence/0172/b0176/battery-page.png). Clock content has room inside its outline, compact 120px widgets fit their text, and the analog and minimal miniatures use simplified, legible forms; [style-tiles.png](evidence/0172/b0176/style-tiles.png) shows each inside its cell. The settled bottom-right edit capture shows the clock within its outline and no separate guide rectangle: [desktop-edit-mode.png](evidence/0172/b0176/desktop-edit-mode.png). The switch hover and 4× detail are recaptured in [battery-toggle-hover.png](evidence/0172/b0176/battery-toggle-hover.png) and [toggle-close-up-4x.png](evidence/0172/b0176/toggle-close-up-4x.png).

All seven captures are private nested-session screenshots at 2880×1800 physical pixels, scale 1.5, using Bead's CPU renderer. No goldens were changed. Capture metadata and state notes are in [the B0176 evidence README](evidence/0172/b0176/README.md).

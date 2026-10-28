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

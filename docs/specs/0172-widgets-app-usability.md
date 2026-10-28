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
| Drag/edit | Parent relay and floating-window behavior are checked in the nested session; desktop widgets show an outline and accept move drags while the app is open. |
| Toggle | Shared kit switch hit background uses the track’s rounded ends, fixing the colored corner rim for all Bead switches. |
| Visual evidence | docs/specs/evidence/0172/ — paired 2880×1800, scale-1.5 nested captures and comparisons to the owner-approved App design. |
| Validation | Full Bead CI and real-input persistence checks are recorded after execution. |

No matching QML Widgets editor exists, so QML surface parity is not applicable. The owner-approved design boards remain the visual reference.

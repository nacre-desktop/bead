# B0176 visual review evidence

These captures use a private labwc parent and nested Niri at 2880×1800 physical pixels and scale 1.5. The selected output is 1920×1200 logical; Bead uses its CPU renderer and a generated wallpaper fixture. The existing B0172 images and goldens remain unchanged.

B0182 recaptured `battery-page.png` and `desktop-edit-mode.png` with the same geometry and renderer. The other B0176 images remain unchanged.

| Image | Review point |
| --- | --- |
| `app-home.png` | Settings-sized sidebar icons, centered row alignment and shortened descriptions. |
| `choose-list.png` | Descriptions remain whole when the sidebar is filtered. |
| `battery-page.png` | Output caption, centered 16:10 stage, and clock content inside its edit outline. |
| `style-tiles.png` | Time + date fits its bar tile; analog and minimal desktop miniatures are legible in 120×64 logical previews. |
| `desktop-edit-mode.png` | Clock at the lower-right desktop position after an app-origin drop; text fits inside the edit outline and no separate guide rectangle remains after settling. |
| `battery-toggle-hover.png` | The Battery placement switch under pointer hover. |
| `toggle-close-up-4x.png` | A 4× crop of the same rounded switch track. |

`capture-safety-bead.json` and the private per-state JSON/log files record the nested run, output geometry, UI state and accepted drag. They are kept in the task's temporary desktop directory, not committed.

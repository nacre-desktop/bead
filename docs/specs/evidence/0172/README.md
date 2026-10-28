# B0172 visual and input evidence

All captures use a private headless labwc parent and nested Niri at 2880×1800 physical pixels, scale 1.5, with Bead's CPU renderer. The app runs as `nacre-shell --app widgets` through the parent command socket; the private Niri rule models the N0090 `nacre-widgets` floating-app rule. A private generated wallpaper fixture supplies the app's preview stage. The test host has no `swaybg`, so the area outside the app uses Niri's solid background.

## Screenshots

| File | State |
| --- | --- |
| `app-home.png` | Widgets overview and stable widget-list sidebar |
| `choose-list.png` | Search-filtered widget list |
| `battery-page.png` | Battery page; same sidebar as home |
| `style-tiles.png` | Clock style miniatures after scrolling |
| `battery-toggle-hover.png` | Hover on the bar switch |
| `toggle-close-up-4x.png` | 4× crop of the checked switch, with the extra outline removed |
| `colour-primary.png` … `colour-neutral.png` | Battery instance using each palette role |
| `add-to-bar.png`, `add-to-desktop.png`, `remove-from-bar.png`, `remove-from-desktop.png` | Each placement button after a synthetic pointer click |
| `drag-in-progress.png` | App-origin drag while the pointer is moving |
| `desktop-edit-mode.png` | Clock placed on the desktop with its edit outline while the app is open |
| `app-to-bar.png` | Power preview dropped onto the bar |
| `desktop-reopened.png`, `bar-reopened.png` | App reopened after each drop; canonical layout hashes match |
| `acceptance.json` | Sanitized parent-socket launch, pointer-input, placement-button, role-store, and persistence proof |

## Reference comparison

Compared with `.lead/reference/App.dc.html`, the Bead detail page keeps the reference's left widget list, title and description, large wallpaper/bar stage, paired bar/desktop controls, and three-column style previews. The home page adds the requested overview and descriptions under each list item. Each screenshot shows the desktop widget list and app content at the requested output size. The four role captures show the Battery preview changing from the primary pink to the palette's secondary green, tertiary, and neutral colours. The final 4× switch crop has no outline-colored track rim.

The original drag bug was a full-stage transparent `stage-drag-source` appended after the bar and desktop previews. Its hit area covered both previews and its payload followed the selected form, so dragging could not begin from the intended preview. The app now has independent `stage-bar` and `desktop-widget-preview` sources. The nested trace records 202 pointer motions per drop, a parent-relayed desktop/bar destination, and equal saved/reopened layout hashes in `acceptance.json`.

The nested test also exposed an output-ID fallback: when the display list had an unnamed record, the desktop drop event had an empty output ID and the terminal-drop guard rejected it. The fallback now uses the active parent/app output ID. The regression tests and full-CI result are summarized in `acceptance.json`.

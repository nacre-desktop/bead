# Sanitized final pair evidence

Header PNGs crop 260×110 logical pixels around each launcher's upper-left contour/header at 1.5× scale. They omit the top bar and central/right provider content. `pair-summary.json` contains only geometry, API outcomes, sparse endpoint RGB comparisons, and the Bead layer-margin trace.

The QML redocked endpoint is retained at its expected docked crop, where the static sample found no panel pixels. The pair did not produce a visible QML return, so it is not counted as a successful visual redock.

Additional crops: `*-idle.png` show the expected initial top-left ROI while idle; `*-drag-held.png` use the latest ready frame strictly before exact pointer release; `*-open-bottom.png`, `*-detached-bottom.png`, and `*-redocked-bottom.png` show the bottom-left contour/search-field area. All remain 260×110 logical pixels and omit the right provider area and top bar.

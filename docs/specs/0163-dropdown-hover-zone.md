# B0163 — bar dropdown keep-open zone

Click-opened status dropdowns remain open while the pointer crosses from the popup into the bar, including gaps between icons and neighboring icons. The shared `StatusPill` controller records click-open ownership and derives a keep-open zone from the visible dropdown width, the bar band, and the owner widget slot, with 4 px tolerance. Pointer exit does not arm its hover close timer; clicking the owner toggles closed, clicking another icon switches the panel, and Escape or the existing outside-dismiss event closes it. A bar click outside that zone dismisses the open dropdown.

Implementation is shared in `bead-ui`'s `StatusPill`, so battery, Bluetooth, Wi-Fi, sound, clock/calendar, notifications, power and updates use one policy. Neighbor hover remains preview-suppressed while a panel is clicked open. No temporary duplicate code or assets were introduced.

Validation: controller tests cover zone geometry, inside/outside bar clicks, pointer exit, neighboring hover, same-widget toggle, switch, and Escape. Nested desktop virtual-pointer capture and full CI results are recorded in the task report. This input-only change has no intended visual differences.

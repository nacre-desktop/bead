# B0103 paired screenshots

Each JPEG places QML on the left and the final Bead 7ac feedback build on the right. Both source screenshots were captured at 2880×1800, scale 1.5, using the same fixture wallpaper and palette. The pair crop keeps the right-side panel and surrounding bar context, with a shared viewport crop and a small label row. Every paired image was rebuilt from separate QML and Bead source directories; all 18 source-image SHA-256 hashes were checked against their capture files and the copied inputs before montage generation.

Files cover idle, control centre open, hover, pressed, audio, battery, network, and Bluetooth, plus the settled volume slider. Battery and Bluetooth detail look closely matched. The open control-centre layout and hover highlight are broadly similar.

The captures expose several state mismatches that prevent a clean parity claim:

- Bead dims Stay awake, Do Not Disturb, the appearance selector, and Display settings while QML shows them enabled. The top-bar update count, clock, and Wi-Fi indicator also differ, so the backing states are not identical.
- QML's pressed capture remains on the control-centre home page; Bead's capture has opened Bluetooth details.
- QML's audio page includes a QuickShell Peak Detect stream at 0%; Bead's page omits it.
- QML's network page has no Hidden network and Saved networks buttons, while Bead shows both.
- The final settled volume screenshots show QML at 88% and Bead at 85%, both without an OSD. The thumb positions differ accordingly.
- In the hover pair, QML gives the Wi-Fi and Bluetooth cards a brighter thin edge/glow. Bead's hover fill is similar, but the edge accent is weaker. The Bluetooth label/subtitle group also sits slightly lower relative to its icon in QML.

Idle wallpaper and the broad panel structure match closely. The displayed bar state differs: update count, Wi-Fi indicator, and clock differ between arms. Bluetooth detail contents and battery page are close; the pressed-state pair is intentionally mismatched (QML remains on the home page while Bead is in Bluetooth details), so it is not a same-state visual comparison.

The QML arm has no UI_POINTER, UI_TIMELINE, INPUT_PROFILE, UI_STATE, UI_COMMIT, or UI_PRESENTED records. Its screenshot sequences sample animation frames about every 25 ms, but do not tie pointer injection to a presented visual update.

Pairs: [idle](idle-pair.jpg), [open](control-center-pair.jpg), [hover](control-hover-pair.jpg),
[hover detail](control-hover-detail-pair.jpg), [pressed](control-pressed-pair.jpg),
[audio](control-audio-pair.jpg), [battery](control-battery-pair.jpg),
[network](control-network-pair.jpg), [Bluetooth](control-bluetooth-pair.jpg),
and [settled slider](slider-settled-pair.jpg).

[Open/close samples](animation-contact.jpg) show the same right-edge direction, with a more visible
Bead fade and different observed completion times. These are approximate screenshot timings, not presentation feedback.

[Pointer outline before/after](slider-outline-before-after.jpg) uses matching 85% Bead rows from the control and final candidate.

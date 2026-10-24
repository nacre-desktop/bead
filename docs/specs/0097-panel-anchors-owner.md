# B0097 — Panel anchoring and motion

Reference: Nacre `3cc38598` (Orient `3efe8d1`); control Bead `417ac32` (including B0096/B0102/B0103), with the same notification-count initialization fix applied to both arms.
Scope: launcher bottom join, right control drawer, status anchors and hover previews. Both arms include the published B0102 companion-state isolation guard; it changes no panel geometry.
Bar artwork, tray, AI/Settings and wallpaper belong to separate units.

## Causes and changes

- Sliding the launcher contour moved its attachment above the bottom frame. The contour now reveals in place while content slides behind it, using QML's 300 ms cubic ease-out, without fading. Prepared frames preserve raster ownership and translate content exactly once. Pointer events inverse-transform the current content slide after visual-bound dismissal checks, keeping moving controls aligned with their hit regions. Query/typing logic is unchanged.
- The right lip opened `sound`, a top popout, instead of `control`. It now opens the right drawer. The system pill background also opens `control`; the command/keybinding route selects the same view. The drawer reveals a fixed-width 424 px body from the right over 280 ms cubic ease-out, without fading.
- Clicked status routes discarded the invoking icon centre. Click and hover now share that centre; battery and notification counts participate in their icon's hit area/centre. Sound, Wi-Fi, Bluetooth, battery, notification history and calendar use their own top attachment. Notification history reuses the existing service/projection rather than introducing another provider.
- Status previews used a fixed 190×72 card with missing detail/device rows. They now use 280 px content, 15 px insets, content height, QML's 230 ms cubic ease-in-out reveal, 350 ms dwell and 120 ms exit grace. Calendar hover uses a non-focus-taking calendar view; notifications show their count. Bluetooth rows use a dense optional-child list and reserve their own column height. Luau owns the row limit and overflow label; the adapter supplies every connected-device record. Wi-Fi previews can report the active access point while the provider has no primary-connection id. Child hover targets take precedence over a clickable parent, matching normal UI hit testing.

General primitive: `reveal` plus `slide_distance` moves retained content behind a stationary growing contour; no shell policy enters the renderer. See ADR 0004.
The notice adapter also seeds its count synchronously before asynchronous driver updates. Both the preliminary candidate and unmodified B0096 control reported cold-start unknown-state errors before recovering; those captures failed the error gate and are excluded. The same initialization fix is applied to the measured control, isolating panel changes without filtering errors.
No temporary duplicates or product dependency changes. The isolated helper build used `libraqm-dev`; private capture tooling includes exported `swaybg` and a `wlr-randr` build with JSON support. Nothing was installed on the desktop. Capture tooling corrected labwc's unsupported decoration setting to a `serverDecoration="no"` rule: the old titlebar displaced injected pointer Y coordinates. Invalid preliminary captures are excluded. A later private test-key binding had malformed KDL, causing Niri to fall back to its default configuration; the trigger-consistency gate caught the absent drawer. That visual batch is excluded and repeated with startup config validation and a binding-dispatch marker. Direct `wtype` injection into the inner and parent compositors did not dispatch its global binding. Those key attempts are excluded; acceptance uses a full-keymap private actuator through the parent compositor, with paced raw evdev keycodes, an explicit focus click, a canonical absolute Wayland display in the spawn helper and a recorded dispatch marker. The preliminary keyboard proof passes in both shells, with identical key/command drawer pixels in the fixed ROI. The independent typing lane uses its own unchanged compositor configuration.

## Evidence and method

Both measured arms include B0103’s initial prepared control-input feedback from `417ac32`. The final measured candidate is `f5919f0` plus the shadow asset-copy fix, product trees `crates:7cb38414` and `shell:a8a9a142` (binary SHA-256 `2029221c…`). Later accepted B0098/B0103 changes through `7ac5ef9` were rebased before delivery and receive full combined-source CI. Review found no B0097 anchor, contour or launcher typing-path change from that integration. Capture and memory results describe the isolated B0097 comparison, not a fresh measurement of the later combined commit.

Private nested sessions only, 2880×1800 at scale 1.5, identical owner wallpaper/palette/settings and audio/power/network fixtures. Bead uses tiny-skia CPU and matched `--trace-state` flags; pointer centres come from the running bar’s input geometry. QML uses its normal graphics backend. Every full PNG was checked for physical dimensions and compositor scale. [Metrics](evidence/0097/metrics.json) retain full-frame and fixed-region RGB RMSE, raw values on 0–255 and percentages of 255. Public pairs are crops/downscaled JPEGs; RMSE uses original PNGs.

QML's current status click routes to its control dropdown; the owner's requested independent anchored popouts are its existing named `nacre popout` surfaces. Popout pairs compare those native QML surfaces, using QML's reported pill centres, against actual Bead clicks. Hover pairs use actual pointer hover in both shells. A private Ctrl+Q binding invokes the existing control command in both sessions. The bar-control proof moves into the pill before pressing, allowing nested pointer entry/preparation to complete; an immediate synthetic move-and-press missed the cold click. This does not establish rapid cold-click latency.

Static physical ROIs are launcher `(560,850)–(2320,1800)`, drawer `(1900,75)–(2880,1800)`, popout `(1050,75)–(2880,1150)` and peek `(1950,75)–(2880,650)`. Trigger consistency uses `(2150,100)–(2860,1750)`.

Motion strips sample opening and closing every 25 ms, with actual timestamps; QML is left and Bead right. They verify direction, growing contour, corners and frame joins. Nested captures are not physical scanout timing. Static launcher/drawer scores include existing body differences and cannot quantify a motion-only fix.

Static ROI RGB RMSE, percent of 255 (lower is closer):

| Surface | Before → after | Evidence |
| --- | --- | --- |
| Launcher | 6.729 → 6.729 | [settled](evidence/0097/launcher-after.jpg), [open](evidence/0097/launcher-open-after.jpg), [close](evidence/0097/launcher-close-after.jpg) |
| Control command | 7.865 → 7.859 | [settled](evidence/0097/control-center-after.jpg), [open](evidence/0097/control-center-open-after.jpg), [close](evidence/0097/control-center-close-after.jpg) |
| Right lip | 37.828 → 7.859 | [pair](evidence/0097/lip-hover-right-after.jpg) |

Bar-background, right-lip and real private Ctrl+Q openings are each identical to the command-open drawer in the inner-drawer ROI: **0.000%**. All paired files have corresponding `-before`/`-after` names in [the evidence directory](evidence/0097).

| Status | Click ROI before → after | Hover ROI before → after | Final pairs |
| --- | --- | --- | --- |
| Sound | 30.359 → 6.659 | 22.209 → 4.923 | [click](evidence/0097/popout-audio-after.jpg), [hover](evidence/0097/hover-audio-after.jpg) |
| Wi-Fi | 40.678 → 11.029 | 22.294 → 5.684 | [click](evidence/0097/popout-network-after.jpg), [hover](evidence/0097/hover-network-after.jpg) |
| Bluetooth | 42.139 → 6.490 | 22.805 → 8.416 | [click](evidence/0097/popout-bluetooth-after.jpg), [hover](evidence/0097/hover-bluetooth-after.jpg) |
| Battery | 28.389 → 5.369 | 21.784 → 4.400 | [click](evidence/0097/popout-battery-after.jpg), [hover](evidence/0097/hover-battery-after.jpg) |
| Notifications | 28.968 → 4.916 | 25.748 → 7.281 | [click](evidence/0097/popout-notifications-after.jpg), [hover](evidence/0097/hover-notifications-after.jpg) |
| Calendar | 7.684 → 7.684 | 52.654 → 14.670 | [click](evidence/0097/popout-calendar-after.jpg), [hover](evidence/0097/hover-calendar-after.jpg) |

Calendar's existing click anchor is preserved. Additional radio-off peeks improve Wi-Fi 16.861 → 3.740% and Bluetooth 16.790 → 3.524%; battery's remaining-time/profile caption matches QML.

Motion scores are medians over the six sampled timestamps, using a 16-pixel joined-edge strip of the JPEG crop:

| Motion | Open before → after | Close before → after |
| --- | --- | --- |
| Launcher | 6.763 → 7.170% | 6.907 → 8.850% |
| Full-height control drawer | 11.915 → 8.715% | 15.589 → 6.278% |

The launcher remains attached to the bottom frame, but its time-aligned scores do **not** improve; command admission/readback timing and existing body differences remain. Matching the configured 300 ms curve does not establish matching command-to-present latency. The full-height drawer strips show both corners and its joined right edge. These are descriptive nested captures, not physical scanout measurements.

The first matched typing cohort failed the closed unreclaimable gate at **+1.496 MiB**, entirely anonymous, despite panel workers/SHM and observed image rasters being released. The reveal shadow path unnecessarily deep-cloned the scene's asset byte map each frame. Shadows now receive an empty asset map; image parts retain their resources. No allocator policy, trim delay or gate changed. The changed candidate passes a fresh three-pair lane; the failed cohort is retained separately, not filtered out.

| CPU typing lane | First-key median | Idle Δ unrec | Open Δ unrec | Closed Δ unrec |
| --- | --- | --- | --- | --- |
| Control / fixed candidate | 5.733 / **5.750 ms** | +0.117 MiB | +0.070 MiB | +0.219 MiB |

Closed paired deltas are +0.234, −2.938 and +0.219 MiB: one control arm has a large allocator-residency outlier. All six raw arms remain in the metrics; three pairs are a bounded guard, not a precise memory-effect estimate. Maximum close-minus-own-bar is 4.102 MiB, within +5 MiB. All first-key samples are below 8 ms.

Full-resolution CPU medians (MiB, five samples per phase):

| Phase | Control unrec | Candidate unrec | Δ anon = Δ unrec | Control / candidate clean-file PSS |
| --- | --- | --- | --- | --- |
| Idle | 21.924 | 22.201 | +0.277 | 20.118 / 20.109 |
| Closed + idle | 25.076 | 24.986 | −0.090 | 19.160 / 19.324 |

SHM is 0.686 MiB in each full-resolution phase; DRM and SwapPss are zero. Thus the settled PSS increase of +0.074 MiB is +0.164 MiB file-backed and −0.090 MiB anonymous, with no framebuffer increase. Both full-resolution closes remain within their own bar+5 MiB. These desktop CPU absolute totals do not establish compliance with the laptop's approximately 10 MiB cap.

## Checks and limits

Full remote Ubuntu CI-equivalent follows `.github/workflows/ci.yml`: fmt, clippy with warnings denied, workspace/trace-memory tests, renderer configurations, generated Luau/schema validation, all Python suites, private-bus/service/packaging/wallpaper checks and cargo-deny for workspace and independent crate graphs. Goldens were regenerated and visually reviewed. The isolated B0097 comparison has 213 byte-identical settled PNGs. After integration, 18 of 213 differ only in accepted B0098 chat/pearl images; B0097 launcher/control/status images remain unchanged. Regression coverage checks icon-centre routing, preview projections, lip configuration and partial prepared reveal frames without raster duplication.

The launcher guard uses three alternating B0085 populated-Home fixture pairs with four keys, CPU renderer, 1278×692 scale 1; it is separate from full-resolution visual captures. The final changed-candidate lane passes. All containment/state-error, full-window/scale, host-swap-in (zero) and source-hash gates pass. Host swap used is zero before accepted arms; this worker made no swap changes. Earlier failed/inconsistent harness captures are excluded from acceptance. No real-engine or presented-latency claim.

Unreclaimable memory is anon + shmem + DRM + SwapPss; clean file-backed PSS is reported separately. The final lane checks candidate-minus-control medians per phase against +0.5 MiB, anonymous growth against 1 MiB, and every close against its own bar+5 MiB. Daemon/provider totals remain separate; no absolute laptop-cap claim.

Known gaps: Qt versus CPU text/icon rasterization and body spacing differences remain visible. The launcher’s empty-Favorites message sits higher and its search focus stroke/corners differ; the control drawer has pre-existing capability-disabled tiles and notification loading text. Battery/sound popout body layout retains small height/slider/text differences; Wi-Fi retains its existing extra bottom action row and height. Calendar retains its existing Monday-first grid versus QML’s Sunday-first grid, and right-corner contour artifacts remain visible on calendar and notification cards. Launcher opening/closing admission differs in the timestamped nested sequences, and its sampled motion RMSE is worse than the control. The bar Wi-Fi indicator still reflects the provider’s missing-primary state while the active-AP preview is connected. Other bar clock/glyph differences belong to B0096. These are explicit residual parity gaps, not pixel-identical acceptance. No live desktop, installed shell, configuration or publication was changed.

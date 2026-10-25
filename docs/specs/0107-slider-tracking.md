# B0107 — continuous slider tracking

Accepted: the revised candidate passes the fast-drag, five-pair median memory and first-key gates.
No live desktop or installed software changed.

## Findings and change

The kit already maps absolute pointer x continuously, without a knob tween or delta drift.
Host actions still used hit cells (5% in control sliders), release snapped to the cell,
and the 750 ms feedback expiry could restore an older topic value. The baseline release
error is 11 px. A fast initial candidate also exposed an existing transaction bug:
release checked committed pressed state while a newer drag was still staged. Five of
its twelve gestures never emitted a released state and retained the previous endpoint.

The review candidate makes `pointer_arg` a view-authored continuous action argument.
The host stages feedback before resolving it; only writes are limited to 34 ms, with
release flushing the newest pending event. Release uses staged pressed/value state.
Keyboard steps stay discrete; the fill endpoint and thumb centre share the same mapping.

General `preview_source`/`preview_expected`/`preview_tolerance` metadata retire feedback
only after matching readback, or removal of the view. Old echoes cannot expire the target.
The kit's `echo_value` callback describes quantized readback without quantizing drag drawing;
the shell supplies the published keyboard-light step count at its adapter edge. The default
readback tolerance is half a percent of the range, configurable with `echo_tolerance`.
A refused write stays pending until convergence, another gesture or view removal; failure
reporting remains the adapter's responsibility. No service IDs enter framework code.

## Captured pixel error

Same-image cursor hotspot and thumb centre, physical pixels, absolute error; nearest-rank p95.
Twelve alternating directions per arm at nominal 2000 physical px/s (observed 1990.6–1998.4), 2880×1800,
scale 1.5, private 60 Hz labwc → niri. Bead uses CPU; QML uses threaded OpenGL.
All three accepted arms use identical ROI screencopy; full-window states verify geometry.
Only distinct cursor/thumb pairs inside the moving interval count as moving samples.
Screencopy perturbs the pipeline: this is a private comparison, not physical scanout timing.

Cursor calibration is per shape: QML switches from hand (−6 px) to arrow (+1 px),
while Bead stays hand. Two stationary QML arrow frames agree; the later +24 px
observation is already moving. Exact normalized white-mask fingerprints identify every
frame (QML: one hand, 299 arrow; Bead: 300 hand each). Recomputing with each shape's
own offset leaves the results unchanged. [Calibration frames](evidence/0107/cursor-calibration.png)
show QML above Bead; the complete shape manifest is in the measurement JSON.

| Arm | Moving n | Moving p50 / p95 | Last capture p50 / p95 |
| --- | ---: | ---: | ---: |
| QML reference | 40 | 94.5 / 156.5 px | 0.5 / 0.5 px |
| Bead control | 31 | 101.5 / 132.0 px | 11.0 / 11.0 px |
| Bead revised | 34 | 97.5 / 155.0 px | 0.5 / 0.5 px |

All twelve revised gestures report released state, exact final downstream write and matching
source readback. A last screenshot alone is not treated as proof of release delivery.
The initial candidate missed five releases; that attempt and its original full-screen
capture method are retained separately in the evidence JSON, not pooled into this comparison.

[QML frame series](evidence/0107/frames-r2-qml.jpg), [control](evidence/0107/frames-r2-bead-control.jpg),
[revised candidate](evidence/0107/frames-r2-bead-final.jpg), [per-frame numbers and source hashes](evidence/0107/measurements.json).
Paired QML/control/revised states:
[idle](evidence/0107/idle.jpg), [open](evidence/0107/control-center.jpg),
[hover](evidence/0107/control-hover.jpg), [pressed](evidence/0107/control-pressed.jpg),
[audio](evidence/0107/control-audio.jpg), [battery](evidence/0107/control-battery.jpg),
[network](evidence/0107/control-network.jpg), [Bluetooth](evidence/0107/control-bluetooth.jpg).
Known inherited B0105 gaps remain: tile/label geometry and wrapping, header/button
typography, unsupported fixture controls disabled only in Bead, action-icon placement,
bar font/glyph/clock differences, and reveal/fade/easing differences.
Full-window originals, traces and containment proofs remain private.

## First key and five-pair memory gate

Five alternating AB/BA/AB/BA/AB populated-Home pairs; five samples per phase after 20 seconds
settled. Unreclaimable is anon + SHM + DRM + SwapPss. PSS and file-backed PSS are informational.
All figures below are MiB, CPU renderer. The gate uses the median of paired deltas.

| Phase | Unreclaimable control / revised median | Paired Δ median | Five paired deltas | PSS control / revised | File control / revised |
| --- | ---: | ---: | --- | ---: | ---: |
| Idle | 6.373 / 6.342 | -0.027 | -0.027, +0.051, -0.164, -0.078, +0.004 | 26.312 / 26.479 | 19.970 / 20.086 |
| Open | 21.494 / 21.350 | -0.070 | -0.078, +0.043, -0.379, -0.070, +0.262 | 42.004 / 42.055 | 20.652 / 20.645 |
| Closed | 7.424 / 7.342 | -0.082 | +1.309, +0.871, -0.305, -0.109, -0.082 | 26.844 / 26.771 | 19.322 / 19.344 |

First-key UI-process CPU (ms), control: 7.010, 6.793, 7.043, 7.040, 7.205;
revised: 7.229, 7.218, 7.210, 7.245, 6.927. Every revised sample is ≤8 ms.
The original single-pair +1.488 MiB closed spike remains recorded. The amended memory rule
requires at least five alternating pairs and permits isolated ≤2 MiB anonymous spikes;
the accepted cohort is judged on its median. DRM and process SwapPss are known zero in every
sample; host swap-in is within the per-arm gate. The laptop absolute cap was not measured.

## Verification

The revised runtime passes the complete `.github/workflows/ci.yml` equivalent in a
remote Arch container, Rust 1.99.0, Cargo `-j2`, nice 10: fmt, clippy `-D warnings`,
workspace tests (including 92 UI and 568 shell tests), four trace memory tests, feature
configurations, Luau/schema, capture/packaging regressions, wallpaper smoke, service/tray
lifecycle and cargo-deny, including independent framework graphs. Full log SHA256:
`f069d5184570a503e49eea5cf0606f48b30bcacaf142393c9e29c875b6f34348`.
The focused proof covers rapid absolute jumps, vertical escape, mapped arguments, old
source echoes, staged release before frame acceptance, coarse readback and keyboard steps.
Rejected setup attempts (missing private swaybg and private GVFS teardown) are not acceptance
evidence; all three accepted pixel arms use the same early private VFS environment.
No laptop builds/tests or changes to live UI, installed software, configuration or services.
Temporary product duplicates: none. Private harness copies expire seven days after review.

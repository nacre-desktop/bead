# Finger stop/cancel probe (stop delivery unproven)

Matched Bead CPU control e2d/9bf and candidate65aa/34d0, one private popup gesture per arm, 20 synthetic notices, source pin passed, no UI errors, and no discarded presentation feedback. A private harness copy (SHA `ea022ec0d1a307faa7d50348fa51c6673005f6eadd713cafa179a0a553089a52`) sends finger16×10px at8ms, then its sender-side `AXIS_STOP`, then after the sender’s300ms hold plus100ms focus handoff six opposite7px events at8ms. However, the client WAYLAND_DEBUG trace has `wl_pointer.axis_source(2)` and `frame()` after the final positive `wl_pointer.axis()` but no `wl_pointer.axis_stop`. The sender marker does not prove the nested compositor delivered the stop; the canonical measurement harness is unchanged.

Between the sender-side stop marker and the later opposite input, control offsets advanced390→1177.76px over395.5ms; candidate offsets advanced130→160px within25.2ms and then plateaued. Without a client `wl_pointer.axis_stop` event, these changes may include delayed direct-input work and do not prove a kinetic fling. There were no positive `AXIS_DELIVER` events after the first opposite input in either arm.

| Arm | First opposite input→presented reverse | Delivery→presented reverse | Render p95 | Presented p95 all-gap | Max gap / missed upper bound | Discarded |
|---|---:|---:|---:|---:|---:|---:|
| Control | 16.78ms | 16.48ms | 3.260ms | 17.49ms |97.95ms /106 |0|
| Candidate |7.52ms|7.28ms|4.151ms|393.49ms|393.49ms /51|0|

The opposite-input cancellation result is valid independently of the stop audit: control presented one stale positive offset at+0.7ms after reversal input before reversing at+16.78ms; candidate’s first post-input presented offset reversed at+7.52ms. The requested kinetic-fling acceptance remains unproven because no wire-level stop reached the client. Candidate meets the one8.33ms frame reversal target for this gesture; its all-gap cadence p95 includes a long post-gesture idle span, so this does not prove sustained frame cadence or physical120Hz (`refresh_ns=0`). See [control JSON](control-pacing.json), [candidate JSON](candidate-pacing.json), and curated [stop/delivery/presentation joins](kinetic-state-joins.json).

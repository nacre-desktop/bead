# B0103 — control input latency

Status: blocked on private capture availability; candidate passes remote CI, but is not accepted.
No latency, parity, first-key or memory acceptance is claimed.

Control pointer positions previously queued independently behind every script/render acknowledgement.
Changing hover targets reprojects services and materializes the view; unchanged hover did so too.
Slider drawing used the topic value while its asynchronous action was pending. The default focus ring
also painted on pointer focus. These are code findings; timing attribution requires the private capture.

## Candidate

- Coalesce consecutive worker hover/drag positions, preserving all other event ordering.
- Skip unchanged hover before service projection; retain projection for actual hover transitions.
- General numeric feedback: `preview_key`/`preview_value` on an action region and
  `ui.preview_value(key, authoritative)` in markup. `preview_inset` maps the track to its thumb centre;
  intermediate pointer positions move the knob continuously while action values retain their declared step. The kit slider uses this; service IDs stay at the edge.
- Hold continuous feedback during the gesture; release settles immediately to the declared action step,
  then allows 750 ms for topic convergence before reconciliation.
  `SceneSource::wakeup_at` drives deferred actions and expiry without a resident polling thread.
- Slider host actions keep their newest value at 34 ms intervals; release flushes the final target.
  A final flush is exempt from the continuous-drag limit. Absolute targets are never skipped based on stale topic equality.
- The legacy sound slider enqueues its PipeWire level without waiting for the worker reply; synchronous API callers retain their existing contract.
- Numeric range gestures retain capture outside their row, clamp at the endpoints, and cannot inherit
  another button's press. Tab focus does not change a value; arrow activation stays discrete and reaches the host adapter.
- Pointer focus retains input routing but suppresses the keyboard outline. Keyboard focus still shows it.
- `virtual-pointer.c` adds one-device 250 Hz drag injection; intermediate events carry monotonic timestamps.

## Acceptance evidence

| Gate | Before | Candidate |
| --- | --- | --- |
| CPU hover → present, 2880×1800 / 1.5 | pending | pending |
| GL hover → present, same geometry | pending | pending |
| CPU drag → present | pending | pending |
| GL drag → present | pending | pending |
| Launcher first-key UI ≤8 ms | pending | pending |
| Idle unreclaimable delta ≤+0.5 MiB | pending | pending |

Per-stage capture must separate pointer receipt, handler/view, layout, bake, commit and presentation.
Private/nested timing does not establish physical scanout latency. QML parity uses the same private
geometry, presentation inputs and daemon fixtures. Missing measurements are not zeros.

## Checks and limitations

The blocking private capture lease did not become available during more than 90 minutes of queued waiting.
No nested session started; there are no paired screenshots or before/after timing results.
Resume the CPU/GL, matched QML, first-key and same-host memory proofs before integration.

Validation is remote only. Full CI follows `.github/workflows/ci.yml`, in an Arch container.
The Arch replica needs Git snapshot metadata, Sway's `swaybg` helper, and Sway's file capability removed
inside the container (Docker omits that capability). No host configuration changed. New tool dependency: wlr-protocols.
The control's existing 5% action step is retained (the QML control declares a 1% step); pointer feedback is continuous
and release settles immediately to the declared step. Percentage readouts remain topic-backed until their echo.
Temporary product duplicates: none.

The 750 ms post-release reconciliation window is a candidate policy, not a proven service-latency bound;
delayed topic convergence beyond it remains an explicit validation risk.

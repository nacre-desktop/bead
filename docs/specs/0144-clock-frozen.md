# B0144 — Clock updates across busy minute boundaries

The clock service could miss a minute without suspend. Each D-Bus event cancelled
the pending sleep; if its handler crossed the minute boundary, the next loop
scheduled the following minute. Drift detection did not help because realtime
and monotonic time advanced together. Repeated boundary traffic could keep it stale.

The loop now checks the current wall-clock slot before every wait. It retains
second-resolution leases, drift detection, resume and timezone refreshes. The
existing publication path logs `CLOCK_TICK value=HH:MM` only when the minute text
changes. No renderer, widget, idle policy or desktop configuration change is needed.

## Regression evidence

- A private logind fixture delays `GetSession` across a minute boundary. The test
  confirms the handler entered before that boundary. The original loop failed to
  publish within three seconds; the candidate passed. No host clock adjustment,
  suspend, real system-bus write or live shell is involved.
- A headless test drives the real bar clock through `update_model`, including
  equal-width text changes and forward/backward jumps, at scales 1 and 1.5.
  Each changed minute creates a scene and nonempty frame damage; equal values idle.
- `spikes/clock-proof.py` uses the existing guarded nested-session harness. It
  captures every minute and records service publications and primary bar commits.
  Optional panel and process-stop/resume phases exercise closed-panel GPU suspend,
  reopen, and private `PrepareForSleep(false)` delivery. Process stop does not
  simulate a stopped monotonic clock; the drift unit test covers that case.

The supplied live journal predates the new marker, so it cannot identify which
path failed in the owner's reported occurrence. A reproduced service defect is
fixed here; that historical occurrence cannot be conclusively attributed to it.

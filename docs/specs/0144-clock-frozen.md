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

Both ordinary nested idle runs passed six minutes without input at 2880×1800,
scale 1.5 (CPU bar, default panel renderers, debug builds). The control was
861d3630 plus the marker; the candidate was 729e024b, before rebasing the two
concurrently accepted render fixes. Every minute had distinct clock pixels;
steady idle intervals had two primary-bar commits per minute. The candidate's
last summary interval also includes later panel traffic. Its first GPU phase
timed out because calendar is transient; the proof now uses the retained GL
wallpaper picker for GPU suspend, alongside the calendar open/close check.

| Idle arm | Duration | Consecutive minute values | Missed repaint |
|---|---:|---:|---:|
| Control | 360 s | 7 | 0 |
| Candidate | 360 s | 7 | 0 |

Clock crops and publication/commit summaries: [control](evidence/0144/baseline.png),
[candidate](evidence/0144/candidate-idle.png), [control data](evidence/0144/baseline.json),
[candidate data](evidence/0144/candidate-idle.json). Ordinary idle did not reproduce
the freeze; the deliberately busy private-bus test did.

The rebased engine (0a1fccc6; proof b135935a plus the short-stop correction)
also passed two idle minutes, calendar open/close, retained wallpaper GL suspend,
and picker reopen. A final recovery arm passed: idle 15:59 / 19 bar commits,
GPU-suspended 16:00 / 26 commits, resumed 16:01 / 30 commits, with distinct clock
pixels at each stage ([image](evidence/0144/recovery.png),
[data](evidence/0144/recovery.json)). Minute summaries extract the five-character
time prefix from interleaved debug output. There were no UI errors in the successful arm.

The initial 65-second SIGSTOP correctly hit the existing six-second main-loop
watchdog. The successful simulation instead brackets a two-second stop across a
minute boundary with private `PrepareForSleep(true/false)` signals, leaving the
watchdog enabled. No actual machine suspend or stopped-monotonic integration test
is claimed; B0140's wall-aligned/drift unit regression also passes.

Full `.github/workflows/ci.yml` equivalent passed in a disposable remote Arch
container (`FULL_CI_PASS`, Rust 1.99.1, Cargo two jobs), including the new private
clock test and every independent framework license graph. Arch setup required
`swaybg` and removal of Sway's packaged file capability inside that container.
An earlier concurrent-build run hit an existing shortcut-test 50 ms VM limit;
the clean full run passed without changing that test. No live install was performed.

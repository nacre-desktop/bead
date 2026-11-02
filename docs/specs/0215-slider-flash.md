# B0215 — slider drag flashes

The retained slider child follows the pointer, while the parent retains its
pre-drag fill. CPU label updates can replace a submitted frame before its
callback arrives. That path skipped masking the parent slider, exposing the
old full fill beneath a downward-moving child. Mask every submitted parent
frame; restore the source scene after drawing.

Scope: the shared slider painting path and regression tests. No kit/frontend
interface change, protocol change, deployment or live desktop access.

Existing source acknowledgement keeps pointer feedback through dragging and
release until final readback converges. Keep it: stale/full echoes must not
retire feedback. The wire value remains linear; audio sliders show its cube
root as a percentage (20% maps to linear 0.008), consistently in both directions.

Nacre N0106 separately removes cached key-step writes to the audio topic.
The watcher owns audio state; instant key-step OSD remains. Absolute slider
set-volume already mutates without publishing optimistic audio state.

Checks: a 40-motion 100%→20% drag with full, delayed and matching echoes for
output, microphone and brightness; stale echoes after release until convergence;
retained-parent and child pixels at scale 1 and 1.5 with a pending callback.
Full Ubuntu CI and the producer's full Arch desktop CI run in private containers.
Nested visual evidence is unavailable: the test host connection timed out.
Live 120 Hz acceptance remains with the owner/lead; headless frames are not scanout.

Validation results are recorded in the unit report; no memory measurement is
claimed. This changes per-frame masking of an existing temporary scene, adding
no persistent cache or buffer. No replacement paths or temporary duplicates remain.

# B0139 — No native pearl on dropdown open

B0131 incorrectly treated a mapped transparent native toplevel as invisible.
On the owner's niri session, opening control or the top dashboard could expose
that target as a large solid square and leave it mapped. `PEARL_SERVER_PREWARM_READY`
confirmed the unwanted native admission. This is a regression, not a detach action.

Remove native preparation from accepted panel-open frames. Existing content and
shared GL warm-up remain offscreen. Native admission is allowed only after an
accepted docked drag press; a release without detach destroys the prepared
window and clears its reservation. Every panel close cancels preparation before
the delayed worker/cache cleanup, including non-retained panels. Existing
covered final-placement handoff and stale-callback checks remain in use.

CPU and GL each passed 20 control and 20 dashboard open/close cycles:
zero native toplevel requests and zero `bead.pearl` windows during open and after
close. The callback-gate unit test passes; actual creation is checked by the
nested protocol trace. Each renderer also passed 20 detach image trials with
zero blanks/reversals and stable native bodies. Render p99 CPU/GL is
12.655/3.288 ms. Capture-loaded handoff p95 37.999/34.342 ms still misses the
unchanged 33 ms gate; no new latency pass is claimed. The child backend is
60 Hz with software timestamps, not physical 120 Hz scanout.

Full remote Arch CI passed, including the new regression and all workflow steps.
The private-copy dirty-Git check and obsolete-pointer harness failures were
corrected without product or threshold changes; complete cohorts use the current
pointer tool. [Raw cohorts, methods and CI receipt](evidence/0139/README.md).
Product delta: +66/−17 lines including tests. No new dependencies, duplicate
paths, live configuration changes or installation. Existing mapped windows from
an old running binary are outside this private validation/deployment scope.

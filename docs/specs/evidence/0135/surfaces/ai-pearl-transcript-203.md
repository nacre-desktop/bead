# Standalone AI transcript probe

The public `pearls.open ai` route was measured in a private nested session with
the synthetic Codex fixture. This is a Bead-only window measurement; it does not
claim QML parity or physical scanout timing. Both captures used the 2880×1800
output at scale 1.5, the actual 470×810 pearl window, and the transcript viewport
at local `[16,181.67]`, size 438×460.33, extent 2709.60. The absolute input point
was derived from the live window and chat geometry, not guessed.

The first candidate probe (203 source before the chat-follow fix) and the
matched control both received the two input directions but stayed at the bottom
offset 2249.27. The inspected state keys were `chat.scroll` and `chat.qscroll`;
the transcript binds its scroll property to `chat.scroll`. That capture showed
the behavior issue: small reader movements near the bottom were snapped back to
the end. The later chat-follow fix keeps reader scroll changes until the actual
bottom is reached.

In the final paired probe, the control remained clamped at 2249.27 through 268
state samples, while the candidate traversed 2249.27→1689→2249.27 across 164
samples (82 distinct offsets) with 160 delivered events: 80 upward and 80
downward. Both arms had zero harness errors; source validation passed and host
swap was zero before and after each arm. The input helper log contains
`AXIS_SENT` / `AXIS_STOP source=finger` markers, but the nested compositor's
wire-level axis source and stop delivery were not independently verified.

| Arm | Render p95 | Presented interval p95, all gaps | Presented drops, upper bound | Reversal from delivery / input |
| --- | ---: | ---: | ---: | ---: |
| Control | 0.039 ms | 16.23 ms | 110 (53.1%) | unmeasured: no viewport movement |
| Candidate | 2.093 ms | 16.03 ms | 89 (51.4%) | 40.00 / 49.22 ms |

Reversal is joined from delivered direction, a changed `chat.scroll`, and its
presentation. Update85 first commits the opposite logical step at1696, but the
previously presented update83 already shows1696. Update87 first presents a
larger offset,1710; that is the visible reversal used for49.22/40.00ms. The candidate improves this surface from no movement to a proven
reversal, but 40 ms is above the one-frame target. Presentation timestamps had
`refresh_ns=0`; these cadence/drop counts are not physical 120 Hz certification.
The compositor advertised a 60 Hz mode. Endpoint images return to the same
bottom position after the up/down gesture and therefore do not show the
intermediate movement; see [control before](ai-pearl-transcript-203-final/control-before.png),
[control after](ai-pearl-transcript-203-final/control-after.png),
[candidate before](ai-pearl-transcript-203-final/candidate-before.png), and
[candidate after](ai-pearl-transcript-203-final/candidate-after.png).

The diagnostic-only run and final run hashes, exact sample counts, ranges,
geometry, source/binary pins, and per-arm p95s are in
[the compact JSON evidence](ai-pearl-transcript-203.json).

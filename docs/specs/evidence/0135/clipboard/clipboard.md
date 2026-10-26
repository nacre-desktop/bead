# Clipboard surface preflight

Two earlier attempts are retained as invalid diagnostics: one sent QML to Dashboard via the wrong route, and one used a Bead build whose page registry failed before READY. Both failures were corrected before the [matched capture below](#corrected-matched-capture); neither is treated as a zero-frame result. The final Bead 65aa control binary was later flagged for shared-target provenance ambiguity, so its row-count and timing values are diagnostic rather than a final before/after gate. The functional candidate still proves the 50-row fixture and scroll-store movement. The fresh clean-target control/AI run used a separate Bead source snapshot; a final clean-target clipboard control measurement remains outstanding.

The earlier QML screenshots are [before](qml-before.png) and [after](qml-after.png), and its pacing JSON is [here](qml-pacing.json). The corrected QML `nacre clipboard` capture is visual-only at y900: Qt exposes neither a scroll-store join nor presentation feedback, so QML timing and reversal remain unmeasured.

## Corrected matched capture

The corrected route was captured with 65aa control (`c20b2440…`, source `f404c54b…`) and candidate (`34d0d01f…`, source `7f050b40…`, shell tree `52a5777a…`), CPU backend, 2880×1800 parent / 1920×1200 logical at1.5, one repeat per input. The Bead control binary has ambiguous shared-Cargo-target provenance and is not a final performance comparator. Source validation passed both arms, host swap was0 KiB, and no UI errors or discarded presentation feedback occurred. The same private cliphist stub and Bead dashboard fixture supplied50 text rows; UI_STATE confirms Bead `dashboard.page=clipboard`, with7 control rows and50 candidate rows.

| Input | Control | Candidate |
|---|---|---|
| Finger | Old view made one commit; no `dashboard.clipboard.scroll` state or reversal proof (not a scrolling baseline). | Store direction is joined to presentation. Input→visible41.36ms; delivery→visible31.97ms; render p95 1.238ms; all-gap presentation p95 16.05ms, max98.77ms; 91 missed-slot upper bound;84 commits. |
| 120ms wheel | No scroll-store join or presentation sample (old view remains non-scrollable). | Proven reversal: input→visible14.13ms; delivery→visible5.55ms; render p95 0.940ms; all-gap presentation p95 10.55ms, max414.28ms; 51 missed-slot upper bound;74 commits. |

The all-gap intervals retain the large idle/clamp gaps; missed-slot values are upper bounds, not a drop-free claim. `refresh_ns=0` means the nested run did not prove physical 120Hz. QML used the published `nacre clipboard` command and the same private50-row cliphist source. In this paired run, y600 is above the QML panel (which starts at logical y628), so the QML input was not a valid scroll interaction; its screenshot is panel-open visual evidence only. A separate QML-only y900 sample below verifies the list visibly moves with a sampled mid-gesture screenshot. Qt emits no state-store join or presentation-feedback trace in this harness, so no QML timing or reversal claim is made. QML’s right-docked clipboard overlay differs structurally from Bead’s compact centered dashboard page; this is a recorded parity gap, not matched geometry. See [candidate before](65aa/candidate-bead-before.png), [candidate after](65aa/candidate-bead-after.png), [control before](65aa/control-bead-before.png), and [QML before](65aa/qml-before.png). Raw per-arm JSON is in `65aa/`.


The corrected QML-only point and sampled motion are documented in [the visual probe](qml-y900-visual.md); its screenshot sample was excluded from performance claims.

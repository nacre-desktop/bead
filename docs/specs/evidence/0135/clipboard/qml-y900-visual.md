# QML clipboard visual input probe

A private QML-only copy of the frozen harness changed the clipboard point to logical(800,900), inside the opened clipboard list, and enabled `PACING_CAPTURE_MID=1`. The private50-row text fixture and Nacre/Orient source pin were unchanged. It completed rc0 with183 commits and no harness errors; because a screenshot was taken during the timed gesture and Qt supplies no scroll-state/presentation-feedback join, no timing claim is accepted.

The mid-gesture image shows the synthetic clipboard list after the corrected point received input. See [before](qml-y900-visual/before.png), [mid](qml-y900-visual/mid.png), and [after](qml-y900-visual/after.png). These screenshots are visual evidence only; they do not supply a QML reversal latency or 120Hz/drop certification.

# B0139 regression evidence

`open-close-proof.py` is the exact private runner: copy it beside the repository's
`spikes/` directory and provide the existing guarded parity-harness arguments.
It opens control 20 times and the top dashboard 20 times per renderer. Every
open must acknowledge the expected view, open state and nonempty size; every
close must acknowledge `target_open=false`. The open observation lasts one
second, with window queries throughout open and close. It sends no pointer drag.
Niri must report zero `bead.pearl` windows, and the entire Wayland client trace
must contain zero `get_toplevel` requests.

The unit test exercises the native-preparation callback gate over 20 no-drag
cycles. Actual Wayland creation is checked by the nested protocol proof, not
inferred from the unit callback alone. Separate 20-trial relative held-button
detach cohorts retain the existing blank/reversal and stable-body image gates.
Timing results remain reported separately and are not silently waived.

[Summary](summary.json), [complete raw cohorts](trials.json.gz), [CI command replica](full-ci.sh)
and [receipt](ci.txt) record the results. CPU and GL each pass 40 open/close
cycles and 20 detach image trials. Capture-loaded timing remains over 33 ms
(CPU37.999/GL34.342 ms); render p99 is CPU12.655/GL3.288 ms. Full remote CI passes. Private sessions only; no live
installation, compositor configuration, or desktop changes. Full-output
captures are not published because they can contain provider inventory.

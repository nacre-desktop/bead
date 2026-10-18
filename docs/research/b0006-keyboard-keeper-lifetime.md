# B0006 keyboard keeper lifetime

Status: Review. Implementation commit `b7e5862055ee7b3d53bd49d0a0436db2c90bb200`;
hosted CI run [39081161039](https://github.com/nacre-desktop/bead/actions/runs/39081161039)
passed all checks, including the new regression step. No measurement or capture
was run.

Acceptance mode now starts the existing owned `wtype` keeper with
`-s 1800000` (30 minutes); legacy mode retains `-s 600000` (10 minutes). No
keeper is created when input capability setup is not requested. Acceptance
liveness is checked after warmup and immediately before baseline, after baseline,
before and after every 60-second phase observation, and before successful
cleanup. An exited keeper raises and invalidates the arm; there is no restart or
capability substitution. The existing `finally` path stops only the owned
process object on success or failure.

Four fake-clock/process tests were added to the prior seven capture-window tests.
They cover 30-minute survival across a 600-second warmup plus baseline and all
phase windows, unchanged legacy duration, expiry at warmup/baseline/phase with
arm invalidation and exact-object cleanup, and no process when unrequested.

Local checks passed:

```sh
nice -n 10 python3 -m unittest discover -s spikes/tests -p test_dashboard_capture_windows.py
nice -n 10 python3 -m py_compile spikes/dashboard-residency.py spikes/launcher-proof.py spikes/tests/test_dashboard_capture_windows.py
nice -n 10 git diff --check
```

The tests use mocked `Popen`, fake clock advancement, and fake `poll` state; they
launch no process and perform no real sleep, display, bus, `/proc`, or capture
operation. They establish source-level lifecycle behavior only, not physical
keyboard input or artifact provenance.

| File | SHA-256 |
| --- | --- |
| `spikes/dashboard-residency.py` | `767d390e71726b9c815a57e2003efe51dadfb816b3f395fc1f6713d332e131fa` |
| `spikes/launcher-proof.py` | `ab16448fbebe41675a41ea2e61f54917971bd3b01d9785feb48574a6505ad594` |
| `spikes/tests/test_dashboard_capture_windows.py` | `ec37fc0e194cfdd73e70a8c79d3db2f573489bb78b81248cf6ee4fce03db5d85` |

B0002 remains gated: no source-proven UI binaries, no certified source pairing
for helper `75157`, inherited `RUSTSEC-2026-0192` still needs an owner ruling,
and no measurement slot is granted.

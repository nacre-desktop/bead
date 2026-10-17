# B0005 capture-window correctness

Status: Review. Harness commit `46b2a9924e6dced7a35e630dc73c525958a25fbe` and
hosted regression-workflow commit `bb984745d2db95b91759451a4f1df19ea1529b54`
are published. Exact-head CI
[39079511622](https://github.com/nacre-desktop/bead/actions/runs/39079511622)
succeeded, including the named Python regression step. No measurement permission
is implied.

## Changes and checks

The acceptance mode is explicit (`BEAD_DASHBOARD_ACCEPTANCE=1`) and validates
the four dashboard pages, four slots, combined legacy phase and optional
`BEAD_DASHBOARD_WARMUP_SECONDS=0|600` before the launcher can start. After the
same UI/helper reaches presentation readiness, the optional warmup runs before
the baseline and the existing idle sampler measures the baseline for 60 seconds.
Every phase endpoint in acceptance mode also calls that sampler for a real
60-second interval. A wait plus a one-second sample is never substituted.

The fixed matched phase budgets are 65/85/105/95 seconds. Their activity limits
remain 5/25/45/35 seconds, matching the former 35/55/75/65 budgets with their
30-second idle allowance. Acceptance rows record the selected mode, requested
and measured warmup, baseline idle, phase budget, activity, pre-window wait and
actual endpoint idle. Activity overrun or a short 60-second observation fails.
Unset acceptance mode keeps the previous phase timing and one-second close and
endpoint observations. Existing swap, own-bar+5 MiB, 25 MiB and 24.5 MiB checks
remain in the capture path.

Seven standard-library fake-clock tests passed:

```sh
PYTHONDONTWRITEBYTECODE=1 nice -n 10 python3 -m unittest discover -s spikes/tests -v
PYTHONDONTWRITEBYTECODE=1 nice -n 10 python3 -m py_compile spikes/dashboard-residency.py spikes/launcher-proof.py spikes/tests/test_dashboard_capture_windows.py
```

The tests cover full idle versus sleep-plus-one-second, warmup order, phase
overrun, identical arm schedules, legacy timing, invalid prelaunch values and
the existing swap/memory rails. Syntax compilation and `git diff --check` pass.
No measurement-path sleep, Bead launcher, compositor, helper, `/proc` sample,
preview, benchmark, remote probe or memory capture was run. No manual local or
remote Rust build was run; hosted CI ran the unchanged repository Rust checks.
The minimal CI change adds the unittest and `py_compile` commands before the
existing service lifecycle proof.

## Identity and schedule

Start: accepted B0002 preflight `bb75590128cbf85b57a416ea881b3482ae802729`;
B0005 spec base `a0ce6fbd968b45a7796e455877c775be25675422`; In-progress checkpoint
`3019722ef956814bb75f6b510a6612833fd04b44`. Harness implementation commit:
`46b2a9924e6dced7a35e630dc73c525958a25fbe`.

| File | SHA-256 |
| --- | --- |
| `spikes/dashboard-residency.py` | `8496208175bcd4a21999af4fc4dca9894ccb378335e605349a742352aabbd15a` |
| `spikes/launcher-proof.py` | `d29ef0bd37f184a120d536a13f1a80ae892e3b27ce72ca44e77c298a43a9a549` |
| `spikes/tests/test_dashboard_capture_windows.py` | `8e4f731086d850e3345644f7c4b649426c67940e994c8842532c3b7ebecf742e` |

The required count is five alternating pairs per compositor: 10 pairs and 20
cells total. The prior planning range of 94–124 minutes omitted the baseline
window and does not match the fixed phase budgets. Based on the earlier observed
4m17s per cell, acceptance adds 120 seconds per cell for phase budgets and 58
seconds per cell for baseline idle, projecting about 145 minutes total before
recovery margin. With a 600-second warmup on each of 20 arms, add 200 minutes:
about 345 minutes (5h45) total. Per compositor that is about 1h13 without warmup
or 2h53 with warmup; separate three-hour compositor windows nominally fit the
warmup variant, with little recovery margin. This is a planning estimate, not a
measurement or slot request.

## Remaining B0002 gates

No source-proven baseline/candidate UI binaries or certified helper/source
pairing are available. `RUSTSEC-2026-0192` remains unresolved by the owner. No
capture grant or window exists. This change establishes harness timing only;
it does not complete B0002 or certify any artifact.

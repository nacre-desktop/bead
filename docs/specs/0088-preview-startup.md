# B0088 - Preview startup: ui.dispatch slowness and the harness crash

Status: **Review** (Claude worker, branch `task/b0088-preview-startup-20261024`). Urgent: the owner's preview (Bead `a150afe3`, Nacre `f50e6830`,
`bead-preview-20261024T1133`) crashed at startup with `subprocess.TimeoutExpired` in `spikes/foundation-preview.py`.

## Finding

* **Not reproduced on the desktop.** Same staged build (`nacre-helper`, `nacre-shell`, shell, tools), run through `scripts/try-bead.py` in a private niri
  (headless parent, CPU and GL renderer, `--real-wallpaper`, also with 24 niced busy loops on 12 cores): the first `ui.dispatch nacre state` answers
  in **35-190 ms** (one "UI provider is unavailable" reply just before the provider registers, about 10 ms), later calls 8-10 ms. The previous build
  (`bead-preview-20261024T0653`, Bead `996031b5` / Nacre `16436a34`) behaves the same (45 ms). No regression from N0057/N0059/N0060: `system_power`,
  audio and capability probes are async with their own 2-3 s timeouts or run on the offload pool, none on the dispatch path.
* **The harness was the fragile part.** `nacrectl call --timeout 2000` is bounded by connect (2 s) + handshake (up to 5 s) + call (2 s) = 9 s, but the
  harness killed it at 4 s and only caught `RuntimeError`, so one slow start on a loaded host (the laptop had 12 GiB of swap in use) ended the run with a
  raw `TimeoutExpired`. Most likely cause of the owner's failure: the freshly started private daemon or `nacrectl` stalled under host memory/CPU pressure.
  Evidence for it beyond the 4 s arithmetic is missing: the failed run's daemon/Bead logs were deleted with its temp directory.

## Change

* `private_call()` in `spikes/foundation-preview.py`: process limit 12 s (above the 9 s worst case), `TimeoutExpired` becomes a retryable `RuntimeError`
  with a clear message, failures carry nacrectl's stderr; both `wait_call` copies share it, retry until a 60 s deadline and name the last error.
* `spikes/tests/test_foundation_preview.py`: hung nacrectl is a `RuntimeError`, stderr is carried, JSON parsed.

## Production start

The harness starts Bead with the exact production flags of `packaging/bin/bead-shell` against a private daemon; the production path adds no extra wait
(the daemon's own session observer also calls `ui.dispatch` with a 5 s deadline and tolerates a late provider). No daemon change was needed, so no Nacre
commit. A real-daemon trial on the live desktop is owner-only and was not run.

## Open

* If it fails again, the new message plus the preserved logs (`BEAD_RAW_DIR`) will show whether the stall is in connect, handshake or the provider.

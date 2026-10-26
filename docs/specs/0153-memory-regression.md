# B0153 — Launcher memory regression

B0150 reported post-expiry closed anon of 501.5 MiB CPU / 499.5 MiB GL from one arm each; attribution was unknown.
The B0114-based private nested capture did not reproduce that growth on baseline or current main.
It records shell `smaps_rollup`, per-mapping `smaps`, allocator stats, cache accounting, and renderer markers at each phase.
No first bad commit or retaining Bead object can be assigned without a reproducible failing build; no product fix is claimed.

| Phase | Initial | Candidate | Limit |
|---|---:|---:|---:|
| Closed after expiry Pss_Anon (CPU) | baseline 27.5 → 28.7 MiB | main 28.2 → 29.1 MiB | ≤ initial + 10 MiB |
| 128 notification rows, post-expiry (CPU) | — | main 31.6 → 32.6 MiB | ≤ initial + 10 MiB |
| Current Nacre helper + 128 notifications, post-expiry (CPU) | — | main 29.1 → 33.9 MiB | ≤ initial + 10 MiB |
| Historical 1c968346 candidate (CPU, invalid partial) | — | bar 23.0; first two closes 24.2 MiB | Not a valid endpoint |
| B0150 source 2d49b069, 101 apps, post-expiry (CPU) | 25.7 MiB | 27.4 MiB (+1.7) | ≤ initial + 10 MiB |
| Synthetic 320-result launcher, post-expiry (CPU) | — | main 9.2 → 11.2 MiB | ≤ initial + 10 MiB |
| Launcher open Pss_Anon / PSS | main 37.8–41.4 MiB across samples | — | Report; existing close gate remains bar + 5 MiB |

## Method and evidence

- Baseline `900369fb` and `origin/main`; private nested niri, 2880×1800 at 1.5 scale, CPU renderer; three open/type/close cycles, then a 125-second observation beyond the 120-second warm icon expiry. B0150's one-arm figures are not a bisect control because their shell/process attribution was unknown.
- Real copied application/icon data, a 128-row notification workload, and a synthetic 320-result launcher were each exercised. With current Nacre helper, shell anon by phase was bar 29.1, notifications 33.6, open 48.3 / 41.8 / 41.9, typed 51.5 / 44.0 / 44.3, closed 41.6 / 41.9 / 42.1, expiry 33.9 MiB. Per-mapping `smaps` and rollups are in the task-private capture output. Wallpaper/video did not start (`WALLPAPER_SURFACE_CREATED` only); desktop test host lacks `libmpv`. Confirmed native GL remains unverified because nested renderer did not expose GL transport.
- Historical source was built from git archive 1c9683461857d20bc7e6718dd4ae1bd8724943ec (SHA-256 ca55d85ed60241d6a6ea58312dfcc1cefbc5592bf7ed014f6e8bf442bcf9f7c3) and captured with the B0114 helper hash. The old UI emitted UI_ERROR unknown state panel.open and failed the third cycle; its partial numbers are not comparable evidence of the reported regression.
- Exact B0150 source commit 2d49b069, rebuilt with release flags and the B0114 Nacre helper, returned shell Pss_Anon 25.7 MiB bar; 35.8 open; 45.8 typed; 55.3 closed after cycle 1; 55.4 closed after cycle 2; 54.2 closed after cycle 3; 27.4 after 125 seconds. The copied menu data contained 101 apps. Per-phase gate JSON and per-mapping smaps completed; the wrapper exited nonzero on unrelated widget state-schema UI_ERRORs.
- Each phase stores shell PID rollup and full `smaps`, plus `cache_bytes` and glibc `mallinfo2` heap counters. Captures use the desktop lock, swap/cgroup guards, and the owner-selected nice priority.
- Capture: the B0114 guarded `parity-compare.py` memory harness, extended in the task-private desktop directory to repeat launcher cycles and save full `smaps` per phase. Its private nested `labwc`→`niri` output is set to 2880×1800/1.5.
- CI regression assertion: `spikes/launcher-proof.py` compares post-expiry shell `Pss_Anon` to its initial closed bar; `spikes/tests/test_launcher_memory_proof.py` verifies the 10 MiB verdict and rejects the B0150-sized jump. The real capture has not produced a failing case for this gate.

## Findings

First bad commit / retained object / product fix: undetermined because baseline 900369fb, B0150 source 2d49b069, and current origin/main do not reproduce post-expiry growth. Historical 1c968346 could not reach a valid launcher lifecycle with the available helper/shell setup. No valid bad endpoint exists for bisect. The 10 MiB harness gate and focused verdict tests are added; full Ubuntu 24.04 CI passed. Smoothness was not changed.

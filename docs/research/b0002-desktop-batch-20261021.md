# B0002 desktop batch — 2026-10-21

## Status: stopped before setup or capture

The owner redirected all B0002 measurements to the owner desktop and ruled that laptop and desktop arms must not be mixed. Laptop evidence remains supporting evidence only. The owner desktop preflight found the owner actively using Niri, so setup and captures stopped before any transfer or desktop modification. No desktop measurement has been run, so there is no desktop diagnostic conclusion or full-batch verdict.

## Desktop preflight

Read-only preflight used `ssh -F ~/.ssh/config desktop`. It reported the owner desktop running CachyOS, load average 0.00, 12GiB available memory, and `/dev/zram0` at 15,569,252,352 bytes with 16,805,888 bytes used and priority 100. The owner has an active local Niri Wayland session (`IdleHint=no`) with Quickshell running. This meets the owner's explicit “stop if actively using it” condition. No files were copied, no desktop files/settings were changed, and no capture started.

## Planned desktop artifacts and method

Control A is the owner-reported restored frozen UI binary `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b`; phase-1 B is the same-toolchain rebuilt control `e21caa16cee883008aeceeea4bb438a2b4ee30944e6d9c8bfe4f9626373b1117`; phase-2 B is fixed UI `306474f058c8a8ab044e6153b69b26e8a48bc1714bbcd8d615db20b9a51777bb`; common helper `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`. Desktop artifact locations/hashes were not checked due the active-use stop.

## Laptop supporting evidence only

The laptop completed the fresh Hyprland ten-arm batch with all five alternating pairs valid. Its median summary passed all four phases, but these values are not mixed with the desktop plan: control/candidate medians were 22.576/22.439MiB never-opened, 22.265/22.148MiB opened-once, 22.343/22.301MiB opened-several, and 22.833/22.719MiB combined-legacy. Anonymous median deltas were +0.004, −0.172, −0.141, and −0.129MiB respectively; DRM remained unknown.

Laptop Niri has six valid arms (A1,B1,A2,B2 replacement,A3,B3); the original B2 and A4 were invalidated by host swap-in of 2.367MiB and 5.574MiB. No Niri five-pair medians were computed. No more laptop arms will run for this task.

If resumed after the owner desktop is idle, first run the opened-once diagnostic on Hyprland for three alternating A/B pairs (A frozen, B rebuilt control), then the fresh full batch with frozen A and fixed B, Hyprland five pairs followed by Niri five pairs, all four phases and batch median mode. Preserve the pinned launcher, per-arm resolved environment/argv/hash records, unconditional guarded zram reset, previous-label check, geometry, scope/watchdog, quiet, close, replacement, and median decision gates.

## Checks and commits

Launcher implementation is commit `6185c6edfedabdd9ea506517d7e4aba4c1169599`; invalid-prior evidence guard is `1b1729d6a546ddcbb24330b308aa908b7b86a5c6`; artifact parameterization is `3109905d57d7c4ee9b3524cefaf48842daa8901f`. Laptop code checks: `nice -n 10 python3 -m unittest spikes.tests.test_b0002_arm -v` — 8 passed; `nice -n 10 python3 -m unittest spikes.tests.test_dashboard_capture_windows` — 51 passed. `python3 -m py_compile spikes/b0002-arm.py spikes/tests/test_b0002_arm.py spikes/dashboard-residency.py spikes/launcher-proof.py spikes/tests/test_dashboard_capture_windows.py` and `git diff --check` passed. The requested desktop mock suite and dry runs were not run because the owner was using the desktop.

The full Bead CI workflow was replicated in an Arch Linux container on dev-1 against commit `3109905d57d7c4ee9b3524cefaf48842daa8901f` and finished `CI_EXIT:0`. The disposable container needed Noto/Liberation fonts for the renderer tests and a Git safe-directory entry for the mounted checkout; these were CI-container-only adjustments. The container and tmux session exited. User-owned temporary files were removed; root-owned Cargo/Rust cache directories remain under the unique `/tmp/bead-b0002-ci-*` names because the remote user cannot remove their contents without elevated permissions.

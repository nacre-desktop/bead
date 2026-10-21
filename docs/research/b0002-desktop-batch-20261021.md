# B0002 desktop batch — 2026-10-21

## Status: stopped after Hyprland full batch; Niri not started

The owner redirected all B0002 measurements to the owner desktop and ruled that laptop and desktop arms must not be mixed. Laptop evidence remains supporting evidence only. An initial desktop preflight stopped because the owner was actively using Niri; the lead later resumed after the local tty reported five days idle and load remained below 1. No laptop arms are mixed with the desktop comparison.

## Desktop preflight

Read-only preflight used `ssh -F ~/.ssh/config desktop`. It reported CachyOS, load average 0.00, 12GiB available memory, and `/dev/zram0` at 15,569,252,352 bytes with 16,805,888 bytes used and priority 100. Although the local Niri session reports `IdleHint=no`, `w` shows the local tty idle for five days; it has remained idle during per-arm checks. Every arm checks load <1, owner tty idle >300s, no competing build/capture/nacre-lead job, and takes the exclusive `~/nacre-lead/desktop-heavy.lock` flock.

## Planned desktop artifacts and method

Control A is the restored frozen UI binary `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b`; phase-1 B is the same-toolchain rebuilt control `e21caa16cee883008aeceeea4bb438a2b4ee30944e6d9c8bfe4f9626373b1117`; phase-2 B is fixed UI `306474f058c8a8ab044e6153b69b26e8a48bc1714bbcd8d615db20b9a51777bb`; common helper `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`. Each desktop arm manifest verifies and records its selected A/B and helper hashes.

## Phase 1 — opened-once diagnostic

Hyprland, three alternating pairs, A=frozen `b01dfcca`, B=same-source rebuild `e21caa16`; all six valid. Every arm had a complete scope/runner report, final held OOM read with zero delta, host swap-in 0, SwapPss 0, 2880×1800 / scale1.5 / 120Hz, 60s idle, and four closes below bar+5MiB.

| Arm | PSS+DRM MiB | Anonymous | File | Shared | DRM |
| --- | ---: | ---: | ---: | ---: | --- |
| A1 | 24.853 | 5.500 | 18.681 | 0.672 | unknown |
| B1 | 25.030 | 5.516 | 18.843 | 0.672 | unknown |
| A2 | 25.004 | 5.438 | 18.895 | 0.672 | unknown |
| B2 | 25.283 | 5.484 | 19.127 | 0.672 | unknown |
| A3 | 24.775 | 5.488 | 18.615 | 0.672 | unknown |
| B3 | 25.186 | 5.551 | 18.963 | 0.672 | unknown |

| Side | Total median (min–max) | Anonymous median | File median | Shared median |
| --- | ---: | ---: | ---: | ---: |
| Frozen A | 24.853 (24.775–25.004) | 5.488 | 18.681 | 0.672 |
| Rebuilt control B | 25.186 (25.030–25.283) | 5.516 | 18.963 | 0.672 |
| B−A | +0.333 | +0.027 | +0.282 | 0 |

The earlier +1.94MiB anonymous opened-once excess did not reproduce between the frozen binary and same-source rebuild on this desktop. Anonymous delta is +0.027MiB; total delta is +0.333MiB and mostly file-backed. This argues against attributing the earlier excess to the frozen binary. It is consistent with host/run variation, but does not by itself identify the laptop-vs-desktop cause.

## Phase 2 — Hyprland batch; stopped before Niri

The printed schedule was Hyprland A1,B1,A2,B2,A3,B3,A4,B4,A5,B5, then Niri A1,B1,A2,B2,A3,B3,A4,B4,A5,B5. Hyprland completed all ten valid arms: the initial A1 invocation error was before sampling (priority assertion after double-nice), and the corrected A1 invocation was used as the first control arm. It had sampling-start false, held final OOM read, OOM delta zero, and host swap-in zero. No environmental replacements were used.

Every valid arm has a complete held-scope final OOM read (zero OOM deltas), `memory.max=12GiB`, `memory.swap.max=0`, host swap-in 0, SwapPss 0, 2880×1800 / scale1.5 / 120Hz, 60-second idle windows and 18 closes within bar+5MiB. Unconditional zram reset logs, environment/argv manifests and artifact hashes are in the private desktop raw tree.

| Arm | Never | Once | Several | Combined | Anonymous (N/O/S/C) | File (N/O/S/C) |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| A1 invocation retry | 24.596 | 24.877 | 26.463 | 25.640 | 5.023/5.488/7.074/6.211 | 18.900/18.717/18.717/18.757 |
| B1 | 25.004 | 25.011 | 25.280 | 25.661 | 5.023/5.434/5.703/6.074 | 19.309/18.905/18.905/18.915 |
| A2 | 24.753 | 24.828 | 25.116 | 25.683 | 5.012/5.477/5.688/6.227 | 19.069/18.680/18.757/18.784 |
| B2 | 25.051 | 25.355 | 26.762 | 25.653 | 5.051/5.430/7.199/6.023 | 19.328/19.254/18.891/18.958 |
| A3 | 24.683 | 26.425 | 25.062 | 25.520 | 5.051/7.094/5.730/6.145 | 18.960/18.659/18.659/18.703 |
| B3 | 24.854 | 24.765 | 25.144 | 25.687 | 5.008/5.262/5.641/6.156 | 19.175/18.831/18.831/18.858 |
| A4 | 24.817 | 24.800 | 25.024 | 27.310 | 5.047/5.562/5.785/7.719 | 19.099/18.565/18.567/18.919 |
| B4 | 24.687 | 25.019 | 26.714 | 25.694 | 5.000/5.492/7.188/6.141 | 19.015/18.854/18.854/18.882 |
| A5 | 24.788 | 26.325 | 25.027 | 25.578 | 5.023/7.027/5.727/6.211 | 19.093/18.626/18.629/18.695 |
| B5 | 24.874 | 24.886 | 24.956 | 26.956 | 5.031/5.523/5.672/7.590 | 19.171/18.690/18.612/18.694 |

Category figures are per-arm PSS anonymous/file/shared medians (MiB); shared was 0.672MiB in every phase. DRM attribution is **unknown** for all arms and remains unknown; graphics are not classified as file-backed.

| Phase | Control median (min–max) | Candidate median (min–max) | Control→candidate anon/file/shared medians | Δ anon/file/shared | Result |
| --- | ---: | ---: | --- | --- | --- |
| Never-opened | 24.753 (24.596–24.817) | 24.874 (24.687–25.051) | 5.023/19.069/0.672 → 5.023/19.175/0.672 | 0.000/+0.105/0.000 | PASS (<25) |
| Opened-once | 24.877 (24.800–26.425) | 25.011 (24.765–25.355) | 5.562/18.659/0.672 → 5.434/18.854/0.672 | −0.129/+0.195/0.000 | 25–27 allowance not applied: DRM unknown |
| Opened-several | 25.062 (25.024–26.463) | 25.280 (24.956–26.762) | 5.730/18.659/0.672 → 5.703/18.854/0.672 | −0.027/+0.195/0.000 | STOP: DRM unknown; file delta +0.195 does not cover +0.218 total-median delta |
| Combined | 25.640 (25.520–27.310) | 25.687 (25.653–26.956) | 6.211/18.757/0.672 → 6.141/18.882/0.672 | −0.070/+0.125/0.000 | 25–27 allowance not applied: DRM unknown |

The batch summarizer stopped after the final Hyprland arm with `AssertionError: 25–27 MiB exception requires measured DRM category on both sides`. This is the required conservative outcome: a 25–27MiB median cannot pass without proving the increase is file-backed, and DRM is unmeasured. Opened-several also has a +0.218MiB candidate-control total median delta versus +0.195MiB file-PSS median delta. Although anonymous medians decreased, the category/total mismatch cannot be resolved by assigning unknown DRM to file-backed memory. **Hyprland result: partial PASS for never-opened; STOP on opened-several allowance gate. Niri was not run.**

## Laptop supporting evidence only

The laptop completed the fresh Hyprland ten-arm batch with all five alternating pairs valid. Its median summary passed all four phases, but these values are not mixed with the desktop plan: control/candidate medians were 22.576/22.439MiB never-opened, 22.265/22.148MiB opened-once, 22.343/22.301MiB opened-several, and 22.833/22.719MiB combined-legacy. Anonymous median deltas were +0.004, −0.172, −0.141, and −0.129MiB respectively; DRM remained unknown.

Laptop Niri has six valid arms (A1,B1,A2,B2 replacement,A3,B3); the original B2 and A4 were invalidated by host swap-in of 2.367MiB and 5.574MiB. No Niri five-pair medians were computed. No more laptop arms will run for this task.

## Checks and commits

Launcher implementation is commit `6185c6edfedabdd9ea506517d7e4aba4c1169599`; invalid-prior evidence guard is `1b1729d6a546ddcbb24330b308aa908b7b86a5c6`; artifact parameterization is `3109905d57d7c4ee9b3524cefaf48842daa8901f`. The desktop harness mock suite and launcher dry runs passed (`mocks.log`: 61 tests, OK). Laptop code checks also passed: `nice -n 10 python3 -m unittest spikes.tests.test_b0002_arm -v` — 8 passed; `nice -n 10 python3 -m unittest spikes.tests.test_dashboard_capture_windows` — 51 passed. `python3 -m py_compile spikes/b0002-arm.py spikes/tests/test_b0002_arm.py spikes/dashboard-residency.py spikes/launcher-proof.py spikes/tests/test_dashboard_capture_windows.py` and `git diff --check` passed.


The full Bead CI workflow was replicated in an Arch Linux container on dev-1 against commit `3109905d57d7c4ee9b3524cefaf48842daa8901f` and finished `CI_EXIT:0`. The latest code is `7b1015f`; a rerun was not started because dev-1 was occupied by several concurrent builds/containers (observed load 2.69/9.31/16.75), so latest-commit full CI remains outstanding. The earlier disposable container needed Noto/Liberation fonts for renderer tests and a Git safe-directory entry; these were CI-container-only adjustments. The container and tmux session exited. User-owned temporary files were removed; root-owned Cargo/Rust cache directories remain under the unique `/tmp/bead-b0002-ci-*` names because the remote user cannot remove their contents without elevated permissions.

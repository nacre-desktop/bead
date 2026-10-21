# B0002 desktop batch — 2026-10-21

## Status: complete — Hyprland and Niri passed the desktop relative gate

The owner redirected all B0002 measurements to the owner desktop and ruled that laptop and desktop arms must not be mixed. Laptop evidence remains supporting evidence only. An initial desktop preflight stopped because the owner was actively using Niri; the lead later resumed after the local tty reported five days idle and load remained below 1. No laptop arms are mixed with the desktop comparison.

## Desktop preflight

Read-only preflight used `ssh -F ~/.ssh/config desktop`. It reported CachyOS, load average 0.00, 12GiB available memory, and `/dev/zram0` at 15,569,252,352 bytes with 16,805,888 bytes used and priority 100. Although the local Niri session reports `IdleHint=no`, `w` shows the local tty idle for five days; it has remained idle during per-arm checks. Every arm checks load <1, owner tty idle >300s, no competing build/capture/nacre-lead job, and takes the exclusive `~/nacre-lead/desktop-heavy.lock` flock.

## Planned desktop artifacts and method

Control A is the restored frozen UI binary `b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b`; phase-1 B is the same-toolchain rebuilt control `e21caa16cee883008aeceeea4bb438a2b4ee30944e6d9c8bfe4f9626373b1117`; phase-2 B is fixed UI `306474f058c8a8ab044e6153b69b26e8a48bc1714bbcd8d615db20b9a51777bb`; common helper `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`. Each desktop arm manifest verifies and records its selected A/B and helper hashes.

## Phase 1 — opened-once diagnostic

Hyprland, three alternating pairs, A=frozen `b01dfcca`, B=same-source rebuild `e21caa16`; all six valid. Every arm had a complete scope/runner report, final held OOM read with zero delta, host swap-in 0, SwapPss 0, 2880×1800 / scale1.5 / 120Hz, 60s idle, and four closes below bar+5MiB.

| Arm | PSS MiB (DRM unknown) | Anonymous | File | Shared | DRM |
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

## Phase 2 — full desktop batch

The printed schedule was Hyprland A1,B1,A2,B2,A3,B3,A4,B4,A5,B5, then Niri A1,B1,A2,B2,A3,B3,A4,B4,A5,B5 (A-first). Hyprland's initial A1 wrapper invocation failed before sampling due to the extra nice increment; its corrected A1 retry is the valid first control. The other 19 arms and corrected A1 passed. No environmental replacements were used.

The spec-agent allowance record `af14fc7` was superseded for this non-reference desktop by the owner rule at `f58558e`: compare each phase's candidate median to the frozen control median on the same host. PASS when the delta is ≤+0.5MiB and anonymous growth is ≤1MiB. Absolute 25/26.5MiB limits apply to the laptop reference host, not this desktop. When any endpoint has unknown DRM, the comparison basis is measured PSS only; DRM remains explicitly unknown and is excluded, never imputed as zero or file-backed.

All 20 valid arms have complete held-scope final OOM reads (zero OOM deltas), `memory.max=12GiB`, `memory.swap.max=0`, host swap-in 0, SwapPss 0, 60-second idle windows and 18 closes within bar+5MiB. Hyprland geometry was 2880×1800, scale1.5, virtual120Hz; Niri was 2880×1800, scale1.5, nested60Hz. Every arm recorded its unconditional zram reset, environment/argv, hashes and raw evidence in the private desktop tree. DRM was unknown throughout.

| Hyprland arm | Never | Once | Several | Combined | Anonymous (N/O/S/C) | File (N/O/S/C) |
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

| Niri arm | Never | Once | Several | Combined | Anonymous (N/O/S/C) | File (N/O/S/C) |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| A1 | 26.496 | 25.705 | 25.920 | 27.759 | 5.758/5.820/6.035/7.812 | 19.586/18.801/18.801/18.862 |
| B1 | 26.568 | 25.852 | 26.074 | 26.498 | 5.758/5.789/6.012/6.414 | 19.658/18.979/18.979/19.000 |
| A2 | 26.559 | 25.697 | 25.818 | 26.341 | 5.738/5.797/6.043/6.414 | 19.668/18.816/18.691/18.843 |
| B2 | 26.862 | 26.139 | 26.334 | 28.240 | 5.766/5.777/5.973/7.852 | 19.944/19.277/19.277/19.305 |
| A3 | 26.458 | 25.804 | 25.974 | 27.828 | 5.762/5.863/6.035/7.848 | 19.544/18.856/18.854/18.896 |
| B3 | 26.576 | 25.770 | 25.918 | 29.179 | 5.762/5.805/5.953/9.129 | 19.662/18.881/18.881/18.966 |
| A4 | 26.523 | 25.779 | 26.478 | 27.715 | 5.816/5.867/6.199/7.785 | 19.555/18.828/19.194/18.846 |
| B4 | 27.054 | 26.098 | 26.333 | 26.717 | 5.777/5.734/5.977/6.332 | 20.124/19.279/19.272/19.301 |
| A5 | 26.399 | 25.666 | 25.886 | 26.409 | 5.785/5.762/5.984/6.484 | 19.462/18.820/18.817/18.841 |
| B5 | 26.733 | 25.954 | 26.149 | 26.525 | 5.762/5.793/5.988/6.434 | 19.819/19.077/19.077/19.008 |

Candidate-control medians (PSS only; DRM unknown) and category median changes:

| Compositor / phase | Control median (min–max) | Candidate median (min–max) | Δ total | Δ anon | Δ file | Δ shared | Verdict |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Hyprland / never | 24.753 (24.596–24.817) | 24.874 (24.687–25.051) | +0.121 | 0.000 | +0.105 | 0.000 | PASS |
| Hyprland / once | 24.877 (24.800–26.425) | 25.011 (24.765–25.355) | +0.134 | −0.129 | +0.195 | 0.000 | PASS |
| Hyprland / several | 25.062 (25.024–26.463) | 25.280 (24.956–26.762) | +0.219 | −0.027 | +0.195 | 0.000 | PASS |
| Hyprland / combined | 25.640 (25.520–27.310) | 25.687 (25.653–26.956) | +0.047 | −0.070 | +0.125 | 0.000 | PASS |
| Niri / never | 26.496 (26.399–26.559) | 26.733 (26.568–27.054) | +0.237 | 0.000 | +0.265 | 0.000 | PASS |
| Niri / once | 25.705 (25.666–25.804) | 25.954 (25.770–26.139) | +0.249 | −0.031 | +0.257 | 0.000 | PASS |
| Niri / several | 25.920 (25.818–26.478) | 26.149 (25.918–26.334) | +0.229 | −0.059 | +0.260 | 0.000 | PASS |
| Niri / combined | 27.715 (26.341–27.828) | 26.717 (26.498–29.179) | −0.998 | −1.352 | +0.162 | 0.000 | PASS |

All eight relative deltas are ≤+0.5MiB and all anonymous median deltas are ≤+1MiB. The desktop baseline is therefore accepted under `f58558e`; the previous one-time absolute allowance is not needed for this non-reference host. The high Niri combined absolute values are host baseline variation and are not a stop condition.

## Laptop supporting evidence only

The laptop completed the fresh Hyprland ten-arm batch with all five alternating pairs valid. Its median summary passed all four phases, but these values are not mixed with the desktop plan: control/candidate medians were 22.576/22.439MiB never-opened, 22.265/22.148MiB opened-once, 22.343/22.301MiB opened-several, and 22.833/22.719MiB combined-legacy. Anonymous median deltas were +0.004, −0.172, −0.141, and −0.129MiB respectively; DRM remained unknown.

Laptop Niri has six valid arms (A1,B1,A2,B2 replacement,A3,B3); the original B2 and A4 were invalidated by host swap-in of 2.367MiB and 5.574MiB. No laptop Niri five-pair medians were computed. No more laptop arms ran for this task.

## Checks and commits

Code commits are `4a5c5e3` (the spec-agent file-backed allowance implementation, superseded on desktop by the owner rule) and `0e3843f` (non-reference relative gate), both with `[no-publish]`. Updated desktop mocks passed: `nice -n 10 python3 -m unittest discover -s spikes/tests -p test_dashboard_capture_windows.py -v` — 53 passed; `nice -n 10 python3 -m unittest spikes.tests.test_b0002_arm -v` — 10 passed; `nice -n 10 python3 -m py_compile spikes/dashboard-residency.py spikes/dashboard-batch-summary.py spikes/tests/test_dashboard_capture_windows.py` passed. The same unittest/compile commands passed locally; `nice -n 10 git diff --check` passed. Desktop summaries were recomputed from the raw evidence using `relative_host=True`.


The full Bead CI workflow from `.github/workflows/ci.yml` was replicated in `ghcr.io/archlinux/archlinux:latest` on dev-1 against code commit `0e3843f2d94ace3f16510ffef454612b2447dd87`; final log result was `CI_EXIT:0` (`desktop-run/ci-0e3843f.log`). The first pass exposed a missing `/root/.cargo/bin` directory for the pinned cargo-deny install; it was added inside the disposable container and the full workflow reran successfully. The container and checkout were removed afterward. Noto/Liberation fonts and Git safe-directory were container-only setup requirements.

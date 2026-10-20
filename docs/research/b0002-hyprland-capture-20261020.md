# B0002 Hyprland capture result — 2026-10-20

## Outcome

The accepted A1 control prefix passed all four required settled phases. B1 passed never-opened, opened-once and opened-several, then correctly stopped at combined-legacy: settled UI PSS+DRM+SwapPss was 24.5419921875 MiB, reaching the owner's 24.5 MiB stop line. This leaves the requested paired matrix incomplete and B0002 NOT PASS. The baseline control for combined-legacy was 24.0107421875 MiB. The difference is a candidate-side signal from one pair only; it does not establish causation or a product regression.

The combined-legacy phase is explicitly part of the B2 accepted matrix (spec steps 2 and acceptance criterion 4), not only a legacy harness rail. The harness computes total(row) as row["pss_plus_drm_mib"] + row["memory"]["swap_pss_mib"]; the stop assertion is assert total(row) < 24.5 in spikes/dashboard-residency.py:165. The combined settled JSON is saved at line 127 before that assertion, while progress.json is saved at line 166 only after it. Thus B1's raw endpoint exists despite progress.json containing only three phases.

## Settled endpoints

Values are MiB, total includes UI PSS+DRM and UI SwapPss. Each settled window had at least 60 seconds idle. A1 baseline warmup/idle were 600.0000866/60.0003067 seconds. B1 baseline warmup/idle were 600.0002941/60.0003471 seconds.

| Phase | A1 control | B1 candidate | B1 idle seconds | B1 categories: anon / file / shmem / SwapPss |
| --- | ---: | ---: | ---: | --- |
| never-opened | 23.698242188 | 23.826171875 | 60.000314124 | 5.906250 / 17.248047 / 0.671875 / 0 |
| opened-once | 23.705078125 | 23.783203125 | 60.000289397 | 5.851563 / 17.261719 / 0.671875 / 0 |
| opened-several | 23.702148438 | 24.035156250 | 60.000278343 | 6.042969 / 17.320313 / 0.671875 / 0 |
| combined-legacy | 24.010742188 | **24.541992188 — STOP** | 60.000314168 | 6.210938 / 17.659180 / 0.671875 / 0 |

The B1 combined-legacy-close-5 sample is a transient close observation (1.000029 seconds), not a settled endpoint: 24.606445 MiB. It passed the separate <=25 MiB close rail. Do not substitute it for the settled phase result.

External helper membership remained stable and external SwapPss was zero. B1 safety evidence records swap.max=0, 12 GiB memory cap, no OOM, host swap-in 0, and minimum MemAvailable 12,632,316 KiB (B1 safety record; A1 recorded 12,707,468 KiB). Geometry was 2880×1800, scale 1.5, 120 Hz. The phase plan used 600-second warmup, 60-second baseline idle, and 65/85/105/95-second phase budgets with full >=60-second idle samples. Both arms used the same lifecycle-only menu-engine route and external helper observer; dashboard enabled was the intentional A/B condition.

## Source and cleanup

Frozen UI source: 1cff9d17d61fb05451546bb0ea13c826c93cb4c9; UI SHA-256 b01dfccaac148b984a6e1089f7a54d41bced473455b3d41890d15ec074a8038b. UI shell tree ID 0c32b0fd6fb8487a09c0e30a2932c29b7a72c286; dashboard fixture tree ID 51dbf56ff49bf55656d7f04257f532dde73ece47.

Helper source: 5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57; helper SHA-256 4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa. Capture harness source: c807591aa0d47e0d1a87bf10dc25c9a0d02c577e. Both arms used Hyprland nested at the geometry above; lifecycle-only BEAD_HARDENING=menu-engine, BEAD_ENGINE_LIFECYCLE_ONLY=1, frozen UI bundle, common helper and fixture tree. No provider flag was set.

The strict corrected postcheck validated A1 without editing its raw files; the earlier rejection was solely a validator field-name mismatch (baseline_timing.idle_seconds versus captured baseline_idle_seconds). B1's failing endpoint is a true settled-value failure, not a validator mismatch. Controller cleanup evidence shows the owned scope inactive/dead with an empty ControlGroup, no owned capture/UI/helper/compositor process, and the controller lock free. A1 remains an accepted immutable prefix; B1 remains invalid and immutable. No retry, rebuild or threshold change was performed.

## Bounded source hypothesis

crates/nacre-shell/src/dashboard.rs:125-163 creates a page Runtime and attaches its sources; dashboard_tests.rs:529-621 verifies each Runtime drop releases its source leases and callbacks. crates/bead-wayland/src/panel.rs:457-488 retains then releases the panel after the 2200 ms close grace; release_panel drops panel Runtime/companion and trims at panel.rs:491-551. The combined phase follows dashboard page cycles with legacy calendar/sound/network/bluetooth/battery/launcher navigation. Existing tests prove per-page Runtime retirement and launcher mirror release separately, but do not prove that the mixed dashboard-to-legacy sequence retires each old companion Runtime before the next factory or leaves no page callback/lease alive after settling.

Smallest hypothesis to test: a stale dashboard companion/view owner or queued page work survives the page-to-legacy transition long enough to contribute to the final settled anonymous footprint. This is unproven; the single paired endpoint cannot identify source or causality. Add a deterministic lifecycle test around the existing panel/view event path that performs dashboard page open/close, then a legacy view open/close, and asserts the previous Runtime's source lease counts return to baseline, callbacks stop, companion surfaces are zero, and the new view remains functional after the 2200 ms retirement. Reuse fake sources and the existing fake event loop/clock; do not add measurement hooks. If those assertions already hold, this hypothesis is falsified and the next diagnosis should compare persisted anonymous/file/shared categories across multiple authorized pairs.

## Raw evidence

Raw files and SHA-256 values, including A1/B1 endpoint JSON, matching smaps, safety records, controller status and logs, are listed in the private evidence note adjacent to the private capture bundle. Raw artifacts remain outside Git. The B1 combined endpoint SHA-256 is 10b811d65794667abbf6a16ff80e330ead6fa4e1cb3931f1b5b452ae4d48524c; A1 combined endpoint SHA-256 is a78130f42993d1f50ea3cae4adf47ecb6cb7c72f263076c8d3768d9082ab8a47.

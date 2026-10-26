# B0135 memory cohort

Five alternating CPU-rendered control/candidate pairs (7–11) were measured in private nested sessions. The gated sum is anonymous PSS + shared-memory PSS + SwapPss + DRM; total PSS and file-backed PSS are informational. Each pair used fresh core, clipboard and picker lifecycles, with 12 seconds after close. Pair-level values and components are in [memory.json](memory.json).

Source audit: PASS (zero errors). The paired builds share the same baseline apart from the measured scrolling changes; exact source, binary, harness and test hashes are recorded in the JSON. This cohort uses candidate tree `203f19d8`, before the later non-allocating AI-follow fix and main idle-preparation guard. None of these lifecycles opens AI chat; the cohort does not measure that app or the full integrated main build. Pairs 1–2 remain excluded diagnostics (incomplete history fixture and unregistered clipboard view).

| Phase | Pairs | Median Δ MiB | Original 0.5 MiB gate | Separate feature budget |
|---|---:|---:|---|---|
| idle | 5/5 | +0.2031 | PASS | — |
| scroll-settings-content-touchpad/open | 5/5 | +0.0664 | PASS | — |
| scroll-settings-nav-touchpad/open | 5/5 | +0.0391 | PASS | — |
| scroll-notifications-list-touchpad/open | 5/5 | +1.4844 | FAIL | 1 MiB: FAIL |
| scroll-notifications-popup-touchpad/open | 5/5 | +1.2578 | FAIL | — |
| scroll-settings-content-touchpad/closed+12s | 5/5 | +0.0312 | PASS | — |
| scroll-settings-nav-touchpad/closed+12s | 5/5 | +0.1055 | PASS | — |
| scroll-notifications-list-touchpad/closed+12s | 5/5 | +0.2578 | PASS | — |
| scroll-notifications-popup-touchpad/closed+12s | 5/5 | -0.5000 | PASS | — |
| clipboard/idle | 5/5 | +0.0703 | PASS | — |
| clipboard/open | 5/5 | +2.1758 | FAIL | 1 MiB: FAIL |
| clipboard/closed+12s | 5/5 | +0.4258 | PASS | — |
| picker/idle | 5/5 | -0.0273 | PASS | — |
| picker/open | 5/5 | -36.6055 | PASS | — |
| picker/closed+12s | 5/5 | -1.4961 | PASS | — |

The original 0.5 MiB gate fails for notifications-list open (+1.4844 MiB), notifications-popup open (+1.2578 MiB), and clipboard open (+2.1758 MiB). Notifications-list and clipboard open also exceed their separate 1 MiB feature budgets. The other listed phases pass the original gate.

The failed open-view deltas are mainly anonymous memory. Median component
deltas (MiB) are history anon +1.3516 / shmem +0.1328, popup anon +1.0820 /
shmem +0.1758, and Clipboard anon +2.1758 / shmem +0.0000. DRM and SwapPss are
zero in these CPU samples. File PSS deltas are −0.0645/−0.0254/−0.0254 MiB,
respectively, and are not gated. Cache versus allocator attribution is incomplete.

Separate application memory is not part of the always-on shell gate:
- `scroll-settings-content-touchpad/settings`: 5/5, median unreclaimable Δ +12.0312 MiB; median component deltas Pss_Anon +6.8985, Pss_Shmem +5.1524, SwapPss +0.0000, DRM +0.0000, file-backed PSS +0.0264 MiB.
- `scroll-settings-nav-touchpad/settings`: 5/5, median unreclaimable Δ +7.3242 MiB; median component deltas Pss_Anon +2.0781, Pss_Shmem +5.2461, SwapPss +0.0000, DRM +0.0000, file-backed PSS +0.0410 MiB.

Component medians need not sum to the median total. These differences are measured residency, not a causal allocation attribution. The native-GL 80-wallpaper reopen stress is a separate backend diagnostic in [the picker evidence](../wallpaper-picker/203-gl80-reopen.md); it is not part of this CPU cohort. Package-list scrolling and daemon/provider totals were not measured.

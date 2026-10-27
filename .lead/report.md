Status: DONE for the two dropdown regressions; focused matrix passes. Full sequential acceptance still has separate route/fixture gaps below.
Code fix `be031320` and report updates are pushed with `[no-publish]`; prior B0161 report remains in parent `1f99b426`.
Measure: `PanelVisibility(true)` to first Wayland presentation feedback; calibrated 65 ms limit on nested 60 Hz TEST host.
Baseline `900369fb`: seven available dropdowns, 21/21 closes PASS; p50/p99 64.200/64.746 ms. Bluetooth unavailable (no private service).
Fixed main: eight dropdowns, 24/24 closes PASS; all-widget p50/p99 52.154/60.932 ms (matched seven: 52.017/60.932); all closed screenshots pixel-identical.
Memory, CPU and swap: baseline/fixed full arms pass; fixed Pss_Anon +13.758 MiB after idle and +18.723 MiB after repeat (<20 MiB limit).
Bisect: `ec6cd311` first bad; parent `860aa794` passes. Reveal ticks reshaped saved primitives, invalidating CPU snapshots; canonical open-state content reuse removes repeated ~270 ms opens and stale pixels.
Blank layer calibration accepts only known Top/Background/Overlay identities with pixel-identical closes; post-idle inventory returns to six layers.
Full sequential arm still has launcher/dashboard/control/wallpaper route and AI-hover fixture gaps on both revisions; these remain unverified, not passes.
Remote Bead full-CI recipe on exact code passed: `BEAD_CI_AND_RELEASE_ARMS_PASS` (exit 0).
Official GitHub CI run `40568352722` passed pushed HEAD `a76ac159`; remote dev-2 full CI also passed the exact code and report commit.
No laptop build, test, install or configuration changes.

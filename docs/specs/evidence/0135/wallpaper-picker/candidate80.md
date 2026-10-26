# Candidate80 hexagon picker diagnostic

The final 68MiB-frame candidate completed the 80-image Bead picker capture with no `UI_ERROR`. The `extras.wallpaper.layout` state was `hexagons`, and `extras.scroll` moved `0 → 518 → 0` over a finger gesture and reversal. The 160 input events were delivered at 8.02ms p95. The paired control80 setup had previously exceeded the 64MiB raster limit before any scroll input, so this candidate capture has no valid control timing counterpart.

The one-repeat CPU diagnostic still misses the smoothness targets: renderer p95 was 54.763ms, the presented all-gap p95 was 584.45ms, and input-to-presented reversal was329.53ms. There were14 commits,15 presented frames,2 discarded feedback records, and `refresh_ns=0`; the timing does not prove physical120Hz scanout. Capture safety passed with zero UI errors. The generated 80-image fixture remains a stress case and this single run does not satisfy any five-pair memory gate.

At scale1.5 on a1920×1200 logical /2880×1800 physical output, the private fixture uses1600×900 generated RGB files. Its one-run shell unreclaimable components (`Pss_Anon`+`Pss_Shmem`+DRM+SwapPss) were39,644KiB idle,182,974KiB open,118,962KiB after close+12s. Treat these as one-arm diagnostic measurements, not a memory gate.

| Pin | Value |
|---|---|
| Rust binary | `d0376ab37bf9a1f6af8d503be6f07e8c3638f412ea5dd649f56d0e6bba90a1dd` |
| Unit bundle source / shell tree | `f3273808910eca0b2bd4a7ee7d9c35071c9ecd0f` / `52a5777ab4f7a86a9ceaefb8d0024ad267170eca` |
| Harness / tests | `1ae3dac417edac2f848b023dfafc3a769e7673e379cfe72b66753de046a42f15` / `ba9bbce9cc1cb89004dfb0106a9e51858917cd871fea44d3e88cd553a74f564b` |
| Nacre / Orient | `37583d61928368bc22a46aa806fd0a2e06cfee55` / `3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8` |
| Scroll endpoints | [before](screenshots/candidate80-touchpad-before.png), [after](screenshots/candidate80-touchpad-after.png) |

The endpoints return to offset0 by the end of the reversing gesture. The joined state trace supplies the scroll extent and visible direction-change evidence.

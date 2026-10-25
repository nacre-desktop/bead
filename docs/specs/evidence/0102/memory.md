# B0102 memory evidence

Backend: CPU; matched no-default-feature debug/stripped builds, 2880×1800 at scale 1.5.

sum of each captured phase's component medians: Pss_Anon + Pss_Shmem + DRM + SwapPss; all stored in KiB, converted to MiB; Pss_File is informational only

| State/sample | Control median MiB | Candidate median MiB | Paired delta median MiB |
|---|---:|---:|---:|
| idle:open | 31.1055 | 31.2812 | +0.2422 |
| idle:closed | 31.1328 | 31.1094 | -0.0039 |
| dashboard-0:open | 49.6289 | 57.1797 | +7.5508 |
| dashboard-0:closed | 31.6719 | 31.9531 | +0.3555 |
| dashboard-1:open | 50.0820 | 50.9414 | +0.9531 |
| dashboard-1:closed | 32.1367 | 33.2070 | +1.0547 |
| dashboard-2:open | 53.2344 | 53.7500 | +0.7109 |
| dashboard-2:closed | 32.2539 | 33.3477 | +0.9453 |
| dashboard-3:open | 51.5273 | 50.4336 | +1.4727 |
| dashboard-3:closed | 32.4844 | 33.6641 | +1.1953 |

Pss_File values are reported separately in memory.json and do not enter the gated sum.

The delta column is the median of the three paired differences, not the difference
of the two group medians. The +0.5 MiB per-phase relative gate FAILS.

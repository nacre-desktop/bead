# B0160: Bound large CPU snapshots

The live one-thread spin backtrace ends in the CPU snapshot worker's `cache_content` path.
Source inspection found no non-terminating cache-admission loop; sparse row compression was the unbounded pixel scan in that path.
Large, high-entropy wallpaper composites could scan millions of pixels and grow a temporary RLE representation before choosing dense storage.
This unit caps sparse-compression work at 4,194,304 pixels; larger snapshots use the already budgeted dense representation.

| Measure | Bound / result |
|---|---:|
| Dense composite | 64 MiB maximum |
| Sparse scan | 4,194,304 pixels/job |
| Large snapshot test | 4320×2700; ≤15 s; dense ≤64 MiB |

The reported owner laptop spin was not reproduced on the private desktop before the interruption. The root cause remains a source-supported hypothesis; full CI and nested real-wallpaper restart evidence are required before attributing the incident conclusively.

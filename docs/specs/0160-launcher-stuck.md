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

The live one-thread spin itself remains unreproduced. A matched private nested launcher run with the copied app catalog measured 9d266afb and current main at about 488 ms first content, with 19 presented frames on both; this does not reproduce the owner's no-animation report. The B0151 control-panel regression did reproduce: Escape closed on 9d266afb but not 5466495a, with outside click closing both. Commit 4c206d51 had made the control panel text-focus-only; B0160 restores its keyboard interactivity. The exact spin trigger remains unconfirmed.

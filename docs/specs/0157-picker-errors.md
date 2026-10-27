# B0157 — Wallpaper picker errors

4K picker activation may already consume the view's retained-byte allowance before the CPU fallback is accepted.
An optional flattened CPU snapshot must not turn budget pressure into an activation error.
Spotlight animation should inspect only its visible neighbors, regardless of catalogue size.
Open cadence, GL/CPU stress, and the 40 MiB picker budget remain acceptance gates; measurements are pending.

| Path | Change | Evidence |
|---|---|---|
| CPU snapshot accept | Drop only the optional flattened cache on native-budget rejection; preserve the prepared CPU layers | Regression test fills the 40 MiB budget |
| Spotlight slide | Limit Luau row traversal to the visible neighborhood | Full remote CI and private desktop stress pending |
| Open cadence | No change yet | B0147 remains the baseline; no improvement claimed |

Temporary duplicates: none. No new kit primitive or dependency.

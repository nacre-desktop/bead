# Daily acceptance result — wallpaper open only

| Check | Result | Evidence |
|---|---:|---|
| harness execution | FAIL | detail=RuntimeError: host swap used is at least 64 MiB; stop between arms, ensure no build or capture is running and MemAvailable > swap_used + 8 GiB, then use the authorized `sudo swapoff -a && sudo swapon -a` reset, log free -h before/after, preserve swap devices/capacity, and recheck <64 MiB |

Failure screenshots: `<TASK>/evidence/cold-open-only-private-service-final-actual/failures`

Runtime calibration: bar points use the standard B0152 layout defaults; use `--profile` if the staged user has a different order.
Surface lifecycle: immediate close is checked with `niri msg layers`; the 120-second idle separately checks that the shell namespace returns to its pre-open mapped-layer count.

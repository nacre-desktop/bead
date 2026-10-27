# B0151 — Widgets finish slice

Occupied bar slots now persist desktop-clock drops at 82% and 90% bar width.
The drag ghost retires after a rejected transaction; hover prewarming resumes when the pointer leaves the dropped widget.
The default content bar remains byte-identical to its prior golden.
The functional and relative first-key CPU gates pass; 60 Hz snap smoothness remains a release blocker.

| Proof | Result |
| --- | --- |
| Drop and functional states | 82% and 90% file/store drops persist; six-state drag, app, resize, pearls and focus proofs pass. |
| Full remote CI | Bead CI-equivalent passes at `96e1754a`; 750 tests and optimized release build. |
| First-key UI CPU | Exact B0153 shell-only harness: main 28.187 ms, candidate 27.602 ms, paired median Δ −0.175 ms (limit +1 ms). |
| After-drag/close memory | Five alternating pairs, CPU, 2880×1800/1.5: shell unreclaimable medians 42.053 / 42.402 MiB, paired Δ +0.232 MiB. All widgets processes exited after close. |
| 60 Hz snap | Fails smoothness: candidate overlay first-visible p50 is about 24 ms later than main; bar p99 gaps and dropped commits are also worse despite earlier first paint. |
| 120 Hz | Not measured: nested Niri remained at 60 Hz. |

The old `+378 MiB` value is invalid as a shell-only delta: its earlier runner enabled the after-drag phase for the candidate but not main. The old aggregate cannot be reconstructed because its raw inputs were cleaned. The exact B0153 method reads the Bead shell PID's smaps rollup and reports other processes separately; in pair 1, excluded compositor/Niri/capture-runtime/daemon rows sum to about 400 MiB while shell rows are about 46 MiB (main) and 38 MiB (candidate). This explains the scale of the old figure, not its exact arithmetic. Across the final-code five-pair run, the after-drag paired median is +0.232 MiB; phase medians are bar +0.207, open +0.445, typed −0.330, closed +0.744, expired +0.121, and desktop-active −0.625 MiB. The 2 MiB widget budget is met; the noisy closed-phase median is reported.

The matched CPU snap capture used synthetic 60 Hz scheduling in a private nested Niri session, not physical scanout. Final-code repeats remained less smooth than main on overlay timing and bar tail gaps, so this slice is not pushed. Existing paired default-bar evidence is in [B0113 evidence](../evidence/0113/idle-pair.png); preview-stage visual polish remains B0152.

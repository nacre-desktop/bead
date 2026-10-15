# Status

## 2026-10-15 — Stopped at Gate C

Feature 7 and one real sound-output panel (Feature 8) are implemented.
Gate C report: [measurements and limits](research/feature8-gate-c.md).
Recommendation: adjust pacing acceptance; owner decision pending.
Do not start the optimization pass or Feature 9.

Source fdc99cd is on main; CI 38719326494 passed. Local 70 tests,
fmt/clippy, CPU-only check, facade/headless/types/protocol proofs pass.
CI also links CPU-only, checks all features and licenses. Final native
reload/state/reopen/SIGKILL proofs pass both compositors. Report proved
30.001 s, numeric-only 0600, with closed-view frame history.

Strict memory gate passes every cycle, closed 60 s and post-lifecycle:

- Hypr bar 21.35 MiB; closed 11.13 +4.79 SwapPss; final 16.05 +2.67 swap.
- niri bar 22.64 MiB; closed 26.13 (+3.50); final 24.56 (+1.92), no swap.
Warm native resident+swap 29.31/32.75 MiB vs QML 163.09/159.07.
Feature 6 ~105 MiB was a larger GL stress panel; comparison limits and
6b's CPU+GL warm increase are explained in the report. Every reopen has
visible-track pixel validation. Prior blank-body and +5.13/+5.03 failures
are preserved, not used as passing visual evidence.

Native opening/closing p99 ms: Hypr 8.56/8.74; niri 41.34/47.27.
Nested niri pacing and 6b step 0a idle invariance remain open. Keep typing
CPU 29.6→32.7% on the later optimization list. Hypr output move/removal
passes; real niri hotplug, physical scanout, IME/Orca/NVIDIA unvalidated.
Native controls/core+ext clipboard, all dismissals, rapid latest-intent
visibility and legacy state work; no microphone/device-list parity claim.

Worktree: ~/.local/share/nacre/ai/worktrees/bead-feature7-gate-c.
Canonical checkout/reference repos remain untouched. Shared target stays
/home/siverteh/Projects/bead/target. Evidence: canonical .research/feature8.
Nested nice10/Cargo-j2 only; no live/physical test, install or release.

# B0131 — Warm covered pearl release

**Superseded admission policy:** [B0139](0139-prewarm-toplevel.md) fixes a live
regression from admitting mapped native targets on dropdown open. Native
admission now starts only with a drag; the B0131 measurements below are historical.

The native target is admitted transparently after the panel opens, before drag.
It keeps the same content size, renderer, GL context and caches. CPU also imports
its existing SHM alias into the compositor under a verified transparent crop;
GL paints an unswapped backbuffer at the full natural extent. Release commits
final offset and content, then unmaps the source in one connection flush.
Generation, UI serial, size and scale guards reject stale frames. Early/cold or
unsupported admission retains the covered fallback; its latency is not gated.
The measured fast path is niri; no Hyprland latency claim is made. Private proofs
import `shell/niri-pearls.kdl` for unfocused, border/shadow-free targets; live
compositor integration remains a separate deployment step.

GL must allocate the natural 1746×981 target before binding its first backbuffer;
resizing an unswapped 1746×948 buffer kept the wrong first frame. Host Luau now
selects QML's 180 ms OutCubic return, with no margin-clamp plateau or guide rebuild.
Control and AI use the same generic primitives. No new dependencies or live changes.

| Final release-build check | CPU | GL |
|---|---:|---:|
| Separate presentation-only trials | 20 | 20 |
| Native handoff p95 / render p99, ms | 31.120 / 9.217 | 31.390 / 2.947 |
| Covered commit p95, ms | 15.473 | 15.559 |
| Separate image trials, blanks / reversals | 20, 0 / 0 | 20, 0 / 0 |
| Capture-loaded native p95, ms (33 limit) | 36.680, **fail** | 34.830, **fail** |
| Image-cohort render p99, ms | 9.426 | 3.266 |

Image and timing cohorts are separate claims, not a simultaneous guarantee in
one set of 20 trials. Image trials also require stable native pose/size and at
least eight post-handoff samples. Parent scheduling is synthetic 120 Hz;
niri's Winit child is fixed at 60 Hz. Software presentation timestamps do not
prove physical scanout or the aspirational ≤17 ms target. No timing limit was
relaxed. A three-pair diagnostic measured about 3.15 ms of screencopy overhead.

The old roughly 442 ms CPU tail came from cold reveal-edge construction and
composition. A same-source release-profile diagnostic already reduced that tail
to 15.912 ms, so the build-profile effect is explicit; no code-only speedup factor.
Prewarming moves cold work before release and keeps retained content at detach.

Budget: ≤1 MiB added always-on unreclaimable memory. Five alternating matched
pairs per backend include anon + SHM + DRM + SwapPss; clean-file PSS is separate.
CPU medians versus control: idle −0.266, closed −0.078 MiB; active docked +4.736,
detached +8.207 MiB. The latter includes +3.734 MiB retained SHM. A diagnostic
trim released 4.49 MiB of glibc-free resident anonymous pages without changing
live allocations; those pages still count in the measured total. Idle scratch
cleanup trims after freeing, but does not eliminate all active residency.
GL idle/closed deltas are −0.242/+0.047 MiB; docked/detached +21.209/−13.182 MiB.
The docked GL cost includes +15.914 MiB DRM for the prepared EGL target; a GL
trim diagnostic releases another 3.93 MiB without changing live allocations.
Matched release-build memory arms reduce median captured handoff from about
504.5 ms to 36.0 ms CPU / 33.4 ms GL (five samples each, not the formal p95).
Active warm storage uses the standing smoothness-first allowance. Private
compositor memory is separate; its CPU closed delta is +41.223 MiB. A diagnostic
gap changed from about 46 to 4.56 MiB after five extra captures, then reversed
after private UI exit. The mechanism is unresolved; no combined desktop-memory
budget pass is claimed.

Paired QML/Bead returns are monotonic and blank-free; sampled 2 px settling is
182.964/135.557 ms, not exact animation completion. Contour crops and cold
control/AI API detach–dock checks pass; complete pixel-style parity is unclaimed.
Full remote Arch CI passes, including 22 proof tests and handoff-ordering tests.
No replacement duplicates remain. Follow-up source: +2153/−122 lines, including
tests, versus integrated main `fb0e3c8f`. [Methods, identities, raw data and limits](evidence/0131/README.md).

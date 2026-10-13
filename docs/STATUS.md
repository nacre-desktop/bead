# Status

2026-10-13: Feature 3 built and validated; Gate B waiting for owner.
Core code published as dfbe296; bundled UI and final handoff follow in this commit.
Typed JSON/Luau, stable-ID typed stores, reactive bindings, asset snapshots,
atomic CPU/GPU staging, error overlay and event-driven watch are built.
State/input/scroll/open/focus survive view replacement; broken files keep old UI.
Simple input is a proof; full controls/layout/animation remain Feature4.

Shared native GL/femtovg selected for auto; CPU fallback when hardware GL absent.
Private GL, wgpu and CPU-fallback GUI reload/error tests pass (79–103ms latest).
20 tests, fmt and clippy pass on final code; both nested owner-demo proofs pass.
CPU-only build and workspace + all independent framework cargo-deny graphs pass.
No system dependencies installed. Nacre/Orient remain read-only.

ONE approved60s live niri run consumed and stopped; all test PIDs verified absent.
2880×1800/1.5/120Hz; physical presentation p50/p99 GL and QML8.333/8.337ms.
GL total95.64MiB vs QML187.89MiB; CPU1.51/3.29% over55s after5s warmup.
Physical flags7, refresh8333313ns; 21/22 discarded updates. Paired desktop load.
Color emoji confirmed renderer log, not just mapped files. No further live approval.
Cold UI startup314.89ms vs QML228.35; later108.01 vs162.31ms.
Earlier nested GL102.43ms callback p99 remains recorded; do not hide it.

Next: owner Gate B decision (go / adjust / stop). Recommended adjust: keep shared
native GL/content layers/CPU fallback; address cold startup and move reload baking
off the UI thread as the experiment grows. Feature4: Taffy layout, animation and
full pointer/keyboard/focus. Do not start before owner OK. CI verification pending;
all local checks pass. Git author/committer use contributors/noreply.

Owner command: `./scripts/dev.sh niri --demo` (nested only; valid/error/recovery).
Click Search/type/scroll/Home; state should survive. Close window or Ctrl+C stops it.
Private raw evidence stays ignored; preserve feature3 logs in canonical .research.

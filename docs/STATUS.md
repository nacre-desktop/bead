# Status

2026-10-14: Feature6b ACTIVE, architecture hardening; STOP after6b before Feature7.
New isolated bead-feature6b/feature6b-architecture at9a8b7c7. Six steps plus owner
step0 are authorized; commit/push+CI per step, nice10/-j2, nested sessions only.
0a ACTIVE: swap interval0 + existing per-surface callbacks implemented; no full pass.
Nested niri GL pairs remain≈32–33ms, though steady swaps<.4ms; CPU+CPU≈16ms.
Hypr120 medians8.34ms; panel p99 up15.68ms. No excess frame counts; idle not proven
unchanged (failed-audio retry samples5–11ticks/60s). Callback batching experiment
failed and was reverted; Mesa interval0 still has a sync-callback throttle.
Physical acceptance needs explicit owner permission; do not launch a live test.
0b batched Hypr queries next;1layer tree/realGL damage;2script+bake worker/600ms stall;
3lazy per-surface CPU/GL;4kit/tokens;5panic/errors/unit/persistence/capabilities;
6features/modules/trace. See FEASIBILITY step0a for exact paired results/failures.
No live configs/installs/releases. Nacre/Orient read-only. Canonical tools/target;
private evidence .research/feature6b. Baseline9a8b7c7 native executable preserved.
Pleamar953b1b4 BSD-3-Clause researched for independent logic/render loops and bounded
rectangle damage; reimplementation preferred, no code copied. Gray square in
bar-panel captures is intentional shell/sample.png fixture, not missing-icon fallback.

## Preserved Feature6 checkpoint

2026-10-14: Feature6 COMPLETE with concerns; steps1–4 finished. STOP here.
Source def7255, measurement-reader ab5bf02; CI38551709096/38552860857 passed.
Matched actual QML header source9e39583 (visuals unchanged in fixture); reference
734a4bc and native7ac9242 milestones retained. Service foundation b9e1769 reused.
Window title now uses canonical JetBrainsMono Nerd Font, same file as QML alias;
late title text refreshes font selection. Tray resolves named/absolute/app indexed
icons plus Adwaita, then valid network-ARGB pixmaps (also when named decoding fails).
Transparent real fixture icon replaces square; same-path asset changes invalidate
retained layers. Package_2/count0 precedes status; updates helper remains future.

## Feature6 proof and verdict

Real private PW null/default sink, volume/mute and3daemon restarts; native SNI
add/update/quit100cycles, actual Activate/AboutToShow/DBusMenu Event; ordinary and
menu-only bar hit testing, right-click/menu and click-mute/scroll-volume routes;
3whole-bus replacements and stale item rejection after quit pass.
Final owner FD20/live tasks7/retained watcher1/subscriptions1 stable; PSS6.48→6.58MiB
late100cycles,0CPUticks60s, subscription drop0/worker joins. Hardware not tested.
Rendered100cycle churn passes on all8CPU/GL/alone/panel rows: FD37–43 per mode stable,
PSS growth.004–.65MiB after warmup, bus tasks7/retained1/subscriptions1/layer1 stable.
Three real ownership issues repaired: menu listeners after owner exit, completed
JoinSet task allocations, extra owned FcFontMatch reference in fontconfig0.11.0.
Small MIT vendor patches preserve attribution. Initial failures retained; no
forced GC, allocator trim or enlarged tolerance. No indefinite leak-free claim.

Private2880×1800/scale1.5 outputs: niri nested60Hz, Hypr virtual120Hz. Distinct
1164×654 GL panel, full shadow geometry and one shared native GL context proven.
PSS+DRM MiB CPUbar /GLbar /CPUbar+GLpanel /GLbar+GLpanel /matched QMLbar:
niri23.00 /65.03 /104.90 /109.24 /118.01; Hypr21.95 /65.70 /105.53 /111.67 /123.82.
Eight60s intervals,220focus actions each, separate8s pacing. Native idle1–2ticks
(~.017–.033% one core). Metadata/state and per-update presentation measured
separately; presentation is private compositor feedback, not physical scanout.
Concerns: niri mixed-surface period≈33ms; title p99 reaches152.34ms. Hypr title
p99 18.54–23.94ms. Final off/on trace statep99 1.89/9.60ms; worst356.58ms sits in
snapshot-reply waiting (.046ms worker CPU), append-window p99<.1ms. Keep failure,
not an append-only diagnosis. Detailed bounds/p50/p95/p99/pacing in FEASIBILITY.
CPU bar has lowest measured standalone cost; GL fixed cost and UI-thread baking
remain concerns for GateC. No optimization/Feature7 work and no next gate started.

45tests/fmt/clippy/CPU-only/private sensors/workspace+all6framework deny pass.
Final CPU/GL and panel captures inspected in examples/bar-*.png. GL unavailable
companion exits bounded, surfaces0. All task processes stopped; references clean.
No live desktop tests, machine installs or Nacre/Orient writes.
Menus inline; logo fixed fixture palette; fixture states differ from QML shims,
which exclude real helpers/tray. Results are component measurements, not full shell.
Roadmap now requires framework component kit/shared QML-matching tokens and
blurred-artwork/level-meter/peek/segmented capabilities before/at GateC; docs only.

## Evidence and reproduction

Final ignored evidence .research/feature6/owned-niri (CPUalone),owned-niri-remaining
(other3rows/QML/traces),owned-hyprland (allrows/QML/traces),owned-lifecycle;
final-matrix-combined.json plus trace analysis/worst intervals. Earlier lifecycle,
churn-diagnosis/corrected/matrix-check/final-* failures are superseded, preserved.
Private reproduction: spikes/measure.py --bar-ui with BEAD_BAR_MATRIX=1,
NACRE_REFERENCE and canonical CARGO_TARGET_DIR/BEAD_RESEARCH_TOOLS/BEAD_RAW_DIR;
Hypr adds --compositor hyprland --virtual120. Build release shell +service examples.
BEAD_SERVICES_LIFECYCLE=1 with --bar-ui runs the private native service proof.
NEVER launch standalone --bar directly on daily desktop for these proofs.
Task worktree/branch preserved: bead-feature6-bar/feature6-reference-bar.
STOP after Feature6 summary. Future changes require the owner's next instruction.


Feature5 code milestone af985ba published after concurrent attribution rewrite.
Equivalent code at earlier5687bea passed CI38524233362; final CI checked separately.
Final proof/publication includes non-focusing indicator mode and asserted live title,
reference comparison harness, restart/stale-action verifier and nested demo configs.
Source revision 8481e4b CI passed; the final docs checkpoint intentionally skips CI.
Features1–4 remain published. Luau ADR0002 chosen; owner may veto.
Feature5 accepted and Luau kept; Feature6 active. No live tests authorized or performed.
Real IME-engine composition and screen-reader end-to-end remain unvalidated;
GateB allocator/driver memory tolerance is2MiB, while owned counts/FDs stay stable.
Off-thread baking/shader-cache startup improvements remain deferred.

Nested demonstration: `./scripts/dev.sh niri --backend` or
`./scripts/dev.sh hyprland --backend`. Two test fixture windows supply live titles;
focus a window and click workspace buttons. Close nested window/Ctrl+C to stop.
The fixture clients use Quickshell only for testing; native shell has no Qt dependency.
Scripts use nice10/Cargo-j2. Never install these test configs as a session.
Recorded events are reduced in one shared backend; views/reload own no transports.
Socket replacement at a known endpoint reconnects. A new real-session socket path
requires reinitializing with the new environment, never guessing another session.
Niri IPC-only output geometry changes have no dedicated event; topology/config
reloads refresh outputs. Physical niri output unplug/replug still needs acceptance.
GPL adapters should be reused for Nacre's Rust migration (Wave2).

Canonical ignored evidence: `.research/feature5` (recovered-raw, final raw/logs,
check logs and final niri stale-ID restart proof). Preserve it; no raw logs in Git.
Public screenshots: `examples/backend-niri.png`, `examples/backend-hyprland.png`.
Measurement boundaries/sample limits and earlier failures are in FEASIBILITY.md.
Reload demonstration remains `./scripts/dev.sh niri --demo` (nested only).

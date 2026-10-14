# Status

## 2026-10-15 — 6b step 4 complete; step 5 next

Framework-owned bead.kit exposes Button, IconButton, Slider, Switch, Tile,
SegmentedControl, List/ListRow, Card, Popover/Peek, Tooltip and TextInput.
The bar and launcher compose it. The factual scale mirror exactly matches
Nacre size-scale.json; the sync/check helper detects later drift. Palette
roles still come from Orient. Point sizes convert to logical pixels at 4/3;
NacreTokens motion durations are 200/120/400 ms.

Bar/Panel edge properties admit all four edges. Layout direction reverses
for bottom/right bars; panel pose moves from its edge; popouts open away
from the bar. A worker's committed panel now updates its layer anchor;
initial fallback geometry had incorrectly left side panels centered.
Only top bar and left/right panels are GUI-validated at this milestone.

Slider click/drag/arrow-key tests, independent tile actions, segmented
selection, token values and panel-edge poses pass. The kit composes retained
blurred artwork, a direct-fill level meter and hidden peek/tooltip states.
GUI captures on both private compositors were inspected; no UI errors,
all generations presented and STOP surfaces=0. Fmt/clippy/workspace tests
and CPU-only build pass. Evidence .research/feature6b/kit-*-final.
Step3 c672af6 CI38644193181 passed; step4 CI recorded after push.
Next: step5 robustness/persistence/capabilities, then step6 hygiene.
Step0a nested niri pacing/idle invariance remains open. No Feature7,
physical test, reference writes, installation or release.

## 2026-10-14 — 6b step 3 complete; step 4 next

Automatic bars use CPU SHM. The companion has a CPU first frame, warms GL
on a later frame callback, and returns to CPU after two idle seconds.
The idle timer stops when released. Animated reopening warms it again.
A current EGL context must be unbound before destruction. glutin keeps
non-reference-counted EGL displays initialized; Bead explicitly terminates
its exclusively owned display only after every GL renderer/surface is gone.
Private two-cycle activate/release proofs pass on niri and Hyprland.

PSS + deduplicated DRM, MiB (2880×1800/1.5; niri 60, Hypr virtual120):
- CPU bar: niri 22.44, Hypr 21.33; neither opens a DRM client.
- Bar + panel warm → idle: niri 143.21 → 76.27; Hypr 150.49 → 75.63.
- DRM warm → idle: niri 66.96 → 0, Hypr 74.95 → 0 MiB.
Warm totals regress against Feature6 CPUbar+GLpanel 104.90/105.53 MiB;
CPU-first raster/allocator retention remains a measured tradeoff.
No trim/forced GC, tolerance changes or live-desktop modifications.
Lifecycle test/fmt/clippy/workspace tests and CPU-only build pass.
Step2 source 1f548b4 CI38643279009 passed. Step3 CI recorded after push.
Evidence: ignored .research/feature6b/policy-{niri,hypr}-cycle.
Step0a pacing/idle invariance remains open. Steps4–6 next, in order.

### Preserved step 2 result

Architecture hardening 6b is active. Stop after 6b; do not start Feature 7.
Steps 0b, 1 and 2 are implemented. Step 0a remains open for nested niri
pacing and idle invariance. Steps 3–6 remain pending.

Step 2 moves Luau handlers, binds, reload compilation and CPU raster baking
onto one worker. The draw thread stages resources before acknowledging the
candidate; only a validated acknowledgement commits stores and the scene.
Retained content survives reloads (61 initial raster builds, unchanged by
five edits to filtered rows). A pixel-identical accepted generation now
requests one-pixel damage and actual presentation feedback; previously its
callback-only commit caused the strict reload proof to time out.

Private Hyprland 2880×1800 / scale 1.5 / virtual 120 Hz results:
- Reload draw-thread preparation median/max: 41.81/234.98 → 0.098/0.153 ms.
- Five file-write-to-presentation samples: 66.46–75.08 ms, including the
  existing 50 ms file-event debounce; state/error/recovery proof passes.
- 600.07 ms Luau stall: 71 commits and 71 presentations; longest interval
  around the stall 9.34 ms, versus zero new commits and 610.15 ms before.
  Whole-run presentation p50/p99 8.37/16.85 ms; jitter is still a concern.
- Typing CPU 29.61 → 32.66% of one core (regression); draw-thread preparation
  p50/p99 0.091/0.457 ms versus 3.70/16.84 ms. Scroll worker cost remains
  unmeasured; the prior 224.55 ms UI tail is not claimed resolved end-to-end.

Workspace tests, fmt, clippy with warnings denied and CPU-only build pass.
The private reload and typing/stall harnesses shut down cleanly. The GVfs
bus diagnostic occurs after the passing proof when dbus-run-session ends;
its exact string exists in gvfsd, not Bead, and the private bus successfully
activated services during the proof. No live desktop or reference changes.
Inherited work is also preserved on local wip/6b-step2-inherited-20261014.
Evidence: ignored .research/feature6b/takeover-{reload,stall,typing}.

Next: implement lazy per-surface rendering, prove CPU first frame and GL
idle release, and compare bar-only/bar+panel PSS plus DRM. Then shared kit
and edge tokens, robustness, feature/module hygiene in roadmap order.
Use the existing bead-feature6b worktree and canonical tools/target.
Nested sessions only; no installation, release or further physical test.

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

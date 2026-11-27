# 0245 — Shell interaction CPU

Status: phase 1 complete; phase 2a implemented; phase 2b framework changes verified.
Host integration and desktop validation against the ~1%-of-one-core goal remain pending.

Phase 2a serializes recovery JSON once before writing; atomic replacement and syncs remain unchanged.
Across sessions, identical bundles skip data writes only for private, same-user regular files/directories.
Missing, corrupt or insecure snapshots take the normal save path; recovery loading is unchanged.
D1/no-renderer/count1: 5,207 / 525,399 encoded bytes use 1 data write versus 2,599 / 262,695 previously.
D1/no-renderer/CI2: full CI passed, including recovery/atomicity/privacy tests. The JSON vector adds transient memory.

## Method

Labels are `host/renderer/run`; H1 is the AMD integrated-GPU test host.
H1 uses a private headless parent compositor and nested stock compositor,
1920×1080, scale 1, 60 Hz, private HOME/XDG and activation-free D-Bus,
identical fonts, palette, static wallpaper, and synthetic service fixtures.
The copied private-session resource benchmark harness provides interactions,
process-tree CPU/PSS/context sampling, memory guards, cooldown, and cleanup.
Original harness files are untouched; raw evidence stays untracked.
Each arm has 60 seconds without the shell, 60 seconds initial idle, six
10-second interaction cycles, and 30 seconds rest, then UI evidence.
Each cycle opens/closes launcher and dashboard with 0.8-second open waits,
switches workspaces 2→1→3→1 with acknowledgements and 0.3-second waits,
then sends four notifications 0.2 seconds apart. Close/open transitions overlap.
These short arms do not remeasure the long-duration steady-idle advantage.

H1 host revision `74baf744d` uses framework `5c0e3136`; scratch `profiling`
inherits release, retains debug symbols, and sets `-C force-frame-pointers=yes`.
H1 captures use a private 4-GiB/no-swap scope, exclusive lock chunks below
590 seconds, and three cooldown readings 10 seconds apart within 2°C of baseline.
CPU is percent of one core for the shell process tree; host services are separate.
PPT includes CPU and integrated GPU. PPT, GPU busy and VRAM deltas are whole-host
interaction means minus each arm's no-shell baseline, not per-process readings.
Detailed diagnostics use a separate binary with logging-only frame identity,
geometry, timestamp and repair-area additions; these runs do not supply power results.

The host's `--renderer cpu|femtovg-gl` and `--panel-renderer` select
`bead_wayland::Options.renderer` for bar and panels; notifications remain CPU.
`BEAD_LAYER_REVEAL=0` holds animation policy constant. Setting it to `1` additionally
requires the reveal protocol and contour data; that compositor offload is unmeasured.

## Reproduction and renderer comparison

| Label | Shell CPU | PPT Δ W | GPU busy Δ pp | VRAM Δ MiB |
|---|---:|---:|---:|---:|
| H1/auto/base1 | 9.26% | — | — | — |
| H1/auto/base2 | 9.26% | — | — | — |
| H1/CPU/cpuA | 9.18% | +0.197 | +2.91 | +18.91 |
| H1/CPU/cpuB | 9.26% | +0.202 | +4.25 | +18.94 |
| H1/GL/glA | 9.16% | +0.247 | +5.95 | +64.43 |
| H1/GL/glB | 9.16% | +0.221 | +1.98 | +65.08 |

H1/auto/{base1,base2} proves SHM for bar, launcher and dashboard and satisfies the
agreed factor-of-two reference reproduction gate. Revision and settling windows differ
from the reference; this experiment does not isolate their effect.
H1/CPU/{cpuA,cpuB} versus H1/GL/{glA,glB}: median CPU is 9.22% versus 9.16%,
median PPT delta +0.199 W versus +0.234 W, and GL adds 45.8 MiB VRAM.
GL alone does not approach the goal; the small power difference is uncertain with
these samples and GPU busy varies widely. GL buffer transport is verified for all
core surfaces with no CPU fallback. Earlier H1/auto/{cpu1,gl1,cpu2} are excluded:
the first tmux launcher dropped renderer environment variables; all actually used CPU.

## Per-step accounting

All rows: H1/CPU/diag-cpu, summed over the six cycles. Close frames are assigned by
that surface's latest command, including overlap with the next panel's opening.
Pixels are millions: semantic damage / actual SHM repair. Full means whole-buffer
repair; the other frames use partial repair. Thread CPU excludes scene-worker work.

| Step / surface | Frames | Full / partial | Pixels M | Frame CPU sum ms | Frame wall median ms |
|---|---:|---:|---:|---:|---:|
| Launcher open | 121 | 6 / 115 | 62.70 / 63.22 | 88.63 | 0.507 |
| Launcher close | 24 | 12 / 12 | 12.90 / 12.90 | 33.03 | 1.171 |
| Dashboard open | 153 | 6 / 147 | 19.72 / 23.54 | 116.14 | 0.401 |
| Dashboard close | 57 | 5 / 52 | 31.56 / 32.66 | 29.10 | 0.432 |
| Workspace / bar | 180 | 0 / 180 | 0.224 / 0.305 | 34.66 | 0.189 |
| Notification arrival / bar | 34 | 0 / 34 | 0.056 / 0.083 | 2.66 | 0.060 |
| Notification lifecycle / overlay | 136 | 54 / 82 | 10.25 / 10.25 | unavailable | 0.208 |
| Settling / bar | 8 | 7 / 1 | 0.672 / 0.672 | 7.32 | 0.968 |

H1/CPU/diag-cpu core-frame wall splits, milliseconds summed: decoration 10.55,
pose 17.12, prepare 39.89, damage 14.19, repair/fallback 7.42, paint 208.72,
commit 15.75. These are render-thread stages, not end-to-end command latency.
Overlay render wall totals 51.75 ms; its stage/thread-CPU split is unavailable.
Overlay repairs are replayed from protocol buffer identities and semantic damage
using the pinned buffer-history/adoption algorithm; outside-repair copies total
0.761 M pixels. The CPU core paths log repairs directly. GL draws whole targets
in its early-return path, so CPU `UI_FRAME` stage counters do not cover GL frames.

## CPU profile and ranked causes

H1/CPU/perf-cpu: frame-pointer `cpu-clock:u`, 499 Hz, all process-tree threads,
user-only callchains, exact interaction window; 1,246 samples, none lost.
Process accounting is 2.68 seconds user plus 2.94 seconds kernel: 52.31% kernel.
Kernel profiling is forbidden by host policy; that share is not assigned to a function.
The table scales user samples by the measured 47.69% user share. Exclusive groups
partition sampled user CPU by nearest relevant caller; inclusive columns overlap.

| Group | Exclusive % shell CPU | Inclusive % shell CPU |
|---|---:|---:|
| Luau evaluation | 13.09 | 14.24 |
| Recovery persistence | 6.35 | 6.35 |
| Layout / UI construction | 5.01 | 18.49 |
| CPU rasterisation | 4.90 | 6.12 |
| Scene build / preparation | 1.99 | 18.68 |
| Wayland / event loop | 1.95 | 6.16 |
| Image / text | 1.53 | 1.99 |
| Damage computation | 0.08 | 0.23 |
| SHM copy; animation ticking | no exclusive samples | 0; 0.08 |
| Other / unresolved user; kernel | 12.78; 52.31 | — |

H1/CPU/perf-cpu top self hotspots (% user CPU): unresolved libc address 11.08,
Luau execute 5.30, free 2.17, write 1.77, malloc 1.52; blend-over is 2.33% in
perf's non-inline self report. Top useful inclusive stacks: generation materialize
39.33%, UI session update 39.00%, frontend build 29.13%, recovery save 13.32%.
Idle wakeups are not separately attributed by this interaction-only user profile.

Ranked cost centres overlap: recovery/write-heavy workers dominate; Luau/layout/scene
construction consumes 20.09% exclusive shell CPU; raster consumes 4.90%, mostly partial.
H1/CPU/task-cpu uses the primary binary plus an external 100-ms per-thread accounting
sampler: 18 threads exceeding 20,000 writes account for 59.36% of shell CPU,
including 45.63% in kernel time, and 99.80% of 1,791,931 write calls.
They account for 86.49% of kernel CPU; their work also includes UI construction.
Snapshots miss exited-thread tails, so these are observed lower bounds, not syscall
stack attribution. Recovery's 6.35% user share remains the directly sampled function cost.
H1/CPU/diag-cpu records 18 first-generation recovery saves in the interaction window,
1,813,962 write calls and 6,318,513 bytes, about 3.48 bytes/call including logs.
Fresh panel/overlay sessions repeat first-presentation saves; the fingerprint guard
only deduplicates inside one session. `serde_json::to_writer(File)` writes byte-vector
JSON without buffering. This explains a renderer-independent syscall workload,
but does not prove all kernel CPU belongs to recovery. GL's negligible CPU gain
is consistent with the relatively small measured raster share.

## Phase 2 plan

Savings below use H1/CPU/{perf-cpu,task-cpu}, not measured fixes;
percent means share of current shell CPU, and candidates overlap.

| Priority / candidate | Estimated saving | Power / memory trade-off | Risk |
|---|---|---|---|
| First: buffer recovery writes; preserve atomic rename and sync | Targets 45.63% observed kernel CPU in write-heavy threads; saved fraction unknown | Likely less CPU/PPT; small bounded writer buffer | Low implementation risk; preserve flush/error/durability semantics |
| Reuse unchanged recovery bundles and warm UI sessions/static layers across close | Up to 26.44% proven user share, plus unassigned kernel; depends on reuse coverage | Less rebuild/PPT; measure retained UI/assets memory | Stale state, resource invalidation, recovery files, lifecycle |
| Cache static UI evaluation/layout independently of animation pose | Up to 20.09% ceiling; target repeated materialization | Less CPU/PPT; retained scene/layout memory | Invalidation and dynamic bindings |
| Damage-only panel raster / avoid full repaints; cache static raster layers | Below 4.90% raster ceiling; existing partial repairs limit gain | Less CPU; cached pixels cost memory, copying costs bandwidth | Reveal contours, buffer history, stale/blank frames |
| Fewer animation frames | Below raster+animation work ceiling; no measured target | Less CPU/GPU/PPT, little memory change | High smoothness risk; preserve duration/easing |
| GL for animated surfaces / compositor reveal | GL alone measured 0.06 core-pp median gain; reveal saving unmeasured | Observed GL +45.8 MiB VRAM and +0.035 W median PPT | Driver/contour fidelity, first-frame and reversal correctness |

Run the smallest buffered-writer experiment first, then session/static-work reuse;
reprofile kernel/syscall costs before promising the target. Alternate controls,
measure CPU **and** PPT, verify smooth transitions, and retain the steady-idle lead.

## Phase 2b — warm source and static layout reuse

Status: framework checks passed; host integration and desktop acceptance pending.

The unit is rebased onto phase 2a on main (`9b065d1a`); the release measurements
below retain their original baseline and candidate revision identifiers.

The teardown is in the framework: `select_panel` releases the previous named
worker when selecting another view, bypassing the ordinary 120-second same-view
keepalive. Overlays release their worker after closing. The host factory then
constructs a new session. Changing `retain_when_closed` alone does not fix named
view switching. Native scene motion already applies offset/opacity to prepared
content; arbitrary Luau `ui.animate` values can change layout and remain dynamic.

A root Panel's bound `open` property now patches `Scene.motion.open` using the
existing materialized cache. Structural uses of the same binding still relayout;
palette, viewport and source invalidation retain their ordinary paths.

`Options.warm_source_budget` opts into a shared, bounded dormant-source cache;
its default is zero. A source must declare a conservative ownership bound through
`SceneSource::warm_retained_bytes` and implement `resume` to refresh its host inputs.
Only closed, settled workers can park. Requested entries leave the cache before
the retiring view enters, so alternating two views needs one dormant slot.
With a budget (B0392), a closed docked panel that can park does so 2 s after its
close settles (its surface unmaps), not at the 120 s keep-alive; a reopen within
those 2 s stays live. A view switched away mid-close waits in one closing slot: the
cache acknowledges that source's own clean packets without a surface and parks it
once dormant; any other callback, a request for it, or 2 s without the ack drops it.
A prepared panel that was never shown (B0405) hears `PanelVisibility(false)` once at its
release and waits in that slot, so it parks like a closed one and drops its host leases;
a source that already heard its close parks at once, with no second one.
An open view replaced by another (B0409: control open, then the launcher) is released as closed too: it hears its close the same way and parks once that is acknowledged, so switching back is a warm reopen instead of a cold one.
A reopened panel's surface is created at its view's last open content extent (B0412): it is recorded when an open panel scene installs, parks with the source (closing slot included) and comes back with it, and a kept view uses its own, so niri configures the content's size on the first commit and no buffer waits a redraw for a second configure; a cold, never-shown view keeps its fallback's size, the extent never crosses views, and a size that changed meanwhile resizes as before. It adds 8 bytes to each parked entry, inside its admission charge. A prepared view that was never shown takes the extent from its staged closed scene (B0413), but only while no open extent is known and only at that scene's open pose: a slide's `open_y`, or the layout's margin offset (`-bounds.y`), which is where the presenter poses an open panel; the slid-out offset exists only on the host's posed copy, and a closed scene that carries one records nothing. Its first open after the start prewarm is then created at its content size as well.
A closing source waits through its own `CompanionVisibility(false)`: a view that hears its close may ask to be closed (Nacre does when a closed scene meets `panel.open`), which carries no state, so a replaced view still parks; any other request or payload still retires it (B0414).
A parked view takes a settled wallpaper palette in the background (B0416). A trailing debounce re-arms on every palette commit (one-shot 1 s timers with a generation, never a periodic one), so palette churn such as a video wallpaper stages nothing; 1 s after the newest commit, and unless a visible palette fence is in flight (its own commit re-arms it), every parked source stages that palette. The worker bakes this one stage although the view is closed (a narrow exception to B0351), its packets carry no fence palette, and the warm cache acknowledges them as it does a closing source's (stage, commit), so the source stays parked and is dormant again; an error or any other event still retires it. The closed bake covers the open scene: an open-binding change only flips `motion.open`, raster keys ignore position, and the idle trim keeps what that bake touched, so the next open builds no raster for the palette (a reveal and a slide panel, tinted text and image, CPU and upload policies: 0 against 1-2 today). Content that itself reads the open store relayouts closed and may still bake at its open. Memory: a text slot replaces its raster in place, an image's previous variant goes at the idle trim 250 ms after the bake, and the host's current scene is replaced at the commit, so the old palette's rasters are freed, not added. The admission charge is not re-read: a palette changes colours, not raster sizes, and a re-read before the trim would count the previous variants. A view taken while its background bake is in flight re-stages the newest palette as before; those packets are transactions ahead of its resume, which never shows them.
A text slot keeps its previous raster variant like an image slot (B0424), so an A, B, A flip (focus ping-pong, hover ink, palette A/B) bakes two rasters, not three. The cost is at most one extra raster per changed text; the previous variant is evicted first under budget pressure and dropped at the idle trim and on look retirement.
`Session::set_projector_frames(false)` (B0426, opt-in; the default keeps every staging projecting): a staging with no store change whose only reason is a due animation (a looping `ui.pulse` stages every display frame) skips the host projector, both calls, if a projection ran less than 100 ms ago (fixed shortcut). The staging that ends the animation, and any model change, project as before; wakeups during a pulse are seen within 100 ms. The animation frame rate is unchanged, only the projector cadence. A host opts in per view whose projector does not need a per-frame clock read (deadlines, relative times). No timer, no memory.
Under `BEAD_VERBOSE_LOG`, `WARM_CLOSING_REFUSED`, `WARM_CLOSING_WAIT`, `WARM_CLOSING_RETIRED` (with the event, or `event=take`) and `WARM_CLOSING_EXPIRED` name why a closing source parks late or retires cold, and `WARM_PARK_REFUSED` a park that is not dormant or over budget (B0411).
Such releases skip the immediate allocator trim; the idle trim 100 ms later stays.
Budget 0 keeps the keep-alive, the cold switch and both trims.
Surfaces, renderer instances, focus and protocol objects follow normal teardown.
Payload-free model refresh hints keep the source warm; mandatory resume polls
current producers. Reload, payload, protocol and error callbacks evict the source.
Hidden views do not run model updates or retain a queue of events.
A worker that ran image/CPU hydration parks only once every such job it was sent
has finished and a later mirror re-read its retained bytes; never while one is in flight.

Reopening uses the normal surface path with an acknowledgement barrier before
showing cached content. Palette and visibility updates precede resume (B0410: the
accepted palette again returns no scene from `Session::set_runtime_palette`, so it
costs no Luau or layout pass; the Settled packet still carries it for the fence); failed
validation cold-creates the view. `Session::sources_current` checks accepted source
bytes, palette and watcher revision without evaluation, including during staged
store/layout updates. Staged source reloads still use `check_sources` and atomic
commit. `current_scene` exports accepted content without rebuilding. Hosts must
also refresh their own models, account for changed geometry, and validate again
before accepting the resumed transaction. Recovery persistence is unchanged.

### Measurements and acceptance

D2/CPU/bench2 compares `fa28eea6` with `daaf5b19` in an Ubuntu 24.04 container,
Rust 1.99.0 release build. The compositor-free fixture has 300 bound list rows
and six open/close cycles. Preparation and display use separate caches, matching
the worker/render ownership boundary. The initial harness incorrectly shared
those roles and is excluded from the following tables. Numbers are medians of
six cycles; timings include allocation instrumentation and are not shell CPU usage.
Cold creates a new Session per open;
resident keeps one Session and prepares its content once before the cycles.

| Metric | Baseline cold | Baseline resident | Candidate resident |
|---|---:|---:|---:|
| Open: UI + preparation + display adoption, ms | 30.880 | 1.668 | 0.477 |
| Close: same stages, ms | 1.735 | 1.704 | 0.479 |
| UI open alone, ms | 11.159 | 1.281 | 0.091 |
| UI close alone, ms | 1.338 | 1.300 | 0.096 |
| Allocation calls/open, all measured stages | 63,376 | 15,077 | 6,434 |
| Allocation calls/close, all measured stages | 15,077 | 15,077 | 6,436 |
| Luau evaluations/open-close cycle | 1 | 0 | 0 |
| Materialize calls/open-close cycle | 2 | 2 | 2 (cached patches) |
| Layouts/open-close cycle | 2 | 2 | 0 |
| Builder preparation calls/open-close cycle | 2 | 2 | 2 |
| Raster builds/open-close cycle | 301 | 0 | 0 |

The candidate's cold open remains 30.164 ms: warm retention, rather than faster
initial construction, supplies that saving. The separate worker tests exercise
actual source admission/resume; the table isolates Session/render work and does
not claim that the measured production host has enabled retention. It excludes
host refresh/source-validation I/O, worker scheduling, surface creation and raster
painting.

Both revisions' 360 prepared pose frames have **zero** evaluations,
materializations, layouts, display scene preparations, raster builds, and
allocation calls. Median frame preparation is 1 microsecond (maximum 5).
This confirms the existing pose-only path; it is not a new animation speedup.
Model expansion/restoration, successful atomic reload, and rejection of invalid
source without replacing the accepted view also pass in this harness.

Candidate retained Rust allocation delta is 3,013,712 bytes (2.87 MiB) for the
Session plus exported scene, and 10,609,771 bytes (10.12 MiB) with the synthetic
CPU builder/display content retained. Peak tracked allocation delta is 10.41 MiB.
These exclude worker stack and untracked native/shared ownership; they are not
RSS. Behavior/markup VM diagnostics are 533,576/1,566,692 bytes and must not be
added to allocator deltas as though their ownership were disjoint. Production
panel memory cannot be inferred from this text-only fixture.

Executed benchmark command: `cargo test -p bead-ui --features test-counters
--release --locked -j2 panel_open_close_baseline -- --ignored --nocapture`.
The pre-final focused suites passed: 164 Wayland, 128 renderer, and 186 UI tests.
Actual-worker
tests retain one factory across six reopens and two factories across six A/B
cycles, for both slot kinds with one dormant-entry budget. They also cover pending
transaction rejection, coalesced input readiness, budget eviction, epoch isolation
and rejected resume. The small actor fixture declares 16 KiB of source ownership;
its admission charge is 2,117,910 bytes against a 2,118,934-byte test budget.
This is a conservative admission proof, distinct from the 300-row heap measurement.
Warnings-denied Clippy caught a deprecated atomic spelling, corrected to
`try_update` before the final gate. D2/CPU/CI5 passed the complete
`.github/workflows/ci.yml` replica on rebased revision `ba11a057`, including formatting,
boundary/negative controls, workspace Clippy/tests, all renderer feature checks,
every example app and minimal-host proof, private service/network/tray/clock proofs, and
all per-manifest dependency/license checks. Source checksums matched before and
after the run. The final follow-up changes documentation only. Cargo-deny's
existing locked `async-executor 1.14.0` yanked-version warning is non-fatal.

Dormant admission charges include the declared host bound, a conservative worker
stack allowance (at least 2 MiB, increased for larger `RUST_MIN_STACK` requests),
prepared/raster cache charges, and actor/cache metadata. Thread creation retains
Rust's existing stack policy.
Shared budget handles may be counted conservatively more than once. Hosts must
include opaque shared fonts and uncharged tree metadata in their declaration;
unknown ownership stays cold. Admission charges are not measured resident memory.
No additional dormant retention is enabled by default. A real host budget and its
retained-memory measurement remain part of host integration.

The measured host has not implemented these opt-in contracts. No desktop grant
or owner live audit has been supplied, so there are no phase-2b shell CPU/PPT or
look-and-feel acceptance results, and the approximately 1%-of-one-core goal is
not established. Raw harnesses and logs remain untracked.

## Phase 2c — UI hot paths: hover and focus patches, allocation churn

Scope: the UI-side items of the hot-path audit (B0252): hover/focus as cache
patches instead of rebuilds, `Stores` copy-on-write, cache-hit read tracking,
the per-node String churn and global locks in `ViewSpec::from_raw`, Taffy id
keys, and the raw-view cache's redundant deep compare on animation frames. No
rendered output changes: each patch is asserted primitive-for-primitive, clip and
node-box equal to a rebuild from a fresh generation.

`bead.hovered`, `bead.focused` and `bead.focus_visible` are no longer structural
in the materialized cache. Hover only ever changes a view's box fill and label
colour (`layout::surface_colors`), so the cache records both colour states for
every node-bearing view and recolours in place. A box that only one hover state
draws (a transparent idle fill with a `hover_fill`, or the default hover fill of
an unfilled button) is inserted in front of the view's own primitives or removed,
renumbering later layers and clips; a view that drew nothing (so culling cannot be
told apart) and an uncoloured empty `TextInput` (placeholder colour) still
rebuild. The focus ring moves with its control, including under scroll; appearing
or leaving still rebuilds. A text patch on a hovered or selected label now keeps
the label's hover/selected colour, which previously fell back to `onSurface`.

`Stores` holds its two maps in `Arc`s with copy-on-write commits, so staging a
transaction no longer copies every store; `from_raw` reads the sans family and
text style once per build and borrows string properties and the Taffy id keys;
cache-hit resolves record a read name once instead of allocating per binding.

`ui.animate` markers (A1) were not done: 10 of the 19 call sites in the Nacre
shell pass the value through `math.min/max/ceil/clamp`, `tostring` or
comparisons, which Luau's standard library evaluates eagerly and which no
metatable can defer, so a marker is not source-compatible. The no-API-change step
remains: animation frames skip the raw-tree deep compare and the second size
walk. Luau evaluation itself stays the cost of an animation frame.

Measurements: `.lead/bench` harness (B0252, extended), oyster, ubuntu 24.04
container, Rust 1.99.0 release build, medians of 30 (microseconds / allocator
calls), base `9469cc81` against this unit, consecutive runs. Timings on that host
vary by up to a third between runs; allocation counts are exact.

| Path | base | after |
|---|---:|---:|
| bar hover crossing | 340 / 2,905 | 40 / 700 |
| bar focus move (ring) | 352 / 2,955 | 42 / 748 |
| bar bound text change (patch) | 228 / 4,296 | 98 / 966 |
| bar update with unchanged value | 11.3 / 342 | 1.7 / 15 |
| bar `Stores::clone` | 5.0 / 328 | 0.0 / 0 |
| bar `ViewSpec::from_raw` | 178 / 1,518 | 173 / 723 |
| bar `layout::layout` | 43 / 359 | 39 / 317 |
| bar Luau animation frame | 1,289 / 4,466 | 1,286 / 3,212 |
| panel (300 rows) hover crossing | 2,233 / 15,207 | 285 / 3,145 |
| panel `ViewSpec::from_raw` | 1,198-1,726 / 8,542 | 1,154-1,461 / 6,700 |
| panel `layout::layout` | 373 / 3,489 | 354 / 3,186 |
| panel scroll step (patch, first in a session) | 353 / 3,121 | 348 / 3,111 |
| panel scroll patch without commit | 185 / 3,121 | 188 / 3,146 |

Scene sizes, `TreeCache::prepare` and damage costs are unchanged. B8 is a no-op at
this layer: a session's scene never carries prepared frames (the worker bakes
every packet), so the clear in `scroll_feedback` costs nothing; the fade-frame
re-bake lives in the worker's `bake` and is a follow-up there. Taffy incremental
relayout (B6) is not done: after this unit only store-driven structure changes,
viewport changes and Luau animation frames lay out, and those re-run the
frontend anyway.

## Phase 3 — end-to-end proxy of the Platform scoreboard (B0272)

Goal: measure a whole `bead run` shell and attribute what is left (target: Noctalia,
~1.2 % of a core during scripted interaction).
### Harness (`.lead/e2e/`, kept in the unit worktree, not tracked)
`job.sh OUT RUNS name=/tree [name=/tree ...]` runs inside `nacre-ci/ubuntu-bead:current`
(sway, grim, wlrctl, pipewire; vendor cache mounted `:ro`). It builds each tree in release,
then per round runs every tree on `examples/bar-with-panels` and on `app-heavy/` (300-row
list panel, 8 tinted icons, badge, clock with seconds) under a private headless sway
(1920x1080 at 120 Hz, pixman). `arm.sh` starts sway and `bead run`; the driver script
sweeps the bar every 2 s, opens/closes a panel every 5 s, scrolls the list (8 wheel steps)
and then leaves 60 s with no input. `sample.py` reads utime+stime and voluntary context
switches per thread from `/proc`; `attribute.py` splits traced arms by `UI_FRAME` stage. Run: `SECS=40 job.sh /out/cmp 2 main=/base cand=/workspace` (SECS =
interaction seconds, default 120); `NOBUILD=name` reuses a prebuilt `/target/name`.
CPU % is of one core; wakeups are voluntary context switches/s.
### Numbers (dev-1 container, `--cpus=6`, CPU renderer)
Origin/main 9e1d8259 (3 rounds of 120 s, Nov 13): bar-with-panels 0.82-0.92 % during
interaction at ~120 wakeups/s, 0.20-0.22 % at 13 wakeups/s for the 60 s after the last
input (flat); heavy 0.66-0.71 % and 0.32-0.35 % at 19 wakeups/s; pure idle 0.02 % at
6 wakeups/s. Bead alone is below the 1.2 % target.

Base (origin/main) vs candidate (fixes below), 40 s interaction + 60 s after, 2 rounds:

| Arm | Interaction CPU | wakeups/s | After-input CPU | wakeups/s |
|---|---:|---:|---:|---:|
| bar-with-panels base | 0.75 / 0.92 % | 120 | 0.20 / 0.23 % | 14 |
| bar-with-panels candidate | 0.60 / 0.72 % | 116-117 | 0.03 / 0.03 % | 5 |
| heavy base | 0.62 / 0.82 % | 115-117 | 0.35 / 0.32 % | 20 |
| heavy candidate | 3.42 / 3.10 % | 153 | 0.12 / 0.13 % | 11 |
After input the shell is back at the idle floor (0.02 %, 5 wakeups/s) for bar-with-panels,
and heavy drops by ~2/3 (its seconds clock still repaints). The heavy interaction rise is
a harness artifact, not a render cost: in the base arms the list scroll never reached the
panel (2 panel frames in the 0.7 s scroll phase), in the candidate arms it did (79 frames,
132 ms of render CPU for 300 rows plus the off-render-thread row evaluation), so the arms
do different work. The race (panel opened 0.6 s before the first wheel step) must be closed
before the next comparison; why the base drops the wheel is not yet understood.
### Attribution (traced arms, ms of thread CPU)
1. Panel pose frames: 593 of 699 panel frames carry `motion=status`; `prepare` is 365 ms of
   990 ms shell CPU (`TreeCache::prepare` re-runs per pose frame of a resizing reveal).
2. 1 Hz seconds lease: taken whenever a folder app declares `clock`, so every second an
   empty Model event reached every registered view and was staged (~7 wakeups/s, 0.2 %
   after input). Heavy also paints 2 bar frames per seconds tick (121 per 60 s, 0.26 ms
   each), one more than needed.
3. Hover sweeps are cheap on the render thread (0.15 ms per bar frame, paint-dominated);
   50-80 % of shell CPU sits off it (worker hover/scroll evaluation, service threads).
   Named threads: `bead` (render) 66-72 %, `bead-worker` 3-10 %, rest service threads.
### Fixes landed here
- Threads are named (`bead-worker`, `bead-svc-bus`, `bead-media-tick`, `bead-scheduler`,
  `bead-audio`, `bead-reap`), so `/proc` and profilers can split CPU by thread.
- The 1 Hz seconds lease is taken only while a view's committed tree can read
  `desktop.clock.seconds` (`Session::reads_store`); minute-only clocks tick at minute
  boundaries. Tests: the lease follows a minute view and a seconds view.
### Remaining
- Fix 1: reuse the prepared frame across pose-only motion frames of a resizing reveal
  (largest render-side item, 365 ms of 990 ms): follow-up unit.
- The heavy bar still paints 2 frames per seconds tick; the driver must wait for the
  panel's first frame before scrolling.

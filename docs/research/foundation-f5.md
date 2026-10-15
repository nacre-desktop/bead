# Foundation F5 close-out — 2026-10-15

F0 and F5a–F5e implemented. **STOP: Features9–10 remain paused.** Typing targets
are not accepted. Resuming notifications/launcher on the daemon requires the
owner's OK and the migration agent's next phase. No live desktop, installed
Nacre, configuration, keybinding, autostart or user unit was changed. No release.

## Checkpoints and CI

All commits use the owner's requested author/committer identity. Main received
normal fast-forward pushes from the isolated foundation worktree. No PR/force push.

| Checkpoint | Main commits | CI run |
| --- | --- | --- |
| Pause in-flight work |32f741d|[38764519973](https://github.com/nacre-desktop/bead/actions/runs/38764519973), green|
| F0 charter/ADR0003 |311a2f0|[38764800206](https://github.com/nacre-desktop/bead/actions/runs/38764800206), green|
| F5a hostable services |40a979c|[38766103118](https://github.com/nacre-desktop/bead/actions/runs/38766103118), green|
| F5b headless harness |39b4270|[38767973872](https://github.com/nacre-desktop/bead/actions/runs/38767973872), green|
| F5c crash safety |f49217c,9dfc295|[38772441876](https://github.com/nacre-desktop/bead/actions/runs/38772441876), green|
| F5d Sources |e3134fd,1e29fd8|[38778562745](https://github.com/nacre-desktop/bead/actions/runs/38778562745), green|
| F5e effect/audit |this checkpoint|[current workflow](https://github.com/nacre-desktop/bead/actions/workflows/ci.yml)|

## Implemented and tested

- F5a: headless dependency graph enforced in CI. The injected notification
  connection/history API uses the host's current-thread Tokio runtime, serde
  snapshots, latest-one subscriptions, bounded processed calls and async stop.
  Existing private in-process use remains a wrapper. Headless wire parity covers
  replacement/actions/expiry/history/DND/inhibition; competing starts cannot
  replace/remove the incumbent or write its history. Stop flushes accepted state.
- F5b: MIT bead-test runs real Luau/stores/Taffy/CPU rendering with pinned,
  permissive fonts and explicit icon fixtures. Committed PNGs/tolerance/diffs,
  scripted pointer/keyboard, AccessKit role/value/focus/action assertions, content
  raster counters and numeric frame-budget replay run without a compositor.
- F5c: main-loop watchdog with sanitized reports/core dumps disabled, atomic
  startup last-good bundle and crash-loop safe bar. Three unclean starts in120s
  select a trusted CPU scene with no application services or background effects.
  Presentation promotes that generation's bounded source snapshot, even if a
  newer generation has committed. No store payload or typed query is persisted.
  Fixed original files can replace a recovered bundle through normal reload.
- F5d: protocol-neutral Source/Mirror with snapshots/RFC7396 patches, explicit
  base/resync, shared bounded payloads, atomic store projection, stale/reconnect
  hooks,2s mark,50ms→2s backoff and call deadlines without mutation replay.
  In-process and fake sources produce identical Luau bindings. Desktop origin
  revisions prevent audio/bus callbacks from relabeling older data as newer.
- F5e: SCTK's existing ext-background-effect-v1 bindings, gated by the advertised
  global and Blur capability. Rust Options/CLI opt-in; default off. Logical
  surface regions resize with configure, clear when hidden/capability is lost,
  and effect objects are destroyed before their surfaces. No vendor protocol,
  compositor rule mutation, shader or new dependency. Safe mode disables effects.

Final local checks:106 workspace tests, fmt, clippy with warnings denied,
CPU-only build and permissive dependency checks. One service test is opt-in;
the ignored watchdog-child entry is executed explicitly by its parent test.
The official Luau0.740 analyzer also accepted the migration agent's generated
protocol draft read-only; no draft code/types were copied into framework crates.

## Nested measurements and limits

Existing measurement harness, private buses/PipeWire/compositors, nice10, shared
Cargo target and-j2 builds. Configured2880×1800/scale1.5; Hypr virtual120Hz,
niri nested60Hz. No physical scanout test. Samples are short and other tasks
were active; these are correctness/lifecycle/memory regression proofs, not new
paired QML, idle invariance or typing/pacing acceptance. Builds were stopped
for the native samples. Raw artifacts remain ignored under .research/foundation.

| Proof | Hyprland | niri |
| --- | ---: | ---: |
| F5c actual UI busy→watchdog abort ms |5077.76|5071.18|
| F5c first-frame/broken source+palette boot/fixed reload/safe mode |pass|pass|
| F5d Source notice protocol/real action/hover/restart/private-bus cleanup |pass|pass|
| F5e advertised protocol / Blur flag |yes /1|yes /1|
| F5e effect objects created / destroyed |7 /7|4 /4|
| F5e bar /first warm /closed5s PSS+DRM MiB |21.51/27.51/21.80|22.74/28.05/22.78|
| F5e final lifecycle delta from bar MiB |−.84|+.04|

Every close/lifecycle sample stays within bar+5MiB; no DRM client fds observed.
Standard protocol support was advertised by **both** tested builds, including
Hyprland, contrary to the plan's compatibility assumption. Only the private
opt-in exercised it. Defaults remain off and the owner's Hyprland layer-rule
policy is unchanged; there is no Hyprland-specific blur implementation.
These are surface-level protocol/lifetime proofs, not translucent-material,
rounded-region or visual blur parity acceptance. KWin/Mutter were not tested.
CPU for bars/small panels; GL only for large animated surfaces. The earlier
~20MiB Mesa residue finding remains the reason for this policy, not remeasured
here. See [the accepted optimization report](optimization-pass.md).

## Font, cache and animation audit — checked, not rebuilt

Active nested smaps show file-backed mappings for the two used font assets.
Selected active font-backed PSS is1196KiB per compositor sample (not all font
engine heap). fontdb makes SharedFile mappings and cosmic-text clones those
shared data references, rather than copying whole font files.

| Cache/accounting | Current boundary |
| --- | --- |
| Retained CPU raster / native GL image pixels |64MiB +1024 entries each|
| CPU /GL opacity composite |64MiB each|
| Compiled markup |8MiB per generation; Lua heap also8MiB|
| Source payload |8MiB encoded,64 levels,65536 values;64 listeners|
| Unacknowledged recovery source snapshots |8MiB /32 generations|
| Stored recovery bundle |8MiB raw /32MiB encoded|
| SHM buffer |64MiB per buffer, three slots; not a global process cap|

**Byte-cap gaps:** Swash/font-engine internals; legacy raw CpuRenderer texture
map (the retained path extracts those textures); selector's count-only cache;
materialized/markup value cache metadata; some optional GPU/per-scale references
and key/metadata accounting. The pixel limits above do not establish a complete
process-wide byte budget. No allocator/font/cache redesign was attempted.

Counter proof:120 combined transform/opacity frames after one initial composite
produce0 actual Taffy passes and0 new content/group rasters. Changed text does
increment the counter. Opaque transform-only audit also has0 layout/content
bakes, but **120 direct composites**: that path paints procedural rounded fills.
Thus a universal no-raster-per-frame invariant is **not established**. Pixel
compositing is not hidden inside the content-build counter. General layout/size
animation is outside this check. These gaps remain visible for a later pass.

## Frozen scope and next decisions

nacre-shell backends: fixes only. No new launcher catalog, ranking or providers;
menu.query belongs to Nacre. No move between repositories mid-feature. The
binary/control-script name collision stays deferred to GateD. GateD is unchanged.

nacre-protocol v1 was absent from published Nacre main at F5d entry. The explicitly
allowed fake-source fallback was taken. The pinned GPL shell adapter, upstream
transcript replay, wire authentication/reconnect and daemon recovery measurements
remain for the next coordinated phase; none is claimed accepted here.

Current service history is bounded256 records/3MiB and uses synchronous writes.
Thirty-day daemon history, durable daemon-kill/3h chaos acceptance and its idle
budgets remain migration/F6/F7 work; these boundaries were sent to that agent.
The paused OSD scale1 golden exposes text outside its pill. Typing29.6→32.7%,
6b0a nested/shared-GL pacing and strict idle invariance remain open.

Extra: a later measured pass can byte-account glyph/materialized caches and
compare the opaque direct-composition path; no such rewrite was included here.

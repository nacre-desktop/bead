# 0254 — Render and worker hot paths

Smallest-change fixes for audit items A3, A4, A5, B1, B2, C1-C4 on the worker and render side, in support of the interaction CPU goal in [0245](0245-interaction-cpu.md). Microbenchmarks below do not establish host-shell CPU; the live gate stays with the lead.

## Changes

- A3: worker packets carry `Arc<Scene>`; the UI thread unwraps the last holder instead of cloning. The host/session copies remain (their ownership crosses this unit's session boundary).
- A4: done in round 2 below.
- A5: each script retains one separate action VM. Every action evaluates fresh exports in a new sandbox environment; reset drops exports and module caches, restores the built-in `require`/`math.random`/`math.randomseed`, re-sandboxes and collects garbage twice. Errors, fallback imports and RNG calls discard the VM. The generation VM, 50-ms allowance and 8-MiB per-VM limit remain separate; resource reporting includes the retained action VM. Only changed store values are re-validated after an action.
- B1: borrow layer-owner names and ordinal keys during `TreeCache::prepare`; resource IDs and cache-key semantics are unchanged.
- B2: `SnapshotFingerprint` hashes borrowed primitives, layer/clip metadata and reveal substitutions; asset identity/content is compared without cloning the scene or building JSON.
- C1: a completion channel wakes the event loop when a snapshot job finishes, replacing the 2-ms polling timer; a premature channel close reports failure.
- C2: the bounded alignment table is retained between damage calculations (at most about 256 KiB per damage tracker instead of a per-frame allocation).
- C3: raster validation is reused for the same immutable prepared frame; frames with mutable image payloads still validate.
- C4: `draw_current` borrows `scene.motion` instead of cloning it.

## Measurements

One oyster container job (ubuntu 24.04, 6 CPUs, rustc 1.99.0, release), the 0252 audit harness, median of 30 runs. Before = main at 01561c71, after = this branch on the same base. Fixtures: `bar` (41 primitives, 127 stores) and `panel` (302 primitives, 300 clips). A3, C1 and C4 time the old and new pattern side by side. Allocation counts are exact; times vary about 10% between runs (bar B2 before measured 36-47 µs across runs).

| Item | Case | Before µs / allocs | After µs / allocs |
|------|------|--------------------|-------------------|
| A3 | worker-to-UI scene handoff (2 clones vs 2 `Arc`), bar / panel | 8.5 / 326, 73.4 / 2516 | 0.0 / 0, 0.0 / 0 |
| A4 | `Mirror::read` per packet (deferred), bar / panel | 20.8 / 177, 177.8 / 1068 | 20.3 / 177, 177.9 / 1068 |
| A5 | `action_values("noop")`, bar / panel | 505.0 / 917, 1072.5 / 2873 | 221.5 / 855, 915.4 / 2801 |
| B1 | `TreeCache::prepare` unchanged scene, bar / panel | 48.1 / 546, 383.8 / 3720 | 38.1 / 302, 323.2 / 1900 |
| B1 | prepare after one text change (bar) / scroll step (panel) | 216.8 / 1177, 441.0 / 3817 | 200.9 / 951, 387.5 / 1997 |
| B2 | frozen-snapshot reuse check, bar / panel | 46.6 / 172, 287.4 / 1271 | 5.6 / 0, 73.2 / 0 |
| C1 | 10-ms snapshot job to completion | 12.06 ms / 18, 5 wakeups | 10.17 ms / 14, 1 wakeup |
| C2 | `FrameDamage` 200 rows, middle shifted | 214.4 / 11 | 112.0 / 10 |
| C3 | three `prepare_on_ui` on one frame, bar / panel | 1.0 / 0, 23.9 / 0 | 0.4 / 0, 23.7 / 0 |
| C4 | `scene.motion` per draw, bar / panel | 0.0 / 0, 0.0 / 1 | 0.0 / 0, 0.0 / 0 |

C3 gives no measurable gain on `panel`. Cost of A5: the retained action VM adds 412 KiB of Lua heap to the bar script after 100 actions (inside the 8-MiB per-VM limit).

## Checks

Full CI replica (fmt, clippy `-D warnings`, workspace tests, `bead check` on every example, boundary scripts, private-bus tests, cargo-deny) passed on oyster for the final branch. No public framework API changes; existing tests are unchanged. Added coverage: action isolation, limits and module reclamation, layer identity, reveal fingerprint invalidation, snapshot-worker termination. Raw harness and logs stay untracked in `.lead/`.

## Round 2 (B0260)

- A4: `SceneSource::state_revision` (default `None`) changes whenever `accessibility_tree`, `state_summary` or `saved_state` may. While it holds, the worker shares the last copies of those three in the next mirror; regions, deadlines and resource counts are still read for every packet. `Session::state_revision` covers a commit (scene, nodes, stores), a label change, any in-place store commit and a host's direct `stores` reconcile (the store count changes); staged transactions, pointer moves inside a control and empty updates keep it. Folder views report it; other sources read every packet until they opt in.
- A3: feedback-only frames (scroll thumb fades) and graph frames keep the committed nodes instead of copying them, and the worker's Accepted packet no longer resends the scene its Ready packet delivered, so the UI thread copies a staged scene twice instead of three times. The host's pending copy stays: the session and the worker each need an owned scene (the worker adds prepared frames) and `SceneSource::stage` returns by value.
- B8: a full `TreeCache::prepare` records its frame hash before the trailing run of parentless one-fill layers (scroll thumbs). The next prepare whose earlier items, clips, ids, image keys and layers match the last frame reuses them and builds only that run; the frame, key included, equals a full prepare. Multi-scale bakes, non-dense image policies and scenes ending in other primitives take the full path.
- B4: computed store reads are visited by reference instead of collected into a `Vec<String>`; a computed cache entry takes its recorded reads instead of cloning them.

Same harness on oyster (3 alternating rounds, base 327d5399), µs / allocator calls. The A3 UI copy is one `Scene::clone` per transaction (bar 5.8 / 163, panel 41 / 1241), not timed end to end.

| Item | Case | Before | After |
|------|------|--------|-------|
| A4 | mirror read for a no-change packet, bar / panel | 20.5 / 177, 171.7 / 1068 | 0.4 / 1, 1.2 / 2 |
| A3 | scroll-thumb fade frame, session stage (panel) | 465 / 3062 | 248 / 1257 |
| B8 | fade frame `TreeCache::prepare` (panel) | 477 / 1906 | 202 / 919 |
| B8 | unchanged scene with a thumb / after a scroll step (panel) | 319 / 1900, 336 / 1900 | 103 / 913, 334 / 1901 |
| B4 | bound text change / hover / Luau animation frame (bar) | 94 / 966, 42 / 700, 1316 / 3213 | 91 / 896, 39 / 670, 1281 / 3153 |

Fade-frame samples follow a sleep to the frame deadline, so they run colder than the other cases. Added coverage: the state revision holds exactly while the tree and summary cannot change; settled packets share copies until the revision changes; feedback frames keep the committed nodes; Accepted reuses the Ready candidate; thumb-only frames equal a full prepare across appear, fade, vanish and content changes.


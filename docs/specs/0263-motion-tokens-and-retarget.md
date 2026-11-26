# 0263 — Motion tokens, sampled animation and panel body retarget

Design review units D16 (easing tokens, frame-paced driver) and D5 (a panel that retargets size,
anchor and shape mid-animation). Hosts replace literal durations with tokens and migrate per-frame
Luau animations to markers; the Nacre shell's peek-grows-into-panel surface builds on the body API.

## API

- One curve list, `bead_core::animation::Curve` (`parse`, `name`, `ease(t)`), used by `ui.animate`,
  `ui.animated`, `Panel.transition`/`return_transition`, host `Motion` specs and the compositor
  mapping. Durations: `bead_core::animation::DURATIONS` / `duration(name)`.
- Luau: `ui.motion.duration.<name>`, `ui.motion.curve.<name>` (frozen); `ui.animated(key, target,
  duration, curve[, from])` and `ui.pulse(key, period, lo, hi)` return markers; `ui.mix(a, b, amount)`
  is a colour expression (alias of `ui.tint`); `kit.motion` carries the duration tokens.
- Panel: root `Panel.body="<id>"`; `shadow` on `Box`; `join_edge` + `join` on any box.
- `bead_render::Motion.body: Option<Rect>` (set by layout from `Panel.body`).

| Name | Formula (t in 0..1) | Protocol value |
|---|---|---|
| `linear` | t | `linear` |
| `out-cubic` (`ease`, `ease-out`) | 1-(1-t)^3 | `ease_out_cubic` |
| `in-cubic` (`ease-in`) | t^3 | `ease_in_cubic` = 6 (v3) |
| `out-quint` | 1-(1-t)^5 | `ease_out_quint` = 5 (v3) |
| `in-out-cubic` (`ease-in-out`) | 4t^3 for t<.5, else 1-(-2t+2)^3/2 | `ease_in_out_cubic` |
| `out-back` | 1+c3(t-1)^3+c1(t-1)^2, c1=1.70158, c3=c1+1 | `ease_out_back` = 7 (v3) |
| `in-quart` (the exit curve) | t^4 | `emphasized_accelerate` |
| `spring` | (1-(1+8t)e^-8t)/(1-9e^-8) | `spring` |

`bead_layer_motion_v1` version 3 (matching the Platform compositor's bump) appends the three values
with `since="3"`; Bead binds `min(server, 3)` and sends 5-7 only on a v3 binding, else the motion
stays on the client frame path (`curves_supported`), never approximated (tested both ways).

Durations (s, the review's 120 Hz table): hover .09, press .06, release .12, trace .12, badge .14,
close .16, fade .16, osd_in .16, switch .18, peek .2, osd_out .2, open .22, toast .22, dock .22,
slide .24, drop .26, pulse 1.6. Reduced motion reaches every target at once; a pulse holds at `hi`.

## Driver

`ui.animate` is unchanged (its number is baked into the view, so each frame re-runs Luau). A marker
carries `from`, `to`, `start`, `duration`, `curve` and `loop`; layout samples it when the view is
built and the materialized cache samples it on every frame: no Luau run, and for `fill`/`color`/
`border_color` amounts, `radius`, `join` and `translate_x/y` (B0348: and `opacity` on an image
view; B0349: and `opacity` on any view, which multiplies its subtree's alpha like Qt Quick `Item.opacity`,
without an offscreen group) no relayout (the cached scene's
primitives, clips, layer bounds and node boxes are patched in place). A marker on a layout property
rebuilds IR and layout without Luau until its motion ends. Frame pacing comes from the motions
themselves, so a build served from the raw cache keeps its deadlines. The per-frame scene copy the
session hands the host remains (owned by the worker hot-path unit).

## Panel body, joined edges and retarget

The root stays at the declared maximum size and emits nothing when transparent; the body child
owns the geometry. Layout records the body's bounds and its box/shadow as the reveal parts, so the
reveal clips `revealed(body, edge, progress)` and reshapes the parts relative to the body (a joined
edge's extension and the shadow margin are preserved); `content_input_rect` uses the body. Size,
anchor and shape retargets are new marker targets: each continues from its sampled value, and the
reveal recomputes its clip from the current body, so interruption at any point is continuous.
Closing mid-grow is the existing reveal reversal from its sampled pose. A joined box extends
`join * radius` behind `join_edge` and is clipped there (shadow included), so the two anchored
corners are square and flush with the bar; `join -> 0` rounds them (detach), `-> 1` docks.

B0421: the frame decorations join a revealing panel (engaged, merged with its final shape) only
once the panel has drawn part of its body: the scene's last drawn progress and the reveal clock
(held at the first pose until a frame showing it is committed) are both above 0. Until then the
frame keeps its plain tiles, so the held first pose, which shows nothing, leaves no rim opening or
panel edge in the bar strips before the slide. During the reveal the panel's own art draws the
join in its crop; at rest both agree, and an open panel stays joined. A close drops the join with
the request, as before, while the panel's art still covers the strips; a reopen mid-close joins
at once. The opt-in compositor reveal (`BEAD_LAYER_REVEAL=1`) never poses the client scene and
keeps joining at once. Test: `the_frame_joins_a_revealing_panel_once_its_reveal_has_drawn`.

Compositor reveal v2 cannot be the mechanism for a growing body: its atlas is baked pixels and a
plane cannot change shape, so D5 motion runs on the client frame path. v2 stays right for a
static-body open/close; making it the default needs the compositor side and is not decided here.

## Evidence

- Tests: every curve (endpoints, monotonicity, out-back overshoot bound, spring settle), marker
  pacing and retarget, pulse loop, paint-only patches (0 Luau, 0 layout, exact geometry, node boxes
  follow), layout markers (0 Luau), reveal with a body at 0/25/50/100 % and after a retarget, grow
  damage within consecutive bodies, `a_frozen_slide_damages_only_where_the_group_was_and_is`,
  `status_peek_draws_its_first_line_while_revealing_and_at_rest` (worker bake, reveal and
  damage-only repaints of the example peek at a 0 and a 32 px scene margin).
- Surface damage (Platform NN0042): frame-based panels send `FrameDamage` rects (body and previous
  body; a paint-only marker frame only its node, 19,840 B). A reopened non-reveal panel paints from
  its frozen CPU snapshot and sent `Damage::full` every frame; it now sends the group's old and new
  rect (`FrameDamage::update_group`), full once at freeze and thaw. Headless sway, 2880x1800 at 1.5,
  px per frame (bytes = px x 4): status open median 118,260 of 364,536 (first frame full), close
  110,430, resident reopen 53,460; calendar reopen 74,880 (its group is the whole surface).
- Panel motion offset content by a fixed 32 px; it now uses the scene's own margin (`-bounds`
  origin: 32 from Luau layout, 0 for folder views), so a `bead run` popup body hangs from the bar.
- Bench (dev-1, release, median of 40, the 0252 bar fixture): `ui.animate` frame 501 us / 3,152
  allocations, `ui.animated` frame 35 us / 349 (the scene copies handed to the host, not the patch).
- Example: `examples/bar-with-panels` `status`; desktop capture: `.lead/capture-peek.sh` (untracked).

## Reveal close (B0277)

- Cause (confirmed): a reveal open without `slide_distance` clips in place (plane offset 0), but close fell
  into the whole-scene branch (`Scene::panel_offset`, `closed_plane_offset`, `tracking_only` contour,
  deliberate since eed9aa56 so content cannot escape the shrinking body): the plane travelled the full
  surface height with the body's far edge, corners, border and shadow riding to the anchor edge, and the
  slab was gone after about 78 % of the close (dead time, a pop), so the close was not the open reversed.
- Fix: a stationary reveal (`reveal`, no `slide_distance`) closes as the open reversed on both paths.
  CPU: `panel_offset` reshapes the body and clips content for a close too (`Motion::reshapes_body`; the
  reveal window, slide-content cache and `reveal_window` follow it). Compositor: `closed_plane_offset` is
  (0,0), the planner sends `RevealContour::reversed()` in `Visible` mode with contour-owned art left out of
  the atlas, and `motion_atlas_ink_is_safe` shares the rule. Protocol unchanged. `slide_distance =
  Some(_)` closes keep the whole-scene translation and `tracking_only` contour, as do non-reveal motions;
  reduced motion still skips. Net: +212/-69 lines, product code about neutral, the rest tests.
- Tests: `reveal_close_mirrors_the_open_and_settles_into_every_anchor_edge` (4 edges: close pose equals the
  open pose at each progress, offset 0, ink stays inside the shrinking body, hidden at progress 0),
  `stationary_reveal_close_is_the_reversed_open_with_no_plane_travel`, and a wayland wire test (visible
  contour once, no tracking mode). Retarget keeps working: both directions sample the same pose function.
- Evidence (dev-1, headless sway 1920x1080@120, CPU path, status peek, `.lead/capture.sh` untracked, 3 s
  slow variant): before, the peek had vanished at 78 % of the close and its far edge rode up as a slab
  (bottom edge y 155, 144, 103, 63, 40, none); after it shrinks into the bar for the whole close (155,
  148, 124, 67, 42, 39, none, last visible frame at 100 %). Frame gaps in the close, normal 0.22 s: p50 8.3,
  p95 10.6, max 10.6 ms (before: 8.0, 10.1, 10.1). The compositor path is covered by the plan and wire
  tests only: the desktop capture is parked (swap gate closed, see the unit handoff).


## Slide pacing and terminal pose (B0283)

- Terminal pose (cold and warm opens, closes; confirmed in code and on sway): B0221 (58c53b15) moved
  the CPU path's `redraw = false` below the motion sample, so the bit that keeps a surface armed until
  its terminal pose was always cleared. A callback after the motion end found neither `redraw` nor an
  active motion (`Animations::active` and `panel_frame_until` both end at `started + duration`) and
  drew nothing: the final pose waited for an unrelated redraw (a gauge tick, input, the close). A cold
  open's restarted reveal hits the same window; its stale `panel_frame_until` is ORed with `active`
  and never cut frames, so it stays. Fix: the clear sets `redraw` to the sampled motion's `active`.
- Start pacing (unchanged): a CPU draw outside the callback (configure, source update) commits at
  once, 4-6 poses before an open's first callback. Pacing them on the callback was tried and dropped:
  on sway the 12-14 ms gap after the first paced frame stayed (it follows the compositor's frame-done
  time, sent after its own render), and warm opens lost a frame (the first callback after a remap
  arrives ~13 ms after the first commit): open max gaps 11.5-17.7 ms against 9.4-14.3 before.
- Warm-open first-frame gap on niri: Bead requests a callback on every held-pose commit (UI_CALLBACK
  pairs in the Shell's trace); the late one answers a 3-5 px commit one refresh later. Compositor-side.
- Test: `companion_slide_commits_its_terminal_pose_on_the_next_callback` (120 and 60 Hz callbacks off
  the motion grid, plain and restarted reveal: terminal pose within one frame of nominal, no gap over
  a frame; the unarmed loop never commits it), replacing the grid-aligned B0170 loop test.
- Evidence (dev-2, headless sway 2880x1800@120 scale 1.5, CPU, `bead run examples/bar-with-panels`, status
  peek right after the calendar (cold) and reopened (warm), 2 runs x 3 cycles, `.lead/job/capture.sh`
  untracked): PANEL_SLIDE logged 2 of 24 slides before (one close at 454 ms, max gap 446 ms), 24 of 24
  after: opens 200-208 ms (nominal 200), closes 142-146 ms (nominal 140); max gaps opens 8.5-13.9 ms,
  closes 11.2-12.7 ms, the over-12 ones being the start gap above.

## Desktop pass (B0273)

- Rig (desktop test host, real amdgpu `renderD129`, shared lock turns handed out by Platform, no swap reset
  by this unit): private headless labwc (gles2) with one output 1920x1080@120 (`wlr-randr --custom-mode`);
  arms (1) client path, main `6907e640`, `BEAD_LAYER_REVEAL=0`, under nacre-niri r3109 nested through
  winit; (2) compositor reveal, the held `claude/b0267-reveal-default` rebased clean onto main
  (`591b61fc`), default; (3) client path on the labwc parent directly. Release builds from the
  `nacre-ci/arch-nacre:current` image (glibc matches the desktop). B0270 (per-axis nine-patch, `113d9c69`)
  is on main and included; B0270 fix 2 (GL transport) was not measured. 3 alternating runs per arm, the
  B0263 phases (status open cold, grow, interrupt, shrink + close, resident reopen, calendar open/close),
  one persistent virtual pointer and keyboard. Scripts: `.lead/capture-labwc.sh`, `gated.py`,
  `summarize.py`, `strips.py`, `pass.sh` in the unit worktree (untracked: they name the host rig).
- Results (pooled 3 runs; gap = presented-frame gap p50/p95/max ms, n = gaps > 12 ms; reveal rows count only the
  frames the client itself drew; UI_ERROR 0 in all 14 captures):

  | phase | client niri | reveal niri | client labwc |
  |---|---|---|---|
  | status open cold | 8.0/8.4/16.6, n 3 | 24.0/24.3/24.3, n 6 (9 frames) | 8.1/8.4/8.4, 0 |
  | grow; interrupt | 8.1/10.5/11.1, 0; max 16.2, 3 | same, 0; 1 | 8.1/10.3/11.1, 0; 8.1/8.4/11.1, 0 |
  | shrink + close | 8.0/15.9/16.1, 9 | 8.0/15.9/16.2, 21 | 7.3/8.4/11.0, 0 |
  | status reopen (resident) | 8.1/9.6/16.3, 3 | 24.1/24.5/24.5, 3 | 8.1/8.4/9.2, 0 |
  | open to first presented, cold / resident | 30.4 / 16.0 ms | 105 / 58 ms | 34.9 / 10.0 ms |
  | first frame after configure, cold | 21.7 ms | 49.5 ms | 29 ms |
  | calendar open cold | 22/243/243 (one outlier run) | 22/28/28 | 2.4/21.6/21.6 |

  Client CPU p50 0.4-0.7 ms per frame (reveal open: one 24 ms full-body raster). Swap used before each capture
  0 KiB except the last (3.4 MiB); lock held 601 s in total.
- Verdict vs <= 12 ms: labwc direct meets it in every status phase (max 11.1); nested niri adds a 16 ms gap at
  peek open, shrink/close and interrupt in every run, also with reveal: compositor side (nested hop). Calendar
  cold open: 22 ms start gap in both arms, plus a 76-243 ms stall in one client run (unattributed).
- B0277 (3 s close/reopen of the top peek, labwc and nested niri): ink bottom edge 45, 42, 40, 38 px, x range
  stays the panel width (127..424 to 132..419), no line travels to the anchor edge, nothing left behind. PASS.
- Tab (B0279/B0289, labwc, `side-right` inset slide dock via its lip, pointer resting on the edge; normal and 3 s
  motion, 2 runs, grim samples ~13 ms apart, so not every frame). (a) open start: exactly one lip/tab region in
  every frame. PASS. But while docked with the pointer on the edge a SECOND 4x48 pill (x 1913..1916, the free lip
  in the 10 px gap) is drawn beside the tab for the whole open state and grace, gone after the close: the lip
  does not clear at hand-over (needs a decision). (b) close end: the last tab sample is 4 px (slow) / 17 px (fast)
  off the rest x, within the 5 / 90 px per-sample travel; the lip is absent after close, so equality with the
  rest rect is not provable at this sampling. (c) tab rides the body edge: constant 2 px gap in every slide
  frame. PASS. Nested niri tab arm not run.
- Verdict on B0267: HOLD. Compositor reveal appears later on this GPU (first presented 105 vs 30 ms cold, 58
  vs 16 resident; first frame after configure 49 vs 22 ms) and has more >12 ms close gaps (21 vs 9); grow and
  interrupt are the same path. Follow-ups: (1) cache the reveal's static body so an open costs no 24 ms raster;
  (2) the 16 ms start gap on nested niri is compositor frame-done timing; (3) docked free lip; (4) calendar stall.

## First open on niri (B0294)

niri sends a layer surface its frame callback only after drawing the surface itself; for a frame it does not draw (a reveal's zero-area held first pose, or the `bead_layer_motion` atlas it draws instead) the callback waits for its ~995 ms fallback timer. Only a first open commits the prepared handoff frame, and the next pose (which starts the B0170 clock) waited for that callback: READY ~980 ms later, close frames=4-5. Now the handoff commit arms a 16 ms redraw that runs unless a newer commit got there first, and a draw whose compositor motion is active no longer waits for the callback (`draw_waits_for_callback`). Client-path pacing is unchanged. Not run against niri: the live READY / PANEL_OPEN_LATENCY lines confirm.

B0420: a commit without visual damage (`damage_px=0`: a new generation's 1x1 feedback damage, or a callback-only commit of a presented one) can get neither a repaint nor a callback from niri until another surface damages the output. A warm reopen drew its parked pose, then the pixel-identical resume scene as frame 1 (its `UI_FRAME` splits end `gl_flush`, `swap`: it carried the 1x1); the storyboard scene arriving 0.3 ms later waited for frame 1's callback, which came with a bar repaint 32 ms on (otherwise niri's ~1 s fallback): first content ~50 ms against 27 cold. Keeping the 1x1 on presented motion frames would not have helped. Now such a commit during a motion or a panel open arms a one-shot stand-in one refresh later (the surface's presented refresh, else 16 ms; `missed_callback_delay`): if nothing committed since, the callback is still owed and a pose waits (`redraw`), it releases the callback and draws. Cost: one timer per such frame, no extra commit (it draws what the callback would have); idle surfaces arm nothing. The stand-in lands a refresh late (in that trace just after niri's next repaint, so the reopen is bounded at ~50 ms, not brought to cold; B0422). Why niri skips the 1x1 is unconfirmed: it sits at (0,0), likely off-output for a 1942 px wide panel surface on a 1920 px output. Not run against niri.
B0420b: the 1x1 feedback damage now sits at the surface centre clamped, per axis, to the part of the surface on its output (`visible_feedback_point`; the layer-shell origin from anchor and margin, `LayerState::origin`), so niri repaints it and calls it back. With no known output size it stays at the corner. Expected in the nested rig: warm reopen first content ~50 -> ~34 ms (frame 1 presented at the next repaint).
B0422 (small variant): a slide's frame 1 rendered only after frame 0's callback missed the next vblank when its render was 4-6 ms (live laptop: `top_split=prepare:4-6`, paint 0.5-1.3 ms; gap = wait 7-8.6 + render). After a panel slide's first committed frame, the same draw now runs a pass ahead for the pose one refresh later (`scroll::tick`, the presented refresh): pose, `prepare_on_ui` and damage against the frame on screen, then it stops (no paint, no commit; the damage state is kept for the frame on screen). Frame 0's callback, or a B0294/B0420 stand-in, then draws that pose without preparing it again: paint or composite, commit. It is presented only while its key holds (`ahead_ready`: UI serial, size, scale, renderer, the panel's source and open/close epochs, mapped, under 100 ms old); otherwise the draw prepares as before. Only the second frame, only with one panel surface, never past the motion's end (a refresh shorter than the slide). Cost: no extra render (the pose is prepared once, earlier) and no extra buffer (pixels are not held, only the prepared pose and one damage-state copy); a discarded pose costs one extra prepare. The pass ahead's main-thread rasters are not counted in `main_builds`. Measure: PANEL_SLIDE `gap_at`/`gap_render_ms` and `render_top` for frames 1-2 of chat, launcher and dashboard opens, and frame 1's UI_FRAME `split` (no `prepare`); frames 2-3 still prepare after their callback.

## Slide gap attribution (B0295)

The always-on `PANEL_SLIDE` line appends `gap_at` (commit index that ended the worst gap), `gap_wait_ms` (previous commit to the frame callback that drew it, 0 if none), `gap_queue_ms` (callback or previous commit to draw start) and `gap_render_ms` (draw start to commit); the three sum to `max_gap_ms`. Earlier fields keep their order. Presentation-feedback gaps are not included: feedback arrives after the slide line is written.
B0408: the line ends with `gap_dispatch_ms`, the part of `gap_wait_ms` between the compositor's own timestamp on that frame callback (`wl_callback.done` time, milliseconds on CLOCK_MONOTONIC, as niri sends it) and its dispatch on Bead's main thread; `gap_wait_ms - gap_dispatch_ms` is the compositor's lateness. 0 without a callback or when the stamp is ahead or over 1 s behind (another clock); 1 ms resolution.

## Slide edge frames (B0301)

A fading panel's opacity group (the body flattened once) used to be built on its first visible frame and, on the GL renderer, the settle frame repainted the whole body instead of reusing it: both 4-6 ms against ~1 ms for a mid-slide frame. An opening fade now builds the group on the hidden held first pose (CPU: `flattened`; GL: the opacity image), and the GL settle frame composites the current group at whole pixels while the content key is unchanged; changed content or a closed panel still paints directly.

## Dashboard close and mid-open frames (B0305)

A revealing panel's far-edge shadow (and art) slides a pre-rasterised band instead of re-rastering per pose, but the band carries an empty margin past the raster's edge. That margin made the template unusable (`matches` failed at build for a top panel's shadow, and `start + band > extent` rejected it within ~8 px of rest), so every pose rastered the whole shadow, at a cost growing with the revealed height (about 7 ms at dashboard height: a close's frames 1-2 under ease-out, and an open's last frames) and uploaded it. The margin is now required to be empty at build and trimmed from the band at use, so these poses slide the band like a mid-slide frame.
The dashboard's 3-6 ms open frames 5-6 are not this (only ~3-5 px are revealed there); PANEL_SLIDE's `render_top=<frame>:<ms>,...` (the three costliest frame renders) shows which frames cost live.

## Panel profile (B0307)

One source of truth for panel motion: `bead_core::animation::PANEL` (`speed` 1600 px/s, `open_min` .18, `open_max` .32, `close` .18, `leave_ms` 150), exposed generated to Luau as `ui.motion.panel` (and `kit.motion.panel`).
An unauthored open `duration` is `clamp(travel / speed, open_min, open_max)` (`Scene::settle_open_duration`, after layout), travel being `slide_distance` or else the body extent along `edge` (width for left/right, height for top/bottom); an authored `duration` wins. The close is `return_duration`, else `close`; the open curve stays EaseOut, the close EmphasizedAccelerate unless `return_transition` is authored (it was parsed but ignored: now honoured).
`kit.Panel` no longer forces `duration`. Every hover-opened panel closes `leave_ms` after the pointer leaves it, its tab and its strip: the edge exit grace is one value for docked and bar-hosted panels, and a close-on-leave view without `panel.leave_delay` uses it (an authored delay wins).
Changed defaults: unauthored open .2 -> .18-.32 s; unauthored close min(.7 x open, .22) -> .18 s; dock exit 450 ms and bar exit 130 ms -> 150 ms; a close-on-leave view with no `panel.leave_delay` now closes (it never did).

## Laptop first and last frames (B0310)

`PANEL_SLIDE` appends `top_split=<phase>:<ms>,...` (the four largest phases of the costliest render): always-on fixed-array marks (`bead_core::profile::split`; `decorations`, `pose`, `prepare`, `damage`, then `activate`, `gl_canvas`, `gl_upload`, `gl_paint`, `gl_flush` and `swap` on GL (B0380; `post` closes every frame) or `paint` and `commit` on the CPU path) read once per committed frame (`top_splits=<frame>:<phase>:<ms>/...;...` repeats them for each `render_top` frame); `other` time (before the first mark, wgpu) is not listed.
A reveal's open-state shadow raster was replaced in its texture slot by the small pose raster on the first frames (under `EDGE_MIN_REVEALED`) and re-uploaded whole (about 7 MB at a 984x650 panel, scale 1.5) when the growing edge first slid it, a one-off ~5-10 ms frame 1-2 of the open. The prepared frame's raster now stays in its slot, clipped out, and the pose is a separate `/pose` item: a presenter that keeps textures uploads the open-state raster in the held first pose, and the close tail reuses it. Measured by a CPU-proxy test on dev-1 (upload bytes per frame): the 7.2 MB moved from frame 2 to the held frame 0. Unexplained by the proxy: the settle frame and a close's frame 2 (read them from `top_split` live).

## CPU paint of edge frames (B0311)

Nacre's shell panels use the CPU renderer. At scale 1.5 in headless sway (a 984x650 top reveal with far-edge shadow) the first close frames and the settle frame damaged the whole body (1.6 M px, 9-10 ms paint) and every mid-slide frame the whole revealed rect (3-4.5 ms) instead of a strip. Two causes in `FrameDamage`: the reveal's `/edge` piece is inserted or removed at the hand-over and unmatched items paired by position, so every later item met a neighbour and damaged both bounds; and the growing body fill (a plain `Rounded` reshaped every pose) damaged old plus new bounds.
Unmatched runs now pair by item id first (an identical pair still falls back to position: its paint order may have changed), and a rounded rect that only grew or shrank at one end damages its far corners and the strip (`grown_band`). Now: close frame 1 damages 175 k px (paint 9.2 -> 4.1 ms), the settle frame 41 k px (4.5 -> 0.3 ms), mid frames 0.6-1.4 ms. The pixel-oracle test gains grow, shrink and hand-over cases.
Left: a reveal without `slide_distance` flattens its content layer (`slide_layer`, 3.4 ms here, more for a heavy body) synchronously on the first slide frame of an open and of a close, because the idle close-layer prebuild (`closing_from_rest`) only covers reveals with `slide_distance`.

## Plain reveal layer prebuild (B0312)

A reveal without `slide_distance` flattened its content layer (`slide_layer`, 3.4-4 ms at a 984x650 body, scale 1.5) on the first slide frame of a close, and an open's layer was built on a moving frame. `ensure_slide_layer` (the slide paint's keep/adopt/repair/flatten step, same key) now also runs on the held first pose of an open, and `closing_from_rest` covers plain reveals (their content is the part above the reshaped body), so the idle close-layer prebuild serves their close: close frame 1 render 4.4-5.2 -> 1.0-1.1 ms (layer 0), the layer moves into the hold (3.6-3.9 ms, free).
An open's frame 2 still repairs the layer when content arrives after the open request (the example's clock rows change between hold and frame 2: 2.4 ms, damage-limited); content, scale or format changes are never served stale (key unchanged).

## Warm reopen layer (B0407)

A reveal with `slide_distance` (the control centre) reopened on its kept renderer flattened its content layer on frame 1, inside the slide clock (live, DP-1 120 Hz: frame 0 0.8-2.1 ms, frame 1 5.7-13.0 ms, 13 of 14 warm opens over 12 ms, 10 at `gap_at=1`), while a cold open builds it in the hold (B0312). `TreeCache` keeps the last reveal pose it derived, keyed by frame, progress and scale. A slid close does not reshape the body, so its last pose (progress 0) is the unposed frame; the reopen's held first pose had the same key and got that frame back, `SlideContent::of` rejected it (its content is not at the closed offset) and the hold built nothing. The key now includes the direction (`motion.open`), so the hold derives its own pose and builds the layer and frame 1 only poses it. Plain reveals (chat) were unaffected: their open and close poses are the same frame. Expected: first_frame +3-5 ms (the cold hold's flatten), frame 1 like a cold frame 1; no memory while closed (the layer is built at reopen and dropped at rest, as before).

## Wallpaper crossfade (B0313)

A wallpaper change fades on the GPU (`Options::wallpaper_fade`, [rust-hosts](../reference/rust-hosts.md#wallpaper-crossfade)); panels stay on the CPU renderer. The wallpaper surface borrows the shared EGL transport for one change only: both images upload first, a held first pose starts the clock (B0170: the first commit starts it), each frame callback draws the next blend (`fade`, or a `grow` disc), and the final SHM buffer is attached before the EGL surface and, when nothing else uses GL, the transport are released. A new wallpaper mid-fade bakes the shown blend into the `from` texture and restarts the curve, so there is no jump. The previous image is read from the still-attached SHM buffer, else from the provider (`prepare_transition`); there is no resident copy.
The palette follows in timer-driven steps (about one per 16.7 ms of the duration, 10 to 60: 30 for 500 ms, B0327) over the same duration (the old chrome over the new wallpaper, or the reverse, would show for the whole fade with a shorter one), mixed in linear light (gamma 2.2) with the same curve, the last step exactly the new palette. A step is a CPU repaint of the bar and open panels; it runs right after a fade frame was submitted (B0315).
Measured on dev-1 (headless sway, 2880x1800 at scale 1.5, Mesa llvmpipe through `BEAD_GL_ALLOW_SOFTWARE=1`, so a CPU rasteriser, not a GPU): 39-42 frames in 0.71 s, a 10-17 ms render per frame, worst gap 24-29 ms (a few 54-64 ms gaps at the first frames while the compositor's own software renderer competes), first frame 14-193 ms (EGL setup and uploads of the first fade; 40-70 ms after). RSS 108 MB idle, 256-270 MB during a fade, 108 MB after it (the 5-second cycle repeats without growth over 10 fades). `fade` and `grow` ran, a retarget every 400 ms and an output unplugged mid-fade did not crash. Not measured: a palette step on a real bar and large panel at scale 1.5 (the proof host has no UI; the existing 10-24 ms full panel repaint is the bound per step) and a real GPU's frame time and VRAM (two 20.7 MB textures plus the EGL window, expected about 45 MB, freed with the transport).

### Staged frames (B0314)

A host with `primary_factory` stages wallpaper frames in steady state (`prepare_staged`, applied on its confirmation). While a fade runs, a staged frame whose palette differs waits for the confirmation without building the palette group and, once accepted, installs through the tween (`install_with_palette_tween`): image fade and the palette steps together. A rejected confirmation installs nothing and calls `discarded`; the palette is unchanged. Outputs of the same change tween once together; frames of an older change still waiting, a palette group, reduced motion, a failed GL setup or no fade keep the fence (`tween_route`). A tween waits without a staged CPU buffer, which the fade would throw away.
`WALLPAPER_TRANSITION` ends with `palette_steps=<n>`: the tween steps applied while that fade ran (0 for an unchanged palette or the fence).

### Fade frames and palette steps (B0315)

`WALLPAPER_TRANSITION` ends with `gap_other_ms=<ms>`: the main-thread CPU time spent outside fade frames before the frame that ended the largest gap (two thread-CPU clock reads per frame, no allocation). Close to `max_gap_ms` the gap was our own main thread (repaints, palette steps, decoration or edge art); near 0 it was waiting: the compositor, the GPU or a blocking call on the main thread (sleeping is not CPU time).

`WALLPAPER_TRANSITION` then ends with `setup=read:,egl:,surface:,canvas:,upload:,first:` (main-thread wall ms of the fade start, one measurement per phase) and `retarget_ms=` (the costliest retarget of the run, 0 if none) and `clock_s=` (when the fade's clock started: outputs of one change print one value); the finish prints its own `WALLPAPER_FADE_FINISH output= ms=` line (B0316).
Measured (dev-1 headless sway, 2880x1800 at scale 1.5, release, llvmpipe so GL frame times mean nothing): a step costs the main thread 0.03 ms (the event hand-over); the bar repaint runs on its worker thread (0.17 ms fill only, 0.38 ms with text, 0.59 ms with 12 tinted icons at fresh tints) and a host that bumps `decoration_variant` per palette rasterises its frame tiles on the main thread, asynchronously through the control watch (0.7 ms for 4 tiles of 120x120, 0.5 MiB). The timer stepped independently of the frames, and its last steps slipped past the end of the fade (7 of 10 counted).
Steps are now driven by the fade's frames: after a frame is submitted, the newest due step is applied (an overdue one is skipped, it shows a colour the wallpaper has passed) and the last frame of the last fade applies the final palette. The timer remains for a cut, a hidden surface and any gap in the frames: it applies a due step only when no frame came for 50 ms. Memory: RSS 123.1 to 123.6 MB over 27 fades, the tinted-icon raster cache stays at its 256 entries (1.1 MB).

### Several outputs share one clock (B0316)

Cause (dev-1 headless sway, two outputs, release, llvmpipe): outputs of one change were set up one after the other on the main thread (read 5 ms, EGL context 9 ms once, surface 4-9 ms, canvas 4-7 ms, two uploads 15 ms, then the first pose). The first output committed its first pose and started its clock, then stalled until the second one's setup was done (`gap_other_ms` 32-37, the setup of the other output); the second one's curve started that much later, and the first output's finish (final buffer attach, 9-20 ms) landed inside the other's last frames.
A fade at its held first pose now waits for the others: while another output with a shown image still has a wallpaper request in flight (`wallpaper_jobs`; a hotplugged output has no image to fade from, so it never holds anything), the first pose is drawn and committed but the clock does not start, for at most 150 ms (two setups of 55-65 ms on the laptop plus slack; a slow or missing partner holds nothing longer, and the held fade then starts alone). The commit that ends the hold starts every held fade at one instant (`Animations::committed`), and the palette tween starts on the same clock (`clock_s` in the marker is one value for the whole change). A fade that drew its last frame waits for the other fades of its clock before it attaches the final buffer, so the attach never lands in their last frames. A retarget during a hold or a shared fade bakes the shown blend as before and holds again if a partner is coming (last request wins).
Setup: the final pixels move into the fade instead of being cloned (one full-size copy less per output); the shown image is still read with a copy and a swizzle (a one-pass read measured no faster, 5.0 ms either way: the cost is the fresh pages), a per-transport renderer would save the canvas phase (4-7 ms of about 40) and is not done. Uploads stay on the main thread (the held pose hides them; moving them is B0317).

### Gap split (B0319)

With two outputs `gap_other_ms` also holds the partner's own fade frames and the palette steps (the host's `palette_changed` runs inside them). Two running thread-CPU sums (inside `fade_committed`'s frames, all outputs; inside `apply_palette_step`, callback included) are sampled at each frame start and commit like the total CPU: `gap_peer_ms=` (partner outputs' frame CPU in the largest gap), `gap_step_ms=` (palette-step CPU in it), the rest is other - peer - step; `step_max_ms=` is the costliest single step of the run (wall). Diagnostic only; no behaviour change.

### Warm mode (B0317)

`ShellControl::wallpaper_warm` (default false) keeps the fade's GL resident between changes. A fade that ends warm rests: `from` is deleted, the transport, EGL surface, canvas and the shown texture stay, the SHM final attach is skipped and the CPU pixels are kept for the release; `committed` still fires with the first frame, so the provider contract is unchanged. The next change retargets the resident run (no read, transport, surface or canvas setup; no bake at rest). A worker thread on a shared EGL context uploads the new frame (`glFinish`, the texture id comes back through a calloop channel); the main thread adopts it as `to`; the newest frame wins and an older answer is deleted. A failure or a driver without surfaceless contexts uploads on the main thread (`WALLPAPER_FADE_FALLBACK`). Warm off releases at once when idle and after the fade when running; output removal, GL failure and shutdown release too; no GL object stays (`WALLPAPER_WARM`, `live_textures`). Non-warm fades keep the cold path and the main-thread upload: the thread and its context would be built and joined per fade.

(B0325) Priming: warm on builds the resting run of every output that has none and a shown SHM buffer (transport, EGL surface, canvas, one texture of the shown image, uploaded by the upload thread; one output per idle turn). Nothing is swapped, drawn or reported: the SHM buffer stays on screen until the first fade's first frame, a primed run keeps no CPU pixels (releasing it attaches nothing), and a frame arriving for an output whose prime is in flight cancels the prime (its texture is deleted) and starts cold. Marker `WALLPAPER_WARM_PRIME output= ms= upload_wait_ms=`.

(B0398) Prime and release off the main thread. Cause (laptop, 2880x1800, CPU host, no other GL surface): the release shut the transport down at the end of every warm cycle, so every prime started cold and paid EGL setup, the upload context, a femtovg program compile and a 20.7 MB copy plus swizzle on the main thread (`MAIN_BUSY warm_prime` 22-127 ms wall, `warm_release` 22-28 ms), dropping 3-12 frames of any animation at the rotation. A prime without a transport now starts the panel's B0342 preparation (one helper and one preparation for both owners): display, config, context, the prewarmed canvas and, while a prime is wanted, the upload thread's shared context are made on `bead-gl-prepare`; the 2 ms poll adopts it when the panel or a prime still wants it and primes again. The fader claims the prewarm canvas (B0376) instead of compiling; a second output, or a panel renderer that drew first, compiles as before. The prime copies the SHM bytes only and the upload thread hands them to GL as BGRA, with no CPU swizzle. The release unbinds the context on the main thread (`Transport::into_prepared`) and joins the upload thread, destroys the contexts and terminates the display on `bead-gl-release`. One EGLDisplay serves the wl_display, so setups never overlap the terminate: the pending shutdown is a join-on-drop handle that every setup drops first, only one preparation exists, and a synchronous owner (a fade start, a bar or panel draw) waits for the preparation in flight instead of initializing again. Left on the main thread: the SHM copy (about 3-5 ms at 2880x1800, mostly fresh pages), the EGL window surface (about 1-2 ms), and at the release the run's GL teardown and the final SHM attach of the kept pixels (a fresh 20.7 MB buffer with a swizzle, estimated 5-10 ms; a follow-up could stage it during the rest). Markers: `WALLPAPER_WARM_PRIME` `ms=` is the sum of the prime's main-thread turns, plus `prepare_wait_ms=`; `WALLPAPER_WARM` gains `ms=` (the release's main-thread time, 0 for `on`). Not measured live yet.

(B0403) Per-turn markers. `WALLPAPER_WARM_PRIME` adds `copy_ms=` (the hand-over of the shown image: the 20.7 MB SHM copy at 2880x1800), `run_ms=` (the resting run's creation in the answer turn: EGL window surface and fader) and `turn_max_ms=` (the longest main-thread turn; the preparation's adopting poll and the hand-over it schedules share one loop turn and count as one). `WALLPAPER_WARM state=off` adds `attach_ms=` (the final SHM write and attach) and `turn_max_ms=`; its `ms=` is the release's main-thread total over all its turns. The bar is no turn over about 8 ms (one 120 Hz frame). Live (docked, eDP-1 plus DP-1 3440x1440) `copy_ms` was 7.7-8.5 and `attach_ms` 14.0-17.6, so the copy and the final write now run on the upload thread: it reads the attached SHM slot in place for the prime and swizzles into the slot main picked for the release, while that pool is pinned (no write, growth, replacement or drop until its answer; a busy pin takes the existing retry path; resize, removal, a cold-start read, a run release on a pinned surface and shutdown flush the queue first). A late answer that finds no pin is dropped. `copy_ms` is then the hand-over and `attach_ms` the answer turn (drop the run, attach the written slot). `run_ms` (EGL surface and fader) stays on main.
(B0406) The preparation warms one canvas per wallpaper output it primes, not one, so a docked second output's resting run no longer compiles femtovg's programs on main (live: `run_ms` 8.4 for the output that claimed the warm canvas, 17 for the one that compiled). A transport another GL user already holds has no warm canvas left; its outputs still compile. `WALLPAPER_WARM_PRIME` appends `surface_ms=` (EGL window surface and its first bind) and `canvas_ms=` (the fader's canvas: claimed or compiled), which split `run_ms`.

### Palette ease (B0327)

A frame the provider answers `palette_only(output, revision)` for (same pixels, new palette: Light/Dark/Auto re-staging the current frame) starts no fade and uploads nothing: the shown wallpaper stays, the revision gets `uploaded` and `committed` as a cut of identical pixels would, and a `PaletteTween` over the provider's `fade` (else `Options::wallpaper_fade`) runs on the timer path with no hold (no fade frames exist). Outputs of the change join one tween (`tween_route`); a frame mid-tween retargets from the mixed palette on screen. Reduced motion, no fade, an older change waiting, the palette fence or no `native-gl` is a cut (today's fence); `palette_only == false` takes today's path unchanged.
(B0341) A frame for the target of a running fade (its first frame committed, no upload queued) is adopted without a retarget or restart: only the run's palette and the revision change. A frame whose palette equals the current one is adopted with no tween (`TweenRoute::Unchanged`), resting runs included.
Every tween takes one step per 60 Hz frame of its duration: `steps = clamp(round(duration_s * 60), 10, 60)` (500 ms gives 30, 700 ms 42); the last step is exactly the duration at t=1, and the overdue-skip, after-frame and timer-backup rules are unchanged.

### Palette at rest (B0333)

Root `Panel{palette_tween = false}` (`Scene::palette_at_rest`) gets only a palette tween's final palette; a normal open panel keeps every step. `apply_palette_step` takes `last` and notifies the panel when `last || (open && !palette_at_rest)`, so a closed retained panel is re-staged once at the end with no prop; a cut counts as last and a retarget delivers the new tween's final step (the old one never ends). Fenced changes (`prepare_wallpaper_palette_group`, a panel joining a fence) are unchanged, and steps are unfenced, so skipping them leaves B0332's settle rule alone. Skipped steps are counted as ` panel_steps_skipped=N` on WALLPAPER_FADE_FINISH (only when nonzero). Slots and other retained panels get no steps through another path: they take the current palette when started.

### Keyframe tracks (B0364/B0365)

`ui.track(driver, keys, values)` (B0365) turns a `ui.animated` or `ui.pulse` marker into a track: the marker's `__animate` object (`from`, `to`, `start`, `duration`, `curve`, `loop`) plus `keys` and `values`. `keys` are n in 2..=128 finite, strictly increasing numbers in the driver's value space, `values` n finite numbers. The sampled value is the piecewise-linear interpolation of `values` over `keys` at the driver's sample `d`, clamped to `values[1]` below `keys[1]` and to `values[n]` above `keys[n]`. The driver table is copied, never mutated; `ui.range(driver)` returns its `from, to`.

Tracks ride the patch path (B0364): no Luau, `view_ir` or layout per frame for `translate_x`/`translate_y`, `scale`, `opacity`, `radius`, `alpha` and `mix`. `scale` (0 < scale <= 8) scales a view and its subtree about its laid-out centre after layout (nested scales multiply and compose with translate and scroll; rects, radii, text size and hit boxes scale, stroke and border widths, shadow blur and `bake_size` do not; text re-rasters while it changes). A malformed track is an "invalid animation marker". Any other prop still relayouts, as a plain marker does.

Retarget is unchanged from the sections above: a new binding value rebuilds the view once, and the view emits new tracks over the new `from..to` (the driver retargets from its current value, so the keys the view passes should span that range).

Caps: 128 keys per track and 32768 track points per view in total (B0364 enforces both).

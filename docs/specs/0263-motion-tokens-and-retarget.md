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

`bead_layer_motion_v1` version 3 (Bead's own MIT XML, matching the Platform compositor's bump)
appends the three values with `since="3"`; no opcodes change. Bead binds `min(server, 3)` and sends
values 5-7 only on a version 3 binding; on a version 1 or 2 compositor (the laptop until its
nacre-niri package updates) a motion using one of them stays on the client frame path
(`curves_supported`), never approximated. `protocol_curves_follow_the_bound_version` covers both.

Durations (s): hover .09, press .06, release .12, trace .12, badge .14, close .16, fade .16,
osd_in .16, switch .18, peek .2, osd_out .2, open .22, toast .22, dock .22, slide .24, drop .26,
pulse 1.6 (the review's 120 Hz table). Reduced motion: every target is reached at once; a pulse
holds at `hi`.

## Driver

`ui.animate` is unchanged (its number is baked into the view, so each frame re-runs Luau). A marker
carries `from`, `to`, `start`, `duration`, `curve` and `loop`; layout samples it when the view is
built and the materialized cache samples it on every frame: no Luau run, and for `fill`/`color`/
`border_color` amounts, `radius`, `join` and `translate_x/y` no relayout (the cached scene's
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

Compositor reveal v2 cannot be the mechanism for a growing body: its atlas is baked pixels and a
plane cannot change shape, so D5 motion runs on the client frame path. v2 stays right for a
static-body open/close; making it the default needs the compositor side and is not decided here.

## Evidence

- Tests: every curve (endpoints, monotonicity, out-back overshoot bound, spring settle), marker
  pacing and retarget, pulse loop, paint-only patches (0 Luau, 0 layout, exact final geometry, node
  boxes follow), layout markers (0 Luau), reveal with a body at 0/25/50/100 % and after a retarget,
  frame damage of a 26-frame grow bounded by the union of consecutive bodies.
- Damage: `body_frames_damage_only_the_union_of_the_old_and_new_body` prints the worst bytes per
  grow frame on the 488x584 logical surface at scale 1.5 (`PEEK_GROW_DAMAGE`), against 46.7 MB for
  a full 2880x1800 output frame.
- Example: `examples/bar-with-panels` `status` panel (More/Less regrow the body as one surface).
- Capture: `.lead/capture-peek.sh` (untracked) runs the example under headless sway at 120 Hz and
  reports PANEL_SLIDE gap p50/p95/max; the 120 Hz desktop capture is booked by the lead.

# 0279 The edge lip is its panel's tab (B0279)

Owner: the left/right pill should open with the panel, ride with it and sit on the panel's inner side.

## Mechanism

While the panel a lip opens is resident, docked on the lip's edge and local (not host-owned, not a
pearl), the lip is drawn as that panel's tab: a `wl_subsurface` of the panel's layer surface
(`edges::LipTab`), rasterised from the same `edge_art` callback at the lip's own size. Its position is
set every panel frame from `Scene::posed_body()` (the body as drawn: cut to the revealed part under a
reveal, moved with the scene under a slide) and `tab_rect` (flush against the body's inner edge, at the
lip's rest place along the edge). A subsurface position is applied with
the parent's commit, so the tab moves in the same frame as the panel pose with no configure round trip
and no renderer dependency (CPU and GL panels alike). A width retarget while open changes the posed
body, so the tab follows with no host code; both directions sample the same pose (continuous reversal).

Inset bodies (B0287): a body inset from the screen edge (a framed dock) would step at hand-over, so `tab_rect` offsets the flush pose outward by the settled body's inset times `1 - attach`: attach 0 is exactly the free lip, attach 1 flush on the body, the close mirrors it. `examples/bar-with-panels` side-left is inset 10 px to show it.

Slide motions (B0289): the general rule is `tab = flush_live + (rest - flush_at_start) x (1 - attach)`; the scene's slide is linear in `1 - attach`, so it reduces to `settled flush + (rest - settled flush) x (1 - attach)`. `tab_rect` uses that when the scene is slid: the tab is exactly the free lip's rest rect at attach 0 (not off-screen with a body that starts there), flush on the settled body at attach 1, travels linearly between, and a reversal mid-motion is continuous (a pure function of attach and the posed body). Reveal keeps the inset easing above. `side-right` is a 10 px inset slide dock (`slide_distance` = width + inset) to show it.

Hand-over: the panel's first posed frame carries the tab at `attach` 0 (over the lip, which holds its
completed-dwell look, spec 0278); the edge lip surface clears only after that frame committed
(`lip_tab_handover`), and on close it redraws before the panel unmaps or its surface goes
(`lip_tab_detach`), so a refresh that catches one of the two commits shows the same pixels twice, never
nothing. Prepared first frames (handoff) skip the pose, so the tab appears on the second frame there.

Interface: `LipState { progress, active, attach, hovered }` (now `Default`; hosts write
`..Default::default()`). `attach`: 0 free, 1 docked, else the panel's eased open progress this frame.
`hovered`: pointer on the lip or tab (the art is re-called). No side field: the panel docks at
`lip.edge`. Hovering the tab holds the panel (the tab is the lip for the exit grace), clicking it closes.
External (host-owned) panels do not attach; a separate Bead app process that owns both its lip
(`edge_chrome`) and its panel (local `Panels`) gets the tab unchanged, as the `bead` folder host shows:
nothing here depends on the hosting process, and no cross-process position feed exists or is needed.

Exit grace and pin (Shell ask): a panel opened from a strip-surface lip closes after the pointer
left both body and lip/tab; since B0307 that is the panel profile's `leave_ms` (150 ms) for docked and
bar-hosted lips alike (it was 450 / 130 ms). A press or key on a hover-opened panel
clears `hover_opened`, so it then closes only on Escape, a lip/tab click or a click outside.

Compositor path (spec 0218): a subsurface is not in the atlas, so while a tab exists the panel's motion
stays on the client frame path (`compositor_motion_candidate` false). Reveal opens already run there by
default (`BEAD_LAYER_REVEAL` unset); a `slide_distance` close of a lip-opened panel now does too.
Follow-up if that cost shows: a translating tab plane in `LayerMotionPlan`.

Top lip: identical mechanics, horizontal; a lip on the bar's edge is bar-hosted (hit via the bar, cue via
`bead.edge.*`), its tab rides under the top panel's bottom edge with the panel's layer order.

## Top tab and bar-hosted hand-over (B0291)

- A bar-hosted lip's cue (`bead.edge.dwell`/`fade`) targets 0 while its tab is attached (`cue_target`): a pending open holds progress at 1 and no strip surface clears it, so the bar would keep a pill under the tab. It returns only after hand-back and a new dwell.
- Tab input follows `lip_surface_geometry(..).input`: the lip's own rect, exactly where the lip takes the pointer on the edge, empty for host-driven hover. The art must fill that rect; a pill smaller than a 50 px top lip should be a thinner lip, not a taller transparent tab.
- Tab rect at thickness 10 and 50 (bar 40 + frame 10) under reveal and slide motions: attach 0 is the lip's rest rect, attach 1 flush, linear and continuous (test `tab_starts_on_the_free_lip_...`).
- Every unmap of the resident panel detaches the tab first (client close, compositor-motion close). Close order: dismiss layer dropped at close request, then tab (viewport, subsurface, surface) and edge-lip redraw when the panel is ready to unmap, then the root unmaps. Tab panels stay on the client path (`edge.tab.is_none()` gates compositor motion).
- Desktop pass (nested niri r3109 and labwc, headless 120 Hz, 4 px ink grid; stopped by the swap gate after 4 of 16 captures: swap used 66188 KiB >= 64 MiB before the 5th, no reset):

| arm | right slide dock | top lip -> status peek |
|---|---|---|
| niri, Bead 6907e640 | tab never leaves the lip's rest rect: a pill at the screen edge (x 1912-1920, y 516-564) in 96% of frames, none at the body's inner edge | pill stays at the bar edge (y 0-8) for the whole open, none under the body |
| niri, this branch | same (single run, slow): pill at the screen edge through open and close | same (single run): pill at y 2-7, body bottom y 155 |
| labwc, this branch | not run | one tab, merged under the body (bottom y 163 = 155 + 8), no second pill |

  Under nested niri no tab showed in either build, so the live "pill on the wrong side" is not the 1x1 clear alone. Not attributed yet: in the right-dock run the client drew only ~7 panel frames for the whole slide (the compositor-motion path, which requires no tab), so the tab was likely never attached there; the top peek drew every frame and still showed none. Follow-up B0292: log why `lip_tab_prepare` attaches or skips, and a WAYLAND_DEBUG=client run on niri.

### Tab attach on layer-motion compositors (B0292)

- `lip_tab_prepare` runs at the top of every companion draw, but `layer_motion_transport_eligible` (the atlas plan) did not check `edge.tab`; only the handoff gate did. A tab panel could therefore take the compositor-motion path with a tab posed once, while the compositor moved the body. It now requires `edge.tab.is_none()`, so a docked panel stays on the client path on any compositor.
- Unconfirmed: the bar-hosted top peek drew every frame on niri and still showed no tab, so `panel_lip` or an early return may also skip there. One stderr line, printed only when the outcome changes: `LIP_TAB attached key=<output/lip> edge=<Edge>` or `LIP_TAB skipped [panel=<name|->] reason=<no_panel_name|no_panel_output|no_motion|pearl|no_control|external_panel|no_lip_for_panel_edge|no_subcompositor_or_viewporter|no_logical_size|pool>`.
- Docked input depth: `EdgeChrome::docked_input_depth` (lip id to px; `[lips.<id>] docked_input_depth` in bead.toml) limits the attached tab's input to that band on the join side (the side touching the panel body, e.g. the top of a top tab), default the full thickness. The free lip keeps its whole rect.

## Verification

Tests: bead-render `posed_body_tracks_the_revealed_inner_edge_and_the_slid_scene` (4 edges, open/close,
slide); bead-wayland edges `tab_rides_the_posed_body_inner_edge_on_every_anchor`, `lip_state_default_*`,
`docked_panels_get_a_longer_exit_grace_*`, `pinning_clears_hover_ownership_*`; bead-cli `lips_need_a_declared_panel_*`. Example: the `bead`
folder host reads `[lips.<id>] edge, opens, length (144), thickness (10)` from bead.toml (a stock
swelling bar as art, 350 ms dwell; a lip-opened panel docks flush with its own edge, centred);
`examples/bar-with-panels` has left and right lips opening side docks and a top lip (inside the bar)
opening the status peek. Headless 120 Hz capture strips: pending (see the unit handoff).

## Pearl slots keep the resident tab (B0297)
The tab belongs to its panel group: `swap_group` swaps `edge.tab` with the slot's group, so a pearl or guide draw (whose `lip_tab_prepare` skips with `pearl`) sees no tab and detaches, poses or redraws nothing.
Before, each pearl draw (about 1/s) dropped the resident's tab and redrew the free pill at the screen edge; the next resident draw re-attached it. The slot's own detach sites (close, unmap) are covered the same way. `edges_panel_changed` returns early inside a slot (like `edge_panel_hover`): a pearl or guide starting there would otherwise redraw the lips without seeing the resident tab and show its free pill beside it.

## Input under the bar (B0298)

- A layer surface takes input only where something visible is interactive. The bar and a panel-mode primary set their hit area as they draw; a primary that is neither (a panels-only app's `(0, 32)` strip under the bar's exclusive zone) starts with an empty input region (`starts_input_less`), so it never swallows clicks meant for windows.
- Reserved frame strips are always input-less; a lip's input is its own rect (see the top-tab rule above).

## Children-only input (B0321)

- A root with `input = "children"` (Luau prop on the view root) makes the bar's input region the union of its direct children's rects that, or whose descendants, are interactive or paint something; empty spacers and the gaps between children take no input, so clicks there reach the windows below.
- `Scene::input_islands` carries the layout-space rects; the bar adds the scene offset and the bar-hosted lips (still reachable, as in B0298), and calls `set_input_region` only when the rect list differs from the last one applied. Reserved frame strips stay input-less. Without the prop nothing is set.

## Lip state while attached (B0300)
- The tab glows only for the pointer on the tab surface (`LipTab::hovered`, set in `edge_pointer`'s tab branch from Enter/Motion/Leave by `tab_hovered`). The free strip keeps setting `edge.hovered`, which holds a hover-opened panel open (the pointer overshooting onto the screen edge must not close it) but never reaches the art of the tab. Before, `draw_tab` read `edge.hovered == key`, so the strip at the far screen edge made the attached tab glow, until the pointer left it.
- Detaching the tab clears `edge.hovered` for its key: the destroyed surface sends no Leave, and a stale hover would block the exit grace.
- A pending external open (`opening`) always redraws when it expires, so a lip whose host reported active without any redraw shows the active state instead of the held dwell look.
- A button-opened panel draws the same states as a lip-opened one except `hovered` (false) and no dwell: free lip `active, attach 0`, then the tab at `attach` 0 rising with the pose (body width 0 and opacity 0 on a reveal's first frame).
- Top lip probe (bar 40 + frame 10, top lip 208 x 50 at rest, scale 1 and 1.5, x = centre +-0/60/100/120, y 36..70): the bar surface takes the pointer to y 48; nothing below y 50 at any x. A top tab starts `active, hovered false` and glows only while the pointer is on it.

## Docked rest lip (B0303)
While its lip is the panel's tab, the free lip draws its rest box with the host art at `LipState { active: true, attach: 0, progress: 0, hovered: false }` instead of clearing it. A host panel contour may leave a hole for each lip (its input region); the rest box keeps that hole filled with the host's frame material, so no cutout shows beside the open panel. Host art draws no pill in that state and never glows it; the stock CLI art returns nothing there. The tab itself is drawn with `attach` of at least 0.001, so only the rest box ever has `active` at `attach` 0 and the tab's held first pose keeps its pill.

## Docked strip, tab lips and bar lip art (B0304)
- While a lip's panel is open on the output (`lip_active`, local or external), its free strip is not a button: default cursor (re-sent when the wanted shape changes, at a lip redraw, so a panel opening or closing under a resting pointer updates it), a left press neither closes nor reopens, and a press that lands on the dismiss catcher over the strip is ignored and keeps the dwell gate shut (otherwise the catcher closed the panel and the resting pointer's dwell reopened it). The strip still sets `edge.hovered` (B0300 hold). The tab stays a button. The free lip draws the B0303 rest box (`active`, `attach` 0, not hovered) whenever its panel is open, not only while a local tab exists, so a parent shell's strip beside a child process's tab shows the plate only.
- `EdgeChrome::tab_lips`: lips another process draws (a child app's parent shell). They only attach as the tab of this process's own panel named `opens` on the lip's edge (`panel_lip`); they create no surface, input region, dwell or cue. Hovering any docked tab also sends `UiEvent::PanelHover { anchor }` to the panel session, which feeds `panel.anchor_hovered`, so a hover-opened panel whose lip lives in another process is held while the pointer is on its tab.
- A bar-hosted lip draws its host art as an input-less subsurface of that output's bar surface at the lip box (`BarArt`), with the same `LipState` as a strip (hover from the bar pointer, the B0303 state when open). A subsurface stacks above the bar by construction and moves with it, so a rebuilt bar surface (bar toggled, hotplug) just replaces it; a layer surface would stack by map order. A host whose bar-hosted lip needs no art returns `None` for `attach` 0; the CLI does that for a lip on its bar's edge (its tab keeps the pill).

## Cross-process hover (B0306)
- A child's panel anchor is two inputs: the parent's lip hover (`hover-open`/`hover-enter`/`hover-leave` requests) and the docked tab's own hover; `panel.anchor_hovered` is their OR. As the tab rides over the parent's lip at open, the tab's enter arrives at once and the parent's strip leave arrives later over IPC; with one shared flag that late leave dropped the anchor under a resting pointer and the 130 ms leave delay closed the panel, which the parent's dwell then reopened (a loop).
- An externally owned panel that closes while its lip is hovered gates the lip's dwell shut until the pointer leaves, as a local panel does (`edges_panel_changed`); checked at every lip redraw, which the host's open-state change triggers.

## Docked elsewhere (B0309)
- A panel reports closed when its close starts (`PanelVisibility(false)`), but its tab rides the closing body until the close ends. A child app that docks a parent's lip (`tab_lips`) tells its host when its tab attaches and when it is gone (`ShellControl::lip_tab_docked(lip id, output, docked)`, from `lip_tab_prepare` and `lip_tab_detach`); the parent's host answers `ShellControl::external_tab_docked(lip id, output)`, and the parent's free lip stays at rest (no pill) while it is true, open or closing. So the pill is never in two places, for a keybind open with a fast Esc, a close interrupted by a reopen (the tab never goes), or a hover open.
- The host must clear a child's docked state when the child exits, or the parent's pill would not come back.

## Edge tip (B0340)

`Options::edge_tip: Option<EdgeTip>` (`Fn(&Lip) -> Option<Scene>`) gives a hovered lip a tooltip: 400 ms after the pointer settles on a Bead-drawn lip, an input-less overlay layer surface fades in over 120 ms (at once under reduced motion). The host draws everything (text, plate, shadow, the transparent margin); Bead only places it: anchored to the lip's screen edge at margin 0, centred on the lip along the edge and clamped inside the output, sized by the Scene's bounds, rendered once by the CPU scene renderer. It shows only while hovered, unpressed, with the lip not active or attached and no panel (resident, pearl, guide, or host-owned) open on that output. It fades out on leave and goes at once on a press, open, chrome change that drops the lip, output removal or teardown; a lip redraw while visible asks the host again (a `None` hides it). `edge_tip: None` changes nothing. `BEAD_VERBOSE_LOG` logs `EDGE_TIP lip=<id> shown|hidden reason=...`.

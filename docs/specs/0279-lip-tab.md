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

Exit grace and pin (Shell ask): a panel opened from a strip-surface lip closes 450 ms after the pointer
left both body and lip/tab (`DOCK_EXIT_MS`); bar-hosted lips keep 130 ms. Chosen over a spec field: the
strip/bar distinction already exists and matches dock/popup. A press or key on a hover-opened panel
clears `hover_opened`, so it then closes only on Escape, a lip/tab click or a click outside.

Compositor path (spec 0218): a subsurface is not in the atlas, so while a tab exists the panel's motion
stays on the client frame path (`compositor_motion_candidate` false). Reveal opens already run there by
default (`BEAD_LAYER_REVEAL` unset); a `slide_distance` close of a lip-opened panel now does too.
Follow-up if that cost shows: a translating tab plane in `LayerMotionPlan`.

Top lip: identical mechanics, horizontal; a lip on the bar's edge is bar-hosted (hit via the bar, cue via
`bead.edge.*`), its tab rides under the top panel's bottom edge with the panel's layer order.

## Verification

Tests: bead-render `posed_body_tracks_the_revealed_inner_edge_and_the_slid_scene` (4 edges, open/close,
slide); bead-wayland edges `tab_rides_the_posed_body_inner_edge_on_every_anchor`, `lip_state_default_*`,
`docked_panels_get_a_longer_exit_grace_*`, `pinning_clears_hover_ownership_*`; bead-cli `lips_need_a_declared_panel_*`. Example: the `bead`
folder host reads `[lips.<id>] edge, opens, length (144), thickness (10)` from bead.toml (a stock
swelling bar as art, 350 ms dwell; a lip-opened panel docks flush with its own edge, centred);
`examples/bar-with-panels` has left and right lips opening side docks and a top lip (inside the bar)
opening the status peek. Headless 120 Hz capture strips: pending (see the unit handoff).

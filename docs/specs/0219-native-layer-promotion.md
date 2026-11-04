# B0219: native layer promotion

## Purpose

Allow a compositor to promote a layer surface's existing content into a floating, interactive surface without replacing its `wl_surface`, layer role, renderer, or buffers. This optional MIT protocol is discovered through the Wayland registry. Framework clients continue using their existing xdg-shell pearl path when the manager is absent.

## Contract

The manager is `bead_layer_promote_manager_v1` version 1, defined in `protocols/bead-layer-promote-v1.xml`. A client creates one controller for a layer-role surface. `promote` stages the supplied surface-local body rectangle, optional seat and button serial for an interactive move, and takes effect with the next root surface commit. Passing a null seat and serial zero requests placement without an interactive move. The compositor owns floating placement while promoted. `move` starts a compositor move for an existing promotion; `dock` and `cancel` stage the reverse transition or abort. Request serials increase per controller and stale configure events are ignored.

Configure events report the global body rectangle, output name, and one of `docked`, `promoted`, or `moving`. Rejection leaves the layer surface client-owned. Clients keep their body origin stable while promoted and must not follow pointer movement by repositioning the layer themselves. If the promoted scene grows beyond its original layer content, `set_body_size` stages the new logical body dimensions with the next root commit and buffer; the client may render a larger natural root extent without changing the retained layer-role configuration. Docking restores that configured layer extent and stages the compositor transition with the same root surface.

## Compatibility and verification

The protocol is optional. On compositors that do not advertise the manager, Bead retains its existing native xdg pearl preparation and fallback behavior. Bead does not assign app IDs or product rules to promoted surfaces; decoration, focus and placement policy belong to the compositor. The protocol adds no application-specific names or paths.

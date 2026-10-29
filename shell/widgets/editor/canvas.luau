-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local registry=require("../registry")
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local drag_layout=ui.resolve_for_check(ui.bind("widgets.drag.layout"))
local drag_phase=ui.resolve_for_check(ui.bind("widgets.drag.phase"))
local dragging=ui.resolve_for_check(ui.bind("widgets.drag.active"))==true
    and (drag_phase=="dragging" or drag_phase=="settle")
local editing=ui.resolve_for_check(ui.bind("widgets.editing"))
local document=registry.normalise((dragging and drag_layout and drag_layout[1]) or (editing and draft[1]) or saved[1]
    or registry.default(ui.resolve_for_check(ui.bind("bead.viewport.width"))))
local render=registry.renderer(document)
local canvas=document.hosts.canvas or {width=ui.resolve_for_check(ui.bind("bead.viewport.width")),height=ui.resolve_for_check(ui.bind("bead.viewport.height")),instances={}}
-- B0193: the host sets `bead.desktop.covered` while a fullscreen window or the lock screen hides
-- this output.  Reading it here makes the canvas rebuild once on each change so the runtime can
-- release the seconds lease; nothing else is sent while covered.
ui.resolve_for_check(ui.bind("bead.desktop.covered"))
require("../desktop-visuals").set_canvas(canvas.width,canvas.height)
return ui.Host{id="desktop-widgets",shape="canvas",layout=canvas,catalogue=registry.catalogue,metadata=require("../editing").seed(document,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("bead.viewport.height")),"desktop"),
    render=function(item,variant)
        local node=render(item,variant)
        return node
    end}

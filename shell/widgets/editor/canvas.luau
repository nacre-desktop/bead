-- SPDX-License-Identifier: GPL-3.0-only
local c=require("../context")
local ui=c.ui
local registry=require("../registry")
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local editing=ui.resolve_for_check(ui.bind("widgets.editing"))
local document=(editing and draft[1]) or saved[1] or registry.default(ui.resolve_for_check(ui.bind("bead.viewport.width")))
local render=registry.renderer(document)
local canvas=document.hosts.canvas or {width=ui.resolve_for_check(ui.bind("bead.viewport.width")),height=ui.resolve_for_check(ui.bind("bead.viewport.height")),instances={}}
local targets=editing and require("../editing").targets(document,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("bead.viewport.height")),false) or {}
if editing then
    local ghost=require("../editing").ghost(document,registry.catalogue,render)
    if ghost then table.insert(targets,ghost) end
end
return ui.Host{id="desktop-widgets",shape="canvas",layout=canvas,catalogue=registry.catalogue,metadata=require("../editing").seed(document,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("bead.viewport.height"))),
    render=function(item,variant)
        local node=render(item,variant)
        return node
    end,table.unpack(targets)}

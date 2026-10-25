-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local c=require("../widgets/context")
local widgets=require("../widgets/registry")
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local editing=ui.resolve_for_check(ui.bind("widgets.editing"))
local document=(editing and draft[1]) or saved[1] or widgets.default(ui.resolve_for_check(ui.bind("bead.viewport.width")))
local render=widgets.renderer(document)
local targets={}
if editing then targets=require("../widgets/editing").targets(document,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("widgets.canvas.height")),true) end
local layout=document.hosts.top_bar
local regions={}
for name,region in layout.regions do regions[name]=table.clone(region) end
regions.left.props={id="left",height=34,gap=12,shrink=1,align="center"}
regions.centre.inline=true
regions.right.props={id="right",height=34,gap=12,shrink=0,align="center"}
regions.right.before=require("../widgets/tray")
return ui.Host{edge="top",id="bar",shape="strip-h",grid={tiles=layout.tiles,mode=layout.mode or "tiles",regions=regions},layout=layout.instances,catalogue=widgets.catalogue,render=render,
    metadata=require("../widgets/editing").seed(document,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("widgets.canvas.height"))),
    width="100%",height=c.header,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",direction="row",table.unpack(targets),
}

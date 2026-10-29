-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local c=require("../widgets/context")
local widgets=require("../widgets/registry")
local saved=ui.resolve_for_check(ui.bind("widgets.layout"))
local draft=ui.resolve_for_check(ui.bind("widgets.draft"))
local drag_layout=ui.resolve_for_check(ui.bind("widgets.drag.layout"))
local drag_phase=ui.resolve_for_check(ui.bind("widgets.drag.phase"))
local dragging=ui.resolve_for_check(ui.bind("widgets.drag.active"))==true
    and (drag_phase=="dragging" or drag_phase=="settle")
local editing=ui.resolve_for_check(ui.bind("widgets.editing"))
local document=(dragging and drag_layout and drag_layout[1]) or (editing and draft[1]) or saved[1]
    or widgets.default(ui.resolve_for_check(ui.bind("bead.viewport.width")))
local canonical=widgets.normalise(document)
document=widgets.project(canonical,ui.resolve_for_check(ui.bind("bead.viewport.width")))
local render=widgets.renderer(document)
local layout=document.hosts.top_bar
local regions={}
for name,region in layout.regions do regions[name]=table.clone(region) end
regions.left.props={id="left",height=34,gap=12,shrink=1,align="center"}
regions.centre.inline=true
regions.right.props={id="right",height=34,gap=12,shrink=0,align="center"}
regions.right.before=require("../widgets/tray")
-- B0198 edit mode: while a widget is dragged the bar is a drop zone, highlighted when targeted.
local drop_target=dragging and drag_phase=="dragging"
local targeted=drop_target and ui.resolve_for_check(ui.bind("widgets.drag.destination"))=="bar"
return ui.Host{edge="top",id="bar",border_width=drop_target and 2 or 0,
    border_color=ui.alpha("primary",targeted and .9 or .45),shape="strip-h",grid={tiles=layout.tiles,mode="content",regions=regions},layout=layout.instances,catalogue=widgets.catalogue,render=render,
    metadata=require("../widgets/editing").seed(canonical,ui.resolve_for_check(ui.bind("bead.viewport.width")),ui.resolve_for_check(ui.bind("widgets.canvas.height")),"bar"),
    width="100%",height=c.header,radius=0,fill="surface",padding_left=15,padding_right=15,align="center",justify="space-between",direction="row",
}

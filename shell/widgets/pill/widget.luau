-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local t=require("../context").t
return {id="pill",title="Pill",icon="pill",category="containers",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},variants={bar={min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
    local popouts=ui.resolve_for_check(ui.bind("bar.popouts"))
    return kit.Pill{id="system-pill",height=34,gap=t.status_gap,shrink=0,padding_left=6,padding_right=6,align="center",fill="surfaceContainer",radius=17,
        on_activate=popouts and ui.action("view.open","control") or nil,
        ui.Host{id="pill-host",shape="strip-h",flatten=true,grid={tiles=60,mode="content",regions={left={inline=true},centre={props={id="status",height=34,gap=t.status_gap,shrink=0,align="center"}},right={inline=true}}},layout=props.instance.children or {},catalogue=props.catalogue,render=props.render},
    }
end}}}

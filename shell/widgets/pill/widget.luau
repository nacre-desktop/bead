-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local t=require("../context").t
return {id="pill",title="Pill",icon="pill",category="containers",data={topics={},actions={}},settings={},defaults={},budget={fps_active=0,fps_idle=0,pause_on={"fullscreen","lock","battery_saver"}},forms={bar={default_style="pill",styles={pill={title="Pill",min_span=1,max_span=60,preferred_span=1,render=function(props,settings,data)
    return kit.Pill{id="system-pill",height=t.pill_height,gap=t.pill_gap,shrink=0,padding=0,align="center",fill="surfaceContainer",radius=t.pill_radius,clip_children=true,
        ui.Host{id="pill-host",shape="strip-h",flatten=true,grid={tiles=60,mode="content",regions={left={inline=true},centre={props={id="status",height=t.pill_height,gap=t.pill_gap,shrink=0,align="center"}},right={inline=true}}},layout=props.instance.children or {},catalogue=props.catalogue,render=props.render},
    }
end}}}},panel={styles={control={view="control"}}}}

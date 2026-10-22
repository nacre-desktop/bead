-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local muted=ui.resolve_for_check(ui.bind("osd.muted"))
local level=ui.resolve_for_check(ui.bind("osd.level"))
return kit.Panel{edge="bottom",id="osd",width=340,height=80,open=ui.bind("panel.open"),duration=.150,slide_distance=6,
    fill="surfaceContainer",radius=22,border_width=1,border_color=ui.alpha("outline",.5),padding_left=18,padding_right=18,padding_top=12,gap=14,
    ui.Row{height=22,on_activate=ui.action("notifications.osd",true),ui.Text{family="IBM Plex Sans",text=ui.bind("osd.label"),font_size=14.667,width=0,grow=1},ui.Text{family="IBM Plex Sans",text=ui.bind("osd.status"),width=64,text_align="right",font_size=14.667,color="onSurfaceVariant"}},
    ui.Box{height=8,radius=4,fill="surfaceContainerHigh",on_activate=ui.action("notifications.osd",true),ui.Box{width=level.."%",height=8,radius=4,fill=muted and ui.alpha("primary",.35) or "primary"}},
}

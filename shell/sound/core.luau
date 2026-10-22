-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Panel{id="sound",edge="right",open=ui.bind("panel.open"),duration=0.28,width=400,height=286,padding=20,gap=14,
 ui.Row{width=360,height=32,align="center",justify="space-between",
  ui.Text{id="title",text="Sound output",font_size=kit.tokens.text.title*4/3,height=30,width=0,grow=1,shrink=1},
  kit.IconButton{id="close",text="close",aria_label="Close sound output",width=32,height=32,on_activate=ui.action("close")},
 },
 ui.Text{id="device",width=360,text=ui.bind("status.device"),height=38,font_size=16,color="onSurfaceVariant"},
 ui.Text{id="volume",width=360,text=ui.bind("status.primary"),height=48,font_size=32,color="primary"},
 ui.Text{id="mute",width=360,text=ui.bind("status.mute"),height=28,font_size=16,color="onSurface"},
 ui.Text{id="health",width=360,text=ui.bind("status.health"),height=28,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"},
}

-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
return kit.Panel{id="sound",edge="right",open=ui.bind("panel.open"),width=400,height=286,padding=20,gap=14,
 ui.Row{width=360,height=32,align="center",justify="space-between",
  ui.Text{id="title",text=ui.tr("bead.panel.sound-title"),font_size=kit.tokens.text.title*4/3,height=30,width=0,grow=1,shrink=1},
  kit.IconButton{id="close",text="close",aria_label=ui.tr("bead.panel.sound-close"),width=32,height=32,on_activate=ui.action("close")},
 },
 ui.Text{id="device",width=360,text=ui.bind("bead.desktop.audio.device"),height=26,font_size=16,color="onSurfaceVariant"},
 ui.Row{width=360,height=40,align="center",gap=12,
  kit.IconButton{id="mute",text=ui.bind("bead.desktop.audio"),aria_label=ui.tr("bead.panel.sound-mute"),on_activate=ui.action("services.audio")},
  ui.Text{id="level",text=ui.bind("bead.desktop.audio.label"),height=30,width=0,grow=1,shrink=1,font_size=24,color="primary"},
 },
 kit.Slider{id="volume",label=ui.tr("bead.panel.sound-volume"),value=ui.bind("bead.desktop.audio.percent"),min=0,max=100,step=5,width=352,on_change="services.audio.volume"},
 ui.Text{id="hint",width=360,text=ui.tr("bead.panel.sound-hint"),height=24,font_size=kit.tokens.text.caption*4/3,color="onSurfaceVariant"},
}

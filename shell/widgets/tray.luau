-- SPDX-License-Identifier: GPL-3.0-only
local c=require("context")
local ui,kit,tray_on=c.ui,c.kit,c.tray_on
return function() return {ui.Text{id="reconnecting",visible=ui.bind("bar.reconnecting"),text=ui.tr("bead.bar.reconnecting"),width=92,height=24,font_size=12,color="error"},
        ui.Row{id="tray",visible=ui.bind("bar.tray.visible"),height=30,gap=4,align="center",
            ui.Repeat{id="items",model=ui.bind("bead.desktop.tray"),key="id",render=function(item)
                return kit.Button{id="item",width=28,height=28,padding=2,fill=item.attention and "secondaryContainer" or "surface",radius=14,aria_label=item.title,on_activate=tray_on() and ui.action("services.tray",item.id) or nil,
                    ui.Image{id="icon",icon=item.icon,width=24,height=24},
                }
            end},
        }}
end

-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
return ui.Panel{id="backend-demo",width=760,height=112,fill="surface",padding=12,gap=8,
    ui.Row{gap=6,height=36,
        ui.Repeat{id="workspaces",model=ui.bind("bead.workspaces"),key="id",render=function(item)
            return ui.Button{id="workspace",text=item.name,width=44,height=32,padding=4,
                selected=item.active,on_activate=ui.action("backend.workspace.switch",item.id)}
        end},
    },
    ui.Text{id="title",text=ui.bind("bead.focused_title"),height=24,font_size=16},
}

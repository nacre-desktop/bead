-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local C=require("./components")
local function icon(id,glyph,label,action)
    return kit.Button{id=id,width=44,height=40,padding=0,radius=10,justify="center",align="center",fill="surfaceContainerLow",
        aria_label=label,on_activate=action,
        ui.Text{id=id.."-glyph",text=glyph,family="Material Symbols Rounded",font_size=20,width=24,height=24,color=action and "primary" or "onSurfaceVariant"}}
end
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    return ui.Column{id="customize",width=940,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
        C.title("title",ui.bind("dashboard.heading"),896),
        ui.Text{id="hint",text=ui.bind("dashboard.summary"),width=896,height=24,font_size=14,color="onSurfaceVariant"},
        ui.Repeat{id="items",model=ui.bind("dashboard.rows"),key="id",render=function(item)
            return ui.Row{id="row",width=896,height=44,gap=8,align="center",
                ui.Text{id="label",text=item.label,width=560,height=24,font_size=16,color="onSurface",shrink=0},
                C.button("toggle",item.value,control and ui.action("dashboard.do",item.toggle) or nil,100,false,"Show or hide page"),
                icon("up","arrow_upward","Move up",control and ui.action("dashboard.do",item.up) or nil),
                icon("down","arrow_downward","Move down",control and ui.action("dashboard.do",item.down) or nil),
            }
        end},
    }
end

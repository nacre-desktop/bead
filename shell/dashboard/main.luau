-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local page=ui.resolve_for_check(ui.bind("dashboard.page"))
local preview=ui.resolve_for_check(ui.bind("dashboard.preview"))
local stale=ui.resolve_for_check(ui.bind("dashboard.stale"))
local content=require("./page")
local nav={}
local tabs=ui.resolve_for_check(ui.bind("dashboard.tabs"))
local message=ui.resolve_for_check(ui.bind("dashboard.message"))
local customize=ui.resolve_for_check(ui.bind("dashboard.customize"))
-- Slots share the strip; labels drop out when a slot gets too narrow.
local count=math.max(1,#tabs)
local tw=math.floor(936/count)
local labeled=tw>=94
local lw=count<=4 and 90 or math.min(112,tw-39)
local rw=labeled and 27+lw or 20
for _,tab in ipairs(tabs) do
    local selected=tab.id==page
    local interactive=not selected
    local Button=interactive and kit.Button or ui.Row
    table.insert(nav,Button{id=tab.id,width=tw,height=44,padding=0,radius=12,justify="center",align="center",
        fill=selected and ui.alpha("primary",0.12) or "transparent",
        aria_label=tab.title,on_activate=interactive and ui.action("view.open","dashboard-"..tab.id) or nil,
        ui.Row{width=rw,gap=7,align="center",justify="center",height=44,
            ui.Text{text=tab.icon,family="Material Symbols Rounded",width=20,height=24,font_size=20,color=selected and "primary" or "onSurfaceVariant"},
            labeled and ui.Text{text=tab.title,width=lw,height=24,font_size=16,color=selected and "primary" or "onSurfaceVariant"} or nil,
        },
    })
end
return kit.Panel{id="dashboard",edge="top",open=ui.bind("panel.open"),duration=kit.motion.color,
    width=984,height=650,padding=0,radius=18,fill="surface",
    ui.Row{id="navigation",position="absolute",left=24,top=12,width=936,height=44,table.unpack(nav)},
    ui.Box{id="stage",position="absolute",left=22,top=68,width=940,height=562,content()},
    ui.Text{id="state",position="absolute",left=24,top=632,width=936,height=16,font_size=12,text_align="right",
        text=preview and (stale and "Preview data · Stale" or "Preview data") or stale and "Stale data" or "",color=stale and not preview and "error" or "onSurfaceVariant"},
    message~="" and ui.Text{id="message",position="absolute",left=24,top=632,width=600,height=16,font_size=12,text=ui.bind("dashboard.message"),color="error"} or nil,
    customize and message=="" and kit.Button{id="customize",position="absolute",left=24,top=626,width=170,height=24,padding=0,radius=8,fill="transparent",
        aria_label="Customize pages",on_activate=ui.action("view.open","dashboard-customize"),
        ui.Text{id="customize-label",text="Customize pages",width=170,height=16,font_size=12,color="onSurfaceVariant"}} or nil,
}

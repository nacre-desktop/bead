-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local page=ui.resolve_for_check(ui.bind("dashboard.page"))
local preview=ui.resolve_for_check(ui.bind("dashboard.preview"))
local stale=ui.resolve_for_check(ui.bind("dashboard.stale"))
local content=require("./page")
local nav={}
for _,tab in ipairs({{"today","Today","dashboard"},{"weather","Weather","partly_cloudy_day"},{"media","Media","music_note"},{"system","System","monitoring"}}) do
    local selected=tab[1]==page
    local interactive=tab[1]~="today" and not selected
    local Button=interactive and kit.Button or ui.Row
    table.insert(nav,Button{id=tab[1],width=234,height=44,padding=0,radius=12,justify="center",align="center",
        fill=selected and ui.alpha("primary",0.12) or "transparent",
        aria_label=tab[2],on_activate=interactive and ui.action("view.open","dashboard-"..tab[1]) or nil,
        ui.Row{width=117,gap=7,align="center",justify="center",height=44,
            ui.Text{text=tab[3],family="Material Symbols Rounded",width=20,height=24,font_size=20,color=selected and "primary" or "onSurfaceVariant"},
            ui.Text{text=tab[2],width=90,height=24,font_size=16,color=selected and "primary" or "onSurfaceVariant"},
        },
    })
end
return kit.Panel{id="dashboard",edge="top",open=ui.bind("panel.open"),duration=kit.motion.color,
    width=984,height=650,padding=0,radius=18,fill="surface",
    ui.Row{id="navigation",position="absolute",left=24,top=12,width=936,height=44,table.unpack(nav)},
    ui.Box{id="stage",position="absolute",left=22,top=68,width=940,height=562,content()},
    ui.Text{id="state",position="absolute",left=24,top=632,width=936,height=16,font_size=12,text_align="right",
        text=preview and (stale and "Preview data · Stale" or "Preview data") or stale and "Stale data" or "",color=stale and not preview and "error" or "onSurfaceVariant"},
}

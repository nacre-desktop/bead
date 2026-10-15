-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local kit=require("bead.kit")
local function noticeHeight(n) return 84+(n.expanded and 48 or 24)+(n.body~="" and (n.expanded and 110 or 44) or 0)+(#n.actions>0 and (math.ceil(#n.actions/2)*40+math.max(0,math.ceil(#n.actions/2)-1)*6) or 0) end
local function notice(n)
    local buttons={id="actions",columns=2,row_height=40,gap=6}
    for i,a in n.actions do buttons[i]=kit.Button{id="action-"..i,width="fill",text=a.label,on_activate=ui.action("notifications.action",a.id)} end
    return kit.Card{id="notice",height=noticeHeight(n),radius=14,padding=14,fill="surfaceContainer",on_activate=ui.action("notifications.details",n.id),aria_label=n.app..": "..n.summary,
        border_width=1,border_color=n.urgency==2 and "error" or ui.alpha("outlineVariant",.45),gap=8,
        ui.Row{gap=12,
            ui.Image{icon=n.icon~="" and n.icon or "dialog-information",width=41,height=41,aria_label=n.app},
            ui.Column{grow=1,gap=4,
                ui.Row{gap=8,ui.Text{text=n.app,font_size=13.333,grow=1,color="onSurfaceVariant"},kit.IconButton{id="dismiss",text="close",aria_label="Dismiss notification",on_activate=ui.action("notifications.dismiss",n.id)}},
                ui.Text{text=n.summary,font_size=16,height=n.expanded and 48 or 24},
                ui.Text{text=n.body,font_size=13.333,height=n.body~="" and (n.expanded and 110 or 44) or 0},
            },
        },ui.Grid(buttons),
    }
end
local rows=ui.resolve_for_check(ui.bind("notifications.rows"))
local height=math.max(0,#rows-1)*8
for _,n in rows do height+=noticeHeight(n) end
return ui.Column{edge="right",id="notifications",width=400,height=math.max(1,math.min(720,height)),fill="transparent",radius=0,
    open=ui.bind("notifications.open"),duration=.230,
    ui.Scroll{id="stream",grow=1,scroll=ui.bind("notifications.scroll.y"),ui.Column{shrink=0,gap=8,ui.Repeat{id="notices",model=ui.bind("notifications.rows"),key="id",render=notice}}},
}

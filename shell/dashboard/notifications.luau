-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local dnd=ui.resolve_for_check(ui.bind("dashboard.dnd"))
    return ui.Box{id="notifications",width=940,height=562,
        ui.Column{id="recent",position="absolute",left=0,top=0,width=564,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
            ui.Row{id="header",width=520,height=40,gap=10,align="center",
                ui.Text{id="title",text=ui.bind("dashboard.heading"),width=520-160-130-20,height=28,font_size=20,color="onSurface"},
                C.button("dnd",dnd and ui.tr("bead.dashboard.dnd-on") or ui.tr("bead.dashboard.dnd-off"),control and ui.action("dashboard.do","notif.dnd") or nil,130,dnd),
                C.button("clear",ui.tr("bead.dashboard.notifications-clear"),control and ui.action("dashboard.do","notif.clear") or nil,160)},
            C.lines("items","dashboard.rows",520,440,ui.tr("bead.dashboard.notifications-none"),true,control,ui.bind("dashboard.notifications.scroll")),
        },
        ui.Column{id="status",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            ui.Text{text="notifications",family="Material Symbols Rounded",font_size=32,width=40,height=40,color="primary"},
            C.title("title",ui.tr("bead.dashboard.notifications-status"),328),
            ui.Text{id="message",text=ui.bind("dashboard.summary"),width=328,height=48,font_size=16,color="onSurface"},
            ui.Text{id="empty",text=ui.bind("dashboard.detail"),width=328,height=48,font_size=14,color="onSurfaceVariant"},
            ui.Text{id="hint",text=ui.tr("bead.dashboard.notifications-hint"),width=328,height=48,font_size=12,color="onSurfaceVariant"},
        },
    }
end

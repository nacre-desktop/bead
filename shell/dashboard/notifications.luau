-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    return ui.Box{id="notifications",width=940,height=562,
        ui.Column{id="recent",position="absolute",left=0,top=0,width=564,height=562,padding=22,gap=16,radius=20,fill="surfaceContainer",
            C.title("title",ui.bind("dashboard.heading"),520),
            C.rows("items","dashboard.rows",520,470,false,"No notifications"),
        },
        ui.Column{id="status",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            ui.Text{text="notifications",family="Material Symbols Rounded",font_size=32,width=40,height=40,color="primary"},
            C.title("title","Status",328),
            ui.Text{id="message",text=ui.bind("dashboard.summary"),width=328,height=48,font_size=16,color="onSurface"},
            ui.Text{id="empty",text=ui.bind("dashboard.detail"),width=328,height=48,font_size=14,color="onSurfaceVariant"},
        },
    }
end

-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    return ui.Box{id="network",width=940,height=562,
        ui.Column{id="hero",position="absolute",left=0,top=0,width=564,height=240,padding=22,gap=8,radius=20,fill="surfaceContainer",
            C.title("connection",ui.bind("dashboard.heading"),520),
            ui.Text{id="signal",text=ui.bind("dashboard.metric"),font_size=64,height=84,color="primary"},
            ui.Text{id="radio",text=ui.bind("dashboard.summary"),font_size=16,height=24,width=520,color="onSurface"},
            ui.Text{id="details",text=ui.bind("dashboard.detail"),font_size=14,height=24,width=520,color="onSurfaceVariant"},
        },
        ui.Column{id="connections",position="absolute",left=0,top=252,width=564,height=310,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            C.title("title","Connections",532),
            C.rows("items","dashboard.rows",532,240,false,"No connections"),
        },
        ui.Column{id="access",position="absolute",left=580,top=0,width=360,height=562,padding=16,gap=12,radius=16,fill="surfaceContainerLow",
            C.title("title","Wi-Fi networks",328),
            C.rows("items","dashboard.secondary",328,470,false,"No networks in range"),
        },
    }
end

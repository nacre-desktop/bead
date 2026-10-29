-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    return ui.Box{id="devices",width=940,height=562,
        ui.Column{id="bluetooth",position="absolute",left=0,top=0,width=464,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
            C.title("title",ui.bind("dashboard.heading"),420),
            ui.Text{id="power",text=ui.bind("dashboard.summary"),width=420,height=24,font_size=14,color="onSurfaceVariant"},
            C.rows("items","dashboard.rows",420,420,false,ui.tr("bead.dashboard.bluetooth-none")),
        },
        ui.Column{id="audio",position="absolute",left=480,top=0,width=460,height=562,padding=22,gap=12,radius=20,fill="surfaceContainerLow",
            C.title("title",ui.tr("bead.dashboard.audio"),416),
            C.rows("items","dashboard.secondary",416,470,false,ui.tr("bead.dashboard.audio-none")),
        },
    }
end

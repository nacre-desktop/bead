-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    return ui.Box{id="brain",width=940,height=562,
        ui.Column{id="notes",position="absolute",left=0,top=0,width=460,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
            C.title("title",ui.bind("dashboard.heading"),416),
            C.lines("items","dashboard.rows",416,470,ui.resolve_for_check(ui.bind("dashboard.detail")),false,false),
        },
        ui.Column{id="tasks",position="absolute",left=476,top=0,width=464,height=562,padding=22,gap=12,radius=20,fill="surfaceContainerLow",
            C.title("title",ui.tr("bead.dashboard.brain-tasks"),420),
            C.lines("items","dashboard.secondary",420,470,ui.tr("bead.dashboard.brain-no-tasks"),false,false),
        },
    }
end

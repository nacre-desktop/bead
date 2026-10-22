-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    return ui.Column{id="ai-usage",width=940,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
        ui.Row{id="header",width=896,height=40,gap=12,align="center",
            ui.Text{id="title",text=ui.bind("dashboard.heading"),width=896-150-12,height=28,font_size=20,color="onSurface"},
            C.button("refresh","Refresh",control and ui.action("dashboard.do","usage.refresh") or nil,150)},
        C.lines("items","dashboard.rows",896,440,ui.resolve_for_check(ui.bind("dashboard.detail")),false,false),
    }
end

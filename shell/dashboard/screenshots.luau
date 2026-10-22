-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    return ui.Column{id="screenshots",width=940,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
        C.title("title",ui.bind("dashboard.heading"),896),
        C.shots("items","dashboard.rows",896,440,ui.resolve_for_check(ui.bind("dashboard.detail")),control),
    }
end

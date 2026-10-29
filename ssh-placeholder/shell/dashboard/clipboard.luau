-- SPDX-License-Identifier: GPL-3.0-only
local ui=require("bead.ui")
local C=require("./components")
return function()
    local control=ui.resolve_for_check(ui.bind("dashboard.control"))
    local confirm=ui.resolve_for_check(ui.bind("dashboard.confirm"))
    local buttons={}
    if confirm then
        table.insert(buttons,C.button("clear",ui.tr("bead.dashboard.clipboard-clear-confirm"),control and ui.action("dashboard.do","clip.clear") or nil,130))
        table.insert(buttons,C.button("cancel",ui.tr("bead.common.cancel"),ui.action("dashboard.do","clip.cancel"),110))
    else
        table.insert(buttons,C.button("ask",ui.tr("bead.dashboard.clipboard-clear"),control and ui.action("dashboard.do","clip.ask") or nil,150))
    end
    return ui.Column{id="clipboard",width=940,height=562,padding=22,gap=12,radius=20,fill="surfaceContainer",
        ui.Row{id="header",width=896,height=40,gap=12,align="center",
            ui.Text{id="title",text=ui.bind("dashboard.heading"),width=896-150-12,height=28,font_size=20,color="onSurface"},
            table.unpack(buttons)},
        C.lines("items","dashboard.rows",896,440,ui.resolve_for_check(ui.bind("dashboard.detail")),true,control,ui.bind("dashboard.clipboard.scroll")),
    }
end
